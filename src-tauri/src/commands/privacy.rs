//! Data lifecycle: retention reminders, identity purging (pseudonymization) and the
//! per-student access and erasure requests described in docs/DPIA-FRIA.md.
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::db::DbState;
use crate::pipeline::redaction;

/// Unicode-aware comparison key for names typed in different cases or spacing.
fn name_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[derive(Debug, Serialize)]
pub struct PurgeSummary {
    purged: usize,
    pending: usize,
}

/// Pseudonymize every confirmed submission of an assignment: remove the alias-to-name
/// link, the unredacted extraction and the source file name, and replace known names
/// in teacher comments and feedback. Scores, redacted text and feedback remain.
#[tauri::command]
pub async fn cmd_purge_assignment_identities(
    db: State<'_, std::sync::Arc<DbState>>,
    assignment_id: i64,
) -> Result<PurgeSummary, String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        db.write(|conn| purge_assignment(conn, assignment_id))
    })
    .await
}

fn purge_assignment(conn: &mut Connection, assignment_id: i64) -> Result<PurgeSummary, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let confirmed: Vec<i64> = tx
        .prepare(
            "SELECT s.id FROM submissions s JOIN alias_student_map m ON m.alias = s.alias
             WHERE s.assignment_id = ?1 AND s.status_pipeline = 'grade_confirmed'",
        )
        .map_err(|e| e.to_string())?
        .query_map([assignment_id], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    // Rewrite every text first: each redactor reads the other students' names,
    // which the second loop removes.
    for &id in &confirmed {
        let redactor = redaction::known_name_redactor(&tx, id).map_err(|e| e.to_string())?;
        rewrite_teacher_text(&tx, id, &redactor)?;
    }
    for &id in &confirmed {
        tx.execute("DELETE FROM alias_student_map WHERE submission_id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE submissions SET text_ocr=NULL, text_initial=NULL, source_name=NULL, source_sha256=NULL,
                    feedback_json=NULL, identities_purged_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE id=?1",
            [id],
        )
        .map_err(|e| e.to_string())?;
    }
    let pending: usize = tx
        .query_row(
            "SELECT COUNT(*) FROM submissions WHERE assignment_id=?1 AND status_pipeline <> 'grade_confirmed'",
            [assignment_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())? as usize;
    tx.execute(
        "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('identities_purged', 'teacher', ?1)",
        [serde_json::json!({"assignment_id": assignment_id, "submissions": confirmed.len()}).to_string()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(PurgeSummary {
        purged: confirmed.len(),
        pending,
    })
}

/// Apply a name redactor to the teacher-written and AI-drafted text of one submission.
/// Returns how many fields changed.
fn rewrite_teacher_text(
    conn: &Connection,
    submission_id: i64,
    redactor: &redaction::NameRedactor,
) -> Result<usize, String> {
    let mut changed = 0;
    let (feedback, draft): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT feedback_final, feedback_json FROM submissions WHERE id=?1",
            [submission_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    for (column, value) in [("feedback_final", feedback), ("feedback_json", draft)] {
        let Some(value) = value else { continue };
        let redacted = redactor.apply(&value);
        if redacted != value {
            conn.execute(
                &format!("UPDATE submissions SET {column}=?1 WHERE id=?2"),
                params![redacted, submission_id],
            )
            .map_err(|e| e.to_string())?;
            changed += 1;
        }
    }
    let comments: Vec<(i64, String)> = conn
        .prepare("SELECT id, comment_teacher FROM results WHERE submission_id=?1 AND comment_teacher IS NOT NULL")
        .map_err(|e| e.to_string())?
        .query_map([submission_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    for (result_id, comment) in comments {
        let redacted = redactor.apply(&comment);
        if redacted != comment {
            conn.execute(
                "UPDATE results SET comment_teacher=?1 WHERE id=?2",
                params![redacted, result_id],
            )
            .map_err(|e| e.to_string())?;
            changed += 1;
        }
    }
    Ok(changed)
}

#[derive(Debug, Serialize)]
pub struct KnownStudent {
    name: String,
    submissions: usize,
    roster_entries: usize,
}

/// Every student name held in the workspace, from redaction links and class rosters.
#[tauri::command]
pub async fn cmd_list_known_students(
    db: State<'_, std::sync::Arc<DbState>>,
) -> Result<Vec<KnownStudent>, String> {
    crate::tasks::run(db.inner().clone(), |db| {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        list_known_students(guard.as_ref().ok_or("the database is closed")?)
    })
    .await
}

fn list_known_students(conn: &Connection) -> Result<Vec<KnownStudent>, String> {
    let mut students: std::collections::BTreeMap<String, KnownStudent> = Default::default();
    let names = |sql: &str| -> Result<Vec<String>, String> {
        conn.prepare(sql)
            .map_err(|e| e.to_string())?
            .query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())
    };
    for name in names("SELECT student_name FROM alias_student_map")? {
        students
            .entry(name_key(&name))
            .or_insert_with(|| KnownStudent {
                name: name.trim().to_string(),
                submissions: 0,
                roster_entries: 0,
            })
            .submissions += 1;
    }
    for name in names("SELECT full_name FROM students")? {
        students
            .entry(name_key(&name))
            .or_insert_with(|| KnownStudent {
                name: name.trim().to_string(),
                submissions: 0,
                roster_entries: 0,
            })
            .roster_entries += 1;
    }
    Ok(students.into_values().collect())
}

/// Submissions whose confirmed identity matches `name`.
fn submissions_of(conn: &Connection, name: &str) -> Result<Vec<i64>, String> {
    let key = name_key(name);
    let rows: Vec<(i64, String)> = conn
        .prepare("SELECT submission_id, student_name FROM alias_student_map ORDER BY submission_id")
        .map_err(|e| e.to_string())?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter(|(_, student)| name_key(student) == key)
        .map(|(id, _)| id)
        .collect())
}

fn roster_entries_of(conn: &Connection, name: &str) -> Result<Vec<(i64, String)>, String> {
    let key = name_key(name);
    let rows: Vec<(i64, String, String)> = conn
        .prepare("SELECT st.id, st.full_name, c.name FROM students st JOIN classes c ON c.id = st.class_id")
        .map_err(|e| e.to_string())?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter(|(_, student, _)| name_key(student) == key)
        .map(|(id, _, class)| (id, class))
        .collect())
}

fn student_export(conn: &Connection, name: &str) -> Result<serde_json::Value, String> {
    let mut submissions = Vec::new();
    for id in submissions_of(conn, name)? {
        let row = conn
            .query_row(
                "SELECT a.title, a.text, r.title, r.subject, r.grade_level, r.version, s.created_at,
                        s.source_name, s.method_ocr, s.text_ocr, s.text_initial, s.text_redacted,
                        s.status_pipeline, s.feedback_final, s.feedback_ai_assisted
                 FROM submissions s JOIN assignments a ON a.id = s.assignment_id
                 JOIN rubrics r ON r.id = a.rubric_id WHERE s.id = ?1",
                [id],
                |row| {
                    Ok(serde_json::json!({
                        "submission_id": id,
                        "assignment": {"title": row.get::<_, Option<String>>(0)?, "instructions": row.get::<_, String>(1)?},
                        "rubric": {"title": row.get::<_, String>(2)?, "subject": row.get::<_, String>(3)?, "grade_level": row.get::<_, String>(4)?, "version": row.get::<_, i64>(5)?},
                        "imported_at": row.get::<_, String>(6)?,
                        "source_file_name": row.get::<_, Option<String>>(7)?,
                        "extraction_method": row.get::<_, Option<String>>(8)?,
                        "extracted_text": row.get::<_, Option<String>>(9)?,
                        "initial_extraction_before_corrections": row.get::<_, Option<String>>(10)?,
                        "redacted_text_sent_to_local_model": row.get::<_, Option<String>>(11)?,
                        "status": row.get::<_, String>(12)?,
                        "feedback": row.get::<_, Option<String>>(13)?,
                        "feedback_ai_assisted": row.get::<_, bool>(14)?,
                    }))
                },
            )
            .map_err(|e| e.to_string())?;
        let results: Vec<serde_json::Value> = conn
            .prepare(
                "SELECT cr.code, cr.description, cr.score_max, r.score_final, r.comment_teacher,
                        r.ai_evidence_json, r.confirmed_at
                 FROM results r JOIN criteria_rubric cr ON cr.id = r.criterion_id
                 WHERE r.submission_id = ?1 ORDER BY cr.sort_order",
            )
            .map_err(|e| e.to_string())?
            .query_map([id], |row| {
                let evidence: Option<String> = row.get(5)?;
                Ok(serde_json::json!({
                    "criterion": row.get::<_, String>(0)?,
                    "description": row.get::<_, String>(1)?,
                    "maximum": row.get::<_, f64>(2)?,
                    "score": row.get::<_, Option<f64>>(3)?,
                    "teacher_comment": row.get::<_, Option<String>>(4)?,
                    "ai_evidence": evidence.and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok()),
                    "confirmed_at": row.get::<_, Option<String>>(6)?,
                }))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let audit: Vec<serde_json::Value> = conn
            .prepare("SELECT created_at, event, actor, model_version FROM logs_audit WHERE submission_id=?1 ORDER BY id")
            .map_err(|e| e.to_string())?
            .query_map([id], |row| {
                Ok(serde_json::json!({
                    "date": row.get::<_, String>(0)?,
                    "event": row.get::<_, String>(1)?,
                    "actor": row.get::<_, String>(2)?,
                    "model": row.get::<_, Option<String>>(3)?,
                }))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let mut entry = row;
        entry["results"] = serde_json::Value::Array(results);
        entry["processing_history"] = serde_json::Value::Array(audit);
        submissions.push(entry);
    }
    let classes: Vec<String> = roster_entries_of(conn, name)?
        .into_iter()
        .map(|(_, class)| class)
        .collect();
    Ok(serde_json::json!({
        "format": "corregir-student-data/1",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "student": name.trim(),
        "classes": classes,
        "submissions": submissions,
        "notes": "Original documents are not stored by Corregir. Encrypted backups created before this export may contain earlier copies of these records.",
    }))
}

/// Export everything the workspace holds about one student (access request) as JSON.
#[tauri::command]
pub async fn cmd_export_student_data(
    app: AppHandle,
    db: State<'_, std::sync::Arc<DbState>>,
    student_name: String,
    destination_path: String,
) -> Result<usize, String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        let destination =
            super::export::validate_destination(&app, db, &destination_path, "json")?;
        db.write(|conn| {
            let data = student_export(conn, &student_name)?;
            let count = data["submissions"].as_array().map_or(0, Vec::len);
            if count == 0 && data["classes"].as_array().is_none_or(Vec::is_empty) {
                return Err("no records were found for this student".into());
            }
            let bytes = serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?;
            crate::pipeline::export::write_atomic(&destination, &bytes)
                .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('student_data_exported', 'teacher', ?1)",
                [serde_json::json!({"submissions": count}).to_string()],
            )
            .map_err(|e| e.to_string())?;
            Ok(count)
        })
    })
    .await
}

#[derive(Debug, Serialize)]
pub struct EraseSummary {
    submissions: usize,
    roster_entries: usize,
    mentions: usize,
}

/// Erase every submission and roster entry of one student (erasure request).
#[tauri::command]
pub async fn cmd_erase_student(
    db: State<'_, std::sync::Arc<DbState>>,
    student_name: String,
) -> Result<EraseSummary, String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        db.write(|conn| erase_student(conn, &student_name))
    })
    .await
}

fn erase_student(conn: &mut Connection, name: &str) -> Result<EraseSummary, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let submissions = submissions_of(&tx, name)?;
    let roster = roster_entries_of(&tx, name)?;
    if submissions.is_empty() && roster.is_empty() {
        return Err("no records were found for this student".into());
    }
    for id in &submissions {
        tx.execute("DELETE FROM submissions WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
    }
    for (id, _) in &roster {
        tx.execute("DELETE FROM students WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
    }
    // Other students' feedback and comments may mention the erased student. Name parts
    // a remaining student shares with the erased one refer to that student and stay.
    let remaining: Vec<(i64, Option<String>)> = tx
        .prepare(
            "SELECT s.id, m.student_name FROM submissions s
             LEFT JOIN alias_student_map m ON m.alias = s.alias ORDER BY s.id",
        )
        .map_err(|e| e.to_string())?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let anonymous = redaction::name_scrubber(name, None).map_err(|e| e.to_string())?;
    let mut mentions = 0;
    for (id, owner) in remaining {
        mentions += match owner {
            Some(owner) => {
                let scrubber =
                    redaction::name_scrubber(name, Some(&owner)).map_err(|e| e.to_string())?;
                rewrite_teacher_text(&tx, id, &scrubber)?
            }
            None => rewrite_teacher_text(&tx, id, &anonymous)?,
        };
    }
    tx.execute(
        "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('student_erased', 'teacher', ?1)",
        [serde_json::json!({"submissions": submissions.len(), "roster_entries": roster.len(), "mentions": mentions}).to_string()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(EraseSummary {
        submissions: submissions.len(),
        roster_entries: roster.len(),
        mentions,
    })
}

#[derive(Debug, Serialize)]
pub struct RetentionDue {
    assignment_id: i64,
    title: Option<String>,
    text: String,
    identified: i64,
    last_confirmed_at: Option<String>,
}

/// Assignments whose identified confirmed grades are older than the retention period.
#[tauri::command]
pub async fn cmd_retention_status(
    db: State<'_, std::sync::Arc<DbState>>,
) -> Result<Vec<RetentionDue>, String> {
    crate::tasks::run(db.inner().clone(), |db| {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        retention_due(
            guard.as_ref().ok_or("the database is closed")?,
            chrono::Utc::now(),
        )
    })
    .await
}

fn retention_due(
    conn: &Connection,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<RetentionDue>, String> {
    let Some(days) = super::settings::load_preferences(conn)?.retention_days else {
        return Ok(Vec::new());
    };
    let cutoff = now - chrono::Duration::days(i64::from(days));
    let mut due = Vec::new();
    for assignment in super::configuration::list_assignments(conn)? {
        let last = assignment
            .last_confirmed_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());
        if assignment.identified > 0 && last.is_some_and(|last| last < cutoff) {
            due.push(RetentionDue {
                assignment_id: assignment.id,
                title: assignment.title,
                text: assignment.text,
                identified: assignment.identified,
                last_confirmed_at: assignment.last_confirmed_at,
            });
        }
    }
    Ok(due)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{redaction, review};

    fn workspace() -> (tempfile::TempDir, crate::db::DbState) {
        let directory = tempfile::tempdir().unwrap();
        let db = crate::db::schema::open_with_key(directory.path(), [13; 32]).unwrap();
        db.write(|conn| {
            conn.execute_batch("INSERT INTO rubrics (id,title,subject,grade_level,content_json) VALUES (1,'Poem','Lingua','3 ESO','{}');
                INSERT INTO criteria_rubric (id,rubric_id,code,description,score_max,sort_order) VALUES (1,1,'C1','Theme',10,0);
                INSERT INTO assignments (id,rubric_id,text,title) VALUES (1,1,'Analyse the poem','Poem');
                INSERT INTO classes (id,name) VALUES (1,'3B'); INSERT INTO students (class_id,full_name) VALUES (1,'Ana Pérez'),(1,'Brais Souto');
                INSERT INTO submissions (id,assignment_id,text_ocr,status_pipeline,source_name) VALUES (1,1,'Ana Pérez writes.','ocr_complete','ana.pdf'),(2,1,'Brais writes.','ocr_complete','brais.pdf'),(3,1,'Pending.','ocr_complete',NULL);").map_err(|e| e.to_string())?;
            redaction::confirm(conn, 1, &[], "Ana Pérez", None).map_err(|e| e.to_string())?;
            redaction::confirm(conn, 2, &[], "Brais Souto", None).map_err(|e| e.to_string())?;
            conn.execute_batch("INSERT INTO results (submission_id,criterion_id,score_final,comment_teacher) VALUES (1,1,8,'Ana, ben feito'),(2,1,6,'Como fixo Ana, revisa o tema');
                UPDATE submissions SET feedback_final='Moi ben, Ana.' WHERE id=1;").map_err(|e| e.to_string())?;
            review::confirm_grade(conn, 1, 0).map_err(|e| e.to_string())?;
            review::confirm_grade(conn, 2, 0).map_err(|e| e.to_string())?;
            Ok(())
        })
        .unwrap();
        (directory, db)
    }

    #[test]
    fn purging_identities_keeps_grades_and_removes_names() {
        let (_directory, db) = workspace();
        let summary = db.write(|conn| purge_assignment(conn, 1)).unwrap();
        assert_eq!((summary.purged, summary.pending), (2, 1));
        let guard = db.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let (names, texts, scores): (i64, i64, f64) = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM alias_student_map),
                        (SELECT COUNT(*) FROM submissions WHERE text_ocr IS NOT NULL OR source_name IS NOT NULL),
                        (SELECT SUM(score_final) FROM results)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((names, texts, scores), (0, 1, 14.0));
        let (feedback, comment): (String, String) = conn
            .query_row(
                "SELECT s.feedback_final, r.comment_teacher FROM submissions s JOIN results r ON r.submission_id=s.id WHERE s.id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(!feedback.contains("Ana") && !comment.contains("Ana"));
        let classmate: String = conn
            .query_row(
                "SELECT comment_teacher FROM results WHERE submission_id=2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(classmate, "Como fixo […], revisa o tema");
        let dump: String = conn
            .query_row(
                "SELECT group_concat(payload_json) FROM logs_audit",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!dump.contains("Ana"));
    }

    #[test]
    fn access_and_erasure_cover_submissions_and_rosters() {
        let (_directory, db) = workspace();
        {
            let guard = db.conn.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            let known = list_known_students(conn).unwrap();
            assert_eq!(known.len(), 2);
            let data = student_export(conn, " ana  PÉREZ ").unwrap();
            assert_eq!(data["submissions"].as_array().unwrap().len(), 1);
            assert_eq!(data["submissions"][0]["results"][0]["score"], 8.0);
            assert_eq!(data["classes"][0], "3B");
        }
        // A different student who shares the given name keeps it in her own feedback.
        db.write(|conn| {
            conn.execute_batch("INSERT INTO submissions (id,assignment_id,text_ocr,status_pipeline) VALUES (4,1,'Text.','ocr_complete')").map_err(|e| e.to_string())?;
            redaction::confirm(conn, 4, &[], "Ana López", None).map_err(|e| e.to_string())?;
            conn.execute("UPDATE submissions SET feedback_final='Moi ben, Ana. Pérez tamén.' WHERE id=4", []).map_err(|e| e.to_string())?;
            Ok(())
        })
        .unwrap();
        let erased = db.write(|conn| erase_student(conn, "Ana Pérez")).unwrap();
        assert_eq!(
            (erased.submissions, erased.roster_entries, erased.mentions),
            (1, 1, 2)
        );
        assert!(db.write(|conn| erase_student(conn, "Ana Pérez")).is_err());
        let guard = db.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM submissions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 3);
        let namesake: String = conn
            .query_row(
                "SELECT feedback_final FROM submissions WHERE id=4",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(namesake, "Moi ben, Ana. […] tamén.");
        let classmate: String = conn
            .query_row(
                "SELECT comment_teacher FROM results WHERE submission_id=2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(classmate, "Como fixo […], revisa o tema");
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM logs_audit WHERE event='student_erased'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!payload.contains("Ana"));
    }

    #[test]
    fn retention_reminders_follow_the_configured_period() {
        let (_directory, db) = workspace();
        let guard = db.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let now = chrono::Utc::now();
        assert!(retention_due(conn, now + chrono::Duration::days(400))
            .unwrap()
            .is_empty());
        conn.execute(
            "INSERT INTO configuration (key,value) VALUES ('preferences','{\"retention_days\":365}')",
            [],
        )
        .unwrap();
        assert!(retention_due(conn, now).unwrap().is_empty());
        let due = retention_due(conn, now + chrono::Duration::days(400)).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].identified, 2);
    }
}
