//! Passphrase de cifrado de la base de datos local. Se genera una sola vez,
//! en el primer arranque, y se guarda en el almacén de credenciales del
//! sistema operativo (Windows Credential Manager / macOS Keychain / Secret
//! Service en Linux) — nunca en texto plano ni en un fichero de config.

use keyring::Entry;
use rand::RngCore;
use thiserror::Error;

const SERVICIO: &str = "corregir-desktop";
const USUARIO: &str = "db-passphrase";

#[derive(Debug, Error)]
pub enum KeychainError {
    #[error("error accediendo al almacén de credenciales del sistema: {0}")]
    Keyring(#[from] keyring::Error),
}

pub fn obtener_o_crear_passphrase() -> Result<String, KeychainError> {
    let entrada = Entry::new(SERVICIO, USUARIO)?;
    match entrada.get_password() {
        Ok(passphrase) => Ok(passphrase),
        Err(keyring::Error::NoEntry) => {
            let passphrase = generar_passphrase();
            entrada.set_password(&passphrase)?;
            Ok(passphrase)
        }
        Err(e) => Err(e.into()),
    }
}

fn generar_passphrase() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Única prueba de este módulo que toca el keychain real del sistema
    /// operativo (deliberado: es justo lo que hay que verificar). El resto
    /// de la suite usa `db::schema::abrir_con_clave` para no depender de él
    /// ni competir por esta misma entrada al correr en paralelo.
    #[test]
    fn es_idempotente() {
        let p1 = obtener_o_crear_passphrase().unwrap();
        let p2 = obtener_o_crear_passphrase().unwrap();
        assert_eq!(p1, p2);
        assert_eq!(p1.len(), 64); // 32 bytes en hexadecimal
    }
}
