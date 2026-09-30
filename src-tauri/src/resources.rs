//! Resolve bundled OCR models, Pdfium, and fonts in development and packaged builds.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

fn resolve(app: &AppHandle, relative: &str) -> PathBuf {
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join(relative);
    if cfg!(debug_assertions) && dev.exists() {
        return dev;
    }
    app.path()
        .resource_dir()
        .map(|directory| directory.join(relative))
        .unwrap_or(dev)
}

pub fn directory_pdfium(app: &AppHandle) -> PathBuf {
    resolve(app, "pdfium/bin")
}

pub fn directory_models_ocr(app: &AppHandle) -> PathBuf {
    resolve(app, "models/ocr")
}

pub fn path_source_pdf(app: &AppHandle) -> PathBuf {
    resolve(app, "fonts").join("Roboto-Regular.ttf")
}
