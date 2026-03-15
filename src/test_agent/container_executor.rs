//! Container-based code executor for test agent validation
//! Supports Docker, Podman, and other OCI-compatible runtimes

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::executor::{ExecutionEnv, ExecutionResult};
use super::LanguageExecutor;
use crate::config::{ContainerConfig, InstallSource};
use crate::detector::Language;
use crate::util::{run_cmd_with_timeout, sanitize_dep_name};

pub struct ContainerExecutor {
    config: ContainerConfig,
    language: Language,
}

impl ContainerExecutor {
    pub fn new(config: ContainerConfig, language: Language) -> Self {
        Self { config, language }
    }

    /// Get the container image for the current language
    fn get_image(&self) -> &str {
        match self.language {
            Language::Python => &self.config.python_image,
            Language::JavaScript => &self.config.javascript_image,
            Language::Rust => &self.config.rust_image,
            Language::Go => &self.config.go_image,
            Language::Java => &self.config.java_image,
        }
    }

    /// Check if container runtime is available
    async fn check_runtime_available(&self) -> bool {
        super::executor::is_tool_available(&self.config.runtime, "--version").await
    }

    /// Generate dependency installation script for JavaScript/TypeScript
    fn generate_node_install_script(&self, deps: &[String]) -> Result<String> {
        // Always write package.json with type:module so ESM import syntax works.
        let mut lines = vec![
            r#"echo '{"name":"skilldo-test","version":"0.1.0","private":true,"type":"module"}' > package.json"#.to_string(),
        ];
        if !deps.is_empty() {
            for dep in deps {
                sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
            }
            // Single-quote each dep to prevent shell metachar interpretation
            // (e.g., >=, ~, ^ are shell operators in unquoted context).
            // The `--` terminator prevents dep names starting with `-` from
            // being interpreted as npm flags.
            let quoted: Vec<String> = deps.iter().map(|d| format!("'{}'", d)).collect();
            lines.push(format!(
                "npm install --no-save --ignore-scripts --no-audit --no-fund -- {} > /dev/null 2>&1",
                quoted.join(" ")
            ));
        }
        Ok(lines.join("\n"))
    }

    /// Generate dependency installation script for Go.
    /// Initializes a Go module and runs `go get` for each dependency.
    fn generate_go_install_script(&self, deps: &[String]) -> Result<String> {
        // go.mod may already exist from a previous pattern run; only init if missing
        let mut lines = vec!["[ -f go.mod ] || go mod init test >/dev/null 2>&1".to_string()];
        for dep in deps {
            sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
            lines.push(format!("go get '{}'", dep));
        }
        Ok(lines.join("\n"))
    }

    /// Generate dependency installation script for Java.
    /// Creates a pom.xml and uses `mvn dependency:copy-dependencies` to fetch jars.
    fn generate_java_install_script(&self, deps: &[String]) -> Result<String> {
        if deps.is_empty() {
            return Ok(String::new());
        }

        for dep in deps {
            sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
        }

        let deps_xml: Vec<String> = deps
            .iter()
            .filter_map(|d| {
                let parts: Vec<&str> = d.splitn(3, ':').collect();
                if parts.len() >= 2 {
                    let group = parts[0];
                    let artifact = parts[1];
                    let version_tag = if let Some(v) = parts.get(2) {
                        format!("<version>{v}</version>")
                    } else {
                        String::new()
                    };
                    Some(format!(
                        "        <dependency><groupId>{group}</groupId><artifactId>{artifact}</artifactId>{version_tag}</dependency>"
                    ))
                } else {
                    None
                }
            })
            .collect();

        if deps_xml.is_empty() {
            return Ok(String::new());
        }

        let pom = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>skilldo</groupId>
    <artifactId>test</artifactId>
    <version>0.1.0</version>
    <dependencies>
{}
    </dependencies>
</project>"#,
            deps_xml.join("\n")
        );

        Ok(format!(
            "mkdir -p deps\ncat > pom.xml << 'POMEOF'\n{pom}\nPOMEOF\nmvn dependency:copy-dependencies -DoutputDirectory=deps -q || echo 'WARNING: Maven dependency resolution failed — tests may fail due to missing jars' >&2"
        ))
    }

    /// Generate run.sh for non-Python languages (JS, Rust, Go, Java)
    /// Python uses `uv run test.py` directly — no run.sh needed
    fn generate_container_script(&self, deps: &[String]) -> Result<String> {
        let install_cmd = match self.language {
            Language::JavaScript => self.generate_node_install_script(deps)?,
            Language::Go => self.generate_go_install_script(deps)?,
            Language::Java => self.generate_java_install_script(deps)?,
            _ => String::new(),
        };

        let run_line = match self.language {
            Language::JavaScript => "node test.js",
            Language::Rust => "rustc main.rs -o main && ./main",
            Language::Go => "go run main.go",
            Language::Java => "javac -cp 'deps/*:.' Main.java && java -cp 'deps/*:.' Main",
            Language::Python => {
                bail!("generate_container_script should not be called for Python")
            }
        };

        Ok(format!(
            r#"#!/bin/sh
set -e
cd /workspace
{}
{}
"#,
            install_cmd, run_line
        ))
    }
}

#[async_trait::async_trait]
impl LanguageExecutor for ContainerExecutor {
    async fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv> {
        info!(
            "Setting up {} environment with {} dependencies",
            self.language.as_str(),
            deps.len()
        );
        // Check if runtime is available
        if !self.check_runtime_available().await {
            bail!(
                "{} runtime not found. Please install {} first.",
                self.config.runtime,
                self.config.runtime
            );
        }

        // Create temporary directory
        let temp_dir = TempDir::new().context("Failed to create temp directory")?;
        debug!("Created temp directory: {}", temp_dir.path().display());

        // Generate a unique container name
        let dir_name = temp_dir
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let container_name = format!("skilldo-test-{}", dir_name.replace('.', ""));

        debug!("Container name: {}", container_name);

        Ok(ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: Some(container_name),
            dependencies: deps.to_vec(),
        })
    }

    async fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult> {
        debug!(
            "Running {} code ({} bytes)",
            self.language.as_str(),
            code.len()
        );

        let is_python = matches!(self.language, Language::Python);

        // Write code to file
        let code_file = match self.language {
            Language::Python => "test.py",
            Language::JavaScript => "test.js",
            Language::Rust => "main.rs",
            Language::Go => "main.go",
            Language::Java => "Main.java",
        };

        let code_path = env.temp_dir.path().join(code_file);
        fs::write(&code_path, code).context("Failed to write test script")?;

        // For non-Python: generate run.sh with install + run commands
        if !is_python {
            let script = self.generate_container_script(&env.dependencies)?;
            let script_path = env.temp_dir.path().join("run.sh");
            let mut script_file = fs::File::create(&script_path)?;
            script_file.write_all(script.as_bytes())?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&script_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&script_path, perms)?;
            }
        }

        // Run container
        let image = self.get_image();
        debug!("Using container image: {}", image);

        let container_name = env
            .container_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container name not set in execution environment"))?;

        // Remove any existing container with this name first
        let _ = Command::new(&self.config.runtime)
            .arg("rm")
            .arg("-f")
            .arg(container_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        // Make temp dir world-writable so --user nobody can write to /workspace.
        // Only needed when running as unprivileged user (skip for local-install).
        #[cfg(unix)]
        if self.config.install_source != InstallSource::LocalInstall {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(env.temp_dir.path(), std::fs::Permissions::from_mode(0o777))?;
        }

        let mut cmd = Command::new(&self.config.runtime);
        cmd.arg("run");

        if self.config.cleanup {
            cmd.arg("--rm");
        }

        // Run as unprivileged user to limit damage from untrusted code.
        // Skip for local-install mode which needs root for pip install.
        if self.config.install_source != InstallSource::LocalInstall {
            cmd.arg("--user").arg("nobody");
        }

        cmd.arg("--name")
            .arg(container_name)
            .arg("-v")
            .arg(format!("{}:/workspace", env.temp_dir.path().display()));

        // Mount source repo for local modes (local-install, local-mount).
        if self.config.install_source != InstallSource::Registry {
            let source = self.config.source_path.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "source_path is required when install_source is '{}'",
                    self.config.install_source
                )
            })?;
            cmd.arg("-v").arg(format!("{}:/src:ro", source));
        }

        // Set language-specific module path for local-mount mode.
        // Only Python needs PYTHONPATH; Go uses modules (not GOPATH) and
        // expects `replace` directives in go.mod for local deps.
        if self.config.install_source == InstallSource::LocalMount
            && matches!(self.language, Language::Python)
        {
            cmd.arg("-e").arg("PYTHONPATH=/src");
        }

        // Python uv needs a writable cache dir when running as nobody
        if matches!(self.language, Language::Python)
            && self.config.install_source != InstallSource::LocalInstall
        {
            cmd.arg("-e").arg("UV_CACHE_DIR=/tmp/uv-cache");
        }

        // Go needs a writable build cache and module cache when running as nobody
        if matches!(self.language, Language::Go)
            && self.config.install_source != InstallSource::LocalInstall
        {
            cmd.arg("-e")
                .arg("GOCACHE=/tmp/go-cache")
                .arg("-e")
                .arg("GOPATH=/tmp/gopath");
        }

        // Pass extra environment variables (private registries, proxies, etc.)
        for (key, value) in &self.config.extra_env {
            warn_dangerous_env_var(key);
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }

        cmd.arg("-w").arg("/workspace").arg(image);

        // Python: use `uv run test.py` (default image has uv pre-installed)
        //   uv reads PEP 723 inline script metadata for deps
        //   local-install: pip install /src first, then run
        // Other languages: `sh run.sh` — traditional install + run
        if is_python {
            match self.config.install_source {
                InstallSource::LocalInstall => {
                    cmd.arg("sh")
                        .arg("-c")
                        .arg("cd /workspace && uv pip install --system /src && uv run test.py");
                }
                _ => {
                    // registry and local-mount: uv handles deps via PEP 723
                    cmd.arg("uv").arg("run").arg("test.py");
                }
            }
        } else {
            cmd.arg("/bin/sh").arg("run.sh");
        }

        if self.config.extra_env.is_empty() {
            debug!("Executing: {:?}", cmd);
        } else {
            let env_keys: Vec<&String> = self.config.extra_env.keys().collect();
            debug!(
                "Executing container command (extra env keys: {:?}, values redacted)",
                env_keys
            );
        }

        // Run with timeout
        let output = self
            .run_with_timeout(
                cmd,
                Duration::from_secs(self.config.timeout),
                container_name,
            )
            .await?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            debug!("✓ Code execution passed");
            Ok(ExecutionResult::Pass(stdout))
        } else {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            debug!("✗ Code execution failed");
            debug!("  stdout: {}", stdout);
            debug!("  stderr: {}", stderr);
            Ok(ExecutionResult::Fail(format!(
                "stdout:\n{}\nstderr:\n{}",
                stdout, stderr
            )))
        }
    }

    async fn cleanup(&self, env: &ExecutionEnv) -> Result<()> {
        if !self.config.cleanup {
            debug!("Cleanup disabled, skipping");
            return Ok(());
        }

        let container_name = env
            .container_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container name not set in execution environment"))?;

        // Force remove container if it's still running
        if let Err(e) = Command::new(&self.config.runtime)
            .arg("rm")
            .arg("-f")
            .arg(container_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
        {
            warn!("Failed to remove container {}: {}", container_name, e);
        }

        debug!("Container {} cleaned up", container_name);
        Ok(())
    }
}

impl ContainerExecutor {
    /// Run a command with a timeout, also killing the container on failure/timeout
    async fn run_with_timeout(
        &self,
        cmd: Command,
        timeout: Duration,
        container_name: &str,
    ) -> Result<std::process::Output> {
        match run_cmd_with_timeout(cmd, timeout).await {
            Ok(output) => Ok(output),
            Err(e) => {
                // Also kill the container on any error (timeout or otherwise)
                if let Err(kill_err) = Command::new(&self.config.runtime)
                    .arg("kill")
                    .arg(container_name)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await
                {
                    warn!("Failed to kill container {}: {}", container_name, kill_err);
                }
                Err(e)
            }
        }
    }
}

/// Env vars that could compromise container isolation or hijack execution.
const DANGEROUS_ENV_VARS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "PATH",
    "HOME",
    "SHELL",
    "USER",
    // Language-specific interpreter hijack vars
    "NODE_OPTIONS",
    "JAVA_TOOL_OPTIONS",
    "GEM_PATH",
    "PERL5LIB",
    "RUBYLIB",
];

fn warn_dangerous_env_var(key: &str) {
    if DANGEROUS_ENV_VARS
        .iter()
        .any(|d| key.eq_ignore_ascii_case(d))
    {
        warn!(
            "extra_env contains sensitive variable '{}' — this may compromise container isolation",
            key
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ExecutionMode;

    #[test]
    fn test_generate_node_install_script() {
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            runtime: "podman".to_string(),
            python_image: "python:3.11-alpine".to_string(),
            javascript_image: "node:18-alpine".to_string(),
            rust_image: "rust:1.70-alpine".to_string(),
            go_image: "golang:1.20-alpine".to_string(),
            java_image: "maven:3-eclipse-temurin-21-alpine".to_string(),
            timeout: 60,
            cleanup: true,
            install_source: InstallSource::Registry,
            source_path: None,
            extra_env: std::collections::HashMap::new(),
        };

        let executor = ContainerExecutor::new(config, Language::JavaScript);

        let deps = vec!["express".to_string(), "cors".to_string()];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert!(script.contains("package.json"), "should write package.json");
        assert!(script.contains("type"), "should include type:module");
        assert!(script.contains("npm install"));
        assert!(script.contains("express"));
        assert!(script.contains("cors"));

        // Empty deps — still writes package.json for ESM support
        let empty = executor.generate_node_install_script(&[]).unwrap();
        assert!(empty.contains("package.json"));
        assert!(empty.contains("type"));
        assert!(!empty.contains("npm install"));

        // Shell injection rejected
        let bad = vec!["express; rm -rf /".to_string()];
        assert!(executor.generate_node_install_script(&bad).is_err());
    }

    fn make_config() -> ContainerConfig {
        ContainerConfig {
            execution_mode: ExecutionMode::Container,
            runtime: "podman".to_string(),
            python_image: "python:3.11-alpine".to_string(),
            javascript_image: "node:18-alpine".to_string(),
            rust_image: "rust:1.70-alpine".to_string(),
            go_image: "golang:1.20-alpine".to_string(),
            java_image: "maven:3-eclipse-temurin-21-alpine".to_string(),
            timeout: 60,
            cleanup: true,
            install_source: InstallSource::Registry,
            source_path: None,
            extra_env: std::collections::HashMap::new(),
        }
    }

    /// Language expectations for parameterized tests.
    /// (language, expected_image, code_file, sample_code)
    fn language_expectations() -> Vec<(Language, &'static str, &'static str, &'static str)> {
        vec![
            (
                Language::Python,
                "python:3.11-alpine",
                "test.py",
                "print('hello')",
            ),
            (
                Language::JavaScript,
                "node:18-alpine",
                "test.js",
                "console.log('hi')",
            ),
            (
                Language::Rust,
                "rust:1.70-alpine",
                "main.rs",
                "fn main() {}",
            ),
            (
                Language::Go,
                "golang:1.20-alpine",
                "main.go",
                "package main",
            ),
            (
                Language::Java,
                "maven:3-eclipse-temurin-21-alpine",
                "Main.java",
                "public class Main { public static void main(String[] args) {} }",
            ),
        ]
    }

    #[test]
    fn test_get_image_per_language() {
        for (lang, expected_image, _, _) in language_expectations() {
            let executor = ContainerExecutor::new(make_config(), lang.clone());
            assert_eq!(
                executor.get_image(),
                expected_image,
                "{}: expected image {}",
                lang.as_str(),
                expected_image
            );
        }
    }

    #[tokio::test]
    async fn test_code_file_per_language() {
        for (lang, _, expected_file, sample_code) in language_expectations() {
            let executor = ContainerExecutor::new(make_config(), lang.clone());
            let temp_dir = TempDir::new().unwrap();
            let env = ExecutionEnv {
                temp_dir,
                interpreter_path: None,
                container_name: None,
                dependencies: vec![],
            };
            let _ = executor.run_code(&env, sample_code).await;
            let code_path = env.temp_dir.path().join(expected_file);
            assert!(
                code_path.exists(),
                "{}: expected {} to exist",
                lang.as_str(),
                expected_file
            );
            let content = fs::read_to_string(&code_path).unwrap();
            assert_eq!(
                content,
                sample_code,
                "{}: file content mismatch",
                lang.as_str()
            );
        }
    }

    #[test]
    fn test_rust_container_script() {
        let executor = ContainerExecutor::new(make_config(), Language::Rust);
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.contains("rustc main.rs -o main && ./main"));
    }

    #[test]
    fn test_go_container_script() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.contains("go mod init test"));
        assert!(script.contains("go run main.go"));
    }

    #[test]
    fn test_python_container_script_not_used() {
        // Python uses uv run directly, not run.sh — generate_container_script should error
        let executor = ContainerExecutor::new(make_config(), Language::Python);
        assert!(executor.generate_container_script(&[]).is_err());
    }

    #[test]
    fn test_javascript_container_script_with_deps() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec!["express".to_string(), "cors".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.contains(
            "npm install --no-save --ignore-scripts --no-audit --no-fund -- 'express' 'cors'"
        ));
        assert!(script.contains("node test.js"));
    }

    #[tokio::test]
    async fn test_cleanup_no_container_name() {
        let executor = ContainerExecutor::new(make_config(), Language::Python);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let result = executor.cleanup(&env).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Container name not set"));
    }

    #[tokio::test]
    async fn test_cleanup_disabled() {
        let mut config = make_config();
        config.cleanup = false;
        let executor = ContainerExecutor::new(config, Language::Python);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: Some("test-container".to_string()),
            dependencies: vec![],
        };
        // Should return Ok without trying to rm anything
        assert!(executor.cleanup(&env).await.is_ok());
    }

    #[tokio::test]
    async fn test_run_code_no_container_name() {
        let executor = ContainerExecutor::new(make_config(), Language::Python);
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
            .contains("Container name not set"));
    }

    #[test]
    fn test_make_config_has_install_source() {
        let config = make_config();
        assert_eq!(config.install_source, InstallSource::Registry);
        assert!(config.source_path.is_none());
    }

    #[test]
    fn test_local_install_config() {
        let mut config = make_config();
        config.install_source = InstallSource::LocalInstall;
        config.source_path = Some("/tmp/my-lib".to_string());
        let executor = ContainerExecutor::new(config, Language::Python);
        assert_eq!(executor.config.install_source, InstallSource::LocalInstall);
    }

    #[tokio::test]
    async fn test_run_code_local_install_missing_source_path() {
        let mut config = make_config();
        config.install_source = InstallSource::LocalInstall;
        config.source_path = None;
        let executor = ContainerExecutor::new(config, Language::Python);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: Some("test-container".to_string()),
            dependencies: vec![],
        };
        let result = executor.run_code(&env, "print('hello')").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("source_path is required"));
    }

    #[test]
    fn test_local_mount_config() {
        let mut config = make_config();
        config.install_source = InstallSource::LocalMount;
        config.source_path = Some("/tmp/my-lib".to_string());
        let executor = ContainerExecutor::new(config, Language::Python);
        assert_eq!(executor.config.install_source, InstallSource::LocalMount);
    }

    // --- ContainerExecutor::new() with Default config ---

    #[test]
    fn test_new_with_default_config() {
        let config = ContainerConfig::default();
        let executor = ContainerExecutor::new(config, Language::Python);
        // Runtime is auto-detected (podman or docker)
        assert!(
            executor.config.runtime == "podman" || executor.config.runtime == "docker",
            "Expected podman or docker, got: {}",
            executor.config.runtime
        );
        assert_eq!(
            executor.config.python_image,
            "ghcr.io/astral-sh/uv:python3.11-bookworm-slim"
        );
        assert_eq!(executor.config.javascript_image, "node:24-alpine");
        assert_eq!(executor.config.rust_image, "rust:1.75-slim");
        assert_eq!(executor.config.go_image, "golang:1.25-alpine");
        assert_eq!(executor.config.timeout, 60);
        assert!(executor.config.cleanup);
        assert_eq!(executor.config.install_source, InstallSource::Registry);
        assert!(executor.config.source_path.is_none());
        assert!(executor.config.extra_env.is_empty());
        assert_eq!(executor.language, Language::Python);
    }

    #[test]
    fn test_new_stores_language() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        assert_eq!(executor.language, Language::Go);
    }

    // --- generate_container_script: JS/TS without deps ---

    #[test]
    fn test_javascript_container_script_without_deps() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.contains("node test.js"));
        // No npm install line when no deps
        assert!(!script.contains("npm install"));
    }

    // --- generate_container_script: Rust/Go with deps (deps should be ignored) ---

    #[test]
    fn test_rust_container_script_with_deps_ignores_them() {
        let executor = ContainerExecutor::new(make_config(), Language::Rust);
        let deps = vec!["serde".to_string(), "tokio".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.contains("rustc main.rs -o main && ./main"));
        // Rust has no install step, deps are ignored
        assert!(!script.contains("cargo install"));
        assert!(!script.contains("serde"));
    }

    #[test]
    fn test_go_container_script_with_deps_installs_them() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let deps = vec!["github.com/gin-gonic/gin".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.contains("go mod init test"));
        assert!(script.contains("go get 'github.com/gin-gonic/gin'"));
        assert!(script.contains("go run main.go"));
    }

    // --- Script content validation (shebang, set -e, cd /workspace) ---

    #[test]
    fn test_container_script_has_shebang_and_set_e() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("set -e"));
        assert!(script.contains("cd /workspace"));
    }

    #[test]
    fn test_container_script_structure_rust() {
        let executor = ContainerExecutor::new(make_config(), Language::Rust);
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("set -e"));
        assert!(script.contains("cd /workspace"));
    }

    #[test]
    fn test_container_script_structure_go() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("set -e"));
        assert!(script.contains("cd /workspace"));
    }

    // --- generate_node_install_script: format and sanitization ---

    #[test]
    fn test_node_install_script_single_dep() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec!["express".to_string()];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert!(
            script.contains("package.json"),
            "should write package.json with type:module"
        );
        assert!(script
            .contains("npm install --no-save --ignore-scripts --no-audit --no-fund -- 'express'"));
    }

    #[test]
    fn test_node_install_script_multiple_deps_format() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec![
            "express".to_string(),
            "cors".to_string(),
            "helmet".to_string(),
        ];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert!(
            script.contains("package.json"),
            "should write package.json with type:module"
        );
        assert!(script.contains("npm install --no-save --ignore-scripts --no-audit --no-fund -- 'express' 'cors' 'helmet'"));
    }

    #[test]
    fn test_node_install_script_scoped_package() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec!["@types/node".to_string()];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert!(script.contains("@types/node"));
    }

    #[test]
    fn test_node_install_script_rejects_backtick() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec!["`whoami`".to_string()];
        assert!(executor.generate_node_install_script(&deps).is_err());
    }

    #[test]
    fn test_node_install_script_rejects_dollar_sign() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec!["$(rm -rf /)".to_string()];
        assert!(executor.generate_node_install_script(&deps).is_err());
    }

    #[test]
    fn test_node_install_script_rejects_pipe() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec!["express | cat /etc/passwd".to_string()];
        assert!(executor.generate_node_install_script(&deps).is_err());
    }

    #[test]
    fn test_node_install_script_rejects_ampersand() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec!["express && rm -rf /".to_string()];
        assert!(executor.generate_node_install_script(&deps).is_err());
    }

    #[test]
    fn test_node_install_script_allows_version_constraint() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec!["express@4.18.2".to_string()];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert!(script.contains("express@4.18.2"));
    }

    // --- setup_environment: container name generation ---

    #[test]
    fn test_setup_environment_container_name_starts_with_prefix() {
        // setup_environment calls check_runtime_available() which will fail
        // without a real runtime, so we test the naming logic directly.
        // The container name format is: "skilldo-test-{dir_name.replace('.', '')}"
        let temp_dir = TempDir::new().unwrap();
        let dir_name = temp_dir
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let container_name = format!("skilldo-test-{}", dir_name.replace('.', ""));
        assert!(container_name.starts_with("skilldo-test-"));
        assert!(!container_name.contains('.'));
    }

    #[test]
    fn test_container_name_removes_dots() {
        // Simulate what setup_environment does with a dir name containing dots
        let dir_name = ".tmp.abc123";
        let container_name = format!("skilldo-test-{}", dir_name.replace('.', ""));
        assert_eq!(container_name, "skilldo-test-tmpabc123");
        assert!(!container_name.contains('.'));
    }

    // --- get_image with Default config (real default images) ---

    #[test]
    fn test_get_image_with_default_config_python() {
        let executor = ContainerExecutor::new(ContainerConfig::default(), Language::Python);
        assert_eq!(
            executor.get_image(),
            "ghcr.io/astral-sh/uv:python3.11-bookworm-slim"
        );
    }

    #[test]
    fn test_get_image_with_default_config_javascript() {
        let executor = ContainerExecutor::new(ContainerConfig::default(), Language::JavaScript);
        assert_eq!(executor.get_image(), "node:24-alpine");
    }

    #[test]
    fn test_get_image_with_default_config_rust() {
        let executor = ContainerExecutor::new(ContainerConfig::default(), Language::Rust);
        assert_eq!(executor.get_image(), "rust:1.75-slim");
    }

    #[test]
    fn test_get_image_with_default_config_go() {
        let executor = ContainerExecutor::new(ContainerConfig::default(), Language::Go);
        assert_eq!(executor.get_image(), "golang:1.25-alpine");
    }

    // --- setup_environment: runtime not found ---

    #[tokio::test]
    async fn test_setup_environment_missing_runtime() {
        let mut config = make_config();
        config.runtime = "nonexistent-runtime-xyz".to_string();
        let executor = ContainerExecutor::new(config, Language::Python);
        let result = executor.setup_environment(&[]).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("runtime not found"),
            "Expected 'runtime not found' in: {}",
            err_msg
        );
    }

    // --- setup_environment: dependencies stored ---

    #[tokio::test]
    async fn test_setup_environment_stores_dependencies() {
        // This requires a real runtime, but we can test with the actual podman/docker
        // if available. If not, this test validates the error path.
        let executor = ContainerExecutor::new(make_config(), Language::Python);
        let deps = vec!["requests".to_string(), "flask".to_string()];
        match executor.setup_environment(&deps).await {
            Ok(env) => {
                assert_eq!(env.dependencies, deps);
                assert!(env.container_name.is_some());
                assert!(env.interpreter_path.is_none());
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("runtime not found"),
                    "unexpected error (expected 'runtime not found'): {msg}"
                );
            }
        }
    }

    // --- cleanup: happy path with real container name (no actual container) ---

    #[tokio::test]
    async fn test_cleanup_with_container_name_and_cleanup_enabled() {
        let executor = ContainerExecutor::new(make_config(), Language::Python);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: Some("skilldo-test-fake123".to_string()),
            dependencies: vec![],
        };
        // cleanup calls `podman rm -f <name>` — the container doesn't exist, but
        // the command failure is silently ignored (let _ = ...). Should return Ok.
        let result = executor.cleanup(&env).await;
        assert!(result.is_ok());
    }

    // --- cleanup: nonexistent runtime logs warning but returns Ok ---

    #[tokio::test]
    async fn test_cleanup_with_nonexistent_runtime_logs_warning() {
        let mut config = make_config();
        config.runtime = "nonexistent-runtime-xyz".to_string();
        let executor = ContainerExecutor::new(config, Language::Python);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: Some("skilldo-test-fake123".to_string()),
            dependencies: vec![],
        };
        // Runtime binary doesn't exist → rm -f spawns fails → warn! logged → Ok
        let result = executor.cleanup(&env).await;
        assert!(result.is_ok());
    }

    // --- run_with_timeout: nonexistent runtime logs kill warning ---

    #[tokio::test]
    async fn test_run_with_timeout_kill_error_logged() {
        let mut config = make_config();
        config.runtime = "nonexistent-runtime-xyz".to_string();
        let executor = ContainerExecutor::new(config, Language::Python);
        // Command that can't be spawned → run_cmd_with_timeout returns Err →
        // kill fallback also fails (nonexistent runtime) → warn! logged
        let cmd = tokio::process::Command::new("nonexistent-binary-xyz");
        let result = executor
            .run_with_timeout(cmd, Duration::from_secs(1), "fake-container")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_with_timeout_kill_succeeds_but_cmd_fails() {
        let mut config = make_config();
        // Use /bin/true as runtime — `true kill fake` spawns OK (exits 0)
        config.runtime = "true".to_string();
        let executor = ContainerExecutor::new(config, Language::Python);
        // Inner command fails to spawn → error path → kill with `true` succeeds
        let cmd = tokio::process::Command::new("nonexistent-binary-xyz");
        let result = executor
            .run_with_timeout(cmd, Duration::from_secs(1), "fake-container")
            .await;
        assert!(result.is_err());
    }

    // --- run_code: non-Python generates run.sh ---

    #[tokio::test]
    async fn test_run_code_js_generates_run_sh() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec!["express".to_string()],
        };
        let _ = executor.run_code(&env, "console.log('hi')").await;
        let script_path = env.temp_dir.path().join("run.sh");
        assert!(script_path.exists());
        let script = fs::read_to_string(&script_path).unwrap();
        assert!(script.contains("#!/bin/sh"));
        assert!(script
            .contains("npm install --no-save --ignore-scripts --no-audit --no-fund -- 'express'"));
        assert!(script.contains("node test.js"));
    }

    #[tokio::test]
    async fn test_run_code_python_does_not_generate_run_sh() {
        let executor = ContainerExecutor::new(make_config(), Language::Python);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let _ = executor.run_code(&env, "print('hello')").await;
        let script_path = env.temp_dir.path().join("run.sh");
        // Python uses `uv run test.py` directly, no run.sh
        assert!(!script_path.exists());
    }

    // --- extra_env config ---

    #[test]
    fn test_config_with_extra_env() {
        let mut config = make_config();
        config
            .extra_env
            .insert("HTTP_PROXY".to_string(), "http://proxy:8080".to_string());
        config
            .extra_env
            .insert("UV_INDEX".to_string(), "https://pypi.corp.com".to_string());
        let executor = ContainerExecutor::new(config, Language::Python);
        assert_eq!(executor.config.extra_env.len(), 2);
        assert_eq!(
            executor.config.extra_env.get("HTTP_PROXY").unwrap(),
            "http://proxy:8080"
        );
    }

    // --- ContainerExecutor field access ---

    #[test]
    fn test_executor_stores_config_fields() {
        let mut config = make_config();
        config.timeout = 120;
        config.cleanup = false;
        let executor = ContainerExecutor::new(config, Language::Rust);
        assert_eq!(executor.language, Language::Rust);
        assert_eq!(executor.config.timeout, 120);
        assert!(!executor.config.cleanup);
    }

    // --- generate_container_script: install_cmd is empty for non-JS/non-Python ---

    #[test]
    fn test_container_script_rust_no_install_line() {
        let executor = ContainerExecutor::new(make_config(), Language::Rust);
        let script = executor.generate_container_script(&[]).unwrap();
        let lines: Vec<&str> = script.lines().collect();
        // Lines should be: #!/bin/sh, set -e, cd /workspace, <empty>, rustc...
        assert!(lines.iter().any(|l| l.contains("rustc")));
        // No npm/pip/cargo install lines
        assert!(!script.contains("install"));
    }

    // --- generate_node_install_script: dep with version range ---

    #[test]
    fn test_node_install_script_version_range() {
        let executor = ContainerExecutor::new(make_config(), Language::JavaScript);
        let deps = vec!["express@>=4.0.0".to_string()];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert!(script.contains("express@>=4.0.0"));
    }

    // --- Container name from setup_environment has no dots ---

    #[test]
    fn test_container_name_format_no_special_chars() {
        // Replicate the naming logic used in setup_environment
        let temp_dir = TempDir::new().unwrap();
        let dir_name = temp_dir
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let container_name = format!("skilldo-test-{}", dir_name.replace('.', ""));

        // Must start with prefix
        assert!(container_name.starts_with("skilldo-test-"));
        // Must not contain dots (invalid in container names)
        assert!(!container_name.contains('.'));
        // Must not be empty after prefix
        assert!(container_name.len() > "skilldo-test-".len());
    }

    // --- run_code for rust generates run.sh with correct permissions ---

    #[cfg(unix)]
    #[tokio::test]
    async fn test_run_code_rust_run_sh_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let executor = ContainerExecutor::new(make_config(), Language::Rust);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let _ = executor.run_code(&env, "fn main() {}").await;
        let script_path = env.temp_dir.path().join("run.sh");
        assert!(script_path.exists());
        let perms = fs::metadata(&script_path).unwrap().permissions();
        // Check that owner-execute bit is set
        assert!(perms.mode() & 0o100 != 0, "run.sh should be executable");
    }

    // --- local-mount missing source_path ---

    #[tokio::test]
    async fn test_run_code_local_mount_missing_source_path() {
        let mut config = make_config();
        config.install_source = InstallSource::LocalMount;
        config.source_path = None;
        let executor = ContainerExecutor::new(config, Language::Python);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: Some("test-container".to_string()),
            dependencies: vec![],
        };
        let result = executor.run_code(&env, "print('hello')").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("source_path is required"));
    }

    #[test]
    fn test_warn_dangerous_env_var_detects_ld_preload() {
        // Should not panic; just logs a warning
        warn_dangerous_env_var("LD_PRELOAD");
        warn_dangerous_env_var("ld_preload"); // case-insensitive
    }

    #[test]
    fn test_warn_dangerous_env_var_allows_safe_vars() {
        // Safe vars should not trigger warning (no panic or error)
        warn_dangerous_env_var("PIP_INDEX_URL");
        warn_dangerous_env_var("HTTPS_PROXY");
        warn_dangerous_env_var("MY_CUSTOM_VAR");
    }

    #[test]
    fn test_dangerous_env_vars_list() {
        assert!(DANGEROUS_ENV_VARS.contains(&"LD_PRELOAD"));
        assert!(DANGEROUS_ENV_VARS.contains(&"PATH"));
        assert!(!DANGEROUS_ENV_VARS.contains(&"PYTHONPATH"));
    }

    // --- generate_go_install_script ---

    #[test]
    fn test_go_install_script_empty_deps() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let script = executor.generate_go_install_script(&[]).unwrap();
        assert!(script.contains("go mod init"));
        assert!(!script.contains("go get"));
    }

    #[test]
    fn test_go_install_script_single_dep() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let deps = vec!["github.com/go-chi/chi/v5".to_string()];
        let script = executor.generate_go_install_script(&deps).unwrap();
        assert!(script.contains("go mod init"));
        assert!(script.contains("go get 'github.com/go-chi/chi/v5'"));
    }

    #[test]
    fn test_go_install_script_multiple_deps() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let deps = vec![
            "github.com/go-chi/chi/v5".to_string(),
            "github.com/rs/zerolog".to_string(),
        ];
        let script = executor.generate_go_install_script(&deps).unwrap();
        assert!(script.contains("go get 'github.com/go-chi/chi/v5'"));
        assert!(script.contains("go get 'github.com/rs/zerolog'"));
    }

    #[test]
    fn test_go_install_script_skips_init_if_gomod_exists() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let script = executor.generate_go_install_script(&[]).unwrap();
        assert!(
            script.contains("[ -f go.mod ]"),
            "should guard go mod init behind existence check"
        );
    }

    #[test]
    fn test_go_install_script_rejects_injection() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let deps = vec!["github.com/foo; rm -rf /".to_string()];
        assert!(executor.generate_go_install_script(&deps).is_err());
    }

    // --- Go container script with run.sh generation ---

    #[tokio::test]
    async fn test_go_run_code_generates_run_sh() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            interpreter_path: None,
            container_name: None,
            dependencies: vec!["github.com/gin-gonic/gin".to_string()],
        };
        let _ = executor
            .run_code(&env, "package main\n\nfunc main() {}")
            .await;
        let script_path = env.temp_dir.path().join("run.sh");
        assert!(script_path.exists());
        let script = fs::read_to_string(&script_path).unwrap();
        assert!(script.contains("go mod init test"));
        assert!(script.contains("go get 'github.com/gin-gonic/gin'"));
        assert!(script.contains("go run main.go"));
    }

    #[test]
    fn test_go_container_script_structure() {
        let executor = ContainerExecutor::new(make_config(), Language::Go);
        let deps = vec!["github.com/spf13/cobra".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("set -e"));
        assert!(script.contains("cd /workspace"));
        assert!(script.contains("go mod init test"));
        assert!(script.contains("go get 'github.com/spf13/cobra'"));
        assert!(script.contains("go run main.go"));
    }

    // --- Java container script tests ---

    #[test]
    fn test_java_container_script_without_deps() {
        let executor = ContainerExecutor::new(make_config(), Language::Java);
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.contains("javac -cp 'deps/*:.' Main.java"));
        assert!(script.contains("java -cp 'deps/*:.' Main"));
        // No mvn/pom lines when no deps
        assert!(!script.contains("pom.xml"));
    }

    #[test]
    fn test_java_container_script_with_deps() {
        let executor = ContainerExecutor::new(make_config(), Language::Java);
        let deps = vec!["com.google.code.gson:gson:2.10.1".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.contains("pom.xml"));
        assert!(script.contains("mvn dependency:copy-dependencies"));
        assert!(script.contains("deps"));
        assert!(script.contains("javac -cp 'deps/*:.' Main.java"));
    }

    #[test]
    fn test_java_container_script_two_part_coord_omits_dep_version() {
        let executor = ContainerExecutor::new(make_config(), Language::Java);
        let deps = vec!["com.google.code.gson:gson".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        // The dependency line should NOT have a <version> tag (project version is OK)
        assert!(
            !script.contains("RELEASE"),
            "two-part coord should not use RELEASE"
        );
        assert!(script.contains("com.google.code.gson"));
        // Verify the dep line doesn't have version but project does
        assert!(script.contains("<artifactId>gson</artifactId>"));
        assert!(!script.contains("<artifactId>gson</artifactId><version>"));
    }

    #[test]
    fn test_java_install_script_rejects_shell_injection() {
        let executor = ContainerExecutor::new(make_config(), Language::Java);
        let deps = vec!["com.evil:lib; rm -rf /".to_string()];
        assert!(executor.generate_java_install_script(&deps).is_err());
    }

    #[test]
    fn test_java_container_script_structure() {
        let executor = ContainerExecutor::new(make_config(), Language::Java);
        let deps = vec!["org.slf4j:slf4j-api:2.0.9".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("set -e"));
        assert!(script.contains("cd /workspace"));
        assert!(script.contains("mkdir -p deps"));
        assert!(script.contains("java -cp 'deps/*:.' Main"));
    }
}
