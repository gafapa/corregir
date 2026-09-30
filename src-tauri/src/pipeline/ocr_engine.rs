//! Local OCR (Milestone 3). Uses the pure Rust `ocrs` engine and `rten`
//! runtime, avoiding a separate native OCR executable. Model weights are
//! bundled in `resources/models/ocr/`.

use std::path::Path;

use image::RgbImage;
use ocrs::{ImageSource, OcrEngine as OcrsEngine, OcrEngineParams};
use rten::Model;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("could not load OCR model '{0}': {1}")]
    Model(String, String),
    #[error("error of the engine of OCR: {0}")]
    Engine(String),
}

pub struct OcrEngine {
    engine: OcrsEngine,
}

impl OcrEngine {
    pub fn load(directory_models: &Path) -> Result<Self, OcrError> {
        let detection_path = directory_models.join("text-detection.rten");
        let detection_model = Model::load_file(&detection_path)
            .map_err(|e| OcrError::Model(detection_path.display().to_string(), e.to_string()))?;

        let recognition_path = directory_models.join("text-recognition.rten");
        let recognition_model = Model::load_file(&recognition_path)
            .map_err(|e| OcrError::Model(recognition_path.display().to_string(), e.to_string()))?;

        let engine = OcrsEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(recognition_model),
            ..Default::default()
        })
        .map_err(|e| OcrError::Engine(e.to_string()))?;

        Ok(Self { engine })
    }

    /// Recognizes the text of an RGB image. It is used both for pages of
    /// rasterized PDFs (without text layer) and for direct captures/photos
    /// provided by the student.
    pub fn recognize_image(&self, image: &RgbImage) -> Result<String, OcrError> {
        let source = ImageSource::from_bytes(image.as_raw(), image.dimensions())
            .map_err(|e| OcrError::Engine(e.to_string()))?;
        let input = self
            .engine
            .prepare_input(source)
            .map_err(|e| OcrError::Engine(e.to_string()))?;
        self.engine
            .get_text(&input)
            .map_err(|e| OcrError::Engine(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::render;
    use std::path::PathBuf;

    fn directory_models() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/models/ocr")
    }

    fn directory_pdfium() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/pdfium/bin")
    }

    fn directory_synthetic_data() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("synthetic-data")
    }

    /// Milestone 3: verify OCR on a synthetic scanned PDF without a text layer.
    #[test]
    fn recognizes_text_in_scanned_pdf() {
        let engine = OcrEngine::load(&directory_models()).expect("OCR models should load");
        let pages = render::process_pdf(
            &directory_pdfium(),
            &directory_synthetic_data().join("language-student-2-scanned.pdf"),
        )
        .unwrap();

        let image = match &pages[0] {
            render::PageContent::ImageForOcr(img) => img,
            _ => panic!("expected a rasterized image"),
        };

        let text = engine.recognize_image(image).unwrap();
        let normalized_text = text.to_lowercase();
        // We do not require perfect accuracy from the OCR engine (reasonable in a
        // small general-purpose model), but it must recognize
        // key fragments of the real content of the submission.
        assert!(
            normalized_text.contains("student") || normalized_text.contains("poem"),
            "OCR text did not contain expected fragments: {text}"
        );
    }
}
