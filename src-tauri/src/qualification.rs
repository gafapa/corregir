//! File-picker substitution compiled only into the isolated qualification build.
use std::{path::Path, sync::Mutex};
use tauri::State;
#[derive(Default)]
pub struct QualificationPicker(Mutex<Option<String>>);

#[tauri::command]
pub fn cmd_set_qualification_picker(
    picker: State<'_, QualificationPicker>,
    path: String,
) -> Result<(), String> {
    let selected = Path::new(&path);
    if !selected.is_absolute() {
        return Err("qualification picker requires an absolute path".into());
    }
    let source = std::env::var_os("CORREGIR_QUALIFICATION_SOURCE_PATH")
        .ok_or("qualification source path is not configured")?;
    let output = std::env::var_os("CORREGIR_QUALIFICATION_OUTPUT_ROOT")
        .ok_or("qualification output root is not configured")?;
    let is_source = selected.canonicalize().ok()
        == Some(
            Path::new(&source)
                .canonicalize()
                .map_err(|error| error.to_string())?,
        );
    let parent = selected
        .parent()
        .ok_or("qualification path has no parent")?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let is_output = parent.starts_with(
        Path::new(&output)
            .canonicalize()
            .map_err(|error| error.to_string())?,
    );
    if !is_source && !is_output {
        return Err(
            "qualification picker path is outside the synthetic fixture/output scope".into(),
        );
    }
    *picker.0.lock().map_err(|error| error.to_string())? = Some(path);
    Ok(())
}
#[tauri::command]
fn open(
    picker: State<'_, QualificationPicker>,
    options: serde_json::Value,
) -> Result<Option<String>, String> {
    let _ = options;
    picker
        .0
        .lock()
        .map(|path| path.clone())
        .map_err(|error| error.to_string())
}
#[tauri::command]
fn save(
    picker: State<'_, QualificationPicker>,
    options: serde_json::Value,
) -> Result<Option<String>, String> {
    open(picker, options)
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("dialog")
        .invoke_handler(tauri::generate_handler![open, save])
        .build()
}
