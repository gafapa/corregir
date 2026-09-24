//! Único punto de salida HTTP del sistema. Todo el resto del pipeline es local.
//! El host de destino está restringido a localhost (Fase A). Ampliar a un host
//! de red interna del centro es una extensión explícita para Fase C/D, no algo
//! que deba activarse por configuración de usuario sin revisión.

use serde::{Deserialize, Serialize};
use thiserror::Error;

const ALLOWED_HOSTS: &[&str] = &["127.0.0.1", "localhost"];

#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("host de inferencia no permitido: '{0}' (solo se permite localhost en Fase A)")]
    HostNoPermitido(String),
    #[error("error de red al contactar el servidor de inferencia: {0}")]
    Red(#[from] reqwest::Error),
    #[error("el modelo devolvió una respuesta que no es JSON válido tras los reintentos: {0}")]
    JsonInvalido(String),
}

#[derive(Debug, Clone, Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage<'a>>,
    stream: bool,
    format: &'a str,
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
    /// `base_url` típicamente `http://127.0.0.1:11434`. Falla si el host no está
    /// en la allow-list, antes de hacer ninguna llamada de red.
    pub fn new(base_url: impl Into<String>) -> Result<Self, InferenceError> {
        let base_url = base_url.into();
        let parsed = url::Url::parse(&base_url)
            .map_err(|_| InferenceError::HostNoPermitido(base_url.clone()))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| InferenceError::HostNoPermitido(base_url.clone()))?;
        if !ALLOWED_HOSTS.contains(&host) {
            return Err(InferenceError::HostNoPermitido(host.to_string()));
        }
        Ok(Self {
            base_url,
            http: reqwest::Client::new(),
        })
    }

    /// Llamada de diagnóstico (Hito 1): confirma el roundtrip completo antes de
    /// construir nada del pipeline de corrección encima.
    pub async fn probar_conexion(&self, model: &str) -> Result<String, InferenceError> {
        let respuesta = self
            .chat_json(
                model,
                "Responde solo con JSON.",
                r#"Responde exactamente con {"ok": true, "mensaje": "conexion establecida"}"#,
            )
            .await?;
        Ok(respuesta.to_string())
    }

    /// Envía una conversación de 2 mensajes (system/user) y fuerza salida JSON.
    /// Reintenta una vez si el modelo no devuelve JSON parseable.
    pub async fn chat_json(
        &self,
        model: &str,
        system: &str,
        user: &str,
    ) -> Result<serde_json::Value, InferenceError> {
        let mut ultimo_error = String::new();
        for intento in 0..2 {
            let cuerpo = OllamaChatRequest {
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
                format: "json",
            };

            let resp = self
                .http
                .post(format!("{}/api/chat", self.base_url))
                .json(&cuerpo)
                .send()
                .await?
                .error_for_status()?
                .json::<OllamaChatResponse>()
                .await?;

            match serde_json::from_str::<serde_json::Value>(&resp.message.content) {
                Ok(valor) => return Ok(valor),
                Err(e) => {
                    ultimo_error = format!(
                        "intento {}: {} (contenido crudo: {})",
                        intento + 1,
                        e,
                        resp.message.content
                    );
                }
            }
        }
        Err(InferenceError::JsonInvalido(ultimo_error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rechaza_host_no_local() {
        let resultado = InferenceClient::new("http://192.168.1.50:11434");
        assert!(matches!(resultado, Err(InferenceError::HostNoPermitido(_))));
    }

    #[test]
    fn acepta_localhost() {
        assert!(InferenceClient::new("http://127.0.0.1:11434").is_ok());
        assert!(InferenceClient::new("http://localhost:11434").is_ok());
    }

    /// Requiere Ollama corriendo en local con el modelo `qwen3:8b` descargado
    /// (`ollama pull qwen3:8b`). No apto para CI sin ese requisito; en este
    /// piloto de un solo profesor se ejecuta contra el Ollama real de su
    /// propio equipo, que es exactamente el escenario de producción.
    #[tokio::test]
    async fn roundtrip_real_contra_ollama() {
        let cliente = InferenceClient::new("http://127.0.0.1:11434").unwrap();
        let respuesta = cliente
            .probar_conexion("qwen3:8b")
            .await
            .expect("Ollama debe estar corriendo en local con qwen3:8b descargado");
        assert!(respuesta.contains("ok"));
    }
}
