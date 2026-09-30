use tauri::State;

use crate::db::DbState;
use crate::pipeline::review;

/// Milestone 6: second mandatory human review checkpoint. See
/// `pipeline::review::confirm_grade`.
#[tauri::command]
pub async fn cmd_confirm_grade(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
) -> Result<f64, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        let total = {
            let guard = db.conn.lock().map_err(|e| e.to_string())?;
            let conn = guard.as_ref().ok_or("the database is closed")?;
            {
                let total =
                    review::confirm_grade(conn, submission_id).map_err(|e| e.to_string())?;
                db.persist(conn).map_err(|e| e.to_string())?;
                total
            }
        };
        Ok(total)
    })
    .await
}
