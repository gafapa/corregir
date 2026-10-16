use tauri::State;

use crate::db::DbState;
use crate::pipeline::review;

/// Milestone 6: second mandatory human review checkpoint. See
/// `pipeline::review::confirm_grade`.
#[tauri::command]
pub async fn cmd_confirm_grade(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
    expected_revision: i64,
) -> Result<f64, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        db.write(|conn| {
            review::confirm_grade(conn, submission_id, expected_revision).map_err(|e| e.to_string())
        })
    })
    .await
}

/// Reopen a confirmed grade so the teacher can correct it; the reason is audited.
#[tauri::command]
pub async fn cmd_reopen_grade(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
    reason: String,
) -> Result<(), String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        db.write(|conn| {
            review::reopen_grade(conn, submission_id, &reason).map_err(|e| e.to_string())
        })
    })
    .await
}
