//! JavaScript/TypeScript-specific SKILL.md parser — extracts version, name,
//! code patterns, and dependencies from a generated SKILL.md file. Used by the
//! test agent to understand what to validate.

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use tracing::{debug, warn};

use super::parser::{extract_section, CodePattern, PatternCategory};
use super::LanguageParser;
use crate::util::sanitize_dep_name;

// Cached regexes for pattern/dependency extraction
static PATTERN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^###\s+(.+?)$").unwrap());
static CODE_BLOCK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?im)^(?:```|~~~)(?:(?:js|javascript|typescript|ts|jsx|tsx)?)?\n([\s\S]*?)\n(?:```|~~~)",
    )
    .unwrap()
});
static IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?m)^import\s+[\s\S]*?from\s+['"]([^'"]+)['"]"#).unwrap());
/// Side-effect imports: `import 'dotenv/config'` (no `from` keyword).
static SIDE_EFFECT_IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?m)^import\s+['"]([^'"]+)['"]"#).unwrap());
static REQUIRE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"require\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap());
static NPM_INSTALL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)npm\s+(?:install|i|add)\s+(?:--save\s+|--save-dev\s+|-S\s+|-D\s+)*(.+?)(?:\s*$|`)",
    )
    .unwrap()
});

/// JavaScript/TypeScript-specific parser for SKILL.md files
pub struct JsParser;

impl JsParser {
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
        } else if text.contains("config")
            || text.contains("setup")
            || text.contains("initialize")
            || text.contains("environment")
        {
            PatternCategory::Configuration
        } else if text.contains("async")
            || text.contains("await")
            || text.contains("promise")
            || text.contains("callback")
            || text.contains("event")
            || text.contains("stream")
            || text.contains("observable")
        {
            PatternCategory::AsyncPattern
        } else if text.contains("error")
            || text.contains("handle")
            || text.contains("catch")
            || text.contains("throw")
            || text.contains("reject")
        {
            PatternCategory::ErrorHandling
        } else {
            PatternCategory::Other
        }
    }

    /// Check if a module name is a Node.js built-in module.
    /// Handles the `node:` prefix (e.g. `node:fs` → checks "fs") and
    /// subpath imports (e.g. `fs/promises` → checks "fs").
    fn is_builtin_module(name: &str) -> bool {
        let name = name.strip_prefix("node:").unwrap_or(name);
        // Strip subpath: fs/promises → fs, assert/strict → assert
        let name = match name.find('/') {
            Some(i) if !name.starts_with('@') => &name[..i],
            _ => name,
        };

        const BUILTIN_MODULES: &[&str] = &[
            "assert",
            "async_hooks",
            "buffer",
            "child_process",
            "cluster",
            "console",
            "constants",
            "crypto",
            "dgram",
            "diagnostics_channel",
            "dns",
            "domain",
            "events",
            "fs",
            "http",
            "http2",
            "https",
            "inspector",
            "module",
            "net",
            "os",
            "path",
            "perf_hooks",
            "process",
            "punycode",
            "querystring",
            "readline",
            "repl",
            "sea",
            "stream",
            "string_decoder",
            "sys",
            "sqlite",
            "test",
            "timers",
            "tls",
            "trace_events",
            "tty",
            "url",
            "util",
            "v8",
            "vm",
            "wasi",
            "worker_threads",
            "zlib",
        ];

        BUILTIN_MODULES.contains(&name)
    }
}

impl LanguageParser for JsParser {
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

        // ES module imports: import ... from 'package'
        for cap in IMPORT_RE.captures_iter(imports_content) {
            let pkg = cap[1].to_string();
            if !Self::is_relative_import(&pkg)
                && !Self::is_builtin_module(&pkg)
                && !dependencies.contains(&pkg)
            {
                dependencies.push(pkg);
            }
        }

        // Side-effect imports: import 'dotenv/config'
        for cap in SIDE_EFFECT_IMPORT_RE.captures_iter(imports_content) {
            let pkg = cap[1].to_string();
            if !Self::is_relative_import(&pkg)
                && !Self::is_builtin_module(&pkg)
                && !dependencies.contains(&pkg)
            {
                dependencies.push(pkg);
            }
        }

        // CommonJS requires: require('package')
        for cap in REQUIRE_RE.captures_iter(imports_content) {
            let pkg = cap[1].to_string();
            if !Self::is_relative_import(&pkg)
                && !Self::is_builtin_module(&pkg)
                && !dependencies.contains(&pkg)
            {
                dependencies.push(pkg);
            }
        }

        // npm install instructions: npm install express body-parser
        // Capture the full tail and split by whitespace to get each package.
        for cap in NPM_INSTALL_RE.captures_iter(imports_content) {
            for name in cap[1].split_whitespace() {
                let pkg = name.to_string();
                // Skip flags (--save, -D, etc.) and relative/internal path specs
                if pkg.starts_with('-') || Self::is_relative_import(&pkg) {
                    continue;
                }
                if !Self::is_builtin_module(&pkg) && !dependencies.contains(&pkg) {
                    dependencies.push(pkg);
                }
            }
        }

        // Normalize subpath imports: `lodash/chunk` → `lodash`, `@scope/pkg/foo` → `@scope/pkg`
        let dependencies: Vec<String> = dependencies
            .into_iter()
            .map(|dep| Self::normalize_package_name(&dep))
            .collect();

        // Deduplicate after normalization
        let mut seen = Vec::new();
        for dep in &dependencies {
            if !seen.contains(dep) {
                seen.push(dep.clone());
            }
        }

        seen.retain(|dep| match sanitize_dep_name(dep) {
            Ok(_) => true,
            Err(e) => {
                warn!("Dropping invalid dependency at ingestion: {}", e);
                false
            }
        });

        debug!("Extracted {} dependencies from SKILL.md", seen.len());
        Ok(seen)
    }
}

impl JsParser {
    /// Check if an import path is relative or internal (`./`, `/`, `#` subpath imports).
    fn is_relative_import(name: &str) -> bool {
        name.starts_with('.') || name.starts_with('/') || name.starts_with('#')
    }

    /// Normalize subpath imports to the root package name.
    /// `lodash/chunk` → `lodash`, `@scope/pkg/utils` → `@scope/pkg`, `express` → `express`
    fn normalize_package_name(name: &str) -> String {
        if name.starts_with('@') {
            // Scoped package: @scope/name/subpath → @scope/name
            let parts: Vec<&str> = name.splitn(3, '/').collect();
            if parts.len() >= 2 {
                format!("{}/{}", parts[0], parts[1])
            } else {
                name.to_string()
            }
        } else {
            // Unscoped package: lodash/chunk → lodash
            name.split('/').next().unwrap_or(name).to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SKILL: &str = r#"---
name: express
version: 4.18.2
language: javascript
---

# express

Fast, unopinionated web framework for Node.js.

## Imports

```javascript
import express from 'express';
import { Router } from 'express';
const path = require('path');
const fs = require('fs');
```

## Core Patterns

### Basic Server Setup

Create an Express server and listen on a port.

```javascript
const app = express();
app.get('/', (req, res) => {
    res.send('Hello World');
});
app.listen(3000);
```

### Error Handling Middleware

Handle errors with Express middleware.

```javascript
app.use((err, req, res, next) => {
    console.error(err.stack);
    res.status(500).send('Something broke!');
});
```

### Async Route Handler

Handle async operations in routes.

```javascript
app.get('/users', async (req, res) => {
    const users = await User.find();
    res.json(users);
});
```
"#;

    #[test]
    fn extract_patterns_from_js_skill() {
        let parser = JsParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert_eq!(patterns.len(), 3);
        assert_eq!(patterns[0].name, "Basic Server Setup");
        assert_eq!(patterns[1].name, "Error Handling Middleware");
        assert_eq!(patterns[2].name, "Async Route Handler");
    }

    #[test]
    fn extract_pattern_categories() {
        let parser = JsParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert_eq!(patterns[0].category, PatternCategory::BasicUsage);
        assert_eq!(patterns[1].category, PatternCategory::ErrorHandling);
        assert_eq!(patterns[2].category, PatternCategory::AsyncPattern);
    }

    #[test]
    fn extract_pattern_code_content() {
        let parser = JsParser;
        let patterns = parser.extract_patterns(SAMPLE_SKILL).unwrap();
        assert!(patterns[0].code.contains("express()"));
        assert!(patterns[0].code.contains("app.listen"));
    }

    #[test]
    fn extract_dependencies_filters_builtins() {
        let parser = JsParser;
        let deps = parser.extract_dependencies(SAMPLE_SKILL).unwrap();
        assert!(deps.contains(&"express".to_string()));
        assert!(
            !deps.iter().any(|d| d == "path"),
            "builtin should be filtered"
        );
        assert!(
            !deps.iter().any(|d| d == "fs"),
            "builtin should be filtered"
        );
    }

    #[test]
    fn extract_version_from_frontmatter() {
        let parser = JsParser;
        assert_eq!(
            parser.extract_version(SAMPLE_SKILL).unwrap(),
            Some("4.18.2".into())
        );
    }

    #[test]
    fn extract_name_from_frontmatter() {
        let parser = JsParser;
        assert_eq!(
            parser.extract_name(SAMPLE_SKILL).unwrap(),
            Some("express".into())
        );
    }

    #[test]
    fn extract_version_unknown_returns_none() {
        let parser = JsParser;
        let skill = "---\nname: test\nversion: unknown\n---\n";
        assert_eq!(parser.extract_version(skill).unwrap(), None);
    }

    #[test]
    fn extract_version_missing_returns_none() {
        let parser = JsParser;
        let skill = "---\nname: test\n---\n";
        assert_eq!(parser.extract_version(skill).unwrap(), None);
    }

    #[test]
    fn no_core_patterns_section_returns_empty() {
        let parser = JsParser;
        let skill = "---\nname: test\n---\n\n# Test\n\nSome text.\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert!(patterns.is_empty());
    }

    #[test]
    fn no_imports_section_returns_empty() {
        let parser = JsParser;
        let skill = "---\nname: test\n---\n\n# Test\n\nSome text.\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.is_empty());
    }

    #[test]
    fn is_builtin_module_basics() {
        assert!(JsParser::is_builtin_module("fs"));
        assert!(JsParser::is_builtin_module("path"));
        assert!(JsParser::is_builtin_module("http"));
        assert!(JsParser::is_builtin_module("crypto"));
        assert!(JsParser::is_builtin_module("os"));
        assert!(JsParser::is_builtin_module("events"));
        assert!(JsParser::is_builtin_module("stream"));
        assert!(JsParser::is_builtin_module("util"));
        assert!(JsParser::is_builtin_module("child_process"));
    }

    #[test]
    fn is_builtin_module_node_prefix() {
        assert!(JsParser::is_builtin_module("node:fs"));
        assert!(JsParser::is_builtin_module("node:path"));
        assert!(JsParser::is_builtin_module("node:http"));
    }

    #[test]
    fn is_builtin_module_subpath() {
        assert!(JsParser::is_builtin_module("fs/promises"));
        assert!(JsParser::is_builtin_module("assert/strict"));
        assert!(JsParser::is_builtin_module("node:fs/promises"));
        // Scoped packages should NOT be treated as builtins
        assert!(!JsParser::is_builtin_module("@scope/pkg"));
    }

    #[test]
    fn is_builtin_rejects_external_packages() {
        assert!(!JsParser::is_builtin_module("express"));
        assert!(!JsParser::is_builtin_module("lodash"));
        assert!(!JsParser::is_builtin_module("react"));
    }

    #[test]
    fn categorize_async_pattern() {
        assert_eq!(
            JsParser::categorize_pattern("Promise Chain", "Chain multiple promises"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn categorize_callback_pattern() {
        assert_eq!(
            JsParser::categorize_pattern("Event Listener", "Listen for DOM events"),
            PatternCategory::AsyncPattern
        );
    }

    #[test]
    fn commonjs_require_dependency_extraction() {
        let parser = JsParser;
        let skill = r#"---
name: test
---

## Imports

```javascript
const express = require('express');
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(deps, vec!["express"]);
    }

    #[test]
    fn npm_install_dependency_extraction() {
        let parser = JsParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```bash\nnpm install express\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"express".to_string()));
    }

    #[test]
    fn npm_install_multiple_packages() {
        let parser = JsParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```bash\nnpm install express body-parser cors\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"express".to_string()));
        assert!(deps.contains(&"body-parser".to_string()));
        assert!(deps.contains(&"cors".to_string()));
    }

    #[test]
    fn npm_i_shorthand() {
        let parser = JsParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```bash\nnpm i express body-parser\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"express".to_string()));
        assert!(deps.contains(&"body-parser".to_string()));
    }

    #[test]
    fn npm_add_shorthand() {
        let parser = JsParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```bash\nnpm add lodash\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"lodash".to_string()));
    }

    #[test]
    fn npm_install_with_flags_and_multiple_packages() {
        let parser = JsParser;
        let skill = "---\nname: test\n---\n\n## Imports\n\n```bash\nnpm install --save express morgan\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"express".to_string()));
        assert!(deps.contains(&"morgan".to_string()));
    }

    #[test]
    fn npm_install_skips_mid_flags() {
        let parser = JsParser;
        // Flags appearing after package names (e.g. --save-exact) should be skipped
        let skill = "---\nname: test\n---\n\n## Imports\n\n```bash\nnpm install express --save-exact body-parser\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"express".to_string()));
        assert!(deps.contains(&"body-parser".to_string()));
        assert!(
            !deps.iter().any(|d| d.starts_with('-')),
            "flags should not appear as deps: {:?}",
            deps
        );
    }

    #[test]
    fn npm_install_skips_relative_paths() {
        let parser = JsParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```bash\nnpm install ./vendor/pkg ../lib express\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(
            deps,
            vec!["express"],
            "relative paths should be filtered from npm install"
        );
    }

    #[test]
    fn import_side_effect() {
        let parser = JsParser;
        let skill =
            "---\nname: test\n---\n\n## Imports\n\n```javascript\nimport 'dotenv/config'\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"dotenv".to_string()),
            "side-effect import should capture 'dotenv', got: {:?}",
            deps
        );
    }

    #[test]
    fn import_multiline_destructured() {
        let parser = JsParser;
        let skill = r#"---
name: test
---

## Imports

```javascript
import {
    Router,
    Request,
    Response,
} from 'express';
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.contains(&"express".to_string()),
            "multi-line destructured import should capture 'express', got: {:?}",
            deps
        );
    }

    #[test]
    fn deduplicates_dependencies() {
        let parser = JsParser;
        let skill = r#"---
name: test
---

## Imports

```javascript
import express from 'express';
const express2 = require('express');
```

```bash
npm install express
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(
            deps.iter().filter(|d| d.as_str() == "express").count(),
            1,
            "should deduplicate"
        );
    }

    #[test]
    fn relative_imports_filtered() {
        let parser = JsParser;
        let skill = r#"---
name: test
---

## Imports

```javascript
import x from './local';
import y from '../parent';
import z from '/absolute/path';
import express from 'express';
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(deps, vec!["express"]);
    }

    #[test]
    fn hash_subpath_imports_filtered() {
        let parser = JsParser;
        let skill = r#"---
name: test
---

## Imports

```javascript
import config from '#app/config';
import logger from '#utils/logger';
import express from 'express';
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(
            deps,
            vec!["express"],
            "# subpath imports should be filtered as internal"
        );
    }

    #[test]
    fn scoped_package_extraction() {
        let parser = JsParser;
        let skill = r#"---
name: test
---

## Imports

```javascript
import styled from '@emotion/react';
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(deps.contains(&"@emotion/react".to_string()));
    }

    #[test]
    fn extract_patterns_typescript_fence() {
        let parser = JsParser;
        let skill = "---\nname: test\n---\n\n## Core Patterns\n\n### Basic\n\nA simple example.\n\n```typescript\nconsole.log('hello');\n```\n";
        let patterns = parser.extract_patterns(skill).unwrap();
        assert_eq!(patterns.len(), 1);
        assert!(patterns[0].code.contains("console.log"));
    }

    #[test]
    fn core_patterns_section_no_code_blocks_errors() {
        let parser = JsParser;
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
        let parser = JsParser;
        let skill = "---\ndescription: no name field\n---\n\n## Overview\n";
        assert_eq!(parser.extract_name(skill).unwrap(), None);
    }

    #[test]
    fn normalize_unscoped_subpath() {
        assert_eq!(JsParser::normalize_package_name("lodash/chunk"), "lodash");
        assert_eq!(JsParser::normalize_package_name("lodash/fp/map"), "lodash");
    }

    #[test]
    fn normalize_scoped_subpath() {
        assert_eq!(
            JsParser::normalize_package_name("@emotion/react/utils"),
            "@emotion/react"
        );
    }

    #[test]
    fn normalize_no_subpath() {
        assert_eq!(JsParser::normalize_package_name("express"), "express");
        assert_eq!(
            JsParser::normalize_package_name("@types/node"),
            "@types/node"
        );
    }

    #[test]
    fn categorize_pattern_configuration() {
        assert_eq!(
            JsParser::categorize_pattern("Setup and Config", "initialize the client"),
            PatternCategory::Configuration
        );
        assert_eq!(
            JsParser::categorize_pattern("Environment Variables", "setup env"),
            PatternCategory::Configuration
        );
    }

    #[test]
    fn categorize_pattern_error_handling() {
        assert_eq!(
            JsParser::categorize_pattern("Error Handling", "catch and throw"),
            PatternCategory::ErrorHandling
        );
        assert_eq!(
            JsParser::categorize_pattern("Try Catch", "handle errors with throw"),
            PatternCategory::ErrorHandling
        );
    }

    #[test]
    fn categorize_pattern_other() {
        assert_eq!(
            JsParser::categorize_pattern("Custom Middleware", "custom middleware"),
            PatternCategory::Other
        );
    }

    #[test]
    fn extract_deps_drops_invalid_names() {
        let parser = JsParser;
        // A dep with a flag-injection attempt should be dropped
        let skill = "---\nname: foo\n---\n\n## Imports\n\n```javascript\nconst x = require('--malicious');\n```\n";
        let deps = parser.extract_dependencies(skill).unwrap();
        assert!(
            deps.is_empty(),
            "should drop deps that fail sanitize_dep_name"
        );
    }

    #[test]
    fn normalize_scoped_bare() {
        // Edge case: just "@scope" with no slash
        assert_eq!(JsParser::normalize_package_name("@scope"), "@scope");
    }

    #[test]
    fn subpath_imports_collapsed_in_extract() {
        let parser = JsParser;
        let skill = r#"---
name: lodash
---

## Imports

```javascript
const _ = require('lodash');
const chunk = require('lodash/chunk');
const uniq = require('lodash/uniq');
```
"#;
        let deps = parser.extract_dependencies(skill).unwrap();
        assert_eq!(
            deps,
            vec!["lodash"],
            "subpath imports should collapse to root package"
        );
    }
}
