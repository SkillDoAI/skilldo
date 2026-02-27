use anyhow::Result;
use tracing::{info, warn};

use super::collector::CollectedData;
use super::normalizer;
use crate::agent5::{Agent5CodeValidator, TestResult, ValidationMode};
use crate::config::{ContainerConfig, PromptsConfig};
use crate::lint::{Severity, SkillLinter};
use crate::llm::client::LlmClient;
use crate::llm::prompts_v2;
use crate::validator::{FunctionalValidator, ValidationResult};

/// Strip markdown code fences from output (```markdown ... ``` or ```...```)
fn strip_markdown_fences(content: &str) -> String {
    let trimmed = content.trim();

    // Check for markdown fences with language specifier
    if trimmed.starts_with("```markdown") && trimmed.ends_with("```") {
        return trimmed
            .strip_prefix("```markdown")
            .and_then(|s| s.strip_suffix("```"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| content.to_string());
    }

    // Check for plain markdown fences
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        return trimmed
            .strip_prefix("```")
            .and_then(|s| s.strip_suffix("```"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| content.to_string());
    }

    content.to_string()
}

pub struct Generator {
    client: Box<dyn LlmClient>,
    agent1_client: Option<Box<dyn LlmClient>>,
    agent2_client: Option<Box<dyn LlmClient>>,
    agent3_client: Option<Box<dyn LlmClient>>,
    agent4_client: Option<Box<dyn LlmClient>>,
    agent5_client: Option<Box<dyn LlmClient>>,
    max_retries: usize,
    prompts_config: PromptsConfig,
    enable_agent5: bool,
    agent5_mode: ValidationMode,
    container_config: ContainerConfig,
    parallel_extraction: bool,      // Run agents 1-3 in parallel
    existing_skill: Option<String>, // Existing SKILL.md for update mode
    model_name: Option<String>,     // For generated_with frontmatter field
}

impl Generator {
    pub fn new(client: Box<dyn LlmClient>, max_retries: usize) -> Self {
        Self {
            client,
            agent1_client: None,
            agent2_client: None,
            agent3_client: None,
            agent4_client: None,
            agent5_client: None,
            max_retries,
            prompts_config: PromptsConfig::default(),
            enable_agent5: true,                   // Default to enabled
            agent5_mode: ValidationMode::Thorough, // Default to thorough mode
            container_config: ContainerConfig::default(),
            parallel_extraction: true,
            existing_skill: None,
            model_name: None,
        }
    }

    pub fn with_agent1_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.agent1_client = Some(client);
        self
    }

    pub fn with_agent2_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.agent2_client = Some(client);
        self
    }

    pub fn with_agent3_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.agent3_client = Some(client);
        self
    }

    pub fn with_agent4_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.agent4_client = Some(client);
        self
    }

    pub fn with_agent5_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.agent5_client = Some(client);
        self
    }

    /// Get the LLM client for a specific agent.
    /// Returns the per-agent client if configured, otherwise the main client.
    fn get_client(&self, agent: u8) -> &dyn LlmClient {
        match agent {
            1 => self.agent1_client.as_deref(),
            2 => self.agent2_client.as_deref(),
            3 => self.agent3_client.as_deref(),
            4 => self.agent4_client.as_deref(),
            5 => self.agent5_client.as_deref(),
            _ => None,
        }
        .unwrap_or(self.client.as_ref())
    }

    pub fn with_prompts_config(mut self, config: PromptsConfig) -> Self {
        self.prompts_config = config;
        self
    }

    pub fn with_agent5(mut self, enabled: bool) -> Self {
        self.enable_agent5 = enabled;
        self
    }

    pub fn with_agent5_mode(mut self, mode: ValidationMode) -> Self {
        self.agent5_mode = mode;
        self
    }

    pub fn with_container_config(mut self, config: ContainerConfig) -> Self {
        self.container_config = config;
        self
    }

    pub fn with_parallel_extraction(mut self, parallel: bool) -> Self {
        self.parallel_extraction = parallel;
        self
    }

    pub fn with_existing_skill(mut self, skill: String) -> Self {
        self.existing_skill = Some(skill);
        self
    }

    pub fn with_model_name(mut self, name: String) -> Self {
        self.model_name = Some(name);
        self
    }

    pub async fn generate(&self, data: &CollectedData) -> Result<String> {
        info!("Starting 5-agent pipeline for {}", data.package_name);

        // Combine docs and changelog for agent 3
        let docs_and_changelog = format!("{}\n\n{}", data.docs_content, data.changelog_content);

        // Combine examples and tests for agent 2 (examples first - they're cleaner)
        let examples_and_tests = if !data.examples_content.is_empty() {
            format!(
                "# Example Files (Real Usage)\n{}\n\n# Test Files (API Usage)\n{}",
                data.examples_content, data.test_content
            )
        } else {
            data.test_content.clone()
        };

        // Build source context for Agent 1 (source + examples or tests)
        let source_with_context = if !data.examples_content.is_empty() {
            // Prefer examples over tests
            format!(
                "# Examples (High-level API)\n{}\n\n# Source Code\n{}",
                data.examples_content, data.source_content
            )
        } else if !data.test_content.is_empty() {
            // Fall back to tests if no examples (they show API usage)
            format!(
                "# Test Code (API usage patterns)\n{}\n\n# Source Code\n{}",
                data.test_content, data.source_content
            )
        } else {
            data.source_content.clone()
        };

        // Agents 1-3 are independent — run them in parallel
        info!("Agent 1: Extracting API surface...");
        info!("Agent 2: Extracting usage patterns...");
        info!("Agent 3: Extracting conventions and pitfalls...");

        let agent1_prompt = prompts_v2::agent1_api_extractor_v2(
            &data.package_name,
            &data.version,
            &source_with_context,
            data.source_file_count,
            self.prompts_config.agent1_custom.as_deref(),
            self.prompts_config.is_overwrite(1),
        );
        let agent2_prompt = prompts_v2::agent2_pattern_extractor_v2(
            &data.package_name,
            &data.version,
            &examples_and_tests,
            self.prompts_config.agent2_custom.as_deref(),
            self.prompts_config.is_overwrite(2),
        );
        let agent3_prompt = prompts_v2::agent3_context_extractor_v2(
            &data.package_name,
            &data.version,
            &docs_and_changelog,
            self.prompts_config.agent3_custom.as_deref(),
            self.prompts_config.is_overwrite(3),
        );

        let client1 = self.get_client(1);
        let client2 = self.get_client(2);
        let client3 = self.get_client(3);

        let (api_surface, patterns, context) = if self.parallel_extraction {
            info!("Running agents 1-3 in parallel...");
            tokio::try_join!(
                client1.complete(&agent1_prompt),
                client2.complete(&agent2_prompt),
                client3.complete(&agent3_prompt),
            )?
        } else {
            info!("Running agents 1-3 sequentially...");
            let api_surface = client1.complete(&agent1_prompt).await?;
            info!("Agent 1 complete");
            let patterns = client2.complete(&agent2_prompt).await?;
            info!("Agent 2 complete");
            let context = client3.complete(&agent3_prompt).await?;
            info!("Agent 3 complete");
            (api_surface, patterns, context)
        };

        info!("Agents 1-3: All extractions complete");

        // Agent 4: Synthesize SKILL.md
        let mut skill_md = if let Some(ref existing) = self.existing_skill {
            // Update mode: patch existing SKILL.md
            info!("Agent 4: Updating existing SKILL.md...");
            let update_prompt = prompts_v2::agent4_update_v2(
                &data.package_name,
                &data.version,
                existing,
                &api_surface,
                &patterns,
                &context,
            );
            self.get_client(4).complete(&update_prompt).await?
        } else {
            // Normal mode: synthesize from scratch
            info!("Agent 4: Synthesizing SKILL.md...");
            let synthesis_prompt = prompts_v2::agent4_synthesizer_v2(
                &data.package_name,
                &data.version,
                data.license.as_deref(),
                &data.project_urls,
                data.language.as_str(),
                &api_surface,
                &patterns,
                &context,
                self.prompts_config.agent4_custom.as_deref(),
                self.prompts_config.is_overwrite(4),
            );
            self.get_client(4).complete(&synthesis_prompt).await?
        };

        // Strip markdown code fences if present (models sometimes wrap output)
        skill_md = strip_markdown_fences(&skill_md);

        // Dual validation loop: Format + Functional
        let linter = SkillLinter::new();
        let functional_validator = FunctionalValidator::new();

        // Always run at least one validation pass. max_retries=0 means
        // "one pass, no retries on failure" (not "skip all validation").
        let validation_passes = self.max_retries.max(1);

        for attempt in 0..validation_passes {
            info!("Validation pass {} of {}", attempt + 1, validation_passes);

            // 1. Format Validation (Linter) - Fast
            info!("  → Running format validation (linter)...");
            let lint_issues = linter.lint(&skill_md)?;
            let has_errors = lint_issues
                .iter()
                .any(|i| matches!(i.severity, Severity::Error));

            if has_errors {
                let error_msgs: Vec<String> = lint_issues
                    .iter()
                    .filter(|i| matches!(i.severity, crate::lint::Severity::Error))
                    .map(|i| format!("- [{}] {}", i.category, i.message))
                    .collect();
                warn!("  ✗ Format validation failed: {} errors", error_msgs.len());

                // Security errors bail IMMEDIATELY — never sent back to the model.
                // Sending security violations to the model for "fixing" lets it
                // learn bypass strategies. Fail fast, fail hard.
                let has_security_errors = lint_issues.iter().any(|i| i.category == "security");
                if has_security_errors {
                    let security_msgs: Vec<String> = lint_issues
                        .iter()
                        .filter(|i| i.category == "security")
                        .map(|i| i.message.clone())
                        .collect();
                    anyhow::bail!(
                        "SECURITY: Generated SKILL.md contains dangerous content that cannot be shipped:\n{}",
                        security_msgs.join("\n")
                    );
                }

                if attempt == validation_passes - 1 {
                    info!("Max retries reached, returning best attempt despite format issues");
                    break;
                }

                // Patch with format fix instructions (non-security errors only)
                let fix_prompt = format!(
                    "Here is the current SKILL.md:\n\n{}\n\nFORMAT VALIDATION FAILED:\n{}\n\nPlease fix these format issues. Keep all content intact.",
                    skill_md,
                    error_msgs.join("\n")
                );

                skill_md = self.get_client(4).complete(&fix_prompt).await?;
                skill_md = strip_markdown_fences(&skill_md);
                continue;
            }

            info!("  ✓ Format validation passed");

            // 2. Functional Validation - Runs code
            // Skip old functional validation if Agent 5 is enabled for Python
            // (Agent 5 is more comprehensive - installs deps with uv and runs actual tests)
            let skip_functional = self.enable_agent5 && data.language.as_str() == "python";

            if skip_functional {
                info!("  ⏭️  Skipping functional validation (Agent 5 enabled for Python)");
            } else {
                info!("  → Running functional validation (code execution)...");
            }

            let functional_result = if skip_functional {
                // Skip to Agent 5 directly
                ValidationResult::Skipped(
                    "Agent 5 enabled - using code generation validation instead".to_string(),
                )
            } else {
                functional_validator.validate(&skill_md, data.language.as_str())?
            };

            match functional_result {
                ValidationResult::Pass(output) => {
                    info!("  ✓ Functional validation passed");
                    info!("    Output: {}", output.lines().next().unwrap_or(""));
                    break; // Format and functional passed, skip Agent 5 if not Python
                }
                ValidationResult::Skipped(reason) => {
                    info!("  ⏭️  Functional validation skipped: {}", reason);

                    // Agent 5: Code generation validation (if enabled for Python)
                    if self.enable_agent5 && data.language.as_str() == "python" {
                        info!("Agent 5: Testing SKILL.md with code generation...");

                        let agent5_llm = self.get_client(5);

                        let agent5 = Agent5CodeValidator::new_python_with_custom(
                            agent5_llm,
                            self.container_config.clone(),
                            self.prompts_config.agent5_custom.clone(),
                        )
                        .with_mode(self.agent5_mode);

                        let validation_result: Result<TestResult, anyhow::Error> =
                            agent5.validate(&skill_md).await;
                        match validation_result {
                            Ok(test_result) => {
                                if test_result.test_cases.is_empty() {
                                    // No patterns found to test — nothing to validate, not a failure
                                    info!("  ⏭️  Agent 5: No testable patterns found, skipping");
                                    break;
                                }
                                if test_result.all_passed() {
                                    info!("  ✓ Agent 5: All {} tests passed", test_result.passed);
                                    break; // All validations passed!
                                } else {
                                    warn!(
                                        "  ✗ Agent 5: {} passed, {} failed",
                                        test_result.passed, test_result.failed
                                    );

                                    // Patch with targeted feedback if we have retries left
                                    if attempt < validation_passes - 1 {
                                        if let Some(feedback) = test_result.generate_feedback() {
                                            let patch_prompt = format!(
                                                "Here is the current SKILL.md:\n\n{}\n\n{}",
                                                skill_md, feedback
                                            );

                                            skill_md =
                                                self.get_client(4).complete(&patch_prompt).await?;
                                            skill_md = strip_markdown_fences(&skill_md);
                                            continue; // Retry with patched content
                                        }
                                    } else {
                                        warn!("  Max retries reached, proceeding despite Agent 5 failures");
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("  ✗ Agent 5 error: {}", e);
                                warn!("    Continuing without Agent 5 validation");
                                break; // Don't fail the whole pipeline for Agent 5 errors
                            }
                        }
                    } else {
                        // No Agent 5, format passed, functional skipped - good enough
                        break;
                    }
                }
                ValidationResult::Fail(error) => {
                    warn!("  ✗ Functional validation failed");
                    warn!("    Error: {}", error.lines().next().unwrap_or(""));

                    if attempt == validation_passes - 1 {
                        // Final safety check: re-lint to catch any security issues
                        // introduced during fix attempts
                        let final_lint = linter.lint(&skill_md)?;
                        let has_security_errors =
                            final_lint.iter().any(|i| i.category == "security");
                        if has_security_errors {
                            let security_msgs: Vec<String> = final_lint
                                .iter()
                                .filter(|i| i.category == "security")
                                .map(|i| i.message.clone())
                                .collect();
                            anyhow::bail!(
                                "SECURITY: Generated SKILL.md contains dangerous content that cannot be shipped:\n{}",
                                security_msgs.join("\n")
                            );
                        }
                        info!("Max retries reached, returning best attempt despite code issues");
                        break;
                    }

                    // Patch with code execution error
                    let fix_prompt = format!(
                        "Here is the current SKILL.md:\n\n{}\n\nCODE EXECUTION FAILED:\n{}\n\nFix the code examples that don't work. Keep all other content intact.",
                        skill_md,
                        error
                    );

                    skill_md = self.get_client(4).complete(&fix_prompt).await?;
                    skill_md = strip_markdown_fences(&skill_md);
                    continue;
                }
            }
        }

        // Normalize output (ensure frontmatter + References)
        skill_md = normalizer::normalize_skill_md(
            &skill_md,
            &data.package_name,
            &data.version,
            data.language.as_str(),
            data.license.as_deref(),
            &data.project_urls,
            self.model_name.as_deref(),
        );

        // Post-normalization lint check — catch any issues introduced by normalization
        let post_issues = linter.lint(&skill_md)?;
        let post_errors: Vec<_> = post_issues
            .iter()
            .filter(|i| matches!(i.severity, Severity::Error))
            .collect();
        if !post_errors.is_empty() {
            warn!(
                "Post-normalization lint found {} errors (returning anyway):",
                post_errors.len()
            );
            for issue in &post_errors {
                warn!("  - [{}] {}", issue.category, issue.message);
            }
        }

        Ok(skill_md)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::Language;
    use crate::llm::client::MockLlmClient;
    use crate::pipeline::collector::CollectedData;

    #[test]
    fn test_strip_markdown_fences() {
        let input = "```markdown\n# Hello\nworld\n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "# Hello\nworld");
    }

    #[test]
    fn test_strip_markdown_fences_plain() {
        let input = "```\n# Hello\nworld\n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "# Hello\nworld");
    }

    #[test]
    fn test_strip_markdown_fences_no_fences() {
        let input = "# Hello\nworld";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "# Hello\nworld");
    }

    #[test]
    fn test_generator_new() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3);

        assert_eq!(gen.max_retries, 3);
        assert!(gen.enable_agent5);
        assert!(matches!(gen.agent5_mode, ValidationMode::Thorough));
        assert!(gen.existing_skill.is_none());
        assert!(gen.model_name.is_none());
        assert!(gen.agent1_client.is_none());
        assert!(gen.agent2_client.is_none());
        assert!(gen.agent3_client.is_none());
        assert!(gen.agent4_client.is_none());
        assert!(gen.agent5_client.is_none());
    }

    #[test]
    fn test_generator_builder_methods() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_agent5(false)
            .with_agent5_mode(ValidationMode::Minimal)
            .with_existing_skill("existing content".to_string())
            .with_model_name("test-model".to_string())
            .with_agent5_client(Box::new(MockLlmClient::new()));

        assert!(!gen.enable_agent5);
        assert!(matches!(gen.agent5_mode, ValidationMode::Minimal));
        assert_eq!(gen.existing_skill.as_deref(), Some("existing content"));
        assert_eq!(gen.model_name.as_deref(), Some("test-model"));
        assert!(gen.agent5_client.is_some());
    }

    fn make_test_data() -> CollectedData {
        CollectedData {
            package_name: "testpkg".to_string(),
            version: "1.0.0".to_string(),
            license: Some("MIT".to_string()),
            project_urls: vec![],
            language: Language::Python,
            source_file_count: 5,
            examples_content: String::new(),
            test_content: "def test_foo(): pass".to_string(),
            docs_content: "# Docs".to_string(),
            source_content: "class Foo: pass".to_string(),
            changelog_content: String::new(),
        }
    }

    #[tokio::test]
    async fn test_generate_produces_skill_md() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1).with_agent5(false);

        let data = make_test_data();
        let result = gen.generate(&data).await.unwrap();

        // Mock Agent 4 produces frontmatter with name/version/ecosystem, normalizer preserves it
        assert!(
            result.contains("---"),
            "should contain frontmatter delimiters"
        );
        assert!(
            result.contains("ecosystem: python"),
            "should contain ecosystem in frontmatter"
        );

        // The mock Agent 4 output contains these sections
        assert!(
            result.contains("## Imports"),
            "should contain Imports section"
        );
        assert!(
            result.contains("## Core Patterns"),
            "should contain Core Patterns section"
        );
        assert!(
            result.contains("## Pitfalls"),
            "should contain Pitfalls section"
        );
    }

    #[tokio::test]
    async fn test_generate_non_python_skips_functional_validation() {
        // Non-Python language: functional validation is skipped, Agent 5 skipped
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1).with_agent5(false);

        let mut data = make_test_data();
        data.language = Language::JavaScript;

        let result = gen.generate(&data).await.unwrap();
        assert!(result.contains("---"), "should contain frontmatter");
        // Pipeline completes without errors for non-Python languages
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_generate_with_existing_skill_uses_update_mode() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_agent5(false)
            .with_existing_skill("# Old SKILL.md".to_string());

        let data = make_test_data();
        let result = gen.generate(&data).await.unwrap();

        // Should still produce valid output (mock returns same Agent 4 response)
        assert!(result.contains("---"));
    }

    #[tokio::test]
    async fn test_generate_with_model_name() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_agent5(false)
            .with_model_name("gpt-5.2".to_string());

        let data = make_test_data();
        let result = gen.generate(&data).await.unwrap();

        // Normalizer should inject generated_with into frontmatter
        assert!(result.contains("generated_with:"));
    }

    #[tokio::test]
    async fn test_generate_with_examples_content() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1).with_agent5(false);

        let mut data = make_test_data();
        data.examples_content = "# Example\nimport testpkg\ntestpkg.run()".to_string();

        let result = gen.generate(&data).await.unwrap();
        assert!(
            result.contains("---"),
            "should produce valid output with examples"
        );
    }

    #[tokio::test]
    async fn test_generate_max_retries_zero_still_validates() {
        // max_retries=0 should still run one validation pass (not skip all validation)
        let gen = Generator::new(Box::new(MockLlmClient::new()), 0).with_agent5(false);

        let data = make_test_data();
        let result = gen.generate(&data).await.unwrap();

        // Output should still have frontmatter (normalization + lint ran)
        assert!(
            result.contains("---"),
            "max_retries=0 should still produce valid output"
        );
        assert!(
            result.contains("ecosystem:"),
            "should have ecosystem in frontmatter"
        );
    }

    #[test]
    fn test_get_client_returns_main_by_default() {
        let client = Box::new(MockLlmClient::new());
        let gen = Generator::new(client, 5);
        // All agents should return the main client when no per-agent override
        // We can't directly compare references, but we can verify the method doesn't panic
        let _ = gen.get_client(1);
        let _ = gen.get_client(2);
        let _ = gen.get_client(3);
        let _ = gen.get_client(4);
        let _ = gen.get_client(5);
    }

    #[test]
    fn test_per_agent_client_builders() {
        let client = Box::new(MockLlmClient::new());
        let gen = Generator::new(client, 5)
            .with_agent1_client(Box::new(MockLlmClient::new()))
            .with_agent5_client(Box::new(MockLlmClient::new()));
        assert!(gen.agent1_client.is_some());
        assert!(gen.agent2_client.is_none());
        assert!(gen.agent3_client.is_none());
        assert!(gen.agent4_client.is_none());
        assert!(gen.agent5_client.is_some());
    }
}
