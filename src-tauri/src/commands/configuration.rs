use tauri::State;

use crate::db::DbState;
use crate::models::{CriterionWithId, NewRubric, RubricWithCriteria};

#[cfg(test)]
#[path = "configuration_tests.rs"]
mod tests;

#[tauri::command]
pub async fn cmd_create_assignment(
    db: State<'_, std::sync::Arc<DbState>>,
    rubric_id: i64,
    text: String,
    material_references: Option<String>,
) -> Result<i64, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        if text.trim().is_empty() || text.len() > 64 * 1024 {
            return Err("assignment text must contain between 1 and 65536 bytes".into());
        }
        let id = {
            let guard = db.conn.lock().map_err(|e| e.to_string())?;
            let conn = guard.as_ref().ok_or("the database is closed")?;
            conn.execute(
                "INSERT INTO assignments (rubric_id, text, materials_ref) VALUES (?1, ?2, ?3)",
                rusqlite::params![rubric_id, text, material_references],
            )
            .map_err(|e| e.to_string())?;
            let id = conn.last_insert_rowid();
            db.persist(conn).map_err(|e| e.to_string())?;
            id
        };
        Ok(id)
    })
    .await
}

#[tauri::command]
pub async fn cmd_create_rubric(
    db: State<'_, std::sync::Arc<DbState>>,
    rubric: NewRubric,
) -> Result<i64, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        validate_rubric(&rubric)?;
        let rubric_id = {
            let mut guard = db.conn.lock().map_err(|e| e.to_string())?;
            let conn = guard.as_mut().ok_or("the database is closed")?;
            let tx = conn.transaction().map_err(|e| e.to_string())?;

            let rubric_id = insert_rubric(&tx, &rubric, 1)?;
            tx.commit().map_err(|e| e.to_string())?;
            db.persist(conn).map_err(|e| e.to_string())?;
            rubric_id
        };
        Ok(rubric_id)
    })
    .await
}

fn insert_rubric(
    conn: &rusqlite::Connection,
    rubric: &NewRubric,
    version: i64,
) -> Result<i64, String> {
    let content_json = serde_json::to_string(&rubric).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO rubrics (title, subject, grade_level, version, content_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            rubric.title,
            rubric.subject,
            rubric.grade_level,
            version,
            content_json
        ],
    )
    .map_err(|e| e.to_string())?;
    let rubric_id = conn.last_insert_rowid();

    for (index, criterion) in rubric.criteria.iter().enumerate() {
        conn.execute(
            "INSERT INTO criteria_rubric (rubric_id, code, description, score_max, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                rubric_id,
                criterion.code,
                criterion.description,
                criterion.score_max,
                index as i64
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(rubric_id)
}

#[tauri::command]
pub async fn cmd_create_rubric_version(
    db: State<'_, std::sync::Arc<DbState>>,
    rubric_id: i64,
    expected_version: i64,
    rubric: NewRubric,
) -> Result<i64, String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        validate_rubric(&rubric)?;
        let mut guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_mut().ok_or("the database is closed")?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let id = create_rubric_version(&tx, rubric_id, expected_version, &rubric)?;
        tx.commit().map_err(|e| e.to_string())?;
        db.persist(conn).map_err(|e| e.to_string())?;
        Ok(id)
    })
    .await
}

fn create_rubric_version(
    conn: &rusqlite::Connection,
    rubric_id: i64,
    expected_version: i64,
    rubric: &NewRubric,
) -> Result<i64, String> {
    validate_rubric(rubric)?;
    let version: i64 = conn
        .query_row(
            "SELECT version FROM rubrics WHERE id=?1",
            [rubric_id],
            |row| row.get(0),
        )
        .map_err(|_| "the source rubric no longer exists")?;
    if version != expected_version {
        return Err("the source rubric version changed; reload it before saving".into());
    }
    insert_rubric(
        conn,
        rubric,
        version
            .checked_add(1)
            .ok_or("rubric version limit reached")?,
    )
}

fn validate_rubric(rubric: &NewRubric) -> Result<(), String> {
    if rubric.title.trim().is_empty()
        || rubric.subject.trim().is_empty()
        || rubric.grade_level.trim().is_empty()
        || rubric.criteria.is_empty()
        || rubric.criteria.len() > 100
        || rubric.title.len() > 200
        || rubric.subject.len() > 200
        || rubric.grade_level.len() > 100
    {
        return Err(
            "a rubric needs a title, subject, grade level, and at least one criterion".into(),
        );
    }
    let mut codes = std::collections::HashSet::new();
    for criterion in &rubric.criteria {
        if criterion.code.trim().is_empty()
            || criterion.code.len() > 32
            || !criterion
                .code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            || criterion.description.trim().is_empty()
            || criterion.description.len() > 4000
            || !codes.insert(criterion.code.trim())
            || !criterion.score_max.is_finite()
            || criterion.score_max <= 0.0
            || criterion.score_max > 10000.0
        {
            return Err("criteria need unique nonempty codes, descriptions, and positive finite maximum scores".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn cmd_list_rubrics(
    db: State<'_, std::sync::Arc<DbState>>,
) -> Result<Vec<RubricWithCriteria>, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("the database is closed")?;

        let mut stmt_rubrics = conn
            .prepare(
                "SELECT id, title, subject, grade_level, version FROM rubrics ORDER BY id DESC",
            )
            .map_err(|e| e.to_string())?;
        let rubrics_base = stmt_rubrics
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

        let mut stmt_criteria = conn
            .prepare(
                "SELECT id, code, description, score_max, sort_order
             FROM criteria_rubric WHERE rubric_id = ?1 ORDER BY sort_order",
            )
            .map_err(|e| e.to_string())?;

        let mut result = Vec::with_capacity(rubrics_base.len());
        for (id, title, subject, grade_level, version) in rubrics_base {
            let criteria = stmt_criteria
                .query_map([id], |row| {
                    Ok(CriterionWithId {
                        id: row.get(0)?,
                        code: row.get(1)?,
                        description: row.get(2)?,
                        score_max: row.get(3)?,
                        sort_order: row.get(4)?,
                    })
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            result.push(RubricWithCriteria {
                id,
                title,
                subject,
                grade_level,
                version,
                criteria,
            });
        }

        Ok(result)
    })
    .await
}

#[derive(serde::Serialize)]
pub struct AssignmentSummary {
    id: i64,
    rubric_id: i64,
    text: String,
    submission_count: i64,
}

#[tauri::command]
pub async fn cmd_list_assignments(
    db: State<'_, std::sync::Arc<DbState>>,
) -> Result<Vec<AssignmentSummary>, String> {
    crate::tasks::run(db.inner().clone(),|db| {
        let guard=db.conn.lock().map_err(|e|e.to_string())?;
        let conn=guard.as_ref().ok_or("the database is closed")?;
        let mut stmt=conn.prepare("SELECT a.id,a.rubric_id,a.text,COUNT(s.id) FROM assignments a LEFT JOIN submissions s ON s.assignment_id=a.id GROUP BY a.id ORDER BY a.id DESC").map_err(|e|e.to_string())?;
        let result=stmt.query_map([],|r|Ok(AssignmentSummary {id:r.get(0)?,rubric_id:r.get(1)?,text:r.get(2)?,submission_count:r.get(3)?})).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        Ok(result)
    }).await
}
