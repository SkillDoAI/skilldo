//! Security scanning for generated SKILL.md files.
//!
//! Three-layer static analysis for AI agent skill files:
//!
//! 1. **YARA rules** — SkillDo + Cisco rule packs: dangerous patterns, prompt injection,
//!    unicode attacks, credential harvesting, code execution (primary detection layer)
//! 2. **Unicode analysis** — Rust-level homoglyph detection, RLO override, mixed-script
//!    analysis (requires character-level logic beyond YARA)
//! 3. **Injection analysis** — markdown injection (HTML comments, image alt), base64-encoded
//!    instructions, exfiltration instruction detection (requires decode/context logic)
//!
//! YARA handles code-block filtering for prose-only rules (e.g. eval/subprocess in code
//! examples are legitimate documentation, not threats).
//!
//! Detection categories informed by public security research including the Trojan Source
//! paper (Boucher & Anderson, 2021), OWASP LLM Top 10, and common adversarial skill
//! patterns documented in the AI agent security community.
//!
//! This is a fast first-pass filter. For semantic analysis of intent, pair with
//! LLM-based review (see `src/review/`).

pub mod injection;
pub mod unicode;
pub mod yara;

use std::fmt;

/// Severity level for a security finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Category of the detected threat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    UnicodeAttack,
    PromptInjection,
    CodeExecution,
    CredentialAccess,
    DataExfiltration,
    Obfuscation,
    Persistence,
    PrivilegeEscalation,
    FilesystemWrite,
    ResourceAbuse,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnicodeAttack => write!(f, "unicode-attack"),
            Self::PromptInjection => write!(f, "prompt-injection"),
            Self::CodeExecution => write!(f, "code-execution"),
            Self::CredentialAccess => write!(f, "credential-access"),
            Self::DataExfiltration => write!(f, "data-exfiltration"),
            Self::Obfuscation => write!(f, "obfuscation"),
            Self::Persistence => write!(f, "persistence"),
            Self::PrivilegeEscalation => write!(f, "privilege-escalation"),
            Self::FilesystemWrite => write!(f, "filesystem-write"),
            Self::ResourceAbuse => write!(f, "resource-abuse"),
        }
    }
}

/// Clamp a byte offset to the nearest valid UTF-8 char boundary.
pub(super) fn to_char_boundary(content: &str, mut offset: usize) -> usize {
    offset = offset.min(content.len());
    while offset > 0 && !content.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Return the 1-based line number for a byte offset in content.
pub(super) fn line_number(content: &str, byte_offset: usize) -> usize {
    let safe_offset = to_char_boundary(content, byte_offset);
    content[..safe_offset]
        .chars()
        .filter(|&c| c == '\n')
        .count()
        + 1
}

/// Extract a snippet (up to 120 chars) from the line containing byte_offset.
pub(super) fn snippet_at(content: &str, byte_offset: usize) -> String {
    let safe_offset = to_char_boundary(content, byte_offset);
    let start = content[..safe_offset]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = content[safe_offset..]
        .find('\n')
        .map(|i| safe_offset + i)
        .unwrap_or(content.len());
    content[start..end].chars().take(120).collect()
}

/// Deduplicate findings by (rule_id, line).
pub(super) fn dedup_findings(findings: &mut Vec<Finding>) {
    findings.sort_by(|a, b| a.rule_id.cmp(&b.rule_id).then(a.line.cmp(&b.line)));
    findings.dedup_by(|a, b| a.rule_id == b.rule_id && a.line == b.line);
}

/// A single security finding.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Finding {
    pub rule_id: String,
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub line: usize,
    pub snippet: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} ({}): {} [line {}]",
            self.rule_id, self.severity, self.category, self.message, self.line
        )
    }
}

/// Result of scanning a SKILL.md file.
#[derive(Debug)]
pub struct ScanReport {
    pub findings: Vec<Finding>,
    pub score: u8,
}

impl ScanReport {
    /// Whether the scan passed (no high/critical findings).
    pub fn passed(&self) -> bool {
        !self.findings.iter().any(|f| f.severity >= Severity::High)
    }

    /// Count findings by severity.
    #[allow(dead_code)]
    pub fn count_by_severity(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }
}

/// Scan a SKILL.md content string for security issues.
///
/// Runs all three detection layers and returns a consolidated report:
/// YARA rules, unicode analysis, and injection analysis.
pub fn scan_skill(content: &str) -> ScanReport {
    let mut findings = Vec::new();

    findings.extend(unicode::scan(content));
    findings.extend(injection::scan(content));

    // YARA: Cisco + SkillDo compiled-in rules
    match yara::YaraScanner::builtin() {
        Ok(scanner) => findings.extend(scanner.scan(content)),
        Err(e) => {
            // YARA is the primary security gate — fail closed.
            tracing::error!("YARA scanner init failed: {e}");
            findings.push(Finding {
                rule_id: "SD-000".to_string(),
                severity: Severity::Critical,
                category: Category::Obfuscation,
                message: format!("YARA scanner init failed: {e}"),
                line: 1,
                snippet: String::new(),
            });
        }
    }

    // Deduplicate cross-scanner findings by (rule_id, line)
    dedup_findings(&mut findings);

    // Score: start at 100, deduct per finding weighted by severity
    let deductions: i32 = findings
        .iter()
        .map(|f| match f.severity {
            Severity::Critical => 30,
            Severity::High => 15,
            Severity::Medium => 5,
            Severity::Low => 2,
            Severity::Info => 0,
        })
        .sum();

    let score = (100i32 - deductions).clamp(0, 100) as u8;

    ScanReport { findings, score }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_skill_scores_100() {
        let content = r#"---
name: requests
version: "2.31.0"
language: python
---

# requests

HTTP library for Python.

## Quick Start

```python
import requests
response = requests.get("https://httpbin.org/get")
print(response.json())
```
"#;
        let report = scan_skill(content);
        assert_eq!(report.score, 100);
        assert!(report.passed());
        assert!(report.findings.is_empty());
    }

    /// Helper: run scan_skill and print a summary for fixture analysis.
    fn run_fixture(name: &str, content: &str) -> ScanReport {
        let report = scan_skill(content);
        eprintln!(
            "\n=== {} === score={} findings={} passed={}",
            name,
            report.score,
            report.findings.len(),
            report.passed()
        );
        for f in &report.findings {
            eprintln!("  {}", f);
        }
        report
    }

    // --- Shared helper unit tests ---

    #[test]
    fn to_char_boundary_ascii() {
        let s = "hello world";
        assert_eq!(to_char_boundary(s, 5), 5);
        assert_eq!(to_char_boundary(s, 0), 0);
        assert_eq!(to_char_boundary(s, 100), s.len());
    }

    #[test]
    fn to_char_boundary_multibyte() {
        let s = "café"; // é is 2 bytes (0xC3 0xA9)
        let e_start = s.find('é').unwrap(); // byte 3
                                            // Offset inside the é (byte 4) should clamp back to byte 3
        assert_eq!(to_char_boundary(s, e_start + 1), e_start);
        // Exact boundary stays
        assert_eq!(to_char_boundary(s, e_start), e_start);
    }

    #[test]
    fn line_number_basics() {
        let content = "line1\nline2\nline3\n";
        assert_eq!(line_number(content, 0), 1); // start of line1
        assert_eq!(line_number(content, 6), 2); // start of line2
        assert_eq!(line_number(content, 12), 3); // start of line3
    }

    #[test]
    fn line_number_single_line() {
        assert_eq!(line_number("no newlines here", 5), 1);
    }

    #[test]
    fn snippet_at_extracts_line() {
        let content = "first line\nsecond line\nthird line";
        let snippet = snippet_at(content, 12); // 's' of "second"
        assert_eq!(snippet, "second line");
    }

    #[test]
    fn snippet_at_first_line() {
        let content = "hello\nworld";
        assert_eq!(snippet_at(content, 0), "hello");
    }

    #[test]
    fn snippet_at_last_line_no_trailing_newline() {
        let content = "aaa\nbbb";
        assert_eq!(snippet_at(content, 4), "bbb");
    }

    #[test]
    fn dedup_findings_removes_duplicates() {
        let mut findings = vec![
            Finding {
                rule_id: "SD-001".into(),
                severity: Severity::High,
                category: Category::UnicodeAttack,
                message: "dup1".into(),
                line: 5,
                snippet: String::new(),
            },
            Finding {
                rule_id: "SD-001".into(),
                severity: Severity::High,
                category: Category::UnicodeAttack,
                message: "dup2".into(),
                line: 5,
                snippet: String::new(),
            },
            Finding {
                rule_id: "SD-002".into(),
                severity: Severity::Medium,
                category: Category::UnicodeAttack,
                message: "different rule".into(),
                line: 5,
                snippet: String::new(),
            },
        ];
        dedup_findings(&mut findings);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].rule_id, "SD-001");
        assert_eq!(findings[1].rule_id, "SD-002");
    }

    #[test]
    fn dedup_findings_keeps_different_lines() {
        let mut findings = vec![
            Finding {
                rule_id: "SD-001".into(),
                severity: Severity::High,
                category: Category::UnicodeAttack,
                message: "line3".into(),
                line: 3,
                snippet: String::new(),
            },
            Finding {
                rule_id: "SD-001".into(),
                severity: Severity::High,
                category: Category::UnicodeAttack,
                message: "line7".into(),
                line: 7,
                snippet: String::new(),
            },
        ];
        dedup_findings(&mut findings);
        assert_eq!(
            findings.len(),
            2,
            "same rule on different lines should both remain"
        );
    }

    // --- Malicious fixtures: MUST detect ---

    #[test]
    fn fixture_malicious_skill() {
        let content = include_str!("../../tests/fixtures/security/malicious-skill.md");
        let report = run_fixture("malicious-skill", content);
        assert!(!report.passed(), "malicious skill must fail");
        assert!(
            report.score < 30,
            "score should be very low, got {}",
            report.score
        );
        // Must detect: system tag, instruction override, credential exfil, secrecy
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(ids.contains(&"SD-101"), "must detect <system> tag");
        assert!(ids.contains(&"SD-102"), "must detect instruction override");
        assert!(ids.contains(&"SD-104"), "must detect secrecy demand");
    }

    #[test]
    fn fixture_evasive_03_prompt_subtle() {
        let content = include_str!("../../tests/fixtures/security/evasive-03-prompt-subtle.md");
        let report = run_fixture("evasive-03-prompt-subtle", content);
        assert!(!report.passed(), "evasive prompt injection must fail");
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(
            ids.contains(&"SD-110"),
            "must detect HTML comment injection or image alt injection"
        );
    }

    #[test]
    fn fixture_evasive_06_unicode() {
        let content = include_str!("../../tests/fixtures/security/evasive-06-unicode-injection.md");
        let report = run_fixture("evasive-06-unicode", content);
        assert!(!report.passed(), "unicode injection must fail");
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(
            ids.contains(&"SD-001") || ids.contains(&"SD-002") || ids.contains(&"SD-003"),
            "must detect unicode attacks, found: {:?}",
            ids
        );
    }

    #[test]
    fn fixture_evasive_10_roleplay() {
        let content = include_str!("../../tests/fixtures/security/evasive-10-roleplay.md");
        let report = run_fixture("evasive-10-roleplay", content);
        assert!(!report.passed(), "roleplay injection must fail");
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(ids.contains(&"SD-107"), "must detect roleplay/jailbreak");
    }

    #[test]
    fn fixture_evasive_08_reverse_shell() {
        let content = include_str!("../../tests/fixtures/security/evasive-08-reverse-shell.md");
        let report = run_fixture("evasive-08-reverse-shell", content);
        assert!(!report.passed(), "reverse shell must fail");
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(ids.contains(&"SD-206"), "must detect reverse shell");
        // SD-202 (.ssh/ path) is in a code block — not detected by pattern/YARA
        // scanning (scan_code_blocks: false). LLM review agent handles this.
    }

    #[test]
    fn fixture_evasive_05_alias_chain() {
        let content = include_str!("../../tests/fixtures/security/evasive-05-alias-chain.md");
        let report = run_fixture("evasive-05-alias-chain", content);
        assert!(!report.passed(), "alias chain exfil must fail");
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        // SD-203/SD-204 patterns are in code blocks — not detected by prose-only scanning.
        // SD-111 (base64 injection) detects the encoded payload in the .zshrc line.
        assert!(
            ids.contains(&"SD-111"),
            "must detect base64-encoded payload, found: {:?}",
            ids
        );
    }

    #[test]
    fn fixture_evasive_12_multi_stage() {
        let content = include_str!("../../tests/fixtures/security/evasive-12-multi-stage.md");
        let report = run_fixture("evasive-12-multi-stage", content);
        // This fixture hides ALL malicious patterns in code blocks.
        // Pattern/YARA scanning skips normal-API patterns in code blocks to
        // avoid false positives on legitimate library documentation.
        // Detection requires the LLM review agent (semantic understanding).
        assert!(
            report.passed(),
            "multi-stage in code blocks evades pattern scanning (by design), got: {:?}",
            report
                .findings
                .iter()
                .map(|f| f.rule_id.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn fixture_evasive_01_base64() {
        let content = include_str!("../../tests/fixtures/security/evasive-01-base64-payload.md");
        let report = run_fixture("evasive-01-base64", content);
        assert!(!report.passed(), "base64 payload must fail");
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(
            ids.contains(&"SD-111"),
            "must detect base64-encoded instructions"
        );
    }

    // --- Clean fixtures: MUST pass ---

    #[test]
    fn fixture_clean_skill() {
        let content = include_str!("../../tests/fixtures/security/clean-skill.md");
        let report = run_fixture("clean-skill", content);
        assert!(
            report.passed(),
            "clean skill must pass, findings: {:?}",
            report
                .findings
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
        );
        assert!(
            report.score >= 90,
            "clean skill score should be >= 90, got {}",
            report.score
        );
    }

    #[test]
    fn fixture_legit_api_skill() {
        let content = include_str!("../../tests/fixtures/security/legit-api-skill.md");
        let report = run_fixture("legit-api-skill", content);
        assert!(
            report.passed(),
            "legit API skill must pass, findings: {:?}",
            report
                .findings
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
        );
        assert!(
            report.score >= 80,
            "legit skill score should be >= 80, got {}",
            report.score
        );
    }
}
