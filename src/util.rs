//! Shared utilities for the skilldo codebase

use anyhow::{Context, Result};
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

/// Find all fenced code blocks in text, returning (tag, body) pairs.
/// The tag is the lowercase text on the opener line (e.g., "python" in ```python).
/// Only matches fences at line boundaries (position 0 or after `\n`).
pub fn find_fenced_blocks(text: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let (fence, start) = match find_next_line_fence(text, pos) {
            Some(result) => result,
            None => break,
        };

        let after = start + fence.len();
        // Closing fence must also be at a line boundary
        if let Some(close_offset) = find_closing_fence(text, after, fence) {
            let raw = &text[after..after + close_offset];
            let (tag, body) = if let Some(nl) = raw.find('\n') {
                let tag_part = raw[..nl].trim().to_ascii_lowercase();
                let body = raw[nl + 1..].trim().to_string();
                (tag_part, body)
            } else {
                (String::new(), raw.trim().to_string())
            };
            blocks.push((tag, body));
            pos = after + close_offset + fence.len();
        } else {
            break;
        }
    }
    blocks
}

/// Find the next fence (``` or ~~~) that starts at a line boundary.
fn find_next_line_fence(text: &str, from: usize) -> Option<(&str, usize)> {
    let mut search_pos = from;
    loop {
        let backtick = text[search_pos..].find("```").map(|i| search_pos + i);
        let tilde = text[search_pos..].find("~~~").map(|i| search_pos + i);

        let (fence, candidate) = match (backtick, tilde) {
            (Some(b), Some(t)) => {
                if t < b {
                    ("~~~", t)
                } else {
                    ("```", b)
                }
            }
            (Some(b), None) => ("```", b),
            (None, Some(t)) => ("~~~", t),
            (None, None) => return None,
        };

        // Check line-boundary: position 0 or preceded by newline
        if candidate == 0 || text.as_bytes()[candidate - 1] == b'\n' {
            return Some((fence, candidate));
        }

        // Not at line boundary — skip past this occurrence
        search_pos = candidate + fence.len();
    }
}

/// Find the closing fence that matches the opener, at a line boundary.
fn find_closing_fence(text: &str, from: usize, fence: &str) -> Option<usize> {
    let mut search_pos = 0;
    let slice = &text[from..];
    loop {
        match slice[search_pos..].find(fence) {
            Some(offset) => {
                let abs = search_pos + offset;
                // Check line-boundary: preceded by newline (closing fences are never at pos 0
                // of the slice because there's at least the tag/body before them)
                if abs == 0 || slice.as_bytes()[abs - 1] == b'\n' {
                    return Some(abs);
                }
                search_pos = abs + fence.len();
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
                    // Strip classifier suffix (e.g., "1.0:javadoc" → "1.0")
                    Some(v) if !v.is_empty() => {
                        let v = v.split(':').next().unwrap_or(v);
                        xml_escape(v)
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

/// Run a command with a timeout, killing the child on expiry.
/// Uses `tokio::process` with `kill_on_drop(true)` — no orphaned threads,
/// no setsid/process-group gymnastics, no LLVM-profdata deadlocks.
///
/// **Note:** `kill_on_drop` sends SIGKILL only to the direct child process,
/// not to any grandchildren it may have spawned. For container workloads this
/// is mitigated by explicit `runtime kill <container>` in the caller's error
/// path. For non-container workloads, grandchild processes may be orphaned.
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
            // Timeout expired. `child` was moved into `wait_with_output(self)`,
            // so tokio drops it when cancelling the inner future → SIGKILL.
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
        // Adjacent opening+closing fences with no content or newline between them
        let text = "``````";
        let blocks = find_fenced_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "");
        assert_eq!(blocks[0].1, "");
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
}
