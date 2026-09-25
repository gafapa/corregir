use std::path::Path;

use tauri::{AppHandle, State};

use crate::db::DbState;
use crate::modelos::EntregaResumen;
use crate::pipeline::{ocr_engine::MotorOcr, render};
use crate::recursos;

/// Hito 3: importa un documento (PDF o imagen suelta) y ejecuta el pipeline
/// de extracción/OCR de inmediato. DOCX queda fuera de alcance de la Fase A
/// (ver nota de simplificación en docs/ARQUITECTURA.md).
///
/// Nota: por simplicidad se combinan aquí lo que el plan describe como dos
/// comandos separados (`cmd_importar_entrega` y `cmd_ejecutar_ocr`); se
/// pueden separar más adelante si la UI necesita reportar progreso
/// intermedio entre ingesta y OCR.
#[tauri::command]
pub fn cmd_importar_entrega(
    app: AppHandle,
    db: State<DbState>,
    motor_ocr: State<MotorOcr>,
    enunciado_id: i64,
    ruta_archivo: String,
) -> Result<i64, String> {
    let ruta = Path::new(&ruta_archivo);
    let extension = ruta
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let (texto, metodo_ocr) = match extension.as_str() {
        "pdf" => {
            let paginas = render::procesar_pdf(&recursos::dir_pdfium(&app), ruta)
                .map_err(|e| e.to_string())?;
            combinar_paginas(&motor_ocr, paginas)?
        }
        "png" | "jpg" | "jpeg" => {
            let imagen = render::cargar_imagen(ruta).map_err(|e| e.to_string())?;
            let texto = motor_ocr
                .reconocer_imagen(&imagen)
                .map_err(|e| e.to_string())?;
            (texto, "ocr_local".to_string())
        }
        otra => {
            return Err(format!(
                "Formato '{otra}' no soportado todavía en la Fase A (solo PDF, PNG, JPG). \
                 DOCX queda pendiente — ver docs/ARQUITECTURA.md."
            ))
        }
    };

    let entrega_id = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO entregas (enunciado_id, texto_ocr, metodo_ocr, estado_pipeline)
             VALUES (?1, ?2, ?3, 'ocr_completado')",
            rusqlite::params![enunciado_id, texto, metodo_ocr],
        )
        .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO logs_auditoria (entrega_id, evento, actor, payload_json)
             VALUES (?1, 'ocr_completado', 'sistema', ?2)",
            rusqlite::params![id, serde_json::json!({ "metodo_ocr": metodo_ocr }).to_string()],
        )
        .map_err(|e| e.to_string())?;

        id
    };

    db.sellar().map_err(|e| e.to_string())?;
    Ok(entrega_id)
}

fn combinar_paginas(
    motor_ocr: &MotorOcr,
    paginas: Vec<render::ContenidoPagina>,
) -> Result<(String, String), String> {
    let mut textos = Vec::new();
    let mut hubo_ocr = false;
    for pagina in paginas {
        match pagina {
            render::ContenidoPagina::TextoNativo(t) => textos.push(t),
            render::ContenidoPagina::ImagenParaOcr(img) => {
                hubo_ocr = true;
                textos.push(
                    motor_ocr
                        .reconocer_imagen(&img)
                        .map_err(|e| e.to_string())?,
                );
            }
        }
    }
    let metodo = if hubo_ocr { "ocr_local" } else { "texto_nativo" };
    Ok((textos.join("\n\n"), metodo.to_string()))
}

#[tauri::command]
pub fn cmd_listar_entregas(
    db: State<DbState>,
    enunciado_id: i64,
) -> Result<Vec<EntregaResumen>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.enunciado_id, e.texto_ocr, e.metodo_ocr, e.estado_pipeline, m.alumno_nombre
             FROM entregas e
             LEFT JOIN alias_alumno_map m ON m.alias = e.alias
             WHERE e.enunciado_id = ?1 ORDER BY e.id",
        )
        .map_err(|e| e.to_string())?;
    let filas = stmt
        .query_map([enunciado_id], |row| {
            Ok(EntregaResumen {
                id: row.get(0)?,
                enunciado_id: row.get(1)?,
                texto_ocr: row.get(2)?,
                metodo_ocr: row.get(3)?,
                estado_pipeline: row.get(4)?,
                alumno_nombre: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(filas)
}
