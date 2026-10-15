use std::path::Path;

use tauri::{AppHandle, Manager, State};

use crate::db::DbState;
use crate::models::AuditLog;
use crate::pipeline::export::{self, ReportLanguage};
use crate::resources;

#[tauri::command]
pub async fn cmd_export_csv(
    app: AppHandle,
    db: State<'_, std::sync::Arc<DbState>>,
    assignment_id: i64,
    destination_path: String,
    language: Option<ReportLanguage>,
) -> Result<usize, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        let destination = validate_destination(&app, db, &destination_path, "csv")?;
        db.write(|conn| {
            let rows = export::generate_csv(
                conn,
                assignment_id,
                &destination,
                language.unwrap_or_default(),
            )
            .map_err(|e| e.to_string())?;
            Ok(rows)
        })
    })
    .await
}

#[tauri::command]
pub async fn cmd_export_logs_csv(
    app: AppHandle,
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: Option<i64>,
    destination_path: String,
) -> Result<usize, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        let destination = validate_destination(&app, db, &destination_path, "csv")?;
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("the database is closed")?;
        export::generate_csv_logs(conn, submission_id, &destination).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn cmd_export_pdf_feedback(
    app: AppHandle,
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: i64,
    destination_path: String,
    language: Option<ReportLanguage>,
) -> Result<(), String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        let destination = validate_destination(&app, db, &destination_path, "pdf")?;
        let path_source = resources::path_source_pdf(&app);
        db.write(|conn| {
            export::generate_pdf_feedback(
                conn,
                submission_id,
                &path_source,
                &destination,
                language.unwrap_or_default(),
            )
            .map_err(|e| e.to_string())
        })
    })
    .await
}

/// Write every confirmed feedback sheet of an assignment into a chosen folder.
#[tauri::command]
pub async fn cmd_export_pdf_batch(
    app: AppHandle,
    db: State<'_, std::sync::Arc<DbState>>,
    assignment_id: i64,
    directory_path: String,
    language: Option<ReportLanguage>,
) -> Result<usize, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        // Validate a representative file inside the folder against the protected roots.
        let probe = Path::new(&directory_path).join("corregir-feedback.pdf");
        let directory = validate_destination(&app, db, &probe.to_string_lossy(), "pdf")?
            .parent()
            .ok_or("missing export directory")?
            .to_path_buf();
        let path_source = resources::path_source_pdf(&app);
        // A partial failure is reported only after the audit row for the reports
        // already written has been saved.
        let batch = db.write(|conn| {
            export::generate_pdf_batch(
                conn,
                assignment_id,
                &path_source,
                &directory,
                language.unwrap_or_default(),
            )
            .map_err(|e| e.to_string())
        })?;
        match batch.failure {
            None => Ok(batch.written),
            Some(error) if batch.written > 0 => Err(format!(
                "{} reports were written before the export stopped: {error}",
                batch.written
            )),
            Some(error) => Err(error.to_string()),
        }
    })
    .await
}

#[tauri::command]
pub async fn cmd_list_logs_audit(
    db: State<'_, std::sync::Arc<DbState>>,
    submission_id: Option<i64>,
) -> Result<Vec<AuditLog>, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
        let guard = db.conn.lock().map_err(|e| e.to_string())?;
        let conn = guard.as_ref().ok_or("the database is closed")?;
        export::list_logs(conn, submission_id).map_err(|e| e.to_string())
    })
    .await
}

pub(super) fn validate_destination(
    app: &AppHandle,
    db: &DbState,
    path: &str,
    extension: &str,
) -> Result<std::path::PathBuf, String> {
    let resources = app.path().resource_dir().map_err(|e| e.to_string())?;
    let dev_resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
    validate_export_path(
        Path::new(path),
        extension,
        &[db.directory.clone(), resources, dev_resources],
    )
}

fn validate_export_path(
    path: &Path,
    extension: &str,
    protected: &[std::path::PathBuf],
) -> Result<std::path::PathBuf, String> {
    if !path.is_absolute()
        || !path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case(extension))
    {
        return Err(format!(
            "choose an absolute destination with the .{extension} extension"
        ));
    }
    let parent = path
        .parent()
        .ok_or("missing export directory")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let candidate = if path.exists() {
        path.canonicalize().map_err(|e| e.to_string())?
    } else {
        parent.join(path.file_name().ok_or("missing export filename")?)
    };
    for root in protected {
        if let Ok(root) = root.canonicalize() {
            if candidate.starts_with(&root) || parent.starts_with(&root) {
                return Err(
                    "exports cannot overwrite application data or bundled resources".into(),
                );
            }
        }
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_internal_and_relative_export_paths() {
        let directory = tempfile::tempdir().unwrap();
        let protected = vec![directory.path().to_path_buf()];
        assert!(
            validate_export_path(&directory.path().join("data.csv"), "csv", &protected).is_err()
        );
        assert!(validate_export_path(Path::new("data.csv"), "csv", &[]).is_err());
        let external = tempfile::tempdir().unwrap();
        assert!(
            validate_export_path(&external.path().join("grades.csv"), "csv", &protected).is_ok()
        );
        assert!(validate_export_path(&external.path().join("db.enc"), "csv", &protected).is_err());
    }
}
