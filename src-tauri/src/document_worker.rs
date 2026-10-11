//! Supervised document parsing in a child process with bounded private pipe output.
use crate::pipeline::{docx, ocr_engine::OcrEngine, render};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const MAX_MESSAGE_BYTES: u64 = (render::MAX_TEXT_BYTES * 6 + 4096) as u64;
const WORKER_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Serialize, Deserialize)]
pub struct WorkerRequest {
    pub file_path: PathBuf,
    pub pdfium_directory: PathBuf,
    pub models_directory: PathBuf,
    pub force_ocr: bool,
    #[serde(default)]
    pub preview_page: Option<u16>,
    #[serde(default)]
    pub source_handle: Option<u64>,
    #[cfg(debug_assertions)]
    #[serde(default)]
    pub sandbox_probe: Option<SandboxProbe>,
}

#[cfg(debug_assertions)]
#[derive(Serialize, Deserialize)]
pub struct SandboxProbe {
    pub private_path: PathBuf,
    pub address: std::net::SocketAddr,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkerMessage {
    Progress {
        page: u16,
        total: u16,
    },
    Complete {
        texts: Vec<String>,
        used_ocr: bool,
        preview: Option<PagePreview>,
        source_hash: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PagePreview {
    pub data_url: String,
    pub page: u16,
    pub total: u16,
}

#[derive(Debug)]
pub struct WorkerResult {
    pub texts: Vec<String>,
    pub used_ocr: bool,
    pub preview: Option<PagePreview>,
    pub source_hash: String,
}

fn send(message: &WorkerMessage) -> Result<(), String> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, message).map_err(|error| error.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|_| output.flush())
        .map_err(|error| error.to_string())
}

fn read_source(request: &WorkerRequest) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    let mut file = if let Some(handle) = request.source_handle {
        use std::os::windows::io::FromRawHandle;
        // SAFETY: the supervising launcher supplies its whitelisted read-only inherited handle.
        unsafe { std::fs::File::from_raw_handle(handle as usize as _) }
    } else {
        std::fs::File::open(&request.file_path).map_err(|error| error.to_string())?
    };
    #[cfg(not(windows))]
    let mut file = std::fs::File::open(&request.file_path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > render::MAX_FILE_BYTES {
        return Err("document exceeds its file limit".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(render::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > render::MAX_FILE_BYTES {
        return Err("document exceeds its file limit".into());
    }
    Ok(bytes)
}

fn process(request: WorkerRequest) -> Result<WorkerResult, String> {
    if !request.file_path.is_absolute()
        || !request.pdfium_directory.is_absolute()
        || !request.models_directory.is_absolute()
    {
        return Err("document worker requires absolute paths".into());
    }
    #[cfg(all(windows, debug_assertions))]
    if let Some(probe) = &request.sandbox_probe {
        let file_denied = std::fs::read(&probe.private_path).is_err();
        let reopen_denied = std::fs::read(&request.file_path).is_err();
        let network_denied =
            std::net::TcpStream::connect_timeout(&probe.address, Duration::from_secs(2)).is_err();
        return Ok(WorkerResult { texts: vec![serde_json::json!({"file_denied":file_denied,"source_reopen_denied":reopen_denied,"network_denied":network_denied}).to_string()], used_ocr:false, preview:None, source_hash:String::new() });
    }
    let source = read_source(&request)?;
    use sha2::Digest;
    let source_hash = hex::encode(sha2::Sha256::digest(&source));
    let extension = request
        .file_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if let Some(page) = request.preview_page {
        use base64::Engine;
        let (image, total) =
            render::preview_bytes(&request.pdfium_directory, &source, &extension, page)
                .map_err(|error| error.to_string())?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        if png.get_ref().len() > 4 * 1024 * 1024 {
            return Err("page preview exceeds its limit".into());
        }
        let data_url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png.into_inner())
        );
        return Ok(WorkerResult {
            texts: Vec::new(),
            used_ocr: false,
            source_hash,
            preview: Some(PagePreview {
                data_url,
                page,
                total,
            }),
        });
    }
    let mut engine = None;
    let mut texts = Vec::new();
    let mut used_ocr = false;
    let mut bytes = 0usize;
    let mut consume = |page, index, total| -> Result<(), render::RenderError> {
        send(&WorkerMessage::Progress { page: index, total })
            .map_err(render::RenderError::Processing)?;
        let text = match page {
            render::PageContent::NativeText(text) => text,
            render::PageContent::ImageForOcr(image) => {
                used_ocr = true;
                if engine.is_none() {
                    engine = Some(
                        OcrEngine::load(&request.models_directory)
                            .map_err(|error| render::RenderError::Processing(error.to_string()))?,
                    );
                }
                engine
                    .as_ref()
                    .unwrap()
                    .recognize_image(&image)
                    .map_err(|error| render::RenderError::Processing(error.to_string()))?
            }
        };
        bytes = bytes.saturating_add(text.len() + 2);
        if bytes > render::MAX_TEXT_BYTES {
            return Err(render::RenderError::LimitExceeded);
        }
        texts.push(text);
        Ok(())
    };
    match extension.as_str() {
        "pdf" => render::visit_pdf_bytes(
            &request.pdfium_directory,
            &source,
            request.force_ocr,
            &mut consume,
        )
        .map_err(|error| error.to_string())?,
        "docx" => consume(
            render::PageContent::NativeText(
                docx::extract_reader(std::io::Cursor::new(&source))
                    .map_err(|error| error.to_string())?,
            ),
            1,
            1,
        )
        .map_err(|error| error.to_string())?,
        "png" | "jpg" | "jpeg" => consume(
            render::PageContent::ImageForOcr(
                render::load_image_bytes(&source).map_err(|error| error.to_string())?,
            ),
            1,
            1,
        )
        .map_err(|error| error.to_string())?,
        _ => return Err("supported import formats are PDF, text-only DOCX, PNG, and JPEG".into()),
    }
    Ok(WorkerResult {
        texts,
        used_ocr,
        preview: None,
        source_hash,
    })
}

/// Called before Tauri startup: this path never opens a workspace or credentials.
pub fn run() -> i32 {
    let result = (|| {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 65536 {
            return Err("document worker request exceeds its limit".into());
        }
        let request = serde_json::from_slice(&bytes)
            .map_err(|_| "invalid document worker request".to_string())?;
        process(request)
    })();
    match result {
        Ok(result) => {
            if send(&WorkerMessage::Complete {
                texts: result.texts,
                used_ocr: result.used_ocr,
                preview: result.preview,
                source_hash: result.source_hash,
            })
            .is_ok()
            {
                0
            } else {
                1
            }
        }
        Err(message) => {
            let _ = send(&WorkerMessage::Error { message });
            1
        }
    }
}

#[cfg(windows)]
type WorkerChild = crate::document_sandbox::SandboxedChild;
#[cfg(not(windows))]
type WorkerChild = std::process::Child;
struct SupervisedChild(WorkerChild);
#[cfg(windows)]
pub(crate) struct WorkerJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Drop for WorkerJob {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the valid, non-inherited job handle.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(windows)]
impl WorkerJob {
    pub(crate) fn attach_handle(
        process: windows_sys::Win32::Foundation::HANDLE,
    ) -> Result<Self, String> {
        use windows_sys::Win32::System::JobObjects::*;
        // SAFETY: null attributes request a non-inherited, unnamed job handle.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err("could not create the document worker job".into());
        }
        let job = Self(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        limits.ProcessMemoryLimit = 1024 * 1024 * 1024;
        // SAFETY: the information pointer and size match the selected class; both handles remain alive.
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if configured == 0 || unsafe { AssignProcessToJobObject(handle, process) } == 0 {
            return Err("could not apply document worker process limits".into());
        }
        Ok(job)
    }
}
impl Drop for SupervisedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn extract(
    request: WorkerRequest,
    cancelled: &AtomicBool,
    progress: impl FnMut(u16, u16),
) -> Result<WorkerResult, String> {
    extract_with_executable(
        &std::env::current_exe().map_err(|error| error.to_string())?,
        request,
        cancelled,
        progress,
    )
}

/// Native integration tests supply the actual application executable here.
pub fn extract_with_executable(
    executable: &std::path::Path,
    mut request: WorkerRequest,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u16, u16),
) -> Result<WorkerResult, String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("import cancelled".into());
    }
    let expected_preview = request.preview_page;
    #[cfg(windows)]
    let worker = WorkerChild::spawn(executable, &mut request, cancelled)?;
    #[cfg(not(windows))]
    let worker = {
        use std::process::{Command, Stdio};
        Command::new(executable)
            .arg("--document-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env_clear()
            .spawn()
            .map_err(|error| error.to_string())?
    };
    let mut child = SupervisedChild(worker);
    {
        let mut input = child
            .0
            .stdin
            .take()
            .ok_or("document worker input is unavailable")?;
        serde_json::to_writer(&mut input, &request).map_err(|error| error.to_string())?;
    }
    let stdout = child
        .0
        .stdout
        .take()
        .ok_or("document worker output is unavailable")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            let result = (&mut reader)
                .take(MAX_MESSAGE_BYTES + 1)
                .read_until(b'\n', &mut line);
            if matches!(result, Ok(0)) {
                break;
            }
            let message = if result.is_err() || line.len() as u64 > MAX_MESSAGE_BYTES {
                Err("document worker returned excessive or unreadable output".to_string())
            } else {
                serde_json::from_slice::<WorkerMessage>(&line)
                    .map_err(|_| "document worker returned invalid output".to_string())
            };
            let terminal = !matches!(&message, Ok(WorkerMessage::Progress { .. }));
            if sender.send(message).is_err() || terminal {
                break;
            }
        }
    });
    let started = Instant::now();
    let result = (|| {
        let mut messages = 0;
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err("import cancelled".into());
            }
            if started.elapsed() >= WORKER_TIMEOUT {
                return Err("document worker exceeded the ten-minute limit".into());
            }
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(message) => {
                    messages += 1;
                    if messages > render::MAX_PAGES as usize + 1 {
                        return Err("document worker returned too many messages".into());
                    }
                    match message? {
                        WorkerMessage::Progress { page, total }
                            if total > 0
                                && total <= render::MAX_PAGES
                                && page > 0
                                && page <= total =>
                        {
                            progress(page, total)
                        }
                        WorkerMessage::Progress { .. } => {
                            return Err("document worker returned invalid progress".into())
                        }
                        WorkerMessage::Error { message } => return Err(message),
                        WorkerMessage::Complete {
                            texts,
                            used_ocr,
                            preview,
                            source_hash,
                        } => {
                            if !source_hash.is_empty()
                                && (source_hash.len() != 64
                                    || !source_hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
                            {
                                return Err("invalid document worker fingerprint".into());
                            }
                            if let Some(ref image) = preview {
                                if expected_preview != Some(image.page)
                                    || image.total == 0
                                    || image.total > render::MAX_PAGES
                                    || image.page > image.total
                                    || !image.data_url.starts_with("data:image/png;base64,")
                                    || image.data_url.len() > 6 * 1024 * 1024
                                    || !texts.is_empty()
                                {
                                    return Err(
                                        "document worker returned an invalid preview".into()
                                    );
                                }
                                use base64::Engine;
                                let png = base64::engine::general_purpose::STANDARD
                                    .decode(&image.data_url["data:image/png;base64,".len()..])
                                    .map_err(|_| "invalid page preview encoding")?;
                                if png.len() < 24
                                    || png[..8] != [137, 80, 78, 71, 13, 10, 26, 10]
                                    || &png[12..16] != b"IHDR"
                                {
                                    return Err("invalid PNG page preview".into());
                                }
                                let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
                                let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
                                if width == 0 || height == 0 || width > 1200 || height > 1700 {
                                    return Err(
                                        "page preview dimensions exceed their limits".into()
                                    );
                                }
                            } else if expected_preview.is_some() {
                                return Err(
                                    "document worker did not return the requested preview".into()
                                );
                            }
                            if preview.is_none()
                                && (texts.is_empty()
                                    || texts.len() > render::MAX_PAGES as usize
                                    || texts.iter().map(|text| text.len() + 2).sum::<usize>()
                                        > render::MAX_TEXT_BYTES)
                            {
                                return Err("document worker result exceeds its limits".into());
                            }
                            while child
                                .0
                                .try_wait()
                                .map_err(|error| error.to_string())?
                                .is_none()
                            {
                                if cancelled.load(Ordering::Relaxed) {
                                    return Err("import cancelled".into());
                                }
                                if started.elapsed() >= WORKER_TIMEOUT {
                                    return Err("document worker did not exit".into());
                                }
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            if !child.0.wait().map_err(|error| error.to_string())?.success() {
                                return Err("document worker exited unsuccessfully".into());
                            }
                            return Ok(WorkerResult {
                                texts,
                                used_ocr,
                                preview,
                                source_hash,
                            });
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(
                        "document worker stopped before completing; the submission was not saved"
                            .into(),
                    )
                }
            }
        }
    })();
    drop(receiver);
    drop(child);
    let _ = reader.join();
    result
}
