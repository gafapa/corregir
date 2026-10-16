//! SQLite lives in memory. Only authenticated encrypted snapshots reach disk.
use crate::crypto::keychain::{self, KeychainError};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use fs2::FileExt;
use rand::RngExt;
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
const CLASSROOM_MIGRATION: &str = include_str!("migrations/0004_classroom.sql");
/// `PRAGMA user_version` after every migration in this file has been applied.
const SCHEMA_VERSION: i64 = 4;
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
    #[error("this work directory was saved by a newer version of Corregir; update the application before opening it")]
    NewerSchema,
}

pub struct DbState {
    pub conn: Mutex<Option<Connection>>,
    pub directory: PathBuf,
    path_encrypted: PathBuf,
    key: Zeroizing<[u8; 32]>,
    _directory_lock: File,
}

impl DbState {
    /// Mutate an isolated in-memory copy and publish it only after durable persistence.
    pub fn write<T>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        self.write_with_persist(work, |connection| {
            self.persist(connection).map_err(|error| error.to_string())
        })
    }

    fn write_with_persist<T>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<T, String>,
        persist: impl FnOnce(&Connection) -> Result<(), String>,
    ) -> Result<T, String> {
        let mut guard = self.conn.lock().map_err(|error| error.to_string())?;
        let current = guard.as_ref().ok_or("the database is closed")?;
        let mut staged = Connection::open_in_memory().map_err(|error| error.to_string())?;
        {
            let copy = rusqlite::backup::Backup::new(current, &mut staged)
                .map_err(|error| error.to_string())?;
            copy.run_to_completion(128, std::time::Duration::ZERO, None)
                .map_err(|error| error.to_string())?;
        }
        staged.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON; PRAGMA temp_store=MEMORY; PRAGMA journal_mode=MEMORY;").map_err(|error| error.to_string())?;
        let result = work(&mut staged)?;
        if !staged.is_autocommit() {
            return Err("workspace mutation left an unfinished transaction".into());
        }
        persist(&staged)?;
        *guard = Some(staged);
        Ok(result)
    }

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
        let cipher =
            Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|_| DbError::InvalidPassphrase)?;
        let mut nonce_bytes = [0u8; 12];
        rand::rng().fill(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);
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
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| DbError::InvalidPassphrase)?;
    let nonce = Nonce::try_from(&content[..12]).map_err(|_| DbError::DecryptionFailed)?;
    cipher
        .decrypt(&nonce, &content[12..])
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

/// Explicit recovery preserves the unreadable encrypted snapshot before replacement.
pub fn recover(directory: &Path, recovered: Connection) -> Result<DbState, DbError> {
    let directory_lock = lock_directory(directory)?;
    if [
        "corregir.sqlite",
        "corregir.sqlite-wal",
        "corregir.sqlite-shm",
        "corregir.sqlite-journal",
    ]
    .iter()
    .any(|name| directory.join(name).exists())
    {
        return Err(DbError::InvalidDatabase);
    }
    let passphrase = Zeroizing::new(keychain::load_passphrase(true)?);
    recover_locked(
        directory,
        recovered,
        derive_key(&passphrase)?,
        directory_lock,
    )
}

fn recover_locked(
    directory: &Path,
    recovered: Connection,
    key: [u8; 32],
    directory_lock: File,
) -> Result<DbState, DbError> {
    let path_encrypted = directory.join(ENCRYPTED_DB_NAME);
    if path_encrypted.exists() {
        if fs::metadata(&path_encrypted)?.len() > MAX_DATABASE_BYTES + 28 {
            return Err(DbError::InvalidDatabase);
        }
        let mut archive = tempfile::Builder::new()
            .prefix("corregir-before-recovery-")
            .suffix(".enc")
            .tempfile_in(directory)?;
        std::io::copy(&mut File::open(&path_encrypted)?, &mut archive)?;
        archive.as_file().sync_all()?;
        archive.keep().map_err(|error| error.error)?;
    }
    let state = DbState {
        conn: Mutex::new(Some(recovered)),
        directory: directory.to_path_buf(),
        path_encrypted,
        key: Zeroizing::new(key),
        _directory_lock: directory_lock,
    };
    state.seal()?;
    Ok(state)
}

#[cfg(test)]
pub fn open_with_key(directory: &Path, key: [u8; 32]) -> Result<DbState, DbError> {
    let directory_lock = lock_directory(directory)?;
    open_locked(directory, key, directory_lock)
}

pub(crate) fn lock_directory(directory: &Path) -> Result<File, DbError> {
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

/// Bring any supported workspace, including a restored backup, to the current schema.
pub fn migrate(conn: &mut Connection) -> Result<(), DbError> {
    // An older application must not write to a schema it does not know.
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(DbError::NewerSchema);
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
    if version < SCHEMA_VERSION {
        let tx = conn.transaction()?;
        tx.execute_batch(CLASSROOM_MIGRATION)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
    }
    Ok(())
}

pub(crate) fn open_locked(
    directory: &Path,
    key: [u8; 32],
    directory_lock: File,
) -> Result<DbState, DbError> {
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
    migrate(&mut conn)?;
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
    fn schemas_from_newer_versions_are_refused_unchanged() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(migrate(&mut conn), Err(DbError::NewerSchema)));
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION + 1);
    }
    #[cfg(windows)]
    #[test]
    fn an_actual_windows_replacement_failure_never_publishes_the_mutation() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempdir().unwrap();
        let state = open_with_key(directory.path(), KEY).unwrap();
        let path = directory.path().join(ENCRYPTED_DB_NAME);
        let before = fs::read(&path).unwrap();
        // FILE_SHARE_READ permits readback but denies replacement while this handle lives.
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(state
            .write(|conn| {
                conn.execute(
                    "INSERT INTO configuration VALUES ('not-published','value')",
                    [],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .is_err());
        assert_eq!(
            state
                .conn
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM configuration WHERE key='not-published'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        drop(locked);
        drop(state);
        let reopened = open_with_key(directory.path(), KEY).unwrap();
        assert_eq!(
            reopened
                .conn
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM configuration WHERE key='not-published'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }
    #[test]
    fn durable_mutations_keep_memory_and_disk_unchanged_on_failure() {
        let directory = tempdir().unwrap();
        let state = open_with_key(directory.path(), KEY).unwrap();
        let before = fs::read(directory.path().join(ENCRYPTED_DB_NAME)).unwrap();
        for stage in ["write", "sync", "replace"] {
            let result = state.write_with_persist(
                |conn| {
                    conn.execute(
                        "INSERT INTO configuration VALUES ('failure-proof','value')",
                        [],
                    )
                    .map_err(|error| error.to_string())?;
                    Ok(())
                },
                |_| Err(format!("injected {stage} failure")),
            );
            assert!(result.is_err());
            let count: i64 = state
                .conn
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM configuration WHERE key='failure-proof'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
            assert_eq!(
                fs::read(directory.path().join(ENCRYPTED_DB_NAME)).unwrap(),
                before
            );
        }
        assert!(state
            .write::<()>(|conn| {
                conn.execute(
                    "INSERT INTO configuration VALUES ('failure-proof','value')",
                    [],
                )
                .unwrap();
                Err("invalid later operation".into())
            })
            .is_err());
        state
            .write(|conn| {
                conn.execute(
                    "INSERT INTO configuration VALUES ('failure-proof','saved')",
                    [],
                )
                .map_err(|error| error.to_string())?;
                Ok(())
            })
            .unwrap();
        drop(state);
        let reopened = open_with_key(directory.path(), KEY).unwrap();
        let value: String = reopened
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT value FROM configuration WHERE key='failure-proof'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "saved");
        assert!(!directory.path().join("corregir.sqlite").exists());
    }
    #[test]
    fn explicit_recovery_preserves_the_original_and_uses_a_new_device_key() {
        let directory = tempdir().unwrap();
        let state = open_with_key(directory.path(), KEY).unwrap();
        let original = fs::read(directory.path().join(ENCRYPTED_DB_NAME)).unwrap();
        let data = state
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .serialize(MAIN_DB)
            .unwrap()
            .to_vec();
        let mut restored = Connection::open_in_memory().unwrap();
        restored
            .deserialize_read_exact(MAIN_DB, data.as_slice(), data.len(), false)
            .unwrap();
        restored
            .execute(
                "INSERT INTO configuration VALUES ('recovery-proof','preserved')",
                [],
            )
            .unwrap();
        drop(state);
        drop(
            recover_locked(
                directory.path(),
                restored,
                [25; 32],
                lock_directory(directory.path()).unwrap(),
            )
            .unwrap(),
        );
        let archives: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("corregir-before-recovery-")
            })
            .collect();
        assert_eq!(archives.len(), 1);
        assert_eq!(fs::read(&archives[0]).unwrap(), original);
        let reopened = open_with_key(directory.path(), [25; 32]).unwrap();
        let value: String = reopened
            .conn
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT value FROM configuration WHERE key='recovery-proof'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "preserved");
        assert!(!directory.path().join("corregir.sqlite").exists());
    }
    #[test]
    fn reads_legacy_aes_010_snapshots_and_keeps_wire_format_compatible() {
        let directory = tempdir().unwrap();
        let state = open_with_key(directory.path(), KEY).unwrap();
        let plain = {
            let guard = state.conn.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO configuration VALUES ('migration-proof','preserved')",
                [],
            )
            .unwrap();
            conn.serialize(MAIN_DB).unwrap().to_vec()
        };
        let legacy_cipher =
            <aes_gcm_legacy::Aes256Gcm as aes_gcm_legacy::aead::KeyInit>::new_from_slice(&KEY)
                .unwrap();
        let nonce = [8u8; 12];
        let encrypted = aes_gcm_legacy::aead::Aead::encrypt(
            &legacy_cipher,
            aes_gcm_legacy::Nonce::from_slice(&nonce),
            plain.as_slice(),
        )
        .unwrap();
        let mut bytes = nonce.to_vec();
        bytes.extend_from_slice(&encrypted);
        fs::write(directory.path().join(ENCRYPTED_DB_NAME), bytes).unwrap();
        drop(state);
        let reopened = open_with_key(directory.path(), KEY).unwrap();
        let guard = reopened.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT value FROM configuration WHERE key='migration-proof'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "preserved"
        );
        let current = fs::read(directory.path().join(ENCRYPTED_DB_NAME)).unwrap();
        let legacy_plain = aes_gcm_legacy::aead::Aead::decrypt(
            &legacy_cipher,
            aes_gcm_legacy::Nonce::from_slice(&current[..12]),
            &current[12..],
        )
        .unwrap();
        assert!(legacy_plain.starts_with(b"SQLite format 3\0"));
    }
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
