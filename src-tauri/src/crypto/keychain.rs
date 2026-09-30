//! Passphrase for the encrypted local database. It is generated on first
//! startup and stored in the operating system's credential store.

use keyring::Entry;
use rand::RngExt;
use thiserror::Error;

const SERVICE: &str = "corregir-desktop";
const USERNAME: &str = "db-passphrase";

#[derive(Debug, Error)]
pub enum KeychainError {
    #[error("could not access the OS credential store: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("the existing encrypted database has no credential; restore its original credential before opening it")]
    MissingExistingCredential,
}

#[cfg(test)]
pub fn get_or_create_passphrase() -> Result<String, KeychainError> {
    load_passphrase(true)
}

pub fn load_passphrase(allow_create: bool) -> Result<String, KeychainError> {
    let entry = Entry::new(SERVICE, USERNAME)?;
    match entry.get_password() {
        Ok(passphrase) => Ok(passphrase),
        Err(keyring::Error::NoEntry) if !allow_create => {
            Err(KeychainError::MissingExistingCredential)
        }
        Err(keyring::Error::NoEntry) => {
            let passphrase = generate_passphrase();
            entry.set_password(&passphrase)?;
            Ok(passphrase)
        }
        Err(e) => Err(e.into()),
    }
}

fn generate_passphrase() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test intentionally touches the OS keychain. Other tests use
    /// `db::schema::open_with_key` to avoid shared credentials.
    #[test]
    #[ignore = "uses the shared OS credential store"]
    fn is_idempotent() {
        let p1 = get_or_create_passphrase().unwrap();
        let p2 = get_or_create_passphrase().unwrap();
        assert_eq!(p1, p2);
        assert_eq!(p1.len(), 64); // 32 bytes in hexadecimal
    }
}
