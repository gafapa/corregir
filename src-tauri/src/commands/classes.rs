//! Class rosters. Names stay in the encrypted workspace and are only used locally for
//! identifier detection, choosing the student during redaction and named exports.
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

use crate::db::DbState;

const MAX_STUDENTS: usize = 300;

#[derive(Debug, Serialize, PartialEq)]
pub struct Student {
    pub id: i64,
    pub full_name: String,
}

#[derive(Debug, Serialize)]
pub struct ClassRoster {
    pub id: i64,
    pub name: String,
    pub students: Vec<Student>,
}

#[tauri::command]
pub async fn cmd_list_classes(
    db: State<'_, std::sync::Arc<DbState>>,
) -> Result<Vec<ClassRoster>, String> {
    crate::tasks::run(db.inner().clone(), |db| {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        list_classes(guard.as_ref().ok_or("the database is closed")?)
    })
    .await
}

pub(crate) fn list_classes(conn: &Connection) -> Result<Vec<ClassRoster>, String> {
    let mut classes = conn
        .prepare("SELECT id, name FROM classes ORDER BY name COLLATE NOCASE, id")
        .map_err(|e| e.to_string())?
        .query_map([], |row| {
            Ok(ClassRoster {
                id: row.get(0)?,
                name: row.get(1)?,
                students: Vec::new(),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut students = conn
        .prepare("SELECT id, full_name FROM students WHERE class_id=?1 ORDER BY full_name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    for class in &mut classes {
        class.students = students
            .query_map([class.id], |row| {
                Ok(Student {
                    id: row.get(0)?,
                    full_name: row.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
    }
    Ok(classes)
}

/// Trim names, drop blanks and case-insensitive duplicates, and enforce limits.
fn clean_roster(students: &[String]) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut cleaned = Vec::new();
    for name in students {
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() {
            continue;
        }
        if name.len() > 200 {
            return Err("student names are limited to 200 bytes".into());
        }
        if seen.insert(name.to_lowercase()) {
            cleaned.push(name);
        }
    }
    if cleaned.len() > MAX_STUDENTS {
        return Err(format!(
            "a class can contain at most {MAX_STUDENTS} students"
        ));
    }
    Ok(cleaned)
}

/// Create a class or replace its name and roster. Students kept in the new roster keep
/// their identifiers, so submissions already linked to them stay linked.
#[tauri::command]
pub async fn cmd_save_class(
    db: State<'_, std::sync::Arc<DbState>>,
    class_id: Option<i64>,
    name: String,
    students: Vec<String>,
) -> Result<i64, String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        db.write(|conn| save_class(conn, class_id, &name, &students))
    })
    .await
}

fn save_class(
    conn: &mut Connection,
    class_id: Option<i64>,
    name: &str,
    students: &[String],
) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err("enter a class name of between 1 and 100 bytes".into());
    }
    let roster = clean_roster(students)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let id = match class_id {
        Some(id) => {
            let changed = tx
                .execute("UPDATE classes SET name=?1 WHERE id=?2", params![name, id])
                .map_err(|e| e.to_string())?;
            if changed != 1 {
                return Err("the class no longer exists".into());
            }
            id
        }
        None => {
            tx.execute("INSERT INTO classes (name) VALUES (?1)", [name])
                .map_err(|e| e.to_string())?;
            tx.last_insert_rowid()
        }
    };
    let existing: Vec<(i64, String)> = tx
        .prepare("SELECT id, full_name FROM students WHERE class_id=?1")
        .map_err(|e| e.to_string())?
        .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let wanted: std::collections::HashMap<String, &String> = roster
        .iter()
        .map(|name| (name.to_lowercase(), name))
        .collect();
    let mut removed = 0;
    for (student_id, full_name) in &existing {
        match wanted.get(&full_name.to_lowercase()) {
            None => {
                tx.execute("DELETE FROM students WHERE id=?1", [student_id])
                    .map_err(|e| e.to_string())?;
                removed += 1;
            }
            // Keep the row (and its links) but apply a corrected capitalization.
            Some(&corrected) if corrected != full_name => {
                tx.execute(
                    "UPDATE students SET full_name=?1 WHERE id=?2",
                    params![corrected, student_id],
                )
                .map_err(|e| e.to_string())?;
            }
            Some(_) => {}
        }
    }
    let kept: std::collections::HashSet<String> = existing
        .iter()
        .map(|(_, full_name)| full_name.to_lowercase())
        .collect();
    let mut added = 0;
    for full_name in &roster {
        if !kept.contains(&full_name.to_lowercase()) {
            tx.execute(
                "INSERT INTO students (class_id, full_name) VALUES (?1, ?2)",
                params![id, full_name],
            )
            .map_err(|e| e.to_string())?;
            added += 1;
        }
    }
    // Counts only: the audit trail never contains student names.
    tx.execute(
        "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('class_saved', 'teacher', ?1)",
        [serde_json::json!({"class_id": id, "students": roster.len(), "added": added, "removed": removed}).to_string()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(id)
}

/// Delete a class and its roster. Assignments and graded submissions remain.
#[tauri::command]
pub async fn cmd_delete_class(
    db: State<'_, std::sync::Arc<DbState>>,
    class_id: i64,
) -> Result<(), String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        db.write(|conn| {
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let changed = tx
                .execute("DELETE FROM classes WHERE id=?1", [class_id])
                .map_err(|e| e.to_string())?;
            if changed != 1 {
                return Err("the class no longer exists".into());
            }
            tx.execute(
                "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('class_deleted', 'teacher', ?1)",
                [serde_json::json!({"class_id": class_id}).to_string()],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_a_roster_keeps_existing_links_and_never_audits_names() {
        let directory = tempfile::tempdir().unwrap();
        let db = crate::db::schema::open_with_key(directory.path(), [11; 32]).unwrap();
        let mut guard = db.conn.lock().unwrap();
        let conn = guard.as_mut().unwrap();
        let roster = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        let id = save_class(
            conn,
            None,
            "3º ESO B",
            &roster(&["Ana  Pérez", "Brais Souto", "ana pérez", " "]),
        )
        .unwrap();
        let first = list_classes(conn).unwrap();
        assert_eq!(first[0].students.len(), 2);
        let brais = first[0]
            .students
            .iter()
            .find(|student| student.full_name == "Brais Souto")
            .unwrap()
            .id;
        save_class(
            conn,
            Some(id),
            "3º ESO B",
            &roster(&["Brais Souto", "Uxía Rey"]),
        )
        .unwrap();
        let second = list_classes(conn).unwrap();
        let names: Vec<&str> = second[0]
            .students
            .iter()
            .map(|student| student.full_name.as_str())
            .collect();
        assert_eq!(names, ["Brais Souto", "Uxía Rey"]);
        assert!(second[0].students.iter().any(|student| student.id == brais));
        // A capitalization fix keeps the student's identifier.
        save_class(
            conn,
            Some(id),
            "3º ESO B",
            &roster(&["BRAIS SOUTO", "Uxía Rey"]),
        )
        .unwrap();
        let third = list_classes(conn).unwrap();
        let brais_row = third[0]
            .students
            .iter()
            .find(|student| student.id == brais)
            .unwrap();
        assert_eq!(brais_row.full_name, "BRAIS SOUTO");
        let payloads: String = conn
            .query_row(
                "SELECT group_concat(payload_json) FROM logs_audit",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!payloads.contains("Brais") && !payloads.contains("Ana"));
        assert!(save_class(conn, Some(9999), "Missing", &[]).is_err());
        assert!(save_class(conn, None, " ", &[]).is_err());
    }
}
