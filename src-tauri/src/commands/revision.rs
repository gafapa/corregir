use tauri::State;

use crate::db::DbState;
use crate::pipeline::revision;

/// Hito 6: segundo checkpoint humano obligatorio. Ver
/// `pipeline::revision::confirmar_nota`.
#[tauri::command]
pub fn cmd_confirmar_nota(db: State<DbState>, entrega_id: i64) -> Result<f64, String> {
    let total = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        revision::confirmar_nota(&conn, entrega_id).map_err(|e| e.to_string())?
    };
    db.sellar().map_err(|e| e.to_string())?;
    Ok(total)
}
