//! Bounded workers for database, parsing, and export operations.
use crate::db::DbState;
use std::sync::Arc;

static WORKERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub async fn run<T, F>(db: Arc<DbState>, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&DbState) -> Result<T, String> + Send + 'static,
{
    let _permit = WORKERS.acquire().await.map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || work(&db))
        .await
        .map_err(|e| e.to_string())?
}
