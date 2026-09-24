//! Resolución de directorios de recursos bundleados (modelos de OCR,
//! biblioteca de Pdfium). En desarrollo (`pnpm tauri dev`) se usa el árbol
//! de fuentes directamente; en la app empaquetada, el resource_dir que
//! gestiona Tauri (ver `bundle.resources` en tauri.conf.json).

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

fn resolver(app: &AppHandle, relativo: &str) -> PathBuf {
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(relativo);
    if dev.exists() {
        return dev;
    }
    app.path()
        .resource_dir()
        .map(|dir| dir.join(relativo))
        .unwrap_or(dev)
}

pub fn dir_pdfium(app: &AppHandle) -> PathBuf {
    resolver(app, "pdfium/bin")
}

pub fn dir_modelos_ocr(app: &AppHandle) -> PathBuf {
    resolver(app, "models/ocr")
}
