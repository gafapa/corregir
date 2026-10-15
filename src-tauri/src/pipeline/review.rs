//! Second mandatory human checkpoint (Milestone 6). The teacher enters a
//! tentative grade in Milestone 5 and explicitly confirms it here. The AI
//! never sets the grade.

use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReviewError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(
        "redaction must be confirmed and every rubric criterion must have a valid teacher score"
    )]
    IncompleteAssessment,
    #[error("the assessment changed; reload and review it before confirming")]
    StaleAssessment,
    #[error("enter a reason of between 1 and 500 characters")]
    InvalidReason,
    #[error("only confirmed grades that still have an identified student can be reopened")]
    NotConfirmed,
    #[error(transparent)]
    Redaction(#[from] crate::pipeline::redaction::RedactionError),
}

pub fn confirm_grade(
    conn: &Connection,
    submission_id: i64,
    expected_revision: i64,
) -> Result<f64, ReviewError> {
    let tx = conn.unchecked_transaction()?;
    let (status, revision): (String, i64) = tx.query_row(
        "SELECT status_pipeline, grade_revision FROM submissions WHERE id = ?1",
        [submission_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if revision != expected_revision {
        return Err(ReviewError::StaleAssessment);
    }
    let (count, valid): (i64, i64) = tx.query_row(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN r.score_final >= 0 AND r.score_final <= cr.score_max THEN 1 ELSE 0 END), 0)
         FROM submissions s JOIN assignments a ON a.id = s.assignment_id
         JOIN criteria_rubric cr ON cr.rubric_id = a.rubric_id
         LEFT JOIN results r ON r.submission_id = s.id AND r.criterion_id = cr.id
         WHERE s.id = ?1", [submission_id], |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if status != "redacted" || count == 0 || count != valid {
        return Err(ReviewError::IncompleteAssessment);
    }
    conn.execute(
        "UPDATE results SET confirmed_by_teacher = 1,
            confirmed_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE submission_id = ?1",
        [submission_id],
    )?;

    conn.execute(
        "UPDATE submissions SET status_pipeline = 'grade_confirmed' WHERE id = ?1",
        [submission_id],
    )?;

    let total: Option<f64> = conn.query_row(
        "SELECT SUM(score_final) FROM results WHERE submission_id = ?1",
        [submission_id],
        |r| r.get(0),
    )?;

    conn.execute(
        "INSERT INTO logs_audit (submission_id, event, actor, payload_json)
         VALUES (?1, 'grade_confirmed', 'teacher', ?2)",
        rusqlite::params![
            submission_id,
            serde_json::json!({ "grade_total": total }).to_string()
        ],
    )?;

    tx.commit()?;
    Ok(total.unwrap_or(0.0))
}

/// Return a confirmed grade to editing. The reason is kept in the audit payload,
/// which audit CSV exports exclude.
pub fn reopen_grade(
    conn: &Connection,
    submission_id: i64,
    reason: &str,
) -> Result<(), ReviewError> {
    let reason = reason.trim();
    if reason.is_empty() || reason.chars().count() > 500 {
        return Err(ReviewError::InvalidReason);
    }
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE submissions SET status_pipeline = 'redacted'
         WHERE id = ?1 AND status_pipeline = 'grade_confirmed' AND identities_purged_at IS NULL",
        [submission_id],
    )?;
    if changed != 1 {
        return Err(ReviewError::NotConfirmed);
    }
    tx.execute(
        "UPDATE results SET confirmed_by_teacher = 0, confirmed_at = NULL WHERE submission_id = ?1",
        [submission_id],
    )?;
    // The reason is free text kept in the audit trail, which outlives identity purges
    // and erasure: known names are replaced as in comments sent to the model.
    let reason = crate::pipeline::redaction::known_name_redactor(&tx, submission_id)?.apply(reason);
    tx.execute(
        "INSERT INTO logs_audit (submission_id, event, actor, payload_json)
         VALUES (?1, 'grade_reopened', 'teacher', ?2)",
        rusqlite::params![
            submission_id,
            serde_json::json!({ "reason": reason }).to_string()
        ],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use tempfile::tempdir;

    fn prepare_submission_with_results(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO rubrics (title, subject, grade_level, content_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubric_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO criteria_rubric (rubric_id, code, description, score_max, sort_order)
             VALUES (?1, 'C1', 'criterion 1', 5, 0), (?1, 'C2', 'criterion 2', 5, 1)",
            [rubric_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assignments (rubric_id, text) VALUES (?1, 'assignment')",
            [rubric_id],
        )
        .unwrap();
        let assignment_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO submissions (assignment_id, text_ocr, status_pipeline) VALUES (?1, 'text', 'ocr_complete')",
            [assignment_id],
        )
        .unwrap();
        let submission_id = conn.last_insert_rowid();
        crate::pipeline::redaction::confirm(conn, submission_id, &[], "Test Student", None)
            .unwrap();

        let c1: i64 = conn
            .query_row("SELECT id FROM criteria_rubric WHERE code='C1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let c2: i64 = conn
            .query_row("SELECT id FROM criteria_rubric WHERE code='C2'", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO results (submission_id, criterion_id, score_final) VALUES (?1, ?2, 3.5), (?1, ?3, 4.0)",
            rusqlite::params![submission_id, c1, c2],
        )
        .unwrap();

        submission_id
    }

    #[test]
    fn confirm_grade_sums_scores_and_marks_status() {
        let directory = tempdir().unwrap();
        let status = schema::open_with_key(directory.path(), [5u8; 32]).unwrap();
        let guard = status.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let submission_id = prepare_submission_with_results(conn);

        let total = confirm_grade(conn, submission_id, 0).unwrap();
        assert_eq!(total, 7.5);

        let status_pipeline: String = conn
            .query_row(
                "SELECT status_pipeline FROM submissions WHERE id = ?1",
                [submission_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status_pipeline, "grade_confirmed");

        let confirmed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM results WHERE submission_id = ?1 AND confirmed_by_teacher = 1",
                [submission_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(confirmed, 2);
    }

    #[test]
    fn reopening_requires_a_reason_and_allows_a_new_confirmation() {
        let directory = tempdir().unwrap();
        let state = schema::open_with_key(directory.path(), [5u8; 32]).unwrap();
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let id = prepare_submission_with_results(conn);
        assert!(matches!(
            reopen_grade(conn, id, "typo"),
            Err(ReviewError::NotConfirmed)
        ));
        confirm_grade(conn, id, 0).unwrap();
        assert!(matches!(
            reopen_grade(conn, id, "  "),
            Err(ReviewError::InvalidReason)
        ));
        reopen_grade(conn, id, "Sum error in C2 for Test Student").unwrap();
        let (status, confirmed): (String, i64) = conn
            .query_row(
                "SELECT status_pipeline, (SELECT COUNT(*) FROM results WHERE submission_id=?1 AND confirmed_by_teacher=1) FROM submissions WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((status.as_str(), confirmed), ("redacted", 0));
        assert_eq!(confirm_grade(conn, id, 0).unwrap(), 7.5);
        let reopened: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM logs_audit WHERE event='grade_reopened'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reopened, 1);
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM logs_audit WHERE event='grade_reopened'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(payload.contains("Sum error in C2 for ") && !payload.contains("Student"));
    }

    #[test]
    fn rejects_confirmation_of_an_obsolete_assessment_without_writes() {
        let directory = tempdir().unwrap();
        let state = schema::open_with_key(directory.path(), [5u8; 32]).unwrap();
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let id = prepare_submission_with_results(conn);
        conn.execute("UPDATE submissions SET grade_revision=1 WHERE id=?1", [id])
            .unwrap();
        assert!(matches!(
            confirm_grade(conn, id, 0),
            Err(ReviewError::StaleAssessment)
        ));
        let status: String = conn
            .query_row(
                "SELECT status_pipeline FROM submissions WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "redacted");
        let confirmed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM results WHERE submission_id=?1 AND confirmed_by_teacher=1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(confirmed, 0);
        assert_eq!(confirm_grade(conn, id, 1).unwrap(), 7.5);
    }

    #[test]
    fn rejects_incomplete_assessment_without_changing_status() {
        let directory = tempdir().unwrap();
        let state = schema::open_with_key(directory.path(), [5u8; 32]).unwrap();
        let guard = state.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let id = prepare_submission_with_results(conn);
        conn.execute(
            "UPDATE results SET score_final = NULL WHERE submission_id = ?1",
            [id],
        )
        .unwrap();
        assert!(matches!(
            confirm_grade(conn, id, 0),
            Err(ReviewError::IncompleteAssessment)
        ));
        let status: String = conn
            .query_row(
                "SELECT status_pipeline FROM submissions WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "redacted");
    }
}
