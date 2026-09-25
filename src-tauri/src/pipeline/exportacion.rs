//! Exportación (Hito 6) y trazabilidad (Hito 7). `generar_csv` es la única
//! función de todo el pipeline que resuelve alias -> nombre real, y solo en
//! el momento de escribir el fichero — ver "frontera de privacidad" en
//! docs/ARQUITECTURA.md.

use std::path::Path;

use rusqlite::Connection;
use thiserror::Error;

use crate::modelos::LogAuditoria;

#[derive(Debug, Error)]
pub enum ExportacionError {
    #[error("error de base de datos: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("error escribiendo el CSV: {0}")]
    Csv(#[from] csv::Error),
    #[error("error de E/S al finalizar el CSV: {0}")]
    Io(#[from] std::io::Error),
    #[error("error generando el PDF: {0}")]
    Pdf(String),
}

/// Corta `texto` en líneas de como máximo `ancho` caracteres, respetando
/// palabras completas — evita que una hoja de feedback larga se salga de la
/// página.
fn envolver_texto(texto: &str, ancho: usize) -> Vec<String> {
    let mut lineas = Vec::new();
    let mut actual = String::new();
    for palabra in texto.split_whitespace() {
        if actual.is_empty() {
            actual.push_str(palabra);
        } else if actual.len() + 1 + palabra.len() <= ancho {
            actual.push(' ');
            actual.push_str(palabra);
        } else {
            lineas.push(std::mem::take(&mut actual));
            actual.push_str(palabra);
        }
    }
    if !actual.is_empty() {
        lineas.push(actual);
    }
    lineas
}

/// Hito 6: hoja de feedback en PDF para un alumno. Igual que `generar_csv`,
/// es de las pocas funciones que resuelve alias -> nombre real, solo al
/// escribir el fichero.
pub fn generar_pdf_feedback(
    conn: &Connection,
    entrega_id: i64,
    ruta_fuente: &Path,
    ruta_destino: &Path,
) -> Result<(), ExportacionError> {
    use printpdf::*;

    let alumno_nombre: String = conn
        .query_row(
            "SELECT m.alumno_nombre FROM entregas e
             JOIN alias_alumno_map m ON m.alias = e.alias
             WHERE e.id = ?1",
            [entrega_id],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "(alumno sin identificar)".to_string());

    let mut stmt = conn.prepare(
        "SELECT cr.codigo, cr.descripcion, cr.puntuacion_max, r.puntuacion_final, r.comentario_docente
         FROM resultados r JOIN criterios_rubrica cr ON cr.id = r.criterio_id
         WHERE r.entrega_id = ?1 ORDER BY cr.orden",
    )?;
    let filas: Vec<(String, String, f64, Option<f64>, Option<String>)> = stmt
        .query_map([entrega_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let comentario_feedback: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM logs_auditoria
             WHERE entrega_id = ?1 AND evento = 'llamada_b_feedback'
             ORDER BY id DESC LIMIT 1",
            [entrega_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|v| {
            v.get("comentario_feedback")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
        });

    let total: f64 = filas.iter().filter_map(|(_, _, _, p, _)| *p).sum();

    let mut lineas: Vec<String> = vec![format!("Corrección — {alumno_nombre}"), String::new()];
    for (codigo, descripcion, max, puntuacion, comentario) in &filas {
        let puntuacion_texto = puntuacion
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string());
        lineas.push(format!("{codigo} ({puntuacion_texto}/{max}): {descripcion}"));
        if let Some(c) = comentario {
            if !c.trim().is_empty() {
                for linea in envolver_texto(&format!("  Comentario del profesor: {c}"), 90) {
                    lineas.push(linea);
                }
            }
        }
        lineas.push(String::new());
    }
    lineas.push(format!("Nota total: {total}"));
    if let Some(feedback) = comentario_feedback {
        lineas.push(String::new());
        lineas.push("Feedback:".to_string());
        lineas.extend(envolver_texto(&feedback, 90));
    }

    let font_bytes = std::fs::read(ruta_fuente)?;
    let mut warnings_fuente = Vec::new();
    let font = ParsedFont::from_bytes(&font_bytes, 0, &mut warnings_fuente)
        .ok_or_else(|| ExportacionError::Pdf("no se pudo interpretar el fichero de fuente".to_string()))?;

    let mut doc = PdfDocument::new("Corregir - Feedback");
    let font_id = doc.add_font(&font);

    let mut ops = vec![
        Op::StartTextSection,
        Op::SetTextCursor {
            pos: Point {
                x: Mm(15.0).into(),
                y: Mm(280.0).into(),
            },
        },
        Op::SetLineHeight { lh: Pt(16.0) },
        Op::SetFont {
            font: PdfFontHandle::External(font_id.clone()),
            size: Pt(11.0),
        },
    ];
    for linea in lineas {
        ops.push(Op::ShowText {
            items: vec![TextItem::Text(linea)],
        });
        ops.push(Op::AddLineBreak);
    }
    ops.push(Op::EndTextSection);

    let page = PdfPage::new(Mm(210.0), Mm(297.0), ops);
    let mut warnings_guardado = Vec::new();
    let bytes = doc.with_pages(vec![page]).save(
        &PdfSaveOptions {
            subset_fonts: true,
            ..Default::default()
        },
        &mut warnings_guardado,
    );
    std::fs::write(ruta_destino, bytes)?;

    Ok(())
}

pub fn generar_csv(
    conn: &Connection,
    enunciado_id: i64,
    ruta_destino: &Path,
) -> Result<usize, ExportacionError> {
    let mut stmt_entregas = conn.prepare(
        "SELECT id, alias FROM entregas
         WHERE enunciado_id = ?1 AND estado_pipeline = 'nota_confirmada'",
    )?;
    let entregas: Vec<(i64, Option<String>)> = stmt_entregas
        .query_map([enunciado_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut escritor = csv::Writer::from_path(ruta_destino)?;
    escritor.write_record(["alumno", "nota_total", "detalle_por_criterio"])?;

    let mut filas_escritas = 0;
    for (entrega_id, alias) in &entregas {
        let alumno_nombre: String = match alias {
            Some(alias) => conn.query_row(
                "SELECT alumno_nombre FROM alias_alumno_map WHERE alias = ?1",
                [alias],
                |r| r.get(0),
            )?,
            None => "(sin alias registrado)".to_string(),
        };

        let mut stmt_res = conn.prepare(
            "SELECT cr.codigo, r.puntuacion_final FROM resultados r
             JOIN criterios_rubrica cr ON cr.id = r.criterio_id
             WHERE r.entrega_id = ?1 ORDER BY cr.orden",
        )?;
        let filas: Vec<(String, Option<f64>)> = stmt_res
            .query_map([entrega_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let total: f64 = filas.iter().filter_map(|(_, p)| *p).sum();
        let detalle = filas
            .iter()
            .map(|(codigo, puntuacion)| {
                format!("{codigo}={}", puntuacion.map(|v| v.to_string()).unwrap_or_default())
            })
            .collect::<Vec<_>>()
            .join("; ");

        escritor.write_record([alumno_nombre, total.to_string(), detalle])?;
        filas_escritas += 1;
    }
    escritor.flush()?;

    conn.execute(
        "INSERT INTO logs_auditoria (evento, actor, payload_json) VALUES ('exportacion_csv', 'profesor', ?1)",
        [serde_json::json!({ "enunciado_id": enunciado_id, "filas": filas_escritas }).to_string()],
    )?;

    Ok(filas_escritas)
}

/// Hito 7: exporta el registro de trazabilidad a CSV. No resuelve nombres
/// reales (el log nunca los contiene, ver comandos que escriben en
/// `logs_auditoria`) — es seguro compartir este fichero con el DPO sin más
/// tratamiento.
pub fn generar_csv_logs(
    conn: &Connection,
    entrega_id: Option<i64>,
    ruta_destino: &Path,
) -> Result<usize, ExportacionError> {
    let logs = listar_logs(conn, entrega_id)?;
    let mut escritor = csv::Writer::from_path(ruta_destino)?;
    escritor.write_record(["fecha", "entrega_id", "evento", "actor", "version_modelo", "payload_json"])?;
    for log in &logs {
        escritor.write_record([
            log.creado_en.clone(),
            log.entrega_id.map(|v| v.to_string()).unwrap_or_default(),
            log.evento.clone(),
            log.actor.clone(),
            log.version_modelo.clone().unwrap_or_default(),
            log.payload_json.clone().unwrap_or_default(),
        ])?;
    }
    escritor.flush()?;
    Ok(logs.len())
}

pub fn listar_logs(
    conn: &Connection,
    entrega_id: Option<i64>,
) -> Result<Vec<LogAuditoria>, ExportacionError> {
    let mut stmt = conn.prepare(
        "SELECT id, entrega_id, evento, actor, version_modelo, payload_json, creado_en
         FROM logs_auditoria
         WHERE ?1 IS NULL OR entrega_id = ?1
         ORDER BY id",
    )?;
    let logs = stmt
        .query_map([entrega_id], |r| {
            Ok(LogAuditoria {
                id: r.get(0)?,
                entrega_id: r.get(1)?,
                evento: r.get(2)?,
                actor: r.get(3)?,
                version_modelo: r.get(4)?,
                payload_json: r.get(5)?,
                creado_en: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(logs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use crate::pipeline::{anonimizacion, revision};
    use tempfile::tempdir;

    #[test]
    fn exporta_solo_entregas_con_nota_confirmada_y_resuelve_el_nombre_real() {
        let dir_bd = tempdir().unwrap();
        let estado = schema::abrir_con_clave(dir_bd.path(), [6u8; 32]).unwrap();
        let conn = estado.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO rubricas (titulo, asignatura, curso, contenido_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubrica_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO criterios_rubrica (rubrica_id, codigo, descripcion, puntuacion_max, orden)
             VALUES (?1, 'C1', 'criterio 1', 10, 0)",
            [rubrica_id],
        )
        .unwrap();
        let criterio_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO enunciados (rubrica_id, texto) VALUES (?1, 'enunciado')",
            [rubrica_id],
        )
        .unwrap();
        let enunciado_id = conn.last_insert_rowid();

        // Entrega 1: confirmada, debe aparecer en el CSV con el nombre real.
        conn.execute(
            "INSERT INTO entregas (enunciado_id, texto_ocr, estado_pipeline) VALUES (?1, 'texto1', 'ocr_completado')",
            [enunciado_id],
        )
        .unwrap();
        let entrega1 = conn.last_insert_rowid();
        anonimizacion::confirmar(&conn, entrega1, &[], "Maria Real", None).unwrap();
        conn.execute(
            "INSERT INTO resultados (entrega_id, criterio_id, puntuacion_final) VALUES (?1, ?2, 8.5)",
            rusqlite::params![entrega1, criterio_id],
        )
        .unwrap();
        revision::confirmar_nota(&conn, entrega1).unwrap();

        // Entrega 2: aún no confirmada, NO debe aparecer en el CSV.
        conn.execute(
            "INSERT INTO entregas (enunciado_id, texto_ocr, estado_pipeline) VALUES (?1, 'texto2', 'ocr_completado')",
            [enunciado_id],
        )
        .unwrap();
        let entrega2 = conn.last_insert_rowid();
        anonimizacion::confirmar(&conn, entrega2, &[], "Otro Alumno", None).unwrap();
        conn.execute(
            "INSERT INTO resultados (entrega_id, criterio_id, puntuacion_final) VALUES (?1, ?2, 2.0)",
            rusqlite::params![entrega2, criterio_id],
        )
        .unwrap();

        let dir_csv = tempdir().unwrap();
        let ruta_csv = dir_csv.path().join("notas.csv");
        let filas = generar_csv(&conn, enunciado_id, &ruta_csv).unwrap();
        assert_eq!(filas, 1);

        let contenido = std::fs::read_to_string(&ruta_csv).unwrap();
        assert!(contenido.contains("Maria Real"));
        assert!(contenido.contains("8.5"));
        assert!(!contenido.contains("Otro Alumno"));
    }

    #[test]
    fn genera_pdf_de_feedback_con_contenido_esperado() {
        let dir_bd = tempdir().unwrap();
        let estado = schema::abrir_con_clave(dir_bd.path(), [12u8; 32]).unwrap();
        let conn = estado.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO rubricas (titulo, asignatura, curso, contenido_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubrica_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO criterios_rubrica (rubrica_id, codigo, descripcion, puntuacion_max, orden)
             VALUES (?1, 'C1', 'criterio uno', 10, 0)",
            [rubrica_id],
        )
        .unwrap();
        let criterio_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO enunciados (rubrica_id, texto) VALUES (?1, 'enunciado')",
            [rubrica_id],
        )
        .unwrap();
        let enunciado_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO entregas (enunciado_id, texto_ocr, estado_pipeline) VALUES (?1, 'texto', 'ocr_completado')",
            [enunciado_id],
        )
        .unwrap();
        let entrega_id = conn.last_insert_rowid();
        anonimizacion::confirmar(&conn, entrega_id, &[], "Alumno PDF", None).unwrap();
        conn.execute(
            "INSERT INTO resultados (entrega_id, criterio_id, puntuacion_final, comentario_docente)
             VALUES (?1, ?2, 7.0, 'buen trabajo')",
            rusqlite::params![entrega_id, criterio_id],
        )
        .unwrap();
        revision::confirmar_nota(&conn, entrega_id).unwrap();
        conn.execute(
            "INSERT INTO logs_auditoria (entrega_id, evento, actor, payload_json)
             VALUES (?1, 'llamada_b_feedback', 'ia', ?2)",
            rusqlite::params![
                entrega_id,
                serde_json::json!({ "comentario_feedback": "Buen trabajo en general.", "inconsistencias": [] })
                    .to_string()
            ],
        )
        .unwrap();

        let ruta_fuente = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/fonts/Roboto-Regular.ttf");
        let dir_pdf = tempdir().unwrap();
        let ruta_pdf = dir_pdf.path().join("feedback.pdf");

        generar_pdf_feedback(&conn, entrega_id, &ruta_fuente, &ruta_pdf).unwrap();

        let bytes = std::fs::read(&ruta_pdf).unwrap();
        assert!(bytes.starts_with(b"%PDF"), "el fichero generado no parece un PDF válido");
        assert!(bytes.len() > 500, "el PDF generado parece sospechosamente pequeño");
    }

    #[test]
    fn exporta_el_log_de_auditoria_sin_nombres_reales() {
        let dir_bd = tempdir().unwrap();
        let estado = schema::abrir_con_clave(dir_bd.path(), [11u8; 32]).unwrap();
        let conn = estado.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO rubricas (titulo, asignatura, curso, contenido_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubrica_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO enunciados (rubrica_id, texto) VALUES (?1, 'enunciado')",
            [rubrica_id],
        )
        .unwrap();
        let enunciado_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO entregas (enunciado_id, texto_ocr, estado_pipeline) VALUES (?1, 'texto', 'ocr_completado')",
            [enunciado_id],
        )
        .unwrap();
        let entrega_id = conn.last_insert_rowid();
        anonimizacion::confirmar(&conn, entrega_id, &[], "Nombre Que No Debe Salir", None).unwrap();
        revision::confirmar_nota(&conn, entrega_id).unwrap();

        let dir_csv = tempdir().unwrap();
        let ruta_csv = dir_csv.path().join("log.csv");
        let filas = generar_csv_logs(&conn, None, &ruta_csv).unwrap();
        assert!(filas >= 2); // al menos anonimizacion_confirmada + nota_confirmada

        let contenido = std::fs::read_to_string(&ruta_csv).unwrap();
        assert!(!contenido.contains("Nombre Que No Debe Salir"));
        assert!(contenido.contains("anonimizacion_confirmada"));
        assert!(contenido.contains("nota_confirmada"));
    }
}
