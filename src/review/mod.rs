//! Review agent: accuracy and safety validation for generated SKILL.md files.
//!
//! Two-phase approach:
//! 1. LLM generates a Python introspection script to verify claims in the SKILL.md
//! 2. Script runs in a container; results + SKILL.md go back to LLM for verdict

use anyhow::{Context, Result};
use std::str::FromStr;
use tracing::{debug, warn};

use crate::config::ContainerConfig;
use crate::detector::Language;
use crate::test_agent::container_executor::ContainerExecutor;
use crate::test_agent::executor::ExecutionResult;
use crate::test_agent::LanguageExecutor;
// Re-export Severity from lint so callers can use `review::Severity`
pub use crate::lint::Severity;
use crate::llm::client::LlmClient;
use crate::llm::prompts_v2;

/// Result of a review pass.
#[derive(Debug, Clone)]
pub struct ReviewResult {
    pub passed: bool,
    pub issues: Vec<ReviewIssue>,
    /// True when the LLM returned an unparseable verdict (non-strict mode only).
    /// The pipeline should retry when this is true and retries remain.
    pub malformed: bool,
    /// Raw introspection output (JSON from container). Passed to create agent
    /// on review failure so it can see actual signatures, not just complaints.
    pub introspection_output: Option<String>,
    /// True when Python container introspection was expected but unavailable
    /// (e.g., container/runtime/script failure). False when introspection is
    /// intentionally skipped or not applicable (non-Python). Propagated to
    /// telemetry so CI consumers can distinguish grounded vs advisory reviews.
    pub degraded: bool,
}

impl Default for ReviewResult {
    fn default() -> Self {
        Self {
            passed: true,
            issues: Vec::new(),
            malformed: false,
            introspection_output: None,
            degraded: false,
        }
    }
}

/// A single issue found during review.
#[derive(Debug, Clone)]
pub struct ReviewIssue {
    pub severity: Severity,
    pub category: String,
    pub complaint: String,
    pub evidence: String,
}

/// Print a numbered list of review issues to stdout.
pub fn print_review_issues(issues: &[ReviewIssue]) {
    for (i, issue) in issues.iter().enumerate() {
        println!(
            "  {}. [{}][{}] {}",
            i + 1,
            issue.severity,
            issue.category,
            issue.complaint
        );
        if !issue.evidence.is_empty() {
            println!("     Evidence: {}", issue.evidence);
        }
    }
}

/// Review agent that validates SKILL.md accuracy and safety.
pub struct ReviewAgent<'a> {
    client: &'a dyn LlmClient,
    container_config: ContainerConfig,
    custom_prompt: Option<String>,
    /// In strict mode, unparseable LLM responses are treated as errors instead of silent passes.
    /// Use strict=true for standalone review (user explicitly asked to review).
    /// Use strict=false in the pipeline (don't block generation on LLM flakiness).
    strict: bool,
    /// When true, skip container introspection regardless of language.
    /// Used when --no-container is passed to the CLI.
    skip_introspection: bool,
}

impl<'a> ReviewAgent<'a> {
    pub fn new(
        client: &'a dyn LlmClient,
        container_config: ContainerConfig,
        custom_prompt: Option<String>,
    ) -> Self {
        Self {
            client,
            container_config,
            custom_prompt,
            strict: false,
            skip_introspection: false,
        }
    }

    /// Enable strict mode: parse failures become errors instead of silent passes.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Skip container introspection (e.g., when --no-container is passed).
    pub fn with_skip_introspection(mut self, skip: bool) -> Self {
        self.skip_introspection = skip;
        self
    }

    /// Run the full review pipeline on a SKILL.md.
    pub async fn review(
        &self,
        skill_md: &str,
        package_name: &str,
        language: &Language,
    ) -> Result<ReviewResult> {
        // Phase A: generate introspection script (Python only, unless skipped)
        let (introspection_output, introspection_degraded) = if self.skip_introspection {
            (
                "INTROSPECTION SKIPPED: container introspection disabled".to_string(),
                false, // Not degraded — user intentionally skipped
            )
        } else if matches!(language, Language::Python) {
            match self
                .run_introspection(skill_md, package_name, language)
                .await
            {
                Ok(output) => {
                    let degraded = output.starts_with("INTROSPECTION SKIPPED");
                    (output, degraded)
                }
                Err(e) => {
                    warn!("  review: container introspection failed: {}", e);
                    (format!("INTROSPECTION FAILED: {}", e), true)
                }
            }
        } else {
            // Non-Python: introspection not applicable, not a degraded state.
            (
                "INTROSPECTION SKIPPED: only Python is supported for container checks".to_string(),
                false,
            )
        };

        // Phase B: LLM verdict (accuracy + safety + consistency)
        let verdict_prompt = prompts_v2::review_verdict_prompt(
            skill_md,
            &introspection_output,
            self.custom_prompt.as_deref(),
            language,
        );
        let verdict_response = self
            .client
            .complete(&verdict_prompt)
            .await
            .context("review verdict LLM call failed")?;

        let mut result = parse_review_response(&verdict_response, self.strict)?;

        // Attach introspection output only when it is valid JSON.
        // Sentinel strings ("INTROSPECTION SKIPPED: ...") and error messages
        // ("INTROSPECTION FAILED: ...") should not be persisted.
        let trimmed_intro = introspection_output.trim();
        if trimmed_intro.starts_with('{') || trimmed_intro.starts_with('[') {
            result.introspection_output = Some(introspection_output.clone());
        }

        // Degraded introspection means the verdict is based on textual analysis
        // only. In strict mode this fails the review; in normal mode it adds a
        // warning so the pipeline can track it as an unresolved issue.
        result.degraded = introspection_degraded;
        if introspection_degraded {
            let severity = if self.strict {
                Severity::Error
            } else {
                Severity::Warning
            };
            if self.strict {
                result.passed = false;
            }
            result.issues.push(ReviewIssue {
                severity,
                category: "introspection".to_string(),
                complaint:
                    "Container introspection failed — verdict is based on textual analysis only"
                        .to_string(),
                evidence: introspection_output.chars().take(500).collect(),
            });
        }

        Ok(result)
    }

    /// Phase A: ask LLM to generate an introspection script, then run it in a container.
    async fn run_introspection(
        &self,
        skill_md: &str,
        package_name: &str,
        language: &Language,
    ) -> Result<String> {
        // Extract version from frontmatter for the prompt
        let version = extract_frontmatter_version(skill_md).unwrap_or_default();

        let introspect_prompt = prompts_v2::review_introspect_prompt(
            skill_md,
            package_name,
            &version,
            self.custom_prompt.as_deref(),
            language,
        );

        let script_response = self
            .client
            .complete(&introspect_prompt)
            .await
            .context("review introspection LLM call failed")?;

        // Extract Python code from LLM response (may be wrapped in fences)
        let script = extract_python_script(&script_response);
        if script.is_empty() {
            anyhow::bail!("LLM returned empty introspection script");
        }

        debug!("review: introspection script ({} bytes)", script.len());

        // Run in container
        let executor = ContainerExecutor::new(self.container_config.clone(), Language::Python);
        let env = executor.setup_environment(&[]).await?;

        let result = executor.run_code(&env, &script).await;
        let _ = executor.cleanup(&env).await;

        match result {
            Ok(ExecutionResult::Pass(stdout)) => {
                // Verify the output is valid JSON — if the script printed garbage,
                // treat it as a failure so the verdict LLM ignores it cleanly.
                let trimmed = stdout.trim();
                if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
                    Ok(stdout)
                } else {
                    warn!("  review: introspection script output is not JSON, ignoring");
                    Ok("INTROSPECTION SKIPPED: script did not produce valid JSON".to_string())
                }
            }
            Ok(ExecutionResult::Fail(stderr)) => {
                warn!(
                    "  review: introspection script failed: {}",
                    stderr.chars().take(200).collect::<String>()
                );
                Ok("INTROSPECTION SKIPPED: script execution failed".to_string())
            }
            Ok(ExecutionResult::Timeout) => {
                Ok("INTROSPECTION SKIPPED: script timed out".to_string())
            }
            Err(e) => Err(e),
        }
    }

    /// Format review issues as feedback for the create agent.
    /// Includes introspection data (actual signatures) when available so create
    /// can copy correct signatures instead of guessing from training data.
    pub fn format_feedback(result: &ReviewResult) -> String {
        if result.issues.is_empty() {
            return String::new();
        }

        let mut feedback = String::from(
            "REVIEW FAILED — Fix the following issues. Do NOT regenerate from scratch.\n\n",
        );

        let accuracy_issues: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.category != "safety")
            .collect();
        let safety_issues: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.category == "safety")
            .collect();

        if !accuracy_issues.is_empty() {
            feedback.push_str("ACCURACY ISSUES:\n");
            for (i, issue) in accuracy_issues.iter().enumerate() {
                feedback.push_str(&format!(
                    "{}. {}\n   Evidence: {}\n",
                    i + 1,
                    issue.complaint,
                    issue.evidence
                ));
            }
            feedback.push('\n');
        }

        if !safety_issues.is_empty() {
            feedback.push_str("SAFETY ISSUES:\n");
            for (i, issue) in safety_issues.iter().enumerate() {
                feedback.push_str(&format!("{}. {}\n", i + 1, issue.complaint));
            }
            feedback.push('\n');
        } else {
            feedback.push_str("SAFETY ISSUES: None\n\n");
        }

        // Include introspection data so create can see actual signatures
        if let Some(ref introspection) = result.introspection_output {
            // Truncate to avoid blowing context on huge outputs
            let truncated: String = introspection.chars().take(3000).collect();
            // Escape triple backticks so they don't break the fenced code block
            let sanitized = truncated.replace("```", "\\`\\`\\`");
            feedback.push_str(&format!(
                "INTROSPECTION DATA (actual library state from container):\n```json\n{}\n```\n\n\
                 Use the signatures and imports above as ground truth — your training data may be outdated.\n\n",
                sanitized
            ));
        }

        feedback.push_str(
            "Instructions:\n\
             - Fix ONLY the listed issues\n\
             - Keep all other content EXACTLY as-is\n\
             - Output the complete SKILL.md\n",
        );

        feedback
    }
}

/// Parse the LLM's JSON verdict response into a ReviewResult.
///
/// When `strict` is false (pipeline mode), unparseable responses default to pass.
/// When `strict` is true (standalone mode), unparseable responses return an error.
fn parse_review_response(response: &str, strict: bool) -> Result<ReviewResult> {
    // Try to extract JSON from the response (may be wrapped in fences or have preamble)
    let json_str = extract_json_block(response);

    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            warn!("review: failed to parse verdict JSON: {}", e);
            if strict {
                anyhow::bail!(
                    "review: LLM returned unparseable response (strict mode). Raw response:\n{}",
                    response.chars().take(500).collect::<String>()
                );
            }
            // Conservative: treat parse failure as pass (don't block pipeline)
            let lower = response.to_lowercase();
            if !lower.contains("\"passed\": true") && !lower.contains("\"passed\":true") {
                warn!("review: treating unparseable response as pass");
            }
            return Ok(ReviewResult {
                malformed: true,
                ..ReviewResult::default()
            });
        }
    };

    let issues: Vec<ReviewIssue> = parsed
        .get("issues")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(ReviewIssue {
                        severity: item
                            .get("severity")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Severity::from_str(s).ok())
                            .unwrap_or(Severity::Error),
                        category: item
                            .get("category")
                            .and_then(|v| v.as_str())
                            .unwrap_or("accuracy")
                            .to_string(),
                        complaint: item.get("complaint").and_then(|v| v.as_str())?.to_string(),
                        evidence: item
                            .get("evidence")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Recompute `passed` from the issues list — trust the structured issues,
    // not the LLM's boolean verdict. An LLM saying "passed: true" with
    // error-severity issues should NOT pass.
    let has_errors = issues
        .iter()
        .any(|i| i.severity != Severity::Warning && i.severity != Severity::Info);
    let passed = !has_errors;

    Ok(ReviewResult {
        passed,
        issues,
        malformed: false,
        introspection_output: None,
        degraded: false,
    })
}

/// Extract a JSON object from a string that may have markdown fences or preamble text.
fn extract_json_block(text: &str) -> String {
    let trimmed = text.trim();

    // Use shared line-anchored parser to avoid truncation on inner fences
    let blocks = crate::util::find_fenced_blocks(trimmed);

    // Prefer json-tagged block
    if let Some((_, body)) = blocks.iter().find(|(tag, _)| tag == "json") {
        return body.clone();
    }

    // Fall back to first untagged block that looks like JSON
    if let Some((_, body)) = blocks
        .iter()
        .find(|(tag, body)| tag.is_empty() && body.trim_start().starts_with('{'))
    {
        return body.clone();
    }

    // Try: find first { and last }
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return trimmed[start..=end].to_string();
            }
        }
    }

    trimmed.to_string()
}

/// Extract a Python script from an LLM response that may include markdown fences.
/// Two-pass: prefer Python-tagged blocks, fall back to untagged/generic blocks.
fn extract_python_script(response: &str) -> String {
    let trimmed = response.trim();

    const PYTHON_TAGS: &[&str] = &["python", "python3", "py"];
    const NON_PYTHON_TAGS: &[&str] = &[
        "json",
        "bash",
        "sh",
        "shell",
        "zsh",
        "text",
        "txt",
        "yaml",
        "yml",
        "toml",
        "sql",
        "javascript",
        "js",
        "typescript",
        "ts",
        "go",
        "rust",
        "html",
        "css",
        "xml",
    ];

    // Handle unclosed Python fences: strip the opener line and return the rest
    if let Some(nl) = trimmed.find('\n') {
        let header = trimmed[..nl].trim();
        let is_python_opener = PYTHON_TAGS
            .iter()
            .any(|tag| header == format!("```{tag}") || header == format!("~~~{tag}"));
        if is_python_opener {
            let rest = trimmed[nl + 1..].trim();
            // If there's a matching close fence, find_fenced_blocks will handle it;
            // this path only fires when the fence is unclosed.
            let blocks = crate::util::find_fenced_blocks(trimmed);
            if blocks.is_empty() && !rest.is_empty() {
                return rest.to_string();
            }
        }
    }

    let blocks = crate::util::find_fenced_blocks(trimmed);

    // Pass 1: prefer Python-tagged blocks
    for (tag, body) in &blocks {
        if PYTHON_TAGS.contains(&tag.as_str()) {
            return body.clone();
        }
    }

    // Pass 2: fall back to first untagged or unknown block (skip non-Python tags and JSON)
    for (tag, body) in &blocks {
        if NON_PYTHON_TAGS.contains(&tag.as_str()) {
            continue;
        }
        if body.starts_with('{') {
            continue;
        }
        return body.clone();
    }

    // No fences — return as-is if it looks like Python
    if trimmed.contains("import ") || trimmed.contains("def ") || trimmed.starts_with('#') {
        return trimmed.to_string();
    }

    String::new()
}

/// Extract version from SKILL.md YAML frontmatter.
fn extract_frontmatter_version(skill_md: &str) -> Option<String> {
    if !skill_md.starts_with("---") {
        return None;
    }
    let end = skill_md[3..].find("---")?;
    let frontmatter = &skill_md[3..3 + end];
    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("version:") {
            let version = rest.trim().trim_matches('"').trim_matches('\'');
            if !version.is_empty() && version != "unknown" {
                return Some(version.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_review_response_passed() {
        let json = r#"{"passed": true, "issues": []}"#;
        let result = parse_review_response(json, false).unwrap();
        assert!(result.passed);
        assert!(result.issues.is_empty());
        assert!(!result.malformed, "valid JSON should not be malformed");
    }

    #[test]
    fn test_parse_review_response_failed_with_issues() {
        let json = r#"{
            "passed": false,
            "issues": [
                {
                    "severity": "error",
                    "category": "accuracy",
                    "complaint": "Wrong weekday for 2019-10-17",
                    "evidence": "datetime says Thursday, SKILL.md says Tuesday"
                },
                {
                    "severity": "warning",
                    "category": "consistency",
                    "complaint": "Version mismatch",
                    "evidence": "pip show says 3.10.8, frontmatter says 3.10.7"
                }
            ]
        }"#;
        let result = parse_review_response(json, false).unwrap();
        assert!(!result.passed);
        assert_eq!(result.issues.len(), 2);
        assert_eq!(result.issues[0].category, "accuracy");
        assert_eq!(result.issues[0].severity, Severity::Error);
        assert!(result.issues[0].complaint.contains("weekday"));
        assert_eq!(result.issues[1].category, "consistency");
    }

    #[test]
    fn test_parse_review_response_in_json_fence() {
        let response = "Here's my verdict:\n```json\n{\"passed\": true, \"issues\": []}\n```\n";
        let result = parse_review_response(response, false).unwrap();
        assert!(result.passed);
    }

    #[test]
    fn test_parse_review_response_in_plain_fence() {
        let response = "```\n{\"passed\": false, \"issues\": [{\"complaint\": \"bad sig\", \"severity\": \"error\", \"category\": \"accuracy\", \"evidence\": \"inspect says X\"}]}\n```";
        let result = parse_review_response(response, false).unwrap();
        assert!(!result.passed);
        assert_eq!(result.issues.len(), 1);
    }

    #[test]
    fn test_parse_review_response_malformed_treats_as_pass() {
        let response = "I couldn't analyze this properly, here are some thoughts...";
        let result = parse_review_response(response, false).unwrap();
        assert!(result.passed); // Conservative: unparseable = pass (pipeline mode)
        assert!(
            result.malformed,
            "malformed should be true on parse failure"
        );
    }

    #[test]
    fn test_parse_review_response_malformed_strict_errors() {
        let response = "I couldn't analyze this properly, here are some thoughts...";
        let result = parse_review_response(response, true);
        assert!(result.is_err()); // Strict mode: unparseable = error
        assert!(result.unwrap_err().to_string().contains("unparseable"));
    }

    #[test]
    fn test_parse_review_response_missing_fields_defaults() {
        let json = r#"{"passed": false, "issues": [{"complaint": "something wrong"}]}"#;
        let result = parse_review_response(json, false).unwrap();
        assert!(!result.passed);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].severity, Severity::Error); // default
        assert_eq!(result.issues[0].category, "accuracy"); // default
        assert_eq!(result.issues[0].evidence, ""); // default
    }

    #[test]
    fn test_parse_review_response_issues_without_complaint_skipped() {
        let json =
            r#"{"passed": false, "issues": [{"severity": "error"}, {"complaint": "real issue"}]}"#;
        let result = parse_review_response(json, false).unwrap();
        assert_eq!(result.issues.len(), 1); // First issue skipped (no complaint)
        assert_eq!(result.issues[0].complaint, "real issue");
    }

    #[test]
    fn test_format_feedback_empty() {
        let result = ReviewResult::default();
        assert!(ReviewAgent::format_feedback(&result).is_empty());
    }

    #[test]
    fn test_format_feedback_accuracy_only() {
        let result = ReviewResult {
            passed: false,
            malformed: false,
            introspection_output: None,
            degraded: false,
            issues: vec![ReviewIssue {
                severity: Severity::Error,
                category: "accuracy".to_string(),
                complaint: "Wrong signature for set_loglevel".to_string(),
                evidence: "inspect.signature() says (level)".to_string(),
            }],
        };
        let feedback = ReviewAgent::format_feedback(&result);
        assert!(feedback.contains("REVIEW FAILED"));
        assert!(feedback.contains("ACCURACY ISSUES:"));
        assert!(feedback.contains("Wrong signature for set_loglevel"));
        assert!(feedback.contains("inspect.signature() says (level)"));
        assert!(feedback.contains("SAFETY ISSUES: None"));
        assert!(feedback.contains("Fix ONLY the listed issues"));
    }

    #[test]
    fn test_format_feedback_safety_issue() {
        let result = ReviewResult {
            passed: false,
            malformed: false,
            introspection_output: None,
            degraded: false,
            issues: vec![ReviewIssue {
                severity: Severity::Error,
                category: "safety".to_string(),
                complaint: "Hidden instruction detected".to_string(),
                evidence: String::new(),
            }],
        };
        let feedback = ReviewAgent::format_feedback(&result);
        assert!(feedback.contains("SAFETY ISSUES:"));
        assert!(feedback.contains("Hidden instruction detected"));
    }

    #[test]
    fn test_extract_json_block_plain() {
        assert_eq!(
            extract_json_block(r#"{"passed": true}"#),
            r#"{"passed": true}"#
        );
    }

    #[test]
    fn test_extract_json_block_with_preamble() {
        let text = "Here's the result:\n{\"passed\": false, \"issues\": []}";
        assert_eq!(
            extract_json_block(text),
            r#"{"passed": false, "issues": []}"#
        );
    }

    #[test]
    fn test_extract_json_block_in_fence() {
        let text = "```json\n{\"passed\": true}\n```";
        assert_eq!(extract_json_block(text), r#"{"passed": true}"#);
    }

    #[test]
    fn test_extract_python_script_in_fence() {
        let text = "Here's the script:\n```python\nimport json\nprint('hello')\n```\n";
        let script = extract_python_script(text);
        assert!(script.contains("import json"));
        assert!(script.contains("print('hello')"));
    }

    #[test]
    fn test_extract_python_script_plain_fence() {
        let text = "```\nimport sys\nprint(sys.version)\n```";
        let script = extract_python_script(text);
        assert!(script.contains("import sys"));
    }

    #[test]
    fn test_extract_python_script_no_fence() {
        let text = "import json\nprint(json.dumps({'a': 1}))";
        let script = extract_python_script(text);
        assert!(script.contains("import json"));
    }

    #[test]
    fn test_extract_python_script_empty_for_non_python() {
        let text = "I don't know what to check.";
        let script = extract_python_script(text);
        assert!(script.is_empty());
    }

    #[test]
    fn test_extract_json_block_unclosed_json_fence() {
        // ```json with no closing ``` — falls through to brace search
        let text = "```json\n{\"a\": 1}\n";
        let result = extract_json_block(text);
        assert_eq!(result, r#"{"a": 1}"#);
    }

    #[test]
    fn test_extract_json_block_plain_fence_non_json() {
        // Plain ``` fence with non-JSON content — falls through
        let text = "```\nhello world\n```";
        let result = extract_json_block(text);
        assert_eq!(result, text.trim()); // no braces, returns as-is
    }

    #[test]
    fn test_extract_json_block_brace_before_end() {
        // Edge case: } appears before { (malformed)
        let text = "} some text {";
        let result = extract_json_block(text);
        // find('{') at 12, rfind('}') at 0 → end < start → falls through
        assert_eq!(result, text.trim());
    }

    #[test]
    fn test_extract_json_block_tilde_json_fence() {
        let text = "~~~json\n{\"passed\": true, \"issues\": []}\n~~~";
        let result = extract_json_block(text);
        assert_eq!(result, "{\"passed\": true, \"issues\": []}");
    }

    #[test]
    fn test_extract_json_block_tilde_plain_fence() {
        let text = "~~~\n{\"key\": \"value\"}\n~~~";
        let result = extract_json_block(text);
        assert_eq!(result, "{\"key\": \"value\"}");
    }

    #[test]
    fn test_extract_python_script_unclosed_python_fence() {
        // ```python with no closing ``` — strip opener, return body only
        let text = "```python\nimport os\n";
        let script = extract_python_script(text);
        assert_eq!(script, "import os");
    }

    #[test]
    fn test_extract_python_script_plain_fence_json_content() {
        // Plain ``` fence with JSON inside — should be skipped (not Python)
        let text = "```\n{\"key\": \"value\"}\n```";
        let script = extract_python_script(text);
        // inner starts with '{', so it's skipped; falls through to "no fence"
        // No "import " or "def " or "#" → returns empty
        assert!(script.is_empty());
    }

    #[test]
    fn test_extract_python_script_tilde_fence() {
        let response = "Here's the script:\n~~~python\nimport json\nprint('ok')\n~~~\n";
        let script = extract_python_script(response);
        assert_eq!(script, "import json\nprint('ok')");
    }

    #[test]
    fn test_extract_python_script_tilde_plain_fence() {
        let response = "~~~\nimport os\n~~~";
        let script = extract_python_script(response);
        assert_eq!(script, "import os");
    }

    #[test]
    fn test_extract_python_script_python3_fence() {
        let response = "```python3\nimport json\nprint('ok')\n```";
        let script = extract_python_script(response);
        assert_eq!(script, "import json\nprint('ok')");
    }

    #[test]
    fn test_extract_python_script_py_fence() {
        let response = "~~~py\nimport os\nprint('done')\n~~~";
        let script = extract_python_script(response);
        assert_eq!(script, "import os\nprint('done')");
    }

    #[test]
    fn test_extract_python_script_skips_json_tagged_block() {
        // JSON-tagged block should be skipped; Python block should be extracted.
        let response =
            "```json\n{\"key\": \"value\"}\n```\n\n```python\nimport os\nprint('done')\n```";
        let script = extract_python_script(response);
        assert!(
            script.contains("import os"),
            "should extract Python block, got: {script}"
        );
        assert!(
            !script.contains("\"key\""),
            "should NOT extract the JSON block"
        );
    }

    #[test]
    fn test_extract_python_script_skips_bash_block() {
        let response = "```bash\npip install foo\n```\n\n```python\nimport foo\n```";
        let script = extract_python_script(response);
        assert_eq!(script, "import foo");
    }

    #[test]
    fn test_extract_python_script_pass2_skips_non_python_tags() {
        // No Python-tagged blocks → Pass 2 must skip bash and pick the untagged block.
        let response = "```bash\npip install foo\n```\n\n```\nimport foo\nprint('ok')\n```";
        let script = extract_python_script(response);
        assert!(
            script.contains("import foo"),
            "Pass 2 should skip bash and pick generic block, got: {script}"
        );
        assert!(!script.contains("pip install"));
    }

    #[test]
    fn test_find_fenced_blocks_tilde_before_backtick() {
        let text = "~~~json\n{}\n~~~\n\n```python\nimport os\n```";
        let blocks = crate::util::find_fenced_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "json");
        assert_eq!(blocks[1].0, "python");
    }

    #[test]
    fn test_find_fenced_blocks_backtick_before_tilde() {
        // When ``` appears before ~~~ in same text, backtick is parsed first.
        let text = "```python\nfirst\n```\n\n~~~json\n{}\n~~~";
        let blocks = crate::util::find_fenced_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "python");
        assert_eq!(blocks[1].0, "json");
    }

    #[test]
    fn test_find_fenced_blocks_single_line_no_block() {
        // Single-line ```code``` — closing fence not at line boundary, no block extracted.
        let text = "```code```";
        let blocks = crate::util::find_fenced_blocks(text);
        assert!(
            blocks.is_empty(),
            "single-line fence is not valid CommonMark"
        );
    }

    #[test]
    fn test_find_fenced_blocks_unclosed_fence() {
        let text = "```python\nimport os\n";
        let blocks = crate::util::find_fenced_blocks(text);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_extract_frontmatter_version() {
        let md = "---\nname: numpy\nversion: 2.1.0\nlanguage: python\n---\n# Content";
        assert_eq!(extract_frontmatter_version(md), Some("2.1.0".to_string()));
    }

    #[test]
    fn test_extract_frontmatter_version_quoted() {
        let md = "---\nname: arrow\nversion: \"1.3.0\"\n---\n# Content";
        assert_eq!(extract_frontmatter_version(md), Some("1.3.0".to_string()));
    }

    #[test]
    fn test_extract_frontmatter_version_missing() {
        let md = "---\nname: arrow\n---\n# Content";
        assert_eq!(extract_frontmatter_version(md), None);
    }

    #[test]
    fn test_extract_frontmatter_version_unknown() {
        let md = "---\nname: arrow\nversion: unknown\n---\n# Content";
        assert_eq!(extract_frontmatter_version(md), None);
    }

    #[test]
    fn test_extract_frontmatter_version_nested_metadata() {
        // Normalizer canonical format: version under metadata:
        let md = "---\nname: arrow\ndescription: python library\nmetadata:\n  version: \"1.4.0\"\n  ecosystem: python\n---\n# Content";
        assert_eq!(extract_frontmatter_version(md), Some("1.4.0".to_string()));
    }

    #[test]
    fn test_extract_frontmatter_no_frontmatter() {
        let md = "# No frontmatter here";
        assert_eq!(extract_frontmatter_version(md), None);
    }

    #[test]
    fn test_parse_review_response_malformed_but_contains_passed_true() {
        // The LLM returned invalid JSON but the text contains "passed": true
        let response = "I found no issues. The result is \"passed\": true so all good.";
        let result = parse_review_response(response, false).unwrap();
        assert!(result.passed); // Fallback detects "passed": true pattern
        assert!(
            result.malformed,
            "malformed should be true even when text heuristic matches"
        );
    }

    #[test]
    fn test_format_feedback_mixed_issues() {
        let result = ReviewResult {
            passed: false,
            malformed: false,
            introspection_output: None,
            degraded: false,
            issues: vec![
                ReviewIssue {
                    severity: Severity::Error,
                    category: "accuracy".to_string(),
                    complaint: "Wrong signature".to_string(),
                    evidence: "expected (x, y)".to_string(),
                },
                ReviewIssue {
                    severity: Severity::Error,
                    category: "safety".to_string(),
                    complaint: "Prompt injection found".to_string(),
                    evidence: String::new(),
                },
            ],
        };
        let feedback = ReviewAgent::format_feedback(&result);
        assert!(feedback.contains("ACCURACY ISSUES:"));
        assert!(feedback.contains("SAFETY ISSUES:"));
        assert!(feedback.contains("Wrong signature"));
        assert!(feedback.contains("Prompt injection found"));
        assert!(!feedback.contains("SAFETY ISSUES: None"));
    }

    #[test]
    fn test_review_result_default() {
        let result = ReviewResult::default();
        assert!(result.passed);
        assert!(result.issues.is_empty());
    }

    // --- Strict mode + degraded introspection ---

    #[tokio::test]
    async fn test_strict_mode_introspection_degraded_fails() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient::new();
        let container_config = crate::config::ContainerConfig {
            runtime: "__missing_runtime__".to_string(),
            ..Default::default()
        };
        let agent = ReviewAgent::new(&client, container_config, None).with_strict(true);

        // Container will fail (runtime doesn't exist), but review() catches container
        // errors and proceeds with degraded introspection + LLM verdict. MockLlmClient
        // returns a valid verdict, so review() always returns Ok here.
        let r = agent
            .review(
                "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test",
                "testpkg",
                &Language::Python,
            )
            .await
            .expect("review should succeed with degraded introspection");

        assert!(
            !r.passed,
            "strict mode should fail when introspection is degraded"
        );
        assert!(
            r.issues.iter().any(|i| i.category == "introspection"),
            "should have an introspection issue"
        );
        assert!(
            r.issues
                .iter()
                .any(|i| i.category == "introspection" && i.severity == Severity::Error),
            "introspection issue should have error severity"
        );
    }

    #[tokio::test]
    async fn test_advisory_mode_introspection_degraded_passes_on_verdict() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient::new();
        let container_config = crate::config::ContainerConfig {
            runtime: "__missing_runtime__".to_string(),
            ..Default::default()
        };
        let agent = ReviewAgent::new(&client, container_config, None).with_strict(false);

        // In advisory (non-strict) mode, degraded introspection should NOT
        // override the LLM verdict. Runtime doesn't exist so container fails,
        // review() catches it and proceeds to Phase B. MockLlmClient returns passed=true.
        let r = agent
            .review(
                "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test",
                "testpkg",
                &Language::Python,
            )
            .await
            .expect("review should succeed in advisory mode");

        // Advisory mode: passed is determined by the LLM verdict alone.
        // Introspection degradation adds a warning but does not fail the review.
        assert!(
            r.passed,
            "advisory mode should pass when LLM verdict passes"
        );
        let intro_issues: Vec<_> = r
            .issues
            .iter()
            .filter(|i| i.category == "introspection")
            .collect();
        assert!(
            !intro_issues.is_empty(),
            "degraded introspection should produce at least one issue"
        );
        assert!(
            intro_issues
                .iter()
                .all(|i| matches!(i.severity, Severity::Warning)),
            "advisory mode should only add warning-level introspection issues, not errors"
        );
    }

    #[test]
    fn test_strict_introspection_degraded_unit() {
        // Unit test: simulate the strict-mode gate logic directly.
        // This tests the exact code path without needing a container or LLM.
        let verdict_json = r#"{"passed": true, "issues": []}"#;
        let mut result = parse_review_response(verdict_json, true).unwrap();
        assert!(result.passed, "verdict alone says passed");

        // Simulate degraded introspection in strict mode
        let introspection_degraded = true;
        let strict = true;
        let introspection_output = "INTROSPECTION SKIPPED: script execution failed";

        if strict && introspection_degraded {
            result.passed = false;
            result.issues.push(ReviewIssue {
                severity: Severity::Error,
                category: "introspection".to_string(),
                complaint:
                    "Container introspection failed — verdict is based on textual analysis only"
                        .to_string(),
                evidence: introspection_output.chars().take(500).collect(),
            });
        }

        assert!(
            !result.passed,
            "strict + degraded should override verdict to failed"
        );
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].category, "introspection");
        assert_eq!(result.issues[0].severity, Severity::Error);
    }

    #[test]
    fn test_introspection_failed_not_persisted() {
        // BUG 1 fix: "INTROSPECTION FAILED: ..." should NOT be stored as
        // introspection_output — only valid JSON (starting with { or [) should.
        let failed_msg = "INTROSPECTION FAILED: container OOM";
        let trimmed = failed_msg.trim();
        // Simulate the guard from review()
        let stored = if trimmed.starts_with('{') || trimmed.starts_with('[') {
            Some(failed_msg.to_string())
        } else {
            None
        };
        assert!(
            stored.is_none(),
            "INTROSPECTION FAILED messages must not be persisted"
        );

        // Valid JSON should be persisted
        let valid_json = r#"{"functions": ["foo", "bar"]}"#;
        let trimmed = valid_json.trim();
        let stored = if trimmed.starts_with('{') || trimmed.starts_with('[') {
            Some(valid_json.to_string())
        } else {
            None
        };
        assert!(
            stored.is_some(),
            "valid JSON introspection should be persisted"
        );
    }

    #[test]
    fn test_format_feedback_escapes_triple_backticks_in_introspection() {
        // BUG 2 fix: if introspection payload contains ```, they must be escaped
        // so they don't break the fenced code block in the feedback prompt.
        let result = ReviewResult {
            passed: false,
            malformed: false,
            degraded: false,
            introspection_output: Some(r#"{"example": "```python\nprint('hi')\n```"}"#.to_string()),
            issues: vec![ReviewIssue {
                severity: Severity::Error,
                category: "accuracy".to_string(),
                complaint: "Wrong signature".to_string(),
                evidence: "test".to_string(),
            }],
        };
        let feedback = ReviewAgent::format_feedback(&result);
        // The raw triple backticks from the payload must be escaped
        assert!(
            !feedback.contains(r#"```python"#),
            "raw triple backticks in introspection should be escaped"
        );
        assert!(
            feedback.contains(r#"\`\`\`python"#),
            "escaped backticks should appear in feedback"
        );
        // The fencing backticks (```json and closing ```) should still be intact
        assert!(feedback.contains("```json"), "fence opener must be present");
    }

    #[test]
    fn test_advisory_introspection_degraded_unit() {
        // Unit test: advisory mode adds a warning but does NOT override the verdict.
        let verdict_json = r#"{"passed": true, "issues": []}"#;
        let mut result = parse_review_response(verdict_json, false).unwrap();

        let introspection_degraded = true;
        let strict = false;
        let introspection_output = "INTROSPECTION FAILED: container OOM";

        // Mirrors the updated review() logic: always add issue on degradation
        if introspection_degraded {
            let severity = if strict {
                Severity::Error
            } else {
                Severity::Warning
            };
            if strict {
                result.passed = false;
            }
            result.issues.push(ReviewIssue {
                severity,
                category: "introspection".to_string(),
                complaint:
                    "Container introspection failed — verdict is based on textual analysis only"
                        .to_string(),
                evidence: introspection_output.chars().take(500).collect(),
            });
        }

        assert!(result.passed, "advisory mode should not override verdict");
        assert_eq!(result.issues.len(), 1, "warning issue should be added");
        assert!(
            matches!(result.issues[0].severity, Severity::Warning),
            "advisory mode should add warning, not error"
        );
    }

    #[test]
    fn test_review_result_degraded_flag_defaults_false() {
        let result = ReviewResult::default();
        assert!(
            !result.degraded,
            "default ReviewResult should not be degraded"
        );
    }

    #[test]
    fn test_parse_review_response_sets_degraded_false() {
        let json = r#"{"passed": true, "issues": []}"#;
        let result = parse_review_response(json, false).unwrap();
        assert!(!result.degraded, "parsed result should start non-degraded");
    }

    #[test]
    fn test_print_review_issues_runs_without_panic() {
        let issues = vec![
            ReviewIssue {
                severity: Severity::Error,
                category: "accuracy".to_string(),
                complaint: "Wrong version".to_string(),
                evidence: "says 1.0, actually 2.0".to_string(),
            },
            ReviewIssue {
                severity: Severity::Warning,
                category: "style".to_string(),
                complaint: "Minor nit".to_string(),
                evidence: String::new(), // empty evidence branch
            },
        ];
        // Exercises both the evidence and no-evidence branches
        print_review_issues(&issues);
    }
}
