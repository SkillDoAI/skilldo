//! Container-based code executor for test agent validation
//! Supports Docker, Podman, and other OCI-compatible runtimes

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tracing::{debug, info};

use super::executor::{ExecutionEnv, ExecutionResult};
use super::LanguageExecutor;
use crate::config::ContainerConfig;
use crate::util::{run_cmd_with_timeout, sanitize_dep_name};

pub struct ContainerExecutor {
    config: ContainerConfig,
    language: String,
}

impl ContainerExecutor {
    pub fn new(config: ContainerConfig, language: &str) -> Self {
        Self {
            config,
            language: language.to_string(),
        }
    }

    /// Get the container image for the current language
    fn get_image(&self) -> &str {
        match self.language.as_str() {
            "python" => &self.config.python_image,
            "javascript" | "typescript" => &self.config.javascript_image,
            "rust" => &self.config.rust_image,
            "go" => &self.config.go_image,
            _ => &self.config.python_image, // Default fallback
        }
    }

    /// Check if container runtime is available
    fn check_runtime_available(&self) -> bool {
        Command::new(&self.config.runtime)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Generate dependency installation script for JavaScript/TypeScript
    fn generate_node_install_script(&self, deps: &[String]) -> Result<String> {
        if deps.is_empty() {
            Ok(String::new())
        } else {
            for dep in deps {
                sanitize_dep_name(dep).map_err(|e| anyhow::anyhow!(e))?;
            }
            Ok(format!(
                "npm install --no-save {} > /dev/null 2>&1",
                deps.join(" ")
            ))
        }
    }

    /// Generate run.sh for non-Python languages (JS, Rust, Go)
    /// Python uses `uv run test.py` directly — no run.sh needed
    fn generate_container_script(&self, deps: &[String]) -> Result<String> {
        let install_cmd = match self.language.as_str() {
            "javascript" | "typescript" => self.generate_node_install_script(deps)?,
            _ => String::new(),
        };

        let code_file = match self.language.as_str() {
            "javascript" | "typescript" => "test.js",
            "rust" => "main.rs",
            "go" => "main.go",
            _ => "test.py",
        };

        let run_line = match self.language.as_str() {
            "javascript" | "typescript" => format!("node {}", code_file),
            "rust" => format!("rustc {} -o main && ./main", code_file),
            "go" => format!("go run {}", code_file),
            _ => format!("python {}", code_file),
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

impl LanguageExecutor for ContainerExecutor {
    fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv> {
        info!(
            "Setting up {} environment with {} dependencies",
            self.language,
            deps.len()
        );
        // Check if runtime is available
        if !self.check_runtime_available() {
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
            python_path: None,
            container_name: Some(container_name),
            dependencies: deps.to_vec(),
        })
    }

    fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult> {
        debug!("Running {} code ({} bytes)", self.language, code.len());

        let is_python = matches!(self.language.as_str(), "python");

        // Write code to file
        let code_file = match self.language.as_str() {
            "python" => "test.py",
            "javascript" | "typescript" => "test.js",
            "rust" => "main.rs",
            "go" => "main.go",
            _ => "test.py",
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
            .status();

        let mut cmd = Command::new(&self.config.runtime);
        cmd.arg("run");

        if self.config.cleanup {
            cmd.arg("--rm");
        }

        cmd.arg("--name")
            .arg(container_name)
            .arg("-v")
            .arg(format!("{}:/workspace", env.temp_dir.path().display()));

        // Mount source repo for local modes (local-install, local-mount).
        // No allowlist check here — install_source is a user-controlled TOML config
        // value, not untrusted input. Invalid values simply skip the PYTHONPATH
        // optimization but still require source_path, so misconfigs surface early.
        if self.config.install_source != "registry" {
            let source = self.config.source_path.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "source_path is required when install_source is '{}'",
                    self.config.install_source
                )
            })?;
            cmd.arg("-v").arg(format!("{}:/src:ro", source));
        }

        // Set PYTHONPATH for local-mount mode
        if self.config.install_source == "local-mount" {
            cmd.arg("-e").arg("PYTHONPATH=/src");
        }

        // Pass extra environment variables (private registries, proxies, etc.)
        for (key, value) in &self.config.extra_env {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }

        cmd.arg("-w").arg("/workspace").arg(image);

        // Python: use `uv run test.py` (default image has uv pre-installed)
        //   uv reads PEP 723 inline script metadata for deps
        //   local-install: pip install /src first, then run
        // Other languages: `sh run.sh` — traditional install + run
        if is_python {
            match self.config.install_source.as_str() {
                "local-install" => {
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
        let output = self.run_with_timeout(
            cmd,
            Duration::from_secs(self.config.timeout),
            container_name,
        )?;

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

    fn cleanup(&self, env: &ExecutionEnv) -> Result<()> {
        if !self.config.cleanup {
            debug!("Cleanup disabled, skipping");
            return Ok(());
        }

        let container_name = env
            .container_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Container name not set in execution environment"))?;

        // Force remove container if it's still running
        let _ = Command::new(&self.config.runtime)
            .arg("rm")
            .arg("-f")
            .arg(container_name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        debug!("Container {} cleaned up", container_name);
        Ok(())
    }
}

impl ContainerExecutor {
    /// Run a command with a timeout, also killing the container on failure/timeout
    fn run_with_timeout(
        &self,
        cmd: Command,
        timeout: Duration,
        container_name: &str,
    ) -> Result<std::process::Output> {
        match run_cmd_with_timeout(cmd, timeout) {
            Ok(output) => Ok(output),
            Err(e) => {
                // Also kill the container on any error (timeout or otherwise)
                let _ = Command::new(&self.config.runtime)
                    .arg("kill")
                    .arg(container_name)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_node_install_script() {
        let config = ContainerConfig {
            runtime: "podman".to_string(),
            python_image: "python:3.11-alpine".to_string(),
            javascript_image: "node:18-alpine".to_string(),
            rust_image: "rust:1.70-alpine".to_string(),
            go_image: "golang:1.20-alpine".to_string(),
            timeout: 60,
            cleanup: true,
            install_source: "registry".to_string(),
            source_path: None,
            extra_env: std::collections::HashMap::new(),
        };

        let executor = ContainerExecutor::new(config, "javascript");

        let deps = vec!["express".to_string(), "cors".to_string()];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert!(script.contains("npm install"));
        assert!(script.contains("express"));
        assert!(script.contains("cors"));

        // Empty deps
        let empty = executor.generate_node_install_script(&[]).unwrap();
        assert!(empty.is_empty());

        // Shell injection rejected
        let bad = vec!["express; rm -rf /".to_string()];
        assert!(executor.generate_node_install_script(&bad).is_err());
    }

    fn make_config() -> ContainerConfig {
        ContainerConfig {
            runtime: "podman".to_string(),
            python_image: "python:3.11-alpine".to_string(),
            javascript_image: "node:18-alpine".to_string(),
            rust_image: "rust:1.70-alpine".to_string(),
            go_image: "golang:1.20-alpine".to_string(),
            timeout: 60,
            cleanup: true,
            install_source: "registry".to_string(),
            source_path: None,
            extra_env: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_rust_container_script() {
        let executor = ContainerExecutor::new(make_config(), "rust");
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.contains("rustc main.rs -o main && ./main"));
    }

    #[test]
    fn test_go_container_script() {
        let executor = ContainerExecutor::new(make_config(), "go");
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.contains("go run main.go"));
    }

    #[test]
    fn test_python_container_script() {
        let executor = ContainerExecutor::new(make_config(), "python");
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.contains("python test.py"));
    }

    #[test]
    fn test_get_image_python() {
        let executor = ContainerExecutor::new(make_config(), "python");
        assert_eq!(executor.get_image(), "python:3.11-alpine");
    }

    #[test]
    fn test_get_image_javascript() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        assert_eq!(executor.get_image(), "node:18-alpine");
    }

    #[test]
    fn test_get_image_typescript() {
        let executor = ContainerExecutor::new(make_config(), "typescript");
        assert_eq!(executor.get_image(), "node:18-alpine");
    }

    #[test]
    fn test_get_image_rust() {
        let executor = ContainerExecutor::new(make_config(), "rust");
        assert_eq!(executor.get_image(), "rust:1.70-alpine");
    }

    #[test]
    fn test_get_image_go() {
        let executor = ContainerExecutor::new(make_config(), "go");
        assert_eq!(executor.get_image(), "golang:1.20-alpine");
    }

    #[test]
    fn test_get_image_unknown_defaults_to_python() {
        let executor = ContainerExecutor::new(make_config(), "haskell");
        assert_eq!(executor.get_image(), "python:3.11-alpine");
    }

    #[test]
    fn test_javascript_container_script_with_deps() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let deps = vec!["express".to_string(), "cors".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.contains("npm install --no-save express cors"));
        assert!(script.contains("node test.js"));
    }

    #[test]
    fn test_unknown_language_container_script() {
        let executor = ContainerExecutor::new(make_config(), "haskell");
        let script = executor.generate_container_script(&[]).unwrap();
        // Unknown falls through to python defaults
        assert!(script.contains("python test.py"));
    }

    #[test]
    fn test_cleanup_no_container_name() {
        let executor = ContainerExecutor::new(make_config(), "python");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let result = executor.cleanup(&env);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Container name not set"));
    }

    #[test]
    fn test_cleanup_disabled() {
        let mut config = make_config();
        config.cleanup = false;
        let executor = ContainerExecutor::new(config, "python");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: Some("test-container".to_string()),
            dependencies: vec![],
        };
        // Should return Ok without trying to rm anything
        assert!(executor.cleanup(&env).is_ok());
    }

    #[test]
    fn test_run_code_no_container_name() {
        let executor = ContainerExecutor::new(make_config(), "python");
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
            .contains("Container name not set"));
    }

    #[test]
    fn test_make_config_has_install_source() {
        let config = make_config();
        assert_eq!(config.install_source, "registry");
        assert!(config.source_path.is_none());
    }

    #[test]
    fn test_local_install_config() {
        let mut config = make_config();
        config.install_source = "local-install".to_string();
        config.source_path = Some("/tmp/my-lib".to_string());
        let executor = ContainerExecutor::new(config, "python");
        assert_eq!(executor.config.install_source, "local-install");
    }

    #[test]
    fn test_run_code_local_install_missing_source_path() {
        let mut config = make_config();
        config.install_source = "local-install".to_string();
        config.source_path = None;
        let executor = ContainerExecutor::new(config, "python");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: Some("test-container".to_string()),
            dependencies: vec![],
        };
        let result = executor.run_code(&env, "print('hello')");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("source_path is required"));
    }

    #[test]
    fn test_local_mount_config() {
        let mut config = make_config();
        config.install_source = "local-mount".to_string();
        config.source_path = Some("/tmp/my-lib".to_string());
        let executor = ContainerExecutor::new(config, "python");
        assert_eq!(executor.config.install_source, "local-mount");
    }

    // --- ContainerExecutor::new() with Default config ---

    #[test]
    fn test_new_with_default_config() {
        let config = ContainerConfig::default();
        let executor = ContainerExecutor::new(config, "python");
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
        assert_eq!(executor.config.javascript_image, "node:20-slim");
        assert_eq!(executor.config.rust_image, "rust:1.75-slim");
        assert_eq!(executor.config.go_image, "golang:1.21-alpine");
        assert_eq!(executor.config.timeout, 60);
        assert!(executor.config.cleanup);
        assert_eq!(executor.config.install_source, "registry");
        assert!(executor.config.source_path.is_none());
        assert!(executor.config.extra_env.is_empty());
        assert_eq!(executor.language, "python");
    }

    #[test]
    fn test_new_stores_language() {
        let executor = ContainerExecutor::new(make_config(), "go");
        assert_eq!(executor.language, "go");
    }

    // --- generate_container_script: JS/TS without deps ---

    #[test]
    fn test_javascript_container_script_without_deps() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.contains("node test.js"));
        // No npm install line when no deps
        assert!(!script.contains("npm install"));
    }

    #[test]
    fn test_typescript_container_script_without_deps() {
        let executor = ContainerExecutor::new(make_config(), "typescript");
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.contains("node test.js"));
        assert!(!script.contains("npm install"));
    }

    #[test]
    fn test_typescript_container_script_with_deps() {
        let executor = ContainerExecutor::new(make_config(), "typescript");
        let deps = vec!["lodash".to_string(), "axios".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.contains("npm install --no-save lodash axios"));
        assert!(script.contains("node test.js"));
    }

    // --- generate_container_script: Rust/Go with deps (deps should be ignored) ---

    #[test]
    fn test_rust_container_script_with_deps_ignores_them() {
        let executor = ContainerExecutor::new(make_config(), "rust");
        let deps = vec!["serde".to_string(), "tokio".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.contains("rustc main.rs -o main && ./main"));
        // Rust has no install step, deps are ignored
        assert!(!script.contains("cargo install"));
        assert!(!script.contains("serde"));
    }

    #[test]
    fn test_go_container_script_with_deps_ignores_them() {
        let executor = ContainerExecutor::new(make_config(), "go");
        let deps = vec!["github.com/gin-gonic/gin".to_string()];
        let script = executor.generate_container_script(&deps).unwrap();
        assert!(script.contains("go run main.go"));
        assert!(!script.contains("go get"));
        assert!(!script.contains("gin"));
    }

    // --- Script content validation (shebang, set -e, cd /workspace) ---

    #[test]
    fn test_container_script_has_shebang_and_set_e() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("set -e"));
        assert!(script.contains("cd /workspace"));
    }

    #[test]
    fn test_container_script_structure_rust() {
        let executor = ContainerExecutor::new(make_config(), "rust");
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("set -e"));
        assert!(script.contains("cd /workspace"));
    }

    #[test]
    fn test_container_script_structure_go() {
        let executor = ContainerExecutor::new(make_config(), "go");
        let script = executor.generate_container_script(&[]).unwrap();
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("set -e"));
        assert!(script.contains("cd /workspace"));
    }

    // --- generate_node_install_script: format and sanitization ---

    #[test]
    fn test_node_install_script_single_dep() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let deps = vec!["express".to_string()];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert_eq!(script, "npm install --no-save express > /dev/null 2>&1");
    }

    #[test]
    fn test_node_install_script_multiple_deps_format() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let deps = vec![
            "express".to_string(),
            "cors".to_string(),
            "helmet".to_string(),
        ];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert_eq!(
            script,
            "npm install --no-save express cors helmet > /dev/null 2>&1"
        );
    }

    #[test]
    fn test_node_install_script_scoped_package() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let deps = vec!["@types/node".to_string()];
        let script = executor.generate_node_install_script(&deps).unwrap();
        assert!(script.contains("@types/node"));
    }

    #[test]
    fn test_node_install_script_rejects_backtick() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let deps = vec!["`whoami`".to_string()];
        assert!(executor.generate_node_install_script(&deps).is_err());
    }

    #[test]
    fn test_node_install_script_rejects_dollar_sign() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let deps = vec!["$(rm -rf /)".to_string()];
        assert!(executor.generate_node_install_script(&deps).is_err());
    }

    #[test]
    fn test_node_install_script_rejects_pipe() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let deps = vec!["express | cat /etc/passwd".to_string()];
        assert!(executor.generate_node_install_script(&deps).is_err());
    }

    #[test]
    fn test_node_install_script_rejects_ampersand() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let deps = vec!["express && rm -rf /".to_string()];
        assert!(executor.generate_node_install_script(&deps).is_err());
    }

    #[test]
    fn test_node_install_script_allows_version_constraint() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
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
        let executor = ContainerExecutor::new(ContainerConfig::default(), "python");
        assert_eq!(
            executor.get_image(),
            "ghcr.io/astral-sh/uv:python3.11-bookworm-slim"
        );
    }

    #[test]
    fn test_get_image_with_default_config_javascript() {
        let executor = ContainerExecutor::new(ContainerConfig::default(), "javascript");
        assert_eq!(executor.get_image(), "node:20-slim");
    }

    #[test]
    fn test_get_image_with_default_config_rust() {
        let executor = ContainerExecutor::new(ContainerConfig::default(), "rust");
        assert_eq!(executor.get_image(), "rust:1.75-slim");
    }

    #[test]
    fn test_get_image_with_default_config_go() {
        let executor = ContainerExecutor::new(ContainerConfig::default(), "go");
        assert_eq!(executor.get_image(), "golang:1.21-alpine");
    }

    #[test]
    fn test_get_image_with_default_config_unknown_falls_back_to_python() {
        let executor = ContainerExecutor::new(ContainerConfig::default(), "ruby");
        assert_eq!(
            executor.get_image(),
            "ghcr.io/astral-sh/uv:python3.11-bookworm-slim"
        );
    }

    // --- setup_environment: runtime not found ---

    #[test]
    fn test_setup_environment_missing_runtime() {
        let mut config = make_config();
        config.runtime = "nonexistent-runtime-xyz".to_string();
        let executor = ContainerExecutor::new(config, "python");
        let result = executor.setup_environment(&[]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("runtime not found"),
            "Expected 'runtime not found' in: {}",
            err_msg
        );
    }

    // --- setup_environment: dependencies stored ---

    #[test]
    fn test_setup_environment_stores_dependencies() {
        // This requires a real runtime, but we can test with the actual podman/docker
        // if available. If not, this test validates the error path.
        let executor = ContainerExecutor::new(make_config(), "python");
        let deps = vec!["requests".to_string(), "flask".to_string()];
        match executor.setup_environment(&deps) {
            Ok(env) => {
                assert_eq!(env.dependencies, deps);
                assert!(env.container_name.is_some());
                assert!(env.python_path.is_none());
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

    #[test]
    fn test_cleanup_with_container_name_and_cleanup_enabled() {
        let executor = ContainerExecutor::new(make_config(), "python");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: Some("skilldo-test-fake123".to_string()),
            dependencies: vec![],
        };
        // cleanup calls `podman rm -f <name>` — the container doesn't exist, but
        // the command failure is silently ignored (let _ = ...). Should return Ok.
        let result = executor.cleanup(&env);
        assert!(result.is_ok());
    }

    // --- run_code: code file selection for different languages ---

    #[test]
    fn test_run_code_writes_js_file() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        // run_code writes the code file before checking container_name,
        // so we can verify the file was written even though it errors.
        let _ = executor.run_code(&env, "console.log('hi')");
        let code_path = env.temp_dir.path().join("test.js");
        assert!(code_path.exists());
        let content = fs::read_to_string(&code_path).unwrap();
        assert_eq!(content, "console.log('hi')");
    }

    #[test]
    fn test_run_code_writes_rust_file() {
        let executor = ContainerExecutor::new(make_config(), "rust");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let _ = executor.run_code(&env, "fn main() {}");
        let code_path = env.temp_dir.path().join("main.rs");
        assert!(code_path.exists());
        let content = fs::read_to_string(&code_path).unwrap();
        assert_eq!(content, "fn main() {}");
    }

    #[test]
    fn test_run_code_writes_go_file() {
        let executor = ContainerExecutor::new(make_config(), "go");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let _ = executor.run_code(&env, "package main");
        let code_path = env.temp_dir.path().join("main.go");
        assert!(code_path.exists());
        let content = fs::read_to_string(&code_path).unwrap();
        assert_eq!(content, "package main");
    }

    #[test]
    fn test_run_code_writes_python_file() {
        let executor = ContainerExecutor::new(make_config(), "python");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let _ = executor.run_code(&env, "print('hello')");
        let code_path = env.temp_dir.path().join("test.py");
        assert!(code_path.exists());
        let content = fs::read_to_string(&code_path).unwrap();
        assert_eq!(content, "print('hello')");
    }

    #[test]
    fn test_run_code_unknown_language_writes_test_py() {
        let executor = ContainerExecutor::new(make_config(), "haskell");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let _ = executor.run_code(&env, "main = putStrLn");
        let code_path = env.temp_dir.path().join("test.py");
        assert!(code_path.exists());
    }

    // --- run_code: non-Python generates run.sh ---

    #[test]
    fn test_run_code_js_generates_run_sh() {
        let executor = ContainerExecutor::new(make_config(), "javascript");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec!["express".to_string()],
        };
        let _ = executor.run_code(&env, "console.log('hi')");
        let script_path = env.temp_dir.path().join("run.sh");
        assert!(script_path.exists());
        let script = fs::read_to_string(&script_path).unwrap();
        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("npm install --no-save express"));
        assert!(script.contains("node test.js"));
    }

    #[test]
    fn test_run_code_python_does_not_generate_run_sh() {
        let executor = ContainerExecutor::new(make_config(), "python");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let _ = executor.run_code(&env, "print('hello')");
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
        let executor = ContainerExecutor::new(config, "python");
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
        let executor = ContainerExecutor::new(config, "rust");
        assert_eq!(executor.language, "rust");
        assert_eq!(executor.config.timeout, 120);
        assert!(!executor.config.cleanup);
    }

    // --- generate_container_script: install_cmd is empty for non-JS/non-Python ---

    #[test]
    fn test_container_script_rust_no_install_line() {
        let executor = ContainerExecutor::new(make_config(), "rust");
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
        let executor = ContainerExecutor::new(make_config(), "javascript");
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
    #[test]
    fn test_run_code_rust_run_sh_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let executor = ContainerExecutor::new(make_config(), "rust");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: None,
            dependencies: vec![],
        };
        let _ = executor.run_code(&env, "fn main() {}");
        let script_path = env.temp_dir.path().join("run.sh");
        assert!(script_path.exists());
        let perms = fs::metadata(&script_path).unwrap().permissions();
        // Check that owner-execute bit is set
        assert!(perms.mode() & 0o100 != 0, "run.sh should be executable");
    }

    // --- local-mount missing source_path ---

    #[test]
    fn test_run_code_local_mount_missing_source_path() {
        let mut config = make_config();
        config.install_source = "local-mount".to_string();
        config.source_path = None;
        let executor = ContainerExecutor::new(config, "python");
        let temp_dir = TempDir::new().unwrap();
        let env = ExecutionEnv {
            temp_dir,
            python_path: None,
            container_name: Some("test-container".to_string()),
            dependencies: vec![],
        };
        let result = executor.run_code(&env, "print('hello')");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("source_path is required"));
    }
}
