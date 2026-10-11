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
        tx.execute("INSERT INTO submissions (assignment_id,text_ocr,method_ocr,status_pipeline) VALUES (?1,?2,?3,'ocr_complete')",rusqlite::params![assignment_id,texts.join("\n\n"),method]).map_err(|e|e.to_string())?;
        let id=tx.last_insert_rowid();
        tx.execute("INSERT INTO configuration (key,value) VALUES (?1,?2)",rusqlite::params![format!("document.source.{id}"),source_hash]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO logs_audit (submission_id,event,actor,payload_json) VALUES (?1,'ocr_complete','system',?2)",rusqlite::params![id,serde_json::json!({"method_ocr":method,"pages":texts.len()}).to_string()]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())?;
        Ok(id)
        })?;
        let _=on_progress.send(ImportProgress {request_id:job.id.clone(),page:texts.len() as u16,total:texts.len() as u16,stage:"saved".into()});
        Ok(id)
    }).await
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
            "SELECT e.id, e.assignment_id, e.text_ocr, e.method_ocr, e.status_pipeline, m.student_name
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
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)

    }).await
}
