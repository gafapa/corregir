use crate::{
    db::{backup, DbState},
    tasks,
};
use std::{io::Write, path::Path};
use tauri::{AppHandle, State};
use zeroize::Zeroizing;

#[tauri::command]
pub async fn cmd_export_backup(
    app: AppHandle,
    db: State<'_, std::sync::Arc<DbState>>,
    destination_path: String,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    tasks::run(db.inner().clone(), move |db| {
        let path =
            super::export::validate_destination(&app, db, &destination_path, "corregirbackup")?;
        let encrypted = {
            let guard = db.conn.lock().map_err(|error| error.to_string())?;
            backup::encrypt(guard.as_ref().ok_or("the database is closed")?, &password)?
        };
        let mut file =
            tempfile::NamedTempFile::new_in(path.parent().ok_or("missing backup directory")?)
                .map_err(|error| error.to_string())?;
        file.write_all(&encrypted)
            .map_err(|error| error.to_string())?;
        file.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        file.persist(path)
            .map_err(|error| error.error.to_string())?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn cmd_restore_backup(
    db: State<'_, std::sync::Arc<DbState>>,
    source_path: String,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    tasks::run(db.inner().clone(), move |db| {
        backup::restore(db, read_backup(&source_path, &password)?)
    })
    .await
}

pub(super) fn read_backup(
    source_path: &str,
    password: &str,
) -> Result<rusqlite::Connection, String> {
    let path = Path::new(source_path);
    if !path.is_absolute()
        || !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("corregirbackup"))
    {
        return Err("select an absolute .corregirbackup file".into());
    }
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len()
        > backup::MAX_BACKUP_BYTES as u64 + 52
    {
        return Err("backup exceeds the 128 MiB limit".into());
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(backup::MAX_BACKUP_BYTES as u64 + 53)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    backup::decrypt(&bytes, password)
}
