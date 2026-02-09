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
}
