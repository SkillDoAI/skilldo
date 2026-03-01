use anyhow::Result;
use tracing::{info, warn};

use super::collector::CollectedData;
use super::normalizer;
use crate::config::{ContainerConfig, PromptsConfig};
use crate::lint::{Severity, SkillLinter};
use crate::llm::client::LlmClient;
use crate::llm::prompts_v2;
use crate::review::{ReviewAgent, ReviewIssue};
use crate::test_agent::{TestCodeValidator, TestResult, ValidationMode};
use crate::validator::{FunctionalValidator, ValidationResult};

/// Output from the generation pipeline.
#[derive(Debug)]
pub struct GenerateOutput {
    /// The generated SKILL.md content.
    pub skill_md: String,
    /// Unresolved review issues that the pipeline could not fix.
    /// Empty if review passed or was disabled.
    pub unresolved_warnings: Vec<ReviewIssue>,
}

/// Strip markdown code fences from output (```markdown ... ``` or ```...```)
fn strip_markdown_fences(content: &str) -> String {
    let trimmed = content.trim();

    // Count leading backticks
    let leading = trimmed.chars().take_while(|c| *c == '`').count();
    if leading < 3 {
        return content.to_string();
    }

    // Count trailing backticks
    let trailing = trimmed.chars().rev().take_while(|c| *c == '`').count();
    if trailing < 3 {
        return content.to_string();
    }

    // Find end of first line (opening fence + optional language tag)
    let rest_after_backticks = &trimmed[leading..];
    let first_newline = match rest_after_backticks.find('\n') {
        Some(pos) => leading + pos,
        None => return content.to_string(),
    };

    // Extract body between fences
    let body = &trimmed[first_newline + 1..trimmed.len() - trailing];
    body.trim().to_string()
}

pub struct Generator {
    client: Box<dyn LlmClient>,
    extract_client: Option<Box<dyn LlmClient>>,
    map_client: Option<Box<dyn LlmClient>>,
    learn_client: Option<Box<dyn LlmClient>>,
    create_client: Option<Box<dyn LlmClient>>,
    review_client: Option<Box<dyn LlmClient>>,
    test_client: Option<Box<dyn LlmClient>>,
    max_retries: usize,
    prompts_config: PromptsConfig,
    enable_test: bool,
    test_mode: ValidationMode,
    enable_review: bool,
    review_max_retries: usize,
    container_config: ContainerConfig,
    parallel_extraction: bool,      // Run extract/map/learn in parallel
    existing_skill: Option<String>, // Existing SKILL.md for update mode
    model_name: Option<String>,     // For generated_with frontmatter field
}

impl Generator {
    pub fn new(client: Box<dyn LlmClient>, max_retries: usize) -> Self {
        Self {
            client,
            extract_client: None,
            map_client: None,
            learn_client: None,
            create_client: None,
            review_client: None,
            test_client: None,
            max_retries,
            prompts_config: PromptsConfig::default(),
            enable_test: true,                   // Default to enabled
            test_mode: ValidationMode::Thorough, // Default to thorough mode
            enable_review: true,                 // Default to enabled
            review_max_retries: 5,               // Default to 5 retries
            container_config: ContainerConfig::default(),
            parallel_extraction: true,
            existing_skill: None,
            model_name: None,
        }
    }

    pub fn with_extract_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.extract_client = Some(client);
        self
    }

    pub fn with_map_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.map_client = Some(client);
        self
    }

    pub fn with_learn_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.learn_client = Some(client);
        self
    }

    pub fn with_create_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.create_client = Some(client);
        self
    }

    pub fn with_review_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.review_client = Some(client);
        self
    }

    pub fn with_test_client(mut self, client: Box<dyn LlmClient>) -> Self {
        self.test_client = Some(client);
        self
    }

    /// Get the LLM client for a specific pipeline stage.
    /// Returns the per-stage client if configured, otherwise the main client.
    fn get_client(&self, stage: &str) -> &dyn LlmClient {
        match stage {
            "extract" => self.extract_client.as_deref(),
            "map" => self.map_client.as_deref(),
            "learn" => self.learn_client.as_deref(),
            "create" => self.create_client.as_deref(),
            "review" => self.review_client.as_deref(),
            "test" => self.test_client.as_deref(),
            _ => None,
        }
        .unwrap_or(self.client.as_ref())
    }

    pub fn with_prompts_config(mut self, config: PromptsConfig) -> Self {
        self.prompts_config = config;
        self
    }

    pub fn with_test(mut self, enabled: bool) -> Self {
        self.enable_test = enabled;
        self
    }

    pub fn with_test_mode(mut self, mode: ValidationMode) -> Self {
        self.test_mode = mode;
        self
    }

    pub fn with_review(mut self, enabled: bool) -> Self {
        self.enable_review = enabled;
        self
    }

    pub fn with_review_max_retries(mut self, max: usize) -> Self {
        self.review_max_retries = max;
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

    pub async fn generate(&self, data: &CollectedData) -> Result<GenerateOutput> {
        info!("Starting pipeline for {}", data.package_name);

        // Combine docs and changelog for learn stage
        let docs_and_changelog = format!("{}\n\n{}", data.docs_content, data.changelog_content);

        // Combine examples and tests for map stage (examples first - they're cleaner)
        let examples_and_tests = if !data.examples_content.is_empty() {
            format!(
                "# Example Files (Real Usage)\n{}\n\n# Test Files (API Usage)\n{}",
                data.examples_content, data.test_content
            )
        } else {
            data.test_content.clone()
        };

        // Build source context for extract stage (source + examples or tests)
        let source_with_context = if !data.examples_content.is_empty() {
            format!(
                "# Examples (High-level API)\n{}\n\n# Source Code\n{}",
                data.examples_content, data.source_content
            )
        } else if !data.test_content.is_empty() {
            format!(
                "# Test Code (API usage patterns)\n{}\n\n# Source Code\n{}",
                data.test_content, data.source_content
            )
        } else {
            data.source_content.clone()
        };

        // extract/map/learn are independent — run them in parallel
        info!("extract: Extracting API surface...");
        info!("map: Extracting usage patterns...");
        info!("learn: Extracting conventions and pitfalls...");

        let extract_prompt = prompts_v2::extract_prompt(
            &data.package_name,
            &data.version,
            &source_with_context,
            data.source_file_count,
            self.prompts_config.extract_custom.as_deref(),
            self.prompts_config.is_overwrite("extract"),
            &data.language,
        );
        let map_prompt = prompts_v2::map_prompt(
            &data.package_name,
            &data.version,
            &examples_and_tests,
            self.prompts_config.map_custom.as_deref(),
            self.prompts_config.is_overwrite("map"),
            &data.language,
        );
        let learn_prompt = prompts_v2::learn_prompt(
            &data.package_name,
            &data.version,
            &docs_and_changelog,
            self.prompts_config.learn_custom.as_deref(),
            self.prompts_config.is_overwrite("learn"),
            &data.language,
        );

        let extract_client = self.get_client("extract");
        let map_client = self.get_client("map");
        let learn_client = self.get_client("learn");

        let (api_surface, patterns, context) = if self.parallel_extraction {
            info!("Running extract/map/learn in parallel...");
            tokio::try_join!(
                extract_client.complete(&extract_prompt),
                map_client.complete(&map_prompt),
                learn_client.complete(&learn_prompt),
            )?
        } else {
            info!("Running extract/map/learn sequentially...");
            let api_surface = extract_client.complete(&extract_prompt).await?;
            info!("extract: complete");
            let patterns = map_client.complete(&map_prompt).await?;
            info!("map: complete");
            let context = learn_client.complete(&learn_prompt).await?;
            info!("learn: complete");
            (api_surface, patterns, context)
        };

        info!("extract/map/learn: All extractions complete");

        // create: Synthesize SKILL.md
        let mut skill_md = if let Some(ref existing) = self.existing_skill {
            // Update mode: patch existing SKILL.md
            info!("create: Updating existing SKILL.md...");
            let update_prompt = prompts_v2::create_update_prompt(
                &data.package_name,
                &data.version,
                existing,
                &api_surface,
                &patterns,
                &context,
                &data.language,
            );
            self.get_client("create").complete(&update_prompt).await?
        } else {
            // Normal mode: synthesize from scratch
            info!("create: Synthesizing SKILL.md...");
            let synthesis_prompt = prompts_v2::create_prompt(
                &data.package_name,
                &data.version,
                data.license.as_deref(),
                &data.project_urls,
                &data.language,
                &api_surface,
                &patterns,
                &context,
                self.prompts_config.create_custom.as_deref(),
                self.prompts_config.is_overwrite("create"),
            );
            self.get_client("create")
                .complete(&synthesis_prompt)
                .await?
        };

        // Strip markdown code fences if present (models sometimes wrap output)
        skill_md = strip_markdown_fences(&skill_md);

        // Dual validation loop: Format + Functional
        let linter = SkillLinter::new();
        // Lazy-init: only construct FunctionalValidator when actually needed
        // (avoids spurious "Docker not available" warning when test agent handles validation)
        let mut functional_validator: Option<FunctionalValidator> = None;

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

                skill_md = self.get_client("create").complete(&fix_prompt).await?;
                skill_md = strip_markdown_fences(&skill_md);
                continue;
            }

            info!("  ✓ Format validation passed");

            // 2. Functional Validation - Runs code
            // The legacy functional validator runs code in a bare container without
            // dependency installation — it only works for stdlib-only snippets.
            // When test agent is enabled, it handles validation properly (uv + deps).
            // When test agent is disabled, skip functional validation entirely since
            // the legacy validator would just fail on any import.
            let skip_reason = if !self.enable_test {
                Some("test agent disabled — legacy validator cannot install dependencies")
            } else if data.language.as_str() == "python" {
                Some("test agent enabled — using code generation validation instead")
            } else {
                None
            };

            if let Some(reason) = skip_reason {
                info!("  ⏭️  Skipping functional validation ({reason})");
            } else {
                info!("  → Running functional validation (code execution)...");
            }

            let functional_result = if let Some(reason) = skip_reason {
                ValidationResult::Skipped(reason.to_string())
            } else {
                let validator = functional_validator.get_or_insert_with(FunctionalValidator::new);
                validator.validate(&skill_md, &data.language)?
            };

            match functional_result {
                ValidationResult::Pass(output) => {
                    info!("  ✓ Functional validation passed");
                    info!("    Output: {}", output.lines().next().unwrap_or(""));
                    break; // Format and functional passed, skip test agent if not Python
                }
                ValidationResult::Skipped(reason) => {
                    info!("  ⏭️  Functional validation skipped: {}", reason);

                    // test: Code generation validation (if enabled for Python)
                    if self.enable_test && data.language.as_str() == "python" {
                        info!("test: Testing SKILL.md with code generation...");

                        let test_llm = self.get_client("test");

                        let test_validator = TestCodeValidator::new_python_with_custom(
                            test_llm,
                            self.container_config.clone(),
                            self.prompts_config.test_custom.clone(),
                        )
                        .with_mode(self.test_mode);

                        let validation_result: Result<TestResult, anyhow::Error> =
                            test_validator.validate(&skill_md).await;
                        match validation_result {
                            Ok(test_result) => {
                                if test_result.test_cases.is_empty() {
                                    // No patterns found to test — nothing to validate, not a failure
                                    info!("  ⏭️  test: No testable patterns found, skipping");
                                    break;
                                }
                                if test_result.all_passed() {
                                    info!("  ✓ test: All {} tests passed", test_result.passed);
                                    break; // All validations passed!
                                } else {
                                    warn!(
                                        "  ✗ test: {} passed, {} failed",
                                        test_result.passed, test_result.failed
                                    );

                                    // Patch with targeted feedback if we have retries left
                                    if attempt < validation_passes - 1 {
                                        if let Some(feedback) = test_result.generate_feedback() {
                                            let patch_prompt = format!(
                                                "Here is the current SKILL.md:\n\n{}\n\n{}",
                                                skill_md, feedback
                                            );

                                            skill_md = self
                                                .get_client("create")
                                                .complete(&patch_prompt)
                                                .await?;
                                            skill_md = strip_markdown_fences(&skill_md);
                                            continue; // Retry with patched content
                                        }
                                    } else {
                                        warn!("  Max retries reached, proceeding despite test failures");
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("  ✗ test error: {}", e);
                                warn!("    Continuing without test validation");
                                break; // Don't fail the whole pipeline for test agent errors
                            }
                        }
                    } else {
                        // No test agent, format passed, functional skipped - good enough
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

                    skill_md = self.get_client("create").complete(&fix_prompt).await?;
                    skill_md = strip_markdown_fences(&skill_md);
                    continue;
                }
            }
        }

        // Review: accuracy + safety validation
        let mut unresolved_warnings: Vec<ReviewIssue> = Vec::new();
        if self.enable_review {
            let review_agent = ReviewAgent::new(
                self.get_client("review"),
                self.container_config.clone(),
                self.prompts_config.review_custom.clone(),
            );

            for review_attempt in 0..=self.review_max_retries {
                info!(
                    "review: Checking accuracy and safety (attempt {}/{})",
                    review_attempt + 1,
                    self.review_max_retries + 1
                );

                let result = review_agent
                    .review(&skill_md, &data.package_name, data.language.as_str())
                    .await?;

                if result.passed {
                    info!("  ✓ review: passed");
                    break;
                }

                if review_attempt == self.review_max_retries {
                    warn!("  review: max retries reached, proceeding with issues");
                    for issue in &result.issues {
                        warn!(
                            "  - [{}][{}] {}",
                            issue.severity, issue.category, issue.complaint
                        );
                    }
                    unresolved_warnings = result.issues;
                    break;
                }

                // Send complaints to create for fixing
                warn!(
                    "  ✗ review: {} issues found, sending to create for fix",
                    result.issues.len()
                );
                let feedback = ReviewAgent::format_feedback(&result);
                let fix_prompt = format!(
                    "Here is the current SKILL.md:\n\n{}\n\n{}",
                    skill_md, feedback
                );
                skill_md = self.get_client("create").complete(&fix_prompt).await?;
                skill_md = strip_markdown_fences(&skill_md);

                // Quick lint check before re-review
                let lint_issues = linter.lint(&skill_md)?;
                let has_security = lint_issues
                    .iter()
                    .any(|i| i.category == "security" && matches!(i.severity, Severity::Error));
                if has_security {
                    let msgs: Vec<String> = lint_issues
                        .iter()
                        .filter(|i| i.category == "security")
                        .map(|i| i.message.clone())
                        .collect();
                    anyhow::bail!("SECURITY: {}", msgs.join("\n"));
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

        // Security errors are always fatal, even post-normalization
        let post_security: Vec<_> = post_issues
            .iter()
            .filter(|i| i.category == "security" && matches!(i.severity, Severity::Error))
            .collect();
        if !post_security.is_empty() {
            let security_msgs: Vec<String> =
                post_security.iter().map(|i| i.message.clone()).collect();
            anyhow::bail!(
                "SECURITY: Post-normalization output contains dangerous content:\n{}",
                security_msgs.join("\n")
            );
        }

        let post_errors: Vec<_> = post_issues
            .iter()
            .filter(|i| matches!(i.severity, Severity::Error) && i.category != "security")
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

        Ok(GenerateOutput {
            skill_md,
            unresolved_warnings,
        })
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
    fn test_strip_markdown_fences_three_backticks() {
        let input = "```markdown\n---\nname: test\n---\n## Imports\n```";
        let result = strip_markdown_fences(input);
        assert!(result.starts_with("---"));
        assert!(!result.contains("```"));
    }

    #[test]
    fn test_strip_markdown_fences_four_backticks() {
        let input = "````markdown\n---\nname: test\n---\n## Imports\n````";
        let result = strip_markdown_fences(input);
        assert!(result.starts_with("---"));
        assert!(!result.contains("````"));
    }

    #[test]
    fn test_strip_markdown_fences_five_backticks() {
        let input = "`````\n---\nname: test\n---\n## Imports\n`````";
        let result = strip_markdown_fences(input);
        assert!(result.starts_with("---"));
    }

    #[test]
    fn test_strip_markdown_fences_no_fences_frontmatter() {
        let input = "---\nname: test\n---\n## Imports";
        let result = strip_markdown_fences(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_strip_markdown_fences_plain_backticks() {
        let input = "```\n---\nname: test\n---\n## Imports\n```";
        let result = strip_markdown_fences(input);
        assert!(result.starts_with("---"));
    }

    #[test]
    fn test_generator_new() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3);

        assert_eq!(gen.max_retries, 3);
        assert!(gen.enable_test);
        assert!(matches!(gen.test_mode, ValidationMode::Thorough));
        assert!(gen.existing_skill.is_none());
        assert!(gen.model_name.is_none());
        assert!(gen.extract_client.is_none());
        assert!(gen.map_client.is_none());
        assert!(gen.learn_client.is_none());
        assert!(gen.create_client.is_none());
        assert!(gen.test_client.is_none());
    }

    #[test]
    fn test_generator_builder_methods() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_test(false)
            .with_test_mode(ValidationMode::Minimal)
            .with_existing_skill("existing content".to_string())
            .with_model_name("test-model".to_string())
            .with_test_client(Box::new(MockLlmClient::new()));

        assert!(!gen.enable_test);
        assert!(matches!(gen.test_mode, ValidationMode::Minimal));
        assert_eq!(gen.existing_skill.as_deref(), Some("existing content"));
        assert_eq!(gen.model_name.as_deref(), Some("test-model"));
        assert!(gen.test_client.is_some());
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
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1).with_test(false);

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();
        let result = &output.skill_md;

        // Mock create agent produces frontmatter with name/version/ecosystem, normalizer preserves it
        assert!(
            result.contains("---"),
            "should contain frontmatter delimiters"
        );
        assert!(
            result.contains("ecosystem: python"),
            "should contain ecosystem in frontmatter"
        );

        // The mock create agent output contains these sections
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
        // Non-Python language: functional validation is skipped, test agent skipped
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1).with_test(false);

        let mut data = make_test_data();
        data.language = Language::JavaScript;

        let output = gen.generate(&data).await.unwrap();
        assert!(
            output.skill_md.contains("---"),
            "should contain frontmatter"
        );
        // Pipeline completes without errors for non-Python languages
        assert!(!output.skill_md.is_empty());
    }

    #[tokio::test]
    async fn test_generate_with_existing_skill_uses_update_mode() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_existing_skill("# Old SKILL.md".to_string());

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        // Should still produce valid output (mock returns same create agent response)
        assert!(output.skill_md.contains("---"));
    }

    #[tokio::test]
    async fn test_generate_with_model_name() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_model_name("gpt-5.2".to_string());

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        // Normalizer should inject generated_with into frontmatter
        assert!(output.skill_md.contains("generated_with:"));
    }

    #[tokio::test]
    async fn test_generate_with_examples_content() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1).with_test(false);

        let mut data = make_test_data();
        data.examples_content = "# Example\nimport testpkg\ntestpkg.run()".to_string();

        let output = gen.generate(&data).await.unwrap();
        assert!(
            output.skill_md.contains("---"),
            "should produce valid output with examples"
        );
    }

    #[tokio::test]
    async fn test_generate_max_retries_zero_still_validates() {
        // max_retries=0 should still run one validation pass (not skip all validation)
        let gen = Generator::new(Box::new(MockLlmClient::new()), 0).with_test(false);

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        // Output should still have frontmatter (normalization + lint ran)
        assert!(
            output.skill_md.contains("---"),
            "max_retries=0 should still produce valid output"
        );
        assert!(
            output.skill_md.contains("ecosystem:"),
            "should have ecosystem in frontmatter"
        );
    }

    #[test]
    fn test_get_client_returns_main_by_default() {
        let client = Box::new(MockLlmClient::new());
        let gen = Generator::new(client, 5);
        // All stages should return the main client when no per-stage override
        // We can't directly compare references, but we can verify the method doesn't panic
        let _ = gen.get_client("extract");
        let _ = gen.get_client("map");
        let _ = gen.get_client("learn");
        let _ = gen.get_client("create");
        let _ = gen.get_client("test");
    }

    #[test]
    fn test_per_stage_client_builders() {
        let client = Box::new(MockLlmClient::new());
        let gen = Generator::new(client, 5)
            .with_extract_client(Box::new(MockLlmClient::new()))
            .with_test_client(Box::new(MockLlmClient::new()));
        assert!(gen.extract_client.is_some());
        assert!(gen.map_client.is_none());
        assert!(gen.learn_client.is_none());
        assert!(gen.create_client.is_none());
        assert!(gen.test_client.is_some());
    }

    // ========================================================================
    // strip_markdown_fences() edge cases
    // ========================================================================

    #[test]
    fn test_strip_markdown_fences_nested_backticks() {
        // Content containing ``` inside should still strip outer fences
        let input = "```markdown\n# Hello\n```python\nprint('hi')\n```\nmore text\n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "# Hello\n```python\nprint('hi')\n```\nmore text");
    }

    #[test]
    fn test_strip_markdown_fences_only_opening_fence() {
        let input = "```markdown\n# Hello\nworld";
        let result = strip_markdown_fences(input);
        // No closing fence, so content should be returned unchanged
        assert_eq!(result, input);
    }

    #[test]
    fn test_strip_markdown_fences_only_closing_fence() {
        let input = "# Hello\nworld\n```";
        let result = strip_markdown_fences(input);
        // No opening fence, so content should be returned unchanged
        assert_eq!(result, input);
    }

    #[test]
    fn test_strip_markdown_fences_empty_string() {
        let result = strip_markdown_fences("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_markdown_fences_just_backticks() {
        // Just "```" alone -- starts_with("```") and ends_with("```") both true,
        // but strip_prefix("```") yields "", and strip_suffix("```") on "" is None,
        // so unwrap_or_else returns the original string.
        let result = strip_markdown_fences("```");
        assert_eq!(result, "```");
    }

    #[test]
    fn test_strip_markdown_fences_just_backticks_markdown() {
        // "```markdown```" -- no newline, not a valid fence pair
        let result = strip_markdown_fences("```markdown```");
        assert_eq!(result, "```markdown```");
    }

    #[test]
    fn test_strip_markdown_fences_whitespace_around() {
        // Leading/trailing whitespace should be handled by trim()
        let input = "  ```markdown\n# Hello\n```  ";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "# Hello");
    }

    // ========================================================================
    // Generator builder methods: review_client, review, review_max_retries,
    // parallel_extraction
    // ========================================================================

    #[test]
    fn test_with_review_client_sets_client() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_review_client(Box::new(MockLlmClient::new()));
        assert!(gen.review_client.is_some());
    }

    #[test]
    fn test_with_review_sets_flag() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3).with_review(false);
        assert!(!gen.enable_review);

        let gen2 = Generator::new(Box::new(MockLlmClient::new()), 3).with_review(true);
        assert!(gen2.enable_review);
    }

    #[test]
    fn test_with_review_max_retries_sets_value() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3).with_review_max_retries(10);
        assert_eq!(gen.review_max_retries, 10);
    }

    #[test]
    fn test_with_parallel_extraction_sets_flag() {
        // Default is true
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3);
        assert!(gen.parallel_extraction);

        let gen2 = gen.with_parallel_extraction(false);
        assert!(!gen2.parallel_extraction);
    }

    #[test]
    fn test_generator_defaults_review_enabled_and_retries() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3);
        assert!(gen.enable_review);
        assert_eq!(gen.review_max_retries, 5);
        assert!(gen.review_client.is_none());
    }

    // ========================================================================
    // get_client() routing: verify per-stage overrides are returned
    // ========================================================================

    /// A mock client that returns a fixed identifier string, used to verify
    /// which client get_client() routes to.
    struct IdentifiableClient {
        id: String,
    }

    impl IdentifiableClient {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }

    #[async_trait::async_trait]
    impl LlmClient for IdentifiableClient {
        async fn complete(&self, _prompt: &str) -> anyhow::Result<String> {
            Ok(self.id.clone())
        }
    }

    #[tokio::test]
    async fn test_get_client_review_returns_override_when_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3)
            .with_review_client(Box::new(IdentifiableClient::new("review-override")));

        let client = gen.get_client("review");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "review-override");
    }

    #[tokio::test]
    async fn test_get_client_test_returns_override_when_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3)
            .with_test_client(Box::new(IdentifiableClient::new("test-override")));

        let client = gen.get_client("test");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "test-override");
    }

    #[tokio::test]
    async fn test_get_client_unknown_stage_returns_main() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3);

        let client = gen.get_client("nonexistent");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "main");
    }

    #[tokio::test]
    async fn test_get_client_review_falls_back_to_main_when_not_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3);
        // review_client is None, should fall back to main

        let client = gen.get_client("review");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "main");
    }

    #[tokio::test]
    async fn test_get_client_test_falls_back_to_main_when_not_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3);

        let client = gen.get_client("test");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "main");
    }

    // ========================================================================
    // GenerateOutput struct
    // ========================================================================

    #[test]
    fn test_generate_output_creation_and_field_access() {
        let output = GenerateOutput {
            skill_md: "# Test SKILL.md".to_string(),
            unresolved_warnings: vec![],
        };
        assert_eq!(output.skill_md, "# Test SKILL.md");
        assert!(output.unresolved_warnings.is_empty());
    }

    #[test]
    fn test_generate_output_with_warnings() {
        let warning = ReviewIssue {
            severity: crate::review::Severity::Warning,
            category: "accuracy".to_string(),
            complaint: "Wrong version".to_string(),
            evidence: "expected 2.0, got 1.0".to_string(),
        };

        let output = GenerateOutput {
            skill_md: "# SKILL".to_string(),
            unresolved_warnings: vec![warning],
        };
        assert_eq!(output.unresolved_warnings.len(), 1);
        assert_eq!(
            output.unresolved_warnings[0].severity,
            crate::review::Severity::Warning
        );
        assert_eq!(output.unresolved_warnings[0].category, "accuracy");
        assert_eq!(output.unresolved_warnings[0].complaint, "Wrong version");
        assert_eq!(
            output.unresolved_warnings[0].evidence,
            "expected 2.0, got 1.0"
        );
    }

    #[test]
    fn test_generate_output_debug_format() {
        let output = GenerateOutput {
            skill_md: "test".to_string(),
            unresolved_warnings: vec![],
        };
        // GenerateOutput derives Debug, ensure it doesn't panic
        let debug_str = format!("{:?}", output);
        assert!(debug_str.contains("GenerateOutput"));
        assert!(debug_str.contains("test"));
    }

    // ========================================================================
    // Builder methods: with_container_config, with_prompts_config
    // ========================================================================

    #[test]
    fn test_with_container_config_sets_config() {
        let custom = ContainerConfig {
            timeout: 300,
            cleanup: false,
            runtime: "docker".to_string(),
            ..ContainerConfig::default()
        };

        let gen = Generator::new(Box::new(MockLlmClient::new()), 3).with_container_config(custom);

        assert_eq!(gen.container_config.timeout, 300);
        assert!(!gen.container_config.cleanup);
        assert_eq!(gen.container_config.runtime, "docker");
    }

    #[test]
    fn test_with_prompts_config_sets_config() {
        let prompts = PromptsConfig {
            override_prompts: true,
            extract_custom: Some("custom extract".to_string()),
            create_custom: Some("custom create".to_string()),
            ..PromptsConfig::default()
        };

        let gen = Generator::new(Box::new(MockLlmClient::new()), 3).with_prompts_config(prompts);

        assert!(gen.prompts_config.override_prompts);
        assert_eq!(
            gen.prompts_config.extract_custom.as_deref(),
            Some("custom extract")
        );
        assert_eq!(
            gen.prompts_config.create_custom.as_deref(),
            Some("custom create")
        );
    }

    // ========================================================================
    // Builder methods: with_map_client, with_learn_client, with_create_client
    // ========================================================================

    #[test]
    fn test_with_map_client_sets_client() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_map_client(Box::new(MockLlmClient::new()));
        assert!(gen.map_client.is_some());
    }

    #[test]
    fn test_with_learn_client_sets_client() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_learn_client(Box::new(MockLlmClient::new()));
        assert!(gen.learn_client.is_some());
    }

    #[test]
    fn test_with_create_client_sets_client() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_create_client(Box::new(MockLlmClient::new()));
        assert!(gen.create_client.is_some());
    }

    // ========================================================================
    // get_client() routing for extract, map, learn, create overrides
    // ========================================================================

    #[tokio::test]
    async fn test_get_client_extract_returns_override_when_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3)
            .with_extract_client(Box::new(IdentifiableClient::new("extract-override")));

        let client = gen.get_client("extract");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "extract-override");
    }

    #[tokio::test]
    async fn test_get_client_map_returns_override_when_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3)
            .with_map_client(Box::new(IdentifiableClient::new("map-override")));

        let client = gen.get_client("map");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "map-override");
    }

    #[tokio::test]
    async fn test_get_client_learn_returns_override_when_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3)
            .with_learn_client(Box::new(IdentifiableClient::new("learn-override")));

        let client = gen.get_client("learn");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "learn-override");
    }

    #[tokio::test]
    async fn test_get_client_create_returns_override_when_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3)
            .with_create_client(Box::new(IdentifiableClient::new("create-override")));

        let client = gen.get_client("create");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "create-override");
    }

    // ========================================================================
    // Sequential extraction path
    // ========================================================================

    #[tokio::test]
    async fn test_generate_sequential_extraction() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_parallel_extraction(false);

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(
            output.skill_md.contains("---"),
            "sequential extraction should still produce valid output"
        );
    }

    // ========================================================================
    // strip_markdown_fences: other language identifiers
    // ========================================================================

    #[test]
    fn test_strip_markdown_fences_yaml_lang() {
        let input = "```yaml\nkey: value\n```";
        let result = strip_markdown_fences(input);
        // New implementation correctly strips the language tag line
        assert_eq!(result, "key: value");
    }

    #[test]
    fn test_strip_markdown_fences_only_backtick_pair() {
        // "``````" -- no newline, not a valid fence pair
        let result = strip_markdown_fences("``````");
        assert_eq!(result, "``````");
    }

    // ========================================================================
    // Source context assembly variations in generate()
    // ========================================================================

    #[tokio::test]
    async fn test_generate_with_empty_test_content() {
        // When both examples and test content are empty, source_content used as-is
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let mut data = make_test_data();
        data.test_content = String::new();
        data.examples_content = String::new();

        let output = gen.generate(&data).await.unwrap();
        assert!(
            output.skill_md.contains("---"),
            "should produce valid output with no examples or tests"
        );
    }

    #[tokio::test]
    async fn test_generate_with_examples_and_tests() {
        // When both examples and tests are present, examples take priority in source context
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let mut data = make_test_data();
        data.examples_content = "import pkg\npkg.hello()".to_string();
        data.test_content = "def test_hello(): pass".to_string();

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // Full builder chain
    // ========================================================================

    #[test]
    fn test_generator_full_builder_chain() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 2)
            .with_extract_client(Box::new(MockLlmClient::new()))
            .with_map_client(Box::new(MockLlmClient::new()))
            .with_learn_client(Box::new(MockLlmClient::new()))
            .with_create_client(Box::new(MockLlmClient::new()))
            .with_review_client(Box::new(MockLlmClient::new()))
            .with_test_client(Box::new(MockLlmClient::new()))
            .with_test(true)
            .with_test_mode(ValidationMode::Adaptive)
            .with_review(true)
            .with_review_max_retries(3)
            .with_container_config(ContainerConfig::default())
            .with_parallel_extraction(false)
            .with_prompts_config(PromptsConfig::default())
            .with_existing_skill("old".to_string())
            .with_model_name("test-model".to_string());

        assert!(gen.extract_client.is_some());
        assert!(gen.map_client.is_some());
        assert!(gen.learn_client.is_some());
        assert!(gen.create_client.is_some());
        assert!(gen.review_client.is_some());
        assert!(gen.test_client.is_some());
        assert!(gen.enable_test);
        assert!(matches!(gen.test_mode, ValidationMode::Adaptive));
        assert!(gen.enable_review);
        assert_eq!(gen.review_max_retries, 3);
        assert!(!gen.parallel_extraction);
        assert_eq!(gen.existing_skill.as_deref(), Some("old"));
        assert_eq!(gen.model_name.as_deref(), Some("test-model"));
        assert_eq!(gen.max_retries, 2);
    }

    // ========================================================================
    // Review disabled path
    // ========================================================================

    #[tokio::test]
    async fn test_generate_with_review_disabled() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(output.skill_md.contains("---"));
        assert!(
            output.unresolved_warnings.is_empty(),
            "review disabled should produce no warnings"
        );
    }

    // ========================================================================
    // ScriptedClient: returns responses in order for precise pipeline control
    // ========================================================================

    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// A mock LLM client that returns pre-scripted responses in order.
    /// Falls back to MockLlmClient when the script is exhausted.
    struct ScriptedClient {
        responses: Arc<Mutex<VecDeque<String>>>,
        fallback: MockLlmClient,
    }

    impl ScriptedClient {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(VecDeque::from(responses))),
                fallback: MockLlmClient::new(),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmClient for ScriptedClient {
        async fn complete(&self, prompt: &str) -> anyhow::Result<String> {
            let next = {
                let mut queue = self.responses.lock().unwrap();
                queue.pop_front()
            };
            if let Some(response) = next {
                Ok(response)
            } else {
                self.fallback.complete(prompt).await
            }
        }
    }

    /// A mock LLM client that delegates to MockLlmClient but overrides
    /// the create stage response (create agent) with custom content.
    struct CustomCreateClient {
        create_response: String,
        fallback: MockLlmClient,
    }

    impl CustomCreateClient {
        fn new(create_response: &str) -> Self {
            Self {
                create_response: create_response.to_string(),
                fallback: MockLlmClient::new(),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmClient for CustomCreateClient {
        async fn complete(&self, prompt: &str) -> anyhow::Result<String> {
            // Override create/fix responses; delegate everything else
            if prompt.contains("creating an agent rules file")
                || prompt.contains("Here is the current SKILL.md")
            {
                Ok(self.create_response.clone())
            } else {
                self.fallback.complete(prompt).await
            }
        }
    }

    /// Valid SKILL.md content that passes the linter.
    fn valid_skill_md() -> &'static str {
        r#"---
name: testpkg
description: A test package
version: 1.0.0
ecosystem: python
---

## Imports

```python
import testpkg
```

## Core Patterns

```python
testpkg.run()
```

## Pitfalls

### Wrong: missing arg

```python
testpkg.run(bad=True)
```

### Right: correct usage

```python
testpkg.run()
```
"#
    }

    /// SKILL.md missing the Pitfalls section (triggers lint error, not security).
    fn skill_md_missing_pitfalls() -> &'static str {
        r#"---
name: testpkg
description: A test package
version: 1.0.0
ecosystem: python
---

## Imports

```python
import testpkg
```

## Core Patterns

```python
testpkg.run()
```
"#
    }

    /// SKILL.md with a security pattern embedded in prose (not in code block).
    fn skill_md_with_security_issue() -> &'static str {
        r#"---
name: testpkg
description: A test package
version: 1.0.0
ecosystem: python
---

## Imports

```python
import testpkg
```

## Core Patterns

```python
testpkg.run()
```

## Pitfalls

To clean up, run: rm -rf /

### Wrong: bad usage

```python
testpkg.run(bad=True)
```

### Right: correct usage

```python
testpkg.run()
```
"#
    }

    // ========================================================================
    // Lint error handling: format validation fails, then retries
    // ========================================================================

    #[tokio::test]
    async fn test_generate_format_error_triggers_retry() {
        // Create client returns content missing Pitfalls on first call,
        // then valid content on the fix call.
        let responses = vec![
            skill_md_missing_pitfalls().to_string(), // First create response (missing section)
            valid_skill_md().to_string(),            // Fix response (valid)
        ];
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(ScriptedClient::new(responses)));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(
            output.skill_md.contains("## Pitfalls"),
            "retry should produce output with Pitfalls section"
        );
    }

    // ========================================================================
    // Security error: lint finds security issue, pipeline bails immediately
    // ========================================================================

    #[tokio::test]
    async fn test_generate_security_error_bails_immediately() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(CustomCreateClient::new(
                skill_md_with_security_issue(),
            )));

        let data = make_test_data();
        let result = gen.generate(&data).await;

        assert!(result.is_err(), "security error should bail the pipeline");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("SECURITY"),
            "error should mention SECURITY: {}",
            err_msg
        );
    }

    // ========================================================================
    // Format max retries exhausted: returns best attempt despite errors
    // ========================================================================

    #[tokio::test]
    async fn test_generate_format_max_retries_exhausted() {
        // Create client always returns content missing Pitfalls (never fixes it)
        let gen = Generator::new(Box::new(MockLlmClient::new()), 2)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(CustomCreateClient::new(
                skill_md_missing_pitfalls(),
            )));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        // Should return despite format issues (max retries reached)
        assert!(
            !output.skill_md.is_empty(),
            "should still return content after max retries"
        );
    }

    // ========================================================================
    // Review loop: review fails, retries, eventually passes
    // ========================================================================

    #[tokio::test]
    async fn test_generate_review_fail_then_pass() {
        // Review client: first verdict fails, second passes
        // Call sequence for review client:
        //   1. introspect prompt -> script
        //   2. verdict prompt -> fail with issues
        //   3. introspect prompt -> script (retry)
        //   4. verdict prompt -> pass
        let review_responses = vec![
            // First review cycle: introspect script
            r#"```python
# /// script
# requires-python = ">=3.10"
# dependencies = ["testpkg"]
# ///
import json
result = {"version_installed": "1.0.0", "version_expected": "1.0.0", "imports": [], "signatures": [], "dates": []}
print(json.dumps(result))
```"#
                .to_string(),
            // First review cycle: verdict - FAIL
            r#"{"passed": false, "issues": [{"severity": "error", "category": "accuracy", "complaint": "Wrong version in frontmatter", "evidence": "expected 1.0.0, got unknown"}]}"#.to_string(),
            // Second review cycle: introspect script
            r#"```python
# /// script
# requires-python = ">=3.10"
# dependencies = ["testpkg"]
# ///
import json
result = {"version_installed": "1.0.0", "version_expected": "1.0.0", "imports": [], "signatures": [], "dates": []}
print(json.dumps(result))
```"#
                .to_string(),
            // Second review cycle: verdict - PASS
            r#"{"passed": true, "issues": []}"#.to_string(),
        ];

        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(true)
            .with_review_max_retries(3)
            .with_review_client(Box::new(ScriptedClient::new(review_responses)));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(
            output.unresolved_warnings.is_empty(),
            "review passed on retry, no unresolved warnings"
        );
    }

    // ========================================================================
    // Review max retries exhausted: unresolved warnings returned
    // ========================================================================

    #[tokio::test]
    async fn test_generate_review_max_retries_has_unresolved_warnings() {
        // Review client always returns failures (need "error" severity to trigger retries,
        // since `passed` is recomputed from issues — warnings-only would pass)
        let fail_verdict = r#"{"passed": false, "issues": [{"severity": "error", "category": "accuracy", "complaint": "Wrong version number", "evidence": "pip says 2.0"}, {"severity": "warning", "category": "accuracy", "complaint": "Stale version number", "evidence": "pip says 2.0"}]}"#;
        let introspect_script = r#"```python
# /// script
# requires-python = ">=3.10"
# dependencies = ["testpkg"]
# ///
import json
result = {"version_installed": "1.0.0", "version_expected": "1.0.0", "imports": [], "signatures": [], "dates": []}
print(json.dumps(result))
```"#;

        // 2 retries = 3 review attempts (0, 1, 2)
        // Each attempt: introspect + verdict = 2 calls
        let mut review_responses = Vec::new();
        for _ in 0..3 {
            review_responses.push(introspect_script.to_string());
            review_responses.push(fail_verdict.to_string());
        }

        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(true)
            .with_review_max_retries(2)
            .with_review_client(Box::new(ScriptedClient::new(review_responses)));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(
            !output.unresolved_warnings.is_empty(),
            "should have unresolved issues when review exhausts retries"
        );
        assert_eq!(output.unresolved_warnings.len(), 2);
        assert_eq!(
            output.unresolved_warnings[0].complaint,
            "Wrong version number"
        );
        assert_eq!(
            output.unresolved_warnings[1].complaint,
            "Stale version number"
        );
    }

    // ========================================================================
    // Non-Python language skips test agent path (functional validation skipped)
    // ========================================================================

    #[tokio::test]
    async fn test_generate_non_python_test_enabled_skips_test_agent() {
        // Non-Python + test enabled: functional validation skipped, no test agent
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(true)
            .with_review(false);

        let mut data = make_test_data();
        data.language = Language::Rust;

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // Post-normalization lint errors (non-security) are warned but not fatal
    // ========================================================================

    #[tokio::test]
    async fn test_generate_post_normalization_lint_errors_non_fatal() {
        // This exercises the post-normalization lint check (lines 581-609).
        // The normalizer + linter may produce non-security lint errors.
        // Using the standard MockLlmClient should produce output that goes
        // through the full path including post-normalization checks.
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        // The output should exist (non-security errors don't bail)
        assert!(!output.skill_md.is_empty());
        assert!(output.skill_md.contains("---"));
    }

    // ========================================================================
    // Review with non-Python language (introspection skipped)
    // ========================================================================

    #[tokio::test]
    async fn test_generate_review_non_python_skips_introspection() {
        // Non-Python language: review runs but introspection is skipped
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(true)
            .with_review_max_retries(0);

        let mut data = make_test_data();
        data.language = Language::JavaScript;

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // LLM error during extract propagates
    // ========================================================================

    /// A client that always returns an error.
    struct FailingClient {
        message: String,
    }

    impl FailingClient {
        fn new(msg: &str) -> Self {
            Self {
                message: msg.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmClient for FailingClient {
        async fn complete(&self, _prompt: &str) -> anyhow::Result<String> {
            Err(anyhow::anyhow!("{}", self.message))
        }
    }

    #[tokio::test]
    async fn test_generate_extract_error_propagates() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_extract_client(Box::new(FailingClient::new("API limit reached")));

        let data = make_test_data();
        let result = gen.generate(&data).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("API limit reached"));
    }

    // ========================================================================
    // LLM error during create propagates
    // ========================================================================

    #[tokio::test]
    async fn test_generate_create_error_propagates() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(FailingClient::new("rate limited")));

        let data = make_test_data();
        let result = gen.generate(&data).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("rate limited"));
    }

    // ========================================================================
    // Sequential extraction with error propagates
    // ========================================================================

    #[tokio::test]
    async fn test_generate_sequential_learn_error_propagates() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_parallel_extraction(false)
            .with_learn_client(Box::new(FailingClient::new("timeout")));

        let data = make_test_data();
        let result = gen.generate(&data).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timeout"));
    }

    // ========================================================================
    // Update mode (existing_skill) with review enabled
    // ========================================================================

    #[tokio::test]
    async fn test_generate_update_mode_with_review() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(true)
            .with_review_max_retries(0)
            .with_existing_skill("# Old SKILL.md content".to_string());

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(output.skill_md.contains("---"));
    }

    // ========================================================================
    // Create returns fenced output that gets stripped
    // ========================================================================

    #[tokio::test]
    async fn test_generate_strips_markdown_fences_from_create() {
        let fenced = format!("```markdown\n{}\n```", valid_skill_md());
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(CustomCreateClient::new(&fenced)));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        // Should have stripped the fences and produced valid output
        assert!(output.skill_md.contains("## Imports"));
        assert!(!output.skill_md.starts_with("```"));
    }

    // ========================================================================
    // Format fix retry also strips fences from fix response
    // ========================================================================

    #[tokio::test]
    async fn test_generate_format_fix_strips_fences() {
        // First response: missing Pitfalls. Fix response: fenced valid content.
        let fenced_valid = format!("```markdown\n{}\n```", valid_skill_md());
        let responses = vec![skill_md_missing_pitfalls().to_string(), fenced_valid];
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(ScriptedClient::new(responses)));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(output.skill_md.contains("## Pitfalls"));
    }

    // ========================================================================
    // strip_markdown_fences: additional edge cases
    // ========================================================================

    #[test]
    fn test_strip_markdown_fences_multiline_content_preserved() {
        // Ensure multi-line content between fences is fully preserved
        let input = "```markdown\nline1\nline2\nline3\n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn test_strip_markdown_fences_plain_with_newline_after_opening() {
        // Plain fences with newline directly after opening ```
        let input = "```\ncontent here\n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "content here");
    }

    #[test]
    fn test_strip_markdown_fences_markdown_fence_with_extra_whitespace_content() {
        // Content with leading/trailing whitespace inside fences gets trimmed
        let input = "```markdown\n  \n  spaced content  \n  \n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "spaced content");
    }

    #[test]
    fn test_strip_markdown_fences_content_that_looks_like_fence_but_isnt() {
        // Backticks in the middle of content, no matching pattern
        let input = "some text ``` more text";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "some text ``` more text");
    }

    #[test]
    fn test_strip_markdown_fences_four_backticks_no_newline() {
        // Four backticks with no newline — no body to extract, returns original
        let result = strip_markdown_fences("````");
        assert_eq!(result, "````");
    }

    #[test]
    fn test_strip_markdown_fences_only_whitespace() {
        let result = strip_markdown_fences("   ");
        assert_eq!(result, "   ");
    }

    #[test]
    fn test_strip_markdown_fences_markdown_with_crlf() {
        // Windows-style line endings
        let input = "```markdown\r\n# Title\r\ncontent\r\n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "# Title\r\ncontent");
    }

    #[test]
    fn test_strip_markdown_fences_python_lang_identifier() {
        // ```python ... ``` - not "markdown" but plain fence check catches it
        let input = "```python\nprint('hello')\n```";
        let result = strip_markdown_fences(input);
        // New implementation correctly strips the language tag line
        assert_eq!(result, "print('hello')");
    }

    #[test]
    fn test_strip_markdown_fences_no_content_between_markdown_fences() {
        // ```markdown\n```  - empty content
        let input = "```markdown\n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_markdown_fences_no_content_between_plain_fences() {
        // ```\n``` - empty content
        let input = "```\n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_markdown_fences_preserves_inner_markdown_fences() {
        // Content has its own ```markdown inside
        let input = "```markdown\n# Doc\n\n```markdown\nnested\n```\n\nmore\n```";
        let result = strip_markdown_fences(input);
        assert_eq!(result, "# Doc\n\n```markdown\nnested\n```\n\nmore");
    }

    // ========================================================================
    // GenerateOutput: Debug with warnings
    // ========================================================================

    #[test]
    fn test_generate_output_debug_with_warnings() {
        let warning = ReviewIssue {
            severity: crate::review::Severity::Warning,
            category: "safety".to_string(),
            complaint: "Contains suspicious pattern".to_string(),
            evidence: "line 42".to_string(),
        };
        let output = GenerateOutput {
            skill_md: "content".to_string(),
            unresolved_warnings: vec![warning],
        };
        let debug_str = format!("{:?}", output);
        assert!(debug_str.contains("GenerateOutput"));
        assert!(debug_str.contains("unresolved_warnings"));
        assert!(debug_str.contains("content"));
    }

    // ========================================================================
    // Sequential extraction: error on extract and map propagate
    // ========================================================================

    #[tokio::test]
    async fn test_generate_sequential_extract_error_propagates() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_parallel_extraction(false)
            .with_extract_client(Box::new(FailingClient::new("extract failed")));

        let data = make_test_data();
        let result = gen.generate(&data).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("extract failed"));
    }

    #[tokio::test]
    async fn test_generate_sequential_map_error_propagates() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_parallel_extraction(false)
            .with_map_client(Box::new(FailingClient::new("map failed")));

        let data = make_test_data();
        let result = gen.generate(&data).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("map failed"));
    }

    // ========================================================================
    // Parallel extraction: error on map and learn propagate
    // ========================================================================

    #[tokio::test]
    async fn test_generate_parallel_map_error_propagates() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_parallel_extraction(true)
            .with_map_client(Box::new(FailingClient::new("map exploded")));

        let data = make_test_data();
        let result = gen.generate(&data).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("map exploded"));
    }

    #[tokio::test]
    async fn test_generate_parallel_learn_error_propagates() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_parallel_extraction(true)
            .with_learn_client(Box::new(FailingClient::new("learn broke")));

        let data = make_test_data();
        let result = gen.generate(&data).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("learn broke"));
    }

    // ========================================================================
    // Source context assembly: tests-only path (no examples, has tests)
    // ========================================================================

    #[tokio::test]
    async fn test_generate_source_context_tests_only() {
        // When examples_content is empty but test_content is not:
        // source_with_context uses test code prefix
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let mut data = make_test_data();
        data.examples_content = String::new();
        data.test_content = "def test_something(): assert True".to_string();

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // Source context assembly: examples + tests (examples take priority)
    // ========================================================================

    #[tokio::test]
    async fn test_generate_source_context_examples_priority_over_tests() {
        // When both examples and tests are present, examples prefix is used
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let mut data = make_test_data();
        data.examples_content = "import pkg; pkg.do_thing()".to_string();
        data.test_content = "def test_thing(): assert True".to_string();

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
        assert!(output.skill_md.contains("---"));
    }

    // ========================================================================
    // Format validation: max_retries=1 with persistent errors
    // ========================================================================

    #[tokio::test]
    async fn test_generate_format_max_retries_one_exhausted() {
        // max_retries=1 means 1 validation pass, no retry on failure
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(CustomCreateClient::new(
                skill_md_missing_pitfalls(),
            )));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        // Should return despite format issues (only 1 pass, no retry)
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // Format error with multiple retries: first two fail, third succeeds
    // ========================================================================

    #[tokio::test]
    async fn test_generate_format_error_multiple_retries() {
        let responses = vec![
            skill_md_missing_pitfalls().to_string(), // First create response (bad)
            skill_md_missing_pitfalls().to_string(), // First fix response (still bad)
            valid_skill_md().to_string(),            // Second fix response (good)
        ];
        let gen = Generator::new(Box::new(MockLlmClient::new()), 5)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(ScriptedClient::new(responses)));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(output.skill_md.contains("## Pitfalls"));
    }

    // ========================================================================
    // Validation: format passes on first try, no retry needed
    // ========================================================================

    #[tokio::test]
    async fn test_generate_format_passes_first_try() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 3)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(CustomCreateClient::new(valid_skill_md())));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(output.skill_md.contains("## Imports"));
        assert!(output.skill_md.contains("## Pitfalls"));
    }

    // ========================================================================
    // Test disabled + Python language: functional validation skipped
    // ========================================================================

    #[tokio::test]
    async fn test_generate_test_disabled_python_skips_functional_validation() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let mut data = make_test_data();
        data.language = Language::Python;

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // Test enabled + Python: exercises the test agent path
    // (MockLlmClient returns mock test agent responses)
    // ========================================================================

    #[tokio::test]
    async fn test_generate_test_enabled_python_exercises_test_agent_path() {
        // With test enabled for Python, the pipeline enters the test agent code path.
        // MockLlmClient generates a mock test script. The actual container execution
        // will fail (no container in test env), but the error is caught gracefully.
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(true)
            .with_review(false);

        let mut data = make_test_data();
        data.language = Language::Python;

        let output = gen.generate(&data).await.unwrap();
        // Pipeline should complete even if test agent container validation fails
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // Test enabled + non-Python: test agent is skipped
    // ========================================================================

    #[tokio::test]
    async fn test_generate_test_enabled_javascript_skips_test_agent() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(true)
            .with_review(false);

        let mut data = make_test_data();
        data.language = Language::JavaScript;

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    #[tokio::test]
    async fn test_generate_test_enabled_go_skips_test_agent() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(true)
            .with_review(false);

        let mut data = make_test_data();
        data.language = Language::Go;

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // get_client: all per-stage fallbacks to main when not set
    // ========================================================================

    #[tokio::test]
    async fn test_get_client_extract_falls_back_to_main_when_not_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3);

        let client = gen.get_client("extract");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "main");
    }

    #[tokio::test]
    async fn test_get_client_map_falls_back_to_main_when_not_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3);

        let client = gen.get_client("map");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "main");
    }

    #[tokio::test]
    async fn test_get_client_learn_falls_back_to_main_when_not_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3);

        let client = gen.get_client("learn");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "main");
    }

    #[tokio::test]
    async fn test_get_client_create_falls_back_to_main_when_not_set() {
        let gen = Generator::new(Box::new(IdentifiableClient::new("main")), 3);

        let client = gen.get_client("create");
        let response = client.complete("anything").await.unwrap();
        assert_eq!(response, "main");
    }

    // ========================================================================
    // Multiple review issues are all reported when max retries exhausted
    // ========================================================================

    #[tokio::test]
    async fn test_generate_review_max_retries_multiple_issues() {
        let fail_verdict = r#"{"passed": false, "issues": [
            {"severity": "error", "category": "accuracy", "complaint": "Wrong version", "evidence": "expected 2.0"},
            {"severity": "warning", "category": "safety", "complaint": "Suspicious code pattern", "evidence": "line 10"}
        ]}"#;
        let introspect_script = r#"```python
# /// script
# requires-python = ">=3.10"
# dependencies = ["testpkg"]
# ///
import json
result = {"version_installed": "1.0.0", "version_expected": "1.0.0", "imports": [], "signatures": [], "dates": []}
print(json.dumps(result))
```"#;

        // 0 retries = 1 attempt
        let review_responses = vec![introspect_script.to_string(), fail_verdict.to_string()];

        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(true)
            .with_review_max_retries(0)
            .with_review_client(Box::new(ScriptedClient::new(review_responses)));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert_eq!(output.unresolved_warnings.len(), 2);
        assert_eq!(output.unresolved_warnings[0].complaint, "Wrong version");
        assert_eq!(
            output.unresolved_warnings[1].complaint,
            "Suspicious code pattern"
        );
    }

    // ========================================================================
    // Review passes on first attempt: no retries needed
    // ========================================================================

    #[tokio::test]
    async fn test_generate_review_passes_first_attempt() {
        let introspect_script = r#"```python
# /// script
# requires-python = ">=3.10"
# dependencies = ["testpkg"]
# ///
import json
result = {"version_installed": "1.0.0", "version_expected": "1.0.0", "imports": [], "signatures": [], "dates": []}
print(json.dumps(result))
```"#;
        let pass_verdict = r#"{"passed": true, "issues": []}"#;

        let review_responses = vec![introspect_script.to_string(), pass_verdict.to_string()];

        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(true)
            .with_review_max_retries(3)
            .with_review_client(Box::new(ScriptedClient::new(review_responses)));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(output.unresolved_warnings.is_empty());
    }

    // ========================================================================
    // Review + test both disabled: minimal pipeline path
    // ========================================================================

    #[tokio::test]
    async fn test_generate_minimal_pipeline_no_test_no_review() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(output.skill_md.contains("---"));
        assert!(output.unresolved_warnings.is_empty());
    }

    // ========================================================================
    // Security error on first validation pass (not during retry)
    // ========================================================================

    #[tokio::test]
    async fn test_generate_security_error_on_first_pass_bails() {
        // Ensure security bail works even when max_retries is high
        let gen = Generator::new(Box::new(MockLlmClient::new()), 10)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(CustomCreateClient::new(
                skill_md_with_security_issue(),
            )));

        let data = make_test_data();
        let result = gen.generate(&data).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SECURITY"));
    }

    // ========================================================================
    // Plain fences stripped from create output (not markdown-tagged)
    // ========================================================================

    #[tokio::test]
    async fn test_generate_strips_plain_fences_from_create() {
        let fenced = format!("```\n{}\n```", valid_skill_md());
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false)
            .with_create_client(Box::new(CustomCreateClient::new(&fenced)));

        let data = make_test_data();
        let output = gen.generate(&data).await.unwrap();

        assert!(output.skill_md.contains("## Imports"));
        assert!(!output.skill_md.starts_with("```"));
    }

    // ========================================================================
    // Changelog content flows through to learn stage
    // ========================================================================

    #[tokio::test]
    async fn test_generate_with_changelog_content() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let mut data = make_test_data();
        data.changelog_content = "## 1.0.0\n- Initial release".to_string();

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // License flows through to normalizer
    // ========================================================================

    #[tokio::test]
    async fn test_generate_with_no_license() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let mut data = make_test_data();
        data.license = None;

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // Project URLs flow through to normalizer
    // ========================================================================

    #[tokio::test]
    async fn test_generate_with_project_urls() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let mut data = make_test_data();
        data.project_urls = vec![
            ("Homepage".to_string(), "https://example.com".to_string()),
            (
                "Repository".to_string(),
                "https://github.com/test/pkg".to_string(),
            ),
        ];

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }

    // ========================================================================
    // Large source_file_count value
    // ========================================================================

    #[tokio::test]
    async fn test_generate_with_large_source_file_count() {
        let gen = Generator::new(Box::new(MockLlmClient::new()), 1)
            .with_test(false)
            .with_review(false);

        let mut data = make_test_data();
        data.source_file_count = 500;

        let output = gen.generate(&data).await.unwrap();
        assert!(!output.skill_md.is_empty());
    }
}
