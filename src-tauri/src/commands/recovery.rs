use std::{path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroizing;

pub struct RecoveryState {
    pub directory: PathBuf,
    pub reason: Mutex<Option<String>>,
}

#[tauri::command]
pub fn cmd_recovery_status(state: State<'_, RecoveryState>) -> Result<Option<String>, String> {
    Ok(state
        .reason
        .lock()
        .map_err(|error| error.to_string())?
        .clone())
}

#[tauri::command]
pub async fn cmd_recover_workspace(
    app: AppHandle,
    state: State<'_, RecoveryState>,
    source_path: String,
    password: String,
) -> Result<(), String> {
    if app
        .try_state::<std::sync::Arc<crate::db::DbState>>()
        .is_some()
    {
        return Err(
            "this workspace is already open; use the empty-workspace restore option instead".into(),
        );
    }
    let directory = state.directory.clone();
    let password = Zeroizing::new(password);
    let recovered = tauri::async_runtime::spawn_blocking(move || {
        let restored = super::backup::read_backup(&source_path, &password)?;
        crate::db::schema::recover(&directory, restored).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    if !app.manage(std::sync::Arc::new(recovered)) {
        return Err(
            "another recovery operation already opened the workspace; restart the application"
                .into(),
        );
    }
    *state.reason.lock().map_err(|error| error.to_string())? = None;
    Ok(())
}
