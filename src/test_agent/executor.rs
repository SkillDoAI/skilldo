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
                debug!("{} code execution failed:\n{}", lang, msg);
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
            ExecutionResult::Timeout => "Test execution timed out".to_string(),
        }
    }
}

/// Python executor using `uv` for fast environment setup
pub struct PythonUvExecutor {
    timeout_secs: u64,
    local_source: Option<String>,
}

impl PythonUvExecutor {
    pub fn new() -> Self {
        Self {
            timeout_secs: 60,
            local_source: None,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn with_local_source(mut self, path: String) -> Self {
        self.local_source = Some(path);
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

        // Install local package if local-install mode is active
        if let Some(ref source) = self.local_source {
            info!("Installing local package from: {}", source);
            let mut pip_cmd = Command::new("uv");
            pip_cmd
                .args(["pip", "install", "-e", source, "--no-deps"])
                .env("UV_CACHE_DIR", &uv_cache)
                .env(
                    "VIRTUAL_ENV",
                    temp_dir.path().join(".venv").to_string_lossy().as_ref(),
                )
                .current_dir(temp_dir.path());
            let pip_output = run_cmd_with_timeout(pip_cmd, Duration::from_secs(60)).await?;
            if !pip_output.status.success() {
                let stderr = String::from_utf8_lossy(&pip_output.stderr);
                bail!("Failed to install local package: {}", stderr);
            }
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
    local_source: Option<String>,
}

const GO_PATH_DIR: &str = "gopath";
const GO_CACHE_DIR: &str = "gocache";
const GO_MOD_CACHE_DIR: &str = "gomodcache";

impl GoExecutor {
    pub fn new() -> Self {
        Self {
            timeout_secs: 60,
            local_source: None,
        }
    }

    pub fn with_local_source(mut self, path: String) -> Self {
        self.local_source = Some(path);
        self
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

        // Validate all dep names first
        for dep in deps {
            sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
        }

        // Local-install: apply replace directives BEFORE go get, so unpublished
        // modules resolve from local source instead of failing on the registry.
        let local_module = if let Some(ref source) = self.local_source {
            let go_mod = std::path::Path::new(source).join("go.mod");
            std::fs::read_to_string(&go_mod).ok().and_then(|content| {
                content.lines().find_map(|line| {
                    line.trim()
                        .strip_prefix("module ")
                        .map(|m| m.trim().to_string())
                })
            })
        } else {
            None
        };

        if let (Some(ref module), Some(ref source)) = (&local_module, &self.local_source) {
            // Apply replace for the target module and any subpackages
            for dep in deps {
                if dep == module.as_str() || dep.starts_with(&format!("{module}/")) {
                    info!("Replacing {} with local source: {}", dep, source);
                    let mut replace_cmd = Command::new("go");
                    replace_cmd
                        .args(["mod", "edit", "-replace", &format!("{dep}={source}")])
                        .current_dir(temp_dir.path());
                    Self::apply_go_env(&mut replace_cmd, temp_dir.path());
                    let replace_output =
                        run_cmd_with_timeout(replace_cmd, Duration::from_secs(30)).await?;
                    if !replace_output.status.success() {
                        let stderr = String::from_utf8_lossy(&replace_output.stderr);
                        warn!("go mod edit -replace failed for {}: {}", dep, stderr);
                    }
                }
            }
        }

        // go get for each dependency (local modules now resolve via replace)
        for dep in deps {
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
    local_source: Option<String>,
}

const CARGO_HOME_DIR: &str = "cargo-home";

impl CargoExecutor {
    pub fn new() -> Self {
        Self {
            timeout_secs: 120,
            local_source: None,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn with_local_source(mut self, path: String) -> Self {
        self.local_source = Some(path);
        self
    }

    /// Set up Rust environment with structured deps (preserves version/features).
    /// Called from the Rust-specific validator path instead of the trait method.
    pub async fn setup_structured_environment(
        &self,
        deps: &[crate::pipeline::collector::StructuredDep],
    ) -> Result<ExecutionEnv> {
        info!(
            "Setting up Rust/Cargo environment with {} structured dependencies",
            deps.len()
        );

        if !is_tool_available("cargo", "--version").await {
            bail!("cargo is not installed or not in PATH");
        }

        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        let cargo_home = temp_dir.path().join(CARGO_HOME_DIR);
        fs::create_dir_all(&cargo_home).context("Failed to create CARGO_HOME dir")?;

        // Build Cargo.toml with structured deps (preserves versions and features)
        let deps_section = if deps.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = deps
                .iter()
                .map(|d| {
                    // Local-install override for target package
                    if let Some(ref source) = self.local_source {
                        let cargo_path = std::path::Path::new(source).join("Cargo.toml");
                        let is_local = std::fs::read_to_string(&cargo_path)
                            .ok()
                            .and_then(|content| {
                                content.lines().find_map(|line| {
                                    let t = line.trim();
                                    if t.starts_with("name") && t.contains('=') {
                                        let val = t.split('=').nth(1)?.trim().trim_matches('"');
                                        Some(val == d.name)
                                    } else {
                                        None
                                    }
                                })
                            })
                            .unwrap_or(false);
                        if is_local {
                            // Forward slashes work in TOML on all platforms
                            let safe = source.replace('\\', "/");
                            return format!("{} = {{ path = \"{}\" }}", d.name, safe);
                        }
                    }
                    // Use raw spec (preserves version + features)
                    if let Some(ref spec) = d.raw_spec {
                        format!("{} = {}", d.name, spec)
                    } else {
                        format!("{} = \"*\"", d.name)
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

        debug!(
            "Structured deps for Cargo.toml: {:?}",
            deps.iter()
                .map(|d| format!("{}={:?}", d.name, d.raw_spec))
                .collect::<Vec<_>>()
        );
        debug!("Generated Cargo.toml:\n{}", cargo_toml);
        fs::write(temp_dir.path().join("Cargo.toml"), &cargo_toml)
            .context("Failed to write Cargo.toml")?;

        let src_dir = temp_dir.path().join("src");
        fs::create_dir_all(&src_dir)?;
        fs::write(src_dir.join("main.rs"), "fn main() {}\n")?;

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

        info!("Rust/Cargo structured environment setup complete");
        let dep_names: Vec<String> = deps.iter().map(|d| d.name.clone()).collect();
        Ok(ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: dep_names,
        })
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
                    if let Some(ref source) = self.local_source {
                        // Local-install: use path dep only for the target package.
                        // Other deps (tokio, reqwest, etc.) come from the registry.
                        let cargo_toml = std::path::Path::new(source).join("Cargo.toml");
                        let is_local_pkg = std::fs::read_to_string(&cargo_toml)
                            .ok()
                            .and_then(|content| {
                                content.lines().find_map(|line| {
                                    let trimmed = line.trim();
                                    if trimmed.starts_with("name") && trimmed.contains('=') {
                                        let val =
                                            trimmed.split('=').nth(1)?.trim().trim_matches('"');
                                        Some(val == *d)
                                    } else {
                                        None
                                    }
                                })
                            })
                            .unwrap_or(false);
                        if is_local_pkg {
                            let safe = source.replace('\\', "/");
                            format!("{d} = {{ path = \"{}\" }}", safe)
                        } else {
                            format!("{d} = \"*\"")
                        }
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
        // --offline: deps already fetched in setup_environment(); skip registry checks.
        // Safe for zero-dep projects too (no registry access needed).
        let mut cargo_cmd = Command::new("cargo");
        cargo_cmd.args(["run", "--quiet", "--offline"]);
        cargo_cmd
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
    local_source: Option<String>,
}

impl NodeExecutor {
    pub fn new() -> Self {
        Self {
            timeout_secs: 60,
            local_source: None,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn with_local_source(mut self, path: String) -> Self {
        self.local_source = Some(path);
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

            // For local-install: install local source (provides the target package),
            // then remaining deps from registry. npm install handles both.
            let install_args: Vec<String> = if let Some(ref source) = self.local_source {
                let pkg_json = std::path::Path::new(source).join("package.json");
                let local_name = std::fs::read_to_string(&pkg_json).ok().and_then(|c| {
                    c.lines().find_map(|l| {
                        let t = l.trim();
                        if t.starts_with("\"name\"") {
                            Some(
                                t.split(':')
                                    .nth(1)?
                                    .trim()
                                    .trim_matches(|c| c == '"' || c == ',')
                                    .to_string(),
                            )
                        } else {
                            None
                        }
                    })
                });
                info!("Installing Node.js from local source: {}", source);
                let mut args = vec![source.clone()];
                // Add non-local deps from registry
                for d in deps {
                    if local_name.as_deref() != Some(d.as_str()) {
                        args.push(d.clone());
                    }
                }
                args
            } else {
                info!("Installing Node.js dependencies: {}", deps.join(", "));
                deps.to_vec()
            };
            let install_refs: Vec<&str> = install_args.iter().map(|s| s.as_str()).collect();
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
                .args(&install_refs)
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

/// Java executor — compiles with `javac` and runs with `java`.
/// For projects with Maven dependencies, creates a minimal pom.xml and uses
/// `mvn dependency:copy-dependencies` if mvn is available.
///
/// **Timeout note:** Each phase gets its own `timeout_secs` budget:
///
///   - `setup_environment`: mvn dependency:copy-dependencies → up to 1× timeout_secs
///   - `run_code`: javac compile → up to 1× timeout_secs
///   - `run_code`: java execute  → up to 1× timeout_secs
///
/// Total wall-clock can reach **3× timeout_secs** (default 120s → 360s / 6 min).
/// Callers should set timeout_secs accordingly.
pub struct JavaExecutor {
    timeout_secs: u64,
    local_source: Option<String>,
}

pub(crate) const MAVEN_REPO_DIR: &str = "m2-repo";

impl JavaExecutor {
    pub fn new() -> Self {
        Self {
            timeout_secs: 120,
            local_source: None,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn with_local_source(mut self, path: String) -> Self {
        self.local_source = Some(path);
        self
    }
}

impl Default for JavaExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LanguageExecutor for JavaExecutor {
    async fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv> {
        info!(
            "Setting up Java environment with {} dependencies",
            deps.len()
        );

        if !is_tool_available("javac", "-version").await {
            bail!("javac is not installed or not in PATH (install a JDK)");
        }

        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        debug!("Created temp directory: {}", temp_dir.path().display());

        // Isolate Maven local repo inside temp dir
        let m2_repo = temp_dir.path().join(MAVEN_REPO_DIR);
        fs::create_dir_all(&m2_repo).context("Failed to create Maven repo dir")?;

        // Validate deps unconditionally (catch bad names even without mvn)
        if !deps.is_empty() {
            for dep in deps {
                sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
            }
        }

        // When local-install is active, exclude the target package's coordinate
        // from Maven fetch to avoid classpath collisions with the local jar.
        // Reuse JavaHandler which handles parent POMs, settings.gradle, etc.
        let local_artifact_id = self.local_source.as_ref().and_then(|source| {
            let handler = crate::ecosystems::java::JavaHandler::new(std::path::Path::new(source));
            handler.get_package_name().ok()
        });

        // Filter out the local package's coordinate from Maven deps.
        // Only match on artifactId (parts[1]) to avoid over-filtering when
        // JavaHandler falls back to group name for Gradle-only repos.
        // For Gradle repos without settings.gradle, the group fallback won't
        // match any artifactId, so nothing is excluded — same as pre-fix behavior.
        let fetch_deps: Vec<String> = if let Some(ref local_id) = local_artifact_id {
            deps.iter()
                .filter(|d| {
                    // Maven coordinate format: groupId:artifactId:version
                    let parts: Vec<&str> = d.split(':').collect();
                    let artifact_id = parts.get(1).unwrap_or(&"");
                    artifact_id != local_id
                })
                .map(|s| s.to_string())
                .collect()
        } else {
            deps.to_vec()
        };

        // Precompute POM before probing mvn — versionless coords are filtered here,
        // so we skip the mvn check entirely when there's nothing fetchable.
        let pom = if fetch_deps.is_empty() {
            None
        } else {
            crate::util::build_maven_pom_xml(&fetch_deps)
        };
        if pom.is_some() {
            let has_mvn = is_tool_available("mvn", "--version").await;
            if !has_mvn {
                warn!(
                    "Maven (mvn) not installed — {} Java {} cannot be downloaded. \
                     Tests may fail with missing classes.",
                    deps.len(),
                    if deps.len() == 1 {
                        "dependency"
                    } else {
                        "dependencies"
                    }
                );
            }
            if let Some(pom) = pom.filter(|_| has_mvn) {
                fs::write(temp_dir.path().join("pom.xml"), &pom)
                    .context("Failed to write pom.xml")?;

                info!("Fetching Java dependencies with Maven...");
                let deps_dir = temp_dir.path().join("deps");
                fs::create_dir_all(&deps_dir)?;

                let mut mvn_cmd = Command::new("mvn");
                mvn_cmd
                    .args([
                        "dependency:copy-dependencies",
                        &format!("-DoutputDirectory={}", deps_dir.display()),
                        &format!("-Dmaven.repo.local={}", m2_repo.display()),
                        "-q",
                    ])
                    .current_dir(temp_dir.path());

                match run_cmd_with_timeout(mvn_cmd, Duration::from_secs(self.timeout_secs)).await {
                    Ok(mvn_output) => {
                        if !mvn_output.status.success() {
                            let stderr = String::from_utf8_lossy(&mvn_output.stderr);
                            warn!("mvn dependency:copy-dependencies failed: {}", stderr);
                        }
                    }
                    Err(e) => {
                        // Timeout or other error — warn and continue without deps.
                        // Cold Maven caches can exceed timeout; javac will fail later
                        // if the deps are actually needed.
                        warn!("Maven dependency fetch failed: {e}");
                    }
                }
            }
        } else if !deps.is_empty() {
            warn!(
                "No fetchable Maven coordinates in {} {} — no jars will be downloaded. \
                 Check that deps have group:artifact:version format.",
                deps.len(),
                if deps.len() == 1 {
                    "dependency"
                } else {
                    "dependencies"
                }
            );
        }

        // Local-install: copy jars from source build output into deps/
        // Supports both Maven (target/) and Gradle (build/libs/) layouts.
        if let Some(ref source) = self.local_source {
            let source_path = std::path::Path::new(source);
            let maven_dir = source_path.join("target");
            let gradle_dir = source_path.join("build").join("libs");
            let jar_dir = if maven_dir.is_dir() {
                Some(maven_dir)
            } else if gradle_dir.is_dir() {
                Some(gradle_dir)
            } else {
                None
            };
            let deps_dir = temp_dir.path().join("deps");
            fs::create_dir_all(&deps_dir)?;
            if let Some(jar_dir) = jar_dir {
                for entry in fs::read_dir(&jar_dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("jar") {
                        let dest = deps_dir.join(entry.file_name());
                        fs::copy(&path, &dest)?;
                        info!("Copied local jar: {}", entry.file_name().to_string_lossy());
                    }
                }
            } else {
                warn!(
                    "Local source {}/target/ and {}/build/libs/ not found — \
                     run `mvn package` or `gradle build` first",
                    source, source
                );
            }
        }

        info!("Java environment setup complete");

        Ok(ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: deps.to_vec(),
        })
    }

    async fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult> {
        debug!("Running Java code ({} bytes)", code.len());

        let script_path = env.temp_dir.path().join("Main.java");
        fs::write(&script_path, code).context("Failed to write Main.java")?;

        // Note: timeout applies independently to javac AND java — total can be 2× timeout_secs
        let timeout = Duration::from_secs(self.timeout_secs);

        // Build classpath: include deps/ if it exists (relative — current_dir is temp_dir)
        let deps_dir = env.temp_dir.path().join("deps");
        let sep = if cfg!(target_os = "windows") {
            ";"
        } else {
            ":"
        };
        let classpath = if deps_dir.is_dir() {
            format!("deps/*{sep}.")
        } else {
            ".".to_string()
        };

        // Compile
        let mut javac_cmd = Command::new("javac");
        javac_cmd
            .args(["-cp", &classpath, "Main.java"])
            .current_dir(env.temp_dir.path());

        let compile_result = run_cmd_with_timeout(javac_cmd, timeout).await;
        match compile_result {
            Ok(output) if !output.status.success() => {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                debug!("Java compilation failed");
                return Ok(ExecutionResult::Fail(stderr));
            }
            Err(e) => {
                if crate::error::SkillDoError::is_timeout(&e) {
                    return Ok(ExecutionResult::Timeout);
                }
                return Err(e);
            }
            _ => {}
        }

        // Run
        let mut java_cmd = Command::new("java");
        java_cmd
            .args(["-cp", &classpath, "Main"])
            .current_dir(env.temp_dir.path());

        let result = run_cmd_with_timeout(java_cmd, timeout).await;
        classify_result(result, self.timeout_secs, "Java", stdout_and_stderr)
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
            "Test execution timed out"
        );
    }

    #[test]
    fn test_executor_defaults_and_timeouts() {
        // Python/Go/Node default to 60s, Cargo/Java defaults to 120s (compilation takes longer)
        assert_eq!(PythonUvExecutor::default().timeout_secs, 60);
        assert_eq!(GoExecutor::default().timeout_secs, 60);
        assert_eq!(NodeExecutor::default().timeout_secs, 60);
        assert_eq!(CargoExecutor::default().timeout_secs, 120);
        assert_eq!(JavaExecutor::default().timeout_secs, 120);

        assert_eq!(PythonUvExecutor::new().with_timeout(30).timeout_secs, 30);
        assert_eq!(GoExecutor::new().with_timeout(120).timeout_secs, 120);
        assert_eq!(NodeExecutor::new().with_timeout(0).timeout_secs, 0);
        assert_eq!(CargoExecutor::new().with_timeout(90).timeout_secs, 90);
        assert_eq!(JavaExecutor::new().with_timeout(90).timeout_secs, 90);
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
        assert!(JavaExecutor::new().cleanup(&make_env()).await.is_ok());
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
        assert_eq!(cloned.error_message(), "Test execution timed out");
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

    #[tokio::test]
    async fn test_cargo_setup_rejects_quoted_cargo_toml_snippet() {
        if !is_tool_available("cargo", "--version").await {
            return;
        }
        let executor = CargoExecutor::new();
        // This is a Cargo.toml snippet with spaces/quotes, not a bare crate name
        let deps = vec!["once_cell = \"1\"".to_string()];
        let result = executor.setup_environment(&deps).await;
        assert!(
            result.is_err(),
            "quoted Cargo.toml snippets should be rejected by sanitize_dep_name"
        );
    }

    // --- JavaExecutor tests ---

    #[tokio::test]
    async fn test_java_setup_environment_no_deps() {
        if !is_tool_available("javac", "-version").await {
            return; // Skip if JDK not installed
        }
        let executor = JavaExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        assert!(env.temp_dir.path().exists());
        let m2 = env.temp_dir.path().join(MAVEN_REPO_DIR);
        assert!(m2.exists(), "Maven repo dir should be created");
    }

    #[tokio::test]
    async fn test_java_run_simple_code() {
        if !is_tool_available("javac", "-version").await {
            return;
        }
        let executor = JavaExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"public class Main {
    public static void main(String[] args) {
        System.out.println("Hello from Java test");
    }
}
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_pass());
        if let ExecutionResult::Pass(output) = result {
            assert!(output.contains("Hello from Java test"));
        }
    }

    #[tokio::test]
    async fn test_java_run_failing_code() {
        if !is_tool_available("javac", "-version").await {
            return;
        }
        let executor = JavaExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"public class Main {
    public static void main(String[] args) {
        System.exit(1);
    }
}
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_fail());
    }

    #[tokio::test]
    async fn test_java_run_compilation_failure() {
        if !is_tool_available("javac", "-version").await {
            return;
        }
        let executor = JavaExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        let code = r#"public class Main {
    public static void main(String[] args) {
        this is not valid java
    }
}
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_fail());
    }

    #[tokio::test]
    async fn test_java_setup_environment_rejects_bad_deps() {
        if !is_tool_available("javac", "-version").await {
            return;
        }
        let executor = JavaExecutor::new();
        // sanitize_dep_name runs unconditionally — bad dep names are always rejected
        let result = executor
            .setup_environment(&["valid-pkg; rm -rf /".to_string()])
            .await;
        assert!(
            result.is_err(),
            "bad dep name should be rejected regardless of mvn availability"
        );
    }

    #[tokio::test]
    async fn test_java_setup_with_maven_deps_creates_pom() {
        if !is_tool_available("javac", "-version").await {
            return;
        }
        if !is_tool_available("mvn", "--version").await {
            return; // Skip if mvn not installed
        }
        let executor = JavaExecutor::new();
        let deps = vec!["com.google.code.gson:gson:2.10.1".to_string()];
        let env = executor.setup_environment(&deps).await.unwrap();

        // Verify pom.xml was created
        let pom_path = env.temp_dir.path().join("pom.xml");
        assert!(
            pom_path.exists(),
            "pom.xml should be created for Maven deps"
        );
        let pom_content = std::fs::read_to_string(&pom_path).unwrap();
        assert!(
            pom_content.contains("com.google.code.gson"),
            "pom.xml should contain groupId"
        );
        assert!(
            pom_content.contains("gson"),
            "pom.xml should contain artifactId"
        );
    }

    #[tokio::test]
    async fn test_java_setup_with_two_part_maven_coord() {
        if !is_tool_available("javac", "-version").await {
            return;
        }
        if !is_tool_available("mvn", "--version").await {
            return;
        }
        let executor = JavaExecutor::new();
        // Two-part coordinate (group:artifact, no version) — should be skipped with warning
        let deps = vec!["com.google.code.gson:gson".to_string()];
        let env = executor.setup_environment(&deps).await.unwrap();
        // No pom.xml should be created since the only dep was skipped (no version)
        let pom_path = env.temp_dir.path().join("pom.xml");
        assert!(
            !pom_path.exists(),
            "pom.xml should not be created when all deps are versionless"
        );
    }

    #[tokio::test]
    async fn test_java_setup_no_deps_no_mvn_needed() {
        // Even without mvn, Java setup with no deps should work (only needs javac)
        if !is_tool_available("javac", "-version").await {
            return;
        }
        let executor = JavaExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();
        let m2 = env.temp_dir.path().join(MAVEN_REPO_DIR);
        assert!(
            m2.exists(),
            "Maven repo dir should be created even with no deps"
        );
        // No pom.xml should be created when there are no deps
        assert!(
            !env.temp_dir.path().join("pom.xml").exists(),
            "pom.xml should not be created with no deps"
        );
    }

    #[tokio::test]
    async fn test_java_run_code_with_deps_dir() {
        // Test that run_code includes deps/ in classpath when the dir exists
        if !is_tool_available("javac", "-version").await {
            return;
        }
        let executor = JavaExecutor::new();
        let env = executor.setup_environment(&[]).await.unwrap();

        // Create a deps directory to exercise the classpath branch
        std::fs::create_dir_all(env.temp_dir.path().join("deps")).unwrap();

        let code = r#"public class Main {
    public static void main(String[] args) {
        System.out.println("Test with deps dir");
    }
}
"#;

        let result = executor.run_code(&env, code).await.unwrap();
        assert!(result.is_pass());
    }

    #[tokio::test]
    async fn test_java_setup_skips_non_maven_deps() {
        // Deps without ":" (not Maven coordinates) are filtered by build_maven_pom_xml
        // before Maven is ever invoked — no mvn guard needed.
        if !is_tool_available("javac", "-version").await {
            return;
        }
        let executor = JavaExecutor::new();
        // Single-part dep (no colon) won't match Maven coordinate format
        let deps = vec!["simplepackage".to_string()];
        let env = executor.setup_environment(&deps).await.unwrap();
        // pom.xml should NOT be created because deps_xml is empty
        assert!(
            !env.temp_dir.path().join("pom.xml").exists(),
            "pom.xml should not be created for non-Maven deps"
        );
    }
}
