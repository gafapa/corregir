use tauri::State;

use crate::db::DbState;
use crate::models::{CriterionWithId, NewRubric, RubricWithCriteria};

#[cfg(test)]
#[path = "configuration_tests.rs"]
mod tests;

fn normalized_title(title: Option<String>) -> Result<Option<String>, String> {
    let title = title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if title.as_ref().is_some_and(|value| value.len() > 200) {
        return Err("assignment titles are limited to 200 bytes".into());
    }
    Ok(title)
}

fn require_class(conn: &rusqlite::Connection, class_id: Option<i64>) -> Result<(), String> {
    if let Some(id) = class_id {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM classes WHERE id=?1)",
                [id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err("the selected class no longer exists".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn cmd_create_assignment(
    db: State<'_, std::sync::Arc<DbState>>,
    rubric_id: i64,
    text: String,
    material_references: Option<String>,
    title: Option<String>,
    class_id: Option<i64>,
) -> Result<i64, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        if text.trim().is_empty() || text.len() > 64 * 1024 {
            return Err("assignment text must contain between 1 and 65536 bytes".into());
        }
        let title = normalized_title(title)?;
        db.write(|conn| {
            require_class(conn, class_id)?;
            conn.execute(
                "INSERT INTO assignments (rubric_id, text, materials_ref, title, class_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![rubric_id, text, material_references, title, class_id],
            )
            .map_err(|e| e.to_string())?;
            let id = conn.last_insert_rowid();
            Ok(id)
        })
    })
    .await
}

/// Rename an assignment or link it to a class roster. Criteria and grades are unchanged.
#[tauri::command]
pub async fn cmd_update_assignment(
    db: State<'_, std::sync::Arc<DbState>>,
    assignment_id: i64,
    title: Option<String>,
    class_id: Option<i64>,
) -> Result<(), String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        let title = normalized_title(title)?;
        db.write(|conn| {
            require_class(conn, class_id)?;
            let changed = conn
                .execute(
                    "UPDATE assignments SET title=?1, class_id=?2 WHERE id=?3",
                    rusqlite::params![title, class_id, assignment_id],
                )
                .map_err(|e| e.to_string())?;
            if changed != 1 {
                return Err("the assignment no longer exists".into());
            }
            Ok(())
        })
    })
    .await
}

/// Delete an assignment with all of its submissions, identities and grades.
#[tauri::command]
pub async fn cmd_delete_assignment(
    db: State<'_, std::sync::Arc<DbState>>,
    assignment_id: i64,
) -> Result<(), String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        db.write(|conn| delete_assignment(conn, assignment_id))
    })
    .await
}

fn delete_assignment(conn: &mut rusqlite::Connection, assignment_id: i64) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let submissions: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM submissions WHERE assignment_id=?1",
            [assignment_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let changed = tx
        .execute("DELETE FROM assignments WHERE id=?1", [assignment_id])
        .map_err(|e| e.to_string())?;
    if changed != 1 {
        return Err("the assignment no longer exists".into());
    }
    tx.execute(
        "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('assignment_deleted', 'teacher', ?1)",
        [serde_json::json!({"assignment_id": assignment_id, "submissions": submissions}).to_string()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_create_rubric(
    db: State<'_, std::sync::Arc<DbState>>,
    rubric: NewRubric,
) -> Result<i64, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        validate_rubric(&rubric)?;
        db.write(|conn| {
            let tx = conn.transaction().map_err(|e| e.to_string())?;

            let rubric_id = insert_rubric(&tx, &rubric, 1)?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(rubric_id)
        })
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
        let levels = if criterion.levels.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&criterion.levels).map_err(|e| e.to_string())?)
        };
        conn.execute(
            "INSERT INTO criteria_rubric (rubric_id, code, description, score_max, sort_order, levels_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                rubric_id,
                criterion.code,
                criterion.description,
                criterion.score_max,
                index as i64,
                levels
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
        db.write(|conn| {
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let id = create_rubric_version(&tx, rubric_id, expected_version, &rubric)?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(id)
        })
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
        if criterion.levels.len() > 10
            || criterion.levels.iter().any(|level| {
                level.label.trim().is_empty()
                    || level.label.len() > 100
                    || level.description.len() > 1000
                    || !level.score.is_finite()
                    || !(0.0..=criterion.score_max).contains(&level.score)
            })
        {
            return Err("performance levels need a name and a score between 0 and the criterion maximum; use at most 10 levels".into());
        }
    }
    Ok(())
}

/// Delete a rubric version that no assignment uses.
#[tauri::command]
pub async fn cmd_delete_rubric(
    db: State<'_, std::sync::Arc<DbState>>,
    rubric_id: i64,
) -> Result<(), String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        db.write(|conn| delete_rubric(conn, rubric_id))
    })
    .await
}

fn delete_rubric(conn: &mut rusqlite::Connection, rubric_id: i64) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let used: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM assignments WHERE rubric_id=?1)",
            [rubric_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if used {
        return Err("this rubric is used by an assignment; delete the assignment first".into());
    }
    let changed = tx
        .execute("DELETE FROM rubrics WHERE id=?1", [rubric_id])
        .map_err(|e| e.to_string())?;
    if changed != 1 {
        return Err("the rubric no longer exists".into());
    }
    tx.execute(
        "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('rubric_deleted', 'teacher', ?1)",
        [serde_json::json!({"rubric_id": rubric_id}).to_string()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
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
                "SELECT id, code, description, score_max, sort_order, levels_json
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
                        levels: parse_levels(row.get(5)?),
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

pub(crate) fn parse_levels(json: Option<String>) -> Vec<crate::models::PerformanceLevel> {
    json.and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

#[derive(serde::Serialize)]
pub struct AssignmentSummary {
    pub id: i64,
    pub rubric_id: i64,
    pub title: Option<String>,
    pub text: String,
    pub class_id: Option<i64>,
    pub submission_count: i64,
    pub redaction_pending: i64,
    pub ready_to_grade: i64,
    pub confirmed: i64,
    /// Confirmed submissions whose identity has not been purged.
    pub identified: i64,
    pub last_confirmed_at: Option<String>,
}

#[tauri::command]
pub async fn cmd_list_assignments(
    db: State<'_, std::sync::Arc<DbState>>,
) -> Result<Vec<AssignmentSummary>, String> {
    crate::tasks::run(db.inner().clone(), |db| {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("the database is closed")?;
        list_assignments(conn)
    })
    .await
}

pub(crate) fn list_assignments(
    conn: &rusqlite::Connection,
) -> Result<Vec<AssignmentSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.rubric_id, a.title, a.text, a.class_id, COUNT(s.id),
                    COALESCE(SUM(s.status_pipeline='ocr_complete'),0),
                    COALESCE(SUM(s.status_pipeline='redacted'),0),
                    COALESCE(SUM(s.status_pipeline='grade_confirmed'),0),
                    COALESCE(SUM(s.status_pipeline='grade_confirmed' AND m.alias IS NOT NULL),0),
                    (SELECT MAX(r.confirmed_at) FROM results r JOIN submissions x ON x.id=r.submission_id WHERE x.assignment_id=a.id)
             FROM assignments a
             LEFT JOIN submissions s ON s.assignment_id=a.id
             LEFT JOIN alias_student_map m ON m.alias=s.alias
             GROUP BY a.id ORDER BY a.id DESC",
        )
        .map_err(|e| e.to_string())?;
    let result = stmt
        .query_map([], |r| {
            Ok(AssignmentSummary {
                id: r.get(0)?,
                rubric_id: r.get(1)?,
                title: r.get(2)?,
                text: r.get(3)?,
                class_id: r.get(4)?,
                submission_count: r.get(5)?,
                redaction_pending: r.get(6)?,
                ready_to_grade: r.get(7)?,
                confirmed: r.get(8)?,
                identified: r.get(9)?,
                last_confirmed_at: r.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(result)
}
