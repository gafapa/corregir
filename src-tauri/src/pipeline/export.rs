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

/// Language of exported reports and spreadsheet headers.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportLanguage {
    Gl,
    Es,
    #[default]
    En,
}

struct Labels {
    report_title: &'static str,
    student: &'static str,
    assignment: &'static str,
    subject: &'static str,
    grade_level: &'static str,
    date: &'static str,
    score: &'static str,
    teacher_comment: &'static str,
    total: &'static str,
    maximum: &'static str,
    grade_ten: &'static str,
    feedback: &'static str,
    ai_note: &'static str,
    page: &'static str,
    of: &'static str,
    alias: &'static str,
}

impl ReportLanguage {
    fn labels(self) -> &'static Labels {
        match self {
            Self::Gl => &Labels {
                report_title: "Informe de avaliación",
                student: "Alumno/a",
                assignment: "Tarefa",
                subject: "Materia",
                grade_level: "Curso",
                date: "Data",
                score: "Puntuación",
                teacher_comment: "Comentario do profesorado",
                total: "Puntuación total",
                maximum: "Máximo",
                grade_ten: "Nota (0-10)",
                feedback: "Retroalimentación",
                ai_note: "Texto redactado coa axuda dunha ferramenta de IA local e revisado polo profesorado.",
                page: "Páxina",
                of: "de",
                alias: "Alias",
            },
            Self::Es => &Labels {
                report_title: "Informe de evaluación",
                student: "Alumno/a",
                assignment: "Tarea",
                subject: "Materia",
                grade_level: "Curso",
                date: "Fecha",
                score: "Puntuación",
                teacher_comment: "Comentario del profesorado",
                total: "Puntuación total",
                maximum: "Máximo",
                grade_ten: "Nota (0-10)",
                feedback: "Retroalimentación",
                ai_note: "Texto redactado con la ayuda de una herramienta de IA local y revisado por el profesorado.",
                page: "Página",
                of: "de",
                alias: "Alias",
            },
            Self::En => &Labels {
                report_title: "Assessment report",
                student: "Student",
                assignment: "Assignment",
                subject: "Subject",
                grade_level: "Year group",
                date: "Date",
                score: "Score",
                teacher_comment: "Teacher comment",
                total: "Total score",
                maximum: "Maximum",
                grade_ten: "Grade (0-10)",
                feedback: "Feedback",
                ai_note: "Drafted with the help of a local AI tool and reviewed by the teacher.",
                page: "Page",
                of: "of",
                alias: "Alias",
            },
        }
    }

    /// Spanish-locale spreadsheets expect semicolons and decimal commas.
    fn decimal_comma(self) -> bool {
        self != Self::En
    }

    fn format_date(self, iso: &str) -> String {
        match (self, iso.get(0..4), iso.get(5..7), iso.get(8..10)) {
            (Self::Gl | Self::Es, Some(year), Some(month), Some(day)) => {
                format!("{day}/{month}/{year}")
            }
            _ => iso.chars().take(10).collect(),
        }
    }
}

/// Round to two decimals and drop trailing zeros, avoiding `7.300000000000001`.
pub fn format_number(value: f64, decimal_comma: bool) -> String {
    let mut text = format!("{:.2}", (value * 100.0).round() / 100.0);
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".into();
    }
    if decimal_comma {
        text = text.replace('.', ",");
    }
    text
}

/// One report line; `size` is the font size in points.
pub(crate) struct ReportLine {
    text: String,
    size: f64,
}

impl ReportLine {
    fn new(text: impl Into<String>, size: f64) -> Self {
        Self {
            text: text.into(),
            size,
        }
    }
    fn body(text: impl Into<String>) -> Self {
        Self::new(text, 11.0)
    }
    fn blank() -> Self {
        Self::new("", 11.0)
    }
}

const PAGE_TOP: f64 = 800.0;
const PAGE_BOTTOM: f64 = 64.0;
const TEXT_WIDTH: f64 = 511.0;

/// Font-aware wrapping and embedded Unicode fonts keep reports readable at any length.
fn render_report(
    lines: &[ReportLine],
    path_source: &Path,
    footer: impl Fn(usize, usize) -> String,
) -> Result<Vec<u8>, ExportError> {
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
    let mut wrapped: Vec<(String, f64)> = Vec::new();
    for ReportLine { text, size } in lines {
        let size = *size;
        for paragraph in text.split('\n') {
            let mut line = String::new();
            let mut width = 0.0;
            for word in paragraph.split_whitespace() {
                let word_width: f64 = word.chars().map(|c| advance(c) * size / 1000.0).sum();
                if !line.is_empty() {
                    let space_width = advance(' ') * size / 1000.0;
                    if width + space_width + word_width > TEXT_WIDTH {
                        wrapped.push((std::mem::take(&mut line), size));
                        width = 0.0;
                    } else {
                        line.push(' ');
                        width += space_width;
                    }
                }
                // Break a word only when it cannot fit on an otherwise empty line.
                for c in word.chars() {
                    let character_width = advance(c) * size / 1000.0;
                    if width + character_width > TEXT_WIDTH && !line.is_empty() {
                        wrapped.push((std::mem::take(&mut line), size));
                        width = 0.0;
                    }
                    line.push(c);
                    width += character_width;
                }
            }
            wrapped.push((line, size));
        }
    }
    if wrapped.is_empty() {
        wrapped.push((String::new(), 11.0));
    }
    // Place lines top to bottom, starting a new page when the next line would cross the footer.
    let mut pages: Vec<Vec<(usize, f64)>> = vec![Vec::new()];
    let mut y = PAGE_TOP;
    for (index, (_, size)) in wrapped.iter().enumerate() {
        let height = size * 1.45;
        if y - height < PAGE_BOTTOM && !pages.last().is_some_and(Vec::is_empty) {
            pages.push(Vec::new());
            y = PAGE_TOP;
        }
        y -= height;
        pages
            .last_mut()
            .expect("at least one page")
            .push((index, y));
    }
    let page_count = pages.len();
    let footers: Vec<String> = (1..=page_count)
        .map(|page| footer(page, page_count))
        .collect();
    let mut glyphs = BTreeMap::new();
    let mut widths = BTreeSet::new();
    for c in wrapped
        .iter()
        .map(|(text, _)| text)
        .chain(footers.iter())
        .flat_map(|s| s.chars())
    {
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
    for (page, placed) in pages.iter().enumerate() {
        let mut operations = Vec::new();
        for (text, size, y) in placed
            .iter()
            .map(|&(index, y)| (&wrapped[index].0, wrapped[index].1, y))
            .chain(std::iter::once((&footers[page], 9.0, 40.0)))
        {
            if text.is_empty() {
                continue;
            }
            operations.extend([
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![Object::Name(b"F1".to_vec()), Object::Real(size as f32)],
                ),
                Operation::new("Td", vec![42.into(), Object::Real(y as f32)]),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        encode(text),
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

/// Name shown in exports: the local identity, or the alias once identities are purged.
fn display_name(student_name: Option<String>, alias: Option<&str>, labels: &Labels) -> String {
    student_name.unwrap_or_else(|| format!("{} {}", labels.alias, alias.unwrap_or("—")))
}

struct ReportCriterion {
    code: String,
    description: String,
    score_max: f64,
    score: f64,
    comment: Option<String>,
    levels: Vec<crate::models::PerformanceLevel>,
}

/// Build one confirmed submission's feedback report. Returns the PDF and the name shown.
fn feedback_report(
    conn: &Connection,
    submission_id: i64,
    path_source: &Path,
    language: ReportLanguage,
) -> Result<(Vec<u8>, String), ExportError> {
    let labels = language.labels();
    let comma = language.decimal_comma();
    #[allow(clippy::type_complexity)]
    let (status, alias, student_name, title, text, subject, grade_level, feedback, ai_assisted, confirmed_at): (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        bool,
        Option<String>,
    ) = conn.query_row(
        "SELECT s.status_pipeline, s.alias, m.student_name, a.title, a.text, r.subject, r.grade_level,
                s.feedback_final, s.feedback_ai_assisted,
                (SELECT MAX(confirmed_at) FROM results WHERE submission_id = s.id)
         FROM submissions s
         JOIN assignments a ON a.id = s.assignment_id
         JOIN rubrics r ON r.id = a.rubric_id
         LEFT JOIN alias_student_map m ON m.alias = s.alias
         WHERE s.id = ?1",
        [submission_id],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        },
    )?;
    if status != "grade_confirmed" {
        return Err(ExportError::Pdf(
            "confirm the grade before exporting feedback".into(),
        ));
    }
    let name = display_name(student_name, alias.as_deref(), labels);
    let mut stmt = conn.prepare(
        "SELECT cr.code, cr.description, cr.score_max, r.score_final, r.comment_teacher, cr.levels_json
         FROM results r JOIN criteria_rubric cr ON cr.id=r.criterion_id
         WHERE r.submission_id=?1 ORDER BY cr.sort_order",
    )?;
    let criteria: Vec<ReportCriterion> = stmt
        .query_map([submission_id], |r| {
            Ok(ReportCriterion {
                code: r.get(0)?,
                description: r.get(1)?,
                score_max: r.get(2)?,
                score: r.get(3)?,
                comment: r.get(4)?,
                levels: crate::commands::configuration::parse_levels(r.get(5)?),
            })
        })?
        .collect::<Result<_, _>>()?;
    let assignment = title.unwrap_or_else(|| {
        let first = text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        let mut short: String = first.chars().take(160).collect();
        if first.chars().count() > 160 {
            short.push('…');
        }
        short
    });
    let date =
        language.format_date(&confirmed_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()));
    let mut lines = vec![
        ReportLine::new(labels.report_title, 16.0),
        ReportLine::blank(),
        ReportLine::body(format!("{}: {name}", labels.student)),
        ReportLine::body(format!("{}: {assignment}", labels.assignment)),
        ReportLine::body(format!(
            "{}: {subject} · {}: {grade_level}",
            labels.subject, labels.grade_level
        )),
        ReportLine::body(format!("{}: {date}", labels.date)),
        ReportLine::blank(),
    ];
    for criterion in &criteria {
        lines.push(ReportLine::new(
            format!("{} · {}", criterion.code, criterion.description),
            11.5,
        ));
        let level = criterion
            .levels
            .iter()
            .find(|level| (level.score - criterion.score).abs() < 1e-9)
            .map(|level| format!(" · {}", level.label))
            .unwrap_or_default();
        lines.push(ReportLine::body(format!(
            "{}: {} / {}{level}",
            labels.score,
            format_number(criterion.score, comma),
            format_number(criterion.score_max, comma)
        )));
        if let Some(comment) = criterion
            .comment
            .as_deref()
            .filter(|c| !c.trim().is_empty())
        {
            lines.push(ReportLine::body(format!(
                "{}: {comment}",
                labels.teacher_comment
            )));
        }
        lines.push(ReportLine::blank());
    }
    let total: f64 = criteria.iter().map(|c| c.score).sum();
    let maximum: f64 = criteria.iter().map(|c| c.score_max).sum();
    lines.push(ReportLine::new(
        format!(
            "{}: {} / {} · {}: {}",
            labels.total,
            format_number(total, comma),
            format_number(maximum, comma),
            labels.grade_ten,
            format_number(grade_out_of_ten(total, maximum), comma)
        ),
        13.0,
    ));
    if let Some(feedback) = feedback.filter(|text| !text.trim().is_empty()) {
        lines.push(ReportLine::blank());
        lines.push(ReportLine::new(labels.feedback, 13.0));
        lines.push(ReportLine::body(feedback));
        if ai_assisted {
            lines.push(ReportLine::new(labels.ai_note, 9.0));
        }
    }
    let bytes = render_report(&lines, path_source, |page, count| {
        format!("{} {page} {} {count}", labels.page, labels.of)
    })?;
    Ok((bytes, name))
}

fn grade_out_of_ten(total: f64, maximum: f64) -> f64 {
    if maximum > 0.0 {
        total / maximum * 10.0
    } else {
        0.0
    }
}

pub fn generate_pdf_feedback(
    conn: &Connection,
    submission_id: i64,
    path_source: &Path,
    path_destination: &Path,
    language: ReportLanguage,
) -> Result<(), ExportError> {
    let (bytes, _) = feedback_report(conn, submission_id, path_source, language)?;
    write_atomic(path_destination, &bytes)?;
    // The sheet contains the student's name and grade; record that it left the workspace.
    conn.execute(
        "INSERT INTO logs_audit (submission_id, event, actor, payload_json) VALUES (?1, 'export_pdf_feedback', 'teacher', ?2)",
        rusqlite::params![submission_id, serde_json::json!({"files": 1}).to_string()],
    )?;
    Ok(())
}

/// A file name derived from the student's name that is safe on Windows.
fn report_file_stem(name: &str) -> String {
    let mut stem: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '\'') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .chars()
        .take(80)
        .collect();
    if stem.is_empty() {
        stem = "report".into();
    }
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.to_uppercase().as_str()) {
        stem.push('_');
    }
    stem
}

/// Outcome of a batch export: the reports written before any failure.
#[derive(Debug)]
pub struct BatchExport {
    pub written: usize,
    pub failure: Option<ExportError>,
}

/// Write one feedback sheet per confirmed submission of an assignment into `directory`.
/// Existing files are never replaced; a numeric suffix is added instead.
pub fn generate_pdf_batch(
    conn: &Connection,
    assignment_id: i64,
    path_source: &Path,
    directory: &Path,
    language: ReportLanguage,
) -> Result<BatchExport, ExportError> {
    let ids: Vec<i64> = conn
        .prepare(
            "SELECT id FROM submissions WHERE assignment_id=?1 AND status_pipeline='grade_confirmed' ORDER BY id",
        )?
        .query_map([assignment_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut written = 0;
    let outcome = ids
        .into_iter()
        .try_for_each(|id| -> Result<(), ExportError> {
            let (bytes, name) = feedback_report(conn, id, path_source, language)?;
            let stem = report_file_stem(&name);
            let mut path = directory.join(format!("{stem}.pdf"));
            let mut copy = 2;
            while path.exists() {
                path = directory.join(format!("{stem} ({copy}).pdf"));
                copy += 1;
            }
            write_atomic(&path, &bytes)?;
            written += 1;
            Ok(())
        });
    // Reports already written have left the workspace, so they are audited even when
    // a later one fails; the caller must persist this row before reporting the failure.
    if written > 0 || outcome.is_ok() {
        conn.execute(
            "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('export_pdf_feedback', 'teacher', ?1)",
            [serde_json::json!({"assignment_id": assignment_id, "files": written, "complete": outcome.is_ok()}).to_string()],
        )?;
    }
    Ok(BatchExport {
        written,
        failure: outcome.err(),
    })
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

/// Sort key for student names in Galician, Spanish and English lists: accents are
/// ignored and "ñ" follows "n".
fn name_sort_key(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            match c {
                'á' | 'à' | 'â' | 'ä' | 'ã' => "a",
                'é' | 'è' | 'ê' | 'ë' => "e",
                'í' | 'ì' | 'î' | 'ï' => "i",
                'ó' | 'ò' | 'ô' | 'ö' | 'õ' => "o",
                'ú' | 'ù' | 'û' | 'ü' => "u",
                'ç' => "c",
                'ñ' => "n~",
                other => return other.to_string(),
            }
            .to_string()
        })
        .collect()
}

/// Confirmed grades as a spreadsheet: one column per criterion, total, maximum and a
/// grade out of 10, sorted by student. UTF-8 with BOM so spreadsheet software keeps
/// accents; Galician and Spanish exports use semicolons and decimal commas.
pub fn generate_csv(
    conn: &Connection,
    assignment_id: i64,
    path_destination: &Path,
    language: ReportLanguage,
) -> Result<usize, ExportError> {
    let labels = language.labels();
    let comma = language.decimal_comma();
    let rubric_id: i64 = conn.query_row(
        "SELECT rubric_id FROM assignments WHERE id=?1",
        [assignment_id],
        |r| r.get(0),
    )?;
    let criteria: Vec<(i64, String, f64)> = conn
        .prepare("SELECT id, code, score_max FROM criteria_rubric WHERE rubric_id=?1 ORDER BY sort_order")?
        .query_map([rubric_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let submissions: Vec<(i64, Option<String>, Option<String>)> = conn
        .prepare(
            "SELECT s.id, s.alias, m.student_name FROM submissions s
             LEFT JOIN alias_student_map m ON m.alias = s.alias
             WHERE s.assignment_id = ?1 AND s.status_pipeline = 'grade_confirmed'",
        )?
        .query_map([assignment_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let mut rows: Vec<(String, Vec<String>)> = Vec::with_capacity(submissions.len());
    let maximum: f64 = criteria.iter().map(|(_, _, max)| max).sum();
    for (submission_id, alias, student_name) in submissions {
        let name = display_name(student_name, alias.as_deref(), labels);
        let mut record = vec![safe_csv_cell(&name)];
        let mut total = 0.0;
        for (criterion_id, _, _) in &criteria {
            let score: Option<f64> = conn
                .query_row(
                    "SELECT score_final FROM results WHERE submission_id=?1 AND criterion_id=?2",
                    [submission_id, *criterion_id],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            total += score.unwrap_or_default();
            record.push(score.map(|v| format_number(v, comma)).unwrap_or_default());
        }
        record.push(format_number(total, comma));
        record.push(format_number(maximum, comma));
        record.push(format_number(grade_out_of_ten(total, maximum), comma));
        rows.push((name_sort_key(&name), record));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    let mut writer = csv::WriterBuilder::new()
        .delimiter(if comma { b';' } else { b',' })
        .from_writer(Vec::new());
    let mut header = vec![labels.student.to_string()];
    header.extend(
        criteria.iter().map(|(_, code, max)| {
            safe_csv_cell(&format!("{code} (/{})", format_number(*max, comma)))
        }),
    );
    header.extend([
        labels.total.to_string(),
        labels.maximum.to_string(),
        labels.grade_ten.to_string(),
    ]);
    writer.write_record(&header)?;
    for (_, record) in &rows {
        writer.write_record(record)?;
    }
    writer.flush()?;
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend(
        writer
            .into_inner()
            .map_err(|e| ExportError::Io(e.into_error()))?,
    );
    write_atomic(path_destination, &bytes)?;

    conn.execute(
        "INSERT INTO logs_audit (event, actor, payload_json) VALUES ('export_csv', 'teacher', ?1)",
        [serde_json::json!({ "assignment_id": assignment_id, "rows": rows.len() }).to_string()],
    )?;

    Ok(rows.len())
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
    fn student_names_sort_ignoring_accents() {
        let mut names = ["Zoe", "Óscar", "Ñoño", "Nuria", "Álvaro", "Brais"];
        names.sort_by_key(|name| name_sort_key(name));
        assert_eq!(names, ["Álvaro", "Brais", "Nuria", "Ñoño", "Óscar", "Zoe"]);
    }

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
        review::confirm_grade(conn, submission_one, 0).unwrap();

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
        let rows = generate_csv(conn, assignment_id, &path_csv, ReportLanguage::Es).unwrap();
        assert_eq!(rows, 1);

        let bytes = std::fs::read(&path_csv).unwrap();
        assert!(bytes.starts_with(b"\xEF\xBB\xBF"));
        let content = String::from_utf8(bytes[3..].to_vec()).unwrap();
        let mut lines = content.lines();
        assert_eq!(
            lines.next().unwrap(),
            "Alumno/a;C1 (/10);Puntuación total;Máximo;Nota (0-10)"
        );
        assert_eq!(lines.next().unwrap(), "Maria Real;8,5;8,5;10;8,5");
        assert!(!content.contains("Other Student"));

        generate_csv(conn, assignment_id, &path_csv, ReportLanguage::En).unwrap();
        let english = std::fs::read_to_string(&path_csv).unwrap();
        assert!(english.contains("Maria Real,8.5,8.5,10,8.5"));
    }

    #[test]
    fn formats_numbers_for_spreadsheets_and_safe_file_names() {
        assert_eq!(format_number(0.1 + 0.2, false), "0.3");
        assert_eq!(format_number(7.0, true), "7");
        assert_eq!(format_number(6.666_666, true), "6,67");
        assert_eq!(format_number(-0.0001, false), "0");
        assert_eq!(report_file_stem("Ana Pérez/López:2"), "Ana Pérez_López_2");
        assert_eq!(report_file_stem("con"), "con_");
        assert_eq!(report_file_stem(" .. "), "report");
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
        conn.execute(
            "UPDATE submissions SET feedback_final='Bo traballo en xeral.', feedback_ai_assisted=1 WHERE id=?1",
            [submission_id],
        )
        .unwrap();
        review::confirm_grade(conn, submission_id, 0).unwrap();

        let path_source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/fonts/Roboto-Regular.ttf");
        let pdf_directory = tempdir().unwrap();
        let path_pdf = pdf_directory.path().join("feedback.pdf");

        generate_pdf_feedback(
            conn,
            submission_id,
            &path_source,
            &path_pdf,
            ReportLanguage::Gl,
        )
        .unwrap();

        let bytes = std::fs::read(&path_pdf).unwrap();
        let pdf = lopdf::Document::load_mem(&bytes).unwrap();
        let pages: Vec<u32> = pdf.get_pages().keys().copied().collect();
        let text = pdf.extract_text(&pages).unwrap();
        for expected in [
            "Informe de avaliación",
            "Student PDF",
            "Puntuación: 7 / 10",
            "buen trabajo",
            "Nota (0-10): 7",
            "Bo traballo en xeral.",
            "revisado polo profesorado",
            "Páxina 1 de 1",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in {text}");
        }
        let audited: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM logs_audit WHERE event='export_pdf_feedback' AND submission_id=?1",
                [submission_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(audited, 1);
        let batch_directory = tempdir().unwrap();
        assert_eq!(
            generate_pdf_batch(
                conn,
                assignment_id,
                &path_source,
                batch_directory.path(),
                ReportLanguage::Es
            )
            .unwrap()
            .written,
            1
        );
        generate_pdf_batch(
            conn,
            assignment_id,
            &path_source,
            batch_directory.path(),
            ReportLanguage::Es,
        )
        .unwrap();
        assert!(batch_directory.path().join("Student PDF.pdf").exists());
        assert!(batch_directory.path().join("Student PDF (2).pdf").exists());
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
        let mut lines = vec![ReportLine::new("Grading report - José Núñez", 16.0)];
        lines.extend((0..100).map(|i| {
            ReportLine::body(format!(
                "Criterion {i}: {}",
                "A very long teacher comment with accented words: también, física, revisión. "
                    .repeat(4)
            ))
        }));
        lines.push(ReportLine::body("END OF REPORT"));
        let bytes = render_report(&lines, &font, |page, count| {
            format!("Page {page} of {count}")
        })
        .unwrap();
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
