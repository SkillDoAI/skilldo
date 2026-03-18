//! Rust-specific SKILL.md parser — extracts version, name, code patterns,
//! and dependencies from a generated SKILL.md file. Used by the test agent
//! to understand what to validate.

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use tracing::{debug, warn};

use super::parser::{extract_section, CodePattern, PatternCategory};
use super::LanguageParser;
use crate::util::sanitize_dep_name;

// Cached regexes for pattern/dependency extraction
static PATTERN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^###\s+(.+?)$").unwrap());
static CODE_BLOCK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(?:```|~~~)(?:rust|rs)?\r?\n([\s\S]*?)(?:```|~~~)").unwrap());
/// Rust-tagged only — used to prefer ```rust blocks over untagged ones in extract_patterns.
static RUST_CODE_BLOCK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(?:```|~~~)(?:rust|rs)\r?\n([\s\S]*?)(?:```|~~~)").unwrap());
static USE_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^use\s+([a-zA-Z_][a-zA-Z0-9_]*)(?:\s+as\s+[a-zA-Z_][a-zA-Z0-9_]*)?(?:::|;)")
        .unwrap()
});
static EXTERN_CRATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^extern\s+crate\s+([a-zA-Z_][a-zA-Z0-9_]*)(?:\s+as\s+[a-zA-Z_][a-zA-Z0-9_]*)?\s*;",
    )
    .unwrap()
});
static CARGO_ADD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"cargo\s+add\s+([a-zA-Z_][a-zA-Z0-9_\-]*)").unwrap());
// Matches #[crate_name::macro] attribute usage (e.g., #[tokio::main], #[tokio::test])
static ATTR_CRATE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"#\[([a-zA-Z_][a-zA-Z0-9_]*)::").unwrap());
static CARGO_TOML_DEP_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?m)^([a-zA-Z_][a-zA-Z0-9_\-]*)\s*="#).unwrap());

/// Rust-specific parser for SKILL.md files
pub struct RustParser;

impl RustParser {
    /// Categorize a pattern based on its name and description
    fn categorize_pattern(name: &str, description: &str) -> PatternCategory {
        let text = format!("{} {}", name.to_lowercase(), description.to_lowercase());

        if text.contains("basic")
            || text.contains("simple")
            || text.contains("hello")
            || text.contains("getting started")
            || text.contains("quickstart")
        {
            PatternCategory::BasicUsage
        } else if text.contains("config") || text.contains("setup") || text.contains("initialize") {
            PatternCategory::Configuration
        } else if text.contains("error") || text.contains("handle") || text.contains("recover") {
            PatternCategory::ErrorHandling
        } else if text.contains("tokio")
            || text.contains("async-std")
            || text.contains("future")
            || text.contains("async")
            || text.contains("concurrent")
        {
            PatternCategory::AsyncPattern
        } else {
            PatternCategory::Other
        }
    }

    /// Check if a Rust crate name is from the standard library.
    fn is_stdlib_crate(name: &str) -> bool {
        const STDLIB_CRATES: &[&str] = &[
            "std",
            "core",
            "alloc",
            "proc_macro",
            "test",
            // Intra-crate path qualifiers (not external deps)
            "crate",
            "self",
            "super",
        ];
        STDLIB_CRATES.contains(&name)
    }
}

impl LanguageParser for RustParser {
    fn extract_patterns(&self, skill_md: &str) -> Result<Vec<CodePattern>> {
        let mut patterns = Vec::new();

        let core_patterns_content =
            match extract_section(skill_md, r"(?mi)^##\s+Core\s+Patterns\s*$")? {
                Some(s) => s,
                None => {
                    debug!("No Core Patterns section found in SKILL.md");
                    return Ok(patterns);
                }
            };

        let pattern_starts: Vec<(usize, String)> = PATTERN_RE
            .captures_iter(core_patterns_content)
            .map(|cap| (cap.get(0).unwrap().start(), cap[1].to_string()))
            .collect();

        for i in 0..pattern_starts.len() {
            let (pattern_start, pattern_name) = &pattern_starts[i];
            let pattern_end = if i + 1 < pattern_starts.len() {
                pattern_starts[i + 1].0
            } else {
                core_patterns_content.len()
            };

            let pattern_section = &core_patterns_content[*pattern_start..pattern_end];

            let description_start = pattern_section.find('\n').unwrap_or(0) + 1;
            let code_block_start = CODE_BLOCK_RE
                .find(pattern_section)
                .map(|m| m.start())
                .unwrap_or(pattern_section.len());

            let description = pattern_section[description_start..code_block_start]
                .trim()
                .to_string();

            // Prefer rust-tagged blocks (```rust) over untagged (```)
            let code_cap_opt = RUST_CODE_BLOCK_RE
                .captures(pattern_section)
                .or_else(|| CODE_BLOCK_RE.captures(pattern_section));
            if let Some(code_cap) = code_cap_opt {
                let code = code_cap[1].trim().to_string();
                let category = Self::categorize_pattern(pattern_name, &description);

                patterns.push(CodePattern {
                    name: pattern_name.clone(),
                    description,
                    code,
                    category,
                });
            }
        }

        if patterns.is_empty() {
            anyhow::bail!(
                "Core Patterns section found but no code blocks extracted. \
                 Check that patterns have ### headings with code fences."
            );
        }

        debug!("Extracted {} patterns from SKILL.md", patterns.len());
        Ok(patterns)
    }

    fn extract_dependencies(&self, skill_md: &str) -> Result<Vec<String>> {
        let mut dependencies = Vec::new();

        let imports_content = match extract_section(skill_md, r"(?m)^##\s+Imports\s*$")? {
            Some(s) => s,
            None => {
                debug!("No Imports section found in SKILL.md");
                return Ok(dependencies);
            }
        };

        // Track whether we're inside a [dependencies] block for Cargo.toml parsing
        let mut in_deps_section = false;

        for line in imports_content.lines() {
            // Check for [dependencies] header (strip inline comments first)
            let effective = line.trim().split('#').next().unwrap_or("").trim();
            if effective == "[dependencies]" {
                in_deps_section = true;
                continue;
            }
            // Exit [dependencies] on next section header or code fence close
            if in_deps_section {
                let t = line.trim();
                if t.starts_with('[') || t.starts_with("```") || t.starts_with("~~~") {
                    in_deps_section = false;
                } else if let Some(cap) = CARGO_TOML_DEP_RE.captures(line) {
                    let crate_name = cap[1].to_string();
                    if !Self::is_stdlib_crate(&crate_name) && !dependencies.contains(&crate_name) {
                        dependencies.push(crate_name);
                    }
                }
            }
        }

        // `use crate_name::*` imports
        for cap in USE_IMPORT_RE.captures_iter(imports_content) {
            let crate_name = cap[1].to_string();
            if !Self::is_stdlib_crate(&crate_name) && !dependencies.contains(&crate_name) {
                dependencies.push(crate_name);
            }
        }

        // `extern crate crate_name;` imports
        for cap in EXTERN_CRATE_RE.captures_iter(imports_content) {
            let crate_name = cap[1].to_string();
            if !Self::is_stdlib_crate(&crate_name) && !dependencies.contains(&crate_name) {
                dependencies.push(crate_name);
            }
        }

        // `cargo add` commands
        for cap in CARGO_ADD_RE.captures_iter(imports_content) {
            let crate_name = cap[1].to_string();
            if !Self::is_stdlib_crate(&crate_name) && !dependencies.contains(&crate_name) {
                dependencies.push(crate_name);
            }
        }

        // Also scan Core Patterns code blocks for `use` statements to catch
        // peer deps (tokio, reqwest, serde_json) that appear in examples but
        // not in the Imports section. Models often list only the target crate
        // in Imports but use peer deps in code examples.
        if let Ok(Some(patterns_content)) =
            extract_section(skill_md, r"(?m)^##\s+Core Patterns\s*$")
        {
            // `use crate_name::...` statements
            for cap in USE_IMPORT_RE.captures_iter(patterns_content) {
                let crate_name = cap[1].to_string();
                if !Self::is_stdlib_crate(&crate_name) && !dependencies.contains(&crate_name) {
                    debug!("Found peer dep in Core Patterns (use): {}", crate_name);
                    dependencies.push(crate_name);
                }
            }
            // `#[crate_name::macro]` attribute macros (e.g., #[tokio::main])
            for cap in ATTR_CRATE_RE.captures_iter(patterns_content) {
                let crate_name = cap[1].to_string();
                if !Self::is_stdlib_crate(&crate_name) && !dependencies.contains(&crate_name) {
                    debug!("Found peer dep in Core Patterns (attr): {}", crate_name);
                    dependencies.push(crate_name);
                }
            }
        }

        dependencies.retain(|dep| match sanitize_dep_name(dep) {
            Ok(_) => true,
            Err(e) => {
                warn!("Dropping invalid dependency at ingestion: {}", e);
                false
            }
        });

        debug!(
            "Extracted {} dependencies from SKILL.md",
            dependencies.len()
        );
        Ok(dependencies)
    }
}

/// Structured dependency extraction — non-trait method for Rust-specific path.
impl RustParser {
    /// Extract structured dependencies from SKILL.md, preserving raw TOML specs.
    /// Merges: (1) TOML [dependencies] block in ## Imports (authoritative, with features),
    /// (2) peer deps from use/attr scanning in Core Patterns (name-only Pattern deps).
    pub fn extract_structured_dependencies(
        &self,
        skill_md: &str,
    ) -> Result<Vec<crate::pipeline::collector::StructuredDep>> {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        // First: get all name-only deps from the existing extraction
        let name_deps = self.extract_dependencies(skill_md)?;

        // Then: try to parse a [dependencies] TOML block from ## Imports
        let mut toml_specs: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        if let Ok(Some(imports_content)) = extract_section(skill_md, r"(?m)^##\s+Imports\s*$") {
            // Find the [dependencies] block inside a toml fence
            let mut in_toml_fence = false;
            let mut toml_block = String::new();

            for line in imports_content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("```toml") {
                    in_toml_fence = true;
                    continue;
                }
                if in_toml_fence && (trimmed == "```" || trimmed == "~~~") {
                    in_toml_fence = false;
                    continue;
                }
                if in_toml_fence {
                    toml_block.push_str(line);
                    toml_block.push('\n');
                }
            }

            if !toml_block.is_empty() {
                // Parse as TOML table
                if let Ok(parsed) = toml_block.parse::<toml::Table>() {
                    // Look for [dependencies] section within the parsed TOML
                    let deps_table =
                        if let Some(deps) = parsed.get("dependencies").and_then(|v| v.as_table()) {
                            deps.clone()
                        } else {
                            // The block might be the deps directly (no [dependencies] header)
                            parsed
                                .iter()
                                .filter(|(_, v)| {
                                    !v.is_table() || {
                                        // Skip [package] and other non-dep sections
                                        let key = v.as_table().map(|t| t.contains_key("version"));
                                        key.unwrap_or(false)
                                    }
                                })
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect()
                        };

                    for (name, value) in &deps_table {
                        let raw = match value {
                            toml::Value::String(s) => format!("\"{}\"", s),
                            other => other.to_string(),
                        };
                        toml_specs.insert(name.clone(), raw);
                    }
                }
            }
        }

        // Merge: upgrade name-only deps with TOML specs where available
        let mut result: Vec<StructuredDep> = Vec::new();
        for name in &name_deps {
            if let Some(raw_spec) = toml_specs.remove(name.as_str()) {
                result.push(StructuredDep {
                    name: name.clone(),
                    raw_spec: Some(raw_spec),
                    source: DepSource::Manifest,
                });
            } else {
                result.push(StructuredDep {
                    name: name.clone(),
                    raw_spec: None,
                    source: DepSource::Pattern,
                });
            }
        }

        // Add any TOML deps that weren't in the name-only list
        for (name, raw_spec) in toml_specs {
            if !result.iter().any(|d| d.name == name) {
                result.push(StructuredDep {
                    name,
                    raw_spec: Some(raw_spec),
                    source: DepSource::Manifest,
                });
            }
        }

        debug!(
            "Extracted {} structured deps ({} with specs)",
            result.len(),
            result.iter().filter(|d| d.raw_spec.is_some()).count()
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SKILL: &str = r##"---
name: serde
version: 1.0.200
language: rust
---

# serde

Serialization framework for Rust.

## Imports

```rust
use serde::{Serialize, Deserialize};
use serde_json;
```

```bash
cargo add serde --features derive
cargo add serde_json
```

## Core Patterns

### Basic Serialization

Serialize a struct to JSON.

```rust
#[derive(Serialize)]
struct Point {
    x: f64,
    y: f64,
}
let point = Point { x: 1.0, y: 2.0 };
let json = serde_json::to_string(&point).unwrap();
```

### Error Handling with Result

Handle deserialization errors.

```rust
let data = r#"{"x": 1.0}"#;
match serde_json::from_str::<Point>(data) {
    Ok(point) => println!("Parsed: {:?}", point),
    Err(e) => eprintln!("Failed: {}", e),
}
```
"##;

    #[test]
    fn extract_patterns_from_rust_skill() {
        let parser = RustParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0].name, "Basic Serialization");
        assert_eq!(patterns[1].name, "Error Handling with Result");
    }

    #[test]
    fn extract_pattern_categories() {
        let parser = RustParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert_eq!(patterns[0].category, PatternCategory::BasicUsage);
        assert_eq!(patterns[1].category, PatternCategory::ErrorHandling);
    }

    #[test]
    fn extract_pattern_code_content() {
        let parser = RustParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert!(patterns[0].code.contains("Serialize"));
        assert!(patterns[0].code.contains("serde_json::to_string"));
    }

    #[test]
    fn extract_dependencies_filters_stdlib() {
        let parser = RustParser;
        let deps = parser.extract_dependencies(SAMPLE_SKILL).unwrap();
        assert!(
            deps.contains(&"serde".to_string()),
            "should contain serde, got: {:?}",
            deps
        );
        assert!(
            deps.contains(&"serde_json".to_string()),
            "should contain serde_json, got: {:?}",
            deps
        );
        assert!(
            !deps.iter().any(|d| d == "std"),
            "stdlib should be filtered"
        );
    }

    #[test]
    fn extract_version_from_frontmatter() {
        let parser = RustParser;
        assert_eq!(
            parser.extract_version(SAMPLE_SKILL).unwrap(),
            Some("1.0.200".into())
        );
    }

    #[test]
    fn extract_name_from_frontmatter() {
        let parser = RustParser;
        assert_eq!(
            parser.extract_name(SAMPLE_SKILL).unwrap(),
            Some("serde".into())
        );
    }

    #[test]
    fn no_core_patterns_section_returns_empty() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n# Test\n\nSome text.\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn no_imports_section_returns_empty() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n# Test\n\nSome text.\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn is_stdlib_crate_basics() {
        assert!(RustParser::is_stdlib_crate("std"));
        assert!(RustParser::is_stdlib_crate("core"));
        assert!(RustParser::is_stdlib_crate("alloc"));
        assert!(RustParser::is_stdlib_crate("proc_macro"));
        assert!(RustParser::is_stdlib_crate("test"));
    }

    #[test]
    fn is_stdlib_rejects_external_crates() {
        assert!(!RustParser::is_stdlib_crate("serde"));
        assert!(!RustParser::is_stdlib_crate("tokio"));
        assert!(!RustParser::is_stdlib_crate("serde_json"));
        assert!(!RustParser::is_stdlib_crate("rand"));
    }

    #[test]
    fn tilde_fenced_code_block_extracted() {
        let parser = RustParser;
        let skill =
            "---\nname: test\n---\n\n## Core Patterns\n\n### Hello\n\n~~~rust\nlet x = 42;\n~~~\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert!(
            !patterns.is_empty(),
            "should extract pattern from ~~~rust fence"
        );
        assert!(
            patterns[0].code.contains("let x = 42"),
            "code should contain let x = 42"
        );
    }

    #[test]
    fn cargo_add_dependency_extraction() {
        let parser = RustParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```bash\ncargo add tokio --features full\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"tokio".to_string()),
            "expected tokio from cargo add, got: {:?}",
            deps
        );
    }

    #[test]
    fn cargo_add_skips_stdlib_crates() {
        let parser = RustParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```bash\ncargo add std\ncargo add serde\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            !deps.contains(&"std".to_string()),
            "stdlib crates should be excluded from cargo add: {:?}",
            deps
        );
        assert!(deps.contains(&"serde".to_string()));
    }

    #[test]
    fn deduplicates_dependencies() {
        let parser = RustParser;
        let skill = r#"---
name: test
---

## Imports

```rust
use serde::{Serialize, Deserialize};
```

```bash
cargo add serde --features derive
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(
            deps.iter().filter(|d| d.as_str() == "serde").count(),
            1,
            "should deduplicate"
        );
    }

    #[test]
    fn categorize_async_tokio_pattern() {
        assert_eq!(
            RustParser::categorize_pattern("Tokio Runtime", "Set up a tokio async runtime"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_async_future_pattern() {
        assert_eq!(
            RustParser::categorize_pattern("Future Combinators", "Chain futures together"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_async_std_pattern() {
        assert_eq!(
            RustParser::categorize_pattern("Async-std Tasks", "Spawn async-std tasks"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_configuration_pattern() {
        assert_eq!(
            RustParser::categorize_pattern("Database Config", "Set up the connection"),
            PatternCategory::Configuration
        );
    }

    #[test]
    fn categorize_error_pattern() {
        assert_eq!(
            RustParser::categorize_pattern("Error Types", "Handle custom error types"),
            PatternCategory::ErrorHandling
        );
    }

    #[test]
    fn core_patterns_section_no_code_blocks_errors() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Core Patterns\n\n### Pattern Without Code\n\nThis has no code fence.\n\n## Next\n";
        let result = parser.extract_patterns(skill);
        assert!(
            result.is_err(),
            "Core Patterns section with headings but no code blocks should error"
        );
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("no code blocks extracted"));
    }

    #[test]
    fn extern_crate_dependency_extraction() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```rust\nextern crate rand;\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"rand".to_string()),
            "expected rand from extern crate, got: {:?}",
            deps
        );
    }

    #[test]
    fn extern_crate_filters_stdlib() {
        let parser = RustParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```rust\nextern crate alloc;\nextern crate rand;\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            !deps.contains(&"alloc".to_string()),
            "stdlib crate alloc should be filtered"
        );
        assert!(deps.contains(&"rand".to_string()));
    }

    #[test]
    fn use_import_filters_stdlib() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```rust\nuse std::collections::HashMap;\nuse serde::Serialize;\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            !deps.iter().any(|d| d == "std"),
            "stdlib should be filtered"
        );
        assert!(deps.contains(&"serde".to_string()));
    }

    #[test]
    fn plain_fence_code_block_extracted() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Core Patterns\n\n### Basic\n\nA simple example.\n\n```\nlet x = 1;\n```\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert_eq!(patterns.len(), 1);
    }

    #[test]
    fn rs_fence_code_block_extracted() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Core Patterns\n\n### Basic\n\nA simple example.\n\n```rs\nlet x = 1;\n```\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert_eq!(patterns.len(), 1);
        assert!(patterns[0].code.contains("let x = 1"));
    }

    #[test]
    fn cargo_toml_dependencies_section() {
        let parser = RustParser;
        let skill = r#"---
name: test
---

## Imports

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = "1.0"
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"tokio".to_string()),
            "expected tokio from Cargo.toml deps, got: {:?}",
            deps
        );
        assert!(
            deps.contains(&"serde".to_string()),
            "expected serde from Cargo.toml deps, got: {:?}",
            deps
        );
    }

    #[test]
    fn extract_name_missing_returns_none() {
        let parser = RustParser;
        let skill = "---\ndescription: no name field\n---\n\n## Overview\n";
        assert_eq!(parser.extract_name(skill).unwrap(), None);
    }

    #[test]
    fn extract_version_unknown_returns_none() {
        let parser = RustParser;
        let skill = "---\nname: test\nversion: unknown\n---\n";
        assert_eq!(parser.extract_version(skill).unwrap(), None);
    }

    #[test]
    fn extract_version_missing_returns_none() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n";
        assert_eq!(parser.extract_version(skill).unwrap(), None);
    }

    #[test]
    fn filters_intra_crate_paths() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```rust\nuse crate::config::Settings;\nuse self::helper;\nuse super::parent;\nuse serde::Deserialize;\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            !deps.iter().any(|d| d == "crate"),
            "should filter `crate`: {:?}",
            deps
        );
        assert!(
            !deps.iter().any(|d| d == "self"),
            "should filter `self`: {:?}",
            deps
        );
        assert!(
            !deps.iter().any(|d| d == "super"),
            "should filter `super`: {:?}",
            deps
        );
        assert!(deps.contains(&"serde".to_string()));
    }

    #[test]
    fn cargo_toml_deps_with_blank_lines() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```toml\n[dependencies]\ntokio = \"1\"\n\nserde = \"1\"\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"tokio".to_string()),
            "should find tokio before blank line: {:?}",
            deps
        );
        assert!(
            deps.contains(&"serde".to_string()),
            "should find serde after blank line: {:?}",
            deps
        );
    }

    #[test]
    fn cargo_toml_deps_exit_on_next_section() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```toml\n[dependencies]\ntokio = \"1\"\n\n[dev-dependencies]\nproptest = \"1\"\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"tokio".to_string()),
            "should find tokio in [dependencies]: {:?}",
            deps
        );
        assert!(
            !deps.contains(&"proptest".to_string()),
            "should stop at [dev-dependencies]: {:?}",
            deps
        );
    }

    #[test]
    fn categorize_async_keyword_pattern() {
        assert_eq!(
            RustParser::categorize_pattern("Async Workers", "Run async tasks in background"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_concurrent_pattern() {
        assert_eq!(
            RustParser::categorize_pattern("Thread Pool", "Run concurrent workers"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_other_pattern() {
        assert_eq!(
            RustParser::categorize_pattern("Serialization", "Convert data to bytes"),
            PatternCategory::Other
        );
    }

    #[test]
    fn extract_deps_from_cargo_add_stops_at_special_chars() {
        let parser = RustParser;
        // cargo add regex captures only [a-zA-Z_][a-zA-Z0-9_-]*, so special chars
        // after the crate name are naturally excluded by the regex
        let skill = "---\nname: test\n---\n\n## Imports\n\n```\ncargo add valid-crate\ncargo add once_cell@1.21\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"valid-crate".to_string()));
        assert!(
            deps.contains(&"once_cell".to_string()),
            "should extract crate name before @: {:?}",
            deps
        );
        // The @version part should NOT appear in the dep name
        assert!(
            !deps.iter().any(|d| d.contains('@')),
            "should not include @version: {:?}",
            deps
        );
    }

    #[test]
    fn extract_deps_ignores_invalid_extern_crate_syntax() {
        let parser = RustParser;
        // `-badname` doesn't match EXTERN_CRATE_RE (requires [a-zA-Z_] start),
        // so it's silently skipped before sanitize_dep_name ever runs.
        let skill = "---\nname: test\n---\n\n## Imports\n\n```\nuse serde::Serialize;\nextern crate -badname;\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"serde".to_string()));
        assert!(
            !deps.iter().any(|d| d.starts_with('-')),
            "invalid syntax should be skipped by regex: {:?}",
            deps
        );
    }

    #[test]
    fn extract_patterns_prefers_rust_tagged_block() {
        let parser = RustParser;
        let skill = r#"---
name: test
---

## Core Patterns

### ✅ Example
```toml
[dependencies]
serde = "1"
```

```rust
fn main() { println!("hello"); }
```
"#;
        let patterns = parser.extract_patterns(skill).unwrap();
        assert_eq!(patterns.len(), 1);
        assert!(
            patterns[0].code.contains("fn main()"),
            "should prefer rust-tagged block over toml block: {}",
            patterns[0].code
        );
    }

    #[test]
    fn code_block_matches_crlf_line_endings() {
        let parser = RustParser;
        let skill = "---\r\nname: test\r\n---\r\n\r\n## Core Patterns\r\n\r\n### \u{2705} Hello\r\n```rust\r\nfn main() {}\r\n```\r\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert_eq!(patterns.len(), 1);
        assert!(patterns[0].code.contains("fn main()"));
    }

    #[test]
    fn aliased_use_import_extracts_crate_name() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```rust\nuse serde_json as json;\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"serde_json".to_string()),
            "aliased import should extract the real crate name: {:?}",
            deps
        );
    }

    #[test]
    fn aliased_extern_crate_extracts_crate_name() {
        let parser = RustParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```rust\nextern crate rand as rng;\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"rand".to_string()),
            "aliased extern crate should extract the real crate name: {:?}",
            deps
        );
    }

    #[test]
    fn dependencies_header_with_inline_comment() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```toml\n[dependencies] # runtime deps\ntokio = { version = \"1\", features = [\"full\"] }\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"tokio".to_string()),
            "should parse deps under [dependencies] with inline comment: {:?}",
            deps
        );
    }
}
