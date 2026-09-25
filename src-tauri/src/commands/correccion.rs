use serde::Deserialize;
use tauri::State;

use crate::db::DbState;
use crate::pipeline::anonimizacion;
use crate::pipeline::inference_client::InferenceClient;
use crate::pipeline::prompt_builder::{self, CriterioPrompt, EvidenciaCriterio, FeedbackYConsistencia};

const OLLAMA_URL_POR_DEFECTO: &str = "http://127.0.0.1:11434";
const MODELO_POR_DEFECTO: &str = "qwen3:8b";

fn enunciado_y_criterios(
    conn: &rusqlite::Connection,
    entrega_id: i64,
) -> Result<(String, Vec<CriterioPrompt>), String> {
    let (enunciado_id,): (i64,) = conn
        .query_row(
            "SELECT enunciado_id FROM entregas WHERE id = ?1",
            [entrega_id],
            |r| Ok((r.get(0)?,)),
        )
        .map_err(|e| e.to_string())?;
    let (rubrica_id, texto_enunciado): (i64, String) = conn
        .query_row(
            "SELECT rubrica_id, texto FROM enunciados WHERE id = ?1",
            [enunciado_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT codigo, descripcion, puntuacion_max FROM criterios_rubrica
             WHERE rubrica_id = ?1 ORDER BY orden",
        )
        .map_err(|e| e.to_string())?;
    let criterios = stmt
        .query_map([rubrica_id], |r| {
            Ok(CriterioPrompt {
                id: r.get(0)?,
                descripcion: r.get(1)?,
                puntuacion_max: r.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok((texto_enunciado, criterios))
}

fn id_de_criterio(
    conn: &rusqlite::Connection,
    entrega_id: i64,
    codigo: &str,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT cr.id FROM criterios_rubrica cr
         JOIN enunciados e ON e.rubrica_id = cr.rubrica_id
         JOIN entregas en ON en.enunciado_id = e.id
         WHERE en.id = ?1 AND cr.codigo = ?2",
        rusqlite::params![entrega_id, codigo],
        |r| r.get(0),
    )
    .map_err(|e| format!("criterio '{codigo}' no encontrado para la entrega {entrega_id}: {e}"))
}

/// Llamada A (Hito 5): localizar evidencia por criterio, sin veredicto ni
/// puntuación. Requiere que la entrega ya esté anonimizada y confirmada
/// (`pipeline::anonimizacion::cargar_confirmado` lo garantiza).
#[tauri::command]
pub async fn cmd_invocar_evidencias(
    db: State<'_, DbState>,
    entrega_id: i64,
) -> Result<Vec<EvidenciaCriterio>, String> {
    let (texto_enunciado, criterios, confirmado) = {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("la base de datos está cerrada")?;
        let (texto_enunciado, criterios) = enunciado_y_criterios(conn, entrega_id)?;
        let confirmado = anonimizacion::cargar_confirmado(conn, entrega_id).map_err(|e| e.to_string())?;
        (texto_enunciado, criterios, confirmado)
    };

    let cliente = InferenceClient::new(OLLAMA_URL_POR_DEFECTO).map_err(|e| e.to_string())?;
    let evidencias = prompt_builder::invocar_evidencias(
        &cliente,
        MODELO_POR_DEFECTO,
        &texto_enunciado,
        &criterios,
        &confirmado,
    )
    .await
    .map_err(|e| e.to_string())?;

    {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("la base de datos está cerrada")?;
        for ev in &evidencias {
            let criterio_id = id_de_criterio(conn, entrega_id, &ev.criterio_id)?;
            conn.execute(
                "INSERT INTO resultados (entrega_id, criterio_id, evidencia_ia_json) VALUES (?1, ?2, ?3)
                 ON CONFLICT(entrega_id, criterio_id) DO UPDATE SET evidencia_ia_json = excluded.evidencia_ia_json",
                rusqlite::params![
                    entrega_id,
                    criterio_id,
                    serde_json::to_string(&ev.evidencia_textual).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        conn.execute(
            "INSERT INTO logs_auditoria (entrega_id, evento, actor, version_modelo, payload_json)
             VALUES (?1, 'llamada_a_evidencias', 'ia', ?2, ?3)",
            rusqlite::params![
                entrega_id,
                MODELO_POR_DEFECTO,
                serde_json::to_string(&evidencias).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    db.sellar().map_err(|e| e.to_string())?;

    Ok(evidencias)
}

#[derive(Debug, Deserialize)]
pub struct EvaluacionCriterio {
    pub criterio_id: String,
    pub puntuacion: f64,
    pub comentario_docente: Option<String>,
}

/// El profesor guarda su propia evaluación por criterio (nota tentativa,
/// aún no oficial — la confirmación final es el Hito 6).
#[tauri::command]
pub fn cmd_guardar_nota_tentativa(
    db: State<DbState>,
    entrega_id: i64,
    evaluaciones: Vec<EvaluacionCriterio>,
) -> Result<(), String> {
    {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("la base de datos está cerrada")?;
        for ev in evaluaciones {
            let criterio_id = id_de_criterio(conn, entrega_id, &ev.criterio_id)?;
            conn.execute(
                "INSERT INTO resultados (entrega_id, criterio_id, puntuacion_final, comentario_docente)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(entrega_id, criterio_id) DO UPDATE SET
                    puntuacion_final = excluded.puntuacion_final,
                    comentario_docente = excluded.comentario_docente",
                rusqlite::params![entrega_id, criterio_id, ev.puntuacion, ev.comentario_docente],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    db.sellar().map_err(|e| e.to_string())?;
    Ok(())
}

/// Llamada B (Hito 5): feedback + detección de inconsistencias entre la
/// evaluación ya guardada por el profesor y la evidencia disponible.
#[tauri::command]
pub async fn cmd_invocar_feedback(
    db: State<'_, DbState>,
    entrega_id: i64,
) -> Result<FeedbackYConsistencia, String> {
    let (texto_enunciado, criterios, confirmado, evaluacion_docente) = {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("la base de datos está cerrada")?;
        let (texto_enunciado, criterios) = enunciado_y_criterios(conn, entrega_id)?;
        let confirmado = anonimizacion::cargar_confirmado(conn, entrega_id).map_err(|e| e.to_string())?;

        let mut stmt = conn
            .prepare(
                "SELECT cr.codigo, r.puntuacion_final FROM resultados r
                 JOIN criterios_rubrica cr ON cr.id = r.criterio_id
                 WHERE r.entrega_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let evaluacion_docente: Vec<(String, Option<f64>)> = stmt
            .query_map([entrega_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        (texto_enunciado, criterios, confirmado, evaluacion_docente)
    };

    let cliente = InferenceClient::new(OLLAMA_URL_POR_DEFECTO).map_err(|e| e.to_string())?;
    let feedback = prompt_builder::invocar_feedback(
        &cliente,
        MODELO_POR_DEFECTO,
        &texto_enunciado,
        &criterios,
        &confirmado,
        &evaluacion_docente,
    )
    .await
    .map_err(|e| e.to_string())?;

    {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("la base de datos está cerrada")?;
        conn.execute(
            "INSERT INTO logs_auditoria (entrega_id, evento, actor, version_modelo, payload_json)
             VALUES (?1, 'llamada_b_feedback', 'ia', ?2, ?3)",
            rusqlite::params![
                entrega_id,
                MODELO_POR_DEFECTO,
                serde_json::to_string(&feedback).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    db.sellar().map_err(|e| e.to_string())?;

    Ok(feedback)
}
