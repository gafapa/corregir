//! Segundo checkpoint humano obligatorio (Hito 6): confirmar como oficial la
//! nota que el profesor ya guardó como tentativa (Hito 5). La IA nunca pone
//! la nota — el profesor la introduce (`cmd_guardar_nota_tentativa`) y aquí
//! solo la confirma.

use rusqlite::Connection;

pub fn confirmar_nota(conn: &Connection, entrega_id: i64) -> Result<f64, rusqlite::Error> {
    conn.execute(
        "UPDATE resultados SET confirmado_por_docente = 1,
            confirmado_en = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE entrega_id = ?1",
        [entrega_id],
    )?;

    conn.execute(
        "UPDATE entregas SET estado_pipeline = 'nota_confirmada' WHERE id = ?1",
        [entrega_id],
    )?;

    let total: Option<f64> = conn.query_row(
        "SELECT SUM(puntuacion_final) FROM resultados WHERE entrega_id = ?1",
        [entrega_id],
        |r| r.get(0),
    )?;

    conn.execute(
        "INSERT INTO logs_auditoria (entrega_id, evento, actor, payload_json)
         VALUES (?1, 'nota_confirmada', 'profesor', ?2)",
        rusqlite::params![
            entrega_id,
            serde_json::json!({ "nota_total": total }).to_string()
        ],
    )?;

    Ok(total.unwrap_or(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use tempfile::tempdir;

    fn preparar_entrega_con_resultados(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO rubricas (titulo, asignatura, curso, contenido_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubrica_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO criterios_rubrica (rubrica_id, codigo, descripcion, puntuacion_max, orden)
             VALUES (?1, 'C1', 'criterio 1', 5, 0), (?1, 'C2', 'criterio 2', 5, 1)",
            [rubrica_id],
        )
        .unwrap();
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

        let c1: i64 = conn
            .query_row("SELECT id FROM criterios_rubrica WHERE codigo='C1'", [], |r| r.get(0))
            .unwrap();
        let c2: i64 = conn
            .query_row("SELECT id FROM criterios_rubrica WHERE codigo='C2'", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO resultados (entrega_id, criterio_id, puntuacion_final) VALUES (?1, ?2, 3.5), (?1, ?3, 4.0)",
            rusqlite::params![entrega_id, c1, c2],
        )
        .unwrap();

        entrega_id
    }

    #[test]
    fn confirmar_nota_suma_puntuaciones_y_marca_estado() {
        let dir = tempdir().unwrap();
        let estado = schema::abrir_con_clave(dir.path(), [5u8; 32]).unwrap();
        let conn = estado.conn.lock().unwrap();
        let entrega_id = preparar_entrega_con_resultados(&conn);

        let total = confirmar_nota(&conn, entrega_id).unwrap();
        assert_eq!(total, 7.5);

        let estado_pipeline: String = conn
            .query_row("SELECT estado_pipeline FROM entregas WHERE id = ?1", [entrega_id], |r| r.get(0))
            .unwrap();
        assert_eq!(estado_pipeline, "nota_confirmada");

        let confirmados: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM resultados WHERE entrega_id = ?1 AND confirmado_por_docente = 1",
                [entrega_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(confirmados, 2);
    }
}
