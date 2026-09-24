//! OCR local (Hito 3). Usa `ocrs`, un motor de OCR puro Rust (vía el
//! runtime `rten`) en vez de Tesseract: evita depender de un binario nativo
//! externo que habría que descargar/instalar/bundlear por separado (el mismo
//! tipo de fricción de toolchain que ya obligó a cambiar de SQLCipher a
//! cifrado a nivel de aplicación en Hito 2 — ver db/schema.rs). Los pesos
//! del modelo (`.rten`, ~12 MB en total) se distribuyen con la app en
//! `resources/models/ocr/`.

use std::path::Path;

use image::RgbImage;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("no se pudo cargar el modelo de OCR '{0}': {1}")]
    Modelo(String, String),
    #[error("error del motor de OCR: {0}")]
    Motor(String),
}

pub struct MotorOcr {
    engine: OcrEngine,
}

impl MotorOcr {
    pub fn cargar(dir_modelos: &Path) -> Result<Self, OcrError> {
        let ruta_deteccion = dir_modelos.join("text-detection.rten");
        let deteccion = Model::load_file(&ruta_deteccion)
            .map_err(|e| OcrError::Modelo(ruta_deteccion.display().to_string(), e.to_string()))?;

        let ruta_reconocimiento = dir_modelos.join("text-recognition.rten");
        let reconocimiento = Model::load_file(&ruta_reconocimiento)
            .map_err(|e| OcrError::Modelo(ruta_reconocimiento.display().to_string(), e.to_string()))?;

        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(deteccion),
            recognition_model: Some(reconocimiento),
            ..Default::default()
        })
        .map_err(|e| OcrError::Motor(e.to_string()))?;

        Ok(Self { engine })
    }

    /// Reconoce el texto de una imagen en RGB. Se usa tanto para páginas de
    /// PDF rasterizadas (sin capa de texto) como para capturas/fotos
    /// entregadas directamente por el alumno.
    pub fn reconocer_imagen(&self, imagen: &RgbImage) -> Result<String, OcrError> {
        let fuente = ImageSource::from_bytes(imagen.as_raw(), imagen.dimensions())
            .map_err(|e| OcrError::Motor(e.to_string()))?;
        let entrada = self
            .engine
            .prepare_input(fuente)
            .map_err(|e| OcrError::Motor(e.to_string()))?;
        self.engine
            .get_text(&entrada)
            .map_err(|e| OcrError::Motor(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::render;
    use std::path::PathBuf;

    fn dir_modelos() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/models/ocr")
    }

    fn dir_pdfium() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/pdfium/bin")
    }

    fn dir_synthetic_data() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("synthetic-data")
    }

    /// Hito 3, verificación end-to-end: rasteriza el PDF sintético
    /// "escaneado" (sin capa de texto) y confirma que el OCR recupera texto
    /// reconocible del contenido real (no solo que "no crashea").
    #[test]
    fn reconoce_texto_de_pdf_escaneado() {
        let motor = MotorOcr::cargar(&dir_modelos()).expect("modelos de OCR deben cargar");
        let paginas = render::procesar_pdf(
            &dir_pdfium(),
            &dir_synthetic_data().join("lengua-alumno2-escaneado.pdf"),
        )
        .unwrap();

        let imagen = match &paginas[0] {
            render::ContenidoPagina::ImagenParaOcr(img) => img,
            _ => panic!("se esperaba una imagen rasterizada"),
        };

        let texto = motor.reconocer_imagen(imagen).unwrap();
        let texto_normalizado = texto.to_lowercase();
        // No exigimos exactitud perfecta del motor de OCR (razonable en un
        // modelo pequeño de propósito general), pero sí que reconozca
        // fragmentos clave del contenido real de la entrega.
        assert!(
            texto_normalizado.contains("respuesta") || texto_normalizado.contains("poema"),
            "texto OCR no contiene fragmentos esperados: {texto}"
        );
    }
}
