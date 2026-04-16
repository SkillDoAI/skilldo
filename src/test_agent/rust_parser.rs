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
/// Matches `cargo add <crate>` with optional `--features <list>`.
/// Group 1 = crate name. Group 2 (optional) = raw feature list, as one of:
///   --features foo,bar            (comma-separated, single token)
///   --features=foo,bar            (equals form)
///   --features "foo bar"          (quoted, space-separated)
///   --features 'foo,bar'          (single-quoted)
/// The matcher captures the raw list verbatim; callers normalise quoting,
/// whitespace, and separators.
static CARGO_ADD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?x)
        cargo\s+add\s+
        ([a-zA-Z_][a-zA-Z0-9_\-]*)              # group 1 = crate name
        (?:                                     # optional --features <list>
            \s+--features[\s=]+
            (?:
                "([^"]*)"                       # group 2a = double-quoted list
              | '([^']*)'                       # group 2b = single-quoted list
              | ([^\s-][^\s]*)                  # group 2c = bare token (won't swallow --flag)
            )
        )?
        "#,
    )
    .unwrap()
});
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
            extract_section(skill_md, r"(?mi)^##\s+Core\s+Patterns\s*$")
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

        // First: get all name-only deps from the existing extraction.
        // Dedup by normalized form (Cargo treats foo-bar and foo_bar as identical).
        let raw_name_deps = self.extract_dependencies(skill_md)?;
        let mut seen_norm = std::collections::HashSet::new();
        let name_deps: Vec<String> = raw_name_deps
            .into_iter()
            .filter(|n| seen_norm.insert(n.replace('-', "_")))
            .collect();

        // Then: try to parse a [dependencies] TOML block from ## Imports
        let mut toml_specs: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        if let Ok(Some(imports_content)) = extract_section(skill_md, r"(?m)^##\s+Imports\s*$") {
            // Parse each TOML fence individually — accumulate specs from all of them.
            let mut in_toml_fence = false;
            let mut toml_block = String::new();

            /// Parse a TOML block and accumulate structured dep specs.
            fn parse_toml_fence(
                toml_block: &str,
                name_deps: &[String],
                toml_specs: &mut std::collections::HashMap<String, String>,
            ) {
                if let Ok(parsed) = toml_block.parse::<toml::Table>() {
                    let deps_table =
                        if let Some(deps) = parsed.get("dependencies").and_then(|v| v.as_table()) {
                            deps.clone()
                        } else {
                            // Only promote entries whose name is already known
                            parsed
                                .iter()
                                .filter(|(k, _)| name_deps.iter().any(|n| n == *k))
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

            for line in imports_content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("```toml") || trimmed.starts_with("~~~toml") {
                    in_toml_fence = true;
                    toml_block.clear();
                    continue;
                }
                if in_toml_fence && (trimmed == "```" || trimmed == "~~~") {
                    in_toml_fence = false;
                    // Parse each fence as it closes
                    if !toml_block.is_empty() {
                        parse_toml_fence(&toml_block, &name_deps, &mut toml_specs);
                    }
                    continue;
                }
                if in_toml_fence {
                    toml_block.push_str(line);
                    toml_block.push('\n');
                }
            }
            // Handle unclosed fence at EOF
            if !toml_block.is_empty() {
                parse_toml_fence(&toml_block, &name_deps, &mut toml_specs);
            }
        }

        // Extract fallback specs from `cargo add X --features Y` commands.
        // These are used when no TOML [dependencies] block provides a spec.
        let mut cargo_add_specs: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Ok(Some(imports_for_cargo_add)) = extract_section(skill_md, r"(?m)^##\s+Imports\s*$")
        {
            for cap in CARGO_ADD_RE.captures_iter(imports_for_cargo_add) {
                // Groups 2/3/4 are mutually exclusive — one quoted variant
                // is captured depending on which syntax the model used.
                let Some(features_raw) = cap
                    .get(2)
                    .or_else(|| cap.get(3))
                    .or_else(|| cap.get(4))
                    .map(|m| m.as_str())
                else {
                    continue;
                };
                let crate_name = cap[1].to_string();
                // Cargo accepts both comma- and whitespace-separated lists,
                // and commas inside quoted strings are equivalent. Splitting
                // on both covers `foo,bar`, `foo bar`, and `foo, bar` alike.
                let quoted: Vec<String> = features_raw
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(|f| format!("\"{f}\""))
                    .collect();
                if quoted.is_empty() {
                    continue;
                }
                let spec = format!("{{ version = \"*\", features = [{}] }}", quoted.join(", "));
                cargo_add_specs.insert(crate_name, spec);
            }
        }

        // Merge: upgrade name-only deps with TOML specs where available.
        // Normalize dash/underscore in the lookup — `use env_logger` should
        // match `env-logger = "0.11"` in the TOML block (Cargo equivalence).
        let mut result: Vec<StructuredDep> = Vec::new();
        for name in &name_deps {
            let norm = name.replace('-', "_");
            let matched_key = toml_specs
                .keys()
                .find(|k| k.replace('-', "_") == norm)
                .cloned();
            if let Some(raw_spec) = matched_key.and_then(|k| toml_specs.remove(&k)) {
                result.push(StructuredDep {
                    name: name.clone(),
                    raw_spec: Some(raw_spec),
                    source: DepSource::Manifest,
                });
                continue;
            }
            // Fallback: match cargo add --features, normalizing dash/underscore
            // (cargo add tokio-util --features codec ↔ use tokio_util).
            let cargo_key = cargo_add_specs
                .keys()
                .find(|k| k.replace('-', "_") == norm)
                .cloned();
            if let Some(cargo_spec) = cargo_key.and_then(|k| cargo_add_specs.remove(&k)) {
                result.push(StructuredDep {
                    name: name.clone(),
                    raw_spec: Some(cargo_spec),
                    source: DepSource::Pattern,
                });
            } else {
                result.push(StructuredDep {
                    name: name.clone(),
                    raw_spec: None,
                    source: DepSource::Pattern,
                });
            }
        }

        // Add any TOML deps that weren't in the name-only list.
        // Normalize dash/underscore: Cargo treats foo-bar and foo_bar as the same crate.
        for (name, raw_spec) in toml_specs {
            let norm_name = name.replace('-', "_");
            if !result.iter().any(|d| d.name.replace('-', "_") == norm_name) {
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
    use crate::pipeline::collector::DepSource;

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
    fn cargo_add_features_preserved_in_structured_deps() {
        let parser = RustParser;
        // SKILL.md with cargo add --features but NO TOML block
        let skill = "---\nname: test\n---\n\n## Imports\n\n```bash\ncargo add tokio --features full\ncargo add serde --features derive\ncargo add reqwest\n```\n";
        let deps = parser.extract_structured_dependencies(skill).unwrap();

        let tokio_dep = deps.iter().find(|d| d.name == "tokio").unwrap();
        assert!(
            tokio_dep.raw_spec.is_some(),
            "tokio should have a raw_spec from cargo add --features"
        );
        let spec = tokio_dep.raw_spec.as_ref().unwrap();
        assert!(
            spec.contains("full"),
            "tokio spec should contain 'full' feature: {spec}"
        );

        let serde_dep = deps.iter().find(|d| d.name == "serde").unwrap();
        assert!(
            serde_dep.raw_spec.as_ref().unwrap().contains("derive"),
            "serde spec should contain 'derive' feature"
        );

        // reqwest has no --features, should have raw_spec = None
        let reqwest_dep = deps.iter().find(|d| d.name == "reqwest").unwrap();
        assert!(
            reqwest_dep.raw_spec.is_none(),
            "reqwest should have no raw_spec (no --features)"
        );
    }

    #[test]
    fn cargo_add_features_equals_form() {
        // `cargo add tokio --features=full` (equals form) should work.
        let parser = RustParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```bash\ncargo add tokio --features=full\n```\n";
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        let tokio = deps.iter().find(|d| d.name == "tokio").unwrap();
        let spec = tokio.raw_spec.as_ref().unwrap();
        assert!(
            spec.contains("full"),
            "features=full should survive: {spec}"
        );
    }

    #[test]
    fn cargo_add_features_space_separated_quoted() {
        // `cargo add tokio --features "net sync"` (shell-quoted, space-separated)
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```bash\ncargo add tokio --features \"net sync\"\n```\n";
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        let tokio = deps.iter().find(|d| d.name == "tokio").unwrap();
        let spec = tokio.raw_spec.as_ref().unwrap();
        assert!(
            spec.contains("net"),
            "net feature should be present: {spec}"
        );
        assert!(
            spec.contains("sync"),
            "sync feature should be present: {spec}"
        );
    }

    #[test]
    fn cargo_add_features_match_dash_underscore_variants() {
        // `cargo add tokio-util --features codec` + `use tokio_util::...`
        // should still preserve the features, since Cargo treats dashes and
        // underscores as equivalent in crate names.
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```rust\nuse tokio_util::codec;\n```\n\n```bash\ncargo add tokio-util --features codec\n```\n";
        let deps = parser.extract_structured_dependencies(skill).unwrap();

        let dep = deps
            .iter()
            .find(|d| d.name == "tokio_util" || d.name == "tokio-util")
            .expect("tokio_util dep should be present");
        let spec = dep
            .raw_spec
            .as_ref()
            .expect("hyphenated cargo add features must survive the dash/underscore lookup");
        assert!(
            spec.contains("codec"),
            "spec should contain the 'codec' feature despite dash/underscore mismatch: {spec}"
        );
    }

    #[test]
    fn cargo_add_features_not_used_when_toml_block_exists() {
        let parser = RustParser;
        // SKILL.md with BOTH cargo add --features AND a TOML block — TOML wins
        let skill = "---\nname: test\n---\n\n## Imports\n\n```bash\ncargo add tokio --features full\n```\n\n```toml\n[dependencies]\ntokio = { version = \"1.51\", features = [\"full\", \"macros\"] }\n```\n";
        let deps = parser.extract_structured_dependencies(skill).unwrap();

        let tokio_dep = deps.iter().find(|d| d.name == "tokio").unwrap();
        let spec = tokio_dep.raw_spec.as_ref().unwrap();
        // TOML block spec should win — has version "1.51" not "*"
        assert!(
            spec.contains("1.51"),
            "TOML block should take precedence over cargo add: {spec}"
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

    // ── extract_structured_dependencies tests ─────────────────────────────

    #[test]
    fn structured_deps_with_toml_block() {
        let parser = RustParser;
        let skill = r#"---
name: mylib
version: 1.0.0
---

## Imports

```rust
use tokio::runtime::Runtime;
use serde_json::Value;
```

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

## Core Patterns
"#;
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        assert!(deps.len() >= 2, "should have at least 2 deps: {:?}", deps);

        let tokio_dep = deps.iter().find(|d| d.name == "tokio").unwrap();
        assert!(
            tokio_dep.raw_spec.as_ref().unwrap().contains("features"),
            "tokio should preserve features: {:?}",
            tokio_dep.raw_spec
        );
        assert_eq!(tokio_dep.source, DepSource::Manifest);

        let serde_dep = deps.iter().find(|d| d.name == "serde_json").unwrap();
        assert!(serde_dep.raw_spec.is_some());
    }

    #[test]
    fn structured_deps_without_toml_block_are_pattern() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```rust\nuse tokio::runtime::Runtime;\n```\n\n## Core Patterns\n";
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        let tokio_dep = deps.iter().find(|d| d.name == "tokio").unwrap();
        assert!(
            tokio_dep.raw_spec.is_none(),
            "without TOML block, raw_spec should be None"
        );
        assert_eq!(tokio_dep.source, DepSource::Pattern);
    }

    #[test]
    fn structured_deps_merges_toml_and_names() {
        let parser = RustParser;
        let skill = r#"---
name: test
---

## Imports

```rust
use tokio::runtime::Runtime;
use reqwest::Client;
```

```toml
tokio = { version = "1", features = ["full"] }
```

## Core Patterns
"#;
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        // tokio should have Manifest spec, reqwest should be Pattern
        let tokio_dep = deps.iter().find(|d| d.name == "tokio").unwrap();
        assert_eq!(tokio_dep.source, DepSource::Manifest);
        assert!(tokio_dep.raw_spec.is_some());

        let reqwest_dep = deps.iter().find(|d| d.name == "reqwest").unwrap();
        assert_eq!(reqwest_dep.source, DepSource::Pattern);
        assert!(reqwest_dep.raw_spec.is_none());
    }

    #[test]
    fn structured_deps_peer_dep_from_core_patterns() {
        let parser = RustParser;
        let skill = r#"---
name: mylib
---

## Imports

```rust
use mylib::Client;
```

## Core Patterns

### Basic Usage

```rust
use mylib::Client;
use reqwest::blocking::get;

#[tokio::main]
async fn main() {
    let client = Client::new();
}
```
"#;
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        // reqwest discovered from Core Patterns use statement
        assert!(
            deps.iter().any(|d| d.name == "reqwest"),
            "should discover reqwest from Core Patterns: {:?}",
            deps.iter().map(|d| &d.name).collect::<Vec<_>>()
        );
        // tokio discovered from #[tokio::main] attribute
        assert!(
            deps.iter().any(|d| d.name == "tokio"),
            "should discover tokio from attribute macro: {:?}",
            deps.iter().map(|d| &d.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn structured_deps_dash_underscore_merge_no_duplicates() {
        let parser = RustParser;
        // `use env_logger` produces "env_logger" in name_deps.
        // TOML block has "env-logger" as the key. Both are the same crate.
        let skill = r#"---
name: mylib
---

## Imports

```rust
use env_logger;
```

```toml
[dependencies]
env-logger = "0.11"
```
"#;
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        // Should have exactly ONE entry for env_logger, not two
        let env_deps: Vec<_> = deps
            .iter()
            .filter(|d| d.name.replace('-', "_") == "env_logger")
            .collect();
        assert_eq!(
            env_deps.len(),
            1,
            "env_logger/env-logger should be deduplicated: {:?}",
            deps.iter().map(|d| &d.name).collect::<Vec<_>>()
        );
        // And it should have the TOML spec
        assert!(
            env_deps[0].raw_spec.is_some(),
            "merged entry should have TOML spec"
        );
    }

    #[test]
    fn structured_deps_empty_skill() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Core Patterns\n";
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        assert!(deps.is_empty());
    }

    /// Regression: `tokio = { workspace = true }` in a direct TOML block (no
    /// [dependencies] header) should only be promoted if `tokio` already appears
    /// in name-only extraction (use/cargo-add). Unknown entries like `edition`
    /// or bare `workspace = true` deps not seen in imports must be filtered out.
    #[test]
    fn structured_deps_workspace_true_in_direct_toml_only_promotes_known() {
        let parser = RustParser;
        let skill = r#"---
name: mylib
---

## Imports

```rust
use tokio::runtime::Runtime;
```

```toml
tokio = { workspace = true }
edition = "2021"
```

## Core Patterns
"#;
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        // tokio is known from `use tokio::...`, so it should be promoted with its TOML spec
        let tokio_dep = deps.iter().find(|d| d.name == "tokio");
        assert!(
            tokio_dep.is_some(),
            "tokio should be promoted (known from use import): {:?}",
            deps.iter().map(|d| &d.name).collect::<Vec<_>>()
        );
        assert!(
            tokio_dep.unwrap().raw_spec.is_some(),
            "tokio should have raw_spec from TOML block"
        );
        // edition is NOT a real dep — it should be filtered out
        assert!(
            !deps.iter().any(|d| d.name == "edition"),
            "edition should be filtered out (not in name_deps): {:?}",
            deps.iter().map(|d| &d.name).collect::<Vec<_>>()
        );
    }

    /// A TOML dep with leading whitespace bypasses the regex-based scanner in
    /// extract_dependencies but is still parsed by the TOML parser in
    /// extract_structured_dependencies.  The merge loop at lines 342-349 should
    /// add it as an extra Manifest dep.
    #[test]
    fn structured_deps_toml_indented_dep_added_as_extra_manifest() {
        let parser = RustParser;
        // The indented `  anyhow = "1"` line will NOT match CARGO_TOML_DEP_RE
        // (requires ^[a-zA-Z_]), but the TOML parser handles it fine.
        let skill = "---\nname: test\n---\n\n## Imports\n\n```rust\nuse serde::Serialize;\n```\n\n```toml\n[dependencies]\nserde = \"1\"\n  anyhow = \"1\"\n```\n\n## Core Patterns\n";
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        // serde is in both use-imports and TOML → Manifest
        let serde_dep = deps.iter().find(|d| d.name == "serde").unwrap();
        assert_eq!(serde_dep.source, DepSource::Manifest);

        // anyhow is ONLY in TOML (missed by regex), not in name_deps → extra Manifest
        let anyhow_dep = deps.iter().find(|d| d.name == "anyhow");
        assert!(
            anyhow_dep.is_some(),
            "indented TOML dep should be added as extra manifest: {:?}",
            deps.iter().map(|d| &d.name).collect::<Vec<_>>()
        );
        assert_eq!(anyhow_dep.unwrap().source, DepSource::Manifest);
        assert!(anyhow_dep.unwrap().raw_spec.is_some());
    }

    /// Malformed TOML in a ```toml fence should be silently ignored.
    #[test]
    fn structured_deps_malformed_toml_ignored() {
        let parser = RustParser;
        let skill = r#"---
name: test
---

## Imports

```rust
use serde::Serialize;
```

```toml
[dependencies
this is = not { valid toml
```

## Core Patterns
"#;
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        // serde should still be extracted from use-import, just without TOML spec
        let serde_dep = deps.iter().find(|d| d.name == "serde").unwrap();
        assert_eq!(serde_dep.source, DepSource::Pattern);
        assert!(
            serde_dep.raw_spec.is_none(),
            "malformed TOML should not produce a raw_spec"
        );
    }

    /// A duplicate dep name in the [dependencies] section should be deduplicated.
    #[test]
    fn cargo_toml_duplicate_in_deps_section_deduplicated() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```toml\n[dependencies]\ntokio = \"1\"\ntokio = \"2\"\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(
            deps.iter().filter(|d| d.as_str() == "tokio").count(),
            1,
            "duplicate deps in [dependencies] should be deduplicated: {:?}",
            deps
        );
    }

    /// A stdlib crate appearing in [dependencies] should be filtered out.
    #[test]
    fn cargo_toml_stdlib_in_deps_section_filtered() {
        let parser = RustParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```toml\n[dependencies]\nstd = \"1\"\ntokio = \"1\"\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            !deps.iter().any(|d| d == "std"),
            "stdlib crate in [dependencies] should be filtered: {:?}",
            deps
        );
        assert!(deps.contains(&"tokio".to_string()));
    }

    /// When a crate from #[attr::macro] in Core Patterns is already in deps
    /// from the Imports section, it should not be duplicated.
    #[test]
    fn attr_crate_already_in_deps_not_duplicated() {
        let parser = RustParser;
        let skill = r#"---
name: test
---

## Imports

```rust
use tokio::runtime::Runtime;
```

## Core Patterns

### Async Main

```rust
#[tokio::main]
async fn main() {
    println!("hello");
}
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(
            deps.iter().filter(|d| d.as_str() == "tokio").count(),
            1,
            "tokio from attr macro should not duplicate use-import: {:?}",
            deps
        );
    }

    /// Multiple TOML fences should all contribute specs (not just the last one).
    #[test]
    fn structured_deps_multiple_toml_fences_accumulated() {
        let parser = RustParser;
        let skill = r#"---
name: test
---

## Imports

```rust
use serde::Serialize;
use tokio::runtime;
```

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
```

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
```

## Core Patterns
"#;
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        let serde_dep = deps.iter().find(|d| d.name == "serde").unwrap();
        assert!(
            serde_dep.raw_spec.as_ref().unwrap().contains("derive"),
            "serde from first fence must be preserved: {:?}",
            serde_dep.raw_spec
        );
        let tokio_dep = deps.iter().find(|d| d.name == "tokio").unwrap();
        assert!(
            tokio_dep.raw_spec.as_ref().unwrap().contains("full"),
            "tokio from second fence must also be captured: {:?}",
            tokio_dep.raw_spec
        );
    }

    /// Tilde-fenced TOML block (~~~toml) should be parsed for structured deps.
    #[test]
    fn structured_deps_tilde_toml_fence() {
        let parser = RustParser;
        let skill = r#"---
name: test
---

## Imports

```rust
use serde::Serialize;
```

~~~toml
[dependencies]
serde = { version = "1", features = ["derive"] }
~~~

## Core Patterns
"#;
        let deps = parser.extract_structured_dependencies(skill).unwrap();
        let serde_dep = deps.iter().find(|d| d.name == "serde").unwrap();
        assert_eq!(serde_dep.source, DepSource::Manifest);
        assert!(
            serde_dep.raw_spec.as_ref().unwrap().contains("features"),
            "tilde-fenced TOML should preserve features: {:?}",
            serde_dep.raw_spec
        );
    }

    /// [dependencies] section should exit on ~~~ fence close.
    #[test]
    fn deps_section_exits_on_tilde_fence_close() {
        let parser = RustParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n~~~toml\n[dependencies]\ntokio = \"1\"\n~~~\n\nNot a dep: fake_crate = \"nope\"\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"tokio".to_string()),
            "should find tokio before ~~~ close: {:?}",
            deps
        );
        assert!(
            !deps.contains(&"fake_crate".to_string()),
            "should stop after ~~~ fence close: {:?}",
            deps
        );
    }

    /// sanitize_dep_name correctly validates dep names extracted by extract_dependencies.
    /// The retain filter at lines 237-243 is a defensive guard: all current regex
    /// patterns (USE_IMPORT_RE, CARGO_TOML_DEP_RE, etc.) produce names that always
    /// pass sanitize_dep_name. This test verifies the guard exists and that valid
    /// deps pass through unchanged.
    #[test]
    fn extract_dependencies_retains_valid_deps_through_sanitize() {
        let parser = RustParser;
        // Dep names with hyphens and underscores — edge cases for sanitize_dep_name
        let skill = r#"---
name: test
---

## Imports

```toml
[dependencies]
my-crate = "1"
my_crate2 = "2"
A123 = "0.1"
```

## Core Patterns

### Example

```rust
fn main() {}
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"my-crate".to_string()),
            "hyphenated dep should pass sanitize: {:?}",
            deps
        );
        assert!(
            deps.contains(&"my_crate2".to_string()),
            "underscored dep should pass sanitize: {:?}",
            deps
        );
        assert!(
            deps.contains(&"A123".to_string()),
            "alphanumeric dep should pass sanitize: {:?}",
            deps
        );
    }

    /// sanitize_dep_name rejects names with invalid characters.
    /// This tests the sanitize function directly since the regex extractors
    /// cannot produce names that fail sanitization.
    #[test]
    fn sanitize_dep_name_rejects_invalid_chars() {
        use crate::util::sanitize_dep_name;
        assert!(sanitize_dep_name("valid-name").is_ok());
        assert!(sanitize_dep_name("also_valid").is_ok());
        // Space is not allowed
        assert!(sanitize_dep_name("bad name").is_err());
        // Semicolon is not allowed
        assert!(sanitize_dep_name("bad;name").is_err());
        // Backtick is not allowed
        assert!(sanitize_dep_name("bad`name").is_err());
        // Empty is rejected
        assert!(sanitize_dep_name("").is_err());
        // Leading hyphen (flag injection)
        assert!(sanitize_dep_name("-e").is_err());
    }
}
