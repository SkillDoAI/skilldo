//! Go-specific SKILL.md parser — extracts version, name, code patterns,
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
    Lazy::new(|| Regex::new(r"(?i)```(?:go(?:lang)?)?\n([\s\S]*?)```").unwrap());
static SINGLE_IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?m)import\s+"([^"]+)""#).unwrap());
static GROUP_IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?s)import\s*\(\s*(.*?)\s*\)"#).unwrap());
static IMPORT_LINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#""([^"]+)""#).unwrap());
static GO_GET_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"go\s+get\s+([a-zA-Z0-9._/\-@]+)").unwrap());

/// Go-specific parser for SKILL.md files
pub struct GoParser;

impl GoParser {
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
        } else if text.contains("goroutine")
            || text.contains("channel")
            || text.contains("concurrent")
            || text.contains("async")
            || text.contains("context")
        {
            PatternCategory::AsyncPattern
        } else {
            PatternCategory::Other
        }
    }

    /// Check if a Go import path is from the standard library.
    fn is_stdlib_package(name: &str) -> bool {
        // Go stdlib packages (top-level). If the import starts with any of these
        // and doesn't contain a dot (which indicates a domain-based import), it's stdlib.
        const STDLIB_PACKAGES: &[&str] = &[
            "archive",
            "bufio",
            "bytes",
            "cmp",
            "compress",
            "container",
            "context",
            "crypto",
            "database",
            "debug",
            "embed",
            "encoding",
            "errors",
            "expvar",
            "flag",
            "fmt",
            "go",
            "hash",
            "html",
            "image",
            "index",
            "internal",
            "io",
            "iter",
            "log",
            "maps",
            "math",
            "mime",
            "net",
            "os",
            "path",
            "plugin",
            "reflect",
            "regexp",
            "runtime",
            "slices",
            "sort",
            "strconv",
            "strings",
            "structs",
            "sync",
            "syscall",
            "testing",
            "text",
            "time",
            "unicode",
            "unsafe",
        ];

        // Go stdlib imports never contain a dot in the first segment
        if name.contains('.') {
            return false;
        }

        // Check if the top-level package matches stdlib
        let top = name.split('/').next().unwrap_or(name);
        STDLIB_PACKAGES.contains(&top)
    }
}

impl LanguageParser for GoParser {
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

        let imports_content = match extract_section(skill_md, r"(?m)^##\s+Imports\s*$")? {
            Some(s) => s,
            None => {
                debug!("No Imports section found in SKILL.md");
                return Ok(dependencies);
            }
        };

        // Go single import: import "github.com/foo/bar"
        for cap in SINGLE_IMPORT_RE.captures_iter(imports_content) {
            let pkg = cap[1].to_string();
            if !Self::is_stdlib_package(&pkg) && !dependencies.contains(&pkg) {
                dependencies.push(pkg);
            }
        }

        // Go grouped import block: import ( ... )
        if let Some(group_cap) = GROUP_IMPORT_RE.captures(imports_content) {
            let block = &group_cap[1];
            for cap in IMPORT_LINE_RE.captures_iter(block) {
                let pkg = cap[1].to_string();
                if !Self::is_stdlib_package(&pkg) && !dependencies.contains(&pkg) {
                    dependencies.push(pkg);
                }
            }
        }

        // go get instructions: go get github.com/foo/bar
        for cap in GO_GET_RE.captures_iter(imports_content) {
            let pkg = cap[1].to_string();
            if !dependencies.contains(&pkg) {
                dependencies.push(pkg);
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
name: chi
version: 5.1.0
language: go
---

# chi

Lightweight HTTP router for Go.

## Imports

```go
import (
    "fmt"
    "net/http"
    "github.com/go-chi/chi/v5"
    "github.com/go-chi/chi/v5/middleware"
)
```

## Core Patterns

### Basic Router Setup

Create a chi router and add routes.

```go
r := chi.NewRouter()
r.Use(middleware.Logger)
r.Get("/", func(w http.ResponseWriter, r *http.Request) {
    w.Write([]byte("hello"))
})
http.ListenAndServe(":3000", r)
```

### Error Handling with Middleware

Handle panics gracefully using middleware.

```go
r := chi.NewRouter()
r.Use(middleware.Recoverer)
r.Get("/panic", func(w http.ResponseWriter, r *http.Request) {
    panic("something went wrong")
})
```

### Route Groups

Group routes under a common prefix.

```go
r := chi.NewRouter()
r.Route("/api", func(r chi.Router) {
    r.Get("/users", listUsers)
    r.Post("/users", createUser)
})
```
"#;

    #[test]
    fn extract_patterns_from_go_skill() {
        let parser = GoParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert_eq!(patterns.len(), 3);
        assert_eq!(patterns[0].name, "Basic Router Setup");
        assert_eq!(patterns[1].name, "Error Handling with Middleware");
        assert_eq!(patterns[2].name, "Route Groups");
    }

    #[test]
    fn extract_pattern_categories() {
        let parser = GoParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert_eq!(patterns[0].category, PatternCategory::BasicUsage);
        assert_eq!(patterns[1].category, PatternCategory::ErrorHandling);
        assert_eq!(patterns[2].category, PatternCategory::Other);
    }

    #[test]
    fn extract_pattern_code_content() {
        let parser = GoParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert!(patterns[0].code.contains("chi.NewRouter()"));
        assert!(patterns[0].code.contains("middleware.Logger"));
    }

    #[test]
    fn extract_dependencies_filters_stdlib() {
        let parser = GoParser;
        let deps = parser.extract_dependencies(SAMPLE_SKILL).unwrap();
        assert!(deps.contains(&"github.com/go-chi/chi/v5".to_string()));
        assert!(deps.contains(&"github.com/go-chi/chi/v5/middleware".to_string()));
        assert!(
            !deps.iter().any(|d| d == "fmt"),
            "stdlib should be filtered"
        );
        assert!(
            !deps.iter().any(|d| d == "net/http"),
            "stdlib should be filtered"
        );
    }

    #[test]
    fn extract_version_from_frontmatter() {
        let parser = GoParser;
        assert_eq!(
            parser.extract_version(SAMPLE_SKILL).unwrap(),
            Some("5.1.0".into())
        );
    }

    #[test]
    fn extract_name_from_frontmatter() {
        let parser = GoParser;
        assert_eq!(
            parser.extract_name(SAMPLE_SKILL).unwrap(),
            Some("chi".into())
        );
    }

    #[test]
    fn extract_version_unknown_returns_none() {
        let parser = GoParser;
        let skill = "---\nname: test\nversion: unknown\n---\n";
        assert_eq!(parser.extract_version(skill).unwrap(), None);
    }

    #[test]
    fn extract_version_missing_returns_none() {
        let parser = GoParser;
        let skill = "---\nname: test\n---\n";
        assert_eq!(parser.extract_version(skill).unwrap(), None);
    }

    #[test]
    fn no_core_patterns_section_returns_empty() {
        let parser = GoParser;
        let skill = "---\nname: test\n---\n\n# Test\n\nSome text.\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn no_imports_section_returns_empty() {
        let parser = GoParser;
        let skill = "---\nname: test\n---\n\n# Test\n\nSome text.\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn is_stdlib_package_basics() {
        assert!(GoParser::is_stdlib_package("fmt"));
        assert!(GoParser::is_stdlib_package("net/http"));
        assert!(GoParser::is_stdlib_package("encoding/json"));
        assert!(GoParser::is_stdlib_package("os"));
        assert!(GoParser::is_stdlib_package("context"));
        assert!(GoParser::is_stdlib_package("sync"));
        assert!(GoParser::is_stdlib_package("io"));
        assert!(GoParser::is_stdlib_package("strings"));
        assert!(GoParser::is_stdlib_package("testing"));
    }

    #[test]
    fn is_stdlib_rejects_external_packages() {
        assert!(!GoParser::is_stdlib_package("github.com/foo/bar"));
        assert!(!GoParser::is_stdlib_package("golang.org/x/net"));
        assert!(!GoParser::is_stdlib_package("github.com/go-chi/chi/v5"));
    }

    #[test]
    fn categorize_goroutine_pattern() {
        assert_eq!(
            GoParser::categorize_pattern("Goroutine Pool", "Run concurrent workers"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_channel_pattern() {
        assert_eq!(
            GoParser::categorize_pattern("Channel Communication", "Send data between goroutines"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_context_pattern() {
        assert_eq!(
            GoParser::categorize_pattern("Context Cancellation", "Cancel operations with context"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn single_import_dependency_extraction() {
        let parser = GoParser;
        let skill = r#"---
name: test
---

## Imports

```go
import "github.com/pkg/errors"
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(deps, vec!["github.com/pkg/errors"]);
    }

    #[test]
    fn go_get_dependency_extraction() {
        let parser = GoParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```bash\ngo get github.com/spf13/cobra\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"github.com/spf13/cobra".to_string()));
    }

    #[test]
    fn deduplicates_dependencies() {
        let parser = GoParser;
        let skill = r#"---
name: test
---

## Imports

```go
import "github.com/foo/bar"
```

```bash
go get github.com/foo/bar
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(
            deps.iter()
                .filter(|d| d.as_str() == "github.com/foo/bar")
                .count(),
            1,
            "should deduplicate"
        );
    }

    #[test]
    fn extract_patterns_golang_fence() {
        let parser = GoParser;
        let skill = "---\nname: test\n---\n\n## Core Patterns\n\n### Basic\n\nA simple example.\n\n```golang\nfmt.Println(\"hello\")\n```\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert_eq!(patterns.len(), 1);
        assert!(patterns[0].code.contains("Println"));
    }

    #[test]
    fn extract_patterns_plain_fence() {
        let parser = GoParser;
        let skill = "---\nname: test\n---\n\n## Core Patterns\n\n### Basic\n\nA simple example.\n\n```\nfmt.Println(\"hello\")\n```\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert_eq!(patterns.len(), 1);
    }

    #[test]
    fn categorize_configuration_pattern() {
        assert_eq!(
            GoParser::categorize_pattern("Database Config", "Set up the connection"),
            PatternCategory::Configuration
        );
    }

    #[test]
    fn core_patterns_section_no_code_blocks_errors() {
        let parser = GoParser;
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
    fn dependency_with_leading_hyphen_dropped_by_sanitizer() {
        let parser = GoParser;
        // `go get -e` — the regex captures `-e`; sanitize_dep_name rejects leading '-'
        let skill = "---\nname: test\n---\n\n## Imports\n\n```bash\ngo get -e\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            !deps.contains(&"-e".to_string()),
            "leading-hyphen dep should be dropped by sanitize_dep_name"
        );
    }
}
