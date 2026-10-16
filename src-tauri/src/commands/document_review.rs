//! Local page comparison and auditable text correction before redaction.
use crate::{db::DbState, document_worker, pipeline::render, resources, tasks};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};
use tauri::{AppHandle, State};

pub fn fingerprint(path: &Path) -> Result<String, String> {
    render::validate_file(path).map_err(|error| error.to_string())?;
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > render::MAX_FILE_BYTES {
            return Err("document exceeds its file limit".into());
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()))
}

static PREVIEW_WORKER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[tauri::command]
pub async fn cmd_preview_document(
    app: AppHandle,
    db: State<'_, Arc<DbState>>,
    submission_id: i64,
    file_path: String,
    page: u16,
) -> Result<document_worker::PagePreview, String> {
    let _permit = PREVIEW_WORKER
        .try_acquire()
        .map_err(|_| "a document preview is already loading; try again when it finishes")?;
    tasks::run(db.inner().clone(), move |db| {
        let expected: String = {
            let guard = db.conn.lock().map_err(|error| error.to_string())?;
            let conn = guard.as_ref().ok_or("the database is closed")?;
            conn.query_row("SELECT source_sha256 FROM submissions WHERE id=?1", [submission_id], |row| row.get::<_, Option<String>>(0))
                .optional().map_err(|error| error.to_string())?.flatten().ok_or("this older submission has no source fingerprint; compare the original outside the app or import it as a new submission")?
        };
        let path = Path::new(&file_path).canonicalize().map_err(|error| error.to_string())?;
        if fingerprint(&path)? != expected { return Err("this file does not match the imported submission; select its original document".into()); }
        let result = document_worker::extract(document_worker::WorkerRequest {
            file_path: path.clone(), pdfium_directory: resources::directory_pdfium(&app), models_directory: resources::directory_models_ocr(&app), force_ocr: false, preview_page: Some(page),
            source_handle: None,
            #[cfg(debug_assertions)]
            sandbox_probe: None,
        }, &AtomicBool::new(false), |_, _| {})?;
        if fingerprint(&path)? != expected { return Err("the source document changed while loading; select it again".into()); }
        if result.source_hash != expected { return Err("the rendered document does not match the source fingerprint".into()); }
        result.preview.ok_or("the document worker did not return a preview".into())
    }).await
}

fn save_text(
    conn: &mut Connection,
    submission_id: i64,
    expected_text: &str,
    text: &str,
) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > render::MAX_TEXT_BYTES {
        return Err("extracted text must contain between 1 and 1048576 bytes".into());
    }
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let (current, status): (Option<String>, String) = tx
        .query_row(
            "SELECT text_ocr,status_pipeline FROM submissions WHERE id=?1",
            [submission_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    if status != "ocr_complete" {
        return Err("text can only be corrected before redaction is confirmed".into());
    }
    if current.as_deref() != Some(expected_text) {
        return Err("the extracted text changed; reload this submission before saving".into());
    }
    if text == expected_text {
        return Ok(());
    }
    // Retain the initial extraction inside the encrypted workspace, never in audit payloads.
    tx.execute(
        "UPDATE submissions SET text_initial=COALESCE(text_initial,?1) WHERE id=?2",
        params![expected_text, submission_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "UPDATE submissions SET text_ocr=?1 WHERE id=?2",
        params![text, submission_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute("INSERT INTO logs_audit (submission_id,event,actor,payload_json) VALUES (?1,'extracted_text_corrected','teacher',?2)", params![submission_id,serde_json::json!({"before_bytes":expected_text.len(),"after_bytes":text.len()}).to_string()]).map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_save_extracted_text(
    db: State<'_, Arc<DbState>>,
    submission_id: i64,
    expected_text: String,
    text: String,
) -> Result<(), String> {
    tasks::run(db.inner().clone(), move |db| {
        db.write(|conn| save_text(conn, submission_id, &expected_text, &text))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrections_are_conflict_blocking_audited_and_closed_after_redaction() {
        let directory = tempfile::tempdir().unwrap();
        let db = crate::db::schema::open_with_key(directory.path(), [12; 32]).unwrap();
        db.write(|conn| {
            conn.execute_batch("INSERT INTO rubrics(id,title,subject,grade_level,content_json) VALUES (1,'Synthetic','Science','Year 8','{}'); INSERT INTO assignments(id,rubric_id,text) VALUES (1,1,'Synthetic task'); INSERT INTO submissions(id,assignment_id,text_ocr,status_pipeline) VALUES (1,1,'speod = distance / time','ocr_complete')").map_err(|error| error.to_string())?;
            Ok(())
        }).unwrap();
        db.write(|conn| {
            save_text(
                conn,
                1,
                "speod = distance / time",
                "speed = distance / time",
            )
        })
        .unwrap();
        assert!(db
            .write(|conn| save_text(conn, 1, "speod = distance / time", "stale change"))
            .is_err());
        assert!(db
            .write(|conn| save_text(conn, 1, "speed = distance / time", "  "))
            .is_err());
        {
            let guard = db.conn.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            let initial: String = conn
                .query_row(
                    "SELECT text_initial FROM submissions WHERE id=1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(initial, "speod = distance / time");
            let count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM logs_audit WHERE event='extracted_text_corrected'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1);
        }
        db.write(|conn| {
            conn.execute(
                "UPDATE submissions SET status_pipeline='redacted' WHERE id=1",
                [],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .unwrap();
        assert!(db
            .write(|conn| save_text(conn, 1, "speed = distance / time", "changed after review"))
            .is_err());
        db.seal_and_close().unwrap();
        drop(db);
        let reopened = crate::db::schema::open_with_key(directory.path(), [12; 32]).unwrap();
        let guard = reopened.conn.lock().unwrap();
        let text: String = guard
            .as_ref()
            .unwrap()
            .query_row("SELECT text_ocr FROM submissions WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(text, "speed = distance / time");
    }
}
