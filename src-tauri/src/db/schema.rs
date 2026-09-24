//! Base de datos local. **Nota de diseño (Hito 2):** el plan original preveía
//! SQLCipher (cifrado continuo a nivel de página). Se descartó en esta
//! implementación porque `bundled-sqlcipher-vendored-openssl` requiere
//! compilar OpenSSL desde fuente, lo que exige un Perl completo (con
//! `Locale::Maketext::Simple`) no disponible con el Perl mínimo de Git Bash
//! en Windows. Exigir instalar Strawberry Perl solo para compilar la app
//! introduce justo el tipo de dependencia frágil que este proyecto quiere
//! evitar (ver ARQUITECTURA.md).
//!
//! En su lugar: SQLite normal (`rusqlite` con feature `bundled`, sin
//! OpenSSL) + **cifrado de sobre a nivel de aplicación** con AES-256-GCM.
//! El fichero en reposo (`corregir.sqlite.enc`) está siempre cifrado; al
//! abrir la app se descifra a una copia de trabajo en claro
//! (`corregir.sqlite`) en el mismo directorio, y se vuelve a cifrar
//! (`sellar`) tras cada operación que escribe datos y al cerrar la
//! aplicación.
//!
//! Riesgo residual documentado (pendiente antes de Fase C, ver
//! docs/DPIA-EIPD.md): `sellar` vuelve a cifrar sobre `.enc`, pero no borra
//! ni sobrescribe la copia en claro (`corregir.sqlite`) -- no se puede hacer
//! de forma segura mientras la conexion de rusqlite sigue teniendo el
//! fichero abierto (en Windows ni siquiera se puede borrar). Consecuencia:
//! la copia en claro queda en disco no solo si el proceso termina de forma
//! abrupta, sino tambien tras un cierre normal de la app, hasta el
//! siguiente arranque (que la sobrescribe al descifrar de nuevo).
//! Arreglarlo de raiz exige poder cerrar la conexion explicitamente antes
//! de salir (cambiar DbState.conn a Mutex<Option<Connection>> y anadir un
//! cierre explicito en el on_window_event), lo que afecta a todos los
//! comandos que usan db.conn.lock(). Se pospone deliberadamente: la Fase A
//! solo usa datos sinteticos, asi que no hay nada sensible que proteger
//! todavia, pero es un bloqueante real antes de procesar datos de alumnos
//! reales (Fase C).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use rusqlite::Connection;
use thiserror::Error;

use crate::crypto::keychain::{self, KeychainError};

const MIGRACION_INICIAL: &str = include_str!("migrations/0001_init.sql");
const NOMBRE_BD_PLANA: &str = "corregir.sqlite";
const NOMBRE_BD_CIFRADA: &str = "corregir.sqlite.enc";

#[derive(Debug, Error)]
pub enum DbError {
    #[error("error de credenciales: {0}")]
    Keychain(#[from] KeychainError),
    #[error("error de base de datos: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("error de E/S en el fichero de base de datos: {0}")]
    Io(#[from] std::io::Error),
    #[error("passphrase con formato inesperado (no es hexadecimal de 32 bytes)")]
    PassphraseInvalida,
    #[error("no se pudo descifrar la base de datos (clave incorrecta o fichero corrupto)")]
    DescifradoFallido,
    #[error("mutex de la conexión envenenado (un hilo anterior entró en pánico mientras la tenía bloqueada)")]
    MutexEnvenenado,
}

/// Estado gestionado por Tauri: conexión + lo necesario para volver a sellar
/// (cifrar) la base de datos tras cada escritura.
pub struct DbState {
    pub conn: Mutex<Connection>,
    ruta_plana: PathBuf,
    ruta_cifrada: PathBuf,
    clave: [u8; 32],
}

impl DbState {
    /// Vuelve a cifrar el fichero en claro sobre el fichero `.enc`. Debe
    /// llamarse tras cualquier comando que escriba datos, y al cerrar la app.
    pub fn sellar(&self) -> Result<(), DbError> {
        let _guard = self.conn.lock().map_err(|_| DbError::MutexEnvenenado)?;
        cifrar_a_disco(&self.ruta_plana, &self.ruta_cifrada, &self.clave)
    }
}

fn derivar_clave(passphrase_hex: &str) -> Result<[u8; 32], DbError> {
    let bytes = hex::decode(passphrase_hex).map_err(|_| DbError::PassphraseInvalida)?;
    bytes.try_into().map_err(|_| DbError::PassphraseInvalida)
}

fn descifrar_a_disco(ruta_cifrada: &Path, ruta_plana: &Path, clave: &[u8; 32]) -> Result<(), DbError> {
    let contenido = fs::read(ruta_cifrada)?;
    if contenido.len() < 12 {
        return Err(DbError::DescifradoFallido);
    }
    let (nonce_bytes, texto_cifrado) = contenido.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(clave));
    let nonce = Nonce::from_slice(nonce_bytes);
    let texto_plano = cipher
        .decrypt(nonce, texto_cifrado)
        .map_err(|_| DbError::DescifradoFallido)?;
    fs::write(ruta_plana, texto_plano)?;
    Ok(())
}

fn cifrar_a_disco(ruta_plana: &Path, ruta_cifrada: &Path, clave: &[u8; 32]) -> Result<(), DbError> {
    let texto_plano = fs::read(ruta_plana)?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(clave));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let texto_cifrado = cipher
        .encrypt(&nonce, texto_plano.as_ref())
        .map_err(|_| DbError::DescifradoFallido)?;

    let mut salida = Vec::with_capacity(12 + texto_cifrado.len());
    salida.extend_from_slice(&nonce);
    salida.extend_from_slice(&texto_cifrado);

    // Escritura atómica: nunca dejar el `.enc` a medio escribir.
    let ruta_temporal = ruta_cifrada.with_extension("enc.tmp");
    fs::write(&ruta_temporal, salida)?;
    fs::rename(&ruta_temporal, ruta_cifrada)?;
    Ok(())
}

/// Abre (o crea) la base de datos en `dir`, obteniendo la clave del keychain
/// del sistema operativo. Punto de entrada real de la aplicación.
pub fn abrir(dir: &Path) -> Result<DbState, DbError> {
    let passphrase_hex = keychain::obtener_o_crear_passphrase()?;
    let clave = derivar_clave(&passphrase_hex)?;
    abrir_con_clave(dir, clave)
}

/// Igual que `abrir`, pero recibe la clave directamente en vez de leerla del
/// keychain del sistema. Separado para que los tests no dependan del
/// almacén de credenciales real del sistema operativo (evita tanto una
/// fuente de "flakiness" por tests en paralelo compitiendo por la misma
/// entrada, como escribir credenciales de prueba en el keychain real).
pub fn abrir_con_clave(dir: &Path, clave: [u8; 32]) -> Result<DbState, DbError> {
    let ruta_plana = dir.join(NOMBRE_BD_PLANA);
    let ruta_cifrada = dir.join(NOMBRE_BD_CIFRADA);

    if ruta_cifrada.exists() {
        descifrar_a_disco(&ruta_cifrada, &ruta_plana, &clave)?;
    }

    let conn = Connection::open(&ruta_plana)?;
    conn.execute_batch(MIGRACION_INICIAL)?;

    let estado = DbState {
        conn: Mutex::new(conn),
        ruta_plana,
        ruta_cifrada,
        clave,
    };
    // Sella de inmediato para que el artefacto cifrado exista desde el primer
    // arranque, no solo tras la primera escritura de datos de usuario.
    estado.sellar()?;
    Ok(estado)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const CLAVE_DE_PRUEBA: [u8; 32] = [42u8; 32];

    #[test]
    fn crea_sella_y_reabre_bd() {
        let dir = tempdir().unwrap();

        {
            let estado = abrir_con_clave(dir.path(), CLAVE_DE_PRUEBA).unwrap();
            {
                let conn = estado.conn.lock().unwrap();
                conn.execute(
                    "INSERT INTO configuracion (clave, valor) VALUES ('test', 'valor')",
                    [],
                )
                .unwrap();
            }
            estado.sellar().unwrap();
            // Tras sellar, no debe quedar rastro legible del dato en el
            // fichero cifrado (comprobación mínima de que no es texto plano).
            let cifrado = fs::read(dir.path().join(NOMBRE_BD_CIFRADA)).unwrap();
            let como_texto = String::from_utf8_lossy(&cifrado);
            assert!(!como_texto.contains("valor"));
        }

        // Reabrir en un DbState nuevo (simula reiniciar la app): el dato debe
        // seguir ahí porque se descifra desde el `.enc`.
        let estado2 = abrir_con_clave(dir.path(), CLAVE_DE_PRUEBA).unwrap();
        let conn2 = estado2.conn.lock().unwrap();
        let valor: String = conn2
            .query_row(
                "SELECT valor FROM configuracion WHERE clave = 'test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(valor, "valor");
    }

    #[test]
    fn falla_con_clave_incorrecta() {
        let dir = tempdir().unwrap();
        let estado = abrir_con_clave(dir.path(), CLAVE_DE_PRUEBA).unwrap();
        drop(estado);

        let ruta_cifrada = dir.path().join(NOMBRE_BD_CIFRADA);
        let ruta_plana = dir.path().join(NOMBRE_BD_PLANA);
        let clave_incorrecta = [7u8; 32];
        let resultado = descifrar_a_disco(&ruta_cifrada, &ruta_plana, &clave_incorrecta);
        assert!(matches!(resultado, Err(DbError::DescifradoFallido)));
    }
}
