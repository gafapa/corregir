mod commands;
mod crypto;
mod db;
mod modelos;
mod pipeline;

use tauri::Manager;

use commands::configuracion::{cmd_crear_rubrica, cmd_listar_rubricas};
use commands::diagnostico::cmd_probar_conexion_ia;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let estado = db::schema::abrir(&dir)
                .map_err(|e| format!("no se pudo abrir la base de datos local: {e}"))?;
            app.manage(estado);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(db) = window.app_handle().try_state::<db::DbState>() {
                    let _ = db.sellar();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            cmd_probar_conexion_ia,
            cmd_crear_rubrica,
            cmd_listar_rubricas,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
