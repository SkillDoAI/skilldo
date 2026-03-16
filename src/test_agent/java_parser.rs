//! Java-specific SKILL.md parser — extracts version, name, code patterns,
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
// Generic fence regex for finding code block boundaries (position only).
// Actual code extraction uses find_fenced_blocks for proper tag-aware parsing.
static CODE_BLOCK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(?:```|~~~)[^\n]*\n([\s\S]*?)(?:```|~~~)").unwrap());
static MAVEN_COORD_RE: Lazy<Regex> = Lazy::new(|| {
    // Match Maven coordinates: group:artifact or group:artifact:version
    // Group must start with a letter (excludes port numbers like 11434).
    // Version may include range syntax like [0,) or [1.0,2.0)
    Regex::new(
        r"([a-zA-Z][a-zA-Z0-9._-]*):([a-zA-Z][a-zA-Z0-9._-]*)(?::([a-zA-Z0-9._\[\],\(\)-]+))?",
    )
    .unwrap()
});

/// Java-specific parser for SKILL.md files
pub struct JavaParser;

impl JavaParser {
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
        } else if text.contains("error")
            || text.contains("exception")
            || text.contains("handle")
            || text.contains("catch")
        {
            PatternCategory::ErrorHandling
        } else if text.contains("thread")
            || text.contains("executor")
            || text.contains("concurrent")
            || text.contains("async")
            || text.contains("completablefuture")
            || text.contains("future")
        {
            PatternCategory::AsyncPattern
        } else {
            PatternCategory::Other
        }
    }
}

impl LanguageParser for JavaParser {
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

            let desc_end = code_block_start.max(description_start);
            let description = pattern_section[description_start..desc_end]
                .trim()
                .to_string();

            if let Some(code_cap) = CODE_BLOCK_RE.captures(pattern_section) {
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

        // Only scan ## Imports for Maven coordinates — Core Patterns contains
        // Java source examples that produce false positives (e.g., "step:done").
        let sections_to_scan = [r"(?m)^##\s+Imports\s*$"];

        for section_re in &sections_to_scan {
            if let Some(content) = extract_section(skill_md, section_re)? {
                for cap in MAVEN_COORD_RE.captures_iter(content) {
                    let coord = cap[0].to_string();
                    // Accept any group:artifact coord — dots in groupId are
                    // convention, not required (e.g., junit:junit is valid)
                    if !dependencies.contains(&coord) {
                        dependencies.push(coord);
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SKILL: &str = r#"---
name: gson
version: 2.10.1
language: java
---

# Gson

JSON serialization/deserialization for Java.

## Imports

```java
import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import java.util.List;
```

Maven:
```xml
com.google.code.gson:gson:2.10.1
```

## Core Patterns

### Basic Serialization

Convert Java objects to JSON strings.

```java
Gson gson = new Gson();
String json = gson.toJson(new int[]{1, 2, 3});
System.out.println(json);
```

### Error Handling with Malformed JSON

Handle parsing errors gracefully.

```java
try {
    Gson gson = new Gson();
    gson.fromJson("invalid", Map.class);
} catch (Exception e) {
    System.out.println("Parse error: " + e.getMessage());
}
```

### Custom Configuration

Configure Gson with GsonBuilder.

```java
Gson gson = new GsonBuilder()
    .setPrettyPrinting()
    .serializeNulls()
    .create();
String json = gson.toJson(Map.of("key", "value"));
```
"#;

    #[test]
    fn extract_patterns_from_java_skill() {
        let parser = JavaParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert_eq!(patterns.len(), 3);
        assert_eq!(patterns[0].name, "Basic Serialization");
        assert_eq!(patterns[1].name, "Error Handling with Malformed JSON");
        assert_eq!(patterns[2].name, "Custom Configuration");
    }

    #[test]
    fn extract_pattern_categories() {
        let parser = JavaParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert_eq!(patterns[0].category, PatternCategory::BasicUsage);
        assert_eq!(patterns[1].category, PatternCategory::ErrorHandling);
        assert_eq!(patterns[2].category, PatternCategory::Configuration);
    }

    #[test]
    fn extract_pattern_code_content() {
        let parser = JavaParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert!(patterns[0].code.contains("Gson"));
        assert!(patterns[0].code.contains("toJson"));
    }

    #[test]
    fn extract_dependencies_only_maven_coords() {
        let parser = JavaParser;
        let deps = parser.extract_dependencies(SAMPLE_SKILL).unwrap();
        // Only Maven coordinates (with ':') should be in deps, not import class names
        assert!(
            deps.iter().all(|d| d.contains(':')),
            "deps should only contain Maven coordinates, not class names: {:?}",
            deps
        );
        assert!(
            !deps.iter().any(|d| d.starts_with("java.")),
            "stdlib should not be in deps"
        );
    }

    #[test]
    fn extract_maven_coordinates() {
        let parser = JavaParser;
        let deps = parser.extract_dependencies(SAMPLE_SKILL).unwrap();
        assert!(
            deps.iter().any(|d| d.contains("com.google.code.gson:gson")),
            "should extract Maven coordinates"
        );
    }

    #[test]
    fn extract_version_from_frontmatter() {
        let parser = JavaParser;
        assert_eq!(
            parser.extract_version(SAMPLE_SKILL).unwrap(),
            Some("2.10.1".into())
        );
    }

    #[test]
    fn extract_name_from_frontmatter() {
        let parser = JavaParser;
        assert_eq!(
            parser.extract_name(SAMPLE_SKILL).unwrap(),
            Some("gson".into())
        );
    }

    #[test]
    fn extract_version_unknown_returns_none() {
        let parser = JavaParser;
        let skill = "---\nname: test\nversion: unknown\n---\n";
        assert_eq!(parser.extract_version(skill).unwrap(), None);
    }

    #[test]
    fn extract_version_missing_returns_none() {
        let parser = JavaParser;
        let skill = "---\nname: test\n---\n";
        assert_eq!(parser.extract_version(skill).unwrap(), None);
    }

    #[test]
    fn no_core_patterns_section_returns_empty() {
        let parser = JavaParser;
        let skill = "---\nname: test\n---\n\n# Test\n\nSome text.\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn no_imports_section_returns_empty() {
        let parser = JavaParser;
        let skill = "---\nname: test\n---\n\n# Test\n\nSome text.\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn categorize_thread_pattern() {
        assert_eq!(
            JavaParser::categorize_pattern("Thread Pool", "Run concurrent tasks"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_exception_pattern() {
        assert_eq!(
            JavaParser::categorize_pattern("Exception Handling", "Catch errors"),
            PatternCategory::ErrorHandling
        );
    }

    #[test]
    fn core_patterns_section_no_code_blocks_errors() {
        let parser = JavaParser;
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
    fn extract_name_missing_returns_none() {
        let parser = JavaParser;
        let skill = "---\ndescription: no name field\n---\n\n## Overview\n";
        assert_eq!(parser.extract_name(skill).unwrap(), None);
    }

    #[test]
    fn extract_deps_skips_import_class_names() {
        let parser = JavaParser;
        let skill = r#"---
name: test
---

## Imports

```java
import static org.junit.Assert.assertEquals;
import com.google.gson.Gson;
```

Add to pom.xml:
```xml
<dependency>
  <groupId>org.junit</groupId>
  <artifactId>junit</artifactId>
</dependency>
```

`com.google.code.gson:gson:2.10.1`
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        // Class names should NOT be in deps
        assert!(!deps.contains(&"org.junit.Assert.assertEquals".to_string()));
        assert!(!deps.contains(&"com.google.gson.Gson".to_string()));
        // Maven coordinates SHOULD be in deps
        assert!(deps.iter().any(|d| d.contains("com.google.code.gson:gson")));
    }

    #[test]
    fn tilde_fenced_code_block_extracted() {
        let parser = JavaParser;
        let skill = "---\nname: test\n---\n\n## Core Patterns\n\n### Hello\n\n~~~java\nSystem.out.println(\"hello\");\n~~~\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert!(
            !patterns.is_empty(),
            "should extract pattern from ~~~java fence"
        );
        assert!(
            patterns[0].code.contains("println"),
            "code should contain println"
        );
    }

    #[test]
    fn categorize_async_via_executor_keyword() {
        assert_eq!(
            JavaParser::categorize_pattern("Executor Service", "Run tasks with executor"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_async_via_concurrent_keyword() {
        assert_eq!(
            JavaParser::categorize_pattern("Concurrent Map", "Using concurrent collections"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_async_via_async_keyword() {
        assert_eq!(
            JavaParser::categorize_pattern("Async Processing", "Process items asynchronously"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_async_via_completablefuture() {
        assert_eq!(
            JavaParser::categorize_pattern("CompletableFuture", "Chain completablefuture tasks"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_async_via_future_keyword() {
        assert_eq!(
            JavaParser::categorize_pattern("Future Result", "Get a future value"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_other_when_no_keywords_match() {
        assert_eq!(
            JavaParser::categorize_pattern("Data Transform", "Convert between formats"),
            PatternCategory::Other
        );
    }

    #[test]
    fn extract_dependencies_drops_invalid_maven_coords() {
        let parser = JavaParser;
        let skill = r#"---
name: test
---

## Imports

`com.example.valid:dep:1.0`
`com.example.bad dep:lib:2.0`
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"com.example.valid:dep:1.0".to_string()));
        // "com.example.bad dep:lib:2.0" has a space — sanitize_dep_name rejects it
        assert!(
            !deps.iter().any(|d| d.contains("bad dep")),
            "invalid deps should be dropped"
        );
    }

    #[test]
    fn extract_patterns_success_exercises_debug_log() {
        // This test covers the debug log path at line 124
        let parser = JavaParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert_eq!(patterns.len(), 3, "should log and return 3 patterns");
    }

    #[test]
    fn extract_dependencies_with_maven_and_imports_exercises_debug_log() {
        // Exercises the debug log at line 166 by extracting deps successfully
        let parser = JavaParser;
        let deps = parser.extract_dependencies(SAMPLE_SKILL).unwrap();
        assert!(!deps.is_empty(), "should have deps and log count");
    }

    #[test]
    fn maven_coord_without_dots_in_group_accepted() {
        let parser = JavaParser;
        let skill = r#"---
name: test
---

## Imports

```
junit:junit:4.13.2
com.real.group:artifact:2.0
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        // Both should be accepted — dots in groupId are convention, not required
        assert!(
            deps.iter().any(|d| d.starts_with("junit:")),
            "dot-less groupId like junit:junit should be accepted"
        );
        assert!(
            deps.iter().any(|d| d.contains("com.real.group")),
            "dotted groupId should be accepted"
        );
    }
}
