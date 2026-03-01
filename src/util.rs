//! Shared utilities for the skilldo codebase

use anyhow::{bail, Context, Result};
use std::fmt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// A string wrapper that masks its contents in Debug/Display output.
/// Prevents accidental logging of API keys and other secrets.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    #[allow(dead_code)]
    pub fn new(s: String) -> Self {
        Self(s)
    }

    /// Intentionally access the raw secret value (for headers, URLs, etc.)
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "***")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "***")
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl PartialEq<&str> for SecretString {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Validate a dependency name is safe for use in shell commands and config files.
/// Rejects names containing shell metacharacters, spaces (flag injection vector),
/// or control sequences.
/// Valid: alphanumeric, hyphens, underscores, dots, forward slashes (scoped npm
/// like `@scope/pkg`), square brackets, commas, at signs, and version constraint
/// operators (`>=`, `<`, `~=`, `^`, `!`).
/// Spaces are NOT allowed — they enable flag injection (e.g., `pkg --malicious`).
pub fn sanitize_dep_name(dep: &str) -> Result<&str, String> {
    if dep.is_empty() {
        return Err("Empty dependency name".to_string());
    }
    // Reject leading hyphens (flag injection: `-e malicious`)
    if dep.starts_with('-') {
        return Err(format!(
            "Dependency name starts with '-' (possible flag injection): {}",
            dep
        ));
    }
    for ch in dep.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => {}
            '-' | '_' | '.' | '/' | '[' | ']' | ',' | '@' => {}
            '>' | '<' | '=' | '!' | '~' | '^' => {} // version constraints
            _ => {
                return Err(format!(
                    "Invalid character '{}' in dependency name: {}",
                    ch, dep
                ));
            }
        }
    }
    Ok(dep)
}

/// Calculate file priority for source file reading order.
/// Lower values = higher priority (read first).
/// Uses `Path::components()` for separator-agnostic matching (works on Unix, macOS, WSL).
pub fn calculate_file_priority(path: &Path, repo_path: &Path) -> i32 {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let relative = path.strip_prefix(repo_path).unwrap_or(path);
    let depth = relative.components().count();

    // Check for internal/private path segments using components (separator-agnostic)
    let is_internal = relative.components().any(|c| {
        let s = c.as_os_str().to_str().unwrap_or("");
        matches!(
            s,
            "_internal" | "_impl" | "testing" | "tests" | "benchmarks" | "tools" | "scripts"
        )
    });

    // Priority 0: Top-level package __init__.py (torch/__init__.py)
    if file_name == "__init__.py" && depth == 2 {
        return 0;
    }

    // Priority 10: Subpackage __init__.py files (torch/nn/__init__.py)
    if file_name == "__init__.py" && depth > 2 {
        return 10;
    }

    // Priority 100: Internal/private files (read last if at all)
    if file_name.starts_with('_') || is_internal {
        return 100;
    }

    // Priority 20: Public top-level modules (torch/nn.py)
    if !file_name.starts_with('_') && depth == 2 {
        return 20;
    }

    // Priority 30: Public subpackage modules (torch/nn/functional.py)
    if !file_name.starts_with('_') && depth == 3 {
        return 30;
    }

    // Priority 50: Everything else (deeper submodules)
    50
}

/// Kill a process by PID. Uses SIGKILL on Unix (Linux, macOS, WSL).
#[cfg(unix)]
fn kill_process(pid: u32) {
    // kill -9 sends SIGKILL to the process.
    // This works on Linux, macOS, and WSL.
    let _ = Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(unix))]
fn kill_process(pid: u32) {
    // On Windows (non-WSL), use taskkill
    let _ = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Run a command with a timeout, killing the child process on expiry.
/// Spawns the command, waits up to `timeout` for it to finish.
/// On timeout, kills the child process and returns an error.
pub fn run_cmd_with_timeout(mut cmd: Command, timeout: Duration) -> Result<std::process::Output> {
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn command")?;

    let pid = child.id();
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = sender.send(result);
    });

    match receiver.recv_timeout(timeout) {
        Ok(result) => result.context("Failed to execute command"),
        Err(_) => {
            kill_process(pid);
            bail!("Command timed out after {:?}", timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_string_hides_in_debug() {
        let secret = SecretString::new("my-api-key-123".to_string());
        let debug_output = format!("{:?}", secret);
        assert_eq!(debug_output, "***");
        assert!(!debug_output.contains("my-api-key"));
    }

    #[test]
    fn test_secret_string_hides_in_display() {
        let secret = SecretString::new("my-api-key-123".to_string());
        let display_output = format!("{}", secret);
        assert_eq!(display_output, "***");
    }

    #[test]
    fn test_secret_string_expose_returns_value() {
        let secret = SecretString::new("my-api-key-123".to_string());
        assert_eq!(secret.expose(), "my-api-key-123");
    }

    #[test]
    fn test_secret_string_from_string() {
        let secret: SecretString = "test-key".to_string().into();
        assert_eq!(secret.expose(), "test-key");
    }

    #[test]
    fn test_secret_string_partial_eq() {
        let secret = SecretString::new("test-key".to_string());
        assert!(secret == "test-key");
    }

    #[test]
    fn test_sanitize_dep_name_valid() {
        assert!(sanitize_dep_name("pandas").is_ok());
        assert!(sanitize_dep_name("scikit-learn").is_ok());
        assert!(sanitize_dep_name("pandas>=2.0,<3").is_ok());
        assert!(sanitize_dep_name("@scope/package").is_ok());
        assert!(sanitize_dep_name("numpy[extra]").is_ok());
        assert!(sanitize_dep_name("flask~=2.0").is_ok());
    }

    #[test]
    fn test_sanitize_dep_name_rejects_shell_injection() {
        assert!(sanitize_dep_name("express; rm -rf /").is_err());
        assert!(sanitize_dep_name("pkg$(whoami)").is_err());
        assert!(sanitize_dep_name("pkg`id`").is_err());
        assert!(sanitize_dep_name("pkg|cat /etc/passwd").is_err());
        assert!(sanitize_dep_name("pkg&& evil").is_err());
        assert!(sanitize_dep_name("").is_err());
    }

    #[test]
    fn test_sanitize_dep_name_rejects_flag_injection() {
        // Leading hyphen — could inject flags like `-e malicious`
        assert!(sanitize_dep_name("-e").is_err());
        assert!(sanitize_dep_name("--malicious-flag").is_err());
        // Spaces enable flag injection: `pkg --malicious`
        assert!(sanitize_dep_name("pkg --malicious").is_err());
        assert!(sanitize_dep_name("express rm").is_err());
    }
}
