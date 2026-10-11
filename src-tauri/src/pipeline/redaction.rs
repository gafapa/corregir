//! Mandatory human redaction review (Milestone 4). Only `confirm` and
//! `load_confirmed` can construct `ConfirmedRedactedText`, so unreviewed text
//! cannot enter the AI pipeline. See the privacy boundary in ARCHITECTURE.md.

use rand::RngExt;
use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RedactionError {
    #[error("submission {0} does not exist")]
    SubmissionNotFound(i64),
    #[error("submission {0} has no OCR text yet")]
    MissingOcrText(i64),
    #[error("submission {0} has not passed human redaction review")]
    NotRedacted(i64),
    #[error(
        "invalid redaction range: offsets must be ordered UTF-8 byte boundaries within the text"
    )]
    InvalidSpan,
    #[error("submission {0} has already passed redaction review")]
    AlreadyReviewed(i64),
    #[error("enter a student name of between 1 and 200 bytes")]
    InvalidName,
    #[error("the redaction preview changed; generate and review it again")]
    InvalidPreview,
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Text that has passed human redaction review. Fields are private so only
/// this module can construct a confirmed value.
pub struct ConfirmedRedactedText {
    text: String,
    alias: String,
}

impl ConfirmedRedactedText {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }
}

fn generate_alias() -> String {
    let mut bytes = [0u8; 4];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn generate_alias_unique(conn: &Connection) -> Result<String, RedactionError> {
    loop {
        let candidate = generate_alias();
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM alias_student_map WHERE alias = ?1)",
            [&candidate],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(candidate);
        }
    }
}

/// Merge overlapping or adjacent ranges before replacing them. Identifier
/// detection can return both an email and a roster name inside that email.
/// Replacing overlapping ranges separately would corrupt the text.
fn merge_spans(spans: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut sorted: Vec<(usize, usize)> = spans.to_vec();
    sorted.sort_by_key(|s| s.0);
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in sorted {
        match merged.last_mut() {
            Some(last) if start <= last.1 => {
                last.1 = last.1.max(end);
            }
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Replace each UTF-8 byte range with `alias`, processing merged ranges from
/// the end so earlier offsets remain valid.
fn redact(text: &str, spans: &[(usize, usize)], alias: &str) -> String {
    let mut merged = merge_spans(spans);
    merged.sort_by_key(|s| std::cmp::Reverse(s.0));
    let mut result = text.to_string();
    for (start, end) in merged {
        result.replace_range(start..end, alias);
    }
    result
}

/// Persist the teacher-approved redaction and local alias-to-student map.
#[cfg(test)]
pub fn confirm(
    conn: &Connection,
    submission_id: i64,
    spans_to_redact: &[(usize, usize)],
    student_name: &str,
    student_class_id: Option<&str>,
) -> Result<ConfirmedRedactedText, RedactionError> {
    confirm_internal(
        conn,
        submission_id,
        spans_to_redact,
        student_name,
        student_class_id,
        None,
    )
}

#[derive(serde::Serialize)]
pub struct RedactionPreview {
    pub text: String,
    pub alias: String,
}

fn reviewed_spans(
    text: &str,
    spans: &[(usize, usize)],
    student_name: &str,
) -> Result<Vec<(usize, usize)>, RedactionError> {
    let name = student_name.trim();
    if name.is_empty() || name.len() > 200 {
        return Err(RedactionError::InvalidName);
    }
    for &(start, end) in spans {
        if start >= end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return Err(RedactionError::InvalidSpan);
        }
    }
    let mut combined = spans.to_vec();
    let pattern = regex::RegexBuilder::new(&regex::escape(name))
        .case_insensitive(true)
        .build()
        .map_err(|_| RedactionError::InvalidName)?;
    combined.extend(
        pattern
            .find_iter(text)
            .map(|value| (value.start(), value.end())),
    );
    Ok(merge_spans(&combined))
}

pub fn preview(
    conn: &Connection,
    submission_id: i64,
    spans: &[(usize, usize)],
    student_name: &str,
) -> Result<RedactionPreview, RedactionError> {
    let (text, status): (String, String) = conn.query_row(
        "SELECT text_ocr,status_pipeline FROM submissions WHERE id=?1",
        [submission_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if status != "ocr_complete" {
        return Err(RedactionError::AlreadyReviewed(submission_id));
    }
    let reviewed = reviewed_spans(&text, spans, student_name)?;
    let alias = generate_alias_unique(conn)?;
    Ok(RedactionPreview {
        text: redact(&text, &reviewed, &alias),
        alias,
    })
}

pub fn confirm_preview(
    conn: &Connection,
    submission_id: i64,
    spans: &[(usize, usize)],
    student_name: &str,
    student_class_id: Option<&str>,
    alias: &str,
    expected_text: &str,
) -> Result<ConfirmedRedactedText, RedactionError> {
    confirm_internal(
        conn,
        submission_id,
        spans,
        student_name,
        student_class_id,
        Some((alias, expected_text)),
    )
}

fn confirm_internal(
    conn: &Connection,
    submission_id: i64,
    spans_to_redact: &[(usize, usize)],
    student_name: &str,
    student_class_id: Option<&str>,
    expected: Option<(&str, &str)>,
) -> Result<ConfirmedRedactedText, RedactionError> {
    let tx = conn.unchecked_transaction()?;
    let status: String = tx.query_row(
        "SELECT status_pipeline FROM submissions WHERE id = ?1",
        [submission_id],
        |r| r.get(0),
    )?;
    if status != "ocr_complete" {
        return Err(RedactionError::AlreadyReviewed(submission_id));
    }
    let text_original: Option<String> = conn
        .query_row(
            "SELECT text_ocr FROM submissions WHERE id = ?1",
            [submission_id],
            |row| row.get(0),
        )
        .map_err(|_| RedactionError::SubmissionNotFound(submission_id))?;
    let text_original = text_original.ok_or(RedactionError::MissingOcrText(submission_id))?;

    let reviewed = reviewed_spans(&text_original, spans_to_redact, student_name)?;
    let alias = match expected {
        Some((alias, _))
            if alias.len() == 8 && alias.bytes().all(|value| value.is_ascii_hexdigit()) =>
        {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM alias_student_map WHERE alias=?1)",
                [alias],
                |row| row.get(0),
            )?;
            if exists {
                return Err(RedactionError::InvalidPreview);
            }
            alias.to_string()
        }
        Some(_) => return Err(RedactionError::InvalidPreview),
        None => generate_alias_unique(conn)?,
    };
    let text_redacted = redact(&text_original, &reviewed, &alias);
    if expected.is_some_and(|(_, text)| text != text_redacted) {
        return Err(RedactionError::InvalidPreview);
    }

    conn.execute(
        "UPDATE submissions SET text_redacted = ?1, alias = ?2, status_pipeline = 'redacted' WHERE id = ?3",
        rusqlite::params![text_redacted, alias, submission_id],
    )?;
    conn.execute(
        "INSERT INTO alias_student_map (alias, student_name, student_class_id, submission_id) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![alias, student_name, student_class_id, submission_id],
    )?;
    conn.execute(
        "INSERT INTO logs_audit (submission_id, event, actor, payload_json) VALUES (?1, 'redaction_confirmed', 'teacher', ?2)",
        rusqlite::params![
            submission_id,
            serde_json::json!({ "redacted_span_count": reviewed.len() }).to_string()
        ],
    )?;

    tx.commit()?;
    Ok(ConfirmedRedactedText {
        text: text_redacted,
        alias,
    })
}

/// Recovers the already persisted confirmation. Fails if the submission has not passed through `confirm` — it is the check that prevents the Milestone 5 from using a text not reviewed.
pub fn load_confirmed(
    conn: &Connection,
    submission_id: i64,
) -> Result<ConfirmedRedactedText, RedactionError> {
    let row = conn.query_row(
        "SELECT text_redacted, alias, status_pipeline FROM submissions WHERE id = ?1",
        [submission_id],
        |row| {
            let text: Option<String> = row.get(0)?;
            let alias: Option<String> = row.get(1)?;
            let status: String = row.get(2)?;
            Ok((text, alias, status))
        },
    );

    let (text, alias, status) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(RedactionError::SubmissionNotFound(submission_id))
        }
        Err(e) => return Err(e.into()),
    };

    match (status.as_str(), text, alias) {
        ("redacted", Some(text), Some(alias)) => Ok(ConfirmedRedactedText { text, alias }),
        _ => Err(RedactionError::NotRedacted(submission_id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use tempfile::tempdir;
    #[test]
    fn preview_automatically_redacts_known_names_and_rejects_a_changed_review() {
        let directory = tempdir().unwrap();
        let state = schema::open_with_key(directory.path(), [9; 32]).unwrap();
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let id = prepare_test_submission(conn, "JOSÉ 😀 explains the answer. José signs it.");
        let reviewed = preview(conn, id, &[], " José ").unwrap();
        assert!(!reviewed.text.contains("JOSÉ"));
        assert!(!reviewed.text.contains("José"));
        assert!(reviewed.text.contains("😀 explains the answer."));
        assert!(matches!(
            confirm_preview(
                conn,
                id,
                &[],
                "Another name",
                None,
                &reviewed.alias,
                &reviewed.text
            ),
            Err(RedactionError::InvalidPreview)
        ));
        assert!(load_confirmed(conn, id).is_err());
        let confirmed = confirm_preview(
            conn,
            id,
            &[],
            " José ",
            None,
            &reviewed.alias,
            &reviewed.text,
        )
        .unwrap();
        assert_eq!(confirmed.text(), reviewed.text);
        assert!(preview(conn, id, &[], "José").is_err());
    }

    fn prepare_test_submission(conn: &Connection, text_ocr: &str) -> i64 {
        conn.execute(
            "INSERT INTO rubrics (title, subject, grade_level, content_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubric_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO assignments (rubric_id, text) VALUES (?1, 'test assignment')",
            [rubric_id],
        )
        .unwrap();
        let assignment_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO submissions (assignment_id, text_ocr, status_pipeline) VALUES (?1, ?2, 'ocr_complete')",
            rusqlite::params![assignment_id, text_ocr],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn invalid_ranges_fail_without_mutating_submission() {
        let directory = tempdir().unwrap();
        let state = schema::open_with_key(directory.path(), [9u8; 32]).unwrap();
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let id = prepare_test_submission(conn, "José 😀");
        for span in [(3, 4), (0, 100), (5, 2), (0, 0)] {
            assert!(matches!(
                confirm(conn, id, &[span], "José", None),
                Err(RedactionError::InvalidSpan)
            ));
        }
        assert!(load_confirmed(conn, id).is_err());
        let confirmed = confirm(conn, id, &[(0, "José".len())], "José", None).unwrap();
        assert_eq!(confirmed.text(), format!("{} 😀", confirmed.alias()));
        assert!(matches!(
            confirm(conn, id, &[], "José", None),
            Err(RedactionError::AlreadyReviewed(_))
        ));
    }

    #[test]
    fn confirmation_redacts_and_persists_alias() {
        let directory = tempdir().unwrap();
        let status = schema::open_with_key(directory.path(), [9u8; 32]).unwrap();
        let guard = status.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        let text = "Name: Maria Lopez. Response: the poem is about childhood.";
        let submission_id = prepare_test_submission(conn, text);

        // "Maria Lopez" is in offsets 8..19 (checked by construction of the text).
        let start = text.find("Maria Lopez").unwrap();
        let end = start + "Maria Lopez".len();

        let confirmed = confirm(conn, submission_id, &[(start, end)], "Maria Lopez", None).unwrap();

        assert!(!confirmed.text().contains("Maria Lopez"));
        assert!(confirmed.text().contains(confirmed.alias()));
        assert!(confirmed.text().contains("the poem is about childhood"));

        // The alias map alias->student must exist and point to the real name.
        let name_real: String = conn
            .query_row(
                "SELECT student_name FROM alias_student_map WHERE alias = ?1",
                [confirmed.alias()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name_real, "Maria Lopez");
    }

    #[test]
    fn load_confirmed_fails_before_confirmation() {
        let directory = tempdir().unwrap();
        let status = schema::open_with_key(directory.path(), [9u8; 32]).unwrap();
        let guard = status.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        let submission_id = prepare_test_submission(conn, "text awaiting redaction");

        let result = load_confirmed(conn, submission_id);
        assert!(matches!(result, Err(RedactionError::NotRedacted(_))));
    }

    #[test]
    fn load_confirmed_succeeds_after_confirmation() {
        let directory = tempdir().unwrap();
        let status = schema::open_with_key(directory.path(), [9u8; 32]).unwrap();
        let guard = status.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        let submission_id = prepare_test_submission(conn, "no identifiers in this text");
        confirm(conn, submission_id, &[], "Student Anonymous", None).unwrap();

        let loaded = load_confirmed(conn, submission_id).unwrap();
        assert_eq!(loaded.text(), "no identifiers in this text");
    }

    /// Overlapping email and roster matches must produce one clean replacement.
    #[test]
    fn confirmation_merges_overlapping_spans_without_corrupting_text() {
        let directory = tempdir().unwrap();
        let status = schema::open_with_key(directory.path(), [13u8; 32]).unwrap();
        let guard = status.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        let text = "Name: Maria Synthetic Lopez Example\nNational ID: 12345678Z (fictional)\n\
                     Email: maria.synthetic.example@invalid.test\n\nResponse...";
        let submission_id = prepare_test_submission(conn, text);

        let name_start = text.find("Maria Synthetic Lopez Example").unwrap();
        let id_start = text.find("12345678Z").unwrap();
        let email_start = text.find("maria.synthetic.example@invalid.test").unwrap();
        let spans = vec![
            (
                name_start,
                name_start + "Maria Synthetic Lopez Example".len(),
            ),
            (id_start, id_start + "12345678Z".len()),
            (
                email_start,
                email_start + "maria.synthetic.example@invalid.test".len(),
            ),
            (email_start, email_start + "maria".len()),
            (email_start + 6, email_start + 15),
        ];

        let confirmed = confirm(
            conn,
            submission_id,
            &spans,
            "Maria Synthetic Lopez Example",
            None,
        )
        .unwrap();
        let text_final = confirmed.text();

        assert!(
            text_final.contains("Response..."),
            "text outside the spans must remain"
        );
        assert!(
            !text_final.contains("maria.synthetic.example"),
            "the email must not survive in any form: {text_final}"
        );
        assert!(
            !text_final.contains("invalid.test"),
            "the email domain must not survive: {text_final}"
        );
        // Each alias is eight hex characters with no leftover identifier fragment.
        for occurrence in text_final.match_indices(confirmed.alias()) {
            let next = text_final
                .as_bytes()
                .get(occurrence.0 + confirmed.alias().len());
            let is_hex_extra = next.is_some_and(|b| b.is_ascii_hexdigit());
            assert!(
                !is_hex_extra,
                "alias has an attached fragment: {text_final}"
            );
        }
    }
}
