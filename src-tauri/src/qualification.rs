//! File-picker and confirmation-dialog substitution compiled only into the isolated
//! qualification build.
use std::{path::Path, sync::Mutex};
use tauri::State;
#[derive(Default)]
pub struct QualificationPicker(Mutex<Option<String>>);

/// Answer given to confirmation dialogs (cancel by default) and the messages asked.
#[derive(Default)]
pub struct QualificationAnswer(Mutex<(bool, Vec<String>)>);

/// Set the answer to later confirmation dialogs; returns and clears the messages
/// asked since the previous call.
#[tauri::command]
pub fn cmd_set_qualification_answer(
    answer: State<'_, QualificationAnswer>,
    confirm: bool,
) -> Result<Vec<String>, String> {
    let mut answer = answer.0.lock().map_err(|error| error.to_string())?;
    answer.0 = confirm;
    Ok(std::mem::take(&mut answer.1))
}

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

/// The dialog plugin's `ask` and `confirm` call `message` and compare the label of
/// the chosen button with their OK label.
#[tauri::command]
fn message(
    answer: State<'_, QualificationAnswer>,
    message: String,
    buttons: Option<serde_json::Value>,
) -> Result<String, String> {
    let labels = match &buttons {
        Some(serde_json::Value::String(kind)) if kind == "YesNo" => ("Yes".into(), "No".into()),
        Some(serde_json::Value::String(kind)) if kind == "OkCancel" => {
            ("Ok".into(), "Cancel".into())
        }
        Some(value) => match value
            .get("OkCancelCustom")
            .and_then(|labels| labels.as_array())
        {
            Some(labels) if labels.len() == 2 => (
                labels[0].as_str().unwrap_or_default().to_string(),
                labels[1].as_str().unwrap_or_default().to_string(),
            ),
            _ => return Err("unsupported qualification dialog buttons".into()),
        },
        None => ("Ok".into(), "Ok".into()),
    };
    let mut answer = answer.0.lock().map_err(|error| error.to_string())?;
    answer.1.push(message);
    Ok(if answer.0 { labels.0 } else { labels.1 })
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("dialog")
        .invoke_handler(tauri::generate_handler![open, save, message])
        .build()
}
