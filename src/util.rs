//! Shared utilities for the skilldo codebase

use anyhow::{bail, Context, Result};
use std::fmt;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Check if a line starts a fenced code block (backtick or tilde, per CommonMark).
pub fn is_fence_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Detect which fence character opens/closes a code block on this line.
/// Returns `Some('`')` for backtick fences, `Some('~')` for tilde fences,
/// or `None` if the line is not a fence line.
pub fn detect_fence_char(line: &str) -> Option<char> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") {
        Some('`')
    } else if trimmed.starts_with("~~~") {
        Some('~')
    } else {
        None
    }
}

/// Build a per-line boolean vec indicating whether each line is inside a
/// fenced code block. Follows CommonMark: a closing fence must use the
/// same character (backtick or tilde) that opened the block, and be at
/// least as long as the opening fence.
pub fn compute_code_block_lines(lines: &[&str]) -> Vec<bool> {
    let mut result = Vec::with_capacity(lines.len());
    let mut open_fence: Option<(char, usize)> = None; // (char, length)
    for line in lines {
        if let Some(ch) = detect_fence_char(line) {
            let run_len = line.trim_start().chars().take_while(|&c| c == ch).count();
            if let Some((open_ch, open_len)) = open_fence {
                // Inside a block -- only close if same fence character
                // and closing fence is at least as long as the opening fence
                if ch == open_ch && run_len >= open_len {
                    open_fence = None;
                }
            } else {
                open_fence = Some((ch, run_len));
            }
        }
        result.push(open_fence.is_some());
    }
    result
}

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
            "_internal"
                | "_impl"
                | "test"
                | "testing"
                | "tests"
                | "benchmarks"
                | "tools"
                | "scripts"
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

/// Run a command with a timeout, killing the child on expiry.
/// Uses `tokio::process` with `kill_on_drop(true)` — no orphaned threads,
/// no setsid/process-group gymnastics, no LLVM-profdata deadlocks.
pub async fn run_cmd_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Result<std::process::Output> {
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to spawn command")?;

    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(result) => result.context("Failed to execute command"),
        Err(_) => {
            // Timeout expired. `kill_on_drop` will send SIGKILL when `child`
            // is dropped (which happens here at scope exit).
            bail!("Command timed out after {:?}", timeout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_fence_char_backtick() {
        assert_eq!(detect_fence_char("```python"), Some('`'));
        assert_eq!(detect_fence_char("```"), Some('`'));
        assert_eq!(detect_fence_char("  ```"), Some('`'));
    }

    #[test]
    fn detect_fence_char_tilde() {
        assert_eq!(detect_fence_char("~~~python"), Some('~'));
        assert_eq!(detect_fence_char("~~~"), Some('~'));
        assert_eq!(detect_fence_char("  ~~~"), Some('~'));
    }

    #[test]
    fn detect_fence_char_none() {
        assert_eq!(detect_fence_char("normal text"), None);
        assert_eq!(detect_fence_char("``not enough"), None);
        assert_eq!(detect_fence_char("~~not enough"), None);
    }

    #[test]
    fn code_block_lines_backtick_open_close() {
        let lines = vec!["prose", "```python", "code", "```", "more prose"];
        let result = compute_code_block_lines(&lines);
        // prose=false, ```=true(opening), code=true, ```=false(closing), more=false
        assert_eq!(result, vec![false, true, true, false, false]);
    }

    #[test]
    fn code_block_lines_tilde_open_close() {
        let lines = vec!["prose", "~~~python", "code", "~~~", "more prose"];
        let result = compute_code_block_lines(&lines);
        assert_eq!(result, vec![false, true, true, false, false]);
    }

    #[test]
    fn code_block_lines_backtick_fence_containing_tilde() {
        // Tilde line inside backtick block is content, not a closer
        let lines = vec!["```python", "~~~", "still code", "```", "prose"];
        let result = compute_code_block_lines(&lines);
        assert_eq!(result, vec![true, true, true, false, false]);
    }

    #[test]
    fn code_block_lines_tilde_fence_containing_backtick() {
        // Backtick line inside tilde block is content, not a closer
        let lines = vec!["~~~python", "```", "still code", "~~~", "prose"];
        let result = compute_code_block_lines(&lines);
        assert_eq!(result, vec![true, true, true, false, false]);
    }

    #[test]
    fn code_block_lines_short_fence_inside_long_fence() {
        // ```` opens with 4 backticks; ``` inside is NOT a closer (too short)
        let lines = vec!["````python", "```", "still code", "````", "prose"];
        let result = compute_code_block_lines(&lines);
        assert_eq!(result, vec![true, true, true, false, false]);
    }

    #[test]
    fn code_block_lines_long_fence_closes_long_fence() {
        // ````` opens with 5; ````` closes (same length)
        let lines = vec!["`````", "code", "`````", "prose"];
        let result = compute_code_block_lines(&lines);
        assert_eq!(result, vec![true, true, false, false]);
    }

    #[test]
    fn code_block_lines_longer_fence_closes() {
        // ``` opens with 3; ```` closes (longer is OK per CommonMark)
        let lines = vec!["```python", "code", "````", "prose"];
        let result = compute_code_block_lines(&lines);
        assert_eq!(result, vec![true, true, false, false]);
    }

    #[test]
    fn code_block_lines_unterminated() {
        let lines = vec!["prose", "```python", "code without close"];
        let result = compute_code_block_lines(&lines);
        assert_eq!(result, vec![false, true, true]);
    }

    #[test]
    fn code_block_lines_empty() {
        let result = compute_code_block_lines(&[]);
        assert!(result.is_empty());
    }

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

    #[tokio::test]
    async fn test_run_cmd_with_timeout_success() {
        let cmd = Command::new("echo");
        let output = run_cmd_with_timeout(cmd, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(output.status.success());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_run_cmd_with_timeout_expires() {
        let mut cmd = Command::new("sleep");
        cmd.arg("999");
        let result = run_cmd_with_timeout(cmd, Duration::from_millis(100)).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("timed out"),
            "Expected timeout error, got: {}",
            err_msg
        );
    }
}
