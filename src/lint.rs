use anyhow::Result;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LintIssue {
    pub severity: Severity,
    pub category: String,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,   // Must fix
    Warning, // Should fix
    Info,    // Nice to have
}

pub struct SkillLinter;

impl Default for SkillLinter {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillLinter {
    pub fn new() -> Self {
        Self
    }

    /// Lint a SKILL.md file content
    pub fn lint(&self, content: &str) -> Result<Vec<LintIssue>> {
        let mut issues = Vec::new();

        // Check frontmatter
        issues.extend(self.check_frontmatter(content));

        // Check structure
        issues.extend(self.check_structure(content));

        // Check content quality
        issues.extend(self.check_content(content));

        // Check for LLM degeneration
        issues.extend(self.check_degeneration(content));

        Ok(issues)
    }

    fn check_frontmatter(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();

        // Parse frontmatter
        let frontmatter = self.extract_frontmatter(content);

        if frontmatter.is_empty() {
            issues.push(LintIssue {
                severity: Severity::Error,
                category: "frontmatter".to_string(),
                message: "Missing frontmatter (---...---)".to_string(),
                suggestion: Some(
                    "Add frontmatter with name, description, version, ecosystem".to_string(),
                ),
            });
            return issues;
        }

        // Required fields
        let required_fields = vec!["name", "description", "version", "ecosystem"];
        for field in required_fields {
            if !frontmatter.contains_key(field) {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "frontmatter".to_string(),
                    message: format!("Missing required field: {}", field),
                    suggestion: Some(format!("Add '{}: <value>' to frontmatter", field)),
                });
            }
        }

        // Check for "unknown" version
        if let Some(version) = frontmatter.get("version") {
            if version == "unknown" {
                issues.push(LintIssue {
                    severity: Severity::Warning,
                    category: "frontmatter".to_string(),
                    message: "Version is 'unknown' — version extraction failed".to_string(),
                    suggestion: Some(
                        "Try --version-from git-tag or --version <version> to set explicitly"
                            .to_string(),
                    ),
                });
            }
        }

        // Check for license field (tessl.io requirement)
        if !frontmatter.contains_key("license") {
            issues.push(LintIssue {
                severity: Severity::Warning,
                category: "frontmatter".to_string(),
                message: "'license' field is missing".to_string(),
                suggestion: Some(
                    "Add 'license: MIT' (or appropriate license) to frontmatter".to_string(),
                ),
            });
        }

        issues
    }

    fn check_structure(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();

        // Required sections
        let required_sections = vec!["## Imports", "## Core Patterns", "## Pitfalls"];

        for section in required_sections {
            if !content.contains(section) {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "structure".to_string(),
                    message: format!("Missing required section: {}", section),
                    suggestion: Some(format!("Add a '{}' section", section)),
                });
            }
        }

        issues
    }

    fn check_content(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();

        // Check for code blocks
        if !content.contains("```") {
            issues.push(LintIssue {
                severity: Severity::Error,
                category: "content".to_string(),
                message: "No code examples found".to_string(),
                suggestion: Some("Add code examples in ```python blocks".to_string()),
            });
        }

        // Check minimum length
        if content.len() < 1000 {
            issues.push(LintIssue {
                severity: Severity::Warning,
                category: "content".to_string(),
                message: format!("Content is very short ({} chars)", content.len()),
                suggestion: Some("Consider adding more examples and explanations".to_string()),
            });
        }

        // Check for Pitfalls with Wrong/Right examples
        if content.contains("## Pitfalls") {
            let has_wrong = content.contains("### Wrong") || content.contains("### ❌");
            let has_right = content.contains("### Right") || content.contains("### ✅");

            if !has_wrong || !has_right {
                issues.push(LintIssue {
                    severity: Severity::Info,
                    category: "content".to_string(),
                    message: "Pitfalls section should include 'Wrong' and 'Right' examples"
                        .to_string(),
                    suggestion: Some(
                        "Use ### Wrong: and ### Right: subsections in Pitfalls".to_string(),
                    ),
                });
            }

            // Check for duplicate Wrong/Right examples
            issues.extend(self.check_duplicate_examples(content));
        }

        issues
    }

    fn check_duplicate_examples(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();

        // Extract code blocks from Pitfalls section
        if let Some(pitfalls_start) = content.find("## Pitfalls") {
            let pitfalls_section = &content[pitfalls_start..];

            // Find the next section (or end of document)
            let pitfalls_end = pitfalls_section[13..] // Skip "## Pitfalls"
                .find("\n## ")
                .map(|pos| pos + 13)
                .unwrap_or(pitfalls_section.len());

            let pitfalls_content = &pitfalls_section[..pitfalls_end];

            // Extract all code blocks
            let code_blocks: Vec<&str> = pitfalls_content
                .split("```")
                .skip(1) // Skip text before first block
                .step_by(2) // Take every other element (the code blocks)
                .map(|block| {
                    // Remove language identifier (e.g., "python\n")
                    block
                        .trim_start_matches(|c: char| c.is_alphanumeric() || c == '-')
                        .trim()
                })
                .collect();

            // Check for consecutive identical code blocks (Wrong vs Right pattern)
            for i in 0..code_blocks.len().saturating_sub(1) {
                if code_blocks[i] == code_blocks[i + 1] && !code_blocks[i].is_empty() {
                    issues.push(LintIssue {
                        severity: Severity::Error,
                        category: "content".to_string(),
                        message: "Found identical 'Wrong' and 'Right' examples in Pitfalls section".to_string(),
                        suggestion: Some("Ensure 'Wrong' and 'Right' examples show different code - the examples should demonstrate what NOT to do vs what TO do".to_string()),
                    });
                    break; // Only report once
                }
            }
        }

        issues
    }

    fn check_degeneration(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Track code block boundaries
        let mut in_code_block = false;
        let mut code_block_lines: Vec<bool> = Vec::with_capacity(lines.len());
        for line in &lines {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
            }
            code_block_lines.push(in_code_block);
        }

        // Check 1: Repeated line prefix (catches multi-line degeneration)
        // If 10+ consecutive non-code lines share a prefix of >= 20 chars, flag it.
        let min_prefix = 20;
        let min_run = 10;
        let mut i = 0;
        while i < lines.len() {
            if code_block_lines[i] || lines[i].len() < min_prefix {
                i += 1;
                continue;
            }
            // Find a safe char boundary at or before min_prefix to avoid splitting multi-byte chars
            let mut end = min_prefix;
            while end > 0 && !lines[i].is_char_boundary(end) {
                end -= 1;
            }
            let prefix = &lines[i][..end];
            let mut run = 1;
            while i + run < lines.len()
                && !code_block_lines[i + run]
                && lines[i + run].starts_with(prefix)
            {
                run += 1;
            }
            if run >= min_run {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "degeneration".to_string(),
                    message: format!(
                        "Repetitive content: {} consecutive lines share prefix '{}'",
                        run, prefix
                    ),
                    suggestion: Some(
                        "LLM output degenerated into repetitive patterns. Regenerate this section."
                            .to_string(),
                    ),
                });
                break;
            }
            i += run;
        }

        // Check 2: Gibberish token (catches nonsense identifiers)
        // Any word outside code blocks longer than 80 chars is almost certainly not real.
        for (idx, line) in lines.iter().enumerate() {
            if code_block_lines[idx] {
                continue;
            }
            for word in line.split_whitespace() {
                let clean = word.trim_matches(|c: char| {
                    c == '*' || c == '`' || c == '_' || c == ',' || c == '-'
                });
                // Skip dotted identifiers (e.g., cryptography.hazmat.primitives.twofactor.hotp.HOTP)
                // These are valid fully-qualified Python/Java/etc. module paths, not gibberish.
                // Each segment must be reasonably short (<=40 chars) and there must be 2+ segments.
                let is_dotted_identifier = clean.contains('.') && {
                    let parts: Vec<&str> = clean.split('.').collect();
                    parts.len() >= 2
                        && parts.iter().all(|part| {
                            !part.is_empty()
                                && part.len() <= 40
                                && part.chars().all(|c| c.is_alphanumeric() || c == '_')
                        })
                };

                if clean.len() > 80 && !is_dotted_identifier {
                    issues.push(LintIssue {
                        severity: Severity::Error,
                        category: "degeneration".to_string(),
                        message: format!(
                            "Nonsense token detected ({} chars): '{}...'",
                            clean.len(),
                            &clean[..40]
                        ),
                        suggestion: Some(
                            "LLM output contains gibberish. Regenerate this section.".to_string(),
                        ),
                    });
                    return issues; // One is enough
                }
            }
        }

        // Check 3: Prompt instruction leak
        // Known phrases from our prompts that should never appear in the output.
        let prompt_leaks = [
            "CRITICAL: Include ALL",
            "CRITICAL: Prioritize PUBLIC APIs",
            "CRITICAL: Mark deprecation status",
            "CRITICAL: This section is MANDATORY",
            "do NOT skip this section",
            "REQUIRED sections:",
            "Focus on the 10-15",
            "Output as JSON",
            "Your job is to",
            "Show the standard import patterns",
            "Add 2 more pitfalls if found",
            "minimum 3, maximum 5 total",
        ];
        let mut in_code = false;
        for line in &lines {
            if line.trim_start().starts_with("```") {
                in_code = !in_code;
                continue;
            }
            if in_code {
                continue;
            }
            for phrase in &prompt_leaks {
                if line.contains(phrase) {
                    issues.push(LintIssue {
                        severity: Severity::Warning,
                        category: "degeneration".to_string(),
                        message: format!("Prompt instruction leak: '{}'", phrase),
                        suggestion: Some(
                            "LLM regurgitated prompt instructions into the output. Regenerate this section."
                                .to_string(),
                        ),
                    });
                    break;
                }
            }
        }

        // Check 4: Unclosed code blocks (truncated output)
        let fence_count = lines
            .iter()
            .filter(|l| l.trim_start().starts_with("```"))
            .count();
        if fence_count % 2 != 0 {
            issues.push(LintIssue {
                severity: Severity::Error,
                category: "degeneration".to_string(),
                message: format!(
                    "Unclosed code block ({} fences, expected even number)",
                    fence_count
                ),
                suggestion: Some(
                    "Output was likely truncated by token limit. Regenerate with higher max_tokens."
                        .to_string(),
                ),
            });
        }

        // Check 5: Excessively long line outside code blocks
        // Real markdown doesn't have 1000+ char lines. Degeneration does.
        for (idx, line) in lines.iter().enumerate() {
            if code_block_lines[idx] {
                continue;
            }
            if line.len() > 1000 {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "degeneration".to_string(),
                    message: format!(
                        "Excessively long line detected ({} chars)",
                        line.len()
                    ),
                    suggestion: Some(
                        "Lines over 1000 chars outside code blocks suggest LLM degeneration. Regenerate this section."
                            .to_string(),
                    ),
                });
                break;
            }
        }

        issues
    }

    fn extract_frontmatter(&self, content: &str) -> HashMap<String, String> {
        let mut frontmatter = HashMap::new();

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() || !lines[0].starts_with("---") {
            return frontmatter;
        }

        let mut in_frontmatter = false;
        for line in lines {
            if line.trim() == "---" {
                if in_frontmatter {
                    break; // End of frontmatter
                }
                in_frontmatter = true;
                continue;
            }

            if in_frontmatter && line.contains(':') {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_string();
                    let value = parts[1].trim().to_string();
                    frontmatter.insert(key, value);
                }
            }
        }

        frontmatter
    }

    /// Print issues in a human-readable format
    pub fn print_issues(&self, issues: &[LintIssue]) {
        if issues.is_empty() {
            println!("✅ No linting issues found!");
            return;
        }

        println!("\n📋 SKILL.md Linting Results:\n");

        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        let warnings: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect();
        let infos: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Info)
            .collect();

        if !errors.is_empty() {
            println!("❌ Errors ({}):", errors.len());
            for issue in &errors {
                println!("   • [{}] {}", issue.category, issue.message);
                if let Some(suggestion) = &issue.suggestion {
                    println!("     💡 {}", suggestion);
                }
            }
            println!();
        }

        if !warnings.is_empty() {
            println!("⚠️  Warnings ({}):", warnings.len());
            for issue in &warnings {
                println!("   • [{}] {}", issue.category, issue.message);
                if let Some(suggestion) = &issue.suggestion {
                    println!("     💡 {}", suggestion);
                }
            }
            println!();
        }

        if !infos.is_empty() {
            println!("ℹ️  Info ({}):", infos.len());
            for issue in &infos {
                println!("   • [{}] {}", issue.category, issue.message);
                if let Some(suggestion) = &issue.suggestion {
                    println!("     💡 {}", suggestion);
                }
            }
            println!();
        }

        println!(
            "Summary: {} errors, {} warnings, {} info",
            errors.len(),
            warnings.len(),
            infos.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_frontmatter() {
        let linter = SkillLinter::new();
        let content = "# Some content\nNo frontmatter here";
        let issues = linter.lint(content).unwrap();

        assert!(issues
            .iter()
            .any(|i| i.message.contains("Missing frontmatter")));
    }

    #[test]
    fn test_missing_license() {
        let linter = SkillLinter::new();
        let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
---

## Imports
```python
import test
```

## Core Patterns
Example code

## Pitfalls
Common mistakes
"#;
        let issues = linter.lint(content).unwrap();

        assert!(issues.iter().any(|i| i.message.contains("license")));
    }

    #[test]
    fn test_valid_skill() {
        let linter = SkillLinter::new();
        let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns

1. First step
2. Second step
3. Third step

```python
# Example code
test.do_something()
```

## Pitfalls

### Wrong: Bad approach
```python
# Bad code
```

### Right: Good approach
```python
# Good code
```
"#;
        let issues = linter.lint(content).unwrap();

        // Should only have info-level issues or none
        let errors = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        assert_eq!(errors, 0);
    }
}
