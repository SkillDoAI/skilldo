//! CLI-based LLM client — shells out to vendor CLIs (claude, codex, gemini)
//! instead of making HTTP API calls. Implements the same `LlmClient` trait.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::debug;

use super::client::LlmClient;

/// LLM client that invokes a CLI tool as a subprocess.
/// Prompt is piped via stdin; response is captured from stdout.
#[allow(dead_code)] // public API; wired up by factory in a follow-up task
pub struct CliClient {
    command: String,
    args: Vec<String>,
    json_path: Option<String>,
}

impl CliClient {
    #[allow(dead_code)] // public API; wired up by factory in a follow-up task
    pub fn new(command: String, args: Vec<String>, json_path: Option<String>) -> Self {
        Self {
            command,
            args,
            json_path,
        }
    }
}

#[async_trait]
impl LlmClient for CliClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        debug!("CLI client: {} {:?}", self.command, self.args);

        let mut child = Command::new(&self.command)
            .args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn CLI: {}", self.command))?;

        // Write prompt to stdin, then drop to signal EOF
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .await
                .context("Failed to write prompt to CLI stdin")?;
        }

        let output = child
            .wait_with_output()
            .await
            .context("Failed to read CLI output")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "CLI exited with {}: {}",
                output.status,
                stderr.lines().next().unwrap_or("(no stderr)")
            );
        }

        let stdout = String::from_utf8(output.stdout).context("CLI output is not valid UTF-8")?;

        // Extract from JSON if json_path is configured
        match &self.json_path {
            Some(field) => {
                let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
                    .with_context(|| "CLI output is not valid JSON")?;
                match parsed.get(field) {
                    Some(serde_json::Value::String(s)) => Ok(s.clone()),
                    Some(other) => Ok(other.to_string()),
                    None => bail!(
                        "CLI JSON output missing field '{}'. Keys: {:?}",
                        field,
                        parsed.as_object().map(|o| o.keys().collect::<Vec<_>>())
                    ),
                }
            }
            None => Ok(stdout),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cli_client_echo_raw() {
        // `cat` echoes stdin back — simplest possible "CLI"
        let client = CliClient::new("cat".to_string(), vec![], None);
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
            Some("result".to_string()),
        );
        let result = client.complete("ignored").await.unwrap();
        assert_eq!(result, "extracted text");
    }

    #[tokio::test]
    async fn test_cli_client_command_not_found() {
        let client = CliClient::new("nonexistent_binary_xyz_12345".to_string(), vec![], None);
        let result = client.complete("hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cli_client_nonzero_exit() {
        let client = CliClient::new(
            "sh".to_string(),
            vec!["-c".to_string(), "exit 1".to_string()],
            None,
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
            Some("result".to_string()),
        );
        let result = client.complete("hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("result"));
    }

    #[tokio::test]
    async fn test_cli_client_stdin_receives_prompt() {
        // `wc -c` counts bytes from stdin — verifies prompt was piped
        let client = CliClient::new("wc".to_string(), vec!["-c".to_string()], None);
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
            Some("count".to_string()),
        );
        let result = client.complete("hello").await.unwrap();
        assert_eq!(result, "42");
    }

    #[tokio::test]
    async fn test_cli_client_invalid_json() {
        let client = CliClient::new(
            "sh".to_string(),
            vec!["-c".to_string(), "echo 'not json'".to_string()],
            Some("result".to_string()),
        );
        let result = client.complete("hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("JSON"));
    }
}
