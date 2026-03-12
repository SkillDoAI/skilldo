//! Local execution environment for test code — creates isolated temp directories,
//! installs dependencies, and runs generated test scripts outside containers.
//! Used as a fallback when container runtime is unavailable.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::LanguageExecutor;
use crate::util::{run_cmd_with_timeout, sanitize_dep_name};

/// Check if a CLI tool is available in PATH.
pub(super) async fn is_tool_available(cmd: &str, arg: &str) -> bool {
    Command::new(cmd)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Classify a command execution result into Pass/Fail/Timeout.
/// `fail_output` extracts the error message from the output on failure.
fn classify_result(
    result: Result<Output>,
    timeout_secs: u64,
    lang: &str,
    fail_output: fn(&Output) -> String,
) -> Result<ExecutionResult> {
    match result {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                debug!("{} code execution passed", lang);
                Ok(ExecutionResult::Pass(stdout))
            } else {
                let msg = fail_output(&output);
                debug!("{} code execution failed", lang);
                Ok(ExecutionResult::Fail(msg))
            }
        }
        Err(e) => {
            if crate::error::SkillDoError::is_timeout(&e) {
                warn!(
                    "{} code execution timed out after {} seconds",
                    lang, timeout_secs
                );
                Ok(ExecutionResult::Timeout)
            } else {
                Err(e)
            }
        }
    }
}

/// Default failure output: stderr only.
fn stderr_only(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Node failure output: combine stdout + stderr (console.log goes to stdout).
fn stdout_and_stderr(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{stdout}\n{stderr}").trim().to_string()
}

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

        if !is_tool_available("uv", "--version").await {
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

        // Isolate UV cache inside temp dir (matches Go/Node cache isolation)
        let uv_cache = temp_dir.path().join("uv-cache");
        fs::create_dir_all(&uv_cache).context("Failed to create UV cache dir")?;

        // Run uv sync to create venv and install dependencies
        info!("Running uv sync to install dependencies...");
        let mut sync_cmd = Command::new("uv");
        sync_cmd
            .arg("sync")
            .arg("--no-dev")
            .env("UV_CACHE_DIR", &uv_cache)
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
        classify_result(result, self.timeout_secs, "Python", stderr_only)
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

const GO_PATH_DIR: &str = "gopath";
const GO_CACHE_DIR: &str = "gocache";
const GO_MOD_CACHE_DIR: &str = "gomodcache";

impl GoExecutor {
    pub fn new() -> Self {
        Self { timeout_secs: 60 }
    }

    /// Apply isolated GOPATH/GOCACHE/GOMODCACHE env vars to a command.
    fn apply_go_env(cmd: &mut Command, base: &Path) {
        cmd.env("GOPATH", base.join(GO_PATH_DIR))
            .env("GOCACHE", base.join(GO_CACHE_DIR))
            .env("GOMODCACHE", base.join(GO_MOD_CACHE_DIR));
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
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

        if !is_tool_available("go", "version").await {
            bail!("go is not installed or not in PATH");
        }

        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        debug!("Created temp directory: {}", temp_dir.path().display());

        // Isolate GOPATH, GOCACHE, and GOMODCACHE inside the temp dir (matches container executor)
        fs::create_dir_all(temp_dir.path().join(GO_PATH_DIR))
            .context("Failed to create GOPATH dir")?;
        fs::create_dir_all(temp_dir.path().join(GO_CACHE_DIR))
            .context("Failed to create GOCACHE dir")?;
        fs::create_dir_all(temp_dir.path().join(GO_MOD_CACHE_DIR))
            .context("Failed to create GOMODCACHE dir")?;

        // go mod init
        let mut init_cmd = Command::new("go");
        init_cmd
            .args(["mod", "init", "test"])
            .current_dir(temp_dir.path());
        Self::apply_go_env(&mut init_cmd, temp_dir.path());
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
            Self::apply_go_env(&mut get_cmd, temp_dir.path());
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
        Self::apply_go_env(&mut go_cmd, env.temp_dir.path());

        let result = run_cmd_with_timeout(go_cmd, timeout).await;
        classify_result(result, self.timeout_secs, "Go", stderr_only)
    }

    async fn cleanup(&self, _env: &ExecutionEnv) -> Result<()> {
        Ok(())
    }
}

/// Cargo executor — runs `cargo run` in a temp directory with Cargo.toml deps
pub struct CargoExecutor {
    timeout_secs: u64,
}

const CARGO_HOME_DIR: &str = "cargo-home";

impl CargoExecutor {
    pub fn new() -> Self {
        Self { timeout_secs: 120 }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

impl Default for CargoExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LanguageExecutor for CargoExecutor {
    async fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv> {
        info!(
            "Setting up Rust/Cargo environment with {} dependencies",
            deps.len()
        );

        if !is_tool_available("cargo", "--version").await {
            bail!("cargo is not installed or not in PATH");
        }

        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        debug!("Created temp directory: {}", temp_dir.path().display());

        // Isolate CARGO_HOME inside temp dir
        let cargo_home = temp_dir.path().join(CARGO_HOME_DIR);
        fs::create_dir_all(&cargo_home).context("Failed to create CARGO_HOME dir")?;

        // Validate dependency names
        for dep in deps {
            sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
        }

        // Build Cargo.toml with dependencies
        let deps_section = if deps.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = deps
                .iter()
                .map(|d| {
                    // Handle "crate_name = version_spec" vs bare "crate_name".
                    // Bare names get `= "*"` — non-reproducible but matches npm's
                    // approach. Version pinning is a follow-up enhancement.
                    if d.contains('=') {
                        d.to_string()
                    } else {
                        format!("{d} = \"*\"")
                    }
                })
                .collect();
            format!("\n[dependencies]\n{}\n", lines.join("\n"))
        };

        let cargo_toml = format!(
            r#"[package]
name = "skilldo-test"
version = "0.1.0"
edition = "2021"
{deps_section}"#
        );

        fs::write(temp_dir.path().join("Cargo.toml"), cargo_toml)
            .context("Failed to write Cargo.toml")?;

        // Create src directory with placeholder main.rs
        let src_dir = temp_dir.path().join("src");
        fs::create_dir_all(&src_dir).context("Failed to create src dir")?;
        fs::write(src_dir.join("main.rs"), "fn main() {}\n")
            .context("Failed to write placeholder main.rs")?;

        // cargo fetch to download dependencies (optional, cargo run will also do it)
        if !deps.is_empty() {
            info!("Fetching Rust dependencies...");
            let mut fetch_cmd = Command::new("cargo");
            fetch_cmd
                .arg("fetch")
                .env("CARGO_HOME", &cargo_home)
                .current_dir(temp_dir.path());
            let fetch_output =
                run_cmd_with_timeout(fetch_cmd, Duration::from_secs(self.timeout_secs)).await?;
            if !fetch_output.status.success() {
                let stderr = String::from_utf8_lossy(&fetch_output.stderr);
                bail!("cargo fetch failed: {}", stderr);
            }
        }

        info!("Rust/Cargo environment setup complete");

        Ok(ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: deps.to_vec(),
        })
    }

    async fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult> {
        debug!("Running Rust code ({} bytes)", code.len());

        let src_dir = env.temp_dir.path().join("src");
        let script_path = src_dir.join("main.rs");
        fs::write(&script_path, code).context("Failed to write main.rs")?;

        let cargo_home = env.temp_dir.path().join(CARGO_HOME_DIR);
        let timeout = Duration::from_secs(self.timeout_secs);
        let mut cargo_cmd = Command::new("cargo");
        cargo_cmd
            .args(["run", "--quiet", "--offline"])
            .env("CARGO_HOME", &cargo_home)
            .current_dir(env.temp_dir.path());

        let result = run_cmd_with_timeout(cargo_cmd, timeout).await;
        classify_result(result, self.timeout_secs, "Rust", stderr_only)
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

        if !is_tool_available("node", "--version").await {
            bail!("node is not installed or not in PATH");
        }

        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        debug!("Created temp directory: {}", temp_dir.path().display());

        // Initialize package.json with "type":"module" so ESM import syntax works.
        let package_json =
            r#"{"name":"skilldo-test","version":"0.1.0","private":true,"type":"module"}"#;
        fs::write(temp_dir.path().join("package.json"), package_json)
            .context("Failed to write package.json")?;

        // Isolate npm cache inside temp dir (matches container executor)
        let npm_cache = temp_dir.path().join("npm-cache");
        fs::create_dir_all(&npm_cache).context("Failed to create npm cache dir")?;

        // npm install for each dependency
        // No shell quoting needed — Command passes args directly to the process.
        // sanitize_dep_name already rejects shell metacharacters, and `--` prevents
        // flag injection.
        if !deps.is_empty() {
            if !is_tool_available("npm", "--version").await {
                bail!("npm is not installed or not in PATH");
            }
            for dep in deps {
                sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
            }

            info!("Installing Node.js dependencies: {}", deps.join(", "));
            let mut npm_cmd = Command::new("npm");
            npm_cmd
                .args([
                    "install",
                    "--no-save",
                    "--ignore-scripts",
                    "--no-audit",
                    "--no-fund",
                    "--",
                ])
                .args(deps)
                .env("npm_config_cache", &npm_cache)
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
        // Node combines stdout + stderr for retry feedback — console.log()
        // output (the LLM's primary diagnostic channel) goes to stdout.
        classify_result(result, self.timeout_secs, "Node.js", stdout_and_stderr)
    }

    async fn cleanup(&self, _env: &ExecutionEnv) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_is_tool_available() {
        // Just check it doesn't panic - availability depends on system
        let _ = is_tool_available("uv", "--version").await;
        let _ = is_tool_available("go", "version").await;
        let _ = is_tool_available("node", "--version").await;
        let _ = is_tool_available("npm", "--version").await;
        // Non-existent tool should return false
        assert!(!is_tool_available("nonexistent-tool-xyz", "--version").await);
    }

    #[test]
    fn test_classify_result_pass() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"hello".to_vec(),
            stderr: b"".to_vec(),
        };
        // ExitStatus::default() is success on Unix
        let result = classify_result(Ok(output), 60, "Test", stderr_only).unwrap();
        assert!(result.is_pass());
        assert_eq!(result.error_message(), "hello");
    }

    #[test]
    fn test_classify_result_fail() {
        // Platform-independent failed ExitStatus
        #[cfg(unix)]
        let failed_status = {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(1 << 8) // exit code 1
        };
        #[cfg(windows)]
        let failed_status = {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(1)
        };
        let output = Output {
            status: failed_status,
            stdout: b"".to_vec(),
            stderr: b"some error".to_vec(),
        };
        let result = classify_result(Ok(output), 60, "Test", stderr_only).unwrap();
        assert!(result.is_fail());
        assert_eq!(result.error_message(), "some error");
    }

    #[test]
    fn test_classify_result_timeout() {
        let err = crate::error::SkillDoError::Timeout(Duration::from_secs(60));
        let result = classify_result(Err(err.into()), 60, "Test", stderr_only).unwrap();
        assert!(matches!(result, ExecutionResult::Timeout));
    }

    #[test]
    fn test_classify_result_other_error() {
        let err = anyhow::anyhow!("some other error");
        let result = classify_result(Err(err), 60, "Test", stderr_only);
        assert!(result.is_err());
    }

    #[test]
    fn test_stdout_and_stderr_combiner() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"log output".to_vec(),
            stderr: b"error detail".to_vec(),
        };
        let combined = stdout_and_stderr(&output);
        assert!(combined.contains("log output"));
        assert!(combined.contains("error detail"));
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
    fn test_executor_defaults_and_timeouts() {
        // Python/Go/Node default to 60s, Cargo defaults to 120s (compilation takes longer)
        assert_eq!(PythonUvExecutor::default().timeout_secs, 60);
        assert_eq!(GoExecutor::default().timeout_secs, 60);
        assert_eq!(NodeExecutor::default().timeout_secs, 60);
        assert_eq!(CargoExecutor::default().timeout_secs, 120);

        assert_eq!(PythonUvExecutor::new().with_timeout(30).timeout_secs, 30);
        assert_eq!(GoExecutor::new().with_timeout(120).timeout_secs, 120);
        assert_eq!(NodeExecutor::new().with_timeout(0).timeout_secs, 0);
        assert_eq!(CargoExecutor::new().with_timeout(90).timeout_secs, 90);
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
        // cleanup() just returns Ok for all executors — TempDir handles actual cleanup
        let make_env = || {
            let temp_dir = TempDir::new().unwrap();
            ExecutionEnv {
                temp_dir,
                interpreter_path: None,
                container_name: None,
                dependencies: vec![],
            }
        };
        assert!(PythonUvExecutor::new().cleanup(&make_env()).await.is_ok());
        assert!(GoExecutor::new().cleanup(&make_env()).await.is_ok());
        assert!(NodeExecutor::new().cleanup(&make_env()).await.is_ok());
        assert!(CargoExecutor::new().cleanup(&make_env()).await.is_ok());
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

    #[tokio::test]
    async fn test_python_setup_uses_local_uv_cache() {
        if !is_tool_available("uv", "--version").await {
            return;
        }
        let executor = PythonUvExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        let uv_cache = env.temp_dir.path().join("uv-cache");
        assert!(
            uv_cache.exists(),
            "UV cache should be created inside temp dir"
        );
    }

    // --- GoExecutor tests ---

    #[tokio::test]
    async fn test_go_setup_environment_no_deps() {
        if !is_tool_available("go", "version").await {
            return; // Skip if go not installed
        }
        let executor = GoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        assert!(env.temp_dir.path().exists());
        assert!(env.temp_dir.path().join("go.mod").exists());
    }

    #[tokio::test]
    async fn test_go_run_simple_code() {
        if !is_tool_available("go", "version").await {
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
        if !is_tool_available("go", "version").await {
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
        if !is_tool_available("go", "version").await {
            return;
        }
        let executor = GoExecutor::new();
        let result = executor
            .setup_environment(&["valid-pkg; rm -rf /".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_go_setup_uses_local_gopath() {
        if !is_tool_available("go", "version").await {
            return;
        }
        let executor = GoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        let go_mod = env.temp_dir.path().join("go.mod");
        assert!(go_mod.exists());
        let gopath = env.temp_dir.path().join(GO_PATH_DIR);
        assert!(gopath.exists(), "GOPATH should be created inside temp dir");
        let gomodcache = env.temp_dir.path().join(GO_MOD_CACHE_DIR);
        assert!(
            gomodcache.exists(),
            "GOMODCACHE should be created inside temp dir"
        );
    }

    // --- NodeExecutor tests ---

    #[tokio::test]
    async fn test_node_setup_environment_no_deps() {
        if !is_tool_available("node", "--version").await {
            return; // Skip if node not installed
        }
        let executor = NodeExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        assert!(env.temp_dir.path().exists());
        assert!(env.temp_dir.path().join("package.json").exists());
    }

    #[tokio::test]
    async fn test_node_run_simple_code() {
        if !is_tool_available("node", "--version").await {
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
        if !is_tool_available("node", "--version").await {
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
        if !is_tool_available("node", "--version").await {
            return;
        }
        let executor = NodeExecutor::new();
        let result = executor
            .setup_environment(&["valid-pkg; rm -rf /".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_node_setup_uses_local_npm_cache() {
        if !is_tool_available("node", "--version").await {
            return;
        }
        let executor = NodeExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        let npm_cache = env.temp_dir.path().join("npm-cache");
        assert!(
            npm_cache.exists(),
            "npm cache should be created inside temp dir"
        );
    }

    // --- CargoExecutor tests ---

    #[tokio::test]
    async fn test_cargo_setup_environment_no_deps() {
        if !is_tool_available("cargo", "--version").await {
            return;
        }
        let executor = CargoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        assert!(env.temp_dir.path().exists());
        assert!(env.temp_dir.path().join("Cargo.toml").exists());
        assert!(env.temp_dir.path().join("src").join("main.rs").exists());
    }

    #[tokio::test]
    async fn test_cargo_run_simple_code() {
        if !is_tool_available("cargo", "--version").await {
            return;
        }
        let executor = CargoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"fn main() {
    println!("Hello from Rust test");
}
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_pass());
        if let ExecutionResult::Pass(output) = result {
            assert!(output.contains("Hello from Rust test"));
        }
    }

    #[tokio::test]
    async fn test_cargo_run_failing_code() {
        if !is_tool_available("cargo", "--version").await {
            return;
        }
        let executor = CargoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"fn main() {
    eprintln!("Test failure");
    std::process::exit(1);
}
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_fail());
    }

    #[tokio::test]
    async fn test_cargo_setup_environment_rejects_bad_deps() {
        if !is_tool_available("cargo", "--version").await {
            return;
        }
        let executor = CargoExecutor::new();
        let result = executor
            .setup_environment(&["valid-pkg; rm -rf /".to_string()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cargo_setup_uses_local_cargo_home() {
        if !is_tool_available("cargo", "--version").await {
            return;
        }
        let executor = CargoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        let cargo_home = env.temp_dir.path().join(CARGO_HOME_DIR);
        assert!(
            cargo_home.exists(),
            "CARGO_HOME should be created inside temp dir"
        );
    }

    #[tokio::test]
    async fn test_cargo_setup_with_deps_generates_cargo_toml() {
        if !is_tool_available("cargo", "--version").await {
            return;
        }
        let executor = CargoExecutor::new();
        let deps = vec!["serde".to_string(), "once_cell".to_string()];
        let env = executor.setup_environment(&deps).await.unwrap();
        let cargo_toml = std::fs::read_to_string(env.temp_dir.path().join("Cargo.toml")).unwrap();
        assert!(
            cargo_toml.contains("[dependencies]"),
            "should have deps section"
        );
        assert!(
            cargo_toml.contains("serde = \"*\""),
            "bare dep should get wildcard version"
        );
        assert!(
            cargo_toml.contains("once_cell = \"*\""),
            "bare dep should get wildcard version"
        );
    }

    #[tokio::test]
    async fn test_cargo_setup_no_deps_generates_minimal_toml() {
        if !is_tool_available("cargo", "--version").await {
            return;
        }
        let executor = CargoExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        let cargo_toml = std::fs::read_to_string(env.temp_dir.path().join("Cargo.toml")).unwrap();
        assert!(
            cargo_toml.contains("[package]"),
            "should have package section"
        );
        assert!(
            !cargo_toml.contains("[dependencies]"),
            "no deps means no deps section"
        );
    }
}
