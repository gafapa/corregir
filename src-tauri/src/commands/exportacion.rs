use std::path::Path;

use tauri::State;

use crate::db::DbState;
use crate::modelos::LogAuditoria;
use crate::pipeline::exportacion;

#[tauri::command]
pub fn cmd_exportar_csv(
    db: State<DbState>,
    enunciado_id: i64,
    ruta_destino: String,
) -> Result<usize, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    exportacion::generar_csv(&conn, enunciado_id, Path::new(&ruta_destino)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_listar_logs_auditoria(
    db: State<DbState>,
    entrega_id: Option<i64>,
) -> Result<Vec<LogAuditoria>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    exportacion::listar_logs(&conn, entrega_id).map_err(|e| e.to_string())
}
