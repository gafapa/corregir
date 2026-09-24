//! Extracción/renderizado de documentos (Hito 3). Genera, para cada página,
//! o bien texto nativo (si el PDF ya lo tiene) o una imagen rasterizada
//! (para pasarla al OCR). Ver "frontera de privacidad" en
//! docs/ARQUITECTURA.md: el documento original nunca se conserva más allá de
//! este paso.
//!
//! Nota de alcance (Hito 3): la vía DOCX real (con control de cambios,
//! imágenes incrustadas) se simplifica en Fase A; ver comentario en
//! `docs/ARQUITECTURA.md` sobre sustituir esto por LibreOffice headless
//! antes de Fase C. Este módulo cubre PDF e imágenes sueltas (capturas).

use std::path::Path;

use image::RgbImage;
use pdfium_render::prelude::*;
use thiserror::Error;

/// Un PDF con menos caracteres de texto "nativo" que esto en una página se
/// trata como escaneada (sin capa de texto útil) y se manda a OCR.
const UMBRAL_CARACTERES_TEXTO_NATIVO: usize = 10;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("error de Pdfium: {0}")]
    Pdfium(#[from] PdfiumError),
    #[error("error de imagen: {0}")]
    Imagen(#[from] image::ImageError),
    #[error("error de E/S: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
pub enum ContenidoPagina {
    TextoNativo(String),
    ImagenParaOcr(RgbImage),
}

/// Abre Pdfium usando la biblioteca nativa bundleada en `dir_pdfium`
/// (ver `resources/pdfium/` y Hito 3 en docs/ARQUITECTURA.md).
fn cargar_pdfium(dir_pdfium: &Path) -> Result<Pdfium, RenderError> {
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dir_pdfium))?;
    Ok(Pdfium::new(bindings))
}

/// Procesa un PDF página a página: texto nativo si lo hay, si no,
/// rasterización a imagen para OCR posterior.
pub fn procesar_pdf(dir_pdfium: &Path, ruta_pdf: &Path) -> Result<Vec<ContenidoPagina>, RenderError> {
    let pdfium = cargar_pdfium(dir_pdfium)?;
    let documento = pdfium.load_pdf_from_file(ruta_pdf, None)?;

    let config_render = PdfRenderConfig::new()
        .set_target_width(1600)
        .set_maximum_height(2200);

    let mut resultado = Vec::new();
    for pagina in documento.pages().iter() {
        let texto = pagina.text()?.all();
        if texto.trim().chars().count() >= UMBRAL_CARACTERES_TEXTO_NATIVO {
            resultado.push(ContenidoPagina::TextoNativo(texto));
        } else {
            let bitmap = pagina.render_with_config(&config_render)?;
            let imagen = bitmap.as_image().into_rgb8();
            resultado.push(ContenidoPagina::ImagenParaOcr(imagen));
        }
    }
    Ok(resultado)
}

/// Carga directamente una imagen suelta (captura/foto entregada por el
/// alumno), siempre destinada a OCR.
pub fn cargar_imagen(ruta_imagen: &Path) -> Result<RgbImage, RenderError> {
    Ok(image::open(ruta_imagen)?.into_rgb8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir_pdfium() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/pdfium/bin")
    }

    fn dir_synthetic_data() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("synthetic-data")
    }

    #[test]
    fn extrae_texto_nativo_de_pdf_con_texto() {
        let paginas = procesar_pdf(
            &dir_pdfium(),
            &dir_synthetic_data().join("lengua-alumno1.pdf"),
        )
        .unwrap();
        assert_eq!(paginas.len(), 1);
        match &paginas[0] {
            ContenidoPagina::TextoNativo(texto) => {
                assert!(texto.contains("nostalgia") || texto.contains("recursos literarios"));
            }
            ContenidoPagina::ImagenParaOcr(_) => panic!("se esperaba texto nativo, no imagen"),
        }
    }

    #[test]
    fn detecta_pdf_escaneado_y_rasteriza() {
        let paginas = procesar_pdf(
            &dir_pdfium(),
            &dir_synthetic_data().join("lengua-alumno2-escaneado.pdf"),
        )
        .unwrap();
        assert_eq!(paginas.len(), 1);
        match &paginas[0] {
            ContenidoPagina::ImagenParaOcr(imagen) => {
                assert!(imagen.width() > 0 && imagen.height() > 0);
            }
            ContenidoPagina::TextoNativo(_) => panic!("se esperaba imagen, no texto nativo"),
        }
    }
}
