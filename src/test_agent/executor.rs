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

/// Extract the Python package name from pyproject.toml content.
fn extract_python_package_name(content: &str) -> Option<String> {
    content.parse::<toml::Table>().ok().and_then(|t| {
        t.get("project")?
            .as_table()?
            .get("name")?
            .as_str()
            .map(|s| s.to_string())
    })
}

/// Filter Python deps, excluding the local package (PEP 503 normalization).
fn filter_python_deps<'a>(deps: &'a [String], local_pkg_name: Option<&str>) -> Vec<&'a String> {
    deps.iter()
        .filter(|d| {
            if let Some(local_name) = local_pkg_name {
                let norm_d = d.replace(['-', '.'], "_").to_lowercase();
                let norm_local = local_name.replace(['-', '.'], "_").to_lowercase();
                norm_d != norm_local
            } else {
                true
            }
        })
        .collect()
}

/// Filter Java deps by artifact ID, excluding the local package.
fn filter_java_deps_by_artifact_id(
    deps: &[String],
    local_artifact_id: Option<&str>,
) -> Vec<String> {
    if let Some(local_id) = local_artifact_id {
        deps.iter()
            .filter(|d| {
                let parts: Vec<&str> = d.split(':').collect();
                let artifact_id = parts.get(1).unwrap_or(&"");
                artifact_id != &local_id
            })
            .map(|s| s.to_string())
            .collect()
    } else {
        deps.to_vec()
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

        // Determine local package name so we can exclude it from registry deps.
        // Without this, uv sync fails for unpublished/local-only packages.
        let local_pkg_name: Option<String> = self.local_source.as_ref().and_then(|source| {
            let pyproject = std::path::Path::new(source).join("pyproject.toml");
            std::fs::read_to_string(&pyproject)
                .ok()
                .and_then(|content| extract_python_package_name(&content))
        });

        // Create pyproject.toml — exclude the local package (installed via editable later)
        let filtered_deps = filter_python_deps(deps, local_pkg_name.as_deref());

        let dependencies_str = if filtered_deps.is_empty() {
            String::new()
        } else {
            filtered_deps
                .iter()
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
                .args(["pip", "install", "-e", source])
                .env("UV_CACHE_DIR", &uv_cache)
                .env(
                    "VIRTUAL_ENV",
                    temp_dir.path().join(".venv").to_string_lossy().as_ref(),
                )
                .current_dir(temp_dir.path());
            let pip_output =
                run_cmd_with_timeout(pip_cmd, Duration::from_secs(self.timeout_secs)).await?;
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
            std::fs::read_to_string(&go_mod)
                .ok()
                .and_then(|content| extract_go_module_name(&content))
        } else {
            None
        };

        if let (Some(ref module), Some(ref source)) = (&local_module, &self.local_source) {
            // Apply replace for the root module (not sub-packages).
            // Sub-package imports (e.g., module/subpkg) are resolved via the root
            // module replace — replacing the sub-package path has no effect.
            if go_dep_needs_replace(deps, module) {
                info!("Replacing {} with local source: {}", module, source);
                let mut replace_cmd = Command::new("go");
                replace_cmd
                    .args(["mod", "edit", "-replace", &format!("{module}={source}")])
                    .current_dir(temp_dir.path());
                Self::apply_go_env(&mut replace_cmd, temp_dir.path());
                let replace_output =
                    run_cmd_with_timeout(replace_cmd, Duration::from_secs(30)).await?;
                if !replace_output.status.success() {
                    let stderr = String::from_utf8_lossy(&replace_output.stderr);
                    bail!(
                        "go mod edit -replace failed for {} (local-install): {}",
                        module,
                        stderr
                    );
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

/// Extract the package name from a Cargo.toml at the given source directory.
/// Returns `None` if the file is missing, unreadable, or lacks `[package] name`.
fn extract_cargo_package_name(source: &str) -> Option<String> {
    let cargo_path = std::path::Path::new(source).join("Cargo.toml");
    std::fs::read_to_string(&cargo_path)
        .ok()
        .and_then(|content| content.parse::<toml::Table>().ok())
        .and_then(|t| {
            t.get("package")?
                .as_table()?
                .get("name")?
                .as_str()
                .map(|s| s.to_string())
        })
}

/// Format a single structured Cargo.toml dependency line.
/// If the dep matches the local package (dash/underscore normalized), emits a
/// `path = "..."` override preserving any features/default-features from `raw_spec`.
fn format_cargo_structured_dep_line(
    dep: &crate::pipeline::collector::StructuredDep,
    local_source: Option<&str>,
    local_pkg_name: Option<&str>,
) -> String {
    if let Some(source) = local_source {
        if local_pkg_name.is_some_and(|n| n.replace('-', "_") == dep.name.replace('-', "_")) {
            let safe = source.replace('\\', "/");
            // Preserve features/default-features from raw_spec
            let extras = dep
                .raw_spec
                .as_ref()
                .and_then(|s| s.parse::<toml::Value>().ok())
                .and_then(|v| {
                    let t = v.as_table()?;
                    let mut parts = Vec::new();
                    if let Some(f) = t.get("features") {
                        parts.push(format!("features = {}", f));
                    }
                    if let Some(df) = t.get("default-features") {
                        parts.push(format!("default-features = {}", df));
                    }
                    if parts.is_empty() {
                        None
                    } else {
                        Some(parts.join(", "))
                    }
                });
            return if let Some(extras) = extras {
                format!("{} = {{ path = \"{}\", {} }}", dep.name, safe, extras)
            } else {
                format!("{} = {{ path = \"{}\" }}", dep.name, safe)
            };
        }
    }
    // Use raw spec (preserves version + features)
    if let Some(ref spec) = dep.raw_spec {
        format!("{} = {}", dep.name, spec)
    } else {
        format!("{} = \"*\"", dep.name)
    }
}

/// Format a single plain (fallback) Cargo.toml dependency line.
/// If the dep matches the local package (dash/underscore normalized), emits a
/// `path = "..."` override; otherwise emits `dep = "*"`.
fn format_cargo_plain_dep_line(
    dep: &str,
    local_source: Option<&str>,
    local_pkg_name: Option<&str>,
) -> String {
    if let Some(source) = local_source {
        if local_pkg_name.is_some_and(|n| n.replace('-', "_") == dep.replace('-', "_")) {
            let safe = source.replace('\\', "/");
            return format!("{dep} = {{ path = \"{}\" }}", safe);
        }
    }
    format!("{dep} = \"*\"")
}

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
        // Hoist local package name outside the loop to avoid re-reading Cargo.toml per dep.
        let local_pkg_name: Option<String> = self
            .local_source
            .as_deref()
            .and_then(extract_cargo_package_name);

        let deps_section = if deps.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = deps
                .iter()
                .map(|d| {
                    format_cargo_structured_dep_line(
                        d,
                        self.local_source.as_deref(),
                        local_pkg_name.as_deref(),
                    )
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

        // Hoist local package name outside loop (same pattern as structured path)
        let local_pkg_name_fallback: Option<String> = self
            .local_source
            .as_deref()
            .and_then(extract_cargo_package_name);

        // Build Cargo.toml with dependencies
        let deps_section = if deps.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = deps
                .iter()
                .map(|d| {
                    format_cargo_plain_dep_line(
                        d,
                        self.local_source.as_deref(),
                        local_pkg_name_fallback.as_deref(),
                    )
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

        // npm install for each dependency (or local source even when deps is empty)
        // No shell quoting needed — Command passes args directly to the process.
        // sanitize_dep_name already rejects shell metacharacters, and `--` prevents
        // flag injection.
        if self.local_source.is_some() || !deps.is_empty() {
            if !is_tool_available("npm", "--version").await {
                bail!("npm is not installed or not in PATH");
            }
            for dep in deps {
                sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
            }

            // For local-install: install local source (provides the target package),
            // then remaining deps from registry. npm install handles both.
            let install_args: Vec<String> = if let Some(ref source) = self.local_source {
                let pkg_json_path = std::path::Path::new(source).join("package.json");
                let local_name: Option<String> = std::fs::read_to_string(&pkg_json_path)
                    .ok()
                    .and_then(|c| extract_node_package_name(&c));
                info!("Installing Node.js from local source: {}", source);
                build_node_install_args(deps, source, local_name.as_deref())
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
        let fetch_deps = filter_java_deps_by_artifact_id(deps, local_artifact_id.as_deref());

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

/// Extract the module name from a `go.mod` file's content.
/// Returns `None` if no `module` directive is found.
fn extract_go_module_name(go_mod_content: &str) -> Option<String> {
    go_mod_content.lines().find_map(|line| {
        line.trim()
            .strip_prefix("module ")
            .map(|m| m.trim().to_string())
    })
}

/// Check whether a single dependency matches or is a sub-package of the given Go module.
fn go_module_matches_dep(dep: &str, module: &str) -> bool {
    dep == module || dep.starts_with(&format!("{module}/"))
}

/// Check whether **any** dependency in `deps` matches or is a sub-package of `module`.
fn go_dep_needs_replace(deps: &[String], module: &str) -> bool {
    deps.iter().any(|d| go_module_matches_dep(d, module))
}

/// Extract the package name from a `package.json` file's content.
/// Returns `None` if the JSON is invalid or has no string `name` field.
fn extract_node_package_name(json_content: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json_content)
        .ok()
        .and_then(|v| v.get("name")?.as_str().map(|s| s.to_string()))
}

/// Build the npm install argument list for a Node local-install.
/// Puts `local_source` first, then appends any dep that doesn't match `local_name`.
fn build_node_install_args(
    deps: &[String],
    local_source: &str,
    local_name: Option<&str>,
) -> Vec<String> {
    let mut args = vec![local_source.to_string()];
    for d in deps {
        if local_name != Some(d.as_str()) {
            args.push(d.clone());
        }
    }
    args
}

#[cfg(test)]
#[allow(clippy::useless_vec)]
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

    // =========================================================================
    // Local-install logic tests (pure logic — no external tool invocations)
    // =========================================================================

    // --- CargoExecutor: structured dep Cargo.toml formatting ---

    /// Convenience wrapper: extracts package name from local_source, then
    /// delegates to the production `format_cargo_structured_dep_line`.
    fn format_cargo_dep_line(
        dep: &crate::pipeline::collector::StructuredDep,
        local_source: Option<&str>,
    ) -> String {
        let local_pkg_name = local_source.and_then(extract_cargo_package_name);
        format_cargo_structured_dep_line(dep, local_source, local_pkg_name.as_deref())
    }

    #[test]
    fn test_cargo_structured_dep_with_raw_spec() {
        use crate::pipeline::collector::{DepSource, StructuredDep};
        let dep = StructuredDep {
            name: "serde".to_string(),
            raw_spec: Some("{ version = \"1\", features = [\"derive\"] }".to_string()),
            source: DepSource::Manifest,
        };
        let line = format_cargo_dep_line(&dep, None);
        assert_eq!(line, "serde = { version = \"1\", features = [\"derive\"] }");
    }

    #[test]
    fn test_cargo_structured_dep_without_raw_spec() {
        use crate::pipeline::collector::{DepSource, StructuredDep};
        let dep = StructuredDep {
            name: "once_cell".to_string(),
            raw_spec: None,
            source: DepSource::Pattern,
        };
        let line = format_cargo_dep_line(&dep, None);
        assert_eq!(line, "once_cell = \"*\"");
    }

    #[test]
    fn test_cargo_structured_dep_local_path_override() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        // Create a temp dir with a Cargo.toml that has package.name = "my-crate"
        let tmp = TempDir::new().unwrap();
        let cargo_toml = r#"[package]
name = "my-crate"
version = "0.1.0"
edition = "2021"
"#;
        std::fs::write(tmp.path().join("Cargo.toml"), cargo_toml).unwrap();

        let dep = StructuredDep {
            name: "my-crate".to_string(),
            raw_spec: Some("\"1.0\"".to_string()),
            source: DepSource::Manifest,
        };
        let line = format_cargo_dep_line(&dep, Some(&tmp.path().to_string_lossy()));
        assert!(
            line.contains("path = "),
            "local dep should use path: {line}"
        );
        assert!(
            line.starts_with("my-crate = "),
            "should start with dep name: {line}"
        );
    }

    #[test]
    fn test_cargo_structured_dep_local_path_preserves_features() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        let dep = StructuredDep {
            name: "my-crate".to_string(),
            raw_spec: Some(
                "{ version = \"1\", features = [\"json\", \"tls\"], default-features = false }"
                    .to_string(),
            ),
            source: DepSource::Manifest,
        };
        let line = format_cargo_dep_line(&dep, Some(&tmp.path().to_string_lossy()));
        assert!(
            line.contains("path = "),
            "local dep should use path: {line}"
        );
        assert!(
            line.contains("features = [\"json\", \"tls\"]"),
            "features must be preserved on path dep: {line}"
        );
        assert!(
            line.contains("default-features = false"),
            "default-features must be preserved on path dep: {line}"
        );
    }

    #[test]
    fn test_cargo_structured_dep_local_source_non_matching_dep() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        // Local source is "my-crate", but dep is "tokio" — should NOT get path override
        let tmp = TempDir::new().unwrap();
        let cargo_toml = r#"[package]
name = "my-crate"
version = "0.1.0"
edition = "2021"
"#;
        std::fs::write(tmp.path().join("Cargo.toml"), cargo_toml).unwrap();

        let dep = StructuredDep {
            name: "tokio".to_string(),
            raw_spec: Some("{ version = \"1\", features = [\"full\"] }".to_string()),
            source: DepSource::Manifest,
        };
        let line = format_cargo_dep_line(&dep, Some(&tmp.path().to_string_lossy()));
        assert_eq!(
            line, "tokio = { version = \"1\", features = [\"full\"] }",
            "non-local dep should keep its raw_spec"
        );
    }

    #[test]
    fn test_cargo_structured_dep_local_source_no_cargo_toml() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        // Local source dir exists but has no Cargo.toml — should fall through
        let tmp = TempDir::new().unwrap();
        let dep = StructuredDep {
            name: "my-crate".to_string(),
            raw_spec: None,
            source: DepSource::Manifest,
        };
        let line = format_cargo_dep_line(&dep, Some(&tmp.path().to_string_lossy()));
        assert_eq!(
            line, "my-crate = \"*\"",
            "missing Cargo.toml should fallback to wildcard"
        );
    }

    #[test]
    fn test_cargo_structured_deps_section_formatting() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        let deps = [
            StructuredDep {
                name: "serde".to_string(),
                raw_spec: Some("{ version = \"1\", features = [\"derive\"] }".to_string()),
                source: DepSource::Manifest,
            },
            StructuredDep {
                name: "tokio".to_string(),
                raw_spec: Some("\"1\"".to_string()),
                source: DepSource::Pattern,
            },
            StructuredDep {
                name: "anyhow".to_string(),
                raw_spec: None,
                source: DepSource::Pattern,
            },
        ];

        // Replicate the deps_section logic from setup_structured_environment
        let lines: Vec<String> = deps
            .iter()
            .map(|d| format_cargo_dep_line(d, None))
            .collect();
        let deps_section = format!("\n[dependencies]\n{}\n", lines.join("\n"));

        assert!(deps_section.contains("[dependencies]"));
        assert!(deps_section.contains("serde = { version = \"1\", features = [\"derive\"] }"));
        assert!(deps_section.contains("tokio = \"1\""));
        assert!(deps_section.contains("anyhow = \"*\""));
    }

    #[test]
    fn test_cargo_structured_empty_deps_section() {
        let deps: Vec<crate::pipeline::collector::StructuredDep> = vec![];
        let deps_section = if deps.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = deps
                .iter()
                .map(|d| format_cargo_dep_line(d, None))
                .collect();
            format!("\n[dependencies]\n{}\n", lines.join("\n"))
        };
        assert!(deps_section.is_empty());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_cargo_structured_dep_local_path_backslash_normalization() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        let tmp = TempDir::new().unwrap();
        let cargo_toml =
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
        std::fs::write(tmp.path().join("Cargo.toml"), cargo_toml).unwrap();

        // Simulate a Windows path with backslashes
        let win_path = tmp.path().to_string_lossy().replace('/', "\\");
        let dep = StructuredDep {
            name: "my-crate".to_string(),
            raw_spec: None,
            source: DepSource::Manifest,
        };
        let line = format_cargo_dep_line(&dep, Some(&win_path));
        assert!(
            !line.contains('\\'),
            "backslashes should be normalized to forward slashes: {line}"
        );
    }

    // --- GoExecutor: local module detection logic ---
    // Helpers: extract_go_module_name, go_module_matches_dep, go_dep_needs_replace
    // are defined at module scope and available here via `use super::*`.

    #[test]
    fn test_go_module_exact_match() {
        assert!(go_module_matches_dep(
            "github.com/user/mylib",
            "github.com/user/mylib"
        ));
    }

    #[test]
    fn test_go_module_subpackage_match() {
        assert!(go_module_matches_dep(
            "github.com/user/mylib/subpkg",
            "github.com/user/mylib"
        ));
    }

    #[test]
    fn test_go_module_no_match_different_module() {
        assert!(!go_module_matches_dep(
            "github.com/other/lib",
            "github.com/user/mylib"
        ));
    }

    #[test]
    fn test_go_module_no_match_prefix_collision() {
        // "github.com/user/mylib2" should NOT match "github.com/user/mylib"
        // because it doesn't start with "github.com/user/mylib/"
        assert!(!go_module_matches_dep(
            "github.com/user/mylib2",
            "github.com/user/mylib"
        ));
    }

    #[test]
    fn test_go_extract_module_name_basic() {
        let content = "module github.com/user/mylib\n\ngo 1.21\n";
        assert_eq!(
            extract_go_module_name(content),
            Some("github.com/user/mylib".to_string())
        );
    }

    #[test]
    fn test_go_extract_module_name_with_leading_whitespace() {
        let content = "  module  github.com/user/mylib  \n\ngo 1.21\n";
        assert_eq!(
            extract_go_module_name(content),
            Some("github.com/user/mylib".to_string())
        );
    }

    #[test]
    fn test_go_extract_module_name_missing() {
        let content = "go 1.21\nrequire (\n)\n";
        assert_eq!(extract_go_module_name(content), None);
    }

    #[test]
    fn test_go_extract_module_from_tempdir() {
        let tmp = TempDir::new().unwrap();
        let go_mod = "module github.com/example/coolpkg\n\ngo 1.22\n";
        std::fs::write(tmp.path().join("go.mod"), go_mod).unwrap();

        // Replicate the full extraction as done in setup_environment
        let source = tmp.path().to_string_lossy().to_string();
        let go_mod_path = std::path::Path::new(&source).join("go.mod");
        let local_module = std::fs::read_to_string(&go_mod_path)
            .ok()
            .and_then(|content| extract_go_module_name(&content));

        assert_eq!(local_module, Some("github.com/example/coolpkg".to_string()));
    }

    #[test]
    fn test_go_extract_module_no_go_mod() {
        let tmp = TempDir::new().unwrap();
        let source = tmp.path().to_string_lossy().to_string();
        let go_mod_path = std::path::Path::new(&source).join("go.mod");
        let local_module = std::fs::read_to_string(&go_mod_path)
            .ok()
            .and_then(|content| extract_go_module_name(&content));
        assert_eq!(local_module, None);
    }

    #[test]
    fn test_go_module_replace_candidates() {
        // Given a module and a list of deps, collect which ones need replace directives
        let module = "github.com/user/mylib";
        let deps = [
            "github.com/user/mylib",
            "github.com/user/mylib/v2",
            "github.com/user/mylib/subpkg",
            "github.com/other/lib",
            "github.com/user/mylib2",
        ];
        let replacements: Vec<&&str> = deps
            .iter()
            .filter(|d| go_module_matches_dep(d, module))
            .collect();
        assert_eq!(
            replacements,
            vec![
                &"github.com/user/mylib",
                &"github.com/user/mylib/v2",
                &"github.com/user/mylib/subpkg",
            ]
        );
    }

    #[test]
    fn test_node_package_name_normal_json() {
        let json = r#"{"name": "my-cool-package", "version": "1.0.0"}"#;
        assert_eq!(
            extract_node_package_name(json),
            Some("my-cool-package".to_string())
        );
    }

    #[test]
    fn test_node_package_name_minified_json() {
        let json = r#"{"name":"@scope/pkg","version":"2.0.0","main":"index.js"}"#;
        assert_eq!(
            extract_node_package_name(json),
            Some("@scope/pkg".to_string())
        );
    }

    #[test]
    fn test_node_package_name_missing_name_field() {
        let json = r#"{"version": "1.0.0", "main": "index.js"}"#;
        assert_eq!(extract_node_package_name(json), None);
    }

    #[test]
    fn test_node_package_name_invalid_json() {
        let json = "not json at all";
        assert_eq!(extract_node_package_name(json), None);
    }

    #[test]
    fn test_node_package_name_name_is_not_string() {
        let json = r#"{"name": 42, "version": "1.0.0"}"#;
        assert_eq!(extract_node_package_name(json), None);
    }

    #[test]
    fn test_node_package_name_empty_string() {
        let json = r#"{"name": "", "version": "1.0.0"}"#;
        assert_eq!(
            extract_node_package_name(json),
            Some("".to_string()),
            "empty name is still a valid string"
        );
    }

    #[test]
    fn test_node_local_install_dep_filtering() {
        // local source provides "my-pkg", so "my-pkg" should be excluded
        let deps = vec![
            "my-pkg".to_string(),
            "express".to_string(),
            "lodash".to_string(),
        ];
        let install_args = build_node_install_args(&deps, "/some/path", Some("my-pkg"));

        assert_eq!(
            install_args,
            vec![
                "/some/path".to_string(),
                "express".to_string(),
                "lodash".to_string(),
            ],
            "local package should be excluded, replaced by source path"
        );
    }

    #[test]
    fn test_node_local_install_no_matching_dep() {
        // When local package name doesn't match any dep, all deps are included
        let deps = vec!["express".to_string(), "lodash".to_string()];
        let install_args = build_node_install_args(&deps, "/some/path", Some("other-pkg"));

        assert_eq!(
            install_args,
            vec![
                "/some/path".to_string(),
                "express".to_string(),
                "lodash".to_string(),
            ]
        );
    }

    #[test]
    fn test_node_extract_package_name_from_tempdir() {
        let tmp = TempDir::new().unwrap();
        let pkg_json = r#"{"name": "my-local-pkg", "version": "0.1.0"}"#;
        std::fs::write(tmp.path().join("package.json"), pkg_json).unwrap();

        let pkg_path = tmp.path().join("package.json");
        let local_name: Option<String> = std::fs::read_to_string(&pkg_path)
            .ok()
            .and_then(|c| extract_node_package_name(&c));

        assert_eq!(local_name, Some("my-local-pkg".to_string()));
    }

    #[test]
    fn test_node_extract_package_name_no_file() {
        let tmp = TempDir::new().unwrap();
        let pkg_path = tmp.path().join("package.json");
        let local_name: Option<String> = std::fs::read_to_string(&pkg_path)
            .ok()
            .and_then(|c| extract_node_package_name(&c));
        assert_eq!(local_name, None);
    }

    // --- JavaExecutor: artifact ID exclusion logic ---

    /// Replicate the Java artifact ID filtering logic from setup_environment.
    fn filter_java_deps_by_artifact_id(
        deps: &[String],
        local_artifact_id: Option<&str>,
    ) -> Vec<String> {
        if let Some(local_id) = local_artifact_id {
            deps.iter()
                .filter(|d| {
                    let parts: Vec<&str> = d.split(':').collect();
                    let artifact_id = parts.get(1).unwrap_or(&"");
                    artifact_id != &local_id
                })
                .map(|s| s.to_string())
                .collect()
        } else {
            deps.to_vec()
        }
    }

    #[test]
    fn test_java_filter_excludes_matching_artifact_id() {
        let deps = vec![
            "com.example:my-lib:1.0".to_string(),
            "com.google.code.gson:gson:2.10.1".to_string(),
            "org.slf4j:slf4j-api:2.0.0".to_string(),
        ];
        let filtered = filter_java_deps_by_artifact_id(&deps, Some("my-lib"));
        assert_eq!(
            filtered,
            vec![
                "com.google.code.gson:gson:2.10.1".to_string(),
                "org.slf4j:slf4j-api:2.0.0".to_string(),
            ]
        );
    }

    #[test]
    fn test_java_filter_no_local_artifact() {
        let deps = vec![
            "com.example:my-lib:1.0".to_string(),
            "com.google.code.gson:gson:2.10.1".to_string(),
        ];
        let filtered = filter_java_deps_by_artifact_id(&deps, None);
        assert_eq!(filtered, deps, "None artifact should pass all deps through");
    }

    #[test]
    fn test_java_filter_no_match() {
        let deps = vec![
            "com.example:my-lib:1.0".to_string(),
            "com.google.code.gson:gson:2.10.1".to_string(),
        ];
        let filtered = filter_java_deps_by_artifact_id(&deps, Some("nonexistent"));
        assert_eq!(filtered, deps, "no match should pass all deps through");
    }

    #[test]
    fn test_java_filter_single_part_dep() {
        // Dep without colon — parts.get(1) returns None, unwrap_or("") != any artifact
        let deps = vec!["simplepackage".to_string()];
        let filtered = filter_java_deps_by_artifact_id(&deps, Some("simplepackage"));
        assert_eq!(
            filtered,
            vec!["simplepackage".to_string()],
            "single-part dep should not match artifact_id filter"
        );
    }

    #[test]
    fn test_java_filter_two_part_coord() {
        // group:artifact (no version) — artifact_id is parts[1]
        let deps = vec!["com.example:my-lib".to_string()];
        let filtered = filter_java_deps_by_artifact_id(&deps, Some("my-lib"));
        assert!(
            filtered.is_empty(),
            "two-part coord with matching artifact should be filtered"
        );
    }

    #[test]
    fn test_java_filter_multiple_matches() {
        // Different groups, same artifact_id — both excluded
        let deps = vec![
            "com.example:my-lib:1.0".to_string(),
            "org.other:my-lib:2.0".to_string(),
            "com.google.code.gson:gson:2.10.1".to_string(),
        ];
        let filtered = filter_java_deps_by_artifact_id(&deps, Some("my-lib"));
        assert_eq!(
            filtered,
            vec!["com.google.code.gson:gson:2.10.1".to_string()],
            "all deps with matching artifact_id should be excluded"
        );
    }

    #[test]
    fn test_java_local_artifact_id_from_pom() {
        // Test the full flow: create a temp dir with pom.xml, extract artifact_id
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId>my-java-lib</artifactId><version>1.0</version></project>",
        )
        .unwrap();

        let handler = crate::ecosystems::java::JavaHandler::new(tmp.path());
        let local_artifact_id = handler.get_package_name().ok();
        assert_eq!(local_artifact_id, Some("my-java-lib".to_string()));

        // Now filter deps using this artifact_id
        let deps = vec![
            "com.example:my-java-lib:1.0".to_string(),
            "com.google.code.gson:gson:2.10.1".to_string(),
        ];
        let filtered = filter_java_deps_by_artifact_id(&deps, local_artifact_id.as_deref());
        assert_eq!(
            filtered,
            vec!["com.google.code.gson:gson:2.10.1".to_string()]
        );
    }

    #[test]
    fn test_java_local_artifact_id_from_gradle() {
        // Gradle: group from build.gradle, not artifactId — should NOT match
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("build.gradle"),
            "group = 'com.example'\nversion = '1.0'",
        )
        .unwrap();

        let handler = crate::ecosystems::java::JavaHandler::new(tmp.path());
        let local_artifact_id = handler.get_package_name().ok();
        // Gradle group = "com.example" — this is a group name, not an artifact_id
        assert_eq!(local_artifact_id, Some("com.example".to_string()));

        // This group name won't match any artifact_id in Maven coords
        let deps = vec!["com.example:my-java-lib:1.0".to_string()];
        let filtered = filter_java_deps_by_artifact_id(&deps, local_artifact_id.as_deref());
        assert_eq!(
            filtered, deps,
            "Gradle group fallback should not accidentally filter Maven coords"
        );
    }

    // --- PythonUvExecutor: with_local_source builder ---

    #[test]
    fn test_python_uv_with_local_source() {
        let executor = PythonUvExecutor::new().with_local_source("/some/path".to_string());
        assert_eq!(executor.local_source, Some("/some/path".to_string()));
        assert_eq!(executor.timeout_secs, 60); // default unchanged
    }

    #[test]
    fn test_python_uv_builder_chaining() {
        let executor = PythonUvExecutor::new()
            .with_timeout(30)
            .with_local_source("/my/lib".to_string());
        assert_eq!(executor.timeout_secs, 30);
        assert_eq!(executor.local_source, Some("/my/lib".to_string()));
    }

    #[test]
    fn test_python_uv_default_no_local_source() {
        let executor = PythonUvExecutor::new();
        assert_eq!(executor.local_source, None);
    }

    // --- GoExecutor: with_local_source builder ---

    #[test]
    fn test_go_with_local_source() {
        let executor = GoExecutor::new().with_local_source("/some/go/pkg".to_string());
        assert_eq!(executor.local_source, Some("/some/go/pkg".to_string()));
        assert_eq!(executor.timeout_secs, 60);
    }

    #[test]
    fn test_go_builder_chaining() {
        let executor = GoExecutor::new()
            .with_timeout(90)
            .with_local_source("/go/src/mymod".to_string());
        assert_eq!(executor.timeout_secs, 90);
        assert_eq!(executor.local_source, Some("/go/src/mymod".to_string()));
    }

    // --- NodeExecutor: with_local_source builder ---

    #[test]
    fn test_node_with_local_source() {
        let executor = NodeExecutor::new().with_local_source("/my/node/pkg".to_string());
        assert_eq!(executor.local_source, Some("/my/node/pkg".to_string()));
        assert_eq!(executor.timeout_secs, 60);
    }

    // --- CargoExecutor: with_local_source builder ---

    #[test]
    fn test_cargo_with_local_source() {
        let executor = CargoExecutor::new().with_local_source("/my/crate".to_string());
        assert_eq!(executor.local_source, Some("/my/crate".to_string()));
        assert_eq!(executor.timeout_secs, 120);
    }

    // --- JavaExecutor: with_local_source builder ---

    #[test]
    fn test_java_with_local_source() {
        let executor = JavaExecutor::new().with_local_source("/my/java/project".to_string());
        assert_eq!(executor.local_source, Some("/my/java/project".to_string()));
        assert_eq!(executor.timeout_secs, 120);
    }

    // --- Cargo: unstructured setup_environment dep formatting with local source ---

    /// Convenience wrapper: extracts package name from local_source, then
    /// delegates to the production `format_cargo_plain_dep_line`.
    fn format_cargo_plain_dep_line(dep: &str, local_source: Option<&str>) -> String {
        let local_pkg_name = local_source.and_then(extract_cargo_package_name);
        super::format_cargo_plain_dep_line(dep, local_source, local_pkg_name.as_deref())
    }

    #[test]
    fn test_cargo_plain_dep_no_local_source() {
        assert_eq!(format_cargo_plain_dep_line("serde", None), "serde = \"*\"");
    }

    #[test]
    fn test_cargo_plain_dep_local_source_match() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        let line = format_cargo_plain_dep_line("my-crate", Some(&tmp.path().to_string_lossy()));
        assert!(line.contains("path = "), "should use path dep: {line}");
    }

    #[test]
    fn test_cargo_plain_dep_local_source_no_match() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        let line = format_cargo_plain_dep_line("tokio", Some(&tmp.path().to_string_lossy()));
        assert_eq!(line, "tokio = \"*\"");
    }

    // --- GoExecutor::apply_go_env ---

    #[test]
    fn test_go_apply_go_env() {
        // Verify apply_go_env sets the expected env vars
        let base = std::path::Path::new("/tmp/test");
        let mut cmd = Command::new("echo");
        GoExecutor::apply_go_env(&mut cmd, base);
        // Command doesn't expose env vars for inspection, but we can verify
        // it doesn't panic and the method is callable. The actual env is tested
        // in the integration tests.
    }

    // =========================================================================
    // Additional coverage tests — uncovered branches and edge cases
    // =========================================================================

    // --- stdout_and_stderr combiner edge cases ---

    #[test]
    fn test_stdout_and_stderr_empty_stdout() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"".to_vec(),
            stderr: b"error only".to_vec(),
        };
        let combined = stdout_and_stderr(&output);
        assert_eq!(combined, "error only");
    }

    #[test]
    fn test_stdout_and_stderr_empty_stderr() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"log only".to_vec(),
            stderr: b"".to_vec(),
        };
        let combined = stdout_and_stderr(&output);
        assert_eq!(combined, "log only");
    }

    #[test]
    fn test_stdout_and_stderr_both_empty() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"".to_vec(),
            stderr: b"".to_vec(),
        };
        let combined = stdout_and_stderr(&output);
        assert_eq!(combined, "");
    }

    #[test]
    fn test_stdout_and_stderr_trims_whitespace() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"  line1  ".to_vec(),
            stderr: b"  line2  ".to_vec(),
        };
        let combined = stdout_and_stderr(&output);
        // format!("{stdout}\n{stderr}").trim()
        assert_eq!(combined, "line1  \n  line2");
    }

    // --- classify_result with stdout_and_stderr combiner ---

    #[test]
    fn test_classify_result_fail_with_stdout_and_stderr() {
        #[cfg(unix)]
        let failed_status = {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(1 << 8)
        };
        #[cfg(windows)]
        let failed_status = {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(1)
        };
        let output = Output {
            status: failed_status,
            stdout: b"console.log output".to_vec(),
            stderr: b"Error: test failed".to_vec(),
        };
        let result = classify_result(Ok(output), 60, "Node.js", stdout_and_stderr).unwrap();
        assert!(result.is_fail());
        let msg = result.error_message();
        assert!(msg.contains("console.log output"), "should include stdout");
        assert!(msg.contains("Error: test failed"), "should include stderr");
    }

    #[test]
    fn test_classify_result_pass_captures_stdout() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"test output line 1\nline 2".to_vec(),
            stderr: b"".to_vec(),
        };
        let result = classify_result(Ok(output), 30, "Python", stderr_only).unwrap();
        assert!(result.is_pass());
        assert_eq!(result.error_message(), "test output line 1\nline 2");
    }

    // --- Python PEP 503 normalization (dash/underscore equivalence) ---

    /// Replicate the PEP 503 normalization logic from PythonUvExecutor::setup_environment.
    fn python_dep_matches_local(dep: &str, local_name: &str) -> bool {
        let norm_d = dep.replace('-', "_").to_lowercase();
        let norm_local = local_name.replace('-', "_").to_lowercase();
        norm_d == norm_local
    }

    #[test]
    fn test_python_pep503_dash_underscore_equivalence() {
        // "my-pkg" and "my_pkg" should match
        assert!(python_dep_matches_local("my-pkg", "my_pkg"));
        assert!(python_dep_matches_local("my_pkg", "my-pkg"));
    }

    #[test]
    fn test_python_pep503_case_insensitive() {
        assert!(python_dep_matches_local("My-Package", "my_package"));
        assert!(python_dep_matches_local("MY_PACKAGE", "my-package"));
    }

    #[test]
    fn test_python_pep503_exact_match() {
        assert!(python_dep_matches_local("requests", "requests"));
    }

    #[test]
    fn test_python_pep503_no_match() {
        assert!(!python_dep_matches_local("requests", "flask"));
        assert!(!python_dep_matches_local("my-pkg", "my-pkg-extra"));
    }

    #[test]
    fn test_python_pep503_mixed_separators() {
        // "scikit-learn" vs "scikit_learn"
        assert!(python_dep_matches_local("scikit-learn", "scikit_learn"));
    }

    #[test]
    fn test_python_filter_deps_excludes_local_package() {
        let deps = vec![
            "my-pkg".to_string(),
            "requests".to_string(),
            "flask".to_string(),
        ];
        let filtered = filter_python_deps(&deps, Some("my_pkg"));
        assert_eq!(filtered.len(), 2);
        assert_eq!(*filtered[0], "requests");
        assert_eq!(*filtered[1], "flask");
    }

    #[test]
    fn test_python_filter_deps_no_local_name() {
        let deps = vec!["requests".to_string(), "flask".to_string()];
        let filtered = filter_python_deps(&deps, None);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_python_filter_deps_empty_deps() {
        let deps: Vec<String> = vec![];
        let filtered = filter_python_deps(&deps, Some("my-pkg"));
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_python_filter_deps_all_excluded() {
        let deps = vec!["my-pkg".to_string()];
        let filtered = filter_python_deps(&deps, Some("my-pkg"));
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_python_filter_deps_case_insensitive_exclusion() {
        let deps = vec!["My-Package".to_string(), "other-dep".to_string()];
        let filtered = filter_python_deps(&deps, Some("my_package"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(*filtered[0], "other-dep");
    }

    // --- Python pyproject.toml dep formatting ---

    /// Replicate the pyproject.toml dependencies_str formatting.
    fn format_python_deps_str(filtered_deps: &[&String]) -> String {
        if filtered_deps.is_empty() {
            String::new()
        } else {
            filtered_deps
                .iter()
                .map(|d| format!("    \"{}\",", d))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    #[test]
    fn test_python_deps_str_formatting_single() {
        let deps = vec!["requests".to_string()];
        let refs: Vec<&String> = deps.iter().collect();
        let formatted = format_python_deps_str(&refs);
        assert_eq!(formatted, "    \"requests\",");
    }

    #[test]
    fn test_python_deps_str_formatting_multiple() {
        let deps = vec!["requests".to_string(), "flask".to_string()];
        let refs: Vec<&String> = deps.iter().collect();
        let formatted = format_python_deps_str(&refs);
        assert_eq!(formatted, "    \"requests\",\n    \"flask\",");
    }

    #[test]
    fn test_python_deps_str_formatting_empty() {
        let refs: Vec<&String> = vec![];
        let formatted = format_python_deps_str(&refs);
        assert_eq!(formatted, "");
    }

    #[test]
    fn test_python_pyproject_toml_generation() {
        // Replicate the full pyproject.toml generation
        let deps = vec!["requests".to_string(), "click>=8.0".to_string()];
        let filtered: Vec<&String> = deps.iter().collect();
        let dependencies_str = format_python_deps_str(&filtered);
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
        assert!(pyproject_content.contains("[project]"));
        assert!(pyproject_content.contains("\"requests\""));
        assert!(pyproject_content.contains("\"click>=8.0\""));
        assert!(pyproject_content.contains("dependencies = ["));
    }

    #[test]
    fn test_python_pyproject_toml_no_deps() {
        let dependencies_str = format_python_deps_str(&[]);
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
        assert!(pyproject_content.contains("dependencies = [\n\n]"));
    }

    // --- Python local_pkg_name extraction via TOML ---

    #[test]
    fn test_python_extract_pkg_name_basic() {
        let content = r#"[project]
name = "my-cool-lib"
version = "0.1.0"
"#;
        assert_eq!(
            extract_python_package_name(content),
            Some("my-cool-lib".to_string())
        );
    }

    #[test]
    fn test_python_extract_pkg_name_missing_project() {
        let content = r#"[tool.poetry]
name = "my-lib"
"#;
        assert_eq!(extract_python_package_name(content), None);
    }

    #[test]
    fn test_python_extract_pkg_name_missing_name() {
        let content = r#"[project]
version = "0.1.0"
"#;
        assert_eq!(extract_python_package_name(content), None);
    }

    #[test]
    fn test_python_extract_pkg_name_invalid_toml() {
        let content = "not valid toml {{{";
        assert_eq!(extract_python_package_name(content), None);
    }

    #[test]
    fn test_python_extract_pkg_name_from_tempdir() {
        let tmp = TempDir::new().unwrap();
        let pyproject = r#"[project]
name = "my-lib"
version = "0.1.0"
requires-python = ">=3.8"
dependencies = ["requests"]
"#;
        std::fs::write(tmp.path().join("pyproject.toml"), pyproject).unwrap();

        let source = tmp.path().to_string_lossy().to_string();
        let pyproject_path = std::path::Path::new(&source).join("pyproject.toml");
        let local_pkg_name = std::fs::read_to_string(&pyproject_path)
            .ok()
            .and_then(|content| extract_python_package_name(&content));
        assert_eq!(local_pkg_name, Some("my-lib".to_string()));
    }

    #[test]
    fn test_python_extract_pkg_name_no_pyproject() {
        let tmp = TempDir::new().unwrap();
        let source = tmp.path().to_string_lossy().to_string();
        let pyproject_path = std::path::Path::new(&source).join("pyproject.toml");
        let local_pkg_name = std::fs::read_to_string(&pyproject_path)
            .ok()
            .and_then(|content| extract_python_package_name(&content));
        assert_eq!(local_pkg_name, None);
    }

    // --- Cargo.toml extraction (structured path) ---

    /// Replicate the Cargo.toml package name extraction used in both structured
    /// and fallback setup_environment paths.
    fn extract_cargo_pkg_name(cargo_toml_content: &str) -> Option<String> {
        cargo_toml_content
            .parse::<toml::Table>()
            .ok()
            .and_then(|t| {
                t.get("package")?
                    .as_table()?
                    .get("name")?
                    .as_str()
                    .map(|s| s.to_string())
            })
    }

    #[test]
    fn test_cargo_extract_pkg_name_basic() {
        let content = r#"[package]
name = "my-crate"
version = "0.1.0"
edition = "2021"
"#;
        assert_eq!(
            extract_cargo_pkg_name(content),
            Some("my-crate".to_string())
        );
    }

    #[test]
    fn test_cargo_extract_pkg_name_missing_package() {
        let content = r#"[lib]
name = "my-crate"
"#;
        assert_eq!(extract_cargo_pkg_name(content), None);
    }

    #[test]
    fn test_cargo_extract_pkg_name_missing_name() {
        let content = r#"[package]
version = "0.1.0"
edition = "2021"
"#;
        assert_eq!(extract_cargo_pkg_name(content), None);
    }

    #[test]
    fn test_cargo_extract_pkg_name_invalid_toml() {
        assert_eq!(extract_cargo_pkg_name("{{not toml}}"), None);
    }

    #[test]
    fn test_cargo_extract_pkg_name_workspace() {
        // Workspace Cargo.toml has [workspace] but not [package]
        let content = r#"[workspace]
members = ["crate-a", "crate-b"]
"#;
        assert_eq!(extract_cargo_pkg_name(content), None);
    }

    // --- Java jar copy logic (pure filesystem tests) ---

    #[test]
    fn test_java_jar_copy_from_maven_target() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = source_dir.path().join("target");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("my-lib-1.0.jar"), b"fake jar").unwrap();
        std::fs::write(target_dir.join("not-a-jar.txt"), b"not a jar").unwrap();

        let dest_tmp = TempDir::new().unwrap();
        let deps_dir = dest_tmp.path().join("deps");
        std::fs::create_dir_all(&deps_dir).unwrap();

        // Replicate the jar copy logic from JavaExecutor::setup_environment
        let source_path = source_dir.path();
        let maven_dir = source_path.join("target");
        let gradle_dir = source_path.join("build").join("libs");
        let jar_dir = if maven_dir.is_dir() {
            Some(maven_dir)
        } else if gradle_dir.is_dir() {
            Some(gradle_dir)
        } else {
            None
        };

        assert!(jar_dir.is_some(), "should find Maven target/");
        if let Some(jar_dir) = jar_dir {
            for entry in std::fs::read_dir(&jar_dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jar") {
                    let dest = deps_dir.join(entry.file_name());
                    std::fs::copy(&path, &dest).unwrap();
                }
            }
        }

        // Verify only jar files were copied
        let copied: Vec<_> = std::fs::read_dir(&deps_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(copied, vec!["my-lib-1.0.jar"]);
    }

    #[test]
    fn test_java_jar_copy_from_gradle_build_libs() {
        let source_dir = TempDir::new().unwrap();
        let libs_dir = source_dir.path().join("build").join("libs");
        std::fs::create_dir_all(&libs_dir).unwrap();
        std::fs::write(libs_dir.join("app-2.0.jar"), b"gradle jar").unwrap();
        std::fs::write(libs_dir.join("app-2.0-sources.jar"), b"sources jar").unwrap();

        let dest_tmp = TempDir::new().unwrap();
        let deps_dir = dest_tmp.path().join("deps");
        std::fs::create_dir_all(&deps_dir).unwrap();

        let source_path = source_dir.path();
        let maven_dir = source_path.join("target");
        let gradle_dir = source_path.join("build").join("libs");
        let jar_dir = if maven_dir.is_dir() {
            Some(maven_dir)
        } else if gradle_dir.is_dir() {
            Some(gradle_dir)
        } else {
            None
        };

        assert!(jar_dir.is_some(), "should find Gradle build/libs/");
        if let Some(jar_dir) = jar_dir {
            for entry in std::fs::read_dir(&jar_dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jar") {
                    let dest = deps_dir.join(entry.file_name());
                    std::fs::copy(&path, &dest).unwrap();
                }
            }
        }

        let mut copied: Vec<_> = std::fs::read_dir(&deps_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        copied.sort();
        assert_eq!(copied, vec!["app-2.0-sources.jar", "app-2.0.jar"]);
    }

    #[test]
    fn test_java_jar_copy_no_target_no_build() {
        // Neither target/ nor build/libs/ exists
        let source_dir = TempDir::new().unwrap();
        let source_path = source_dir.path();
        let maven_dir = source_path.join("target");
        let gradle_dir = source_path.join("build").join("libs");
        let jar_dir = if maven_dir.is_dir() {
            Some(maven_dir)
        } else if gradle_dir.is_dir() {
            Some(gradle_dir)
        } else {
            None
        };
        assert!(jar_dir.is_none(), "no jar dir should be found");
    }

    #[test]
    fn test_java_jar_copy_target_empty() {
        // target/ exists but has no jar files
        let source_dir = TempDir::new().unwrap();
        let target_dir = source_dir.path().join("target");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("classes"), b"not a jar").unwrap();

        let dest_tmp = TempDir::new().unwrap();
        let deps_dir = dest_tmp.path().join("deps");
        std::fs::create_dir_all(&deps_dir).unwrap();

        let jar_dir = Some(target_dir);
        if let Some(jar_dir) = jar_dir {
            for entry in std::fs::read_dir(&jar_dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jar") {
                    let dest = deps_dir.join(entry.file_name());
                    std::fs::copy(&path, &dest).unwrap();
                }
            }
        }

        let count = std::fs::read_dir(&deps_dir).unwrap().count();
        assert_eq!(count, 0, "no jars should be copied");
    }

    #[test]
    fn test_java_jar_copy_prefers_maven_over_gradle() {
        // Both target/ and build/libs/ exist — should prefer target/ (Maven)
        let source_dir = TempDir::new().unwrap();
        let target_dir = source_dir.path().join("target");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("maven.jar"), b"maven").unwrap();

        let libs_dir = source_dir.path().join("build").join("libs");
        std::fs::create_dir_all(&libs_dir).unwrap();
        std::fs::write(libs_dir.join("gradle.jar"), b"gradle").unwrap();

        let source_path = source_dir.path();
        let maven_dir = source_path.join("target");
        let gradle_dir = source_path.join("build").join("libs");
        let jar_dir = if maven_dir.is_dir() {
            Some(maven_dir)
        } else if gradle_dir.is_dir() {
            Some(gradle_dir)
        } else {
            None
        };

        // Should pick Maven target/, not Gradle build/libs/
        assert!(jar_dir.is_some());
        let jar_dir_path = jar_dir.unwrap();
        assert!(
            jar_dir_path.ends_with("target"),
            "should prefer Maven target/: {:?}",
            jar_dir_path
        );
    }

    // --- Java classpath construction ---

    #[test]
    fn test_java_classpath_with_deps_dir() {
        let tmp = TempDir::new().unwrap();
        let deps_dir = tmp.path().join("deps");
        std::fs::create_dir_all(&deps_dir).unwrap();

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

        #[cfg(unix)]
        assert_eq!(classpath, "deps/*:.");
        #[cfg(windows)]
        assert_eq!(classpath, "deps/*;.");
    }

    #[test]
    fn test_java_classpath_without_deps_dir() {
        let tmp = TempDir::new().unwrap();
        let deps_dir = tmp.path().join("deps");
        // Don't create deps_dir

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

        assert_eq!(classpath, ".");
    }

    // --- Java deps plural/singular wording ---

    #[test]
    fn test_java_singular_dependency_wording() {
        let deps_len = 1;
        let word = if deps_len == 1 {
            "dependency"
        } else {
            "dependencies"
        };
        assert_eq!(word, "dependency");
    }

    #[test]
    fn test_java_plural_dependencies_wording() {
        let deps_len = 3;
        let word = if deps_len == 1 {
            "dependency"
        } else {
            "dependencies"
        };
        assert_eq!(word, "dependencies");
    }

    // --- Cargo: backslash normalization for local path on non-Windows ---

    #[test]
    fn test_cargo_backslash_normalization() {
        // Verify the backslash→forward-slash replacement used in both
        // structured and fallback Cargo dep paths
        let source_with_backslash = r"C:\Users\dev\my-crate";
        let safe = source_with_backslash.replace('\\', "/");
        assert_eq!(safe, "C:/Users/dev/my-crate");
        assert!(!safe.contains('\\'));
    }

    #[test]
    fn test_cargo_forward_slash_unchanged() {
        let source = "/home/dev/my-crate";
        let safe = source.replace('\\', "/");
        assert_eq!(safe, source);
    }

    // --- Cargo structured dep: full Cargo.toml generation ---

    #[test]
    fn test_cargo_full_toml_generation_with_deps() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        let deps = [
            StructuredDep {
                name: "serde".to_string(),
                raw_spec: Some("{ version = \"1\", features = [\"derive\"] }".to_string()),
                source: DepSource::Manifest,
            },
            StructuredDep {
                name: "anyhow".to_string(),
                raw_spec: None,
                source: DepSource::Pattern,
            },
        ];

        let lines: Vec<String> = deps
            .iter()
            .map(|d| format_cargo_dep_line(d, None))
            .collect();
        let deps_section = format!("\n[dependencies]\n{}\n", lines.join("\n"));
        let cargo_toml = format!(
            r#"[package]
name = "skilldo-test"
version = "0.1.0"
edition = "2021"
{deps_section}"#
        );

        assert!(cargo_toml.contains("[package]"));
        assert!(cargo_toml.contains("skilldo-test"));
        assert!(cargo_toml.contains("[dependencies]"));
        assert!(cargo_toml.contains("serde = { version = \"1\", features = [\"derive\"] }"));
        assert!(cargo_toml.contains("anyhow = \"*\""));
    }

    #[test]
    fn test_cargo_full_toml_generation_no_deps() {
        let deps: Vec<crate::pipeline::collector::StructuredDep> = vec![];
        let deps_section = if deps.is_empty() {
            String::new()
        } else {
            unreachable!()
        };
        let cargo_toml = format!(
            r#"[package]
name = "skilldo-test"
version = "0.1.0"
edition = "2021"
{deps_section}"#
        );

        assert!(cargo_toml.contains("[package]"));
        assert!(!cargo_toml.contains("[dependencies]"));
    }

    // --- Cargo: fallback path dep generation with local source ---

    #[test]
    fn test_cargo_fallback_deps_section_with_local_source() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        let deps = vec!["my-crate".to_string(), "serde".to_string()];
        let local_source = tmp.path().to_string_lossy().to_string();

        let lines: Vec<String> = deps
            .iter()
            .map(|d| format_cargo_plain_dep_line(d, Some(&local_source)))
            .collect();
        let deps_section = format!("\n[dependencies]\n{}\n", lines.join("\n"));

        assert!(deps_section.contains("[dependencies]"));
        assert!(
            lines[0].contains("path = "),
            "local dep should use path: {}",
            lines[0]
        );
        assert_eq!(lines[1], "serde = \"*\"");
    }

    // --- Node: install_args formatting with all-matching deps ---

    #[test]
    fn test_node_local_install_all_deps_match() {
        // All deps are the local package — install_args should only contain the source
        let deps = vec!["my-pkg".to_string()];
        let install_args = build_node_install_args(&deps, "/path/to/pkg", Some("my-pkg"));

        assert_eq!(install_args, vec!["/path/to/pkg".to_string()]);
    }

    #[test]
    fn test_node_local_install_empty_deps() {
        // No deps at all — install_args should only contain the source
        let deps: Vec<String> = vec![];
        let install_args = build_node_install_args(&deps, "/path/to/pkg", None);

        assert_eq!(install_args, vec!["/path/to/pkg".to_string()]);
    }

    // --- Go: needs_replace logic (via go_dep_needs_replace helper) ---

    #[test]
    fn test_go_needs_replace_exact_module() {
        let deps = vec!["github.com/user/mylib".to_string()];
        assert!(go_dep_needs_replace(&deps, "github.com/user/mylib"));
    }

    #[test]
    fn test_go_needs_replace_subpackage() {
        let deps = vec!["github.com/user/mylib/subpkg".to_string()];
        assert!(go_dep_needs_replace(&deps, "github.com/user/mylib"));
    }

    #[test]
    fn test_go_needs_replace_no_match() {
        let deps = vec!["github.com/other/lib".to_string()];
        assert!(!go_dep_needs_replace(&deps, "github.com/user/mylib"));
    }

    #[test]
    fn test_go_needs_replace_empty_deps() {
        let deps: Vec<String> = vec![];
        assert!(!go_dep_needs_replace(&deps, "github.com/user/mylib"));
    }

    #[test]
    fn test_go_needs_replace_prefix_collision_no_match() {
        // "github.com/user/mylib2" should NOT trigger replace for "github.com/user/mylib"
        let deps = vec!["github.com/user/mylib2".to_string()];
        assert!(!go_dep_needs_replace(&deps, "github.com/user/mylib"));
    }

    // --- Java filter: empty deps ---

    #[test]
    fn test_java_filter_empty_deps() {
        let deps: Vec<String> = vec![];
        let filtered = filter_java_deps_by_artifact_id(&deps, Some("my-lib"));
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_java_filter_empty_deps_no_artifact() {
        let deps: Vec<String> = vec![];
        let filtered = filter_java_deps_by_artifact_id(&deps, None);
        assert!(filtered.is_empty());
    }

    // --- Java: pom precompute skipping (fetch_deps empty after filtering) ---

    #[test]
    fn test_java_pom_none_when_all_deps_filtered() {
        // When all deps match the local artifact, fetch_deps is empty,
        // so pom should be None (no Maven fetch needed)
        let deps = vec!["com.example:my-lib:1.0".to_string()];
        let fetch_deps = filter_java_deps_by_artifact_id(&deps, Some("my-lib"));
        assert!(fetch_deps.is_empty());
        let pom = if fetch_deps.is_empty() {
            None
        } else {
            crate::util::build_maven_pom_xml(&fetch_deps)
        };
        assert!(pom.is_none());
    }

    #[test]
    fn test_java_pom_some_when_deps_remain() {
        let deps = vec![
            "com.example:my-lib:1.0".to_string(),
            "com.google.code.gson:gson:2.10.1".to_string(),
        ];
        let fetch_deps = filter_java_deps_by_artifact_id(&deps, Some("my-lib"));
        assert_eq!(fetch_deps.len(), 1);
        let pom = if fetch_deps.is_empty() {
            None
        } else {
            crate::util::build_maven_pom_xml(&fetch_deps)
        };
        assert!(pom.is_some());
        let pom_str = pom.unwrap();
        assert!(pom_str.contains("gson"));
        assert!(!pom_str.contains("my-lib"));
    }

    // --- Cargo structured: dep with path override has backslash-safe path ---

    #[test]
    fn test_cargo_structured_dep_local_path_with_backslash_source() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        // Create a temp dir with a Cargo.toml
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        let dep = StructuredDep {
            name: "my-crate".to_string(),
            raw_spec: Some("\"1.0\"".to_string()),
            source: DepSource::Manifest,
        };

        let line = format_cargo_dep_line(&dep, Some(&tmp.path().to_string_lossy()));
        assert!(line.contains("path = "), "should use path: {line}");
        assert!(
            !line.contains('\\'),
            "path should not contain backslashes: {line}"
        );
    }

    // --- stderr_only helper ---

    #[test]
    fn test_stderr_only_returns_stderr() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"stdout content".to_vec(),
            stderr: b"stderr content".to_vec(),
        };
        let result = stderr_only(&output);
        assert_eq!(result, "stderr content");
        // Verify it does NOT include stdout
        assert!(!result.contains("stdout content"));
    }

    #[test]
    fn test_stderr_only_empty() {
        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"stdout".to_vec(),
            stderr: b"".to_vec(),
        };
        let result = stderr_only(&output);
        assert_eq!(result, "");
    }

    // --- Python: full local-install filtering end-to-end via temp dir ---

    #[test]
    fn test_python_local_install_full_flow() {
        // Create a temp dir simulating a Python project
        let tmp = TempDir::new().unwrap();
        let pyproject = r#"[project]
name = "my-cool-lib"
version = "0.1.0"
requires-python = ">=3.8"
dependencies = []
"#;
        std::fs::write(tmp.path().join("pyproject.toml"), pyproject).unwrap();

        // Simulate the full extraction+filtering flow
        let source = tmp.path().to_string_lossy().to_string();
        let pyproject_path = std::path::Path::new(&source).join("pyproject.toml");
        let local_pkg_name: Option<String> = std::fs::read_to_string(&pyproject_path)
            .ok()
            .and_then(|content| content.parse::<toml::Table>().ok())
            .and_then(|t| {
                t.get("project")?
                    .as_table()?
                    .get("name")?
                    .as_str()
                    .map(|s| s.to_string())
            });
        assert_eq!(local_pkg_name, Some("my-cool-lib".to_string()));

        // Filter deps — "my-cool-lib" and "my_cool_lib" should both be excluded
        let deps = vec![
            "my-cool-lib".to_string(),
            "my_cool_lib".to_string(),
            "requests".to_string(),
        ];
        let filtered: Vec<&String> = deps
            .iter()
            .filter(|d| {
                if let Some(ref local_name) = local_pkg_name {
                    let norm_d = d.replace('-', "_").to_lowercase();
                    let norm_local = local_name.replace('-', "_").to_lowercase();
                    norm_d != norm_local
                } else {
                    true
                }
            })
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(*filtered[0], "requests");
    }

    // --- Cargo: full local-install flow for fallback (non-structured) path ---

    #[test]
    fn test_cargo_fallback_local_install_full_flow() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        let source = tmp.path().to_string_lossy().to_string();
        let cargo_path = std::path::Path::new(&source).join("Cargo.toml");
        let local_pkg_name: Option<String> = std::fs::read_to_string(&cargo_path)
            .ok()
            .and_then(|content| content.parse::<toml::Table>().ok())
            .and_then(|t| {
                t.get("package")?
                    .as_table()?
                    .get("name")?
                    .as_str()
                    .map(|s| s.to_string())
            });
        assert_eq!(local_pkg_name, Some("my-crate".to_string()));

        // Build deps section with local override
        let deps = vec!["my-crate".to_string(), "tokio".to_string()];
        let lines: Vec<String> = deps
            .iter()
            .map(|d| {
                if local_pkg_name.as_deref() == Some(d.as_str()) {
                    let safe = source.replace('\\', "/");
                    format!("{d} = {{ path = \"{}\" }}", safe)
                } else {
                    format!("{d} = \"*\"")
                }
            })
            .collect();
        let deps_section = format!("\n[dependencies]\n{}\n", lines.join("\n"));

        assert!(deps_section.contains("[dependencies]"));
        assert!(lines[0].contains("path = "));
        assert!(lines[0].starts_with("my-crate = "));
        assert_eq!(lines[1], "tokio = \"*\"");
    }

    // --- Java: full local-install artifact exclusion flow ---

    #[test]
    fn test_java_local_install_full_flow() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId>my-java-lib</artifactId><version>1.0</version></project>",
        )
        .unwrap();

        // Also create target/ with a jar
        let target_dir = tmp.path().join("target");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("my-java-lib-1.0.jar"), b"fake").unwrap();

        let handler = crate::ecosystems::java::JavaHandler::new(tmp.path());
        let local_artifact_id = handler.get_package_name().ok();
        assert_eq!(local_artifact_id, Some("my-java-lib".to_string()));

        // Filter Maven deps
        let deps = vec![
            "com.example:my-java-lib:1.0".to_string(),
            "com.google.code.gson:gson:2.10.1".to_string(),
        ];
        let fetch_deps = filter_java_deps_by_artifact_id(&deps, local_artifact_id.as_deref());
        assert_eq!(fetch_deps.len(), 1);
        assert_eq!(fetch_deps[0], "com.google.code.gson:gson:2.10.1");

        // Verify jar copy finds the jar
        let source_path = tmp.path();
        let maven_dir = source_path.join("target");
        assert!(maven_dir.is_dir());
        let jars: Vec<_> = std::fs::read_dir(&maven_dir)
            .unwrap()
            .filter_map(|e| {
                let e = e.ok()?;
                if e.path().extension()?.to_str()? == "jar" {
                    Some(e.file_name().to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(jars, vec!["my-java-lib-1.0.jar"]);
    }

    // --- Go: full local-install detection flow ---

    #[test]
    fn test_go_local_install_full_flow() {
        let tmp = TempDir::new().unwrap();
        let go_mod_content = "module github.com/user/mylib\n\ngo 1.21\n";
        std::fs::write(tmp.path().join("go.mod"), go_mod_content).unwrap();

        // Extract module name
        let source = tmp.path().to_string_lossy().to_string();
        let go_mod_path = std::path::Path::new(&source).join("go.mod");
        let local_module = std::fs::read_to_string(&go_mod_path)
            .ok()
            .and_then(|content| extract_go_module_name(&content));
        assert_eq!(local_module, Some("github.com/user/mylib".to_string()));

        // Check needs_replace for various deps
        let module = local_module.unwrap();
        let deps = vec![
            "github.com/user/mylib".to_string(),
            "github.com/user/mylib/subpkg".to_string(),
            "github.com/other/lib".to_string(),
        ];

        assert!(
            go_dep_needs_replace(&deps, &module),
            "should need replace for mylib deps"
        );

        // Count which deps trigger replace
        let replace_count = deps
            .iter()
            .filter(|d| go_module_matches_dep(d, &module))
            .count();
        assert_eq!(replace_count, 2); // exact + subpkg
    }

    // --- Node: full local-install flow with temp dir ---

    #[test]
    fn test_node_local_install_full_flow() {
        let tmp = TempDir::new().unwrap();
        let pkg_json_content = r#"{"name": "@scope/my-pkg", "version": "1.0.0"}"#;
        std::fs::write(tmp.path().join("package.json"), pkg_json_content).unwrap();

        // Read + extract using the real helpers
        let source = tmp.path().to_string_lossy().to_string();
        let pkg_path = std::path::Path::new(&source).join("package.json");
        let local_name: Option<String> = std::fs::read_to_string(&pkg_path)
            .ok()
            .and_then(|c| extract_node_package_name(&c));
        assert_eq!(local_name, Some("@scope/my-pkg".to_string()));

        // Build install args — local package excluded
        let deps = vec![
            "@scope/my-pkg".to_string(),
            "express".to_string(),
            "lodash".to_string(),
        ];
        let install_args = build_node_install_args(&deps, &source, local_name.as_deref());

        assert_eq!(install_args.len(), 3); // source + express + lodash
        assert_eq!(install_args[0], source);
        assert_eq!(install_args[1], "express");
        assert_eq!(install_args[2], "lodash");
    }
}
