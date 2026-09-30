//! SQLite lives in memory. Only authenticated encrypted snapshots reach disk.
use crate::crypto::keychain::{self, KeychainError};
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use fs2::FileExt;
use rusqlite::{Connection, MAIN_DB};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;
use zeroize::Zeroizing;

const INITIAL_MIGRATION: &str = include_str!("migrations/0001_init.sql");
const LEGACY_SCHEMA_MIGRATION: &str = include_str!("migrations/0002_legacy_names.sql");
const STATE_MIGRATION: &str = include_str!("migrations/0003_workflow.sql");
const ENCRYPTED_DB_NAME: &str = "corregir.sqlite.enc";
const MAX_DATABASE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("credential error: {0}")]
    Keychain(#[from] KeychainError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database file I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid passphrase format (expected 32 hexadecimal bytes)")]
    InvalidPassphrase,
    #[error("could not decrypt the database (wrong key or corrupt file)")]
    DecryptionFailed,
    #[error("database connection mutex poisoned by a previous panic")]
    PoisonedMutex,
    #[error("another instance is already using this data directory")]
    AlreadyOpen,
    #[error("database exceeds the 512 MiB safety limit or is corrupt")]
    InvalidDatabase,
}

pub struct DbState {
    pub conn: Mutex<Option<Connection>>,
    pub directory: PathBuf,
    path_encrypted: PathBuf,
    key: Zeroizing<[u8; 32]>,
    _directory_lock: File,
}

impl DbState {
    pub fn seal(&self) -> Result<(), DbError> {
        let guard = self.conn.lock().map_err(|_| DbError::PoisonedMutex)?;
        if let Some(conn) = guard.as_ref() {
            self.persist(conn)?;
        }
        Ok(())
    }

    pub fn persist(&self, conn: &Connection) -> Result<(), DbError> {
        let data = conn.serialize(MAIN_DB)?;
        if data.len() as u64 > MAX_DATABASE_BYTES {
            return Err(DbError::InvalidDatabase);
        }
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(self.key.as_ref()));
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let encrypted = cipher
            .encrypt(&nonce, data.as_ref())
            .map_err(|_| DbError::DecryptionFailed)?;
        let mut snapshot = tempfile::NamedTempFile::new_in(&self.directory)?;
        snapshot.write_all(&nonce)?;
        snapshot.write_all(&encrypted)?;
        snapshot.as_file().sync_all()?;
        snapshot
            .persist(&self.path_encrypted)
            .map_err(|error| error.error)?;
        Ok(())
    }

    pub fn seal_and_close(&self) -> Result<(), DbError> {
        let mut guard = self.conn.lock().map_err(|_| DbError::PoisonedMutex)?;
        if let Some(conn) = guard.as_ref() {
            self.persist(conn)?;
        }
        *guard = None;
        Ok(())
    }
}

fn derive_key(passphrase: &str) -> Result<[u8; 32], DbError> {
    let bytes = Zeroizing::new(hex::decode(passphrase).map_err(|_| DbError::InvalidPassphrase)?);
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| DbError::InvalidPassphrase)
}

fn decrypt(path: &Path, key: &[u8; 32]) -> Result<Zeroizing<Vec<u8>>, DbError> {
    if fs::metadata(path)?.len() > MAX_DATABASE_BYTES + 28 {
        return Err(DbError::InvalidDatabase);
    }
    let content = fs::read(path)?;
    if content.len() < 28 {
        return Err(DbError::DecryptionFailed);
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(&content[..12]), &content[12..])
        .map(Zeroizing::new)
        .map_err(|_| DbError::DecryptionFailed)
}

pub fn open(directory: &Path) -> Result<DbState, DbError> {
    let directory_lock = lock_directory(directory)?;
    let passphrase = Zeroizing::new(keychain::load_passphrase(
        !directory.join(ENCRYPTED_DB_NAME).exists(),
    )?);
    open_locked(directory, derive_key(&passphrase)?, directory_lock)
}

#[cfg(test)]
pub fn open_with_key(directory: &Path, key: [u8; 32]) -> Result<DbState, DbError> {
    let directory_lock = lock_directory(directory)?;
    open_locked(directory, key, directory_lock)
}

fn lock_directory(directory: &Path) -> Result<File, DbError> {
    fs::create_dir_all(directory)?;
    let directory_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("corregir.lock"))?;
    directory_lock
        .try_lock_exclusive()
        .map_err(|_| DbError::AlreadyOpen)?;
    Ok(directory_lock)
}

fn open_locked(directory: &Path, key: [u8; 32], directory_lock: File) -> Result<DbState, DbError> {
    let path_encrypted = directory.join(ENCRYPTED_DB_NAME);
    let mut conn = Connection::open_in_memory()?;
    // Authenticate the existing snapshot before considering a legacy working copy.
    if path_encrypted.exists() {
        let data = decrypt(&path_encrypted, &key)?;
        conn.deserialize_read_exact(MAIN_DB, data.as_slice(), data.len(), false)?;
    }
    let legacy_path = directory.join("corregir.sqlite");
    if legacy_path.exists() {
        if fs::metadata(&legacy_path)?.len() > MAX_DATABASE_BYTES {
            return Err(DbError::InvalidDatabase);
        }
        // SQLite recovers committed legacy WAL/journal changes before the backup.
        let legacy = Connection::open(&legacy_path)?;
        let backup = rusqlite::backup::Backup::new(&legacy, &mut conn)?;
        backup.run_to_completion(128, std::time::Duration::from_millis(5), None)?;
    }
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON; PRAGMA temp_store = MEMORY; PRAGMA journal_mode = MEMORY;")?;
    let integrity: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(DbError::InvalidDatabase);
    }
    let legacy_schema: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='rubricas')",
        [],
        |row| row.get(0),
    )?;
    if legacy_schema {
        let tx = conn.transaction()?;
        tx.execute_batch(LEGACY_SCHEMA_MIGRATION)?;
        tx.commit()?;
    }
    conn.execute_batch(INITIAL_MIGRATION)?;
    let has_revision: bool = conn
        .prepare("PRAGMA table_info(submissions)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == "grade_revision");
    if !has_revision {
        let tx = conn.transaction()?;
        tx.execute_batch(STATE_MIGRATION)?;
        tx.commit()?;
    }
    let state = DbState {
        conn: Mutex::new(Some(conn)),
        directory: directory.to_path_buf(),
        path_encrypted,
        key: Zeroizing::new(key),
        _directory_lock: directory_lock,
    };
    state.seal()?;
    // Remove only obsolete application-owned plaintext files after durable migration.
    for name in [
        "corregir.sqlite",
        "corregir.sqlite-journal",
        "corregir.sqlite-wal",
        "corregir.sqlite-shm",
    ] {
        match fs::remove_file(directory.join(name)) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    const KEY: [u8; 32] = [42; 32];
    #[test]
    fn creates_seals_and_reopens_database_without_plaintext() {
        let directory = tempdir().unwrap();
        {
            let state = open_with_key(directory.path(), KEY).unwrap();
            {
                let guard = state.conn.lock().unwrap();
                let conn = guard.as_ref().unwrap();
                conn.execute(
                    "INSERT INTO configuration VALUES ('test','secret value')",
                    [],
                )
                .unwrap();
                state.persist(conn).unwrap();
            }
            assert!(!directory.path().join("corregir.sqlite").exists());
            assert!(!String::from_utf8_lossy(
                &fs::read(directory.path().join(ENCRYPTED_DB_NAME)).unwrap()
            )
            .contains("secret value"));
        }
        let state = open_with_key(directory.path(), KEY).unwrap();
        let guard = state.conn.lock().unwrap();
        let value: String = guard
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT value FROM configuration WHERE key='test'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, "secret value");
    }
    #[test]
    fn rejects_second_instance_and_releases_lock_on_drop() {
        let directory = tempdir().unwrap();
        let state = open_with_key(directory.path(), KEY).unwrap();
        assert!(matches!(
            open_with_key(directory.path(), KEY),
            Err(DbError::AlreadyOpen)
        ));
        state.seal_and_close().unwrap();
        state.seal_and_close().unwrap();
        assert!(state.conn.lock().unwrap().is_none());
        drop(state);
        assert!(open_with_key(directory.path(), KEY).is_ok());
    }
    #[test]
    fn rejects_wrong_key_without_creating_plaintext() {
        let directory = tempdir().unwrap();
        drop(open_with_key(directory.path(), KEY).unwrap());
        assert!(matches!(
            open_with_key(directory.path(), [7; 32]),
            Err(DbError::DecryptionFailed)
        ));
        assert!(!directory.path().join("corregir.sqlite").exists());
    }
    #[test]
    fn recovers_newer_legacy_working_copy_before_removing_it() {
        let directory = tempdir().unwrap();
        drop(open_with_key(directory.path(), KEY).unwrap());
        let legacy = Connection::open(directory.path().join("corregir.sqlite")).unwrap();
        legacy.execute_batch(INITIAL_MIGRATION).unwrap();
        legacy
            .execute(
                "INSERT INTO configuration VALUES ('recovered','latest')",
                [],
            )
            .unwrap();
        drop(legacy);
        let state = open_with_key(directory.path(), KEY).unwrap();
        assert!(!directory.path().join("corregir.sqlite").exists());
        let guard = state.conn.lock().unwrap();
        let value: String = guard
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT value FROM configuration WHERE key='recovered'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, "latest");
    }
}
