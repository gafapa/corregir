use crate::pipeline::inference_client::InferenceClient;

const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_MODEL: &str = "qwen3:8b";

/// Milestone 1: manual testing of the roundtrip full Rust↔Ollama and React↔Tauri,
/// before building anything on top of the pipeline.
#[tauri::command]
pub async fn cmd_test_ai_connection(model: Option<String>) -> Result<String, String> {
    let model = model.unwrap_or_else(|| DEFAULT_MODEL.to_string());
    if model.trim().is_empty() || model.len() > 128 {
        return Err("enter a model name of at most 128 bytes".into());
    }
    let client = InferenceClient::new(DEFAULT_OLLAMA_URL).map_err(|e| e.to_string())?;
    client
        .test_connection(&model)
        .await
        .map_err(|e| e.to_string())
}
