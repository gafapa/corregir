//! Portable password-protected snapshots, independent of the OS credential.
use super::DbState;
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngExt;
use rusqlite::{Connection, MAIN_DB};
use zeroize::Zeroizing;

pub const MAX_BACKUP_BYTES: usize = 128 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"CORRBA01";
const HEADER_BYTES: usize = 36;

fn derive(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, String> {
    if password.chars().count() < 12 || password.len() > 1024 {
        return Err(
            "use a backup password of at least 12 characters and at most 1024 bytes".into(),
        );
    }
    let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|error| error.to_string())?;
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| "could not derive the backup encryption key")?;
    Ok(key)
}

pub fn encrypt(conn: &Connection, password: &str) -> Result<Vec<u8>, String> {
    let serialized = conn.serialize(MAIN_DB).map_err(|error| error.to_string())?;
    if serialized.len() > MAX_BACKUP_BYTES {
        return Err("the workspace exceeds the 128 MiB portable backup limit".into());
    }
    let plain = Zeroizing::new(serialized.to_vec());
    let mut header = [0u8; HEADER_BYTES];
    header[..8].copy_from_slice(MAGIC);
    rand::rng().fill(&mut header[8..]);
    let key = derive(password, &header[8..24])?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|_| "could not initialize backup encryption")?;
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&header[24..36]),
            Payload {
                msg: &plain,
                aad: &header,
            },
        )
        .map_err(|_| "could not encrypt backup")?;
    let mut result = header.to_vec();
    result.extend_from_slice(&encrypted);
    Ok(result)
}

pub fn decrypt(bytes: &[u8], password: &str) -> Result<Connection, String> {
    if bytes.len() < HEADER_BYTES + 16
        || bytes.len() > MAX_BACKUP_BYTES + HEADER_BYTES + 16
        || &bytes[..8] != MAGIC
    {
        return Err("not a supported encrypted Corregir backup".into());
    }
    let key = derive(password, &bytes[8..24])?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|_| "could not initialize backup encryption")?;
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(&bytes[24..36]),
                Payload {
                    msg: &bytes[HEADER_BYTES..],
                    aad: &bytes[..HEADER_BYTES],
                },
            )
            .map_err(|_| "incorrect backup password or damaged backup")?,
    );
    if !plain.starts_with(b"SQLite format 3\0") {
        return Err("backup does not contain a SQLite workspace".into());
    }
    let mut conn = Connection::open_in_memory().map_err(|error| error.to_string())?;
    conn.deserialize_read_exact(MAIN_DB, plain.as_slice(), plain.len(), false)
        .map_err(|error| error.to_string())?;
    conn.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON; PRAGMA temp_store=MEMORY; PRAGMA journal_mode=MEMORY;").map_err(|error| error.to_string())?;
    let integrity: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|_| "invalid backup database")?;
    if integrity != "ok" {
        return Err("backup database integrity check failed".into());
    }
    let custom_objects: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type IN ('trigger','view')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if custom_objects != 0 {
        return Err("backup contains unsupported database objects".into());
    }
    for query in ["SELECT id,title,subject,grade_level,version,content_json FROM rubrics", "SELECT id,rubric_id,code,description,score_max,sort_order FROM criteria_rubric", "SELECT id,rubric_id,text,materials_ref FROM assignments", "SELECT id,assignment_id,text_ocr,text_redacted,status_pipeline,grade_revision,feedback_json,feedback_revision FROM submissions", "SELECT alias,student_name,submission_id FROM alias_student_map", "SELECT id,submission_id,criterion_id,score_final,ai_evidence_json,comment_teacher FROM results", "SELECT id,submission_id,event,actor,model_version,created_at FROM logs_audit", "SELECT key,value FROM configuration"] { conn.prepare(query).map_err(|_| "backup schema is not compatible with this application")?; }
    let foreign_key_error = conn
        .prepare("PRAGMA foreign_key_check")
        .map_err(|error| error.to_string())?
        .exists([])
        .map_err(|error| error.to_string())?;
    if foreign_key_error {
        return Err("backup has broken references".into());
    }
    super::super::commands::settings::load(&conn)?.validate()?;
    Ok(conn)
}

pub fn restore(db: &DbState, restored: Connection) -> Result<(), String> {
    let mut guard = db.conn.lock().map_err(|error| error.to_string())?;
    let current = guard.as_ref().ok_or("the database is closed")?;
    for table in [
        "rubrics",
        "criteria_rubric",
        "assignments",
        "submissions",
        "results",
        "alias_student_map",
        "logs_audit",
    ] {
        let count: i64 = current
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .map_err(|error| error.to_string())?;
        if count != 0 {
            return Err(
                "restore requires an empty workspace; existing work has not been replaced".into(),
            );
        }
    }
    db.persist(&restored).map_err(|error| error.to_string())?;
    *guard = Some(restored);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const PASSWORD: &str = "synthetic recovery password";
    #[test]
    fn restores_on_a_new_device_with_a_different_key_and_refuses_overwrite() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let source = super::super::schema::open_with_key(first.path(), [3; 32]).unwrap();
        let bytes = {
            let guard = source.conn.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute("INSERT INTO rubrics(title,subject,grade_level,content_json) VALUES ('Recovery test','Science','Year 8','{}')",[]).unwrap();
            encrypt(conn, PASSWORD).unwrap()
        };
        assert!(!bytes.windows(13).any(|value| value == b"Recovery test"));
        assert!(decrypt(&bytes, "incorrect password").is_err());
        let target = super::super::schema::open_with_key(second.path(), [4; 32]).unwrap();
        restore(&target, decrypt(&bytes, PASSWORD).unwrap()).unwrap();
        assert!(restore(&target, decrypt(&bytes, PASSWORD).unwrap()).is_err());
        drop(target);
        let reopened = super::super::schema::open_with_key(second.path(), [4; 32]).unwrap();
        let guard = reopened.conn.lock().unwrap();
        let title: String = guard
            .as_ref()
            .unwrap()
            .query_row("SELECT title FROM rubrics", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "Recovery test");
        assert!(!second.path().join("corregir.sqlite").exists());
    }
    #[test]
    fn rejects_tampering_truncation_and_short_passwords() {
        let dir = tempfile::tempdir().unwrap();
        let db = super::super::schema::open_with_key(dir.path(), [5; 32]).unwrap();
        let guard = db.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        assert!(encrypt(conn, "short").is_err());
        let mut bytes = encrypt(conn, PASSWORD).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(decrypt(&bytes, PASSWORD).is_err());
        assert!(decrypt(&bytes[..20], PASSWORD).is_err());
        bytes[0] ^= 1;
        assert!(decrypt(&bytes, PASSWORD).is_err());
    }
}
