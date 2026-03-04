//! Python-specific SKILL.md parser — extracts version, name, code patterns,
//! and dependencies from a generated SKILL.md file. Used by the test agent
//! to understand what to validate.

use anyhow::Result;
use regex::Regex;
use tracing::{debug, warn};

use super::parser::{extract_section, CodePattern, PatternCategory};
use super::LanguageParser;
use crate::util::sanitize_dep_name;

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

    /// Check if a package name is likely a local module rather than a PyPI package.
    /// Uses an explicit list of common local names — no length heuristic, since
    /// short names like `ray`, `gym`, `dbt`, `jax`, `six`, `bs4` are valid PyPI packages.
    fn is_likely_local_module(name: &str) -> bool {
        const LOCAL_MODULE_PATTERNS: &[&str] = &[
            "cli", "main", "app", "config", "utils", "helpers", "models", "views", "routes",
            "handlers", "tests", "test", "example", "src", "lib", "core", "api", "client",
            "server",
        ];

        LOCAL_MODULE_PATTERNS.contains(&name)
    }
}

impl LanguageParser for PythonParser {
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

        let pattern_re = Regex::new(r"(?m)^###\s+(.+?)$")?;
        let code_block_re = Regex::new(r"(?i)```(?:python|py)?\n([\s\S]*?)```")?;

        let pattern_starts: Vec<(usize, String)> = pattern_re
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
            let code_block_start = code_block_re
                .find(pattern_section)
                .map(|m| m.start())
                .unwrap_or(pattern_section.len());

            let description = pattern_section[description_start..code_block_start]
                .trim()
                .to_string();

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

        let import_re = Regex::new(r"(?m)^import\s+([a-zA-Z0-9_]+)")?;
        for cap in import_re.captures_iter(imports_content) {
            let pkg = cap[1].to_string();
            if !Self::is_stdlib_package(&pkg) && !dependencies.contains(&pkg) {
                dependencies.push(pkg);
            }
        }

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

        let pip_re = Regex::new(r"pip\s+install\s+([a-zA-Z0-9_-]+)")?;
        for cap in pip_re.captures_iter(imports_content) {
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
        let skill_md = "name: pandas\nversion: 3.0.0\nlanguage: python\n\n## Core Patterns\n";
        let parser = PythonParser;
        let version = parser.extract_version(skill_md).unwrap();
        assert_eq!(version, Some("3.0.0".to_string()));
    }

    #[test]
    fn test_extract_version_unknown() {
        let skill_md = "name: mylib\nversion: unknown\n\n## Core Patterns\n";
        let parser = PythonParser;
        let version = parser.extract_version(skill_md).unwrap();
        assert_eq!(version, None);
    }

    #[test]
    fn test_extract_version_missing() {
        let skill_md = "name: mylib\n\n## Core Patterns\n";
        let parser = PythonParser;
        let version = parser.extract_version(skill_md).unwrap();
        assert_eq!(version, None);
    }

    #[test]
    fn test_extract_patterns_case_insensitive_heading() {
        let skill_md = r#"
# Test

## core patterns

### Basic Example

Simple usage.

```python
import mylib
mylib.run()
```

## Next
"#;
        let parser = PythonParser;
        let patterns = parser.extract_patterns(skill_md).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].name, "Basic Example");
    }

    #[test]
    fn test_extract_patterns_py_fence() {
        let skill_md = r#"
# Test

## Core Patterns

### Py Fence

Example with ```py fence.

```py
import mylib
mylib.run()
```

## Next
"#;
        let parser = PythonParser;
        let patterns = parser.extract_patterns(skill_md).unwrap();
        assert_eq!(patterns.len(), 1);
        assert!(patterns[0].code.contains("mylib.run()"));
    }

    #[test]
    fn test_extract_patterns_unlabeled_fence() {
        let skill_md = r#"
# Test

## Core Patterns

### Unlabeled Fence

Example with unlabeled fence.

```
import mylib
mylib.run()
```

## Next
"#;
        let parser = PythonParser;
        let patterns = parser.extract_patterns(skill_md).unwrap();
        assert_eq!(patterns.len(), 1);
        assert!(patterns[0].code.contains("mylib.run()"));
    }

    #[test]
    fn test_extract_patterns_section_found_no_blocks_errors() {
        let skill_md = r#"
# Test

## Core Patterns

### Pattern Without Code

This pattern has no code block at all.

## Next
"#;
        let parser = PythonParser;
        let result = parser.extract_patterns(skill_md);
        assert!(
            result.is_err(),
            "section found with no code blocks should error"
        );
    }

    #[test]
    fn test_extract_patterns_no_section_returns_empty() {
        let skill_md = r#"
# Test

## Imports

```python
import mylib
```
"#;
        let parser = PythonParser;
        let patterns = parser.extract_patterns(skill_md).unwrap();
        assert!(
            patterns.is_empty(),
            "no Core Patterns section should return empty Vec"
        );
    }

    #[test]
    fn short_package_names_not_dropped() {
        // Valid 2-3 char PyPI packages must not be treated as local modules
        let skill_md = r#"
# Test

## Imports

```python
from ray import serve
from gym import spaces
from jax import numpy
from six import moves
from bs4 import BeautifulSoup
from dbt import cli
```

## Next
"#;
        let parser = PythonParser;
        let deps = parser.extract_dependencies(skill_md).unwrap();
        for pkg in &["ray", "gym", "jax", "six", "bs4", "dbt"] {
            assert!(
                deps.contains(&pkg.to_string()),
                "short package '{pkg}' should not be dropped"
            );
        }
    }

    #[test]
    fn local_module_names_still_filtered() {
        let skill_md = r#"
# Test

## Imports

```python
from cli import run
from config import settings
from utils import helpers
```

## Next
"#;
        let parser = PythonParser;
        let deps = parser.extract_dependencies(skill_md).unwrap();
        assert!(
            !deps.contains(&"cli".to_string()),
            "cli should be filtered as local"
        );
        assert!(
            !deps.contains(&"config".to_string()),
            "config should be filtered as local"
        );
        assert!(
            !deps.contains(&"utils".to_string()),
            "utils should be filtered as local"
        );
    }
}
