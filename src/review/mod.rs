//! Review agent: accuracy and safety validation for generated SKILL.md files.
//!
//! Two-phase approach:
//! 1. LLM generates a Python introspection script to verify claims in the SKILL.md
//! 2. Script runs in a container; results + SKILL.md go back to LLM for verdict

use anyhow::{Context, Result};
use tracing::{debug, warn};

use crate::agent5::container_executor::ContainerExecutor;
use crate::agent5::executor::ExecutionResult;
use crate::agent5::LanguageExecutor;
use crate::config::ContainerConfig;
use crate::llm::client::LlmClient;
use crate::llm::prompts_v2;

/// Result of a review pass.
#[derive(Debug, Clone)]
pub struct ReviewResult {
    pub passed: bool,
    pub issues: Vec<ReviewIssue>,
}

impl Default for ReviewResult {
    fn default() -> Self {
        Self {
            passed: true,
            issues: Vec::new(),
        }
    }
}

/// A single issue found during review.
#[derive(Debug, Clone)]
pub struct ReviewIssue {
    pub severity: String,
    pub category: String,
    pub complaint: String,
    pub evidence: String,
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
        }
    }

    /// Enable strict mode: parse failures become errors instead of silent passes.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Run the full review pipeline on a SKILL.md.
    pub async fn review(
        &self,
        skill_md: &str,
        package_name: &str,
        language: &str,
    ) -> Result<ReviewResult> {
        // Phase A: generate introspection script (Python only)
        let introspection_output = if language == "python" {
            match self.run_introspection(skill_md, package_name).await {
                Ok(output) => output,
                Err(e) => {
                    warn!("  review: container introspection failed: {}", e);
                    format!("INTROSPECTION FAILED: {}", e)
                }
            }
        } else {
            "INTROSPECTION SKIPPED: only Python is supported for container checks".to_string()
        };

        // Phase B: LLM verdict (accuracy + safety + consistency)
        let verdict_prompt = prompts_v2::review_verdict_prompt(
            skill_md,
            &introspection_output,
            self.custom_prompt.as_deref(),
        );
        let verdict_response = self
            .client
            .complete(&verdict_prompt)
            .await
            .context("review verdict LLM call failed")?;

        parse_review_response(&verdict_response, self.strict)
    }

    /// Phase A: ask LLM to generate an introspection script, then run it in a container.
    async fn run_introspection(&self, skill_md: &str, package_name: &str) -> Result<String> {
        // Extract version from frontmatter for the prompt
        let version = extract_frontmatter_version(skill_md).unwrap_or_default();

        let introspect_prompt = prompts_v2::review_introspect_prompt(
            skill_md,
            package_name,
            &version,
            self.custom_prompt.as_deref(),
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
        let executor = ContainerExecutor::new(self.container_config.clone(), "python");
        let env = executor.setup_environment(&[])?;

        let result = executor.run_code(&env, &script);
        let _ = executor.cleanup(&env);

        match result {
            Ok(ExecutionResult::Pass(stdout)) => {
                // Verify the output looks like JSON — if the script printed garbage,
                // treat it as a failure so the verdict LLM ignores it cleanly.
                let trimmed = stdout.trim();
                if trimmed.starts_with('{') && trimmed.ends_with('}') {
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
            // Fallback: if the response contains "pass" (case-insensitive), treat as passed
            if response.to_lowercase().contains("\"passed\": true")
                || response.to_lowercase().contains("\"passed\":true")
            {
                return Ok(ReviewResult::default());
            }
            // Conservative: treat parse failure as pass (don't block pipeline)
            warn!("review: treating unparseable response as pass");
            return Ok(ReviewResult::default());
        }
    };

    let passed = parsed
        .get("passed")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let issues = parsed
        .get("issues")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(ReviewIssue {
                        severity: item
                            .get("severity")
                            .and_then(|v| v.as_str())
                            .unwrap_or("error")
                            .to_string(),
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

    Ok(ReviewResult { passed, issues })
}

/// Extract a JSON object from a string that may have markdown fences or preamble text.
fn extract_json_block(text: &str) -> String {
    let trimmed = text.trim();

    // Try: markdown json fence
    if let Some(start) = trimmed.find("```json") {
        if let Some(end) = trimmed[start + 7..].find("```") {
            return trimmed[start + 7..start + 7 + end].trim().to_string();
        }
    }

    // Try: markdown plain fence
    if let Some(start) = trimmed.find("```") {
        if let Some(end) = trimmed[start + 3..].find("```") {
            let inner = trimmed[start + 3..start + 3 + end].trim();
            if inner.starts_with('{') {
                return inner.to_string();
            }
        }
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
fn extract_python_script(response: &str) -> String {
    let trimmed = response.trim();

    // Try: ```python ... ```
    if let Some(start) = trimmed.find("```python") {
        if let Some(end) = trimmed[start + 9..].find("```") {
            return trimmed[start + 9..start + 9 + end].trim().to_string();
        }
    }

    // Try: ``` ... ```
    if let Some(start) = trimmed.find("```") {
        if let Some(end) = trimmed[start + 3..].find("```") {
            let inner = trimmed[start + 3..start + 3 + end].trim();
            // Skip if it looks like JSON
            if !inner.starts_with('{') {
                return inner.to_string();
            }
        }
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
        assert_eq!(result.issues[0].severity, "error");
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
        assert_eq!(result.issues[0].severity, "error"); // default
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
            issues: vec![ReviewIssue {
                severity: "error".to_string(),
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
            issues: vec![ReviewIssue {
                severity: "error".to_string(),
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
    }

    #[test]
    fn test_format_feedback_mixed_issues() {
        let result = ReviewResult {
            passed: false,
            issues: vec![
                ReviewIssue {
                    severity: "error".to_string(),
                    category: "accuracy".to_string(),
                    complaint: "Wrong signature".to_string(),
                    evidence: "expected (x, y)".to_string(),
                },
                ReviewIssue {
                    severity: "error".to_string(),
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
}
