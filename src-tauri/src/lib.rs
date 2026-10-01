mod commands;
mod crypto;
mod db;
mod models;
mod pipeline;
mod resources;
mod tasks;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};

#[derive(Default)]
struct ClosingState(AtomicBool);

use commands::backup::{cmd_export_backup, cmd_restore_backup};
use commands::configuration::{
    cmd_create_assignment, cmd_create_rubric, cmd_create_rubric_version, cmd_list_assignments,
    cmd_list_rubrics,
};
use commands::diagnostics::cmd_test_ai_connection;
use commands::export::{
    cmd_export_csv, cmd_export_logs_csv, cmd_export_pdf_feedback, cmd_list_logs_audit,
};
use commands::grading::{
    cmd_load_grading_state, cmd_request_evidence, cmd_request_feedback, cmd_save_grade_tentative,
};
use commands::ingestion::{cmd_cancel_import, cmd_import_submission, cmd_list_submissions};
use commands::recovery::{cmd_recover_workspace, cmd_recovery_status, RecoveryState};
use commands::redaction::{cmd_confirm_redaction, cmd_detect_identifiers};
use commands::review::cmd_confirm_grade;
use commands::settings::{cmd_load_ollama_settings, cmd_save_ollama_settings};
use pipeline::ocr_engine::OcrEngine;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = if cfg!(debug_assertions) {
                match std::env::var_os("CORREGIR_DEV_DATA_DIR") {
                    Some(path) => {
                        let path = std::path::PathBuf::from(path);
                        if !path.is_absolute() {
                            return Err("CORREGIR_DEV_DATA_DIR must be an absolute path".into());
                        }
                        path
                    }
                    None => app.path().app_data_dir()?,
                }
            } else {
                app.path().app_data_dir()?
            };
            std::fs::create_dir_all(&directory)?;
            let recovery_reason=match db::schema::open(&directory) {
                Ok(db_state)=> {app.manage(std::sync::Arc::new(db_state));None},
                Err(error @ (db::schema::DbError::DecryptionFailed | db::schema::DbError::Keychain(crypto::keychain::KeychainError::MissingExistingCredential)))=>Some(error.to_string()),
                Err(error)=>return Err(format!("could not open the local database: {error}").into()),
            };
            app.manage(RecoveryState { directory, reason:std::sync::Mutex::new(recovery_reason) });

            let ocr_engine = OcrEngine::load(&resources::directory_models_ocr(&app.handle()))
                .map_err(|e| format!("could not load OCR models: {e}"))?;
            app.manage(std::sync::Arc::new(ocr_engine));
            app.manage(std::sync::Arc::new(
                commands::ingestion::ImportJobs::default(),
            ));

            app.manage(ClosingState::default());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                let closing = app.state::<ClosingState>();
                if closing.0.swap(true, Ordering::SeqCst) {
                    return;
                }
                let Some(db)=app.try_state::<std::sync::Arc<db::DbState>>().map(|state|state.inner().clone()) else {app.exit(0);return;};
                tauri::async_runtime::spawn(async move {
                    let result = tauri::async_runtime::spawn_blocking(move || db.seal_and_close()).await;
                    match result {
                        Ok(Ok(())) => app.exit(0),
                        _ => {
                            app.state::<ClosingState>().0.store(false, Ordering::SeqCst);
                            let _ = app.emit("database-close-error", "Could not save the encrypted database. Free disk space and try closing again.");
                            eprintln!("warning: encrypted database close failed; the application remains open");
                        }
                    }
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            cmd_test_ai_connection,
            cmd_create_rubric,
            cmd_create_rubric_version,
            cmd_load_ollama_settings,
            cmd_save_ollama_settings,
            cmd_export_backup,
            cmd_restore_backup,
            cmd_recover_workspace,
            cmd_recovery_status,
            cmd_list_rubrics,
            cmd_list_assignments,
            cmd_load_grading_state,
            cmd_create_assignment,
            cmd_import_submission,
            cmd_cancel_import,
            cmd_list_submissions,
            cmd_detect_identifiers,
            cmd_confirm_redaction,
            cmd_request_evidence,
            cmd_save_grade_tentative,
            cmd_request_feedback,
            cmd_confirm_grade,
            cmd_export_csv,
            cmd_export_logs_csv,
            cmd_export_pdf_feedback,
            cmd_list_logs_audit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
