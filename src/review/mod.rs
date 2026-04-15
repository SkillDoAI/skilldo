//! Review agent: accuracy and safety validation for generated SKILL.md files.
//!
//! LLM-only verdict: evaluates the SKILL.md for accuracy, safety, and consistency.

use anyhow::{Context, Result};
use std::str::FromStr;
use tracing::warn;

use crate::detector::Language;
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
    /// Raw LLM verdict response for debug stage dumps.
    pub raw_verdict: String,
}

impl Default for ReviewResult {
    fn default() -> Self {
        Self {
            passed: true,
            issues: Vec::new(),
            malformed: false,
            raw_verdict: String::new(),
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
    write_review_issues(issues, &mut std::io::stdout()).ok();
}

/// Write review issues to the given writer (testable variant).
pub fn write_review_issues(
    issues: &[ReviewIssue],
    out: &mut dyn std::io::Write,
) -> std::io::Result<()> {
    for (i, issue) in issues.iter().enumerate() {
        writeln!(
            out,
            "  {}. [{}][{}] {}",
            i + 1,
            issue.severity,
            issue.category,
            issue.complaint
        )?;
        if !issue.evidence.is_empty() {
            writeln!(out, "     Evidence: {}", issue.evidence)?;
        }
    }
    Ok(())
}

/// Review agent that validates SKILL.md accuracy and safety.
pub struct ReviewAgent<'a> {
    client: &'a dyn LlmClient,
    custom_prompt: Option<String>,
    /// In strict mode, unparseable LLM responses are treated as errors instead of silent passes.
    /// Use strict=true for standalone review (user explicitly asked to review).
    /// Use strict=false in the pipeline (don't block generation on LLM flakiness).
    strict: bool,
}

impl<'a> ReviewAgent<'a> {
    pub fn new(client: &'a dyn LlmClient, custom_prompt: Option<String>) -> Self {
        Self {
            client,
            custom_prompt,
            strict: false,
        }
    }

    /// Enable strict mode: parse failures become errors instead of silent passes.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Run the review on a SKILL.md (LLM verdict only).
    /// Stage outputs are optional context for cross-referencing:
    /// - `api_surface`: extract-stage output (method signatures — ground truth)
    /// - `patterns`: map-stage output (usage patterns from tests)
    /// - `behavioral_semantics`: observable behaviors extracted from learn-stage output
    pub async fn review(
        &self,
        skill_md: &str,
        language: &Language,
        api_surface: Option<&str>,
        patterns: Option<&str>,
        behavioral_semantics: Option<&str>,
    ) -> Result<ReviewResult> {
        // LLM verdict (accuracy + safety + consistency)
        let verdict_prompt = prompts_v2::review_verdict_prompt(
            skill_md,
            self.custom_prompt.as_deref(),
            language,
            api_surface,
            patterns,
            behavioral_semantics,
        );
        self.call_with_split(&verdict_prompt).await
    }

    /// Split the prompt at the `SKILL.MD UNDER REVIEW:` boundary and call the
    /// LLM with system/user separation.  Falls back to a single-message call
    /// if the marker is absent (defensive — current templates always include it).
    async fn call_with_split(&self, verdict_prompt: &str) -> Result<ReviewResult> {
        let verdict_response =
            if let Some(split_pos) = verdict_prompt.find("SKILL.MD UNDER REVIEW:") {
                let system = &verdict_prompt[..split_pos];
                let user = &verdict_prompt[split_pos..];
                self.client
                    .complete_with_system(system, user)
                    .await
                    .context("review verdict LLM call failed")?
            } else {
                tracing::warn!(
                    "review: 'SKILL.MD UNDER REVIEW:' marker not found in verdict prompt; \
                     falling back to single-message call (system-prompt split disabled)"
                );
                self.client
                    .complete(verdict_prompt)
                    .await
                    .context("review verdict LLM call failed")?
            };

        let mut result = parse_review_response(&verdict_response, self.strict)?;
        result.raw_verdict = verdict_response;

        Ok(result)
    }

    /// Format review issues as feedback for the create agent.
    pub fn format_feedback(result: &ReviewResult) -> String {
        if result.issues.is_empty() {
            return String::new();
        }

        let mut feedback = String::from(
            "REVIEW FAILED — Fix the following issues. Do NOT regenerate from scratch.\n\n",
        );

        let non_safety_issues: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.category != "safety")
            .collect();
        let safety_issues: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.category == "safety")
            .collect();

        if !non_safety_issues.is_empty() {
            feedback.push_str("ISSUES:\n");
            for (i, issue) in non_safety_issues.iter().enumerate() {
                feedback.push_str(&format!(
                    "{}. [{}] {}\n   Evidence: {}\n",
                    i + 1,
                    issue.category,
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
/// When `strict` is false (pipeline mode), unparseable responses return `passed: false`
/// with `malformed: true` — the review gate is NOT bypassed.
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
            // Security fix: treat parse failure as failed (don't silently bypass review gate)
            // but always flag as malformed so the caller knows to retry.
            warn!("review: unparseable response — treating as failed (malformed)");
            return Ok(ReviewResult {
                passed: false,
                malformed: true,
                raw_verdict: response.to_string(),
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
                    // Only process object entries — skip bare strings, numbers, etc.
                    let obj = item.as_object()?;
                    Some(ReviewIssue {
                        severity: obj
                            .get("severity")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Severity::from_str(s).ok())
                            .unwrap_or(Severity::Error),
                        category: obj
                            .get("category")
                            .and_then(|v| v.as_str())
                            .unwrap_or("accuracy")
                            .to_string(),
                        complaint: obj
                            .get("complaint")
                            .and_then(|v| v.as_str())
                            .unwrap_or("(no description)")
                            .to_string(),
                        evidence: obj
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

    // If the LLM said "passed: false" but provided no issues, treat as malformed —
    // something was detected but not articulated. Trigger a retry.
    // Also treat non-boolean `passed` (e.g. "false" string) as malformed.
    let passed_field = parsed.get("passed");
    let (llm_said_passed, passed_type_invalid) = match passed_field {
        Some(v) => match v.as_bool() {
            Some(b) => (b, false),
            None => {
                warn!("review: `passed` field is not a boolean: {:?}", v);
                (false, true)
            }
        },
        None => (true, false), // missing field — assume pass, let issues decide
    };
    // Only mark as malformed if we have NO usable issues. A non-boolean `passed`
    // field is unusual but the issues themselves may still be valid.
    let malformed =
        (passed_type_invalid && issues.is_empty()) || (!llm_said_passed && issues.is_empty());
    if malformed && !passed_type_invalid {
        warn!("review: LLM said passed=false but provided no issues (malformed)");
    }

    let passed = !has_errors && !malformed;

    Ok(ReviewResult {
        passed,
        issues,
        malformed,
        raw_verdict: String::new(), // populated by caller (ReviewAgent::review)
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

    // Try: find first { then scan } positions (last to first) to find
    // the largest valid JSON object. This handles cases like
    // `{valid JSON} extra text }` where first-{-to-last-} would fail.
    if let Some(start) = trimmed.find('{') {
        // Collect all } positions after start, try from last to first
        let brace_positions: Vec<usize> = trimmed[start..]
            .rmatch_indices('}')
            .map(|(i, _)| start + i)
            .collect();
        for end in brace_positions {
            let candidate = &trimmed[start..=end];
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                return candidate.to_string();
            }
        }
    }

    trimmed.to_string()
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
    fn test_parse_review_response_malformed_treats_as_failed() {
        let response = "I couldn't analyze this properly, here are some thoughts...";
        let result = parse_review_response(response, false).unwrap();
        assert!(!result.passed); // Malformed = failed (don't silently bypass review gate)
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
    fn test_parse_review_response_issues_without_complaint_get_default() {
        let json =
            r#"{"passed": false, "issues": [{"severity": "error"}, {"complaint": "real issue"}]}"#;
        let result = parse_review_response(json, false).unwrap();
        assert_eq!(result.issues.len(), 2); // Both kept — missing complaint gets default
        assert_eq!(result.issues[0].complaint, "(no description)");
        assert_eq!(result.issues[1].complaint, "real issue");
        assert!(!result.passed); // error-severity issue present → not passed
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
            raw_verdict: String::new(),
            issues: vec![ReviewIssue {
                severity: Severity::Error,
                category: "accuracy".to_string(),
                complaint: "Wrong signature for set_loglevel".to_string(),
                evidence: "inspect.signature() says (level)".to_string(),
            }],
        };
        let feedback = ReviewAgent::format_feedback(&result);
        assert!(feedback.contains("REVIEW FAILED"));
        assert!(feedback.contains("ISSUES:"));
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
            raw_verdict: String::new(),
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
    fn test_extract_json_block_unclosed_json_fence() {
        let text = "```json\n{\"a\": 1}\n";
        let result = extract_json_block(text);
        assert_eq!(result, r#"{"a": 1}"#);
    }

    #[test]
    fn test_extract_json_block_plain_fence_non_json() {
        let text = "```\nhello world\n```";
        let result = extract_json_block(text);
        assert_eq!(result, text.trim());
    }

    #[test]
    fn test_extract_json_block_brace_before_end() {
        let text = "} some text {";
        let result = extract_json_block(text);
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
    fn test_find_fenced_blocks_tilde_before_backtick() {
        let text = "~~~json\n{}\n~~~\n\n```python\nimport os\n```";
        let blocks = crate::util::find_fenced_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "json");
        assert_eq!(blocks[1].0, "python");
    }

    #[test]
    fn test_find_fenced_blocks_backtick_before_tilde() {
        let text = "```python\nfirst\n```\n\n~~~json\n{}\n~~~";
        let blocks = crate::util::find_fenced_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "python");
        assert_eq!(blocks[1].0, "json");
    }

    #[test]
    fn test_find_fenced_blocks_single_line_no_block() {
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
    fn test_parse_review_response_malformed_but_contains_passed_true() {
        let response = "I found no issues. The result is \"passed\": true so all good.";
        let result = parse_review_response(response, false).unwrap();
        assert!(!result.passed); // Malformed = failed regardless of prose content
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
            raw_verdict: String::new(),
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
        assert!(feedback.contains("ISSUES:"));
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

    /// Non-boolean `passed` (e.g. a string "false") with valid error issues:
    /// issues are trusted, malformed should be false, passed should be false.
    #[test]
    fn test_parse_passed_non_boolean_with_issues_not_malformed() {
        let json = r#"{
            "passed": "false",
            "issues": [
                {
                    "severity": "error",
                    "category": "accuracy",
                    "complaint": "Version mismatch",
                    "evidence": "pip says 3.10, SKILL.md says 3.9"
                }
            ]
        }"#;
        let result = parse_review_response(json, false).unwrap();
        assert!(
            !result.malformed,
            "non-boolean passed with valid issues should NOT be malformed"
        );
        assert!(
            !result.passed,
            "error-severity issue present means not passed"
        );
        assert_eq!(result.issues.len(), 1);
    }

    /// Non-boolean `passed` (e.g. a string) with NO issues:
    /// should be malformed (we can't trust the verdict without issues).
    #[test]
    fn test_parse_passed_non_boolean_without_issues_is_malformed() {
        let json = r#"{"passed": "yes", "issues": []}"#;
        let result = parse_review_response(json, false).unwrap();
        assert!(
            result.malformed,
            "non-boolean passed with no issues should be malformed"
        );
        // malformed results are treated as non-passing
        assert!(!result.passed);
    }

    /// Missing `passed` field entirely: assume pass, let issues decide.
    #[test]
    fn test_parse_missing_passed_field_defaults_to_pass() {
        let json = r#"{"issues": []}"#;
        let result = parse_review_response(json, false).unwrap();
        assert!(result.passed, "missing passed field assumes pass");
        assert!(!result.malformed, "missing passed field is not malformed");
    }

    /// Non-boolean `passed` with only warning-severity issues:
    /// not malformed (issues exist), and passed (no error-severity).
    #[test]
    fn test_parse_passed_non_boolean_with_warning_issues() {
        let json = r#"{
            "passed": "true",
            "issues": [
                {
                    "severity": "warning",
                    "category": "consistency",
                    "complaint": "Minor formatting issue",
                    "evidence": "line 42"
                }
            ]
        }"#;
        let result = parse_review_response(json, false).unwrap();
        assert!(
            !result.malformed,
            "non-boolean passed with issues is not malformed"
        );
        assert!(
            result.passed,
            "only warning-severity issues means still passed"
        );
    }

    #[tokio::test]
    async fn test_review_agent_basic_pass() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient::new();
        let agent = ReviewAgent::new(&client, None).with_strict(false);

        let r = agent
            .review(
                "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test",
                &Language::Python,
                None,
                None,
                None,
            )
            .await
            .expect("review should succeed");

        assert!(r.passed, "MockLlmClient returns passed=true verdict");
    }

    #[tokio::test]
    async fn test_review_agent_strict_mode() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient::new();
        let agent = ReviewAgent::new(&client, None).with_strict(true);

        let r = agent
            .review(
                "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test",
                &Language::Python,
                None,
                None,
                None,
            )
            .await
            .expect("review should succeed");

        assert!(r.passed, "MockLlmClient returns passed=true verdict");
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
                evidence: String::new(),
            },
        ];
        print_review_issues(&issues);
    }

    // ========================================================================
    // call_with_split: marker-found (happy) and marker-missing (fallback)
    // ========================================================================

    use std::sync::{Arc, Mutex};

    /// Mock that records which LlmClient method was invoked and always
    /// returns a valid `passed: true` JSON verdict.
    struct RecordingClient {
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl RecordingClient {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn call_log(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl LlmClient for RecordingClient {
        async fn complete(&self, _prompt: &str) -> anyhow::Result<String> {
            self.calls.lock().unwrap().push("complete");
            Ok(r#"{"passed": true, "issues": []}"#.to_string())
        }

        async fn complete_with_system(&self, _system: &str, _user: &str) -> anyhow::Result<String> {
            self.calls.lock().unwrap().push("complete_with_system");
            Ok(r#"{"passed": true, "issues": []}"#.to_string())
        }
    }

    #[tokio::test]
    async fn test_call_with_split_happy_path_uses_complete_with_system() {
        let client = RecordingClient::new();
        let agent = ReviewAgent::new(&client, None);

        let prompt = "Review instructions here\n\nSKILL.MD UNDER REVIEW:\n# My Skill\n";
        let result = agent.call_with_split(prompt).await.unwrap();

        assert!(result.passed);
        assert_eq!(client.call_log(), vec!["complete_with_system"]);
    }

    #[tokio::test]
    async fn test_call_with_split_fallback_uses_complete() {
        let client = RecordingClient::new();
        let agent = ReviewAgent::new(&client, None);

        // Prompt without the marker — triggers the fallback warn + complete() path
        let prompt = "Review this document for accuracy.\nHere is the content:\n# My Skill\n";
        let result = agent.call_with_split(prompt).await.unwrap();

        assert!(result.passed);
        assert_eq!(client.call_log(), vec!["complete"]);
    }
}
