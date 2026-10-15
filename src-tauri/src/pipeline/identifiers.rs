//! Detection of direct identifiers (Milestone 4): validated rules and regexes
//! (DNI/NIE with control digit, email, phone, IBAN) + matching
//! against the class roster supplied by the teacher.
//!
//! Scope note: the plan also envisioned a local NER model
//! (ONNX via `ort`) for indirect identifiers ("I am the only student
//! who..."). This is left out of this phase: integrating `ort` adds another
//! native dependency (onnxruntime). Automatic detection is insufficient on
//! its own; the mandatory human review in `pipeline::redaction` remains the
//! safeguard. Adding NER later would only add candidates to that review.

use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IdentifierCandidate {
    pub kind: String,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

const DNI_CONTROL_LETTERS: &str = "TRWAGMYFPDXBNJZSQVHLCKE";

fn letter_dni_valid(number: u32, letter: char) -> bool {
    DNI_CONTROL_LETTERS
        .chars()
        .nth((number % 23) as usize)
        .map(|l| l.eq_ignore_ascii_case(&letter))
        .unwrap_or(false)
}

static RE_DNI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{8})[-\s]?([A-Za-z])\b").unwrap());
static RE_NIE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([XYZxyz])[-\s]?(\d{7})[-\s]?([A-Za-z])\b").unwrap());
static RE_EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap());
// Spanish mobile and landline numbers, optionally with +34 and grouped as
// 666123456, 666 123 456, 666 12 34 56 or 981-12-34-56.
static RE_PHONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+34[\s.-]?)?\b[6789]\d{2}(?:(?:[\s.-]?\d{3}){2}|(?:[\s.-]?\d{2}){3})\b")
        .unwrap()
});
static RE_IBAN: LazyLock<Regex> = LazyLock::new(|| {
    // Structural check (ES + 22 digits, with optional separators);
    // The mod-97 control digit is not validated; this is sufficient to flag the
    // candidate for human review, not to certify that it is a real IBAN.
    Regex::new(r"\bES\d{2}[ -]?(?:\d{4}[ -]?){4}\d{2}\b").unwrap()
});

/// Direct identifiers detectable by rule, with checksum validation
/// when applicable (DNI/NIE).
pub fn detect_by_rule(text: &str) -> Vec<IdentifierCandidate> {
    let mut candidates = Vec::new();

    for m in RE_DNI.captures_iter(text) {
        let total = m.get(0).unwrap();
        let number: u32 = m[1].parse().unwrap_or(0);
        let letter = m[2].chars().next().unwrap_or(' ');
        if letter_dni_valid(number, letter) {
            candidates.push(IdentifierCandidate {
                kind: "dni".into(),
                text: total.as_str().to_string(),
                start: total.start(),
                end: total.end(),
            });
        }
    }

    for m in RE_NIE.captures_iter(text) {
        let total = m.get(0).unwrap();
        let prefix = match m[1].to_ascii_uppercase().as_str() {
            "X" => 0,
            "Y" => 1,
            "Z" => 2,
            _ => continue,
        };
        let remaining_digits: u32 = m[2].parse().unwrap_or(0);
        let number = prefix * 10_000_000 + remaining_digits;
        let letter = m[3].chars().next().unwrap_or(' ');
        if letter_dni_valid(number, letter) {
            candidates.push(IdentifierCandidate {
                kind: "nie".into(),
                text: total.as_str().to_string(),
                start: total.start(),
                end: total.end(),
            });
        }
    }

    for m in RE_EMAIL.find_iter(text) {
        candidates.push(IdentifierCandidate {
            kind: "email".into(),
            text: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        });
    }

    for m in RE_PHONE.find_iter(text) {
        candidates.push(IdentifierCandidate {
            kind: "phone".into(),
            text: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        });
    }

    for m in RE_IBAN.find_iter(text) {
        candidates.push(IdentifierCandidate {
            kind: "iban".into(),
            text: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        });
    }

    candidates
}

/// Find each full roster name and its parts in `text`, including cases where
/// a student uses only their given name. Matching is case insensitive and
/// literal, so OCR errors are not tolerated.
/// Name particles that are also ordinary words; they are not matched on their own.
pub(crate) const NAME_PARTICLES: &[&str] = &[
    "de", "da", "do", "das", "dos", "del", "la", "las", "los", "y", "e", "i", "van", "von",
];

/// Roster names in the text. Full names match in any case and across line breaks.
/// A single name part matches only as a whole capitalized word: with a whole class
/// roster, parts such as "Mar", "Vila" or "Ana" would otherwise flag ordinary words
/// ("o mar", "a vila", "análise").
pub fn detect_by_roster(text: &str, roster: &[String]) -> Vec<IdentifierCandidate> {
    let mut candidates = Vec::new();
    // (term, is a full name); a term that is both keeps the full-name rule.
    let mut terms: std::collections::BTreeMap<String, (String, bool)> = Default::default();
    for full_name in roster {
        let cleaned = full_name.split_whitespace().collect::<Vec<_>>().join(" ");
        if cleaned.is_empty() {
            continue;
        }
        for part in cleaned.split(' ') {
            if part.chars().count() >= 3 && !NAME_PARTICLES.contains(&part.to_lowercase().as_str())
            {
                terms
                    .entry(part.to_lowercase())
                    .or_insert((part.to_string(), false));
            }
        }
        terms.insert(cleaned.to_lowercase(), (cleaned, true));
    }
    let mut terms: Vec<(String, bool)> = terms.into_values().collect();
    // Longer terms first, so a full name is detected
    // as a single candidate instead of being split into its parts.
    terms.sort_by_key(|(term, _)| std::cmp::Reverse(term.len()));

    // Match on the original text: lowercasing can change UTF-8 lengths (for example
    // the ohm sign), so offsets found in a lowercased copy are not valid here.
    for (term, full_name) in &terms {
        let words: Vec<String> = term.split(' ').map(regex::escape).collect();
        let Ok(pattern) = regex::RegexBuilder::new(&format!(r"\b{}\b", words.join(r"\s+")))
            .case_insensitive(true)
            .build()
        else {
            continue;
        };
        for found in pattern.find_iter(text) {
            if !full_name
                && !found
                    .as_str()
                    .chars()
                    .next()
                    .is_some_and(char::is_uppercase)
            {
                continue;
            }
            let (start, end) = (found.start(), found.end());
            let overlaps = candidates
                .iter()
                .any(|c: &IdentifierCandidate| start < c.end && end > c.start);
            if !overlaps {
                candidates.push(IdentifierCandidate {
                    kind: "name_roster".into(),
                    text: found.as_str().to_string(),
                    start,
                    end,
                });
            }
        }
    }

    candidates
}

pub fn detect_all(text: &str, roster: &[String]) -> Vec<IdentifierCandidate> {
    let mut candidates = detect_by_rule(text);
    candidates.extend(detect_by_roster(text, roster));
    candidates.sort_by_key(|c| c.start);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dni_valid_and_rejects_invalid() {
        // 12345678 % 23 = 14 -> control letter 'Z'.
        let candidates = detect_by_rule("My national ID is 12345678Z.");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, "dni");

        let invalid_candidates = detect_by_rule("My national ID is 12345678A.");
        assert!(invalid_candidates.is_empty());
    }

    #[test]
    fn detects_email_and_phone() {
        let candidates = detect_by_rule("Contact: maria.example@invalid.test or at 611223344.");
        let kinds: Vec<_> = candidates.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"email"));
        assert!(kinds.contains(&"phone"));
    }

    #[test]
    fn detects_grouped_and_prefixed_phone_numbers() {
        for text in [
            "Ring 666 123 456 today",
            "Ring 666 12 34 56 today",
            "Ring +34 666123456 today",
            "Ring 981-12-34-56 today",
        ] {
            let phones: Vec<_> = detect_by_rule(text)
                .into_iter()
                .filter(|c| c.kind == "phone")
                .collect();
            assert_eq!(phones.len(), 1, "{text}");
            assert!(phones[0].text.ends_with(['6', '5']), "{text}");
        }
        assert!(detect_by_rule("Answer: 12 m/s and 4096 bytes")
            .iter()
            .all(|c| c.kind != "phone"));
    }

    #[test]
    fn roster_offsets_stay_valid_when_lowercasing_changes_lengths() {
        // U+212A KELVIN SIGN lowercases to a one-byte "k".
        let roster = vec!["Ana Pérez".to_string()];
        let text = "T = 300 \u{212A}. ANA PÉREZ signs.";
        let candidates = detect_by_roster(text, &roster);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].text, "ANA PÉREZ");
        assert_eq!(&text[candidates[0].start..candidates[0].end], "ANA PÉREZ");
    }

    #[test]
    fn detects_full_roster_name_without_fragmenting() {
        let roster = vec!["Maria Synthetic Lopez Example".to_string()];
        let text = "Name: Maria Synthetic Lopez Example\nResponse: ...";
        let candidates = detect_by_roster(text, &roster);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].text, "Maria Synthetic Lopez Example");
    }

    #[test]
    fn roster_parts_need_a_capitalized_whole_word() {
        let roster = vec![
            "Mar Vila del Río".to_string(),
            "Ana Souto".to_string(),
            "Uxía Pereira".to_string(),
        ];
        let text = "Na análise do mar e da vila, Mar cita a ANA SOUTO.\nUxía\nPereira del norte.";
        let candidates = detect_by_roster(text, &roster);
        let found: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(found, ["Uxía\nPereira", "ANA SOUTO", "Mar"]);
    }

    #[test]
    fn detects_given_name_when_it_is_the_only_match() {
        let roster = vec!["Juan Perez Garcia".to_string()];
        let text = "Hello, I am Juan and this is my response.";
        let candidates = detect_by_roster(text, &roster);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].text, "Juan");
    }

    #[test]
    fn detects_identifiers_in_synthetic_student_two() {
        let text = "Name: Maria Synthetic Lopez Example\nNational ID: 12345678Z (fictional)\n\
                     Email: maria.synthetic.example@invalid.test\n\nResponse...";
        let roster = vec!["Maria Synthetic Lopez Example".to_string()];
        let candidates = detect_all(text, &roster);
        let kinds: Vec<_> = candidates.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"name_roster"));
        assert!(kinds.contains(&"dni"));
        assert!(kinds.contains(&"email"));
    }

    /// Document that `detect_all` DOES return overlapping candidates
    /// (here, an email containing fragments of a roster name) —
    /// reason why `pipeline::redaction::confirm` has to
    /// merge spans before redacting instead of assuming they come
    /// disjoint. See the redaction overlap regression test.
    #[test]
    fn detect_all_can_return_overlapping_candidates() {
        let text = "Name: Maria Synthetic Lopez Example\nNational ID: 12345678Z (fictional)\n\
                     Email: Maria.Synthetic.Example@invalid.test\n\nResponse...";
        let roster = vec!["Maria Synthetic Lopez Example".to_string()];
        let candidates = detect_all(text, &roster);
        let has_overlap = candidates.iter().enumerate().any(|(i, a)| {
            candidates
                .iter()
                .enumerate()
                .any(|(j, b)| i != j && a.start < b.end && a.end > b.start)
        });
        assert!(has_overlap, "expected an actual overlap between candidates");
    }
}
