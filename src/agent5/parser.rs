use anyhow::Result;
use regex::Regex;
use tracing::debug;

use super::LanguageParser;

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

/// Python-specific parser for SKILL.md files
pub struct PythonParser;

impl PythonParser {
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
            || text.contains("try")
            || text.contains("catch")
            || text.contains("handle")
        {
            PatternCategory::ErrorHandling
        } else if text.contains("async") || text.contains("await") || text.contains("concurrent") {
            PatternCategory::AsyncPattern
        } else {
            PatternCategory::Other
        }
    }

    /// Check if a package name is from Python stdlib
    fn is_stdlib_package(name: &str) -> bool {
        // Common Python stdlib modules (not exhaustive, but covers most cases)
        const STDLIB_MODULES: &[&str] = &[
            "abc",
            "aifc",
            "argparse",
            "array",
            "ast",
            "asynchat",
            "asyncio",
            "asyncore",
            "atexit",
            "audioop",
            "base64",
            "bdb",
            "binascii",
            "binhex",
            "bisect",
            "builtins",
            "bz2",
            "calendar",
            "cgi",
            "cgitb",
            "chunk",
            "cmath",
            "cmd",
            "code",
            "codecs",
            "codeop",
            "collections",
            "colorsys",
            "compileall",
            "concurrent",
            "configparser",
            "contextlib",
            "contextvars",
            "copy",
            "copyreg",
            "crypt",
            "csv",
            "ctypes",
            "curses",
            "dataclasses",
            "datetime",
            "dbm",
            "decimal",
            "difflib",
            "dis",
            "distutils",
            "doctest",
            "dummy_threading",
            "email",
            "encodings",
            "enum",
            "errno",
            "faulthandler",
            "fcntl",
            "filecmp",
            "fileinput",
            "fnmatch",
            "formatter",
            "fractions",
            "ftplib",
            "functools",
            "gc",
            "getopt",
            "getpass",
            "gettext",
            "glob",
            "graphlib",
            "grp",
            "gzip",
            "hashlib",
            "heapq",
            "hmac",
            "html",
            "http",
            "imaplib",
            "imghdr",
            "imp",
            "importlib",
            "inspect",
            "io",
            "ipaddress",
            "itertools",
            "json",
            "keyword",
            "lib2to3",
            "linecache",
            "locale",
            "logging",
            "lzma",
            "mailbox",
            "mailcap",
            "marshal",
            "math",
            "mimetypes",
            "mmap",
            "modulefinder",
            "msilib",
            "msvcrt",
            "multiprocessing",
            "netrc",
            "nis",
            "nntplib",
            "numbers",
            "operator",
            "optparse",
            "os",
            "ossaudiodev",
            "parser",
            "pathlib",
            "pdb",
            "pickle",
            "pickletools",
            "pipes",
            "pkgutil",
            "platform",
            "plistlib",
            "poplib",
            "posix",
            "posixpath",
            "pprint",
            "profile",
            "pstats",
            "pty",
            "pwd",
            "py_compile",
            "pyclbr",
            "pydoc",
            "queue",
            "quopri",
            "random",
            "re",
            "readline",
            "reprlib",
            "resource",
            "rlcompleter",
            "runpy",
            "sched",
            "secrets",
            "select",
            "selectors",
            "shelve",
            "shlex",
            "shutil",
            "signal",
            "site",
            "smtpd",
            "smtplib",
            "sndhdr",
            "socket",
            "socketserver",
            "spwd",
            "sqlite3",
            "ssl",
            "stat",
            "statistics",
            "string",
            "stringprep",
            "struct",
            "subprocess",
            "sunau",
            "symbol",
            "symtable",
            "sys",
            "sysconfig",
            "syslog",
            "tabnanny",
            "tarfile",
            "telnetlib",
            "tempfile",
            "termios",
            "test",
            "textwrap",
            "threading",
            "time",
            "timeit",
            "tkinter",
            "token",
            "tokenize",
            "trace",
            "traceback",
            "tracemalloc",
            "tty",
            "turtle",
            "turtledemo",
            "types",
            "typing",
            "unicodedata",
            "unittest",
            "urllib",
            "uu",
            "uuid",
            "venv",
            "warnings",
            "wave",
            "weakref",
            "webbrowser",
            "winreg",
            "winsound",
            "wsgiref",
            "xdrlib",
            "xml",
            "xmlrpc",
            "zipapp",
            "zipfile",
            "zipimport",
            "zlib",
            "_thread",
        ];

        STDLIB_MODULES.contains(&name)
    }

    /// Check if a package name is likely a local module rather than a PyPI package
    fn is_likely_local_module(name: &str) -> bool {
        // Common patterns for local modules in test/example code
        const LOCAL_MODULE_PATTERNS: &[&str] = &[
            "cli", "main", "app", "config", "utils", "helpers", "models", "views", "routes",
            "handlers", "tests", "test", "example", "src", "lib", "core", "api", "client",
            "server",
        ];

        // Very short names (2-3 chars) are often local modules unless they're known packages
        if name.len() <= 3 && !matches!(name, "jwt" | "aws" | "grpc" | "PIL") {
            return true;
        }

        LOCAL_MODULE_PATTERNS.contains(&name)
    }
}

impl LanguageParser for PythonParser {
    fn extract_version(&self, skill_md: &str) -> Result<Option<String>> {
        // Extract version from frontmatter (line 2 typically)
        // Format: "version: 3.0.0" or "version: unknown"
        for line in skill_md.lines().take(10) {
            // Check first 10 lines for frontmatter
            let trimmed = line.trim();
            if trimmed.starts_with("version:") {
                let version = trimmed.strip_prefix("version:").unwrap().trim().to_string();

                // Return None if version is "unknown" or empty
                if version.is_empty() || version == "unknown" {
                    debug!("Version field found but set to 'unknown'");
                    return Ok(None);
                }

                debug!("Extracted version from SKILL.md: {}", version);
                return Ok(Some(version));
            }
        }

        debug!("No version field found in SKILL.md frontmatter");
        Ok(None)
    }

    fn extract_name(&self, skill_md: &str) -> Result<Option<String>> {
        for line in skill_md.lines().take(10) {
            let trimmed = line.trim();
            if trimmed.starts_with("name:") {
                let name = trimmed.strip_prefix("name:").unwrap().trim().to_string();

                if !name.is_empty() {
                    debug!("Extracted package name from SKILL.md: {}", name);
                    return Ok(Some(name));
                }
            }
        }
        Ok(None)
    }

    fn extract_patterns(&self, skill_md: &str) -> Result<Vec<CodePattern>> {
        let mut patterns = Vec::new();

        // Find the Core Patterns section
        let core_patterns_re = Regex::new(r"(?m)^##\s+Core\s+Patterns\s*$")?;
        let next_section_re = Regex::new(r"(?m)^##\s+")?;

        let start_pos = match core_patterns_re.find(skill_md) {
            Some(m) => m.end(),
            None => {
                debug!("No Core Patterns section found in SKILL.md");
                return Ok(patterns);
            }
        };

        // Find the next section (end of Core Patterns)
        let section_content = &skill_md[start_pos..];
        let end_pos = next_section_re
            .find(section_content)
            .map(|m| m.start())
            .unwrap_or(section_content.len());

        let core_patterns_content = &section_content[..end_pos];

        // Extract patterns: ### Pattern Name followed by description and ```python code block
        let pattern_re = Regex::new(r"(?m)^###\s+(.+?)$")?;
        let code_block_re = Regex::new(r"```python\n([\s\S]*?)```")?;

        let pattern_starts: Vec<(usize, String)> = pattern_re
            .captures_iter(core_patterns_content)
            .map(|cap| (cap.get(0).unwrap().start(), cap[1].to_string()))
            .collect();

        // Process each pattern
        for i in 0..pattern_starts.len() {
            let (pattern_start, pattern_name) = &pattern_starts[i];
            let pattern_end = if i + 1 < pattern_starts.len() {
                pattern_starts[i + 1].0
            } else {
                core_patterns_content.len()
            };

            let pattern_section = &core_patterns_content[*pattern_start..pattern_end];

            // Extract description (text between header and code block)
            let description_start = pattern_section.find('\n').unwrap_or(0) + 1;
            let code_block_start = code_block_re
                .find(pattern_section)
                .map(|m| m.start())
                .unwrap_or(pattern_section.len());

            let description = pattern_section[description_start..code_block_start]
                .trim()
                .to_string();

            // Extract code block
            if let Some(code_cap) = code_block_re.captures(pattern_section) {
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

        debug!("Extracted {} patterns from SKILL.md", patterns.len());
        Ok(patterns)
    }

    fn extract_dependencies(&self, skill_md: &str) -> Result<Vec<String>> {
        let mut dependencies = Vec::new();

        // Find the Imports section
        let imports_re = Regex::new(r"(?m)^##\s+Imports\s*$")?;
        let next_section_re = Regex::new(r"(?m)^##\s+")?;

        let start_pos = match imports_re.find(skill_md) {
            Some(m) => m.end(),
            None => {
                debug!("No Imports section found in SKILL.md");
                return Ok(dependencies);
            }
        };

        // Find the next section
        let section_content = &skill_md[start_pos..];
        let end_pos = next_section_re
            .find(section_content)
            .map(|m| m.start())
            .unwrap_or(section_content.len());

        let imports_content = &section_content[..end_pos];

        // Extract package names from various import patterns
        // Pattern 1: import package
        let import_re = Regex::new(r"(?m)^import\s+([a-zA-Z0-9_]+)")?;
        for cap in import_re.captures_iter(imports_content) {
            let pkg = cap[1].to_string();
            if !Self::is_stdlib_package(&pkg) && !dependencies.contains(&pkg) {
                dependencies.push(pkg);
            }
        }

        // Pattern 2: from package import ...
        let from_import_re = Regex::new(r"(?m)^from\s+([a-zA-Z0-9_]+)")?;
        for cap in from_import_re.captures_iter(imports_content) {
            let pkg = cap[1].to_string();
            if !Self::is_stdlib_package(&pkg)
                && !Self::is_likely_local_module(&pkg)
                && !dependencies.contains(&pkg)
            {
                dependencies.push(pkg);
            }
        }

        // Pattern 3: pip install package (from code blocks)
        let pip_re = Regex::new(r"pip\s+install\s+([a-zA-Z0-9_-]+)")?;
        for cap in pip_re.captures_iter(imports_content) {
            let pkg = cap[1].to_string();
            if !dependencies.contains(&pkg) {
                dependencies.push(pkg);
            }
        }

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

    #[test]
    fn test_extract_patterns() {
        let skill_md = r#"
# Test SKILL.md

## Core Patterns

### Basic Usage Example

This shows how to create a simple instance.

```python
from mylib import MyClass

obj = MyClass()
print(obj.hello())
```

### Advanced Configuration

Configure the system with options.

```python
from mylib import MyClass

obj = MyClass(debug=True, timeout=30)
obj.configure()
```

## Next Section
"#;

        let parser = PythonParser;
        let patterns = parser.extract_patterns(skill_md).unwrap();

        assert_eq!(patterns.len(), 2);

        assert_eq!(patterns[0].name, "Basic Usage Example");
        assert!(patterns[0].description.contains("simple instance"));
        assert!(patterns[0].code.contains("MyClass()"));
        assert_eq!(patterns[0].category, PatternCategory::BasicUsage);

        assert_eq!(patterns[1].name, "Advanced Configuration");
        assert!(patterns[1].code.contains("debug=True"));
        assert_eq!(patterns[1].category, PatternCategory::Configuration);
    }

    #[test]
    fn test_extract_dependencies() {
        let skill_md = r#"
# Test SKILL.md

## Imports

```python
import torch
from fastapi import FastAPI
import os
from typing import List
```

You can install with:
```bash
pip install torch fastapi
```

## Next Section
"#;

        let parser = PythonParser;
        let deps = parser.extract_dependencies(skill_md).unwrap();

        // Should extract torch and fastapi, but not os or typing (stdlib)
        assert!(deps.contains(&"torch".to_string()));
        assert!(deps.contains(&"fastapi".to_string()));
        assert!(!deps.contains(&"os".to_string()));
        assert!(!deps.contains(&"typing".to_string()));
    }

    #[test]
    fn test_categorize_pattern() {
        assert_eq!(
            PythonParser::categorize_pattern("Basic Example", "A simple intro"),
            PatternCategory::BasicUsage
        );

        assert_eq!(
            PythonParser::categorize_pattern("Configuration", "Setup the app"),
            PatternCategory::Configuration
        );

        assert_eq!(
            PythonParser::categorize_pattern("Error Handling", "Handle exceptions"),
            PatternCategory::ErrorHandling
        );

        assert_eq!(
            PythonParser::categorize_pattern("Async Tasks", "Using await"),
            PatternCategory::AsyncPattern
        );

        assert_eq!(
            PythonParser::categorize_pattern("Random Feature", "Does something"),
            PatternCategory::Other
        );
    }

    #[test]
    fn test_is_stdlib_package() {
        assert!(PythonParser::is_stdlib_package("os"));
        assert!(PythonParser::is_stdlib_package("json"));
        assert!(PythonParser::is_stdlib_package("datetime"));
        assert!(PythonParser::is_stdlib_package("typing"));

        assert!(!PythonParser::is_stdlib_package("torch"));
        assert!(!PythonParser::is_stdlib_package("fastapi"));
        assert!(!PythonParser::is_stdlib_package("numpy"));
    }

    #[test]
    fn test_extract_version() {
        let skill_md_with_version = r#"
name: pandas
version: 3.0.0
language: python

## Core Patterns
"#;

        let parser = PythonParser;
        let version = parser.extract_version(skill_md_with_version).unwrap();
        assert_eq!(version, Some("3.0.0".to_string()));
    }

    #[test]
    fn test_extract_version_unknown() {
        let skill_md_unknown = r#"
name: mylib
version: unknown

## Core Patterns
"#;

        let parser = PythonParser;
        let version = parser.extract_version(skill_md_unknown).unwrap();
        assert_eq!(version, None); // "unknown" returns None
    }

    #[test]
    fn test_extract_version_missing() {
        let skill_md_no_version = r#"
name: mylib

## Core Patterns
"#;

        let parser = PythonParser;
        let version = parser.extract_version(skill_md_no_version).unwrap();
        assert_eq!(version, None); // No version field returns None
    }
}
