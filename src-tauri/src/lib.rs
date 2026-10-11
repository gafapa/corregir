mod commands;
mod crypto;
mod db;
#[cfg(windows)]
mod document_sandbox;
pub mod document_worker;
mod inference_jobs;
mod models;
mod pipeline;
#[cfg(feature = "qualification")]
mod qualification;
mod resources;
mod tasks;
mod workspace;

pub fn run_document_worker() -> i32 {
    document_worker::run()
}

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, Manager};

#[derive(Default)]
struct ClosingState(AtomicBool);

use commands::backup::{
    cmd_backup_status, cmd_export_backup, cmd_restore_backup, cmd_verify_backup,
};
use commands::configuration::{
    cmd_create_assignment, cmd_create_rubric, cmd_create_rubric_version, cmd_list_assignments,
    cmd_list_rubrics,
};
use commands::diagnostics::cmd_test_ai_connection;
use commands::document_review::{cmd_preview_document, cmd_save_extracted_text};
use commands::export::{
    cmd_export_csv, cmd_export_logs_csv, cmd_export_pdf_feedback, cmd_list_logs_audit,
};
use commands::grading::{
    cmd_load_grading_state, cmd_request_evidence, cmd_request_feedback, cmd_save_grade_tentative,
};
use commands::ingestion::{cmd_cancel_import, cmd_import_submission, cmd_list_submissions};
use commands::recovery::{cmd_recover_workspace, cmd_recovery_status, RecoveryState};
use commands::redaction::{cmd_confirm_redaction, cmd_detect_identifiers, cmd_preview_redaction};
use commands::review::cmd_confirm_grade;
use commands::settings::{cmd_load_ollama_settings, cmd_save_ollama_settings};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(feature = "qualification")]
    let builder = builder
        .manage(qualification::QualificationPicker::default())
        .plugin(qualification::plugin());
    #[cfg(not(feature = "qualification"))]
    let builder = builder.plugin(tauri_plugin_dialog::init());
    builder
        .setup(|app| {
            if cfg!(feature = "qualification") && app.config().identifier != "com.corregir.qualification" {
                return Err("qualification builds require the isolated qualification identifier".into());
            }
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
            let executable_directory = std::env::current_exe()?.parent().ok_or("application directory is missing")?.to_path_buf();
            let workspace = workspace::WorkspaceState::resolve(&executable_directory, directory)?;
            let directory = workspace.directory()?;
            let portable = workspace.portable;
            app.manage(workspace);
            let recovery_reason = if portable { None } else { match db::schema::open(&directory) {
                Ok(db_state)=> {app.manage(std::sync::Arc::new(db_state));None},
                Err(error @ (db::schema::DbError::DecryptionFailed | db::schema::DbError::Keychain(crypto::keychain::KeychainError::MissingExistingCredential)))=>Some(error.to_string()),
                Err(error)=>return Err(format!("could not open the local database: {error}").into()),
            }};
            app.manage(RecoveryState { directory, reason:std::sync::Mutex::new(recovery_reason) });

            app.manage(std::sync::Arc::new(
                commands::ingestion::ImportJobs::default(),
            ));

            app.manage(ClosingState::default());
            app.manage(std::sync::Arc::new(inference_jobs::InferenceJobs::default()));
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
            #[cfg(feature = "qualification")]
            qualification::cmd_set_qualification_picker,
            cmd_test_ai_connection,
            cmd_create_rubric,
            cmd_create_rubric_version,
            cmd_load_ollama_settings,
            cmd_save_ollama_settings,
            cmd_export_backup,
            cmd_restore_backup,
            cmd_backup_status,
            cmd_verify_backup,
            cmd_recover_workspace,
            cmd_recovery_status,
            workspace::cmd_workspace_status,
            workspace::cmd_inspect_work_directory,
            workspace::cmd_open_work_directory,
            cmd_list_rubrics,
            cmd_list_assignments,
            cmd_load_grading_state,
            cmd_create_assignment,
            cmd_import_submission,
            cmd_cancel_import,
            cmd_list_submissions,
            cmd_preview_document,
            cmd_save_extracted_text,
            cmd_detect_identifiers,
            cmd_confirm_redaction,
            cmd_preview_redaction,
            cmd_request_evidence,
            cmd_save_grade_tentative,
            cmd_request_feedback,
            inference_jobs::cmd_cancel_inference,
            cmd_confirm_grade,
            cmd_export_csv,
            cmd_export_logs_csv,
            cmd_export_pdf_feedback,
            cmd_list_logs_audit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
