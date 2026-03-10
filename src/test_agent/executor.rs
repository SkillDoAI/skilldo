//! Local execution environment for test code — creates isolated temp directories,
//! installs dependencies, and runs generated test scripts outside containers.
//! Used as a fallback when container runtime is unavailable.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::LanguageExecutor;
use crate::util::{run_cmd_with_timeout, sanitize_dep_name};

/// Represents an isolated execution environment
#[derive(Debug)]
pub struct ExecutionEnv {
    pub temp_dir: TempDir,
    #[allow(dead_code)]
    pub interpreter_path: Option<PathBuf>, // Path to language interpreter (e.g., Python venv)
    pub container_name: Option<String>, // For container-based execution
    pub dependencies: Vec<String>,      // Dependencies to install (for non-Python languages)
}

/// Result of code execution
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    Pass(String), // stdout
    Fail(String), // stderr
    Timeout,
}

impl ExecutionResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, ExecutionResult::Pass(_))
    }

    #[allow(dead_code)]
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
pub struct PythonUvExecutor {
    timeout_secs: u64,
}

impl PythonUvExecutor {
    pub fn new() -> Self {
        Self { timeout_secs: 60 }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Check if uv is available in PATH
    async fn check_uv_available() -> bool {
        Command::new("uv")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Default for PythonUvExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LanguageExecutor for PythonUvExecutor {
    async fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv> {
        info!(
            "Setting up Python environment with {} dependencies",
            deps.len()
        );

        // Check if uv is available
        if !Self::check_uv_available().await {
            bail!("uv is not installed or not in PATH (install: https://docs.astral.sh/uv/)");
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
        )
        .await?;

        if !sync_output.status.success() {
            let stderr = String::from_utf8_lossy(&sync_output.stderr);
            bail!("Failed to setup environment with uv sync: {}", stderr);
        }

        info!("✓ Environment setup complete");

        // Determine Python executable path
        let interpreter_path = if cfg!(target_os = "windows") {
            temp_dir
                .path()
                .join(".venv")
                .join("Scripts")
                .join("python.exe")
        } else {
            temp_dir.path().join(".venv").join("bin").join("python")
        };

        if !interpreter_path.exists() {
            bail!(
                "Python executable not found at expected path: {}",
                interpreter_path.display()
            );
        }

        Ok(ExecutionEnv {
            temp_dir,
            interpreter_path: Some(interpreter_path),
            container_name: None,
            dependencies: deps.to_vec(),
        })
    }

    async fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult> {
        debug!("Running Python code ({} bytes)", code.len());

        // Write code to test.py
        let script_path = env.temp_dir.path().join("test.py");
        fs::write(&script_path, code).context("Failed to write test script")?;

        // Run with timeout
        let timeout = Duration::from_secs(self.timeout_secs);
        let interpreter_path = env
            .interpreter_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("interpreter_path not set in execution environment"))?;
        let mut python_cmd = Command::new(interpreter_path);
        python_cmd
            .arg(&script_path)
            .current_dir(env.temp_dir.path());

        let result = run_cmd_with_timeout(python_cmd, timeout).await;

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
                if crate::error::SkillDoError::is_timeout(&e) {
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

    async fn cleanup(&self, env: &ExecutionEnv) -> Result<()> {
        debug!(
            "Cleaning up environment at {}",
            env.temp_dir.path().display()
        );
        // TempDir automatically cleans up when dropped
        Ok(())
    }
}

/// Go executor — runs `go run main.go` in a temp directory with `go mod init`
pub struct GoExecutor {
    timeout_secs: u64,
}

impl GoExecutor {
    pub fn new() -> Self {
        Self { timeout_secs: 60 }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Check if go is available in PATH
    async fn check_go_available() -> bool {
        Command::new("go")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Default for GoExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LanguageExecutor for GoExecutor {
    async fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv> {
        info!("Setting up Go environment with {} dependencies", deps.len());

        if !Self::check_go_available().await {
            bail!("go is not installed or not in PATH");
        }

        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        debug!("Created temp directory: {}", temp_dir.path().display());

        // go mod init
        let mut init_cmd = Command::new("go");
        init_cmd
            .args(["mod", "init", "test"])
            .current_dir(temp_dir.path());
        let init_output = run_cmd_with_timeout(init_cmd, Duration::from_secs(30)).await?;
        if !init_output.status.success() {
            let stderr = String::from_utf8_lossy(&init_output.stderr);
            bail!("go mod init failed: {}", stderr);
        }

        // go get for each dependency
        for dep in deps {
            sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
            info!("Installing Go dependency: {}", dep);
            let mut get_cmd = Command::new("go");
            get_cmd.args(["get", dep]).current_dir(temp_dir.path());
            let get_output = run_cmd_with_timeout(get_cmd, Duration::from_secs(120)).await?;
            if !get_output.status.success() {
                let stderr = String::from_utf8_lossy(&get_output.stderr);
                bail!("go get {} failed: {}", dep, stderr);
            }
        }

        info!("Go environment setup complete");

        Ok(ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: deps.to_vec(),
        })
    }

    async fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult> {
        debug!("Running Go code ({} bytes)", code.len());

        let script_path = env.temp_dir.path().join("main.go");
        fs::write(&script_path, code).context("Failed to write main.go")?;

        let timeout = Duration::from_secs(self.timeout_secs);
        let mut go_cmd = Command::new("go");
        go_cmd
            .args(["run", "main.go"])
            .current_dir(env.temp_dir.path());

        let result = run_cmd_with_timeout(go_cmd, timeout).await;

        match result {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    debug!("Go code execution passed");
                    Ok(ExecutionResult::Pass(stdout))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    debug!("Go code execution failed");
                    Ok(ExecutionResult::Fail(stderr))
                }
            }
            Err(e) => {
                if crate::error::SkillDoError::is_timeout(&e) {
                    warn!(
                        "Go code execution timed out after {} seconds",
                        self.timeout_secs
                    );
                    Ok(ExecutionResult::Timeout)
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn cleanup(&self, _env: &ExecutionEnv) -> Result<()> {
        Ok(())
    }
}

/// Node.js executor — runs `node test.js` in a temp directory with `npm install`
pub struct NodeExecutor {
    timeout_secs: u64,
}

impl NodeExecutor {
    pub fn new() -> Self {
        Self { timeout_secs: 60 }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Check if node is available in PATH
    async fn check_node_available() -> bool {
        Command::new("node")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Default for NodeExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LanguageExecutor for NodeExecutor {
    async fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv> {
        info!(
            "Setting up Node.js environment with {} dependencies",
            deps.len()
        );

        if !Self::check_node_available().await {
            bail!("node is not installed or not in PATH");
        }

        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        debug!("Created temp directory: {}", temp_dir.path().display());

        // Initialize package.json with "type":"module" so ESM import syntax works.
        let package_json =
            r#"{"name":"skilldo-test","version":"0.1.0","private":true,"type":"module"}"#;
        fs::write(temp_dir.path().join("package.json"), package_json)
            .context("Failed to write package.json")?;

        // npm install for each dependency
        // No shell quoting needed — Command passes args directly to the process.
        // sanitize_dep_name already rejects shell metacharacters, and `--` prevents
        // flag injection.
        if !deps.is_empty() {
            for dep in deps {
                sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
            }

            info!("Installing Node.js dependencies: {}", deps.join(", "));
            let mut npm_cmd = Command::new("npm");
            npm_cmd
                .args(["install", "--no-save", "--"])
                .args(deps)
                .current_dir(temp_dir.path());
            let npm_output = run_cmd_with_timeout(npm_cmd, Duration::from_secs(120)).await?;
            if !npm_output.status.success() {
                let stderr = String::from_utf8_lossy(&npm_output.stderr);
                bail!("npm install failed: {}", stderr);
            }
        }

        info!("Node.js environment setup complete");

        Ok(ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: deps.to_vec(),
        })
    }

    async fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult> {
        debug!("Running Node.js code ({} bytes)", code.len());

        let script_path = env.temp_dir.path().join("test.js");
        fs::write(&script_path, code).context("Failed to write test.js")?;

        let timeout = Duration::from_secs(self.timeout_secs);
        let mut node_cmd = Command::new("node");
        node_cmd.arg("test.js").current_dir(env.temp_dir.path());

        let result = run_cmd_with_timeout(node_cmd, timeout).await;

        match result {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    debug!("Node.js code execution passed");
                    Ok(ExecutionResult::Pass(stdout))
                } else {
                    // Combine stdout + stderr for retry feedback — console.log()
                    // output (the LLM's primary diagnostic channel) goes to stdout.
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let combined = format!("{stdout}\n{stderr}").trim().to_string();
                    debug!("Node.js code execution failed");
                    Ok(ExecutionResult::Fail(combined))
                }
            }
            Err(e) => {
                if crate::error::SkillDoError::is_timeout(&e) {
                    warn!(
                        "Node.js code execution timed out after {} seconds",
                        self.timeout_secs
                    );
                    Ok(ExecutionResult::Timeout)
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn cleanup(&self, _env: &ExecutionEnv) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_uv_available() {
        // Just check it doesn't panic - availability depends on system
        let _ = PythonUvExecutor::check_uv_available().await;
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

    #[tokio::test]
    async fn test_setup_environment_no_deps() {
        let executor = PythonUvExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        assert!(env.interpreter_path.is_some());
        assert!(env.interpreter_path.as_ref().unwrap().exists());
        assert!(env.temp_dir.path().exists());
    }

    #[tokio::test]
    async fn test_run_simple_code() {
        let executor = PythonUvExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"
print("Hello from test")
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_pass());

        if let ExecutionResult::Pass(output) = result {
            assert!(output.contains("Hello from test"));
        }
    }

    #[tokio::test]
    async fn test_run_failing_code() {
        let executor = PythonUvExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"
raise ValueError("Test error")
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_fail());

        if let ExecutionResult::Fail(error) = result {
            assert!(error.contains("ValueError"));
        }
    }

    #[tokio::test]
    async fn test_cleanup_is_noop() {
        // cleanup() just returns Ok — TempDir handles actual cleanup
        let executor = PythonUvExecutor::new();
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec![],
        };
        assert!(executor.cleanup(&env).await.is_ok());
    }

    #[tokio::test]
    async fn test_run_code_missing_interpreter_path() {
        let executor = PythonUvExecutor::new();
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let result = executor.run_code(&env, "print('hello')").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("interpreter_path not set"));
    }

    #[tokio::test]
    async fn test_run_code_nonexistent_python() {
        let executor = PythonUvExecutor::new();
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: Some(PathBuf::from("/nonexistent/python3")),
            container_name: None,
            dependencies: vec![],
        };
        // Should fail because the python binary doesn't exist
        let result = executor.run_code(&env, "print('hello')").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_setup_environment_rejects_bad_deps() {
        let executor = PythonUvExecutor::new();
        // If uv isn't available, setup_environment will bail before dep validation.
        // If uv IS available, it should still reject the bad dep name.
        let result = executor
            .setup_environment(&["valid-pkg; rm -rf /".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_setup_with_dependencies() {
        let executor = PythonUvExecutor::new();
        let deps = vec!["click".to_string()];
        let env = executor.setup_environment(&deps).await.unwrap();

        let code = r#"
import click
print(f"Click version: {click.__version__}")
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_pass());
    }

    // --- Clone derive coverage ---

    #[test]
    fn test_execution_result_clone_pass() {
        let original = ExecutionResult::Pass("output".to_string());
        let cloned = original.clone();
        assert!(cloned.is_pass());
        assert_eq!(cloned.error_message(), "output");
    }

    #[test]
    fn test_execution_result_clone_fail() {
        let original = ExecutionResult::Fail("some error".to_string());
        let cloned = original.clone();
        assert!(cloned.is_fail());
        assert_eq!(cloned.error_message(), "some error");
    }

    #[test]
    fn test_execution_result_clone_timeout() {
        let original = ExecutionResult::Timeout;
        let cloned = original.clone();
        assert!(!cloned.is_pass());
        assert!(!cloned.is_fail());
        assert_eq!(
            cloned.error_message(),
            "Test execution timed out (60 seconds)"
        );
    }

    // --- Debug derive coverage ---

    #[test]
    fn test_execution_result_debug_pass() {
        let result = ExecutionResult::Pass("hello".to_string());
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Pass"));
        assert!(debug_str.contains("hello"));
    }

    #[test]
    fn test_execution_result_debug_fail() {
        let result = ExecutionResult::Fail("error msg".to_string());
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Fail"));
        assert!(debug_str.contains("error msg"));
    }

    #[test]
    fn test_execution_result_debug_timeout() {
        let result = ExecutionResult::Timeout;
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Timeout"));
    }

    #[test]
    fn test_execution_env_debug() {
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: Some(PathBuf::from("/usr/bin/python3")),
            container_name: Some("test-ctr".to_string()),
            dependencies: vec!["requests".to_string()],
        };
        let debug_str = format!("{:?}", env);
        assert!(debug_str.contains("ExecutionEnv"));
        assert!(debug_str.contains("test-ctr"));
        assert!(debug_str.contains("requests"));
    }

    #[test]
    fn test_execution_env_debug_none_fields() {
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let debug_str = format!("{:?}", env);
        assert!(debug_str.contains("ExecutionEnv"));
        assert!(debug_str.contains("None"));
    }

    // --- PythonUvExecutor::new() direct usage ---

    #[test]
    fn test_python_uv_executor_new_timeout() {
        let executor = PythonUvExecutor::new();
        assert_eq!(executor.timeout_secs, 60);
    }

    // --- ExecutionEnv field access ---

    #[test]
    fn test_execution_env_dependencies_stored() {
        let temp_dir = TempDir::new().unwrap();
        let deps = vec!["numpy".to_string(), "pandas".to_string()];
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: deps,
        };
        assert_eq!(env.dependencies.len(), 2);
        assert_eq!(env.dependencies[0], "numpy");
        assert_eq!(env.dependencies[1], "pandas");
    }

    #[test]
    fn test_execution_env_container_name_field() {
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: Some("my-container".to_string()),
            dependencies: vec![],
        };
        assert_eq!(env.container_name.as_deref(), Some("my-container"));
    }

    #[test]
    fn test_execution_env_interpreter_path_field() {
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: Some(PathBuf::from("/usr/local/bin/python3")),
            container_name: None,
            dependencies: vec![],
        };
        assert_eq!(
            env.interpreter_path.as_deref(),
            Some(std::path::Path::new("/usr/local/bin/python3"))
        );
    }

    // --- with_timeout chaining ---

    #[test]
    fn test_with_timeout_chained() {
        let executor = PythonUvExecutor::new().with_timeout(120);
        assert_eq!(executor.timeout_secs, 120);
    }

    #[test]
    fn test_with_timeout_zero() {
        let executor = PythonUvExecutor::new().with_timeout(0);
        assert_eq!(executor.timeout_secs, 0);
    }

    // --- error_message with empty strings ---

    #[test]
    fn test_execution_result_pass_empty_message() {
        let result = ExecutionResult::Pass(String::new());
        assert!(result.is_pass());
        assert_eq!(result.error_message(), "");
    }

    #[test]
    fn test_execution_result_fail_empty_message() {
        let result = ExecutionResult::Fail(String::new());
        assert!(result.is_fail());
        assert_eq!(result.error_message(), "");
    }

    // --- GoExecutor tests ---

    #[tokio::test]
    async fn test_check_go_available() {
        // Just check it doesn't panic - availability depends on system
        let _ = GoExecutor::check_go_available().await;
    }

    #[test]
    fn test_go_executor_default() {
        let executor = GoExecutor::default();
        assert_eq!(executor.timeout_secs, 60);
    }

    #[test]
    fn test_go_executor_with_timeout() {
        let executor = GoExecutor::new().with_timeout(30);
        assert_eq!(executor.timeout_secs, 30);
    }

    #[test]
    fn test_go_executor_with_timeout_chained() {
        let executor = GoExecutor::new().with_timeout(120);
        assert_eq!(executor.timeout_secs, 120);
    }

    #[test]
    fn test_go_executor_with_timeout_zero() {
        let executor = GoExecutor::new().with_timeout(0);
        assert_eq!(executor.timeout_secs, 0);
    }

    #[tokio::test]
    async fn test_go_setup_environment_no_deps() {
        if !GoExecutor::check_go_available().await {
            return; // Skip if go not installed
        }
        let executor = GoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        assert!(env.temp_dir.path().exists());
        assert!(env.temp_dir.path().join("go.mod").exists());
    }

    #[tokio::test]
    async fn test_go_run_simple_code() {
        if !GoExecutor::check_go_available().await {
            return;
        }
        let executor = GoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"package main

import "fmt"

func main() {
    fmt.Println("Hello from Go test")
}
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_pass());
        if let ExecutionResult::Pass(output) = result {
            assert!(output.contains("Hello from Go test"));
        }
    }

    #[tokio::test]
    async fn test_go_run_failing_code() {
        if !GoExecutor::check_go_available().await {
            return;
        }
        let executor = GoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"package main

import "log"

func main() {
    log.Fatal("Test failure")
}
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_fail());
    }

    #[tokio::test]
    async fn test_go_setup_environment_rejects_bad_deps() {
        if !GoExecutor::check_go_available().await {
            return;
        }
        let executor = GoExecutor::new();
        let result = executor
            .setup_environment(&["valid-pkg; rm -rf /".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_go_cleanup_is_noop() {
        let executor = GoExecutor::new();
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec![],
        };
        assert!(executor.cleanup(&env).await.is_ok());
    }

    // --- NodeExecutor tests ---

    #[tokio::test]
    async fn test_check_node_available() {
        // Just check it doesn't panic - availability depends on system
        let _ = NodeExecutor::check_node_available().await;
    }

    #[test]
    fn test_node_executor_default() {
        let executor = NodeExecutor::default();
        assert_eq!(executor.timeout_secs, 60);
    }

    #[test]
    fn test_node_executor_with_timeout() {
        let executor = NodeExecutor::new().with_timeout(30);
        assert_eq!(executor.timeout_secs, 30);
    }

    #[test]
    fn test_node_executor_with_timeout_chained() {
        let executor = NodeExecutor::new().with_timeout(120);
        assert_eq!(executor.timeout_secs, 120);
    }

    #[test]
    fn test_node_executor_with_timeout_zero() {
        let executor = NodeExecutor::new().with_timeout(0);
        assert_eq!(executor.timeout_secs, 0);
    }

    #[tokio::test]
    async fn test_node_setup_environment_no_deps() {
        if !NodeExecutor::check_node_available().await {
            return; // Skip if node not installed
        }
        let executor = NodeExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        assert!(env.temp_dir.path().exists());
        assert!(env.temp_dir.path().join("package.json").exists());
    }

    #[tokio::test]
    async fn test_node_run_simple_code() {
        if !NodeExecutor::check_node_available().await {
            return;
        }
        let executor = NodeExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"console.log("Hello from Node.js test");"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_pass());
        if let ExecutionResult::Pass(output) = result {
            assert!(output.contains("Hello from Node.js test"));
        }
    }

    #[tokio::test]
    async fn test_node_run_failing_code() {
        if !NodeExecutor::check_node_available().await {
            return;
        }
        let executor = NodeExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"process.exit(1);"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_fail());
    }

    #[tokio::test]
    async fn test_node_setup_environment_rejects_bad_deps() {
        if !NodeExecutor::check_node_available().await {
            return;
        }
        let executor = NodeExecutor::new();
        let result = executor
            .setup_environment(&["valid-pkg; rm -rf /".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_node_cleanup_is_noop() {
        let executor = NodeExecutor::new();
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec![],
        };
        assert!(executor.cleanup(&env).await.is_ok());
    }
}
