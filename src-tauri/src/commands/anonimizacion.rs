use tauri::State;

use crate::db::DbState;
use crate::pipeline::anonimizacion;
use crate::pipeline::identificadores::{self, CandidatoIdentificador};

#[tauri::command]
pub fn cmd_detectar_identificadores(
    db: State<DbState>,
    entrega_id: i64,
    roster: Vec<String>,
) -> Result<Vec<CandidatoIdentificador>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let texto: Option<String> = conn
        .query_row(
            "SELECT texto_ocr FROM entregas WHERE id = ?1",
            [entrega_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let texto = texto.ok_or_else(|| format!("la entrega {entrega_id} no tiene texto OCR"))?;
    Ok(identificadores::detectar_todos(&texto, &roster))
}

/// Checkpoint humano obligatorio: `spans` son los rangos (inicio, fin) que
/// el profesor confirmó redactar en la pantalla de revisión, no lo que
/// detectó automáticamente el paso anterior sin más.
#[tauri::command]
pub fn cmd_confirmar_anonimizacion(
    db: State<DbState>,
    entrega_id: i64,
    spans: Vec<(usize, usize)>,
    alumno_nombre: String,
    alumno_id_clase: Option<String>,
) -> Result<String, String> {
    let alias = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let confirmado = anonimizacion::confirmar(
            &conn,
            entrega_id,
            &spans,
            &alumno_nombre,
            alumno_id_clase.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        confirmado.alias().to_string()
    };
    db.sellar().map_err(|e| e.to_string())?;
    Ok(alias)
}
