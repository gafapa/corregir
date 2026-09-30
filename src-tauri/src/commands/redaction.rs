use tauri::State;

use crate::db::DbState;
use crate::pipeline::identifiers::{self, IdentifierCandidate};
use crate::pipeline::redaction;

#[tauri::command]
pub async fn cmd_detect_identifiers(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
    roster: Vec<String>,
) -> Result<Vec<IdentifierCandidate>, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("the database is closed")?;
        let text: Option<String> = conn
            .query_row(
                "SELECT text_ocr FROM submissions WHERE id = ?1",
                [submission_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let text = text.ok_or_else(|| format!("submission {submission_id} has no OCR text"))?;
        Ok(identifiers::detect_all(&text, &roster))
    })
    .await
}

/// Apply only the spans approved by the teacher during human review.
#[tauri::command]
pub async fn cmd_confirm_redaction(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
    spans: Vec<(usize, usize)>,
    student_name: String,
    student_class_id: Option<String>,
) -> Result<String, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        let alias = {
            let guard = db.conn.lock().map_err(|e| e.to_string())?;
            let conn = guard.as_ref().ok_or("the database is closed")?;
            let confirmed = redaction::confirm(
                conn,
                submission_id,
                &spans,
                &student_name,
                student_class_id.as_deref(),
            )
            .map_err(|e| e.to_string())?;
            db.persist(conn).map_err(|e| e.to_string())?;
            confirmed.alias().to_string()
        };
        Ok(alias)
    })
    .await
}
