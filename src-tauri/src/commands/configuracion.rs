use tauri::State;

use crate::db::DbState;
use crate::modelos::{CriterioConId, NuevaRubrica, RubricaConCriterios};

#[tauri::command]
pub fn cmd_crear_enunciado(
    db: State<DbState>,
    rubrica_id: i64,
    texto: String,
    materiales_ref: Option<String>,
) -> Result<i64, String> {
    let id = {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("la base de datos está cerrada")?;
        conn.execute(
            "INSERT INTO enunciados (rubrica_id, texto, materiales_ref) VALUES (?1, ?2, ?3)",
            rusqlite::params![rubrica_id, texto, materiales_ref],
        )
        .map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    };
    db.sellar().map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub fn cmd_crear_rubrica(db: State<DbState>, rubrica: NuevaRubrica) -> Result<i64, String> {
    let rubrica_id = {
        let mut guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_mut().ok_or("la base de datos está cerrada")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        let contenido_json = serde_json::to_string(&rubrica).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO rubricas (titulo, asignatura, curso, version, contenido_json)
             VALUES (?1, ?2, ?3, 1, ?4)",
            rusqlite::params![rubrica.titulo, rubrica.asignatura, rubrica.curso, contenido_json],
        )
        .map_err(|e| e.to_string())?;
        let rubrica_id = tx.last_insert_rowid();

        for (indice, criterio) in rubrica.criterios.iter().enumerate() {
            tx.execute(
                "INSERT INTO criterios_rubrica (rubrica_id, codigo, descripcion, puntuacion_max, orden)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    rubrica_id,
                    criterio.codigo,
                    criterio.descripcion,
                    criterio.puntuacion_max,
                    indice as i64
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;
        rubrica_id
    };

    // Vuelve a cifrar el fichero en disco tras la escritura (ver db/schema.rs).
    db.sellar().map_err(|e| e.to_string())?;
    Ok(rubrica_id)
}

#[tauri::command]
pub fn cmd_listar_rubricas(db: State<DbState>) -> Result<Vec<RubricaConCriterios>, String> {
    let guard = db.conn.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("la base de datos está cerrada")?;

    let mut stmt_rubricas = conn
        .prepare("SELECT id, titulo, asignatura, curso, version FROM rubricas ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rubricas_base = stmt_rubricas
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut stmt_criterios = conn
        .prepare(
            "SELECT id, codigo, descripcion, puntuacion_max, orden
             FROM criterios_rubrica WHERE rubrica_id = ?1 ORDER BY orden",
        )
        .map_err(|e| e.to_string())?;

    let mut resultado = Vec::with_capacity(rubricas_base.len());
    for (id, titulo, asignatura, curso, version) in rubricas_base {
        let criterios = stmt_criterios
            .query_map([id], |row| {
                Ok(CriterioConId {
                    id: row.get(0)?,
                    codigo: row.get(1)?,
                    descripcion: row.get(2)?,
                    puntuacion_max: row.get(3)?,
                    orden: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        resultado.push(RubricaConCriterios {
            id,
            titulo,
            asignatura,
            curso,
            version,
            criterios,
        });
    }

    Ok(resultado)
}
