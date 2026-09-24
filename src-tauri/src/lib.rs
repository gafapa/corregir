mod commands;
mod crypto;
mod db;
mod modelos;
mod pipeline;
mod recursos;

use tauri::Manager;

use commands::anonimizacion::{cmd_confirmar_anonimizacion, cmd_detectar_identificadores};
use commands::configuracion::{cmd_crear_enunciado, cmd_crear_rubrica, cmd_listar_rubricas};
use commands::correccion::{cmd_guardar_nota_tentativa, cmd_invocar_evidencias, cmd_invocar_feedback};
use commands::diagnostico::cmd_probar_conexion_ia;
use commands::ingesta::{cmd_importar_entrega, cmd_listar_entregas};
use pipeline::ocr_engine::MotorOcr;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let estado_db = db::schema::abrir(&dir)
                .map_err(|e| format!("no se pudo abrir la base de datos local: {e}"))?;
            app.manage(estado_db);

            let motor_ocr = MotorOcr::cargar(&recursos::dir_modelos_ocr(&app.handle()))
                .map_err(|e| format!("no se pudieron cargar los modelos de OCR: {e}"))?;
            app.manage(motor_ocr);

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
            cmd_crear_enunciado,
            cmd_importar_entrega,
            cmd_listar_entregas,
            cmd_detectar_identificadores,
            cmd_confirmar_anonimizacion,
            cmd_invocar_evidencias,
            cmd_guardar_nota_tentativa,
            cmd_invocar_feedback,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
