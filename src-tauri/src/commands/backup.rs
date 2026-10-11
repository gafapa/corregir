use crate::{
    db::{backup, DbState},
    tasks,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};
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
        // Exercise the real recovery decoder before declaring a backup usable.
        let _verified = backup::decrypt(&encrypted, &password)?;
        let hash = digest(&encrypted);
        let mut file =
            tempfile::NamedTempFile::new_in(path.parent().ok_or("missing backup directory")?)
                .map_err(|error| error.to_string())?;
        file.write_all(&encrypted)
            .map_err(|error| error.to_string())?;
        file.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        file.persist(&path)
            .map_err(|error| error.error.to_string())?;
        record_backup(
            db,
            BackupRecord {
                path: path.to_string_lossy().into(),
                hash,
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                verified_at: chrono::Utc::now().to_rfc3339(),
            },
        )?;
        Ok(())
    })
    .await
}

#[derive(Serialize, Deserialize)]
struct BackupRecord {
    path: String,
    hash: String,
    created_at: Option<String>,
    verified_at: String,
}
#[derive(Serialize)]
pub struct BackupStatus {
    last_created_at: Option<String>,
    last_verified_at: Option<String>,
    needs_backup: bool,
    integrity_ok: Option<bool>,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn record_backup(db: &DbState, record: BackupRecord) -> Result<(), String> {
    db.write(|conn| {
        let json = serde_json::to_string(&record).map_err(|error| error.to_string())?;
        conn.execute("INSERT INTO configuration(key,value) VALUES ('backup.health',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [json]).map_err(|error| error.to_string())?;
        Ok(())
    })
}

fn load_record(db: &DbState) -> Result<Option<BackupRecord>, String> {
    use rusqlite::OptionalExtension;
    let guard = db.conn.lock().map_err(|error| error.to_string())?;
    let saved: Option<String> = guard
        .as_ref()
        .ok_or("the database is closed")?
        .query_row(
            "SELECT value FROM configuration WHERE key='backup.health'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    saved
        .map(|json| serde_json::from_str(&json).map_err(|error| error.to_string()))
        .transpose()
}

fn file_digest(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let size = file.metadata().map_err(|error| error.to_string())?.len();
    if size > backup::MAX_BACKUP_BYTES as u64 + 52 {
        return Err("backup exceeds its size limit".into());
    }
    let mut source = file.take(backup::MAX_BACKUP_BYTES as u64 + 53);
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    let mut count = 0;
    loop {
        let read = source.read(&mut bytes).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        count += read as u64;
        if count > backup::MAX_BACKUP_BYTES as u64 + 52 {
            return Err("backup exceeds its size limit".into());
        }
        hash.update(&bytes[..read]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[tauri::command]
pub async fn cmd_backup_status(
    db: State<'_, std::sync::Arc<DbState>>,
) -> Result<BackupStatus, String> {
    tasks::run(db.inner().clone(), backup_health).await
}

fn backup_health(db: &DbState) -> Result<BackupStatus, String> {
    let Some(record) = load_record(db)? else {
        return Ok(BackupStatus {
            last_created_at: None,
            last_verified_at: None,
            needs_backup: true,
            integrity_ok: None,
        });
    };
    let integrity_ok = file_digest(Path::new(&record.path)).is_ok_and(|hash| hash == record.hash);
    let fresh = record
        .created_at
        .as_ref()
        .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok())
        .is_some_and(|date| {
            let age = chrono::Utc::now().signed_duration_since(date);
            age >= chrono::Duration::zero() && age < chrono::Duration::days(7)
        });
    Ok(BackupStatus {
        last_created_at: record.created_at,
        last_verified_at: Some(record.verified_at),
        needs_backup: !fresh || !integrity_ok,
        integrity_ok: Some(integrity_ok),
    })
}

#[tauri::command]
pub async fn cmd_verify_backup(
    db: State<'_, std::sync::Arc<DbState>>,
    source_path: String,
    password: String,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
    tasks::run(db.inner().clone(), move |db| {
        let bytes = read_backup_bytes(&source_path)?;
        let _restored = backup::decrypt(&bytes, &password)?;
        let path = Path::new(&source_path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let hash = digest(&bytes);
        let created_at = load_record(db)?
            .filter(|record| record.hash == hash)
            .and_then(|record| record.created_at);
        record_backup(
            db,
            BackupRecord {
                path: path.to_string_lossy().into(),
                hash,
                created_at,
                verified_at: chrono::Utc::now().to_rfc3339(),
            },
        )
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
    backup::decrypt(&read_backup_bytes(source_path)?, password)
}

fn read_backup_bytes(source_path: &str) -> Result<Vec<u8>, String> {
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
    if bytes.len() > backup::MAX_BACKUP_BYTES + 52 {
        return Err("backup exceeds the 128 MiB limit".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verified_backup_health_detects_missing_changed_and_old_copies() {
        let directory = tempfile::tempdir().unwrap();
        let db = crate::db::schema::open_with_key(directory.path(), [23; 32]).unwrap();
        assert!(backup_health(&db).unwrap().needs_backup);
        let bytes = {
            let guard = db.conn.lock().unwrap();
            backup::encrypt(guard.as_ref().unwrap(), "synthetic backup password").unwrap()
        };
        let path = directory.path().join("synthetic.corregirbackup");
        std::fs::write(&path, &bytes).unwrap();
        let decoded = read_backup(path.to_str().unwrap(), "synthetic backup password").unwrap();
        assert_eq!(
            decoded
                .query_row("SELECT COUNT(*) FROM rubrics", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let hash = digest(&bytes);
        record_backup(
            &db,
            BackupRecord {
                path: path.to_string_lossy().into(),
                hash: hash.clone(),
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                verified_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .unwrap();
        let health = backup_health(&db).unwrap();
        assert!(!health.needs_backup);
        assert_eq!(health.integrity_ok, Some(true));
        record_backup(
            &db,
            BackupRecord {
                path: path.to_string_lossy().into(),
                hash,
                created_at: Some((chrono::Utc::now() - chrono::Duration::days(8)).to_rfc3339()),
                verified_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .unwrap();
        assert!(backup_health(&db).unwrap().needs_backup);
        std::fs::write(&path, b"damaged backup").unwrap();
        assert_eq!(backup_health(&db).unwrap().integrity_ok, Some(false));
        assert!(read_backup(path.to_str().unwrap(), "synthetic backup password").is_err());
        std::fs::remove_file(path).unwrap();
        assert_eq!(backup_health(&db).unwrap().integrity_ok, Some(false));
    }
}
