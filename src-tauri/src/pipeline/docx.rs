//! Bounded, local extraction of text-only Word documents. No files are unpacked.
use super::render::{self, RenderError};
use quick_xml::{events::Event, Reader};
use std::{io::Read, path::Path};
const MAX_XML_BYTES: u64 = 4 * 1024 * 1024;

pub fn extract(path: &Path) -> Result<String, RenderError> {
    render::validate_file(path)?;
    let file = std::fs::File::open(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| RenderError::Processing(error.to_string()))?;
    if archive.len() > 2000 {
        return Err(RenderError::LimitExceeded);
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| RenderError::Processing(error.to_string()))?;
        total = total
            .checked_add(entry.size())
            .ok_or(RenderError::LimitExceeded)?;
        if total > 128 * 1024 * 1024 || entry.is_symlink() || entry.encrypted() {
            return Err(RenderError::LimitExceeded);
        }
        let name = entry.name();
        if name.ends_with(".bin")
            || name.starts_with("word/media/")
            || name.starts_with("word/embeddings/")
            || name.starts_with("word/header")
            || name.starts_with("word/footer")
            || name == "word/footnotes.xml"
            || name == "word/endnotes.xml"
        {
            return Err(RenderError::Processing("this DOCX contains images, objects, headers or notes; export it as PDF to preserve all content".into()));
        }
    }
    let entry = archive
        .by_name("word/document.xml")
        .map_err(|_| RenderError::Processing("not a supported Word DOCX document".into()))?;
    if entry.size() > MAX_XML_BYTES {
        return Err(RenderError::LimitExceeded);
    }
    let mut xml = String::new();
    entry.take(MAX_XML_BYTES + 1).read_to_string(&mut xml)?;
    if xml.len() as u64 > MAX_XML_BYTES {
        return Err(RenderError::LimitExceeded);
    }
    parse_text(&xml)
}

fn parse_text(xml: &str) -> Result<String, RenderError> {
    let mut reader = Reader::from_str(xml);
    let mut text = String::new();
    let mut in_text = false;
    let mut depth = 0usize;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| RenderError::Processing(error.to_string()))?;
        match event {
            Event::DocType(_) => {
                return Err(RenderError::Processing(
                    "DOCX document type declarations are not supported".into(),
                ))
            }
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                }
                if depth > 256 {
                    return Err(RenderError::LimitExceeded);
                }
                match tag.local_name().as_ref() {
                    "ins" | "del" | "moveFrom" | "moveTo" | "drawing" | "object" | "pict" | "altChunk" | "fldChar" | "fldSimple" | "oMath" | "oMathPara" | "numPr" => return Err(RenderError::Processing("accept tracked changes and convert drawings, equations, automatic numbering or fields to plain text, or export this DOCX as PDF".into())),
                    "t" => in_text = true,
                    "tab" => text.push('\t'),
                    "br" | "cr" => text.push('\n'),
                    _ => ()
                }
            }
            Event::End(tag) => {
                depth = depth.saturating_sub(1);
                match tag.local_name().as_ref() {
                    "t" => in_text = false,
                    "p" => text.push('\n'),
                    "tc" => text.push('\t'),
                    _ => (),
                }
            }
            Event::Text(value) if in_text => text.push_str(&value.xml10_content()),
            Event::GeneralRef(value) if in_text => {
                let name: &str = value.as_ref();
                let escaped = format!("&{name};");
                text.push_str(
                    &quick_xml::escape::unescape(&escaped)
                        .map_err(|error| RenderError::Processing(error.to_string()))?,
                );
            }
            Event::Eof => break,
            _ => (),
        }
        if text.len() > render::MAX_TEXT_BYTES {
            return Err(RenderError::LimitExceeded);
        }
    }
    if depth != 0 || text.trim().is_empty() {
        return Err(RenderError::Processing(
            "the DOCX has no complete, readable text document".into(),
        ));
    }
    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn imports_unicode_paragraphs_and_table_cells_without_plaintext_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.docx");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all("<w:document xmlns:w='urn:test'><w:body><w:p><w:r><w:t>Poésía &amp; &#937;</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>".as_bytes()).unwrap();
        zip.finish().unwrap();
        assert_eq!(extract(&path).unwrap(), "Poésía & Ω\nCell");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn refuses_revisions_objects_entities_and_incomplete_xml() {
        for xml in [
            "<!DOCTYPE x [<!ENTITY x SYSTEM 'file:///secret'>]><x/>",
            "<w:p><w:del/></w:p>",
            "<w:p><w:drawing/></w:p>",
            "<w:p><w:t>unfinished",
        ] {
            assert!(parse_text(xml).is_err());
        }
    }
    #[test]
    fn refuses_zip_bombs_before_decompression() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.docx");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(&vec![b'a'; MAX_XML_BYTES as usize + 1])
            .unwrap();
        zip.finish().unwrap();
        assert!(matches!(extract(&path), Err(RenderError::LimitExceeded)));
    }
}
