// Unicode attack detection for SKILL.md content.
//
// Detects homoglyphs, invisible characters, bidirectional overrides,
// and mixed-script attacks that can hide malicious instructions.
//
// Based on the Trojan Source paper (Boucher & Anderson, 2021) and Unicode
// Technical Report #36 (Unicode Security Considerations).

use super::{Category, Finding, Severity};

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

/// Invisible/zero-width Unicode characters that can hide instructions.
const INVISIBLE_CHARS: &[(char, &str)] = &[
    ('\u{200B}', "zero-width space"),
    ('\u{200C}', "zero-width non-joiner"),
    ('\u{200D}', "zero-width joiner"),
    ('\u{FEFF}', "byte order mark"),
    ('\u{00AD}', "soft hyphen"),
    ('\u{2060}', "word joiner"),
    ('\u{2061}', "function application"),
    ('\u{2062}', "invisible times"),
    ('\u{2063}', "invisible separator"),
    ('\u{2064}', "invisible plus"),
    ('\u{180E}', "mongolian vowel separator"),
];

/// Bidirectional control characters (Trojan Source attack vectors).
const BIDI_CHARS: &[(char, &str)] = &[
    ('\u{200E}', "left-to-right mark"),
    ('\u{200F}', "right-to-left mark"),
    ('\u{202A}', "left-to-right embedding"),
    ('\u{202B}', "right-to-left embedding"),
    ('\u{202C}', "pop directional formatting"),
    ('\u{202D}', "left-to-right override"),
    ('\u{202E}', "right-to-left override"),
    ('\u{2066}', "left-to-right isolate"),
    ('\u{2067}', "right-to-left isolate"),
    ('\u{2068}', "first strong isolate"),
    ('\u{2069}', "pop directional isolate"),
];

/// Unicode tag characters (U+E0001-U+E007F) used for steganographic encoding.
/// The os-info-checker-es6 malware (2025) used these to hide payloads in npm packages.
const TAG_CHAR_RANGE: (u32, u32) = (0xE0001, 0xE007F);

/// Variation selectors (U+E0100-U+E01EF) that can encode hidden data.
const VARIATION_SEL_RANGE: (u32, u32) = (0xE0100, 0xE01EF);

/// Scan content for unicode-based attacks.
pub fn scan(content: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    detect_homoglyphs(content, &mut findings);
    detect_invisible_chars(content, &mut findings);
    detect_bidi_chars(content, &mut findings);
    detect_mixed_scripts(content, &mut findings);
    detect_tag_steganography(content, &mut findings);
    findings
}

fn line_number(content: &str, byte_offset: usize) -> usize {
    content[..byte_offset]
        .chars()
        .filter(|&c| c == '\n')
        .count()
        + 1
}

fn snippet_at(content: &str, byte_offset: usize) -> String {
    let start = content[..byte_offset]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = content[byte_offset..]
        .find('\n')
        .map(|i| byte_offset + i)
        .unwrap_or(content.len());
    content[start..end].chars().take(120).collect()
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
        rule_id: "SD-001",
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

fn detect_invisible_chars(content: &str, findings: &mut Vec<Finding>) {
    let mut found: Vec<(usize, char, &str)> = Vec::new();

    for (byte_offset, ch) in content.char_indices() {
        for &(invisible, name) in INVISIBLE_CHARS {
            if ch == invisible {
                found.push((byte_offset, invisible, name));
            }
        }
    }

    if found.is_empty() {
        return;
    }

    let count = found.len();
    let unique_types: Vec<&str> = {
        let mut types: Vec<&str> = found.iter().map(|(_, _, name)| *name).collect();
        types.sort_unstable();
        types.dedup();
        types
    };

    let (first_offset, _, _) = found[0];
    let severity = if count > 5 {
        Severity::Critical
    } else {
        Severity::High
    };

    findings.push(Finding {
        rule_id: "SD-002",
        severity,
        category: Category::UnicodeAttack,
        message: format!(
            "{count} invisible Unicode character(s) detected ({}) — may hide instructions between visible text",
            unique_types.join(", ")
        ),
        line: line_number(content, first_offset),
        snippet: snippet_at(content, first_offset),
    });
}

fn detect_bidi_chars(content: &str, findings: &mut Vec<Finding>) {
    let mut found: Vec<(usize, &str)> = Vec::new();
    let mut has_rlo = false;

    for (byte_offset, ch) in content.char_indices() {
        for &(bidi, name) in BIDI_CHARS {
            if ch == bidi {
                found.push((byte_offset, name));
                if ch == '\u{202E}' {
                    has_rlo = true;
                }
            }
        }
    }

    if found.is_empty() {
        return;
    }

    let count = found.len();
    let (first_offset, _) = found[0];

    findings.push(Finding {
        rule_id: "SD-003",
        severity: Severity::Critical,
        category: Category::UnicodeAttack,
        message: format!(
            "{count} bidirectional control character(s) — text may display differently than it executes (Trojan Source)"
        ),
        line: line_number(content, first_offset),
        snippet: snippet_at(content, first_offset),
    });

    if has_rlo {
        let rlo_offset = found
            .iter()
            .find(|(_, n)| *n == "right-to-left override")
            .map(|(o, _)| *o)
            .unwrap_or(0);
        findings.push(Finding {
            rule_id: "SD-004",
            severity: Severity::Critical,
            category: Category::UnicodeAttack,
            message:
                "Right-to-left override (U+202E) — reverses displayed text to hide true content"
                    .into(),
            line: line_number(content, rlo_offset),
            snippet: snippet_at(content, rlo_offset),
        });
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
            rule_id: "SD-005",
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
            rule_id: "SD-005",
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

/// Detect Unicode tag characters and variation selectors used for steganographic encoding.
/// These are supplemental plane characters that encode hidden ASCII payloads.
fn detect_tag_steganography(content: &str, findings: &mut Vec<Finding>) {
    let mut tag_count = 0u32;
    let mut var_sel_count = 0u32;
    let mut first_offset: Option<usize> = None;

    for (byte_offset, ch) in content.char_indices() {
        let cp = ch as u32;
        if cp >= TAG_CHAR_RANGE.0 && cp <= TAG_CHAR_RANGE.1 {
            tag_count += 1;
            if first_offset.is_none() {
                first_offset = Some(byte_offset);
            }
        } else if cp >= VARIATION_SEL_RANGE.0 && cp <= VARIATION_SEL_RANGE.1 {
            var_sel_count += 1;
            if first_offset.is_none() {
                first_offset = Some(byte_offset);
            }
        }
    }

    let total = tag_count + var_sel_count;
    if total == 0 {
        return;
    }

    let offset = first_offset.unwrap_or(0);
    findings.push(Finding {
        rule_id: "SD-006",
        severity: Severity::Critical,
        category: Category::UnicodeAttack,
        message: format!(
            "{total} Unicode steganography character(s) — {tag_count} tag chars (U+E0001-E007F), \
             {var_sel_count} variation selectors (U+E0100-E01EF) — may encode hidden payloads"
        ),
        line: line_number(content, offset),
        snippet: snippet_at(content, offset),
    });
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
    fn detects_invisible_zero_width_space() {
        let content = "normal\u{200B}text\u{200B}here";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-002"));
    }

    #[test]
    fn detects_bidi_override() {
        let content = "display \u{202E}txet neddih this";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-003"));
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
    fn bom_detected_as_invisible() {
        let content = "\u{FEFF}# Normal looking header";
        let findings = scan(content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-002"));
    }

    #[test]
    fn detects_tag_characters() {
        // U+E0001 (language tag) through U+E007F — used in os-info-checker-es6 attack
        let content = format!(
            "normal text{}{}{}more text",
            '\u{E0001}', '\u{E0041}', '\u{E007F}'
        );
        let findings = scan(&content);
        assert!(
            findings.iter().any(|f| f.rule_id == "SD-006"),
            "must detect tag steganography chars, got: {:?}",
            findings.iter().map(|f| &f.rule_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn detects_variation_selectors() {
        // U+E0100-U+E01EF — variation selectors supplement
        let content = format!("normal{}text{}here", '\u{E0100}', '\u{E0101}');
        let findings = scan(&content);
        assert!(findings.iter().any(|f| f.rule_id == "SD-006"));
    }

    #[test]
    fn no_tag_false_positive_on_clean_text() {
        let content = "This is perfectly normal ASCII text with no tricks.";
        let findings = scan(content);
        assert!(!findings.iter().any(|f| f.rule_id == "SD-006"));
    }
}
