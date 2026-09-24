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
}
