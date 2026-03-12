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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_field_returns_value() {
        let md = "name: pandas\nversion: 3.0.0\n";
        assert_eq!(frontmatter_field(md, "name"), Some("pandas".to_string()));
        assert_eq!(frontmatter_field(md, "version"), Some("3.0.0".to_string()));
    }

    #[test]
    fn frontmatter_field_returns_none_for_missing() {
        let md = "name: pandas\n";
        assert_eq!(frontmatter_field(md, "version"), None);
    }

    #[test]
    fn frontmatter_field_returns_none_for_empty_value() {
        let md = "version:\n";
        assert_eq!(frontmatter_field(md, "version"), None);
    }

    #[test]
    fn frontmatter_field_returns_none_for_whitespace_only_value() {
        let md = "version:   \n";
        assert_eq!(frontmatter_field(md, "version"), None);
    }

    #[test]
    fn frontmatter_field_strips_quotes() {
        let md = "name: \"pandas\"\n";
        assert_eq!(frontmatter_field(md, "name"), Some("pandas".to_string()));
        let md2 = "name: 'pandas'\n";
        assert_eq!(frontmatter_field(md2, "name"), Some("pandas".to_string()));
    }

    #[test]
    fn frontmatter_field_only_scans_first_15_lines() {
        let mut md = String::new();
        for _ in 0..20 {
            md.push_str("filler: stuff\n");
        }
        md.push_str("name: hidden\n");
        assert_eq!(frontmatter_field(&md, "name"), None);
    }

    #[test]
    fn extract_section_returns_none_for_missing() {
        let md = "## Imports\n\nsome content\n";
        let result = extract_section(md, r"(?m)^##\s+Core\s+Patterns\s*$").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn extract_section_returns_content_up_to_next_section() {
        let md = "## Core Patterns\n\npattern content\n\n## Imports\n\nimport content\n";
        let result = extract_section(md, r"(?mi)^##\s+Core\s+Patterns\s*$")
            .unwrap()
            .unwrap();
        assert!(result.contains("pattern content"));
        assert!(!result.contains("import content"));
    }

    #[test]
    fn extract_section_returns_rest_when_no_next_section() {
        let md = "## Core Patterns\n\nall the rest\n";
        let result = extract_section(md, r"(?mi)^##\s+Core\s+Patterns\s*$")
            .unwrap()
            .unwrap();
        assert!(result.contains("all the rest"));
    }
}
