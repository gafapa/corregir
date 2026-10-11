use crate::pipeline::inference_client::InferenceClient;

const DEFAULT_MODEL: &str = "qwen3:8b";

/// Milestone 1: manual testing of the roundtrip full Rust↔Ollama and React↔Tauri,
/// before building anything on top of the pipeline.
#[tauri::command]
pub async fn cmd_test_ai_connection(
    model: Option<String>,
    url: Option<String>,
    jobs: tauri::State<'_, std::sync::Arc<crate::inference_jobs::InferenceJobs>>,
) -> Result<String, String> {
    let settings = super::settings::OllamaSettings {
        model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        url: url.unwrap_or_else(|| "http://127.0.0.1:11434".into()),
    };
    settings.validate()?;
    let client = InferenceClient::new(&settings.url).map_err(|e| e.to_string())?;
    let id = format!("diagnostic-{}", rand::random::<u64>());
    let mut job = jobs.inner().begin(id, None).await?;
    job.run(async {
        client
            .test_connection(&settings.model)
            .await
            .map_err(|e| e.to_string())
    })
    .await
}
