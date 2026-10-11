//! Explicit portable work directories with a password-wrapped random database key.
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroizing;

const KEY_FILE: &str = "corregir.workspace-key";
const MAGIC: &[u8; 8] = b"CORRKEY1";
const KEY_BYTES: usize = 84;
pub const LOCATION_FILE: &str = "corregir-portable.json";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Location {
    workspace: PathBuf,
}

pub struct WorkspaceState {
    pub portable: bool,
    location: Option<PathBuf>,
    directory: Mutex<PathBuf>,
    opening: AtomicBool,
}
#[derive(Serialize)]
pub struct WorkspaceStatus {
    portable: bool,
    directory: String,
    opened: bool,
}
#[derive(Serialize)]
pub struct DirectoryStatus {
    initialized: bool,
    windows_bound: bool,
}

impl WorkspaceState {
    pub fn resolve(executable_directory: &Path, normal_directory: PathBuf) -> Result<Self, String> {
        let location = executable_directory.join(LOCATION_FILE);
        let (portable, directory) = if location.exists() {
            if fs::metadata(&location)
                .map_err(|error| error.to_string())?
                .len()
                > 4096
            {
                return Err("portable location file is too large".into());
            }
            let config: Location =
                serde_json::from_slice(&fs::read(&location).map_err(|error| error.to_string())?)
                    .map_err(|_| "invalid portable location file")?;
            if config.workspace.as_os_str().is_empty() {
                return Err("portable work directory is empty".into());
            }
            let directory = if config.workspace.is_absolute() {
                config.workspace
            } else {
                executable_directory.join(config.workspace)
            };
            (true, directory)
        } else {
            (false, normal_directory)
        };
        Ok(Self {
            portable,
            location: portable.then_some(location),
            directory: Mutex::new(directory),
            opening: AtomicBool::new(false),
        })
    }
    pub fn directory(&self) -> Result<PathBuf, String> {
        self.directory
            .lock()
            .map(|path| path.clone())
            .map_err(|error| error.to_string())
    }
    fn remember(&self, directory: &Path) -> Result<(), String> {
        let location = self
            .location
            .as_ref()
            .ok_or("portable work-directory selection is unavailable")?;
        let parent = location
            .parent()
            .ok_or("portable application directory is missing")?;
        let parent = parent.canonicalize().map_err(|error| error.to_string())?;
        let directory = directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let workspace = directory
            .strip_prefix(&parent)
            .unwrap_or(&directory)
            .to_path_buf();
        let bytes = serde_json::to_vec_pretty(&Location { workspace })
            .map_err(|error| error.to_string())?;
        let mut file =
            tempfile::NamedTempFile::new_in(&parent).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        file.persist(location).map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn password_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, String> {
    if password.chars().count() < 12 || password.len() > 1024 {
        return Err(
            "use a work-directory password of at least 12 characters and at most 1024 bytes".into(),
        );
    }
    let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|error| error.to_string())?;
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| "could not derive the work-directory key")?;
    Ok(key)
}
fn wrap_key(key: &[u8; 32], password: &str) -> Result<Vec<u8>, String> {
    let mut header = [0u8; 36];
    header[..8].copy_from_slice(MAGIC);
    rand::rng().fill(&mut header[8..]);
    let wrapping = password_key(password, &header[8..24])?;
    let cipher =
        Aes256Gcm::new_from_slice(wrapping.as_ref()).map_err(|_| "invalid wrapping key")?;
    let nonce = Nonce::try_from(&header[24..]).map_err(|_| "invalid key nonce")?;
    let encrypted = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: key,
                aad: &header,
            },
        )
        .map_err(|_| "could not protect the work-directory key")?;
    let mut bytes = header.to_vec();
    bytes.extend_from_slice(&encrypted);
    Ok(bytes)
}
fn unwrap_key(bytes: &[u8], password: &str) -> Result<Zeroizing<[u8; 32]>, String> {
    if bytes.len() != KEY_BYTES || &bytes[..8] != MAGIC {
        return Err("invalid work-directory key file; preserve it and restore a verified backup into a new directory".into());
    }
    let wrapping = password_key(password, &bytes[8..24])?;
    let cipher =
        Aes256Gcm::new_from_slice(wrapping.as_ref()).map_err(|_| "invalid wrapping key")?;
    let nonce = Nonce::try_from(&bytes[24..36]).map_err(|_| "invalid key nonce")?;
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &bytes[36..],
                    aad: &bytes[..36],
                },
            )
            .map_err(|_| "incorrect work-directory password or damaged key file")?,
    );
    Ok(Zeroizing::new(
        plain
            .as_slice()
            .try_into()
            .map_err(|_| "invalid database key length")?,
    ))
}

fn inspect(directory: &Path) -> Result<DirectoryStatus, String> {
    if !directory.is_absolute() {
        return Err("select an absolute work-directory path".into());
    }
    if directory.exists() && !directory.is_dir() {
        return Err("select a directory, not a file".into());
    }
    let initialized = directory.join(KEY_FILE).exists();
    let windows_bound = (!initialized && directory.join("corregir.sqlite.enc").exists())
        || [
            "corregir.sqlite",
            "corregir.sqlite-wal",
            "corregir.sqlite-shm",
            "corregir.sqlite-journal",
        ]
        .iter()
        .any(|file| directory.join(file).exists());
    Ok(DirectoryStatus {
        initialized,
        windows_bound,
    })
}

fn open_portable(directory: &Path, password: &str) -> Result<crate::db::DbState, String> {
    if inspect(directory)?.windows_bound {
        return Err("this directory contains a Windows-bound workspace; export a password-protected backup and restore it into a new work directory".into());
    }
    let lock = crate::db::schema::lock_directory(directory).map_err(|error| error.to_string())?;
    // Recheck under the same lock used by normal database writers.
    if inspect(directory)?.windows_bound {
        return Err("the directory already contains a Windows-bound workspace".into());
    }
    let key_path = directory.join(KEY_FILE);
    let key = if key_path.exists() {
        if fs::metadata(&key_path)
            .map_err(|error| error.to_string())?
            .len()
            != KEY_BYTES as u64
        {
            return Err("invalid work-directory key file size".into());
        }
        unwrap_key(
            &fs::read(&key_path).map_err(|error| error.to_string())?,
            password,
        )?
    } else {
        let mut key = Zeroizing::new([0; 32]);
        rand::rng().fill(key.as_mut());
        let bytes = wrap_key(&key, password)?;
        let mut file =
            tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        file.persist_noclobber(&key_path)
            .map_err(|error| error.to_string())?;
        key
    };
    crate::db::schema::open_locked(directory, *key, lock).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn cmd_workspace_status(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
) -> Result<WorkspaceStatus, String> {
    Ok(WorkspaceStatus {
        portable: state.portable,
        directory: state.directory()?.to_string_lossy().into_owned(),
        opened: app
            .try_state::<std::sync::Arc<crate::db::DbState>>()
            .is_some(),
    })
}
#[tauri::command]
pub fn cmd_inspect_work_directory(directory: String) -> Result<DirectoryStatus, String> {
    inspect(Path::new(&directory))
}
#[tauri::command]
pub async fn cmd_open_work_directory(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
    directory: String,
    password: String,
) -> Result<(), String> {
    if !state.portable
        || app
            .try_state::<std::sync::Arc<crate::db::DbState>>()
            .is_some()
    {
        return Err("close the application before choosing another work directory".into());
    }
    if state
        .opening
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("a work directory is already opening".into());
    }
    struct OpeningGuard<'a>(&'a AtomicBool);
    impl Drop for OpeningGuard<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _guard = OpeningGuard(&state.opening);
    let directory = PathBuf::from(directory);
    let password = Zeroizing::new(password);
    let db = crate::tasks::run_blocking(move || open_portable(&directory, &password)).await?;
    let directory = db
        .directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    state.remember(&directory)?;
    if !app.manage(std::sync::Arc::new(db)) {
        return Err(
            "another operation already opened the workspace; restart the application".into(),
        );
    }
    *state.directory.lock().map_err(|error| error.to_string())? = directory;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const PASSWORD: &str = "synthetic portable password";
    #[test]
    fn directory_moves_without_host_credentials_and_wrong_password_never_changes_data() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let db = open_portable(first.path(), PASSWORD).unwrap();
        db.write(|conn| { conn.execute("INSERT INTO rubrics(title,subject,grade_level,content_json) VALUES ('Portable fixture','Science','Year 8','{}')", []).map_err(|error| error.to_string())?; Ok(()) }).unwrap();
        assert!(open_portable(first.path(), PASSWORD)
            .err()
            .unwrap()
            .contains("another instance"));
        drop(db);
        for file in [KEY_FILE, "corregir.sqlite.enc"] {
            fs::copy(first.path().join(file), second.path().join(file)).unwrap();
        }
        let before = fs::read(second.path().join("corregir.sqlite.enc")).unwrap();
        assert!(open_portable(second.path(), "incorrect password").is_err());
        assert_eq!(
            before,
            fs::read(second.path().join("corregir.sqlite.enc")).unwrap()
        );
        assert!(!before.windows(16).any(|bytes| bytes == b"Portable fixture"));
        let moved = open_portable(second.path(), PASSWORD).unwrap();
        let guard = moved.conn.lock().unwrap();
        assert_eq!(
            guard
                .as_ref()
                .unwrap()
                .query_row("SELECT title FROM rubrics", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "Portable fixture"
        );
        assert!(!second.path().join("corregir.sqlite").exists());
    }
    #[test]
    fn rejects_tampered_keys_and_windows_workspaces_without_replacing_files() {
        let directory = tempfile::tempdir().unwrap();
        let db = crate::db::schema::open_with_key(directory.path(), [9; 32]).unwrap();
        drop(db);
        let before = fs::read(directory.path().join("corregir.sqlite.enc")).unwrap();
        assert!(open_portable(directory.path(), PASSWORD).is_err());
        assert_eq!(
            before,
            fs::read(directory.path().join("corregir.sqlite.enc")).unwrap()
        );
        assert!(!directory.path().join(KEY_FILE).exists());
        let mut wrapped = wrap_key(&[3; 32], PASSWORD).unwrap();
        wrapped[40] ^= 1;
        assert!(unwrap_key(&wrapped, PASSWORD).is_err());
        assert!(wrap_key(&[3; 32], "short").is_err());
    }
    #[test]
    fn relative_locator_follows_the_application_and_invalid_locator_fails_closed() {
        let first = tempfile::tempdir().unwrap();
        let normal = tempfile::tempdir().unwrap();
        fs::write(first.path().join(LOCATION_FILE), br#"{"workspace":"work"}"#).unwrap();
        let state = WorkspaceState::resolve(first.path(), normal.path().into()).unwrap();
        assert!(state.portable);
        assert_eq!(state.directory().unwrap(), first.path().join("work"));
        fs::create_dir(first.path().join("work")).unwrap();
        state.remember(&first.path().join("work")).unwrap();
        let location: Location =
            serde_json::from_slice(&fs::read(first.path().join(LOCATION_FILE)).unwrap()).unwrap();
        assert_eq!(location.workspace, Path::new("work"));
        fs::write(first.path().join(LOCATION_FILE), b"bad JSON").unwrap();
        assert!(WorkspaceState::resolve(first.path(), normal.path().into()).is_err());
    }
}
