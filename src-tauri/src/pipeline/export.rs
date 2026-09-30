//! Exports and audit history (Milestones 6 and 7). Real names are resolved
//! locally only when generating grade or feedback files.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use thiserror::Error;

use crate::models::AuditLog;

/// Prevent spreadsheet software from interpreting untrusted cells as formulas.
fn safe_csv_cell(value: &str) -> String {
    if value.trim_start().starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r', '\n'])
    {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("file I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PDF generation error: {0}")]
    Pdf(String),
}

/// Font-aware wrapping and embedded Unicode fonts keep reports readable at any length.
fn render_report(lines: &[String], path_source: &Path) -> Result<Vec<u8>, ExportError> {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};
    use skrifa::instance::{LocationRef, Size};
    use skrifa::raw::TableProvider;
    use skrifa::{FontRef, GlyphId, MetadataProvider};
    use std::collections::{BTreeMap, BTreeSet};
    let font_bytes = std::fs::read(path_source)?;
    let face = FontRef::new(&font_bytes).map_err(|e| ExportError::Pdf(e.to_string()))?;
    let head = face.head().map_err(|e| ExportError::Pdf(e.to_string()))?;
    let scale = 1000.0 / head.units_per_em() as f64;
    let charmap = face.charmap();
    let glyph_metrics = face.glyph_metrics(Size::unscaled(), LocationRef::default());
    let font_metrics = face.metrics(Size::unscaled(), LocationRef::default());
    let advance = |c: char| {
        charmap
            .map(c)
            .and_then(|g| glyph_metrics.advance_width(g))
            .unwrap_or(600.0) as f64
            * scale
    };
    let mut wrapped = Vec::new();
    for text in lines {
        for paragraph in text.split('\n') {
            let mut line = String::new();
            let mut width = 0.0;
            for word in paragraph.split_whitespace() {
                let word_width: f64 = word.chars().map(|c| advance(c) * 11.0 / 1000.0).sum();
                if !line.is_empty() {
                    let space_width = advance(' ') * 11.0 / 1000.0;
                    if width + space_width + word_width > 511.0 {
                        wrapped.push(std::mem::take(&mut line));
                        width = 0.0;
                    } else {
                        line.push(' ');
                        width += space_width;
                    }
                }
                // Break a word only when it cannot fit on an otherwise empty line.
                for c in word.chars() {
                    let character_width = advance(c) * 11.0 / 1000.0;
                    if width + character_width > 511.0 && !line.is_empty() {
                        wrapped.push(std::mem::take(&mut line));
                        width = 0.0;
                    }
                    line.push(c);
                    width += character_width;
                }
            }
            wrapped.push(line);
        }
    }
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    let page_count = wrapped.len().div_ceil(43);
    let footers: Vec<String> = (1..=page_count)
        .map(|page| format!("Page {page} of {page_count}"))
        .collect();
    let mut glyphs = BTreeMap::new();
    let mut widths = BTreeSet::new();
    for c in wrapped.iter().chain(footers.iter()).flat_map(|s| s.chars()) {
        let actual = if charmap.map(c).is_some() { c } else { '?' };
        let glyph = charmap
            .map(actual)
            .ok_or_else(|| ExportError::Pdf("font lacks a fallback glyph".into()))?;
        glyphs.entry(glyph.to_u32() as u16).or_insert(actual);
        widths.insert(glyph.to_u32() as u16);
    }
    let encode = |text: &str| -> Vec<u8> {
        text.chars()
            .flat_map(|c| {
                (charmap
                    .map(c)
                    .or_else(|| charmap.map('?'))
                    .unwrap()
                    .to_u32() as u16)
                    .to_be_bytes()
            })
            .collect()
    };
    let mut cmap = String::from("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
    let entries: Vec<_> = glyphs.iter().collect();
    for chunk in entries.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (glyph, c) in chunk {
            let unicode: String = c
                .encode_utf16(&mut [0; 2])
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect();
            cmap.push_str(&format!("<{glyph:04X}> <{unicode}>\n"));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_file = doc.add_object(Stream::new(
        dictionary! { "Length1" => font_bytes.len() as i64 },
        font_bytes.clone(),
    ));

    let descriptor = doc.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => "Roboto", "Flags" => 32,
        "FontBBox" => vec![Object::Integer((head.x_min() as f64*scale) as i64), Object::Integer((head.y_min() as f64*scale) as i64), Object::Integer((head.x_max() as f64*scale) as i64), Object::Integer((head.y_max() as f64*scale) as i64)],
        "ItalicAngle" => 0, "Ascent" => (font_metrics.ascent as f64*scale) as i64,
        "Descent" => (font_metrics.descent as f64*scale) as i64, "CapHeight" => 710, "StemV" => 80,
        "FontFile2" => font_file
    });
    let mut pdf_widths = Vec::<Object>::new();
    for gid in widths {
        pdf_widths.push((gid as i64).into());
        pdf_widths.push(Object::Array(vec![((glyph_metrics
            .advance_width(GlyphId::new(gid as u32))
            .unwrap_or(600.0) as f64
            * scale) as i64)
            .into()]));
    }
    let cid = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "Roboto", "FontDescriptor" => descriptor,
        "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        "CIDToGIDMap" => "Identity", "DW" => 600, "W" => Object::Array(pdf_widths)
    });
    let unicode = doc.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "Roboto", "Encoding" => "Identity-H", "DescendantFonts" => vec![cid.into()], "ToUnicode" => unicode });
    let mut kids = Vec::<Object>::new();
    for (index, chunk) in wrapped.chunks(43).enumerate() {
        let mut operations = Vec::new();
        for (line, y) in chunk
            .iter()
            .enumerate()
            .map(|(i, s)| (s, 785 - i as i64 * 16))
            .chain(std::iter::once((&footers[index], 40)))
        {
            operations.extend([
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 11.into()]),
                Operation::new("Td", vec![42.into(), y.into()]),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        encode(line),
                        lopdf::StringFormat::Hexadecimal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ]);
        }
        let bytes = Content { operations }
            .encode()
            .map_err(|e| ExportError::Pdf(e.to_string()))?;
        let content = doc.add_object(Stream::new(dictionary! {}, bytes));
        let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(),0.into(),595.into(),842.into()], "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } }, "Contents" => content });
        kids.push(page.into());
    }
    doc.objects.insert(
        pages_id,
        dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => page_count as i64 }.into(),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc.compress();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    Ok(bytes)
}

pub fn generate_pdf_feedback(
    conn: &Connection,
    submission_id: i64,
    path_source: &Path,
    path_destination: &Path,
) -> Result<(), ExportError> {
    let (status, student_name): (String,String) = conn.query_row(
        "SELECT s.status_pipeline, m.student_name FROM submissions s JOIN alias_student_map m ON m.alias=s.alias WHERE s.id=?1",
        [submission_id], |r|Ok((r.get(0)?,r.get(1)?)))?;
    if status != "grade_confirmed" {
        return Err(ExportError::Pdf(
            "confirm the grade before exporting feedback".into(),
        ));
    }
    let mut stmt=conn.prepare("SELECT cr.code, cr.description, cr.score_max, r.score_final, r.comment_teacher FROM results r JOIN criteria_rubric cr ON cr.id=r.criterion_id WHERE r.submission_id=?1 ORDER BY cr.sort_order")?;
    let rows: Vec<(String, String, f64, f64, Option<String>)> = stmt
        .query_map([submission_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut lines = vec![format!("Grading report - {student_name}"), String::new()];
    for (code, description, max, score, comment) in &rows {
        lines.push(format!("{code} ({score}/{max}): {description}"));
        if let Some(comment) = comment {
            lines.push(format!("Teacher comment: {comment}"));
        }
        lines.push(String::new());
    }
    lines.push(format!(
        "Total score: {}",
        rows.iter().map(|r| r.3).sum::<f64>()
    ));
    let feedback: Option<String> = conn.query_row("SELECT feedback_json FROM submissions WHERE id=?1 AND feedback_revision=grade_revision",[submission_id],|r|r.get(0)).optional()?.flatten();
    if let Some(json) = feedback {
        let value: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| ExportError::Pdf(e.to_string()))?;
        if let Some(text) = value.get("comment_feedback").and_then(|v| v.as_str()) {
            lines.push(String::new());
            lines.push("Teacher-reviewed AI draft:".into());
            lines.push(text.into());
        }
    }
    let bytes = render_report(&lines, path_source)?;
    write_atomic(path_destination, &bytes)?;
    Ok(())
}

pub fn write_atomic(destination: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    use std::io::Write;
    let parent = destination
        .parent()
        .ok_or_else(|| std::io::Error::other("destination has no parent"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(destination).map_err(|e| e.error)?;
    Ok(())
}

pub fn generate_csv(
    conn: &Connection,
    assignment_id: i64,
    path_destination: &Path,
) -> Result<usize, ExportError> {
    let mut stmt_submissions = conn.prepare(
        "SELECT id, alias FROM submissions
         WHERE assignment_id = ?1 AND status_pipeline = 'grade_confirmed'",
    )?;
    let submissions: Vec<(i64, Option<String>)> = stmt_submissions
        .query_map([assignment_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(["student", "grade_total", "criterion_details"])?;

    let mut rows_written = 0;
    for (submission_id, alias) in &submissions {
        let student_name: String = match alias {
            Some(alias) => conn.query_row(
                "SELECT student_name FROM alias_student_map WHERE alias = ?1",
                [alias],
                |r| r.get(0),
            )?,
            None => "(without a registered alias)".to_string(),
        };

        let mut stmt_res = conn.prepare(
            "SELECT cr.code, r.score_final FROM results r
             JOIN criteria_rubric cr ON cr.id = r.criterion_id
             WHERE r.submission_id = ?1 ORDER BY cr.sort_order",
        )?;
        let rows: Vec<(String, Option<f64>)> = stmt_res
            .query_map([submission_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let total: f64 = rows.iter().filter_map(|(_, p)| *p).sum();
        let details = rows
            .iter()
            .map(|(code, score)| {
                format!(
                    "{code}={}",
                    score.map(|v| v.to_string()).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("; ");

        writer.write_record([
            safe_csv_cell(&student_name),
            total.to_string(),
            safe_csv_cell(&details),
        ])?;
        rows_written += 1;
    }
    writer.flush()?;
    let bytes = writer
        .into_inner()
        .map_err(|e| ExportError::Io(e.into_error()))?;
    write_atomic(path_destination, &bytes)?;

    conn.execute(
        "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('export_csv', 'teacher', ?1)",
        [serde_json::json!({ "assignment_id": assignment_id, "rows": rows_written }).to_string()],
    )?;

    Ok(rows_written)
}

/// Milestone 7: exports the traceability record to CSV. It does not resolve real names (the log never contains them, see commands that write in `logs_audit`) — it is safe to share this file with the DPO without further processing.
pub fn generate_csv_logs(
    conn: &Connection,
    submission_id: Option<i64>,
    path_destination: &Path,
) -> Result<usize, ExportError> {
    let logs = list_logs(conn, submission_id)?;
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(["date", "submission_id", "event", "actor", "model_version"])?;
    for log in &logs {
        writer.write_record([
            safe_csv_cell(&log.created_at),
            log.submission_id.map(|v| v.to_string()).unwrap_or_default(),
            safe_csv_cell(&log.event),
            safe_csv_cell(&log.actor),
            safe_csv_cell(log.model_version.as_deref().unwrap_or_default()),
        ])?;
    }
    writer.flush()?;
    let bytes = writer
        .into_inner()
        .map_err(|e| ExportError::Io(e.into_error()))?;
    write_atomic(path_destination, &bytes)?;
    Ok(logs.len())
}

pub fn list_logs(
    conn: &Connection,
    submission_id: Option<i64>,
) -> Result<Vec<AuditLog>, ExportError> {
    let mut stmt = conn.prepare(
        "SELECT id, submission_id, event, actor, model_version, payload_json, created_at
         FROM logs_audit
         WHERE ?1 IS NULL OR submission_id = ?1
         ORDER BY id",
    )?;
    let logs = stmt
        .query_map([submission_id], |r| {
            Ok(AuditLog {
                id: r.get(0)?,
                submission_id: r.get(1)?,
                event: r.get(2)?,
                actor: r.get(3)?,
                model_version: r.get(4)?,
                payload_json: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(logs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use crate::pipeline::{redaction, review};
    use tempfile::tempdir;

    #[test]
    fn exports_only_confirmed_grades_with_real_names() {
        let database_directory = tempdir().unwrap();
        let status = schema::open_with_key(database_directory.path(), [6u8; 32]).unwrap();
        let guard = status.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        conn.execute(
            "INSERT INTO rubrics (title, subject, grade_level, content_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubric_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO criteria_rubric (rubric_id, code, description, score_max, sort_order)
             VALUES (?1, 'C1', 'criterion 1', 10, 0)",
            [rubric_id],
        )
        .unwrap();
        let criterion_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO assignments (rubric_id, text) VALUES (?1, 'assignment')",
            [rubric_id],
        )
        .unwrap();
        let assignment_id = conn.last_insert_rowid();

        // Submission 1: confirmed, must appear in the CSV with the real name.
        conn.execute(
            "INSERT INTO submissions (assignment_id, text_ocr, status_pipeline) VALUES (?1, 'text one', 'ocr_complete')",
            [assignment_id],
        )
        .unwrap();
        let submission_one = conn.last_insert_rowid();
        redaction::confirm(conn, submission_one, &[], "Maria Real", None).unwrap();
        conn.execute(
            "INSERT INTO results (submission_id, criterion_id, score_final) VALUES (?1, ?2, 8.5)",
            rusqlite::params![submission_one, criterion_id],
        )
        .unwrap();
        review::confirm_grade(conn, submission_one).unwrap();

        // Submission 2: still not confirmed, MUST NOT appear in the CSV.
        conn.execute(
            "INSERT INTO submissions (assignment_id, text_ocr, status_pipeline) VALUES (?1, 'text two', 'ocr_complete')",
            [assignment_id],
        )
        .unwrap();
        let submission_two = conn.last_insert_rowid();
        redaction::confirm(conn, submission_two, &[], "Other Student", None).unwrap();
        conn.execute(
            "INSERT INTO results (submission_id, criterion_id, score_final) VALUES (?1, ?2, 2.0)",
            rusqlite::params![submission_two, criterion_id],
        )
        .unwrap();

        let csv_directory = tempdir().unwrap();
        let path_csv = csv_directory.path().join("grades.csv");
        let rows = generate_csv(conn, assignment_id, &path_csv).unwrap();
        assert_eq!(rows, 1);

        let content = std::fs::read_to_string(&path_csv).unwrap();
        assert!(content.contains("Maria Real"));
        assert!(content.contains("8.5"));
        assert!(!content.contains("Other Student"));
    }

    #[test]
    fn generates_pdf_of_feedback_with_content_expected() {
        let database_directory = tempdir().unwrap();
        let status = schema::open_with_key(database_directory.path(), [12u8; 32]).unwrap();
        let guard = status.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        conn.execute(
            "INSERT INTO rubrics (title, subject, grade_level, content_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubric_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO criteria_rubric (rubric_id, code, description, score_max, sort_order)
             VALUES (?1, 'C1', 'criterion uno', 10, 0)",
            [rubric_id],
        )
        .unwrap();
        let criterion_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO assignments (rubric_id, text) VALUES (?1, 'assignment')",
            [rubric_id],
        )
        .unwrap();
        let assignment_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO submissions (assignment_id, text_ocr, status_pipeline) VALUES (?1, 'text', 'ocr_complete')",
            [assignment_id],
        )
        .unwrap();
        let submission_id = conn.last_insert_rowid();
        redaction::confirm(conn, submission_id, &[], "Student PDF", None).unwrap();
        conn.execute(
            "INSERT INTO results (submission_id, criterion_id, score_final, comment_teacher)
             VALUES (?1, ?2, 7.0, 'buen trabajo')",
            rusqlite::params![submission_id, criterion_id],
        )
        .unwrap();
        review::confirm_grade(conn, submission_id).unwrap();
        conn.execute(
            "INSERT INTO logs_audit (submission_id, event, actor, payload_json)
             VALUES (?1, 'ai_feedback_request', 'ai', ?2)",
            rusqlite::params![
                submission_id,
                serde_json::json!({ "comment_feedback": "Buen trabajo in general.", "inconsistencies": [] })
                    .to_string()
            ],
        )
        .unwrap();

        let path_source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/fonts/Roboto-Regular.ttf");
        let pdf_directory = tempdir().unwrap();
        let path_pdf = pdf_directory.path().join("feedback.pdf");

        generate_pdf_feedback(conn, submission_id, &path_source, &path_pdf).unwrap();

        let bytes = std::fs::read(&path_pdf).unwrap();
        assert!(
            bytes.starts_with(b"%PDF"),
            "The generated file does not seem to be a valid PDF"
        );
        assert!(
            bytes.len() > 500,
            "The generated PDF seems suspiciously small"
        );
    }

    #[test]
    fn exports_audit_log_without_real_names() {
        let database_directory = tempdir().unwrap();
        let status = schema::open_with_key(database_directory.path(), [11u8; 32]).unwrap();
        let guard = status.conn.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        conn.execute(
            "INSERT INTO rubrics (title, subject, grade_level, content_json) VALUES ('t','a','c','{}')",
            [],
        )
        .unwrap();
        let rubric_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO assignments (rubric_id, text) VALUES (?1, 'assignment')",
            [rubric_id],
        )
        .unwrap();
        let assignment_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO submissions (assignment_id, text_ocr, status_pipeline) VALUES (?1, 'text', 'ocr_complete')",
            [assignment_id],
        )
        .unwrap();
        let submission_id = conn.last_insert_rowid();
        redaction::confirm(
            conn,
            submission_id,
            &[],
            "Name That Should Not Appear",
            None,
        )
        .unwrap();
        let csv_directory = tempdir().unwrap();
        let path_csv = csv_directory.path().join("log.csv");
        let rows = generate_csv_logs(conn, None, &path_csv).unwrap();
        assert!(rows >= 1);

        let content = std::fs::read_to_string(&path_csv).unwrap();
        assert!(!content.contains("Name That Should Not Appear"));
        assert!(content.contains("redaction_confirmed"));
        assert!(!content.contains("grade_confirmed"));
    }

    #[test]
    fn paginates_unicode_reports_without_losing_the_last_line() {
        let font = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/fonts/Roboto-Regular.ttf");
        let mut lines = vec!["Grading report - José Núñez".to_string()];
        lines.extend((0..100).map(|i| {
            format!(
                "Criterion {i}: {}",
                "A very long teacher comment with accented words: también, física, revisión. "
                    .repeat(4)
            )
        }));
        lines.push("END OF REPORT".into());
        let bytes = render_report(&lines, &font).unwrap();
        let pdf = lopdf::Document::load_mem(&bytes).unwrap();
        let pages: Vec<u32> = pdf.get_pages().keys().copied().collect();
        assert!(pages.len() > 2);
        let text = pdf.extract_text(&pages).unwrap();
        assert!(text.contains("José Núñez"));
        assert!(text.contains("END OF REPORT"));
        if let Some(path) = std::env::var_os("CORREGIR_PDF_QA_PATH") {
            write_atomic(Path::new(&path), &bytes).unwrap();
        }
    }

    #[test]
    fn escapes_spreadsheet_formulas() {
        for value in ["=1+1", " +SUM(A1)", "@SUM(A1)", "\t=1", "-2+3"] {
            assert!(safe_csv_cell(value).starts_with('\''));
        }
        assert_eq!(safe_csv_cell("Maria"), "Maria");
    }
}
