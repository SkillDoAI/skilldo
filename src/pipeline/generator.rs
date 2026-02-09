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
    agent5_client: Option<Box<dyn LlmClient>>, // Optional separate client for Agent 5
    max_retries: usize,
    prompts_config: PromptsConfig,
    enable_agent5: bool,
    agent5_mode: ValidationMode,
    container_config: ContainerConfig,
    existing_skill: Option<String>, // Existing SKILL.md for update mode
    model_name: Option<String>,     // For generated_with frontmatter field
}

impl Generator {
    pub fn new(client: Box<dyn LlmClient>, max_retries: usize) -> Self {
        Self {
            client,
            agent5_client: None, // Use main client by default
            max_retries,
            prompts_config: PromptsConfig::default(),
            enable_agent5: true,                   // Default to enabled
            agent5_mode: ValidationMode::Thorough, // Default to thorough mode
            container_config: ContainerConfig::default(),
            existing_skill: None,
            model_name: None,
        }
    }

    pub fn with_agent5_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.agent5_client = Some(client);
        self
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

        // Agent 1: Extract API surface (use source + examples + tests for comprehensive coverage)
        info!("Agent 1: Extracting API surface...");
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
        let api_surface = self
            .client
            .complete(&prompts_v2::agent1_api_extractor_v2(
                &data.package_name,
                &data.version,
                &source_with_context,
                data.source_file_count,
                self.prompts_config.agent1_custom.as_deref(),
                self.prompts_config.is_overwrite(1),
            ))
            .await?;

        // Agent 2: Extract patterns from examples + tests (examples are better than tests)
        info!("Agent 2: Extracting usage patterns...");
        let patterns = self
            .client
            .complete(&prompts_v2::agent2_pattern_extractor_v2(
                &data.package_name,
                &data.version,
                &examples_and_tests,
                self.prompts_config.agent2_custom.as_deref(),
                self.prompts_config.is_overwrite(2),
            ))
            .await?;

        // Agent 3: Extract context from docs
        info!("Agent 3: Extracting conventions and pitfalls...");
        let context = self
            .client
            .complete(&prompts_v2::agent3_context_extractor_v2(
                &data.package_name,
                &data.version,
                &docs_and_changelog,
                self.prompts_config.agent3_custom.as_deref(),
                self.prompts_config.is_overwrite(3),
            ))
            .await?;

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
            self.client.complete(&update_prompt).await?
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
            self.client.complete(&synthesis_prompt).await?
        };

        // Strip markdown code fences if present (models sometimes wrap output)
        skill_md = strip_markdown_fences(&skill_md);

        // Dual validation loop: Format + Functional
        let linter = SkillLinter::new();
        let functional_validator = FunctionalValidator::new();

        for attempt in 0..self.max_retries {
            info!("Validation pass {} of {}", attempt + 1, self.max_retries);

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

                if attempt == self.max_retries - 1 {
                    info!("Max retries reached, returning best attempt despite format issues");
                    break;
                }

                // Patch with format fix instructions
                let fix_prompt = format!(
                    "Here is the current SKILL.md:\n\n{}\n\nFORMAT VALIDATION FAILED:\n{}\n\nPlease fix these format issues. Keep all content intact.",
                    skill_md,
                    error_msgs.join("\n")
                );

                skill_md = self.client.complete(&fix_prompt).await?;
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

                        // Use agent5_client if configured, otherwise use main client
                        let agent5_llm = self
                            .agent5_client
                            .as_ref()
                            .map(|c| c.as_ref())
                            .unwrap_or(self.client.as_ref());

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
                                if test_result.all_passed() {
                                    info!("  ✓ Agent 5: All {} tests passed", test_result.passed);
                                    break; // All validations passed!
                                } else {
                                    warn!(
                                        "  ✗ Agent 5: {} passed, {} failed",
                                        test_result.passed, test_result.failed
                                    );

                                    // Patch with targeted feedback if we have retries left
                                    if attempt < self.max_retries - 1 {
                                        if let Some(feedback) = test_result.generate_feedback() {
                                            let patch_prompt = format!(
                                                "Here is the current SKILL.md:\n\n{}\n\n{}",
                                                skill_md, feedback
                                            );

                                            skill_md = self.client.complete(&patch_prompt).await?;
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

                    if attempt == self.max_retries - 1 {
                        info!("Max retries reached, returning best attempt despite code issues");
                        break;
                    }

                    // Patch with code execution error
                    let fix_prompt = format!(
                        "Here is the current SKILL.md:\n\n{}\n\nCODE EXECUTION FAILED:\n{}\n\nFix the code examples that don't work. Keep all other content intact.",
                        skill_md,
                        error
                    );

                    skill_md = self.client.complete(&fix_prompt).await?;
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

        Ok(skill_md)
    }
}
