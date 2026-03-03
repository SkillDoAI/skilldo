//! Security scanning for generated SKILL.md files.
//!
//! Three-layer static analysis for AI agent skill files:
//!
//! 1. **Pattern matching** — dangerous code patterns in examples (exec, creds, exfil)
//! 2. **Unicode attacks** — homoglyphs, invisible chars, bidi overrides, mixed scripts
//! 3. **Prompt injection** — instruction overrides, encoded payloads, markdown injection
//!
//! Detection categories informed by public security research including the Trojan Source
//! paper (Boucher & Anderson, 2021), OWASP LLM Top 10, and common adversarial skill
//! patterns documented in the AI agent security community.
//!
//! This is a fast first-pass filter. For semantic analysis of intent, pair with
//! LLM-based review (see `src/review/`).

pub mod injection;
pub mod patterns;
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
/// Runs all four detection layers and returns a consolidated report:
/// unicode attacks, injection patterns, dangerous code patterns, and YARA rules.
pub fn scan_skill(content: &str) -> ScanReport {
    let mut findings = Vec::new();

    findings.extend(unicode::scan(content));
    findings.extend(injection::scan(content));
    findings.extend(patterns::scan(content));

    // YARA: Cisco + SkillDo compiled-in rules
    match yara::YaraScanner::builtin() {
        Ok(scanner) => findings.extend(scanner.scan(content)),
        Err(e) => {
            // Rule compilation failure is a bug, not a runtime error.
            // Log it but don't block the pipeline — the other 3 layers still run.
            tracing::warn!("YARA scanner init failed: {e}");
        }
    }

    // Deduplicate cross-scanner findings by (rule_id, line)
    findings.sort_by(|a, b| a.rule_id.cmp(&b.rule_id).then(a.line.cmp(&b.line)));
    findings.dedup_by(|a, b| a.rule_id == b.rule_id && a.line == b.line);

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
        assert!(
            report.score == 0,
            "score should be 0 for this, got {}",
            report.score
        );
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(ids.contains(&"SD-206"), "must detect reverse shell");
        assert!(ids.contains(&"SD-202"), "must detect credential access");
    }

    #[test]
    fn fixture_evasive_05_alias_chain() {
        let content = include_str!("../../tests/fixtures/security/evasive-05-alias-chain.md");
        let report = run_fixture("evasive-05-alias-chain", content);
        assert!(!report.passed(), "alias chain exfil must fail");
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(
            ids.contains(&"SD-203") || ids.contains(&"SD-204"),
            "must detect obfuscation or persistence, found: {:?}",
            ids
        );
    }

    #[test]
    fn fixture_evasive_12_multi_stage() {
        let content = include_str!("../../tests/fixtures/security/evasive-12-multi-stage.md");
        let report = run_fixture("evasive-12-multi-stage", content);
        assert!(!report.passed(), "multi-stage attack must fail");
        let ids: Vec<&str> = report.findings.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(
            ids.contains(&"SD-201"),
            "must detect dynamic code execution"
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
