use crate::{db::DbState, pipeline::inference_client::InferenceClient};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OllamaSettings {
    pub url: String,
    pub model: String,
}
impl Default for OllamaSettings {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:11434".into(),
            model: "qwen3:8b".into(),
        }
    }
}
impl OllamaSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.url.len() > 256
            || self.model.is_empty()
            || self.model.len() > 128
            || !self
                .model
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_:/.-".contains(&byte))
        {
            return Err(
                "enter a local server URL and a model name of at most 128 ASCII characters".into(),
            );
        }
        InferenceClient::new(&self.url).map_err(|error| error.to_string())?;
        Ok(())
    }
}
pub fn load(conn: &Connection) -> Result<OllamaSettings, String> {
    let saved: Option<String> = conn
        .query_row(
            "SELECT value FROM configuration WHERE key='ollama'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let settings: OllamaSettings = saved
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    Ok(settings)
}
#[tauri::command]
pub async fn cmd_load_ollama_settings(
    db: State<'_, std::sync::Arc<DbState>>,
) -> Result<OllamaSettings, String> {
    crate::tasks::run(db.inner().clone(), |db| {
        let guard = db.conn.lock().map_err(|error| error.to_string())?;
        load(guard.as_ref().ok_or("the database is closed")?)
    })
    .await
}
#[tauri::command]
pub async fn cmd_save_ollama_settings(
    db: State<'_, std::sync::Arc<DbState>>,
    settings: OllamaSettings,
) -> Result<(), String> {
    crate::tasks::run(db.inner().clone(), move |db| {
        settings.validate()?;
        let guard = db.conn.lock().map_err(|error| error.to_string())?;
        let conn = guard.as_ref().ok_or("the database is closed")?;
        conn.execute("INSERT INTO configuration (key,value) VALUES ('ollama',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&settings).map_err(|error| error.to_string())?]).map_err(|error| error.to_string())?;
        db.persist(conn).map_err(|error| error.to_string())
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_are_persisted_and_reject_remote_urls() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::schema::open_with_key(dir.path(), [21; 32]).unwrap();
        let settings = OllamaSettings {
            url: "http://localhost:11435".into(),
            model: "llama3.2:3b".into(),
        };
        settings.validate().unwrap();
        {
            let guard = db.conn.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO configuration VALUES ('ollama',?1)",
                [serde_json::to_string(&settings).unwrap()],
            )
            .unwrap();
            db.persist(conn).unwrap();
        }
        drop(db);
        let db = crate::db::schema::open_with_key(dir.path(), [21; 32]).unwrap();
        assert_eq!(
            load(db.conn.lock().unwrap().as_ref().unwrap()).unwrap(),
            settings
        );
        assert!(OllamaSettings {
            url: "http://example.com".into(),
            ..settings.clone()
        }
        .validate()
        .is_err());
        assert!(OllamaSettings {
            model: "a\nmodel".into(),
            ..settings
        }
        .validate()
        .is_err());
    }
}
