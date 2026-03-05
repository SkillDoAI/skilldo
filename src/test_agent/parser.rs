//! Shared types and helpers for SKILL.md parsing — used by all language-specific parsers.

use anyhow::Result;
use regex::Regex;

/// Represents a code pattern extracted from SKILL.md
#[derive(Debug, Clone, PartialEq)]
pub struct CodePattern {
    pub name: String,
    pub description: String,
    pub code: String,
    pub category: PatternCategory,
}

/// Categories for prioritizing pattern testing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternCategory {
    BasicUsage,
    Configuration,
    ErrorHandling,
    AsyncPattern,
    Other,
}

/// Extract the content of a `## Section` from SKILL.md, bounded by the next `## ` heading.
/// Returns `None` if the section header is not found.
pub fn extract_section<'a>(skill_md: &'a str, header_pattern: &str) -> Result<Option<&'a str>> {
    let header_re = Regex::new(header_pattern)?;
    let start_pos = match header_re.find(skill_md) {
        Some(m) => m.end(),
        None => return Ok(None),
    };
    let after = &skill_md[start_pos..];
    let next_section_re = Regex::new(r"(?m)^##\s+")?;
    let end_pos = next_section_re
        .find(after)
        .map(|m| m.start())
        .unwrap_or(after.len());
    Ok(Some(&after[..end_pos]))
}

/// Extract a value from SKILL.md YAML frontmatter by key (e.g. "version", "name").
/// Handles both top-level fields and fields nested under `metadata:`.
pub fn frontmatter_field(skill_md: &str, key: &str) -> Option<String> {
    let prefix = format!("{}:", key);
    for line in skill_md.lines().take(15) {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix(&prefix) {
            let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}
