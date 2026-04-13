//! CLI-based LLM client — shells out to vendor CLIs (claude, codex, gemini)
//! instead of making HTTP API calls. Implements the same `LlmClient` trait.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::debug;

use super::client::LlmClient;

/// LLM client that invokes a CLI tool as a subprocess.
/// Prompt is piped via stdin; response is captured from stdout.
/// Uses `kill_on_drop(true)` and a timeout to prevent orphaned processes.
pub struct CliClient {
    command: String,
    args: Vec<String>,
    /// CLI flag(s) for passing a system prompt (e.g., ["--system-prompt"] for
    /// claude, ["-s"] for codex). When non-empty, complete_with_system() appends
    /// these flags + the system text to the command. When empty, falls back to
    /// concatenating system + user into stdin.
    system_args: Vec<String>,
    json_path: Option<String>,
    timeout_secs: u64,
}

impl CliClient {
    pub fn new(
        command: String,
        args: Vec<String>,
        system_args: Vec<String>,
        json_path: Option<String>,
        timeout_secs: u64,
    ) -> Self {
        Self {
            command,
            args,
            system_args,
            json_path,
            timeout_secs,
        }
    }
}

#[async_trait]
impl LlmClient for CliClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        self.run_cli(prompt, &[]).await
    }

    async fn complete_with_system(&self, system: &str, user: &str) -> Result<String> {
        if system.is_empty() || self.system_args.is_empty() {
            // No system args configured or empty system → default concat
            if system.is_empty() {
                return self.run_cli(user, &[]).await;
            }
            return self.run_cli(&format!("{}\n\n{}", system, user), &[]).await;
        }
        // Inject configured system args + the system text as extra CLI args
        debug!(
            "CLI client: injecting system prompt via {:?} ({} chars)",
            self.system_args,
            system.len()
        );
        let mut extra: Vec<String> = self.system_args.clone();
        extra.push(system.to_string());
        self.run_cli(user, &extra).await
    }
}

impl CliClient {
    /// Shared implementation — spawns the CLI with configured args + optional
    /// extra args (used by complete_with_system to inject system prompt flags).
    async fn run_cli(&self, prompt: &str, extra_args: &[String]) -> Result<String> {
        debug!(
            "CLI client: {} ({} + {} arg(s))",
            self.command,
            self.args.len(),
            extra_args.len()
        );

        let mut child = Command::new(&self.command)
            .args(&self.args)
            .args(extra_args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("Failed to spawn CLI: {}", self.command))?;

        // Wrap stdin write + wait in a single timeout so neither can hang.
        // Write stdin concurrently with wait_with_output to avoid deadlock:
        // if the CLI fills its stdout pipe buffer before reading all stdin,
        // sequential write-then-wait would block on both sides.
        let timeout = Duration::from_secs(self.timeout_secs);
        let stdin_handle = child.stdin.take();
        let prompt_bytes = prompt.as_bytes().to_vec();
        let output = match tokio::time::timeout(timeout, async {
            // Spawn stdin write as a concurrent task so output reading can proceed.
            let stdin_task = tokio::spawn(async move {
                if let Some(mut stdin) = stdin_handle {
                    if let Err(e) = stdin.write_all(&prompt_bytes).await {
                        if e.kind() != std::io::ErrorKind::BrokenPipe {
                            return Err(anyhow::anyhow!(
                                "Failed to write prompt to CLI stdin: {e}"
                            ));
                        }
                        debug!("CLI stdin closed early (broken pipe) — continuing");
                    }
                }
                Ok(())
            });

            let output = child
                .wait_with_output()
                .await
                .context("Failed to read CLI output")?;
            // Surface any stdin write error (ignoring JoinError — child is dead anyway)
            if let Ok(Err(e)) = stdin_task.await {
                return Err(e);
            }
            Ok(output)
        })
        .await
        {
            Ok(result) => result?,
            Err(_) => {
                bail!(
                    "CLI command '{}' timed out after {}s",
                    self.command,
                    self.timeout_secs
                );
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let preview: String = stderr.lines().take(5).collect::<Vec<_>>().join(" | ");
            bail!(
                "CLI exited with {}: {}",
                output.status,
                if preview.is_empty() {
                    "(no stderr)"
                } else {
                    &preview
                }
            );
        }

        let stdout = String::from_utf8(output.stdout).context("CLI output is not valid UTF-8")?;

        // Extract from JSON if json_path is configured.
        // Supports dot-notation for nested paths: "data.response"
        match &self.json_path {
            Some(path) => {
                let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
                    .with_context(|| "CLI output is not valid JSON")?;
                let segments: Vec<&str> = path.split('.').collect();
                let value = segments.iter().try_fold(&parsed, |acc, key| acc.get(*key));
                match value {
                    Some(serde_json::Value::String(s)) => Ok(s.clone()),
                    Some(other) => Ok(other.to_string()),
                    None => {
                        // Walk path to find where traversal broke and report those keys
                        let mut node = &parsed;
                        let mut failed_at = &segments[0];
                        for seg in &segments {
                            match node.get(*seg) {
                                Some(next) => node = next,
                                None => {
                                    failed_at = seg;
                                    break;
                                }
                            }
                        }
                        if let Some(obj) = node.as_object() {
                            let available: Vec<_> = obj.keys().collect();
                            bail!(
                                "CLI JSON output missing '{}' in path '{}'. Available keys: {:?}",
                                failed_at,
                                path,
                                available
                            )
                        } else {
                            let kind = match node {
                                serde_json::Value::String(_) => "string",
                                serde_json::Value::Number(_) => "number",
                                serde_json::Value::Bool(_) => "bool",
                                serde_json::Value::Array(_) => "array",
                                serde_json::Value::Null => "null",
                                _ => "non-object",
                            };
                            bail!(
                                "CLI JSON path '{}': cannot look up '{}' in a {} value",
                                path,
                                failed_at,
                                kind
                            )
                        }
                    }
                }
            }
            None => Ok(stdout),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default timeout for tests (generous — most finish instantly).
    const TEST_TIMEOUT: u64 = 30;

    #[tokio::test]
    async fn test_cli_client_echo_raw() {
        // `cat` echoes stdin back — simplest possible "CLI"
        let client = CliClient::new("cat".to_string(), vec![], vec![], None, TEST_TIMEOUT);
        let result = client.complete("hello world").await.unwrap();
        assert_eq!(result.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_cli_client_json_extraction() {
        let client = CliClient::new(
            "sh".to_string(),
            vec![
                "-c".to_string(),
                r#"echo '{"result": "extracted text"}'"#.to_string(),
            ],
            vec![],
            Some("result".to_string()),
            TEST_TIMEOUT,
        );
        let result = client.complete("ignored").await.unwrap();
        assert_eq!(result, "extracted text");
    }

    #[tokio::test]
    async fn test_cli_client_json_nested_path() {
        let client = CliClient::new(
            "sh".to_string(),
            vec![
                "-c".to_string(),
                r#"echo '{"data": {"response": "nested value"}}'"#.to_string(),
            ],
            vec![],
            Some("data.response".to_string()),
            TEST_TIMEOUT,
        );
        let result = client.complete("ignored").await.unwrap();
        assert_eq!(result, "nested value");
    }

    #[tokio::test]
    async fn test_cli_client_json_nested_path_missing() {
        let client = CliClient::new(
            "sh".to_string(),
            vec![
                "-c".to_string(),
                r#"echo '{"data": {"other": "value"}}'"#.to_string(),
            ],
            vec![],
            Some("data.response".to_string()),
            TEST_TIMEOUT,
        );
        let result = client.complete("ignored").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // Should report the failing segment and keys at that level
        assert!(
            err.contains("'response'"),
            "should name failing segment: {err}"
        );
        assert!(
            err.contains("data.response"),
            "should include full path: {err}"
        );
        assert!(
            err.contains("other"),
            "should show available keys at 'data': {err}"
        );
    }

    #[tokio::test]
    async fn test_cli_client_command_not_found() {
        let client = CliClient::new(
            "nonexistent_binary_xyz_12345".to_string(),
            vec![],
            vec![],
            None,
            TEST_TIMEOUT,
        );
        let result = client.complete("hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cli_client_nonzero_exit() {
        let client = CliClient::new(
            "sh".to_string(),
            vec!["-c".to_string(), "exit 1".to_string()],
            vec![],
            None,
            TEST_TIMEOUT,
        );
        let result = client.complete("hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("CLI exited with"));
    }

    #[tokio::test]
    async fn test_cli_client_json_missing_field() {
        let client = CliClient::new(
            "sh".to_string(),
            vec!["-c".to_string(), r#"echo '{"other": "value"}'"#.to_string()],
            vec![],
            Some("result".to_string()),
            TEST_TIMEOUT,
        );
        let result = client.complete("hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("result"));
    }

    #[tokio::test]
    async fn test_cli_client_stdin_receives_prompt() {
        // `wc -c` counts bytes from stdin — verifies prompt was piped
        let client = CliClient::new(
            "wc".to_string(),
            vec!["-c".to_string()],
            vec![],
            None,
            TEST_TIMEOUT,
        );
        let result = client.complete("12345").await.unwrap();
        let byte_count: usize = result.trim().parse().unwrap();
        assert_eq!(byte_count, 5);
    }

    #[tokio::test]
    async fn test_cli_client_json_non_string_value() {
        // JSON value is a number, not string — should be returned as string
        let client = CliClient::new(
            "sh".to_string(),
            vec!["-c".to_string(), r#"echo '{"count": 42}'"#.to_string()],
            vec![],
            Some("count".to_string()),
            TEST_TIMEOUT,
        );
        let result = client.complete("hello").await.unwrap();
        assert_eq!(result, "42");
    }

    #[tokio::test]
    async fn test_cli_client_json_path_type_mismatch() {
        // Path traversal into a non-object value should report the type
        let client = CliClient::new(
            "sh".to_string(),
            vec![
                "-c".to_string(),
                r#"echo '{"data": "just a string"}'  "#.to_string(),
            ],
            vec![],
            Some("data.nested".to_string()),
            TEST_TIMEOUT,
        );
        let result = client.complete("ignored").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("string"), "should report value type: {err}");
        assert!(
            err.contains("nested"),
            "should report failing segment: {err}"
        );
    }

    #[tokio::test]
    async fn test_cli_client_invalid_json() {
        let client = CliClient::new(
            "sh".to_string(),
            vec!["-c".to_string(), "echo 'not json'".to_string()],
            vec![],
            Some("result".to_string()),
            TEST_TIMEOUT,
        );
        let result = client.complete("hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("JSON"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cli_client_broken_pipe() {
        // `true` exits immediately without reading stdin — triggers BrokenPipe
        // on large prompts. Send enough data to make the pipe buffer overflow.
        let client = CliClient::new("true".to_string(), vec![], vec![], None, TEST_TIMEOUT);
        let large_prompt = "x".repeat(1_000_000);
        // Should succeed (BrokenPipe is silently handled, exit code 0)
        let result = client.complete(&large_prompt).await;
        assert!(result.is_ok(), "BrokenPipe should be handled gracefully");
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_cli_client_broken_pipe() {
        // `cmd /c exit 0` exits immediately — Windows equivalent of `true`
        let client = CliClient::new(
            "cmd".to_string(),
            vec!["/c".to_string(), "exit".to_string(), "0".to_string()],
            vec![],
            None,
            TEST_TIMEOUT,
        );
        let large_prompt = "x".repeat(1_000_000);
        let result = client.complete(&large_prompt).await;
        assert!(result.is_ok(), "BrokenPipe should be handled gracefully");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cli_client_timeout() {
        // `sleep 10` with a 1-second timeout should fail with a clear message
        let client = CliClient::new(
            "sleep".to_string(),
            vec!["10".to_string()],
            vec![],
            None,
            1, // 1-second timeout
        );
        let result = client.complete("").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("timed out"),
            "Expected timeout message, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_cli_client_timeout() {
        // `ping -n 11 127.0.0.1` waits ~10 seconds (Windows-compatible, works with stdin redirection)
        // timeout.exe doesn't work with redirected stdin — it exits immediately.
        let client = CliClient::new(
            "ping".to_string(),
            vec!["-n".to_string(), "11".to_string(), "127.0.0.1".to_string()],
            vec![],
            None,
            1,
        );
        let result = client.complete("").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("timed out"),
            "Expected timeout message, got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_cli_complete_with_system_no_args_concat() {
        // When system_args is empty, complete_with_system concatenates
        let client = CliClient::new("cat".to_string(), vec![], vec![], None, TEST_TIMEOUT);
        let result = client
            .complete_with_system("system rules", "user data")
            .await
            .unwrap();
        assert!(result.contains("system rules"));
        assert!(result.contains("user data"));
    }

    #[tokio::test]
    async fn test_cli_complete_with_system_empty_system() {
        // Empty system string → just sends user data
        let client = CliClient::new("cat".to_string(), vec![], vec![], None, TEST_TIMEOUT);
        let result = client.complete_with_system("", "user only").await.unwrap();
        assert_eq!(result.trim(), "user only");
    }

    #[tokio::test]
    async fn test_cli_complete_with_system_args_injects_flag() {
        // Use `sh -c 'cat'` which ignores extra args (--sys and system text)
        // and just reads stdin (user data). This exercises the code path where
        // system_args is non-empty → extra args are appended to the command.
        let client = CliClient::new(
            "sh".to_string(),
            vec!["-c".to_string(), "cat".to_string()],
            vec!["--sys".to_string()],
            None,
            TEST_TIMEOUT,
        );
        let result = client
            .complete_with_system("system rules", "user data")
            .await
            .unwrap();
        // sh -c 'cat' reads stdin only; --sys and system text are args to sh, ignored
        assert_eq!(result.trim(), "user data");
    }
}
