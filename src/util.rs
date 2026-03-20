//! Shared utilities for the skilldo codebase

use anyhow::{Context, Result};
use std::fmt;
use std::path::{Path, PathBuf};
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

/// Find all fenced code blocks in text, returning (tag, body) pairs.
/// The tag is the lowercase text on the opener line (e.g., "python" in ```python).
/// Only matches fences at line boundaries (position 0 or after `\n`).
pub fn find_fenced_blocks(text: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let (fence_char, start, fence_len) = match find_next_line_fence(text, pos) {
            Some(result) => result,
            None => break,
        };

        let after = start + fence_len;
        // Extract tag from the opener line (text after the fence chars)
        let line_end = text[after..]
            .find('\n')
            .map(|i| after + i)
            .unwrap_or(text.len());
        let tag = text[after..line_end].trim().to_ascii_lowercase();
        let body_start = if line_end < text.len() {
            line_end + 1
        } else {
            line_end
        };

        // Closing fence must use same char, at least same length, at line boundary
        if let Some(close_offset) = find_closing_fence(text, body_start, fence_char, fence_len) {
            let body = text[body_start..body_start + close_offset]
                .trim()
                .to_string();
            blocks.push((tag, body));
            // Skip past the closing fence line
            let close_abs = body_start + close_offset;
            pos = text[close_abs..]
                .find('\n')
                .map(|i| close_abs + i + 1)
                .unwrap_or(text.len());
        } else {
            break;
        }
    }
    blocks
}

/// Find the next fence (``` or ~~~) that starts at a line boundary.
/// Returns the fence character, its position, and the full fence length (3+).
fn find_next_line_fence(text: &str, from: usize) -> Option<(char, usize, usize)> {
    let mut search_pos = from;
    loop {
        let backtick = text[search_pos..].find("```").map(|i| search_pos + i);
        let tilde = text[search_pos..].find("~~~").map(|i| search_pos + i);

        let (fence_char, candidate) = match (backtick, tilde) {
            (Some(b), Some(t)) => {
                if t < b {
                    ('~', t)
                } else {
                    ('`', b)
                }
            }
            (Some(b), None) => ('`', b),
            (None, Some(t)) => ('~', t),
            (None, None) => return None,
        };

        // Check line-boundary: position 0 or preceded by newline
        if candidate == 0 || text.as_bytes()[candidate - 1] == b'\n' {
            // Count the full fence length (3 or more consecutive fence chars)
            let fence_len = text[candidate..]
                .chars()
                .take_while(|&c| c == fence_char)
                .count();
            return Some((fence_char, candidate, fence_len));
        }

        // Not at line boundary — skip past this occurrence
        search_pos = candidate + 3;
    }
}

/// Find the closing fence that matches the opener, at a line boundary.
/// Per CommonMark, the closing fence must use the same char and be at least
/// as long as the opening fence.
fn find_closing_fence(text: &str, from: usize, fence_char: char, min_len: usize) -> Option<usize> {
    let base_fence = if fence_char == '`' { "```" } else { "~~~" };
    let mut search_pos = 0;
    let slice = &text[from..];
    loop {
        match slice[search_pos..].find(base_fence) {
            Some(offset) => {
                let abs = search_pos + offset;
                // Check line-boundary
                if abs == 0 || slice.as_bytes()[abs - 1] == b'\n' {
                    // Count fence length at this position
                    let len = slice[abs..]
                        .chars()
                        .take_while(|&c| c == fence_char)
                        .count();
                    if len >= min_len {
                        return Some(abs);
                    }
                }
                search_pos = abs + 3;
            }
            None => return None,
        }
    }
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
            // ':' added for Maven coordinates (group:artifact:version).
            // Safe for other ecosystems: Go import paths don't use ':',
            // npm/pip/cargo names don't use ':', and their parsers never produce it.
            '-' | '_' | '.' | '/' | '[' | ']' | '(' | ')' | ',' | '@' | ':' => {}
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

/// Escape XML special characters in a string to prevent XML injection.
/// Used when interpolating user-supplied values (e.g., Maven coordinates)
/// into XML templates like pom.xml.
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Strip XML comments (`<!-- ... -->`) to avoid matching commented-out tags.
/// Shared by java.rs POM parsers and java_parser.rs dependency extraction.
pub fn strip_xml_comments(content: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<!--.*?-->").unwrap());
    RE.replace_all(content, "").to_string()
}

/// Build a minimal Maven pom.xml from a list of Maven coordinates.
/// Each dep should be "group:artifact:version". Deps without a version are
/// skipped with a warning. Returns `None` if no valid deps remain.
/// Used by both bare-metal and container Java executors.
pub fn build_maven_pom_xml(deps: &[String]) -> Option<String> {
    let deps_xml: Vec<String> = deps
        .iter()
        .filter_map(|d| {
            let parts: Vec<&str> = d.splitn(3, ':').collect();
            if parts.len() >= 2 {
                let group = xml_escape(parts[0]);
                let artifact = xml_escape(parts[1]);
                let version = match parts.get(2).copied() {
                    // splitn(3) folds classifier into version for 4-part coords:
                    //   "g:a:1.0:javadoc" → parts[2] = "1.0:javadoc"
                    // split(':').next() strips the classifier, yielding "1.0".
                    // For version ranges like "[1.0,2.0)" there's no inner ':',
                    // so split(':').next() returns the whole string unchanged.
                    Some(v) if !v.is_empty() => {
                        // split(':').next() always returns Some for non-empty strings
                        let version_only = v.split(':').next().unwrap();
                        if version_only != v {
                            tracing::debug!(
                                "Stripped classifier from Maven coord: '{v}' → '{version_only}'"
                            );
                        }
                        xml_escape(version_only)
                    }
                    Some(_) | None => {
                        tracing::warn!(
                            "Maven dep '{}:{}' has no version — skipping",
                            parts[0],
                            parts[1]
                        );
                        return None;
                    }
                };
                Some(format!(
                    "        <dependency>\n            \
                     <groupId>{group}</groupId>\n            \
                     <artifactId>{artifact}</artifactId>\n            \
                     <version>{version}</version>\n        \
                     </dependency>"
                ))
            } else {
                tracing::debug!("Skipping non-Maven dep '{d}' (no ':' separator)");
                None
            }
        })
        .collect();

    if deps_xml.is_empty() {
        return None;
    }

    Some(format!(
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
    ))
}

/// Filter a list of paths to only include those that resolve within `boundary`.
/// Canonicalizes both the boundary and each path, then checks that the
/// canonical path starts with the canonical boundary. This prevents symlink
/// traversal attacks where a symlink inside a repo points to `/etc` or `~/.ssh`.
///
/// Paths that cannot be canonicalized (e.g., dangling symlinks) are silently
/// skipped. Paths that escape the boundary are logged at warn level and skipped.
pub fn filter_within_boundary(paths: Vec<PathBuf>, boundary: &Path) -> Vec<PathBuf> {
    let canonical_boundary = match boundary.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                "Cannot canonicalize repo boundary {}: {} — rejecting all paths",
                boundary.display(),
                e
            );
            return Vec::new();
        }
    };

    paths
        .into_iter()
        .filter(|path| {
            match path.canonicalize() {
                Ok(canonical) => {
                    if canonical.starts_with(&canonical_boundary) {
                        true
                    } else {
                        tracing::warn!(
                            "Skipping symlink escaping repo boundary: {}",
                            path.display()
                        );
                        false
                    }
                }
                Err(_) => {
                    // Dangling symlink or permission error — skip silently
                    false
                }
            }
        })
        .collect()
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

    // Priority 100: Internal/private files (read last if at all)
    if is_internal {
        return 100;
    }

    // Priority 0: Top-level package __init__.py (torch/__init__.py)
    if file_name == "__init__.py" && depth == 2 {
        return 0;
    }

    // Priority 10: Subpackage __init__.py files (torch/nn/__init__.py)
    if file_name == "__init__.py" && depth > 2 {
        return 10;
    }

    // Priority 100: Other _-prefixed files
    if file_name.starts_with('_') {
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

/// Run a command with a timeout, killing the child (and its descendants) on expiry.
///
/// On Unix: spawns the child in a new process group via `setpgid(0, 0)`.
/// On timeout, sends `SIGKILL` to the entire process group (`kill(-pgid, SIGKILL)`),
/// ensuring compilers, package managers, and other grandchildren are cleaned up.
///
/// On Windows: relies on `kill_on_drop(true)` which kills only the direct child.
/// Full process-tree cleanup on Windows requires `TerminateJobObject`, which is
/// not yet implemented.
pub async fn run_cmd_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Result<std::process::Output> {
    // On Unix, put the child in its own process group so we can kill the whole
    // tree on timeout.
    #[cfg(unix)]
    // SAFETY: setpgid is async-signal-safe per POSIX.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Failed to spawn command")?;

    // Grab the pid before moving child into wait_with_output.
    #[cfg(unix)]
    let child_pid = child.id();

    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(result) => result.context("Failed to execute command"),
        Err(_) => {
            // Timeout expired. Kill the entire process group on Unix.
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                // SAFETY: sending a signal to a process group is safe.
                // Negative pid means kill the whole process group.
                unsafe {
                    let ret = libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
                    if ret == -1 {
                        tracing::debug!(
                            "Process group kill failed (pid={}): {}",
                            pid,
                            std::io::Error::last_os_error()
                        );
                    }
                }
            }
            // On all platforms, tokio kill_on_drop handles the direct child
            // when the future is cancelled and child is dropped.
            Err(crate::error::SkillDoError::Timeout(timeout).into())
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

    /// Verify that timeout kills grandchild processes via process-group kill.
    ///
    /// Spawns a shell that launches a background `sleep 999` grandchild, then
    /// verifies that after timeout both the shell and the sleep are gone.
    #[tokio::test]
    #[cfg(unix)]
    async fn test_run_cmd_with_timeout_kills_process_group() {
        use std::path::Path;

        // Create a marker file that the grandchild will write to prove it started.
        let marker_dir = tempfile::tempdir().unwrap();
        let marker = marker_dir.path().join("grandchild_started");

        // Shell script: touch a marker, then sleep forever.
        // The shell is the child; sleep is the grandchild.
        let script = format!("touch {} && sleep 999", marker.display());
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&script);

        let result = run_cmd_with_timeout(cmd, Duration::from_millis(500)).await;
        assert!(result.is_err(), "should have timed out");

        // The marker proves the grandchild started running before timeout.
        assert!(
            Path::new(&marker).exists(),
            "grandchild should have started (marker file missing)"
        );

        // Give the OS a moment to reap the killed processes.
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify no `sleep 999` processes survive. Use pgrep to search.
        // We check that our specific sleep-999 is gone by scanning /proc or
        // using ps. pgrep -f matches the full command line.
        let ps_output = std::process::Command::new("ps")
            .args(["ax", "-o", "pid,command"])
            .output()
            .expect("ps should work");
        let ps_text = String::from_utf8_lossy(&ps_output.stdout);

        // Count lines containing "sleep 999" that are NOT the ps command itself.
        let orphaned: Vec<&str> = ps_text
            .lines()
            .filter(|line| line.contains("sleep 999") && !line.contains("ps "))
            .collect();

        assert!(
            orphaned.is_empty(),
            "grandchild sleep processes should have been killed by process-group SIGKILL, \
             but found: {:?}",
            orphaned
        );
    }

    #[test]
    fn test_calculate_file_priority() {
        let repo = Path::new("/repo");

        // Priority 0: top-level __init__.py (depth 2)
        assert_eq!(
            calculate_file_priority(Path::new("/repo/pkg/__init__.py"), repo),
            0
        );

        // Priority 10: deeper __init__.py (depth > 2)
        assert_eq!(
            calculate_file_priority(Path::new("/repo/pkg/sub/__init__.py"), repo),
            10
        );

        // Priority 20: public top-level module (depth 2)
        assert_eq!(
            calculate_file_priority(Path::new("/repo/pkg/core.py"), repo),
            20
        );

        // Priority 30: public subpackage module (depth 3)
        assert_eq!(
            calculate_file_priority(Path::new("/repo/pkg/sub/utils.py"), repo),
            30
        );

        // Priority 50: deeper module (depth 4+)
        assert_eq!(
            calculate_file_priority(Path::new("/repo/a/b/c/deep.py"), repo),
            50
        );

        // Priority 100: underscore-prefixed file
        assert_eq!(
            calculate_file_priority(Path::new("/repo/pkg/_private.py"), repo),
            100
        );

        // Priority 100: internal directory
        assert_eq!(
            calculate_file_priority(Path::new("/repo/pkg/_internal/foo.py"), repo),
            100
        );

        // Priority 100: test directory
        assert_eq!(
            calculate_file_priority(Path::new("/repo/tests/test_main.py"), repo),
            100
        );

        // Priority 100: __init__.py inside internal/test dirs (not priority 0!)
        assert_eq!(
            calculate_file_priority(Path::new("/repo/tests/__init__.py"), repo),
            100
        );
        assert_eq!(
            calculate_file_priority(Path::new("/repo/scripts/__init__.py"), repo),
            100
        );
        assert_eq!(
            calculate_file_priority(Path::new("/repo/benchmarks/sub/__init__.py"), repo),
            100
        );
    }

    // ========================================================================
    // find_fenced_blocks
    // ========================================================================

    #[test]
    fn test_find_fenced_blocks_basic() {
        let text = "```python\nimport os\n```";
        let blocks = find_fenced_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "python");
        assert_eq!(blocks[0].1, "import os");
    }

    #[test]
    fn test_find_fenced_blocks_tilde_fence() {
        let text = "~~~python\nimport os\n~~~";
        let blocks = find_fenced_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "python");
    }

    #[test]
    fn test_find_fenced_blocks_mixed_tilde_before_backtick() {
        let text = "~~~json\n{}\n~~~\n\n```python\nimport os\n```";
        let blocks = find_fenced_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "json");
        assert_eq!(blocks[1].0, "python");
    }

    #[test]
    fn test_find_fenced_blocks_mixed_backtick_before_tilde() {
        let text = "```python\nfirst\n```\n\n~~~json\n{}\n~~~";
        let blocks = find_fenced_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "python");
        assert_eq!(blocks[1].0, "json");
    }

    #[test]
    fn test_find_fenced_blocks_unclosed_fence() {
        let text = "```python\nimport os\n";
        let blocks = find_fenced_blocks(text);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_find_fenced_blocks_line_anchored_ignores_mid_line() {
        // Fences embedded mid-line (e.g., in string literals) should NOT be treated as blocks.
        let text = "x = \"```python\"\ny = \"```\"";
        let blocks = find_fenced_blocks(text);
        assert!(
            blocks.is_empty(),
            "mid-line fences should be ignored, got {} block(s)",
            blocks.len()
        );
    }

    #[test]
    fn test_find_fenced_blocks_line_anchored_ignores_mid_line_tilde() {
        let text = "x = \"~~~python\"\ny = \"~~~\"";
        let blocks = find_fenced_blocks(text);
        assert!(
            blocks.is_empty(),
            "mid-line tilde fences should be ignored, got {} block(s)",
            blocks.len()
        );
    }

    #[test]
    fn test_find_fenced_blocks_closing_must_be_line_anchored() {
        // Opening fence is at line start, but closing fence is mid-line → no block.
        let text = "```python\nimport os # ```\nmore code";
        let blocks = find_fenced_blocks(text);
        assert!(
            blocks.is_empty(),
            "mid-line closing fence should not close block, got {} block(s)",
            blocks.len()
        );
    }

    #[test]
    fn test_find_fenced_blocks_empty_block_no_newline() {
        // 6 consecutive backticks = a single 6-char opener with no closer (unclosed)
        let text = "``````";
        let blocks = find_fenced_blocks(text);
        assert_eq!(blocks.len(), 0, "6-char fence with no closer is unclosed");
    }

    #[test]
    fn test_find_fenced_blocks_long_fence_needs_matching_close() {
        // 4-char opener needs 4+ char closer — inner ``` should NOT close it
        let text = "````json\ninner ```\nstill inside\n````\n";
        let blocks = find_fenced_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert!(
            blocks[0].1.contains("inner ```"),
            "inner ``` should be part of the body, not a closer"
        );
    }

    // ── xml_escape tests ──

    #[test]
    fn xml_escape_no_special_chars() {
        assert_eq!(xml_escape("com.google.gson"), "com.google.gson");
    }

    #[test]
    fn xml_escape_ampersand() {
        assert_eq!(xml_escape("a&b"), "a&amp;b");
    }

    #[test]
    fn xml_escape_angle_brackets() {
        assert_eq!(xml_escape("<foo>"), "&lt;foo&gt;");
    }

    #[test]
    fn xml_escape_quotes() {
        assert_eq!(xml_escape("a\"b"), "a&quot;b");
    }

    #[test]
    fn xml_escape_all_special_chars() {
        assert_eq!(xml_escape("a&b<c>d\"e"), "a&amp;b&lt;c&gt;d&quot;e");
    }

    #[test]
    fn xml_escape_apostrophe() {
        assert_eq!(xml_escape("a'b"), "a&apos;b");
    }

    #[test]
    fn xml_escape_version_range() {
        // Version ranges like [0,) should pass through unchanged
        assert_eq!(xml_escape("[0,)"), "[0,)");
    }

    #[test]
    fn build_maven_pom_basic() {
        let deps = vec!["com.google.code.gson:gson:2.10.1".into()];
        let pom = build_maven_pom_xml(&deps).unwrap();
        assert!(pom.contains("<groupId>com.google.code.gson</groupId>"));
        assert!(pom.contains("<artifactId>gson</artifactId>"));
        assert!(pom.contains("<version>2.10.1</version>"));
    }

    #[test]
    fn build_maven_pom_empty_deps() {
        assert!(build_maven_pom_xml(&[]).is_none());
    }

    #[test]
    fn build_maven_pom_skips_versionless() {
        let deps = vec!["com.example:lib".into()];
        assert!(build_maven_pom_xml(&deps).is_none());
    }

    #[test]
    fn build_maven_pom_xml_escapes_values() {
        let deps = vec!["com.example:lib&test:1.0".into()];
        let pom = build_maven_pom_xml(&deps).unwrap();
        assert!(pom.contains("lib&amp;test"));
    }

    // ── filter_within_boundary tests ──

    #[test]
    fn filter_within_boundary_keeps_normal_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("hello.txt");
        std::fs::write(&file, "content").unwrap();

        let result = filter_within_boundary(vec![file.clone()], root);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], file);
    }

    #[cfg(unix)]
    #[test]
    fn filter_within_boundary_skips_symlink_escaping_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create a directory outside the repo boundary
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("secret.txt");
        std::fs::write(&outside_file, "sensitive data").unwrap();

        // Create a symlink inside the repo that points outside
        let symlink_path = root.join("escape");
        std::os::unix::fs::symlink(outside.path(), &symlink_path).unwrap();

        let escaped_file = symlink_path.join("secret.txt");
        let normal_file = root.join("normal.txt");
        std::fs::write(&normal_file, "safe content").unwrap();

        let result = filter_within_boundary(vec![escaped_file, normal_file.clone()], root);
        assert_eq!(result.len(), 1, "should keep only the normal file");
        assert_eq!(result[0], normal_file);
    }

    #[cfg(unix)]
    #[test]
    fn filter_within_boundary_keeps_internal_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create a real file and a symlink to it within the same directory
        let real_file = root.join("real.txt");
        std::fs::write(&real_file, "content").unwrap();
        let symlink_path = root.join("link.txt");
        std::os::unix::fs::symlink(&real_file, &symlink_path).unwrap();

        let result = filter_within_boundary(vec![symlink_path.clone()], root);
        assert_eq!(result.len(), 1, "internal symlink should be kept");
    }

    #[test]
    fn filter_within_boundary_skips_dangling_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create a file path that doesn't exist (can't be canonicalized)
        let nonexistent = root.join("ghost.txt");
        let real_file = root.join("real.txt");
        std::fs::write(&real_file, "content").unwrap();

        let result = filter_within_boundary(vec![nonexistent, real_file.clone()], root);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], real_file);
    }

    #[test]
    fn filter_within_boundary_empty_paths() {
        let dir = tempfile::tempdir().unwrap();
        let result = filter_within_boundary(vec![], dir.path());
        assert!(result.is_empty());
    }

    #[test]
    fn filter_within_boundary_noncanonicalize_boundary_rejects_all() {
        // If the boundary itself can't be canonicalized, fail closed (reject all)
        let nonexistent_boundary = Path::new("/nonexistent_dir_abc123");
        let paths = vec![PathBuf::from("/some/path")];
        let result = filter_within_boundary(paths, nonexistent_boundary);
        assert!(
            result.is_empty(),
            "should reject all paths when boundary is not canonicalizable"
        );
    }

    #[test]
    fn build_maven_pom_strips_classifier() {
        // 4-part Maven coordinate: group:artifact:version:classifier
        let deps = vec!["com.google.code.gson:gson:2.10.1:javadoc".into()];
        let pom = build_maven_pom_xml(&deps).unwrap();
        assert!(
            pom.contains("<version>2.10.1</version>"),
            "classifier should be stripped"
        );
        assert!(
            !pom.contains("javadoc"),
            "classifier should not appear in POM"
        );
    }

    #[test]
    fn build_maven_pom_skips_empty_version() {
        // group:artifact: (trailing colon, empty version)
        let deps = vec!["com.example:lib:".into()];
        assert!(
            build_maven_pom_xml(&deps).is_none(),
            "empty version after colon should be skipped"
        );
    }

    #[test]
    fn build_maven_pom_skips_non_maven_dep() {
        // Single token with no colon separator
        let deps = vec!["just-a-name".into()];
        assert!(build_maven_pom_xml(&deps).is_none());
    }
}
