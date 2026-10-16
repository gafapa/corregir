use crate::db::DbState;
use crate::models::SubmissionSummary;
use crate::pipeline::render;
use crate::resources;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

#[derive(Default)]
pub struct ImportJobs {
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
}
struct ImportJob {
    id: String,
    cancelled: Arc<AtomicBool>,
    jobs: Arc<ImportJobs>,
}
impl Drop for ImportJob {
    fn drop(&mut self) {
        if let Ok(mut active) = self.jobs.active.lock() {
            active.remove(&self.id);
        }
    }
}
impl ImportJob {
    fn check(&self) -> Result<(), render::RenderError> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err(render::RenderError::Cancelled)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Serialize)]
pub struct ImportProgress {
    request_id: String,
    page: u16,
    total: u16,
    stage: String,
}
static IMPORT_WORKER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[tauri::command]
pub fn cmd_cancel_import(
    jobs: State<'_, Arc<ImportJobs>>,
    request_id: String,
) -> Result<(), String> {
    if let Some(flag) = jobs
        .active
        .lock()
        .map_err(|e| e.to_string())?
        .get(&request_id)
    {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn cmd_import_submission(
    app: AppHandle,
    db: State<'_, Arc<DbState>>,
    jobs: State<'_, Arc<ImportJobs>>,
    assignment_id: i64,
    file_path: String,
    request_id: String,
    on_progress: Channel<ImportProgress>,
    force_ocr: Option<bool>,
) -> Result<i64, String> {
    if request_id.len() > 128 || request_id.is_empty() {
        return Err("invalid import request ID".into());
    }
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut active = jobs.active.lock().map_err(|e| e.to_string())?;
        if active.len() >= 2 || active.contains_key(&request_id) {
            return Err("an import is already queued; wait or cancel it".into());
        }
        active.insert(request_id.clone(), flag.clone());
    }
    let job = ImportJob {
        id: request_id,
        cancelled: flag,
        jobs: jobs.inner().clone(),
    };
    let _ = on_progress.send(ImportProgress {
        request_id: job.id.clone(),
        page: 0,
        total: 0,
        stage: "queued".into(),
    });
    let _permit = IMPORT_WORKER.acquire().await.map_err(|e| e.to_string())?;
    let db = db.inner().clone();
    crate::tasks::run(db,move |db| {
        job.check().map_err(|e|e.to_string())?;
        let path = Path::new(&file_path).canonicalize().map_err(|error| error.to_string())?;
        let source_name = source_name(&path);
        let source_hash = super::document_review::fingerprint(&path)?;
        let result = crate::document_worker::extract(crate::document_worker::WorkerRequest {
            file_path: path.clone(),
            pdfium_directory: resources::directory_pdfium(&app),
            models_directory: resources::directory_models_ocr(&app),
            force_ocr: force_ocr.unwrap_or(false),
            preview_page: None,
            source_handle: None,
            #[cfg(debug_assertions)]
            sandbox_probe: None,
        }, &job.cancelled, |page, total| {
            let _ = on_progress.send(ImportProgress { request_id: job.id.clone(), page, total, stage: "extracting".into() });
        })?;
        if source_hash != super::document_review::fingerprint(&path)? { return Err("the source document changed during import; select it again".into()); }
        if result.source_hash != source_hash { return Err("the extracted document does not match the source fingerprint".into()); }
        let texts = result.texts;
        let used_ocr = result.used_ocr;
        job.check().map_err(|e|e.to_string())?;
        let id = db.write(|conn| {
        job.check().map_err(|e|e.to_string())?;
        let tx=conn.transaction().map_err(|e|e.to_string())?;
        let method=if used_ocr {"ocr_local"} else {"text_native"};
        tx.execute("INSERT INTO submissions (assignment_id,text_ocr,method_ocr,status_pipeline,source_name,source_sha256) VALUES (?1,?2,?3,'ocr_complete',?4,?5)",rusqlite::params![assignment_id,texts.join("\n\n"),method,source_name,source_hash]).map_err(|e|e.to_string())?;
        let id=tx.last_insert_rowid();
        tx.execute("INSERT INTO logs_audit (submission_id,event,actor,payload_json) VALUES (?1,'ocr_complete','system',?2)",rusqlite::params![id,serde_json::json!({"method_ocr":method,"pages":texts.len()}).to_string()]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())?;
        Ok(id)
        })?;
        let _=on_progress.send(ImportProgress {request_id:job.id.clone(),page:texts.len() as u16,total:texts.len() as u16,stage:"saved".into()});
        Ok(id)
    }).await
}

/// File name only; the folder path is not kept. Names often identify the student, so
/// this stays in the encrypted workspace and is never sent to the model.
fn source_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    Some(name.chars().take(255).collect())
}

/// Delete one submission with its identity link, scores and feedback.
#[tauri::command]
pub async fn cmd_delete_submission(
    db: State<'_, Arc<DbState>>,
    submission_id: i64,
) -> Result<(), String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        db.write(|conn| delete_submission(conn, submission_id))
    })
    .await
}

pub(crate) fn delete_submission(
    conn: &mut rusqlite::Connection,
    submission_id: i64,
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let assignment_id: i64 = tx
        .query_row(
            "SELECT assignment_id FROM submissions WHERE id=?1",
            [submission_id],
            |row| row.get(0),
        )
        .map_err(|_| "the submission no longer exists")?;
    tx.execute("DELETE FROM submissions WHERE id=?1", [submission_id])
        .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('submission_deleted', 'teacher', ?1)",
        [serde_json::json!({"submission_id": submission_id, "assignment_id": assignment_id}).to_string()],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_list_submissions(
    db: State<'_, std::sync::Arc<DbState>>,
    assignment_id: i64,
) -> Result<Vec<SubmissionSummary>, String> {
    let db = db.inner().clone();
    crate::tasks::run(db, move |db| {
    let guard = db.conn.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("the database is closed")?;
    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.assignment_id, COALESCE(e.text_ocr, e.text_redacted), e.method_ocr,
                    e.status_pipeline, m.student_name, m.student_id, e.source_name,
                    e.identities_purged_at IS NOT NULL,
                    EXISTS(SELECT 1 FROM results r WHERE r.submission_id = e.id AND r.ai_evidence_json IS NOT NULL)
             FROM submissions e
             LEFT JOIN alias_student_map m ON m.alias = e.alias
             WHERE e.assignment_id = ?1 ORDER BY e.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([assignment_id], |row| {
            Ok(SubmissionSummary {
                id: row.get(0)?,
                assignment_id: row.get(1)?,
                text_ocr: row.get(2)?,
                method_ocr: row.get(3)?,
                status_pipeline: row.get(4)?,
                student_name: row.get(5)?,
                student_id: row.get(6)?,
                source_name: row.get(7)?,
                identities_purged: row.get(8)?,
                has_evidence: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)

    }).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleting_a_submission_removes_its_identity_and_keeps_an_audit_record() {
        let directory = tempfile::tempdir().unwrap();
        let db = crate::db::schema::open_with_key(directory.path(), [12; 32]).unwrap();
        let mut guard = db.conn.lock().unwrap();
        let conn = guard.as_mut().unwrap();
        conn.execute_batch("INSERT INTO rubrics (id,title,subject,grade_level,content_json) VALUES (1,'t','s','g','{}'); INSERT INTO assignments (id,rubric_id,text) VALUES (1,1,'task'); INSERT INTO submissions (id,assignment_id,text_ocr,status_pipeline) VALUES (5,1,'Ana answers','ocr_complete');").unwrap();
        crate::pipeline::redaction::confirm(conn, 5, &[], "Ana", None).unwrap();
        delete_submission(conn, 5).unwrap();
        let remaining: i64 = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM submissions)+(SELECT COUNT(*) FROM alias_student_map)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM logs_audit WHERE event='submission_deleted'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(payload.contains("\"submission_id\":5"));
        assert!(delete_submission(conn, 5).is_err());
    }
}
