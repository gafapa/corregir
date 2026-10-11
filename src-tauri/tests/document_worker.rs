use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use tauri_app_lib::document_worker::{extract_with_executable, WorkerRequest};

fn request(path: PathBuf) -> WorkerRequest {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    WorkerRequest {
        file_path: path,
        pdfium_directory: root.join("resources/pdfium/bin"),
        models_directory: root.join("resources/models/ocr"),
        force_ocr: false,
        preview_page: None,
        source_handle: None,
        #[cfg(debug_assertions)]
        sandbox_probe: None,
    }
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("synthetic-data")
        .join(name)
}

#[test]
fn actual_worker_extracts_native_and_mixed_pages_without_opening_a_workspace() {
    let executable = Path::new(env!("CARGO_BIN_EXE_tauri-app"));
    let cancelled = AtomicBool::new(false);
    let native = extract_with_executable(
        executable,
        request(fixture("language-student-1.pdf")),
        &cancelled,
        |_, _| {},
    )
    .unwrap();
    assert!(!native.used_ocr);
    assert!(native.texts.join(" ").contains("nostalgia"));
    let mixed = extract_with_executable(
        executable,
        request(fixture("language-student-2-mixed.pdf")),
        &cancelled,
        |_, _| {},
    )
    .unwrap();
    assert!(mixed.used_ocr);
    let text = mixed.texts.join(" ").to_lowercase();
    assert!(
        text.contains("poem") || text.contains("perfect"),
        "the scanned answer body must be included"
    );
}

#[test]
fn cancelling_an_actual_ocr_worker_stops_before_any_result_is_saved() {
    let cancelled = AtomicBool::new(false);
    let mut cancelled_at = None;
    let result = extract_with_executable(
        Path::new(env!("CARGO_BIN_EXE_tauri-app")),
        request(fixture("language-student-2-scanned.pdf")),
        &cancelled,
        |_, _| {
            cancelled_at = Some(Instant::now());
            cancelled.store(true, Ordering::Relaxed);
        },
    );
    assert_eq!(result.unwrap_err(), "import cancelled");
    assert!(
        cancelled_at
            .expect("worker reported extraction progress")
            .elapsed()
            .as_secs()
            < 15
    );
}

#[test]
fn a_malformed_document_fails_without_crashing_the_host() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("damaged.pdf");
    std::fs::write(&path, b"invalid PDF fixture").unwrap();
    let result = extract_with_executable(
        Path::new(env!("CARGO_BIN_EXE_tauri-app")),
        request(path),
        &AtomicBool::new(false),
        |_, _| {},
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
#[test]
fn actual_worker_returns_a_bounded_page_preview_and_rejects_invalid_pages() {
    let executable = Path::new(env!("CARGO_BIN_EXE_tauri-app"));
    let mut preview_request = request(fixture("language-student-1.pdf"));
    preview_request.preview_page = Some(1);
    let result = extract_with_executable(
        executable,
        preview_request,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert!(result.texts.is_empty());
    let preview = result.preview.unwrap();
    assert_eq!(preview.page, 1);
    assert_eq!(preview.total, 1);
    assert!(preview.data_url.starts_with("data:image/png;base64,iVBOR"));
    assert!(preview.data_url.len() < 6 * 1024 * 1024);
    let mut invalid = request(fixture("language-student-1.pdf"));
    invalid.preview_page = Some(2);
    assert!(
        extract_with_executable(executable, invalid, &AtomicBool::new(false), |_, _| {}).is_err()
    );
}

#[cfg(windows)]
#[test]
fn restricted_worker_cannot_read_other_files_reopen_the_source_or_connect_to_loopback() {
    let directory = tempfile::tempdir().unwrap();
    let private = directory.path().join("private-synthetic.txt");
    std::fs::write(&private, b"private synthetic sentinel").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut probe = request(fixture("language-student-1.pdf"));
    probe.sandbox_probe = Some(tauri_app_lib::document_worker::SandboxProbe {
        private_path: private.clone(),
        address: listener.local_addr().unwrap(),
    });
    let result = extract_with_executable(
        Path::new(env!("CARGO_BIN_EXE_tauri-app")),
        probe,
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let evidence: serde_json::Value = serde_json::from_str(&result.texts[0]).unwrap();
    assert_eq!(evidence["file_denied"], true);
    assert_eq!(evidence["source_reopen_denied"], true);
    assert_eq!(evidence["network_denied"], true);
    assert_eq!(
        std::fs::read(private).unwrap(),
        b"private synthetic sentinel"
    );
}
