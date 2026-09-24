use crate::pipeline::inference_client::InferenceClient;

const OLLAMA_URL_POR_DEFECTO: &str = "http://127.0.0.1:11434";
const MODELO_POR_DEFECTO: &str = "qwen3:8b";

/// Hito 1: prueba manual del roundtrip completo Rust↔Ollama y React↔Tauri,
/// antes de construir nada del pipeline de corrección encima.
#[tauri::command]
pub async fn cmd_probar_conexion_ia(modelo: Option<String>) -> Result<String, String> {
    let modelo = modelo.unwrap_or_else(|| MODELO_POR_DEFECTO.to_string());
    let cliente = InferenceClient::new(OLLAMA_URL_POR_DEFECTO).map_err(|e| e.to_string())?;
    cliente
        .probar_conexion(&modelo)
        .await
        .map_err(|e| e.to_string())
}
