//! The only HTTP egress point in the system. The rest of the pipeline is local.
//! The destination host is restricted to localhost (Phase A). Extending to an internal network host
//! of the center is an explicit extension for Phase C/D, not something
//! that should be activated by user configuration without review.

use serde::{Deserialize, Serialize};
use thiserror::Error;

const ALLOWED_HOSTS: &[&str] = &["127.0.0.1", "localhost"];

#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("inference host '{0}' is not allowed (Phase A permits localhost only)")]
    HostNotAllowed(String),
    #[error("Error connecting to the inference server: {0}")]
    Network(#[from] reqwest::Error),
    #[error("the model did not return valid JSON after retries: {0}")]
    InvalidJson(String),
    #[error("inference response exceeds the 2 MiB safety limit")]
    ResponseTooLarge,
    #[error("the document and rubric exceed the local model context limit; use a shorter submission or assignment")]
    InputTooLarge,
}

#[derive(Debug, Clone, Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage<'a>>,
    stream: bool,
    format: &'a serde_json::Value,
    think: bool,
    options: OllamaOptions,
}

#[derive(Debug, Clone, Serialize)]
struct OllamaOptions {
    temperature: f64,
    num_predict: u32,
    num_ctx: u32,
}

#[derive(Debug, Clone, Serialize)]
struct OllamaMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

pub struct InferenceClient {
    base_url: String,
    http: reqwest::Client,
}

impl InferenceClient {
    /// `base_url` typically `http://127.0.0.1:11434`. Fails if the host is not
    /// in the allow-list, before making any network call.
    pub fn new(base_url: impl Into<String>) -> Result<Self, InferenceError> {
        let base_url = base_url.into();
        let parsed = url::Url::parse(&base_url)
            .map_err(|_| InferenceError::HostNotAllowed(base_url.clone()))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| InferenceError::HostNotAllowed(base_url.clone()))?;
        if !ALLOWED_HOSTS.contains(&host)
            || parsed.scheme() != "http"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
        {
            return Err(InferenceError::HostNotAllowed(host.to_string()));
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(180))
                .build()?,
        })
    }

    /// Diagnostic call (Milestone 1): confirms the full roundtrip before
    /// building anything of the correction pipeline.
    pub async fn test_connection(&self, model: &str) -> Result<String, InferenceError> {
        let response = self
            .chat_json(
                model,
                "Respond only with JSON.",
                r#"Respond exactly with {"ok": true, "message": "connection established"}"#,
            )
            .await?;
        Ok(response.to_string())
    }

    /// Sends a conversation of 2 messages (system/user) and forces output JSON.
    /// Retries once if the model does not return a parseable JSON.
    pub async fn chat_json(
        &self,
        model: &str,
        system: &str,
        user: &str,
    ) -> Result<serde_json::Value, InferenceError> {
        self.chat_json_with_schema(model, system, user, &serde_json::json!("json"))
            .await
    }

    pub async fn chat_json_with_schema(
        &self,
        model: &str,
        system: &str,
        user: &str,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value, InferenceError> {
        // UTF-8 bytes conservatively bound token count; reserve output and template space.
        let context_budget = system.len().saturating_add(user.len()).saturating_add(2560);
        if context_budget > 32768 {
            return Err(InferenceError::InputTooLarge);
        }
        let context_size = (context_budget as u32).next_power_of_two().max(4096);
        let mut last_error = String::new();
        for attempt in 0..2 {
            let body = OllamaChatRequest {
                model,
                messages: vec![
                    OllamaMessage {
                        role: "system",
                        content: system,
                    },
                    OllamaMessage {
                        role: "user",
                        content: user,
                    },
                ],
                stream: false,
                format: schema,
                think: false,
                options: OllamaOptions {
                    temperature: 0.0,
                    num_predict: 2048,
                    num_ctx: context_size,
                },
            };

            let mut response = self
                .http
                .post(format!("{}/api/chat", self.base_url))
                .json(&body)
                .send()
                .await?
                .error_for_status()?;
            const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
            if response
                .content_length()
                .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
            {
                return Err(InferenceError::ResponseTooLarge);
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                    return Err(InferenceError::ResponseTooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            let resp: OllamaChatResponse = serde_json::from_slice(&bytes)
                .map_err(|e| InferenceError::InvalidJson(e.to_string()))?;

            match serde_json::from_str::<serde_json::Value>(&resp.message.content) {
                Ok(value) => return Ok(value),
                Err(e) => {
                    last_error = format!("attempt {}: {}", attempt + 1, e);
                }
            }
        }
        Err(InferenceError::InvalidJson(last_error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_host_not_local() {
        let result = InferenceClient::new("http://192.168.1.50:11434");
        assert!(matches!(result, Err(InferenceError::HostNotAllowed(_))));
    }

    #[test]
    fn accepts_localhost() {
        assert!(InferenceClient::new("http://127.0.0.1:11434").is_ok());
        assert!(InferenceClient::new("http://localhost:11434").is_ok());
    }

    #[test]
    fn rejects_unsafe_url_components() {
        for url in [
            "https://localhost:11434",
            "http://user@localhost:11434",
            "http://localhost:11434/path",
            "http://localhost:11434?target=remote",
        ] {
            assert!(InferenceClient::new(url).is_err());
        }
    }

    #[tokio::test]
    async fn rejects_oversized_prompts_before_network_access() {
        let result = InferenceClient::new("http://127.0.0.1:1")
            .unwrap()
            .chat_json("test", "system", &"x".repeat(32768))
            .await;
        assert!(matches!(result, Err(InferenceError::InputTooLarge)));
    }

    fn response_server(response: String) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut buffer = [0; 8192];
            let _ = stream.read(&mut buffer).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}"), server)
    }
    #[tokio::test]
    async fn blocks_redirects_and_oversized_responses() {
        let (url,server)=response_server("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://example.com/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into());
        let result = InferenceClient::new(url)
            .unwrap()
            .chat_json("test", "system", "user")
            .await;
        assert!(matches!(result, Err(InferenceError::InvalidJson(_))));
        server.join().unwrap();
        let (url, server) = response_server(
            "HTTP/1.1 200 OK\r\nContent-Length: 2097153\r\nConnection: close\r\n\r\n".into(),
        );
        let result = InferenceClient::new(url)
            .unwrap()
            .chat_json("test", "system", "user")
            .await;
        assert!(matches!(result, Err(InferenceError::ResponseTooLarge)));
        server.join().unwrap();
    }

    /// Requires Ollama running locally with the model `qwen3:8b` downloaded
    /// (`ollama pull qwen3:8b`). The pilot runs against the teacher's local
    /// Ollama instance, so this test is not suitable for CI without that setup.
    #[tokio::test]
    #[ignore = "requires a local Ollama server and qwen3:8b"]
    async fn roundtrip_against_local_ollama() {
        let client = InferenceClient::new("http://127.0.0.1:11434").unwrap();
        let response = client
            .test_connection("qwen3:8b")
            .await
            .expect("Ollama must be running locally with qwen3:8b downloaded");
        assert!(response.contains("ok"));
    }
}
