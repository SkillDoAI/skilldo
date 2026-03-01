use anyhow::Result;
use std::fs;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;
use tracing::{debug, info, warn};

use crate::config::detect_container_runtime;
use crate::detector::Language;
use crate::util::run_cmd_with_timeout;

/// Default timeout for validation commands (seconds)
const VALIDATION_TIMEOUT_SECS: u64 = 60;

/// Functional validator - actually runs code from SKILL.md
pub struct FunctionalValidator {
    /// Container runtime name (e.g. "docker", "podman"), or None if unavailable
    container_runtime: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// Code runs successfully
    Pass(String),
    /// Code failed to run
    Fail(String),
    /// Validation skipped (no container runtime, no python, etc.)
    Skipped(String),
}

impl FunctionalValidator {
    pub fn new() -> Self {
        // Detect runtime; check it actually responds to `ps` (daemon running)
        let detected = detect_container_runtime();
        let container_runtime = if Command::new(&detected)
            .arg("ps")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            info!("{} available - using containerized validation", detected);
            Some(detected)
        } else {
            warn!("No container runtime responding - using system Python (less safe)");
            None
        };

        Self { container_runtime }
    }

    /// Validate a generated SKILL.md by running extracted code
    pub fn validate(&self, skill_md: &str, language: &Language) -> Result<ValidationResult> {
        match language {
            Language::Python => self.validate_python(skill_md),
            Language::JavaScript => Ok(ValidationResult::Skipped(
                "JavaScript validation not yet implemented".to_string(),
            )),
            _ => Ok(ValidationResult::Skipped(format!(
                "Validation not supported for {}",
                language.as_str()
            ))),
        }
    }

    /// Validate Python SKILL.md
    fn validate_python(&self, skill_md: &str) -> Result<ValidationResult> {
        // Extract a simple test case from SKILL.md
        let test_code = self.extract_python_hello_world(skill_md)?;

        if test_code.is_empty() {
            return Ok(ValidationResult::Skipped(
                "No runnable code found in SKILL.md".to_string(),
            ));
        }

        info!("Extracted test code:\n{}", test_code);

        // Run the test
        if let Some(ref runtime) = self.container_runtime {
            self.run_python_container(runtime, &test_code)
        } else {
            self.run_python_system(&test_code)
        }
    }

    /// Extract a simple "hello world" test from Python SKILL.md
    fn extract_python_hello_world(&self, skill_md: &str) -> Result<String> {
        let mut in_code_block = false;
        let mut code_lines = Vec::new();
        let mut found_import = false;

        // Look for first complete code example (anywhere in the document)
        for line in skill_md.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```python") || trimmed.starts_with("```py") {
                in_code_block = true;
                continue;
            }

            if line.trim() == "```" && in_code_block {
                // End of code block - if we have some executable code, use it
                if !code_lines.is_empty() {
                    // Prefer blocks with imports, but accept any executable code
                    if found_import || code_lines.len() >= 2 {
                        break;
                    }
                }
                in_code_block = false;
                code_lines.clear();
                found_import = false;
                continue;
            }

            if in_code_block {
                let trimmed = line.trim();

                // Track if we found an import
                if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                    found_import = true;
                }

                // Skip comments and empty lines
                if !trimmed.starts_with('#') && !trimmed.is_empty() {
                    code_lines.push(line.to_string());
                }
            }
        }

        if code_lines.is_empty() {
            return Ok(String::new()); // Return empty string, caller will handle as Skipped
        }

        // Add a simple assertion at the end if none exists
        let code = code_lines.join("\n");
        if !code.contains("assert") && !code.contains("print") {
            // Add a print to verify it at least runs
            Ok(format!("{}\nprint('✓ Code executed successfully')", code))
        } else {
            Ok(code)
        }
    }

    /// Run Python code in container (docker or podman)
    fn run_python_container(&self, runtime: &str, code: &str) -> Result<ValidationResult> {
        let temp = TempDir::new()?;
        let script_path = temp.path().join("test.py");
        fs::write(&script_path, code)?;

        debug!("Running Python in {}: python:3.11-alpine", runtime);

        let mut cmd = Command::new(runtime);
        cmd.arg("run")
            .arg("--rm")
            .arg("-v")
            .arg(format!("{}:/workspace", temp.path().display()))
            .arg("python:3.11-alpine")
            .arg("python")
            .arg("/workspace/test.py");

        let output = run_cmd_with_timeout(cmd, Duration::from_secs(VALIDATION_TIMEOUT_SECS))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            info!("✓ Validation passed");
            Ok(ValidationResult::Pass(stdout.to_string()))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("✗ Validation failed: {}", stderr);
            Ok(ValidationResult::Fail(stderr.to_string()))
        }
    }

    /// Run Python code using system Python (fallback, less safe)
    fn run_python_system(&self, code: &str) -> Result<ValidationResult> {
        // Check if python3 is available
        if !Command::new("python3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok(ValidationResult::Skipped(
                "Python3 not available on system".to_string(),
            ));
        }

        let temp = TempDir::new()?;
        let script_path = temp.path().join("test.py");
        fs::write(&script_path, code)?;

        debug!("Running Python with system python3");

        let cmd = Command::new("python3");
        let mut cmd = cmd;
        cmd.arg(&script_path);

        let output = run_cmd_with_timeout(cmd, Duration::from_secs(VALIDATION_TIMEOUT_SECS))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            info!("✓ Validation passed (system python)");
            Ok(ValidationResult::Pass(stdout.to_string()))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("✗ Validation failed: {}", stderr);
            Ok(ValidationResult::Fail(stderr.to_string()))
        }
    }
}

impl Default for FunctionalValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::Language;

    #[test]
    fn test_extract_hello_world() {
        let skill_md = r#"
## Core Patterns

### Basic Usage
```python
import math
x = 5
result = math.sqrt(x)
print(result)
```
"#;

        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();

        assert!(code.contains("import math"));
        assert!(code.contains("math.sqrt"));
    }

    #[test]
    fn test_validate_simple_python() {
        let skill_md = r#"
```python
x = 1 + 1
assert x == 2
print("✓ Test passed")
```
"#;

        let validator = FunctionalValidator::new();
        let result = validator.validate(skill_md, &Language::Python).unwrap();

        match result {
            ValidationResult::Pass(_) => { /* expected */ }
            ValidationResult::Skipped(msg) => {
                println!("Skipped: {}", msg);
            }
            ValidationResult::Fail(err) => {
                panic!("Should not fail: {}", err);
            }
        }
    }

    #[test]
    fn test_validate_invalid_python() {
        let skill_md = r#"
```python
import nonexistent_module
x = undefined_function()
```
"#;

        let validator = FunctionalValidator::new();
        let result = validator.validate(skill_md, &Language::Python).unwrap();

        match result {
            ValidationResult::Fail(_) => { /* expected */ }
            ValidationResult::Skipped(msg) => {
                println!("Skipped: {}", msg);
            }
            ValidationResult::Pass(_) => {
                panic!("Should not pass with invalid code");
            }
        }
    }

    #[test]
    fn test_validate_javascript_skipped() {
        let validator = FunctionalValidator::new();
        let result = validator
            .validate("# SKILL.md", &Language::JavaScript)
            .unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("JavaScript validation not yet implemented".to_string())
        );
    }

    #[test]
    fn test_validate_typescript_skipped() {
        let validator = FunctionalValidator::new();
        let result = validator
            .validate("# SKILL.md", &Language::JavaScript)
            .unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("JavaScript validation not yet implemented".to_string())
        );
    }

    #[test]
    fn test_validate_unknown_ecosystem_skipped() {
        let validator = FunctionalValidator::new();
        let result = validator.validate("# SKILL.md", &Language::Rust).unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("Validation not supported for rust".to_string())
        );
    }

    #[test]
    fn test_extract_no_code_blocks() {
        let skill_md = r#"
# SKILL.md
Some documentation without any python code blocks.
"#;

        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        assert!(code.is_empty());

        // Also confirm validate returns Skipped for this case
        let result = validator.validate(skill_md, &Language::Python).unwrap();
        match result {
            ValidationResult::Skipped(msg) => {
                assert!(msg.contains("No runnable code"));
            }
            _ => panic!("Expected Skipped for SKILL.md with no code blocks"),
        }
    }

    #[test]
    fn test_extract_code_adds_print_when_no_assertion() {
        let skill_md = r#"
```python
import os
x = os.getcwd()
```
"#;

        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        assert!(code.contains("import os"));
        assert!(code.contains("print("));
        assert!(code.contains("Code executed successfully"));
    }

    #[test]
    fn test_validation_result_equality() {
        assert_eq!(
            ValidationResult::Pass("x".into()),
            ValidationResult::Pass("x".into())
        );
        assert_ne!(
            ValidationResult::Pass("x".into()),
            ValidationResult::Pass("y".into())
        );
        assert_ne!(
            ValidationResult::Pass("x".into()),
            ValidationResult::Fail("x".into())
        );
        assert_eq!(
            ValidationResult::Skipped("s".into()),
            ValidationResult::Skipped("s".into())
        );
    }

    #[test]
    fn test_extract_py_fence_variant() {
        let skill_md = "```py\nimport os\nx = os.getcwd()\n```\n";
        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        assert!(code.contains("import os"), "Should parse ```py fences");
    }

    #[test]
    fn test_extract_two_line_block_no_import_is_accepted() {
        // Two lines, no import → accepted because code_lines.len() >= 2
        let skill_md = "```python\nx = 1\ny = 2\n```\n";
        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("x = 1"),
            "Two-line block without import should be accepted"
        );
    }

    #[test]
    fn test_extract_block_of_only_comments_is_discarded() {
        // Block containing only comments → code_lines stays empty
        let skill_md = "```python\n# just a comment\n# another comment\n```\n";
        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        assert!(code.is_empty(), "Block with only comments should be empty");
    }

    #[test]
    fn test_extract_comments_and_blank_lines_stripped() {
        let skill_md = "```python\nimport os\n# comment\n\nx = os.getcwd()\n```\n";
        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        assert!(!code.contains("# comment"), "Comments should be stripped");
        assert!(code.contains("import os"));
        assert!(code.contains("os.getcwd()"));
    }

    #[test]
    fn test_extract_from_import_sets_found_import() {
        let skill_md = "```python\nfrom os import getcwd\nx = getcwd()\n```\n";
        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("from os import getcwd"),
            "'from' imports should be recognized"
        );
    }

    #[test]
    fn test_extract_code_with_assert_does_not_append_print() {
        let skill_md = "```python\nimport os\nassert os.path.exists('.')\n```\n";
        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        assert!(
            !code.contains("Code executed successfully"),
            "Should not append print when assert exists"
        );
    }

    #[test]
    fn test_extract_code_with_print_does_not_append_extra_print() {
        let skill_md = "```python\nimport os\nprint(os.getcwd())\n```\n";
        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        // Count occurrences of "print(" - should be exactly 1 (the original)
        let count = code.matches("print(").count();
        assert_eq!(count, 1, "Should not add extra print when one exists");
    }

    #[test]
    fn test_extract_unclosed_code_block() {
        // Code block never closed → code_lines accumulate through EOF
        // After the loop, code_lines is non-empty so the code is returned
        let skill_md = "```python\nimport os\nx = os.getcwd()\n";
        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("import os"),
            "Unclosed code block should still return accumulated code"
        );
    }

    #[test]
    fn test_functional_validator_default_equals_new() {
        let v1 = FunctionalValidator::new();
        let v2 = FunctionalValidator::default();
        assert_eq!(v1.container_runtime, v2.container_runtime);
    }

    #[test]
    fn test_validate_empty_string_returns_skipped() {
        let validator = FunctionalValidator::new();
        let result = validator.validate("", &Language::Python).unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("No runnable code found in SKILL.md".to_string())
        );
    }

    fn require_container_runtime() -> Option<String> {
        let rt = detect_container_runtime();
        // Verify the runtime daemon is actually responding
        if Command::new(&rt)
            .arg("ps")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(rt)
        } else {
            None
        }
    }

    #[test]
    #[ignore] // Requires container runtime (docker or podman)
    fn test_run_python_container_pass() {
        let runtime = match require_container_runtime() {
            Some(rt) => rt,
            None => {
                eprintln!("Skipping: no container runtime available");
                return;
            }
        };
        let validator = FunctionalValidator {
            container_runtime: Some(runtime.clone()),
        };
        let result = validator
            .run_python_container(&runtime, "print('hello from container')")
            .unwrap();
        match result {
            ValidationResult::Pass(out) => assert!(out.contains("hello from container")),
            other => panic!("Expected Pass, got {:?}", other),
        }
    }

    #[test]
    #[ignore] // Requires container runtime (docker or podman)
    fn test_run_python_container_fail() {
        let runtime = match require_container_runtime() {
            Some(rt) => rt,
            None => {
                eprintln!("Skipping: no container runtime available");
                return;
            }
        };
        let validator = FunctionalValidator {
            container_runtime: Some(runtime.clone()),
        };
        let result = validator
            .run_python_container(&runtime, "raise ValueError('boom')")
            .unwrap();
        match result {
            ValidationResult::Fail(err) => assert!(err.contains("ValueError")),
            other => panic!("Expected Fail, got {:?}", other),
        }
    }

    #[test]
    #[ignore] // Requires container runtime (docker or podman)
    fn test_run_python_container_syntax_error() {
        let runtime = match require_container_runtime() {
            Some(rt) => rt,
            None => {
                eprintln!("Skipping: no container runtime available");
                return;
            }
        };
        let validator = FunctionalValidator {
            container_runtime: Some(runtime.clone()),
        };
        let result = validator
            .run_python_container(&runtime, "def foo(:\n  pass")
            .unwrap();
        match result {
            ValidationResult::Fail(err) => assert!(err.contains("SyntaxError")),
            other => panic!("Expected Fail, got {:?}", other),
        }
    }

    #[test]
    #[ignore] // Requires python3 on system
    fn test_run_python_system_pass() {
        let validator = FunctionalValidator {
            container_runtime: None,
        };
        let result = validator
            .run_python_system("print('hello from system')")
            .unwrap();
        match result {
            ValidationResult::Pass(out) => assert!(out.contains("hello from system")),
            ValidationResult::Skipped(_) => { /* python3 not available, ok */ }
            other => panic!("Expected Pass or Skipped, got {:?}", other),
        }
    }

    #[test]
    #[ignore] // Requires python3 on system
    fn test_run_python_system_fail() {
        let validator = FunctionalValidator {
            container_runtime: None,
        };
        let result = validator
            .run_python_system("raise RuntimeError('oops')")
            .unwrap();
        match result {
            ValidationResult::Fail(err) => assert!(err.contains("RuntimeError")),
            ValidationResult::Skipped(_) => { /* python3 not available, ok */ }
            other => panic!("Expected Fail or Skipped, got {:?}", other),
        }
    }

    #[test]
    #[ignore] // Requires container runtime (docker or podman)
    fn test_validate_python_full_container_path() {
        let runtime = match require_container_runtime() {
            Some(rt) => rt,
            None => {
                eprintln!("Skipping: no container runtime available");
                return;
            }
        };
        let validator = FunctionalValidator {
            container_runtime: Some(runtime),
        };
        let skill_md = "```python\nimport os\nprint(os.getcwd())\n```\n";
        let result = validator.validate(skill_md, &Language::Python).unwrap();
        match result {
            ValidationResult::Pass(out) => assert!(!out.is_empty()),
            other => panic!("Expected Pass, got {:?}", other),
        }
    }

    #[test]
    #[ignore] // Requires python3 on system
    fn test_validate_python_full_system_path() {
        let validator = FunctionalValidator {
            container_runtime: None,
        };
        let skill_md = "```python\nimport os\nprint(os.getcwd())\n```\n";
        let result = validator.validate(skill_md, &Language::Python).unwrap();
        match result {
            ValidationResult::Pass(out) => assert!(!out.is_empty()),
            ValidationResult::Skipped(_) => { /* python3 not available, ok */ }
            other => panic!("Expected Pass or Skipped, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_whitespace_only_returns_skipped() {
        let validator = FunctionalValidator::new();
        let result = validator
            .validate("   \n\n  \n", &Language::Python)
            .unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("No runnable code found in SKILL.md".to_string())
        );
    }

    #[test]
    fn test_extract_multiple_code_blocks_prefers_imports() {
        let skill_md = r#"
# SKILL.md

## Example 1
```python
x = 1
```

## Example 2
```python
import os
os.path.exists('.')
```
"#;

        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();

        // The first block (x = 1) is only 1 line with no import, so it gets skipped.
        // The second block has an import, so it should be selected.
        assert!(code.contains("import os"));
    }

    #[test]
    fn test_extract_single_line_block_too_short() {
        let skill_md = r#"
```python
x = 1
```
"#;

        let validator = FunctionalValidator::new();
        let code = validator.extract_python_hello_world(skill_md).unwrap();

        // Single line, no import => skipped (needs import OR >= 2 lines)
        assert!(code.is_empty());
    }

    #[test]
    fn test_validate_no_code_returns_skipped() {
        let skill_md = r#"
# SKILL.md

This document has only text. No python code blocks at all.

Some more prose here.
"#;

        let validator = FunctionalValidator::new();
        let result = validator.validate(skill_md, &Language::Python).unwrap();

        assert_eq!(
            result,
            ValidationResult::Skipped("No runnable code found in SKILL.md".to_string())
        );
    }

    #[test]
    fn test_validation_result_variants() {
        // Verify all three variants can be created and matched
        let pass = ValidationResult::Pass("output".to_string());
        let fail = ValidationResult::Fail("error".to_string());
        let skipped = ValidationResult::Skipped("reason".to_string());

        match &pass {
            ValidationResult::Pass(msg) => assert_eq!(msg, "output"),
            _ => panic!("Expected Pass variant"),
        }

        match &fail {
            ValidationResult::Fail(msg) => assert_eq!(msg, "error"),
            _ => panic!("Expected Fail variant"),
        }

        match &skipped {
            ValidationResult::Skipped(msg) => assert_eq!(msg, "reason"),
            _ => panic!("Expected Skipped variant"),
        }
    }

    // --- New tests for coverage improvement ---

    /// Construct with explicit container_runtime = Some("podman")
    #[test]
    fn test_construct_with_podman_runtime() {
        let v = FunctionalValidator {
            container_runtime: Some("podman".to_string()),
        };
        assert_eq!(v.container_runtime, Some("podman".to_string()));
    }

    /// Construct with explicit container_runtime = Some("docker")
    #[test]
    fn test_construct_with_docker_runtime() {
        let v = FunctionalValidator {
            container_runtime: Some("docker".to_string()),
        };
        assert_eq!(v.container_runtime, Some("docker".to_string()));
    }

    /// Construct with container_runtime = None
    #[test]
    fn test_construct_with_no_runtime() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        assert!(v.container_runtime.is_none());
    }

    /// validate_python returns Skipped when container_runtime is None and no runnable blocks
    #[test]
    fn test_validate_python_no_runtime_no_code_returns_skipped() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v
            .validate("# Just prose, no code blocks", &Language::Python)
            .unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("No runnable code found in SKILL.md".to_string())
        );
    }

    /// validate_python returns Skipped when container_runtime is Some but no runnable blocks
    #[test]
    fn test_validate_python_with_runtime_no_code_returns_skipped() {
        let v = FunctionalValidator {
            container_runtime: Some("podman".to_string()),
        };
        let result = v
            .validate("# Just prose, no code blocks", &Language::Python)
            .unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("No runnable code found in SKILL.md".to_string())
        );
    }

    /// validate() with ecosystem = "go" returns unsupported
    #[test]
    fn test_validate_go_ecosystem_skipped() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.validate("# SKILL.md", &Language::Go).unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("Validation not supported for go".to_string())
        );
    }

    /// validate() with Rust ecosystem returns unsupported
    #[test]
    fn test_validate_rust_ecosystem_skipped() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.validate("# SKILL.md", &Language::Rust).unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("Validation not supported for rust".to_string())
        );
    }

    /// validate() with Go ecosystem returns unsupported
    #[test]
    fn test_validate_go_ecosystem_skipped_with_none_runtime() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.validate("# SKILL.md", &Language::Go).unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("Validation not supported for go".to_string())
        );
    }

    /// validate() javascript with explicit None runtime
    #[test]
    fn test_validate_javascript_no_runtime_skipped() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.validate("# SKILL.md", &Language::JavaScript).unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("JavaScript validation not yet implemented".to_string())
        );
    }

    /// validate() typescript with explicit Some runtime
    #[test]
    fn test_validate_typescript_with_runtime_skipped() {
        let v = FunctionalValidator {
            container_runtime: Some("docker".to_string()),
        };
        let result = v.validate("# SKILL.md", &Language::JavaScript).unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("JavaScript validation not yet implemented".to_string())
        );
    }

    /// extract_python_hello_world: multiple blocks where first has import and 1 line
    /// (should take the first block with import even if it's only 1 code line)
    #[test]
    fn test_extract_first_block_with_import_wins() {
        let skill_md = r#"
```python
import json
```

```python
x = 1
y = 2
```
"#;
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        // First block has import (found_import=true) and 1 code line.
        // The break condition is: found_import || code_lines.len() >= 2
        // found_import is true, so it breaks at the closing fence of first block.
        assert!(
            code.contains("import json"),
            "First block with import should be selected"
        );
        assert!(
            !code.contains("x = 1"),
            "Second block should not be included"
        );
    }

    /// extract_python_hello_world: block with only empty lines should be discarded
    #[test]
    fn test_extract_block_with_only_empty_lines() {
        let skill_md = "```python\n\n\n\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.is_empty(),
            "Block with only empty lines should produce no code"
        );
    }

    /// extract_python_hello_world: mixed ```python and ```py fences
    #[test]
    fn test_extract_mixed_python_and_py_fences() {
        let skill_md = r#"
```python
# only a comment
```

```py
from pathlib import Path
p = Path('.')
```
"#;
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("from pathlib import Path"),
            "Should pick the ```py block with real code"
        );
    }

    /// extract_python_hello_world: first block has import but comment-only lines after it
    /// (import line is real code, so code_lines has 1 entry, found_import=true => break)
    #[test]
    fn test_extract_import_only_block_accepted() {
        let skill_md = "```python\nimport sys\n# just a comment\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("import sys"),
            "Block with import and comments should be accepted"
        );
    }

    /// extract_python_hello_world: non-python code blocks should be ignored
    #[test]
    fn test_extract_ignores_non_python_fences() {
        let skill_md = r#"
```bash
echo "hello"
```

```javascript
console.log("hi");
```

```python
import os
x = os.getpid()
```
"#;
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(!code.contains("echo"), "Bash block should be ignored");
        assert!(!code.contains("console"), "JS block should be ignored");
        assert!(
            code.contains("import os"),
            "Python block should be extracted"
        );
    }

    /// Debug trait on ValidationResult produces expected output
    #[test]
    fn test_validation_result_debug_pass() {
        let r = ValidationResult::Pass("ok".to_string());
        let dbg = format!("{:?}", r);
        assert!(dbg.contains("Pass"));
        assert!(dbg.contains("ok"));
    }

    #[test]
    fn test_validation_result_debug_fail() {
        let r = ValidationResult::Fail("err".to_string());
        let dbg = format!("{:?}", r);
        assert!(dbg.contains("Fail"));
        assert!(dbg.contains("err"));
    }

    #[test]
    fn test_validation_result_debug_skipped() {
        let r = ValidationResult::Skipped("reason".to_string());
        let dbg = format!("{:?}", r);
        assert!(dbg.contains("Skipped"));
        assert!(dbg.contains("reason"));
    }

    /// Clone trait on ValidationResult
    #[test]
    fn test_validation_result_clone() {
        let original = ValidationResult::Pass("data".to_string());
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    /// validate_python dispatches correctly when runtime is None and code is extractable
    /// but python3 may or may not be available -- we just ensure we don't get an error
    #[test]
    fn test_validate_python_no_runtime_with_code_dispatches() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let skill_md = "```python\nimport os\nprint(os.getcwd())\n```\n";
        let result = v.validate(skill_md, &Language::Python).unwrap();
        // Without container runtime, it falls through to run_python_system.
        // On systems without python3, this returns Skipped.
        // On systems with python3, this returns Pass.
        // Either way it should not be Fail for valid code.
        match result {
            ValidationResult::Pass(_) | ValidationResult::Skipped(_) => { /* expected */ }
            ValidationResult::Fail(err) => {
                panic!(
                    "Valid code should not fail (might skip if no python3): {}",
                    err
                );
            }
        }
    }

    /// extract: a block ending at EOF (no closing fence) with import should still be returned
    #[test]
    fn test_extract_unclosed_block_with_import() {
        let skill_md = "```python\nfrom os import path\npath.exists('.')\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("from os import path"),
            "Unclosed block with import should still return code"
        );
    }

    /// extract: first block is 1-liner without import (rejected),
    /// second block is 1-liner without import (rejected),
    /// no acceptable block found => empty
    #[test]
    fn test_extract_multiple_single_line_blocks_all_rejected() {
        let skill_md = "```python\nx = 1\n```\n\n```python\ny = 2\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.is_empty(),
            "Multiple single-line blocks without imports should all be rejected"
        );
    }

    /// extract: first block 1-liner rejected, second block 2-liner accepted
    #[test]
    fn test_extract_skips_short_block_picks_longer() {
        let skill_md = r#"
```python
x = 1
```

```python
a = 10
b = a + 5
```
"#;
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(code.contains("a = 10"), "Should pick the two-line block");
        assert!(
            code.contains("b = a + 5"),
            "Should include second line of two-line block"
        );
        assert!(
            !code.contains("x = 1"),
            "Single-line block should have been cleared"
        );
    }

    /// extract: code block with trailing whitespace lines only (no real code)
    #[test]
    fn test_extract_block_with_whitespace_lines_only() {
        let skill_md = "```python\n   \n  \n    \n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.is_empty(),
            "Block with only whitespace lines should be treated as empty"
        );
    }

    /// extract: fence with extra text after ```python (e.g. ```python3) should still match
    #[test]
    fn test_extract_fence_with_extra_text_after_python() {
        // "```python3" starts_with "```python" so it should match
        let skill_md = "```python3\nimport sys\nprint(sys.version)\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("import sys"),
            "```python3 should be recognized as a python fence"
        );
    }

    /// extract: the appended print message contains the checkmark
    #[test]
    fn test_extract_appended_print_contains_checkmark() {
        let skill_md = "```python\nimport os\nx = os.getcwd()\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("\u{2713}"),
            "Appended print should contain checkmark character"
        );
    }

    /// validate_python with empty code blocks and explicit runtime returns Skipped
    #[test]
    fn test_validate_python_empty_blocks_with_runtime_returns_skipped() {
        let v = FunctionalValidator {
            container_runtime: Some("podman".to_string()),
        };
        let skill_md = "```python\n# only comments\n```\n";
        let result = v.validate(skill_md, &Language::Python).unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("No runnable code found in SKILL.md".to_string())
        );
    }

    // ---- Tests exercising run_python_container / run_python_system ----

    /// run_python_container with a nonexistent runtime errors at spawn
    /// (exercises TempDir creation, file write, Command setup inside the function)
    #[test]
    fn test_run_python_container_nonexistent_runtime_returns_error() {
        let v = FunctionalValidator {
            container_runtime: Some("nonexistent_runtime_xyz".to_string()),
        };
        let err = v
            .run_python_container("nonexistent_runtime_xyz", "print('hi')")
            .unwrap_err();
        // The error comes from run_cmd_with_timeout failing to spawn
        let msg = format!("{}", err);
        assert!(
            msg.contains("spawn") || msg.contains("No such file") || msg.contains("not found"),
            "Expected spawn/not-found error, got: {}",
            msg
        );
    }

    /// validate_python dispatches to run_python_container when runtime is set,
    /// and propagates the spawn error
    #[test]
    fn test_validate_python_with_nonexistent_runtime_propagates_error() {
        let v = FunctionalValidator {
            container_runtime: Some("nonexistent_runtime_xyz".to_string()),
        };
        let skill_md = "```python\nimport os\nprint(os.getcwd())\n```\n";
        let result = v.validate(skill_md, &Language::Python);
        // Should be Err because the container runtime doesn't exist
        assert!(
            result.is_err(),
            "Expected error from nonexistent container runtime"
        );
    }

    /// run_python_system with valid code returns Pass (python3 is available on this system)
    #[test]
    fn test_run_python_system_pass_valid_code() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.run_python_system("print('system test ok')").unwrap();
        match result {
            ValidationResult::Pass(out) => {
                assert!(
                    out.contains("system test ok"),
                    "Expected stdout to contain our message, got: {}",
                    out
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available, acceptable */ }
            ValidationResult::Fail(err) => {
                panic!("Valid print() code should not fail: {}", err);
            }
        }
    }

    /// run_python_system with invalid code returns Fail
    #[test]
    fn test_run_python_system_fail_invalid_code() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v
            .run_python_system("raise RuntimeError('deliberate failure')")
            .unwrap();
        match result {
            ValidationResult::Fail(err) => {
                assert!(
                    err.contains("RuntimeError"),
                    "Expected RuntimeError in stderr, got: {}",
                    err
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available, acceptable */ }
            ValidationResult::Pass(_) => {
                panic!("Code that raises should not Pass");
            }
        }
    }

    /// run_python_system with syntax error returns Fail
    #[test]
    fn test_run_python_system_syntax_error() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.run_python_system("def foo(:\n  pass").unwrap();
        match result {
            ValidationResult::Fail(err) => {
                assert!(
                    err.contains("SyntaxError"),
                    "Expected SyntaxError in stderr, got: {}",
                    err
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available, acceptable */ }
            ValidationResult::Pass(_) => {
                panic!("Syntax error code should not Pass");
            }
        }
    }

    /// validate_python with no runtime dispatches to run_python_system for valid code
    #[test]
    fn test_validate_python_system_path_valid_code() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let skill_md = "```python\nx = 42\nprint(f'answer={x}')\n```\n";
        let result = v.validate(skill_md, &Language::Python).unwrap();
        match result {
            ValidationResult::Pass(out) => {
                assert!(
                    out.contains("answer=42"),
                    "Expected answer=42 in output, got: {}",
                    out
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available, acceptable */ }
            ValidationResult::Fail(err) => {
                panic!("Valid print code should not fail: {}", err);
            }
        }
    }

    /// validate_python with no runtime dispatches to run_python_system for invalid code
    #[test]
    fn test_validate_python_system_path_invalid_code() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let skill_md = "```python\nimport nonexistent_module_xyz_abc\nnonexistent_module_xyz_abc.do_stuff()\n```\n";
        let result = v.validate(skill_md, &Language::Python).unwrap();
        match result {
            ValidationResult::Fail(err) => {
                assert!(
                    err.contains("ModuleNotFoundError") || err.contains("No module named"),
                    "Expected import error, got: {}",
                    err
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available, acceptable */ }
            ValidationResult::Pass(_) => {
                panic!("Importing nonexistent module should not Pass");
            }
        }
    }

    /// run_python_container writes the script file correctly before failing on runtime
    #[test]
    fn test_run_python_container_writes_temp_script() {
        // We can verify the function creates the temp dir and writes the file
        // by checking that it gets past file creation to the Command::new step.
        // With a nonexistent runtime, the error message will be about the command,
        // not about file I/O.
        let v = FunctionalValidator {
            container_runtime: Some("nonexistent_runtime_xyz".to_string()),
        };
        let code = "import os\nprint(os.getcwd())";
        let err = v
            .run_python_container("nonexistent_runtime_xyz", code)
            .unwrap_err();
        // If file writing failed, we'd see a different error. The spawn error
        // confirms file ops succeeded.
        let msg = format!("{:#}", err);
        assert!(
            !msg.contains("Permission denied") && !msg.contains("No space left"),
            "File I/O should succeed; error should be about command spawn: {}",
            msg
        );
    }

    /// run_python_system with code that produces both stdout and stderr
    #[test]
    fn test_run_python_system_with_assertion_error() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v
            .run_python_system("assert False, 'test assertion'")
            .unwrap();
        match result {
            ValidationResult::Fail(err) => {
                assert!(
                    err.contains("AssertionError"),
                    "Expected AssertionError, got: {}",
                    err
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available, acceptable */ }
            ValidationResult::Pass(_) => {
                panic!("assert False should not Pass");
            }
        }
    }

    /// validate_python end-to-end: code with assert that passes
    #[test]
    fn test_validate_python_system_path_passing_assertion() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let skill_md =
            "```python\nimport os\nassert os.path.exists('.')\nprint('assertion passed')\n```\n";
        let result = v.validate(skill_md, &Language::Python).unwrap();
        match result {
            ValidationResult::Pass(out) => {
                assert!(
                    out.contains("assertion passed"),
                    "Expected 'assertion passed' in output, got: {}",
                    out
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available, acceptable */ }
            ValidationResult::Fail(err) => {
                panic!("os.path.exists('.') should not fail: {}", err);
            }
        }
    }

    /// run_python_system with empty code still runs (produces no output)
    #[test]
    fn test_run_python_system_empty_code() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.run_python_system("").unwrap();
        match result {
            ValidationResult::Pass(out) => {
                assert!(
                    out.is_empty() || out.trim().is_empty(),
                    "Empty code should produce no output"
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available, acceptable */ }
            _ => panic!("Empty code should Pass (or Skip if no python3)"),
        }
    }

    /// run_python_system with multiline code that succeeds
    #[test]
    fn test_run_python_system_multiline_code() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = "x = [1, 2, 3]\ny = sum(x)\nprint(f'sum={y}')";
        let result = v.run_python_system(code).unwrap();
        match result {
            ValidationResult::Pass(out) => {
                assert!(out.contains("sum=6"), "Expected sum=6, got: {}", out);
            }
            ValidationResult::Skipped(_) => { /* python3 not available, acceptable */ }
            ValidationResult::Fail(err) => {
                panic!("Valid multiline code should not fail: {}", err);
            }
        }
    }

    // --- Additional coverage tests ---

    /// extract: block with mixed comments and empty lines only => empty
    #[test]
    fn test_extract_block_comments_and_empty_lines_mixed() {
        let skill_md = "```python\n# comment\n\n# another\n\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.is_empty(),
            "Block with only comments and empty lines should be empty"
        );
    }

    /// extract: first block 1-liner no import (rejected), second block has import via ```py fence
    #[test]
    fn test_extract_first_short_second_py_fence_with_import() {
        let skill_md = "```python\nx = 1\n```\n\n```py\nimport json\njson.dumps({})\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            !code.contains("x = 1"),
            "First short block should be cleared"
        );
        assert!(
            code.contains("import json"),
            "Second ```py block with import should be selected"
        );
    }

    /// extract: closing ``` inside a non-python block should not affect state
    /// (non-python fences are not entered, so ``` won't toggle in_code_block)
    #[test]
    fn test_extract_closing_fence_outside_python_block_ignored() {
        let skill_md = r#"
```bash
echo "hello"
```

```python
import sys
print(sys.version)
```
"#;
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(code.contains("import sys"));
        assert!(!code.contains("echo"));
    }

    /// extract: code block with assert keyword embedded in string should still count
    #[test]
    fn test_extract_assert_in_string_counts_as_assert() {
        let skill_md = "```python\nimport os\nx = 'assert this'\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        // "assert" appears in the string literal, so contains("assert") is true
        // => no print appended
        assert!(
            !code.contains("Code executed successfully"),
            "Should not append print when 'assert' appears anywhere in code"
        );
    }

    /// extract: code block with print embedded in variable name should count
    #[test]
    fn test_extract_print_in_variable_name_counts() {
        let skill_md = "```python\nimport os\nprinter = os.getcwd()\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        // "print" is a substring of "printer", so contains("print") is true
        // => no extra print appended (this matches the actual behavior)
        assert!(
            !code.contains("Code executed successfully"),
            "Should not append print when 'print' appears as substring"
        );
    }

    /// validate dispatch: "rust" ecosystem returns not-supported Skipped
    #[test]
    fn test_validate_rust_ecosystem_returns_not_supported() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.validate("# SKILL.md", &Language::Rust).unwrap();
        match &result {
            ValidationResult::Skipped(msg) => {
                assert!(
                    msg.contains("not supported"),
                    "Expected 'not supported' message, got: {}",
                    msg
                );
                assert!(msg.contains("rust"));
            }
            _ => panic!("Expected Skipped for rust, got {:?}", result),
        }
    }

    /// validate dispatch: "go" ecosystem returns not-supported Skipped
    #[test]
    fn test_validate_go_ecosystem_returns_not_supported_message() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.validate("# SKILL.md", &Language::Go).unwrap();
        match &result {
            ValidationResult::Skipped(msg) => {
                assert!(
                    msg.contains("not supported"),
                    "Expected 'not supported' message, got: {}",
                    msg
                );
                assert!(msg.contains("go"));
            }
            _ => panic!("Expected Skipped for go, got {:?}", result),
        }
    }

    /// ValidationResult: PartialEq across different variants
    #[test]
    fn test_validation_result_ne_across_variants() {
        let pass = ValidationResult::Pass("x".into());
        let fail = ValidationResult::Fail("x".into());
        let skipped = ValidationResult::Skipped("x".into());
        assert_ne!(pass, fail);
        assert_ne!(pass, skipped);
        assert_ne!(fail, skipped);
    }

    /// ValidationResult: Clone produces independent copy
    #[test]
    fn test_validation_result_clone_independence() {
        let original = ValidationResult::Fail("error".to_string());
        let cloned = original.clone();
        assert_eq!(original, cloned);
        // Both are independent; modifying one shouldn't affect the other
        // (String is owned, so this is guaranteed by Rust's type system)
        let _moved = original; // original still usable after clone
        assert_eq!(cloned, ValidationResult::Fail("error".to_string()));
    }

    /// ValidationResult: Debug output for all three variants includes variant name
    #[test]
    fn test_validation_result_debug_all_variants() {
        let cases = vec![
            (ValidationResult::Pass("p".into()), "Pass"),
            (ValidationResult::Fail("f".into()), "Fail"),
            (ValidationResult::Skipped("s".into()), "Skipped"),
        ];
        for (val, expected_variant) in cases {
            let dbg = format!("{:?}", val);
            assert!(
                dbg.contains(expected_variant),
                "Debug of {:?} should contain '{}'",
                val,
                expected_variant
            );
        }
    }

    /// FunctionalValidator::default() does not panic
    #[test]
    fn test_functional_validator_default_does_not_panic() {
        let _v = FunctionalValidator::default();
        // If we get here, it didn't panic
    }

    /// run_python_system with a simple valid script (python3 is available)
    #[test]
    fn test_run_python_system_simple_valid_script() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let result = v.run_python_system("x = 2 + 2\nprint(x)").unwrap();
        match result {
            ValidationResult::Pass(out) => {
                assert!(
                    out.trim().contains('4'),
                    "Expected '4' in output, got: {}",
                    out
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available */ }
            ValidationResult::Fail(err) => panic!("Simple arithmetic should not fail: {}", err),
        }
    }

    /// validate_python with empty code extraction returns Skipped
    #[test]
    fn test_validate_python_empty_extraction_returns_skipped() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        // Skill with non-python code blocks only
        let skill_md = "```bash\necho hello\n```\n";
        let result = v.validate(skill_md, &Language::Python).unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("No runnable code found in SKILL.md".to_string())
        );
    }

    /// extract: unclosed fence at end of document with only comments => empty
    #[test]
    fn test_extract_unclosed_fence_comments_only() {
        let skill_md = "```python\n# only a comment\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.is_empty(),
            "Unclosed block with only comments should be empty"
        );
    }

    /// extract: unclosed fence with empty lines only => empty
    #[test]
    fn test_extract_unclosed_fence_empty_lines_only() {
        let skill_md = "```python\n\n\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.is_empty(),
            "Unclosed block with only empty lines should be empty"
        );
    }

    /// extract: unclosed fence with 1 line, no import => still returned
    /// (after loop, code_lines has 1 entry, which is non-empty)
    #[test]
    fn test_extract_unclosed_fence_single_line_no_import() {
        let skill_md = "```python\nx = 42\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        // The loop exits without hitting the closing-fence acceptance check.
        // After loop: code_lines = ["x = 42"], which is non-empty,
        // so the code is returned despite being only 1 line.
        assert!(
            code.contains("x = 42"),
            "Unclosed fence with code should still return it"
        );
    }

    /// extract: closed block with import on 1 line, then unclosed second block
    /// First block accepted (import found), second block never reached.
    #[test]
    fn test_extract_closed_block_accepted_ignores_unclosed_second() {
        let skill_md = "```python\nimport os\n```\n```python\nthis never closes\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("import os"),
            "First valid block should be returned"
        );
        assert!(
            !code.contains("this never closes"),
            "Second block should not be included after first was accepted"
        );
    }

    /// extract: multiple blocks all have only comments => empty
    #[test]
    fn test_extract_multiple_comment_only_blocks() {
        let skill_md = "```python\n# a\n```\n\n```py\n# b\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.is_empty(),
            "Multiple comment-only blocks should result in empty"
        );
    }

    /// extract: block with from-import and no other lines => accepted (found_import=true)
    #[test]
    fn test_extract_from_import_only_accepted() {
        let skill_md = "```python\nfrom pathlib import Path\n```\n";
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = v.extract_python_hello_world(skill_md).unwrap();
        assert!(
            code.contains("from pathlib import Path"),
            "Single from-import line should be accepted"
        );
    }

    /// run_python_system: script that writes to stdout and exits 0
    #[test]
    fn test_run_python_system_stdout_captured() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let code = "import sys\nprint('line1')\nprint('line2', file=sys.stdout)";
        let result = v.run_python_system(code).unwrap();
        match result {
            ValidationResult::Pass(out) => {
                assert!(out.contains("line1"), "Should capture first print");
                assert!(out.contains("line2"), "Should capture second print");
            }
            ValidationResult::Skipped(_) => { /* python3 not available */ }
            ValidationResult::Fail(err) => panic!("Valid code should not fail: {}", err),
        }
    }

    /// validate_python: code with no assert or print, auto-appended print runs
    #[test]
    fn test_validate_python_auto_appended_print_runs() {
        let v = FunctionalValidator {
            container_runtime: None,
        };
        let skill_md = "```python\nimport os\nx = os.getcwd()\n```\n";
        let result = v.validate(skill_md, &Language::Python).unwrap();
        match result {
            ValidationResult::Pass(out) => {
                assert!(
                    out.contains("Code executed successfully"),
                    "Auto-appended print should produce output, got: {}",
                    out
                );
            }
            ValidationResult::Skipped(_) => { /* python3 not available */ }
            ValidationResult::Fail(err) => panic!("Code with auto-print should not fail: {}", err),
        }
    }
}
