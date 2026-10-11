//! Extraction/rendering of documents (Milestone 3). Generates, for each page,
//! either native text (if the PDF already has it) or a rasterized image
//! (to pass it to OCR). See "privacy boundary" in
//! docs/ARCHITECTURE.md: the original document is never kept beyond
//! this step.
//!
//! Scope note (Milestone 3): the full DOCX path (with tracked changes,
//! embedded images) is simplified in Phase A; see comment in
//! `docs/ARCHITECTURE.md` about replacing this with LibreOffice headless
//! before Phase C. This module covers PDF and standalone images (captures).

use std::path::Path;

use image::RgbImage;
use pdfium_render::prelude::*;
use thiserror::Error;

/// A PDF with fewer native text characters than this per page
/// is treated as scanned (without useful text layer) and is sent to OCR.
const NATIVE_TEXT_CHARACTER_THRESHOLD: usize = 10;
static PDFIUM_INSTANCE: std::sync::OnceLock<Pdfium> = std::sync::OnceLock::new();
static PDFIUM_PIPELINE: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_PAGES: u16 = 50;
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("document exceeds the file, page, image, or text safety limits")]
    LimitExceeded,
    #[error("import cancelled")]
    Cancelled,
    #[error("document processing failed: {0}")]
    Processing(String),
    #[error("error of Pdfium: {0}")]
    Pdfium(#[from] PdfiumError),
    #[error("error of image: {0}")]
    Image(#[from] image::ImageError),
    #[error("document I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
pub enum PageContent {
    NativeText(String),
    ImageForOcr(RgbImage),
}

/// Load Pdfium from the bundled native library in `directory_pdfium`
/// (see `resources/pdfium/` and Milestone 3 in docs/ARCHITECTURE.md).
fn load_pdfium(directory_pdfium: &Path) -> Result<&'static Pdfium, RenderError> {
    if let Some(pdfium) = PDFIUM_INSTANCE.get() {
        return Ok(pdfium);
    }
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
        directory_pdfium,
    ))?;
    Ok(PDFIUM_INSTANCE.get_or_init(|| Pdfium::new(bindings)))
}

/// Processes PDF page by page: native text if available, if not,
/// rasterization to image for subsequent OCR.
#[cfg(test)]
pub fn process_pdf(
    directory_pdfium: &Path,
    path_pdf: &Path,
) -> Result<Vec<PageContent>, RenderError> {
    let mut pages = Vec::new();
    visit_pdf(directory_pdfium, path_pdf, |page, _, _| {
        pages.push(page);
        Ok(())
    })?;
    Ok(pages)
}

pub fn validate_file(path: &Path) -> Result<(), RenderError> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(RenderError::LimitExceeded);
    }
    Ok(())
}

/// Consume one page at a time so rasterized documents cannot accumulate in RAM.
#[cfg(test)]
pub fn visit_pdf<F>(directory_pdfium: &Path, path_pdf: &Path, consume: F) -> Result<(), RenderError>
where
    F: FnMut(PageContent, u16, u16) -> Result<(), RenderError>,
{
    visit_pdf_with_ocr(directory_pdfium, path_pdf, false, consume)
}

#[cfg(test)]
pub fn visit_pdf_with_ocr<F>(
    directory_pdfium: &Path,
    path_pdf: &Path,
    force_ocr: bool,
    consume: F,
) -> Result<(), RenderError>
where
    F: FnMut(PageContent, u16, u16) -> Result<(), RenderError>,
{
    validate_file(path_pdf)?;
    // Pdfium initialization and destruction affect process-global native state.
    let _parser_guard = PDFIUM_PIPELINE
        .lock()
        .map_err(|_| RenderError::Processing("the PDF parser worker failed previously".into()))?;
    let pdfium = load_pdfium(directory_pdfium)?;
    let document = pdfium.load_pdf_from_file(path_pdf, None)?;
    visit_document(&document, force_ocr, consume)
}

pub fn visit_pdf_bytes<F>(
    directory_pdfium: &Path,
    bytes: &[u8],
    force_ocr: bool,
    consume: F,
) -> Result<(), RenderError>
where
    F: FnMut(PageContent, u16, u16) -> Result<(), RenderError>,
{
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(RenderError::LimitExceeded);
    }
    let _guard = PDFIUM_PIPELINE
        .lock()
        .map_err(|_| RenderError::Processing("PDF parser lock failed".into()))?;
    let document = load_pdfium(directory_pdfium)?.load_pdf_from_byte_slice(bytes, None)?;
    visit_document(&document, force_ocr, consume)
}

fn visit_document<F>(
    document: &PdfDocument<'_>,
    force_ocr: bool,
    mut consume: F,
) -> Result<(), RenderError>
where
    F: FnMut(PageContent, u16, u16) -> Result<(), RenderError>,
{
    let total = document.pages().len();
    if total == 0 || total > MAX_PAGES as i32 {
        return Err(RenderError::LimitExceeded);
    }
    let config = PdfRenderConfig::new()
        .set_target_width(1600)
        .set_maximum_height(2200);
    let mut text_bytes = 0;
    for (index, page) in document.pages().iter().enumerate() {
        let page_text = page.text()?;
        if page_text.len() as usize > MAX_TEXT_BYTES {
            return Err(RenderError::LimitExceeded);
        }
        let text = page_text.all();
        text_bytes += text.len();
        if text_bytes > MAX_TEXT_BYTES {
            return Err(RenderError::LimitExceeded);
        }
        // Form XObjects may wrap images; conservatively OCR them as well.
        let has_images = page.objects().iter().any(|object| {
            object.as_image_object().is_some() || object.as_x_object_form_object().is_some()
        });
        let content = if !force_ocr
            && !has_images
            && text.trim().chars().count() >= NATIVE_TEXT_CHARACTER_THRESHOLD
        {
            PageContent::NativeText(text)
        } else {
            PageContent::ImageForOcr(page.render_with_config(&config)?.as_image()?.into_rgb8())
        };
        consume(content, index as u16 + 1, total as u16)?;
    }
    Ok(())
}

/// Load a standalone image or photo for OCR.
#[cfg(test)]
pub fn load_image(path_image: &Path) -> Result<RgbImage, RenderError> {
    validate_file(path_image)?;
    load_image_bytes(&std::fs::read(path_image)?)
}

pub fn load_image_bytes(bytes: &[u8]) -> Result<RgbImage, RenderError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(RenderError::LimitExceeded);
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(6000);
    limits.max_image_height = Some(6000);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode()?;
    if decoded.width() as u64 * decoded.height() as u64 > 16_000_000 {
        return Err(RenderError::LimitExceeded);
    }
    Ok(decoded.into_rgb8())
}

/// Rasterize one selected page locally; PDF parsing remains in the document worker.
pub fn preview_bytes(
    directory_pdfium: &Path,
    bytes: &[u8],
    extension: &str,
    page_number: u16,
) -> Result<(RgbImage, u16), RenderError> {
    if page_number == 0 || bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(RenderError::LimitExceeded);
    }
    match extension {
        "pdf" => {
            let _guard = PDFIUM_PIPELINE.lock().map_err(|_| RenderError::Processing("PDF parser lock failed".into()))?;
            let document = load_pdfium(directory_pdfium)?.load_pdf_from_byte_slice(bytes,None)?;
            let total = document.pages().len();
            if total == 0 || total > MAX_PAGES as i32 || page_number as i32 > total { return Err(RenderError::LimitExceeded); }
            let page = document.pages().get(page_number as i32 - 1)?;
            let image = page.render_with_config(&PdfRenderConfig::new().set_target_width(1200).set_maximum_height(1700))?.as_image()?.into_rgb8();
            Ok((image,total as u16))
        },
        "png" | "jpg" | "jpeg" if page_number == 1 => Ok((image::imageops::thumbnail(&load_image_bytes(bytes)?,1200,1700),1)),
        _ => Err(RenderError::Processing("page preview supports PDF, PNG and JPEG; export Word documents as PDF to compare their layout".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn directory_pdfium() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/pdfium/bin")
    }

    fn directory_synthetic_data() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("synthetic-data")
    }

    #[test]
    fn rejects_oversized_files_and_images_before_ocr() {
        let directory = tempfile::tempdir().unwrap();
        let huge = directory.path().join("huge.pdf");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert!(matches!(
            validate_file(&huge),
            Err(RenderError::LimitExceeded)
        ));
        let image = directory.path().join("wide.png");
        image::RgbImage::new(6001, 1).save(&image).unwrap();
        assert!(load_image(&image).is_err());
    }
    #[test]
    fn stops_streaming_when_consumer_cancels() {
        let result = visit_pdf(
            &directory_pdfium(),
            &directory_synthetic_data().join("language-student-1.pdf"),
            |_, _, _| Err(RenderError::Cancelled),
        );
        assert!(
            matches!(result, Err(RenderError::Cancelled)),
            "unexpected parser result: {result:?}"
        );
    }

    #[test]
    fn mixed_pages_and_the_manual_override_use_full_page_ocr() {
        let mixed = process_pdf(
            &directory_pdfium(),
            &directory_synthetic_data().join("language-student-2-mixed.pdf"),
        )
        .unwrap();
        assert!(matches!(mixed[0], PageContent::ImageForOcr(_)));
        visit_pdf_with_ocr(
            &directory_pdfium(),
            &directory_synthetic_data().join("language-student-1.pdf"),
            true,
            |page, _, _| {
                assert!(matches!(page, PageContent::ImageForOcr(_)));
                Ok(())
            },
        )
        .unwrap();
    }

    #[test]
    fn extracts_native_text_from_pdf() {
        let pages = process_pdf(
            &directory_pdfium(),
            &directory_synthetic_data().join("language-student-1.pdf"),
        )
        .unwrap();
        assert_eq!(pages.len(), 1);
        match &pages[0] {
            PageContent::NativeText(text) => {
                assert!(text.contains("nostalgia") || text.contains("literary devices"));
            }
            PageContent::ImageForOcr(_) => panic!("expected native text, not an image"),
        }
    }

    #[test]
    fn detects_pdf_scanned_and_rasterizes() {
        let pages = process_pdf(
            &directory_pdfium(),
            &directory_synthetic_data().join("language-student-2-scanned.pdf"),
        )
        .unwrap();
        assert_eq!(pages.len(), 1);
        match &pages[0] {
            PageContent::ImageForOcr(image) => {
                assert!(image.width() > 0 && image.height() > 0);
            }
            PageContent::NativeText(_) => panic!("expected an image, not native text"),
        }
    }
}
