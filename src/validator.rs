use anyhow::Result;
use std::fs;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;
use tracing::{debug, info, warn};

use crate::util::run_cmd_with_timeout;

/// Default timeout for validation commands (seconds)
const VALIDATION_TIMEOUT_SECS: u64 = 60;

/// Functional validator - actually runs code from SKILL.md
pub struct FunctionalValidator {
    use_docker: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// Code runs successfully
    Pass(String),
    /// Code failed to run
    Fail(String),
    /// Validation skipped (no docker, no python, etc.)
    Skipped(String),
}

impl FunctionalValidator {
    pub fn new() -> Self {
        let use_docker = Self::is_docker_available();
        if use_docker {
            info!("Docker available - using containerized validation");
        } else {
            warn!("Docker not available - using system Python (less safe)");
        }

        Self { use_docker }
    }

    /// Check if Docker is available
    fn is_docker_available() -> bool {
        Command::new("docker")
            .arg("ps")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Validate a generated SKILL.md by running extracted code
    pub fn validate(&self, skill_md: &str, ecosystem: &str) -> Result<ValidationResult> {
        match ecosystem {
            "python" => self.validate_python(skill_md),
            "javascript" | "typescript" => Ok(ValidationResult::Skipped(
                "JavaScript validation not yet implemented".to_string(),
            )),
            _ => Ok(ValidationResult::Skipped(format!(
                "Validation not supported for {}",
                ecosystem
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
        if self.use_docker {
            self.run_python_docker(&test_code)
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

    /// Run Python code in Docker container
    fn run_python_docker(&self, code: &str) -> Result<ValidationResult> {
        let temp = TempDir::new()?;
        let script_path = temp.path().join("test.py");
        fs::write(&script_path, code)?;

        debug!("Running Python in Docker: python:3.11-alpine");

        let mut cmd = Command::new("docker");
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
        let result = validator.validate(skill_md, "python").unwrap();

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
        let result = validator.validate(skill_md, "python").unwrap();

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
        let result = validator.validate("# SKILL.md", "javascript").unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("JavaScript validation not yet implemented".to_string())
        );
    }

    #[test]
    fn test_validate_typescript_skipped() {
        let validator = FunctionalValidator::new();
        let result = validator.validate("# SKILL.md", "typescript").unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("JavaScript validation not yet implemented".to_string())
        );
    }

    #[test]
    fn test_validate_unknown_ecosystem_skipped() {
        let validator = FunctionalValidator::new();
        let result = validator.validate("# SKILL.md", "rust").unwrap();
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
        let result = validator.validate(skill_md, "python").unwrap();
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
        assert_eq!(v1.use_docker, v2.use_docker);
    }

    #[test]
    fn test_validate_empty_string_returns_skipped() {
        let validator = FunctionalValidator::new();
        let result = validator.validate("", "python").unwrap();
        assert_eq!(
            result,
            ValidationResult::Skipped("No runnable code found in SKILL.md".to_string())
        );
    }

    #[test]
    fn test_validate_whitespace_only_returns_skipped() {
        let validator = FunctionalValidator::new();
        let result = validator.validate("   \n\n  \n", "python").unwrap();
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
        let result = validator.validate(skill_md, "python").unwrap();

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
}
