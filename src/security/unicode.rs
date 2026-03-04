// Unicode attack detection for SKILL.md content.
//
// Detects homoglyphs, right-to-left override (RLO), and mixed-script
// attacks that require character-level Unicode analysis beyond YARA.
//
// Invisible character detection (SD-002), general bidirectional control
// character detection (SD-003), and tag steganography detection (SD-006)
// have been migrated to YARA rules (rules/skilldo/unicode_attacks.yara).
//
// Based on the Trojan Source paper (Boucher & Anderson, 2021) and Unicode
// Technical Report #36 (Unicode Security Considerations).

use super::{line_number, snippet_at, Category, Finding, Severity};

/// Known homoglyph pairs: (non-Latin char, Latin lookalike).
/// Cyrillic and Greek characters that are visually identical to Latin.
const HOMOGLYPHS: &[(char, char)] = &[
    // Cyrillic → Latin
    ('а', 'a'),
    ('е', 'e'),
    ('о', 'o'),
    ('р', 'p'),
    ('с', 'c'),
    ('у', 'y'),
    ('х', 'x'),
    ('А', 'A'),
    ('В', 'B'),
    ('Е', 'E'),
    ('К', 'K'),
    ('М', 'M'),
    ('Н', 'H'),
    ('О', 'O'),
    ('Р', 'P'),
    ('С', 'C'),
    ('Т', 'T'),
    ('У', 'Y'),
    ('Х', 'X'),
    // Greek → Latin
    ('α', 'a'),
    ('β', 'b'),
    ('ε', 'e'),
    ('η', 'n'),
    ('ι', 'i'),
    ('κ', 'k'),
    ('ν', 'v'),
    ('ο', 'o'),
    ('ρ', 'p'),
    ('τ', 't'),
    ('υ', 'u'),
    ('χ', 'x'),
];

/// Scan content for unicode-based attacks.
pub fn scan(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    detect_homoglyphs(content, &mut findings);
    detect_rlo(content, &mut findings);
    detect_mixed_scripts(content, &mut findings);
    findings
}

fn detect_homoglyphs(content: &str, findings: &mut Vec<Finding>) {
    let mut found: Vec<(usize, char, char)> = Vec::new();

    for (byte_offset, ch) in content.char_indices() {
        for &(homoglyph, latin) in HOMOGLYPHS {
            if ch == homoglyph {
                found.push((byte_offset, homoglyph, latin));
            }
        }
    }

    if found.is_empty() {
        return;
    }

    let count = found.len();
    let samples: Vec<String> = found
        .iter()
        .take(5)
        .map(|(_, h, l)| format!("'{h}'→'{l}'"))
        .collect();

    let (first_offset, _, _) = found[0];
    let severity = if count > 3 {
        Severity::Critical
    } else {
        Severity::High
    };

    findings.push(Finding {
        rule_id: "SD-001".to_string(),
        severity,
        category: Category::UnicodeAttack,
        message: format!(
            "{count} homoglyph character(s) — visually identical to Latin but different Unicode: {}",
            samples.join(", ")
        ),
        line: line_number(content, first_offset),
        snippet: snippet_at(content, first_offset),
    });
}

/// Detect right-to-left override (U+202E) — the most dangerous bidi control char.
/// General bidi detection (SD-003) is handled by YARA.
fn detect_rlo(content: &str, findings: &mut Vec<Finding>) {
    for (byte_offset, ch) in content.char_indices() {
        if ch == '\u{202E}' {
            findings.push(Finding {
                rule_id: "SD-004".to_string(),
                severity: Severity::Critical,
                category: Category::UnicodeAttack,
                message:
                    "Right-to-left override (U+202E) — reverses displayed text to hide true content"
                        .into(),
                line: line_number(content, byte_offset),
                snippet: snippet_at(content, byte_offset),
            });
            return; // One finding is enough
        }
    }
}

fn detect_mixed_scripts(content: &str, findings: &mut Vec<Finding>) {
    let mut latin_count = 0u32;
    let mut cyrillic_count = 0u32;
    let mut greek_count = 0u32;

    for ch in content.chars() {
        if ('\u{0041}'..='\u{024F}').contains(&ch) {
            latin_count += 1;
        } else if ('\u{0400}'..='\u{04FF}').contains(&ch) {
            cyrillic_count += 1;
        } else if ('\u{0370}'..='\u{03FF}').contains(&ch) {
            greek_count += 1;
        }
    }

    // Latin + Cyrillic is the classic homoglyph attack vector
    if latin_count > 0 && cyrillic_count > 0 {
        findings.push(Finding {
            rule_id: "SD-005".to_string(),
            severity: Severity::High,
            category: Category::UnicodeAttack,
            message: format!(
                "Mixed Latin ({latin_count} chars) and Cyrillic ({cyrillic_count} chars) — common in homoglyph attacks"
            ),
            line: 1,
            snippet: String::new(),
        });
    }

    // Latin + Greek (less common but still a vector)
    if latin_count > 0 && greek_count > 2 {
        findings.push(Finding {
            rule_id: "SD-005".to_string(),
            severity: Severity::Medium,
            category: Category::UnicodeAttack,
            message: format!(
                "Mixed Latin ({latin_count} chars) and Greek ({greek_count} chars) — potential homoglyph vector"
            ),
            line: 1,
            snippet: String::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_findings_on_clean_ascii() {
        let findings = scan("Hello world, this is normal text.");
        assert!(findings.is_empty());
    }

    #[test]
    fn detects_cyrillic_homoglyphs() {
        // 'а' is Cyrillic, not Latin 'a'
        let findings = scan("import requests\nаpi_key = 'secret'");
        assert!(!findings.is_empty());
        assert!(findings.iter().any(|f| f.rule_id == "SD-001"));
    }

    #[test]
    fn detects_rlo_override() {
        let content = "display \u{202E}txet neddih this";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-004"));
    }

    #[test]
    fn detects_mixed_latin_cyrillic() {
        // Mix of real Latin and Cyrillic lookalikes
        let content = "The variable nаme has а Cyrillic а";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-005"));
    }

    #[test]
    fn high_severity_for_many_homoglyphs() {
        let content = "аеосух"; // 6 Cyrillic homoglyphs
        let findings = scan(content);
        let homoglyph = findings.iter().find(|f| f.rule_id == "SD-001").unwrap();
        assert_eq!(homoglyph.severity, Severity::Critical);
    }

    #[test]
    fn detects_mixed_latin_greek() {
        // Latin text with enough Greek chars (>2) to trigger
        let content = "The function uses αβγ parameters";
        let findings = scan(content);
        let mixed = findings
            .iter()
            .find(|f| f.rule_id == "SD-005" && f.message.contains("Greek"));
        assert!(mixed.is_some(), "should detect Latin+Greek mix");
        assert_eq!(mixed.unwrap().severity, Severity::Medium);
    }

    #[test]
    fn no_greek_finding_for_few_chars() {
        // Only 2 Greek chars — below the >2 threshold
        let content = "Use αβ notation";
        let findings = scan(content);
        assert!(
            !findings
                .iter()
                .any(|f| f.rule_id == "SD-005" && f.message.contains("Greek")),
            "2 Greek chars should not trigger"
        );
    }

    #[test]
    fn homoglyph_samples_capped_at_five() {
        // 8 distinct homoglyphs — samples list should cap at 5
        let content = "аеосухАВ test";
        let findings = scan(content);
        let f = findings.iter().find(|f| f.rule_id == "SD-001").unwrap();
        let arrow_count = f.message.matches('→').count();
        assert!(arrow_count <= 5, "samples should be capped at 5");
    }

    #[test]
    fn rlo_not_found_on_clean_text() {
        let findings = scan("perfectly normal text with no tricks");
        assert!(!findings.iter().any(|f| f.rule_id == "SD-004"));
    }
}
