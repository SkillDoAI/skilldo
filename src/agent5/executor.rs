use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tracing::{debug, info, warn};

use super::LanguageExecutor;
use crate::util::{run_cmd_with_timeout, sanitize_dep_name};

/// Represents an isolated execution environment
#[derive(Debug)]
pub struct ExecutionEnv {
    pub temp_dir: TempDir,
    #[allow(dead_code)]
    pub python_path: Option<PathBuf>, // For uv-based execution
    pub container_name: Option<String>, // For container-based execution
    pub dependencies: Vec<String>,      // Dependencies to install (for non-Python languages)
}

/// Result of code execution
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    Pass(String), // stdout
    Fail(String), // stderr
    #[allow(dead_code)]
    Timeout,
}

impl ExecutionResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, ExecutionResult::Pass(_))
    }

    pub fn is_fail(&self) -> bool {
        matches!(self, ExecutionResult::Fail(_))
    }

    pub fn error_message(&self) -> String {
        match self {
            ExecutionResult::Pass(msg) => msg.clone(),
            ExecutionResult::Fail(msg) => msg.clone(),
            ExecutionResult::Timeout => "Test execution timed out (60 seconds)".to_string(),
        }
    }
}

/// Python executor using `uv` for fast environment setup
#[allow(dead_code)]
pub struct PythonUvExecutor {
    timeout_secs: u64,
}

#[allow(dead_code)]
impl PythonUvExecutor {
    pub fn new() -> Self {
        Self { timeout_secs: 60 }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Check if uv is available in PATH
    fn check_uv_available() -> bool {
        Command::new("uv")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Default for PythonUvExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageExecutor for PythonUvExecutor {
    fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv> {
        info!(
            "Setting up Python environment with {} dependencies",
            deps.len()
        );

        // Check if uv is available
        if !Self::check_uv_available() {
            bail!("uv is not installed or not in PATH. Install with: pip install uv");
        }

        // Create temporary directory
        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        debug!("Created temp directory: {}", temp_dir.path().display());

        // Validate dependency names before writing to pyproject.toml
        for dep in deps {
            sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
        }

        // Create pyproject.toml
        let dependencies_str = if deps.is_empty() {
            String::new()
        } else {
            deps.iter()
                .map(|d| format!("    \"{}\",", d))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let pyproject_content = format!(
            r#"[project]
name = "skilldo-test"
version = "0.1.0"
requires-python = ">=3.8"
dependencies = [
{}
]
"#,
            dependencies_str
        );

        let pyproject_path = temp_dir.path().join("pyproject.toml");
        fs::write(&pyproject_path, pyproject_content).context("Failed to write pyproject.toml")?;

        debug!("Created pyproject.toml with dependencies: {:?}", deps);

        // Run uv sync to create venv and install dependencies
        info!("Running uv sync to install dependencies...");
        let mut sync_cmd = Command::new("uv");
        sync_cmd
            .arg("sync")
            .arg("--no-dev")
            .current_dir(temp_dir.path());

        let sync_output = run_cmd_with_timeout(
            sync_cmd,
            Duration::from_secs(120), // 2 minutes for dependency installation
        )?;

        if !sync_output.status.success() {
            let stderr = String::from_utf8_lossy(&sync_output.stderr);
            bail!("Failed to setup environment with uv sync: {}", stderr);
        }

        info!("✓ Environment setup complete");

        // Determine Python executable path
        let python_path = if cfg!(target_os = "windows") {
            temp_dir
                .path()
                .join(".venv")
                .join("Scripts")
                .join("python.exe")
        } else {
            temp_dir.path().join(".venv").join("bin").join("python")
        };

        if !python_path.exists() {
            bail!(
                "Python executable not found at expected path: {}",
                python_path.display()
            );
        }

        Ok(ExecutionEnv {
            temp_dir,
            python_path: Some(python_path),
            container_name: None,
            dependencies: deps.to_vec(),
        })
    }

    fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult> {
        debug!("Running Python code ({} bytes)", code.len());

        // Write code to test.py
        let script_path = env.temp_dir.path().join("test.py");
        fs::write(&script_path, code).context("Failed to write test script")?;

        // Run with timeout
        let timeout = Duration::from_secs(self.timeout_secs);
        let python_path = env
            .python_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Python path not set in execution environment"))?;
        let mut python_cmd = Command::new(python_path);
        python_cmd
            .arg(&script_path)
            .current_dir(env.temp_dir.path());

        let result = run_cmd_with_timeout(python_cmd, timeout);

        match result {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    debug!("✓ Code execution passed");
                    Ok(ExecutionResult::Pass(stdout))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    debug!("✗ Code execution failed");
                    Ok(ExecutionResult::Fail(stderr))
                }
            }
            Err(e) => {
                if e.to_string().contains("timed out") {
                    warn!(
                        "Code execution timed out after {} seconds",
                        self.timeout_secs
                    );
                    Ok(ExecutionResult::Timeout)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn cleanup(&self, env: &ExecutionEnv) -> Result<()> {
        debug!(
            "Cleaning up environment at {}",
            env.temp_dir.path().display()
        );
        // TempDir automatically cleans up when dropped
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_uv_available() {
        // Just check it doesn't panic - availability depends on system
        let _ = PythonUvExecutor::check_uv_available();
    }

    #[test]
    fn test_execution_result_is_pass() {
        let result = ExecutionResult::Pass("ok".into());
        assert!(result.is_pass());
        assert!(!result.is_fail());
    }

    #[test]
    fn test_execution_result_is_fail() {
        let result = ExecutionResult::Fail("err".into());
        assert!(result.is_fail());
        assert!(!result.is_pass());
    }

    #[test]
    fn test_execution_result_timeout() {
        let result = ExecutionResult::Timeout;
        assert!(!result.is_pass());
        assert!(!result.is_fail());
    }

    #[test]
    fn test_execution_result_error_message() {
        assert_eq!(
            ExecutionResult::Pass("stdout output".into()).error_message(),
            "stdout output"
        );
        assert_eq!(
            ExecutionResult::Fail("stderr output".into()).error_message(),
            "stderr output"
        );
        assert_eq!(
            ExecutionResult::Timeout.error_message(),
            "Test execution timed out (60 seconds)"
        );
    }

    #[test]
    fn test_python_uv_executor_default() {
        let executor = PythonUvExecutor::default();
        assert_eq!(executor.timeout_secs, 60);
    }

    #[test]
    fn test_python_uv_executor_with_timeout() {
        let executor = PythonUvExecutor::new().with_timeout(30);
        assert_eq!(executor.timeout_secs, 30);
    }

    #[test]
    fn test_setup_environment_no_deps() {
        let executor = PythonUvExecutor::new();
        let env = executor.setup_environment(&[]).unwrap();

        assert!(env.python_path.is_some());
        assert!(env.python_path.as_ref().unwrap().exists());
        assert!(env.temp_dir.path().exists());
    }

    #[test]
    fn test_run_simple_code() {
        let executor = PythonUvExecutor::new();
        let env = executor.setup_environment(&[]).unwrap();

        let code = r#"
print("Hello from test")
"#;

        let result = executor.run_code(&env, code).unwrap();
        assert!(result.is_pass());

        if let ExecutionResult::Pass(output) = result {
            assert!(output.contains("Hello from test"));
        }
    }

    #[test]
    fn test_run_failing_code() {
        let executor = PythonUvExecutor::new();
        let env = executor.setup_environment(&[]).unwrap();

        let code = r#"
raise ValueError("Test error")
"#;

        let result = executor.run_code(&env, code).unwrap();
        assert!(result.is_fail());

        if let ExecutionResult::Fail(error) = result {
            assert!(error.contains("ValueError"));
        }
    }

    #[test]
    fn test_cleanup_is_noop() {
        // cleanup() just returns Ok — TempDir handles actual cleanup
        let executor = PythonUvExecutor::new();
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        assert!(executor.cleanup(&env).is_ok());
    }

    #[test]
    fn test_run_code_missing_python_path() {
        let executor = PythonUvExecutor::new();
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let result = executor.run_code(&env, "print('hello')");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Python path not set"));
    }

    #[test]
    fn test_run_code_nonexistent_python() {
        let executor = PythonUvExecutor::new();
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: Some(PathBuf::from("/nonexistent/python3")),
            container_name: None,
            dependencies: vec![],
        };
        // Should fail because the python binary doesn't exist
        let result = executor.run_code(&env, "print('hello')");
        assert!(result.is_err());
    }

    #[test]
    fn test_setup_environment_rejects_bad_deps() {
        let executor = PythonUvExecutor::new();
        // If uv isn't available, setup_environment will bail before dep validation.
        // If uv IS available, it should still reject the bad dep name.
        let result = executor.setup_environment(&["valid-pkg; rm -rf /".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_setup_with_dependencies() {
        let executor = PythonUvExecutor::new();
        let deps = vec!["click".to_string()];
        let env = executor.setup_environment(&deps).unwrap();

        let code = r#"
import click
print(f"Click version: {click.__version__}")
"#;

        let result = executor.run_code(&env, code).unwrap();
        assert!(result.is_pass());
    }
}
