//! Test code validator — orchestrates the parse → generate → execute loop.
//! Parses patterns from SKILL.md, generates test code via LLM, and runs it
//! in a container. Returns pass/fail with actionable feedback for retries.

use anyhow::Result;
use tracing::{debug, info, warn};

use super::container_executor::ContainerExecutor;
use super::executor::{
    CargoExecutor, ExecutionResult, GoExecutor, JavaExecutor, NodeExecutor, PythonUvExecutor,
};
use super::go_code_gen::GoCodeGenerator;
use super::go_parser::GoParser;
use super::java_code_gen::JavaCodeGenerator;
use super::java_parser::JavaParser;
use super::js_code_gen::JsCodeGenerator;
use super::js_parser::JsParser;
use super::python_code_gen::PythonCodeGenerator;
use super::python_parser::PythonParser;
use super::rust_code_gen::RustCodeGenerator;
use super::rust_parser::RustParser;
use super::{CodePattern, LanguageCodeGenerator, LanguageExecutor, LanguageParser};
use crate::config::{ContainerConfig, ExecutionMode, InstallSource};
use crate::detector::Language;
use crate::llm::client::LlmClient;

/// Validation mode for test agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationMode {
    /// Test ALL patterns — the real "thorough" mode. Every code block in
    /// Core Patterns gets compiled and run against the library. This is the
    /// only mode that mechanically guarantees no broken examples ship.
    #[default]
    Thorough,
    /// Test 2-3 patterns from different categories (BasicUsage, Configuration,
    /// ErrorHandling). Fast iteration mode — good for replay-based prompt tuning.
    Quick,
    /// Test 1 pattern initially, expand if it passes easily (future: diff-based)
    Adaptive,
    /// Test just 1 pattern — always takes the first pattern regardless of
    /// category. Unlike Quick (which selects by category and can return 0 if
    /// no priority categories match), Minimal never returns an empty set.
    Minimal,
}

/// Result of a single test case
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TestCase {
    pub pattern_name: String,
    pub result: ExecutionResult,
    pub generated_code: String,
}

/// Overall test result
#[derive(Debug, Clone)]
pub struct TestResult {
    pub passed: usize,
    pub failed: usize,
    pub test_cases: Vec<TestCase>,
}

impl TestResult {
    pub fn all_passed(&self) -> bool {
        self.failed == 0 && self.passed > 0
    }

    pub fn generate_feedback(&self, language: &Language) -> Option<String> {
        if self.all_passed() {
            return None;
        }

        let passed_patterns: Vec<String> = self
            .test_cases
            .iter()
            .filter(|tc| tc.result.is_pass())
            .map(|tc| format!("- {}", tc.pattern_name))
            .collect();

        let failed_tests: Vec<String> = self
            .test_cases
            .iter()
            .filter(|tc| !tc.result.is_pass())
            .map(|tc| {
                format!(
                    "Pattern: {}\nGenerated test code:\n```{}\n{}\n```\nError: {}",
                    tc.pattern_name,
                    language.as_str(),
                    tc.generated_code,
                    tc.result.error_message(),
                )
            })
            .collect();

        if failed_tests.is_empty() {
            return None;
        }

        let passed_section = if passed_patterns.is_empty() {
            String::new()
        } else {
            format!(
                "PATTERNS THAT PASSED (keep these EXACTLY as-is in the SKILL.md):\n{}\n\n",
                passed_patterns.join("\n")
            )
        };

        Some(format!(
            r#"SKILL.md PATCH REQUIRED — Do NOT regenerate from scratch.

{}PATTERNS THAT FAILED (fix or replace ONLY these):

{}

Instructions:
- Output the COMPLETE SKILL.md with passing patterns UNCHANGED
- For each failing pattern: fix the code example to use the correct API, or replace with a more common alternative
- Do NOT add, remove, or reorder patterns that passed
- Do NOT change imports, pitfalls, references, or other sections unless directly affected by the fix
"#,
            passed_section,
            failed_tests.join("\n\n---\n\n")
        ))
    }
}

/// Main test agent coordinator
pub struct TestCodeValidator<'a> {
    language: Language,
    parser: Box<dyn LanguageParser>,
    code_generator: Box<dyn LanguageCodeGenerator + 'a>,
    executor: Box<dyn LanguageExecutor>,
    #[cfg_attr(not(test), allow(dead_code))]
    execution_mode: ExecutionMode,
    mode: ValidationMode,
    /// Install source from config; when not Registry, local_package is set on code_generator
    install_source: InstallSource,
    /// Timeout for executor operations (stored for Rust concrete path)
    executor_timeout: u64,
    /// Local source path (stored for Rust concrete path)
    local_source: Option<String>,
}

impl<'a> TestCodeValidator<'a> {
    /// Create a new test agent validator for the given language.
    /// Returns an error for unsupported languages.
    pub fn new(
        language: &Language,
        llm_client: &'a dyn LlmClient,
        config: ContainerConfig,
        custom_instructions: Option<String>,
    ) -> anyhow::Result<Self> {
        let install_source = config.install_source;
        let execution_mode = config.execution_mode;
        let timeout = config.timeout;
        // Extract local source path for bare-metal executors
        let local_source = if install_source != InstallSource::Registry {
            config.source_path.clone()
        } else {
            None
        };
        // Container + local-install is only implemented for Python.
        // For other languages, fall back to bare-metal so local_source is used.
        // LocalMount keeps container mode (uses volume mount semantics).
        let execution_mode = if execution_mode == ExecutionMode::Container
            && install_source == InstallSource::LocalInstall
            && !matches!(language, Language::Python)
        {
            tracing::warn!(
                "Container local-install is not yet supported for {} \
                 — falling back to bare metal. \
                 Set `execution_mode = \"bare-metal\"` to suppress this warning.",
                language.as_str()
            );
            ExecutionMode::BareMetal
        } else {
            execution_mode
        };
        match language {
            Language::Python => {
                let executor: Box<dyn LanguageExecutor> = match execution_mode {
                    ExecutionMode::BareMetal => {
                        let mut exe = PythonUvExecutor::new().with_timeout(timeout);
                        if let Some(ref src) = local_source {
                            exe = exe.with_local_source(src.clone());
                        }
                        Box::new(exe)
                    }
                    ExecutionMode::Container => {
                        Box::new(ContainerExecutor::new(config, Language::Python))
                    }
                };
                Ok(Self {
                    language: Language::Python,
                    parser: Box::new(PythonParser),
                    code_generator: Box::new(
                        PythonCodeGenerator::new(llm_client)
                            .with_custom_instructions(custom_instructions),
                    ),
                    executor,
                    execution_mode,
                    mode: ValidationMode::default(),
                    install_source,
                    executor_timeout: timeout,
                    local_source: local_source.clone(),
                })
            }
            Language::Go => {
                let executor: Box<dyn LanguageExecutor> = match execution_mode {
                    ExecutionMode::BareMetal => {
                        let mut exe = GoExecutor::new().with_timeout(timeout);
                        if let Some(ref src) = local_source {
                            exe = exe.with_local_source(src.clone());
                        }
                        Box::new(exe)
                    }
                    ExecutionMode::Container => {
                        Box::new(ContainerExecutor::new(config, Language::Go))
                    }
                };
                Ok(Self {
                    language: Language::Go,
                    parser: Box::new(GoParser),
                    code_generator: Box::new(
                        GoCodeGenerator::new(llm_client)
                            .with_custom_instructions(custom_instructions),
                    ),
                    executor,
                    execution_mode,
                    mode: ValidationMode::default(),
                    install_source,
                    executor_timeout: timeout,
                    local_source: local_source.clone(),
                })
            }
            Language::JavaScript => {
                let executor: Box<dyn LanguageExecutor> = match execution_mode {
                    ExecutionMode::BareMetal => {
                        let mut exe = NodeExecutor::new().with_timeout(timeout);
                        if let Some(ref src) = local_source {
                            exe = exe.with_local_source(src.clone());
                        }
                        Box::new(exe)
                    }
                    ExecutionMode::Container => {
                        Box::new(ContainerExecutor::new(config, Language::JavaScript))
                    }
                };
                Ok(Self {
                    language: Language::JavaScript,
                    parser: Box::new(JsParser),
                    code_generator: Box::new(
                        JsCodeGenerator::new(llm_client)
                            .with_custom_instructions(custom_instructions),
                    ),
                    executor,
                    execution_mode,
                    mode: ValidationMode::default(),
                    install_source,
                    executor_timeout: timeout,
                    local_source: local_source.clone(),
                })
            }
            Language::Rust => {
                let (executor, execution_mode): (Box<dyn LanguageExecutor>, ExecutionMode) =
                    match execution_mode {
                        ExecutionMode::BareMetal => {
                            let mut exe = CargoExecutor::new().with_timeout(timeout);
                            if let Some(ref src) = local_source {
                                exe = exe.with_local_source(src.clone());
                            }
                            (Box::new(exe), ExecutionMode::BareMetal)
                        }
                        ExecutionMode::Container => (
                            Box::new(ContainerExecutor::new(config.clone(), Language::Rust)),
                            ExecutionMode::Container,
                        ),
                    };
                Ok(Self {
                    language: Language::Rust,
                    parser: Box::new(RustParser),
                    code_generator: Box::new(
                        RustCodeGenerator::new(llm_client)
                            .with_custom_instructions(custom_instructions),
                    ),
                    executor,
                    execution_mode,
                    mode: ValidationMode::default(),
                    install_source,
                    executor_timeout: timeout,
                    local_source: local_source.clone(),
                })
            }
            Language::Java => {
                let (executor, execution_mode): (Box<dyn LanguageExecutor>, ExecutionMode) =
                    match execution_mode {
                        ExecutionMode::BareMetal => {
                            // Java needs more time: Maven download + javac + java = 3× timeout.
                            // Floor at 120s to avoid cold-cache Maven timeouts.
                            let mut exe = JavaExecutor::new().with_timeout(timeout.max(120));
                            if let Some(ref src) = local_source {
                                exe = exe.with_local_source(src.clone());
                            }
                            (Box::new(exe), ExecutionMode::BareMetal)
                        }
                        ExecutionMode::Container => (
                            Box::new(ContainerExecutor::new(config.clone(), Language::Java)),
                            ExecutionMode::Container,
                        ),
                    };
                Ok(Self {
                    language: Language::Java,
                    parser: Box::new(JavaParser),
                    code_generator: Box::new(
                        JavaCodeGenerator::new(llm_client)
                            .with_custom_instructions(custom_instructions),
                    ),
                    executor,
                    execution_mode,
                    mode: ValidationMode::default(),
                    install_source,
                    executor_timeout: timeout,
                    local_source: local_source.clone(),
                })
            }
        }
    }

    /// Returns the execution mode used by this validator's executor.
    #[cfg(test)]
    pub fn execution_mode(&self) -> ExecutionMode {
        self.execution_mode
    }

    /// Create a new test agent validator for Python (convenience wrapper).
    /// Used by integration tests in tests/test_agent_integration.rs.
    #[allow(dead_code)]
    pub fn new_python(
        llm_client: &'a dyn LlmClient,
        config: ContainerConfig,
    ) -> anyhow::Result<Self> {
        Self::new(&Language::Python, llm_client, config, None)
    }

    /// Create a new test agent validator for Python with custom instructions (append-only).
    /// Convenience wrapper; integration tests may use this directly.
    #[allow(dead_code)]
    pub fn new_python_with_custom(
        llm_client: &'a dyn LlmClient,
        config: ContainerConfig,
        custom_instructions: Option<String>,
    ) -> anyhow::Result<Self> {
        Self::new(&Language::Python, llm_client, config, custom_instructions)
    }

    /// Set the validation mode
    pub fn with_mode(mut self, mode: ValidationMode) -> Self {
        self.mode = mode;
        self
    }

    /// Select patterns to test based on validation mode
    fn select_patterns<'b>(&self, patterns: &'b [CodePattern]) -> Vec<&'b CodePattern> {
        if patterns.is_empty() {
            return Vec::new();
        }

        match self.mode {
            ValidationMode::Thorough => {
                // Test ALL patterns — every code block in Core Patterns gets
                // compiled and run. This is the mechanical guarantee that no
                // broken examples ship.
                info!("Thorough mode: testing all {} patterns", patterns.len());
                patterns.iter().collect()
            }
            ValidationMode::Quick => {
                // Test up to 3 patterns: prioritize Basic, Config, Error handling.
                // Good for fast iteration during prompt tuning.
                let mut selected = Vec::new();

                for category in [
                    super::parser::PatternCategory::BasicUsage,
                    super::parser::PatternCategory::Configuration,
                    super::parser::PatternCategory::ErrorHandling,
                ] {
                    if let Some(pattern) = patterns.iter().find(|p| p.category == category) {
                        selected.push(pattern);
                        if selected.len() >= 3 {
                            break;
                        }
                    }
                }

                // If we don't have 3 yet, fill with any remaining patterns
                if selected.len() < 3 {
                    for pattern in patterns.iter() {
                        if !selected.contains(&pattern) {
                            selected.push(pattern);
                            if selected.len() >= 3 {
                                break;
                            }
                        }
                    }
                }

                selected
            }
            ValidationMode::Minimal => {
                // Test the first pattern (regardless of category)
                vec![&patterns[0]]
            }
            ValidationMode::Adaptive => {
                // For now, same as Minimal — future: diff-based, only test
                // changed patterns between old and new SKILL.md
                vec![&patterns[0]]
            }
        }
    }

    /// Validate SKILL.md by generating and running test code.
    /// `collected_deps` are structured deps from the source project's manifest
    /// (e.g., Cargo.toml). They supplement whatever the test parser extracts from
    /// the generated SKILL.md, ensuring deps the model omitted still get installed.
    ///
    /// NOTE: `collected_deps` is currently only applied for Rust in BareMetal
    /// execution mode; it is a no-op for other languages/modes until support
    /// is added.
    pub async fn validate(
        &self,
        skill_md: &str,
        collected_deps: &[crate::pipeline::collector::StructuredDep],
    ) -> Result<TestResult> {
        info!(
            "Test agent: starting code generation validation (mode: {:?})",
            self.mode
        );

        // 1. Extract patterns, dependencies, package name, and version
        info!("  → Parsing SKILL.md...");
        let patterns = self.parser.extract_patterns(skill_md)?;
        let deps = self.parser.extract_dependencies(skill_md)?;
        let package_name = self.parser.extract_name(skill_md)?;
        let version = self.parser.extract_version(skill_md)?;

        if let Some(ref name) = package_name {
            info!("  Package name: {}", name);
        }
        if let Some(ref v) = version {
            info!("  Package version: {}", v);
        }

        // For local modes, tell the code generator to exclude the main package from PEP 723 deps
        if self.install_source != InstallSource::Registry {
            if let Some(ref name) = package_name {
                debug!(
                    "  Local mode ({}): excluding \"{}\" from PEP 723 deps",
                    self.install_source, name
                );
                self.code_generator.set_local_package(Some(name.clone()));
            }
        }
        info!(
            "  Found {} patterns and {} dependencies",
            patterns.len(),
            deps.len()
        );
        debug!("  Dependencies: {:?}", deps);

        if patterns.is_empty() {
            warn!("  No patterns found in SKILL.md, skipping validation");
            return Ok(TestResult {
                passed: 0,
                failed: 0,
                test_cases: Vec::new(),
            });
        }

        // 2. Select patterns based on mode
        let selected_patterns = self.select_patterns(&patterns);
        info!("  Selected {} patterns to test", selected_patterns.len());

        for pattern in &selected_patterns {
            debug!(
                "    - {} ({})",
                pattern.name,
                format!("{:?}", pattern.category)
            );
        }

        // 3. Setup environment once (reuse for all tests)
        info!("  → Setting up {} environment...", self.language.as_str());
        let env = if matches!(self.language, Language::Rust)
            && self.execution_mode == ExecutionMode::BareMetal
        {
            // Rust bare-metal: use structured deps for lossless Cargo.toml
            // (preserves exact versions, features, git refs from the source project).
            // Container mode uses self.executor which handles Rust via Cargo.toml
            // generation with wildcard deps — structured specs are not carried through
            // the container path yet.
            let rust_parser = super::rust_parser::RustParser;
            let mut structured_deps = rust_parser.extract_structured_dependencies(skill_md)?;
            // Enrich with collected deps from the source manifest.
            // Collected deps are authoritative (have raw TOML specs with versions/features);
            // parser deps may be name-only patterns. Upgrade name-only deps with manifest
            // specs, or add entirely new deps.
            let before = structured_deps.len();
            for cd in collected_deps {
                let norm = cd.name.replace('-', "_");
                match structured_deps
                    .iter_mut()
                    .find(|d| d.name.replace('-', "_") == norm)
                {
                    Some(existing) if existing.raw_spec.is_none() && cd.raw_spec.is_some() => {
                        *existing = cd.clone();
                    }
                    Some(_) => {}
                    None => structured_deps.push(cd.clone()),
                }
            }
            if structured_deps.len() > before {
                info!(
                    "  Enriched deps: {} from SKILL.md + {} from source manifest",
                    before,
                    structured_deps.len() - before
                );
            }
            debug!(
                "  Structured deps: {} total, {} with specs",
                structured_deps.len(),
                structured_deps
                    .iter()
                    .filter(|d| d.raw_spec.is_some())
                    .count()
            );
            let mut cargo_exec =
                super::executor::CargoExecutor::new().with_timeout(self.executor_timeout);
            if let Some(ref src) = self.local_source {
                cargo_exec = cargo_exec.with_local_source(src.clone());
            }
            cargo_exec
                .setup_structured_environment(&structured_deps)
                .await?
        } else {
            self.executor.setup_environment(&deps).await?
        };

        let mut test_cases = Vec::new();

        // 4. Generate and run tests for each pattern (with one test-code retry)
        for pattern in selected_patterns {
            info!("  → Testing pattern: {}", pattern.name);

            // Generate test code using LLM
            let test_code = match self.code_generator.generate_test_code(pattern).await {
                Ok(code) => code,
                Err(e) => {
                    warn!("    Failed to generate test code: {}", e);
                    test_cases.push(TestCase {
                        pattern_name: pattern.name.clone(),
                        result: ExecutionResult::Fail(format!("Code generation failed: {}", e)),
                        generated_code: String::new(),
                    });
                    continue;
                }
            };

            debug!("    Generated {} bytes of test code", test_code.len());

            // Execute the generated code
            let result = match self.executor.run_code(&env, &test_code).await {
                Ok(r) => r,
                Err(e) => {
                    warn!("    Execution error: {}", e);
                    self.executor.cleanup(&env).await.ok();
                    return Err(e);
                }
            };

            // If test failed, retry test code generation once with the error context.
            // This fixes the test code (not the SKILL.md pattern) — the most common failure
            // mode is the LLM writing code that doesn't compile, not a bad pattern.
            let (final_result, final_code) = match &result {
                ExecutionResult::Fail(error) => {
                    info!("    ✗ Test failed, retrying test code with error context...");
                    match self
                        .code_generator
                        .retry_test_code(pattern, &test_code, error)
                        .await
                    {
                        Ok(retry_code) => {
                            match self.executor.run_code(&env, &retry_code).await {
                                Ok(retry_result) => (retry_result, retry_code),
                                Err(e) => {
                                    warn!("    Retry execution error: {}", e);
                                    // Fall through with original failure
                                    (result, test_code)
                                }
                            }
                        }
                        Err(e) => {
                            warn!("    Retry code generation failed: {}", e);
                            (result, test_code)
                        }
                    }
                }
                _ => (result, test_code),
            };

            match &final_result {
                ExecutionResult::Pass(output) => {
                    info!("    ✓ Test passed");
                    debug!(
                        "    Output: {}",
                        output.lines().next().unwrap_or("(no output)")
                    );
                }
                ExecutionResult::Fail(error) => {
                    // Show first 5 lines of error at warn level for diagnosis
                    let summary: String = error.lines().take(5).collect::<Vec<_>>().join("\n");
                    warn!("    ✗ Test failed (after retry):\n{}", summary);
                    debug!("    Full error:\n{}", error);
                }
                ExecutionResult::Timeout => {
                    warn!("    ⏱  Test timed out");
                }
            }

            test_cases.push(TestCase {
                pattern_name: pattern.name.clone(),
                result: final_result,
                generated_code: final_code,
            });
        }

        // 5. Cleanup
        self.executor.cleanup(&env).await?;

        // 6. Analyze results
        let passed = test_cases.iter().filter(|tc| tc.result.is_pass()).count();
        let failed = test_cases.len() - passed;

        if failed == 0 {
            info!("  ✓ All {} tests passed!", passed);
        } else {
            let failed_names: Vec<&str> = test_cases
                .iter()
                .filter(|tc| !tc.result.is_pass())
                .map(|tc| tc.pattern_name.as_str())
                .collect();
            warn!(
                "  ✗ {} tests passed, {} failed: {}",
                passed,
                failed,
                failed_names.join(", ")
            );
        }

        Ok(TestResult {
            passed,
            failed,
            test_cases,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::parser::PatternCategory;
    use super::*;

    #[test]
    fn test_select_patterns_minimal() {
        let patterns = [
            CodePattern {
                name: "Basic".to_string(),
                description: "".to_string(),
                code: "".to_string(),
                category: PatternCategory::BasicUsage,
            },
            CodePattern {
                name: "Config".to_string(),
                description: "".to_string(),
                code: "".to_string(),
                category: PatternCategory::Configuration,
            },
        ];

        // Create a mock validator (can't easily test without real LLM client)
        // Just test the pattern selection logic separately
        let mode = ValidationMode::Minimal;
        let selected_count = match mode {
            ValidationMode::Minimal => 1,
            ValidationMode::Quick => 3.min(patterns.len()),
            ValidationMode::Thorough => patterns.len(),
            ValidationMode::Adaptive => 1,
        };

        assert_eq!(selected_count, 1);
    }

    #[test]
    fn test_test_result_all_passed() {
        let result = TestResult {
            passed: 3,
            failed: 0,
            test_cases: vec![],
        };

        assert!(result.all_passed());
        assert!(result.generate_feedback(&Language::Python).is_none());
    }

    #[test]
    fn test_test_result_with_failures() {
        let result = TestResult {
            passed: 1,
            failed: 1,
            test_cases: vec![
                TestCase {
                    pattern_name: "Test 1".to_string(),
                    result: ExecutionResult::Pass("ok".to_string()),
                    generated_code: "print('ok')".to_string(),
                },
                TestCase {
                    pattern_name: "Test 2".to_string(),
                    result: ExecutionResult::Fail("error".to_string()),
                    generated_code: "raise Error()".to_string(),
                },
            ],
        };

        assert!(!result.all_passed());
        let feedback = result.generate_feedback(&Language::Python).unwrap();
        assert!(feedback.contains("Test 2"));
        assert!(feedback.contains("error"));
    }

    #[test]
    fn test_select_patterns_adaptive_mode() {
        // Adaptive mode currently behaves like Minimal (1 pattern).
        // Verify it returns a subset, not all patterns.
        let patterns = [
            CodePattern {
                name: "Basic".to_string(),
                description: "".to_string(),
                code: "".to_string(),
                category: PatternCategory::BasicUsage,
            },
            CodePattern {
                name: "Config".to_string(),
                description: "".to_string(),
                code: "".to_string(),
                category: PatternCategory::Configuration,
            },
            CodePattern {
                name: "Error".to_string(),
                description: "".to_string(),
                code: "".to_string(),
                category: PatternCategory::ErrorHandling,
            },
            CodePattern {
                name: "Async".to_string(),
                description: "".to_string(),
                code: "".to_string(),
                category: PatternCategory::AsyncPattern,
            },
        ];

        // Simulate Adaptive selection logic (mirrors select_patterns)
        let mode = ValidationMode::Adaptive;
        let selected_count = match mode {
            ValidationMode::Minimal => 1,
            ValidationMode::Quick => 3.min(patterns.len()),
            ValidationMode::Thorough => patterns.len(),
            ValidationMode::Adaptive => 1,
        };

        assert!(
            selected_count < patterns.len(),
            "Adaptive should not return all patterns"
        );
    }

    #[test]
    fn test_generate_feedback_all_passed() {
        let result = TestResult {
            passed: 3,
            failed: 0,
            test_cases: vec![
                TestCase {
                    pattern_name: "Basic".to_string(),
                    result: ExecutionResult::Pass("ok".to_string()),
                    generated_code: "print('ok')".to_string(),
                },
                TestCase {
                    pattern_name: "Config".to_string(),
                    result: ExecutionResult::Pass("ok".to_string()),
                    generated_code: "print('config')".to_string(),
                },
                TestCase {
                    pattern_name: "Error".to_string(),
                    result: ExecutionResult::Pass("ok".to_string()),
                    generated_code: "print('error')".to_string(),
                },
            ],
        };

        // All tests passed, so no feedback should be generated
        assert!(result.generate_feedback(&Language::Python).is_none());
    }

    #[test]
    fn test_test_result_some_failed() {
        let result = TestResult {
            passed: 2,
            failed: 1,
            test_cases: vec![],
        };

        assert!(!result.all_passed());
    }

    // --- Mock implementations for trait-based testing ---

    use super::super::{ExecutionEnv, LanguageCodeGenerator, LanguageExecutor, LanguageParser};
    use std::sync::{Arc, Mutex};

    struct MockParser {
        patterns: Vec<CodePattern>,
        deps: Vec<String>,
        name: Option<String>,
        version: Option<String>,
    }

    impl MockParser {
        fn new(patterns: Vec<CodePattern>) -> Self {
            Self {
                patterns,
                deps: vec!["some-dep".to_string()],
                name: Some("mock-pkg".to_string()),
                version: Some("1.0.0".to_string()),
            }
        }
    }

    impl LanguageParser for MockParser {
        fn extract_patterns(&self, _skill_md: &str) -> Result<Vec<CodePattern>> {
            Ok(self.patterns.clone())
        }
        fn extract_dependencies(&self, _skill_md: &str) -> Result<Vec<String>> {
            Ok(self.deps.clone())
        }
        fn extract_version(&self, _skill_md: &str) -> Result<Option<String>> {
            Ok(self.version.clone())
        }
        fn extract_name(&self, _skill_md: &str) -> Result<Option<String>> {
            Ok(self.name.clone())
        }
    }

    struct MockCodeGenerator {
        result: Mutex<std::result::Result<String, String>>,
        local_package: Arc<Mutex<Option<String>>>,
    }

    impl MockCodeGenerator {
        fn succeeding(code: &str) -> Self {
            Self {
                result: Mutex::new(Ok(code.to_string())),
                local_package: Arc::new(Mutex::new(None)),
            }
        }

        fn failing(msg: &str) -> Self {
            Self {
                result: Mutex::new(Err(msg.to_string())),
                local_package: Arc::new(Mutex::new(None)),
            }
        }

        fn local_package_handle(&self) -> Arc<Mutex<Option<String>>> {
            Arc::clone(&self.local_package)
        }
    }

    #[async_trait::async_trait]
    impl LanguageCodeGenerator for MockCodeGenerator {
        async fn generate_test_code(&self, _pattern: &CodePattern) -> Result<String> {
            let guard = self.result.lock().unwrap();
            match &*guard {
                Ok(code) => Ok(code.clone()),
                Err(msg) => Err(anyhow::anyhow!("{}", msg)),
            }
        }

        fn set_local_package(&self, package: Option<String>) {
            let mut guard = self.local_package.lock().unwrap();
            *guard = package;
        }
    }

    struct MockExecutor {
        run_result: Mutex<std::result::Result<ExecutionResult, String>>,
    }

    impl MockExecutor {
        fn passing(output: &str) -> Self {
            Self {
                run_result: Mutex::new(Ok(ExecutionResult::Pass(output.to_string()))),
            }
        }

        fn failing_execution(error: &str) -> Self {
            Self {
                run_result: Mutex::new(Ok(ExecutionResult::Fail(error.to_string()))),
            }
        }

        fn erroring(msg: &str) -> Self {
            Self {
                run_result: Mutex::new(Err(msg.to_string())),
            }
        }

        fn timing_out() -> Self {
            Self {
                run_result: Mutex::new(Ok(ExecutionResult::Timeout)),
            }
        }
    }

    #[async_trait::async_trait]
    impl LanguageExecutor for MockExecutor {
        async fn setup_environment(&self, _deps: &[String]) -> Result<ExecutionEnv> {
            let temp_dir = tempfile::TempDir::new()?;
            Ok(ExecutionEnv {
                temp_dir,
                interpreter_path: None,
                container_name: None,
                dependencies: vec![],
            })
        }

        async fn run_code(&self, _env: &ExecutionEnv, _code: &str) -> Result<ExecutionResult> {
            let guard = self.run_result.lock().unwrap();
            match &*guard {
                Ok(r) => Ok(r.clone()),
                Err(msg) => Err(anyhow::anyhow!("{}", msg)),
            }
        }

        async fn cleanup(&self, _env: &ExecutionEnv) -> Result<()> {
            Ok(())
        }
    }

    fn make_validator<'a>(
        parser: Box<dyn LanguageParser>,
        code_generator: Box<dyn LanguageCodeGenerator + 'a>,
        executor: Box<dyn LanguageExecutor>,
        mode: ValidationMode,
        install_source: InstallSource,
    ) -> TestCodeValidator<'a> {
        TestCodeValidator {
            language: Language::Python,
            parser,
            code_generator,
            executor,
            execution_mode: ExecutionMode::Container,
            mode,
            install_source,
            executor_timeout: 120,
            local_source: None,
        }
    }

    fn basic_pattern() -> CodePattern {
        CodePattern {
            name: "Basic Usage".to_string(),
            description: "Basic usage example".to_string(),
            code: "import foo\nfoo.bar()".to_string(),
            category: PatternCategory::BasicUsage,
        }
    }

    fn config_pattern() -> CodePattern {
        CodePattern {
            name: "Configuration".to_string(),
            description: "Configuration example".to_string(),
            code: "import foo\nfoo.config()".to_string(),
            category: PatternCategory::Configuration,
        }
    }

    fn error_pattern() -> CodePattern {
        CodePattern {
            name: "Error Handling".to_string(),
            description: "Error handling example".to_string(),
            code: "try:\n  foo.bar()\nexcept: pass".to_string(),
            category: PatternCategory::ErrorHandling,
        }
    }

    fn async_pattern() -> CodePattern {
        CodePattern {
            name: "Async Usage".to_string(),
            description: "Async pattern".to_string(),
            code: "await foo.bar()".to_string(),
            category: PatternCategory::AsyncPattern,
        }
    }

    fn other_pattern(name: &str) -> CodePattern {
        CodePattern {
            name: name.to_string(),
            description: "Other pattern".to_string(),
            code: "foo.other()".to_string(),
            category: PatternCategory::Other,
        }
    }

    // --- select_patterns tests using real validator ---

    #[test]
    fn test_select_patterns_empty_patterns() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            InstallSource::Registry,
        );
        let patterns: Vec<CodePattern> = vec![];
        let selected = validator.select_patterns(&patterns);
        assert!(selected.is_empty());
    }

    #[test]
    fn test_select_patterns_minimal_returns_first() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Minimal,
            InstallSource::Registry,
        );
        let patterns = vec![config_pattern(), basic_pattern()];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "Configuration");
    }

    #[test]
    fn test_select_patterns_adaptive_returns_first() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Adaptive,
            InstallSource::Registry,
        );
        let patterns = vec![error_pattern(), basic_pattern()];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "Error Handling");
    }

    #[test]
    fn test_select_patterns_thorough_exercises_info_log() {
        // Activate tracing so the info!() format args are evaluated by llvm-cov.
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();

        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            InstallSource::Registry,
        );
        let patterns = vec![basic_pattern(), config_pattern(), error_pattern()];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn test_select_patterns_quick_exercises_selection_logic() {
        // Activate tracing so all code paths within Quick mode are evaluated.
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();

        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Quick,
            InstallSource::Registry,
        );
        let patterns = vec![
            basic_pattern(),
            config_pattern(),
            error_pattern(),
            async_pattern(),
        ];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 3);
        // Should prioritize BasicUsage, Configuration, ErrorHandling
        let names: Vec<&str> = selected.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Basic Usage"));
        assert!(names.contains(&"Configuration"));
        assert!(names.contains(&"Error Handling"));
    }

    #[test]
    fn test_select_patterns_thorough_returns_all() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            InstallSource::Registry,
        );
        let patterns = vec![
            async_pattern(),
            error_pattern(),
            config_pattern(),
            basic_pattern(),
            other_pattern("Extra"),
        ];
        let selected = validator.select_patterns(&patterns);
        // Thorough mode tests ALL patterns
        assert_eq!(selected.len(), 5);
    }

    #[test]
    fn test_select_patterns_thorough_single_pattern() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            InstallSource::Registry,
        );
        let patterns = vec![basic_pattern()];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn test_select_patterns_quick_prioritizes_categories() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Quick,
            InstallSource::Registry,
        );
        let patterns = vec![
            async_pattern(),
            error_pattern(),
            config_pattern(),
            basic_pattern(),
            other_pattern("Extra"),
        ];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 3);
        let names: Vec<&str> = selected.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Basic Usage"));
        assert!(names.contains(&"Configuration"));
        assert!(names.contains(&"Error Handling"));
    }

    #[test]
    fn test_select_patterns_quick_fills_remaining() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Quick,
            InstallSource::Registry,
        );
        let patterns = vec![
            basic_pattern(),
            other_pattern("Extra1"),
            other_pattern("Extra2"),
            other_pattern("Extra3"),
        ];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].name, "Basic Usage");
    }

    #[test]
    fn test_select_patterns_quick_caps_at_3() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Quick,
            InstallSource::Registry,
        );
        let patterns = vec![
            basic_pattern(),
            config_pattern(),
            error_pattern(),
            async_pattern(),
            other_pattern("Extra"),
        ];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn test_select_patterns_quick_no_priority_categories() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Quick,
            InstallSource::Registry,
        );
        let patterns = vec![
            async_pattern(),
            other_pattern("A"),
            other_pattern("B"),
            other_pattern("C"),
        ];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].name, "Async Usage");
        assert_eq!(selected[1].name, "A");
        assert_eq!(selected[2].name, "B");
    }

    // --- validate() tests ---

    #[tokio::test]
    async fn test_validate_no_patterns_returns_empty_result() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("code")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Thorough,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 0);
        assert!(result.test_cases.is_empty());
        // Empty result: passed=0, failed=0 -> all_passed() should be false
        assert!(!result.all_passed());
    }

    #[tokio::test]
    async fn test_validate_all_pass() {
        let patterns = vec![basic_pattern(), config_pattern()];
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("print('ok')")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Thorough,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        assert_eq!(result.passed, 2);
        assert_eq!(result.failed, 0);
        assert!(result.all_passed());
        assert_eq!(result.test_cases.len(), 2);
        assert_eq!(result.test_cases[0].pattern_name, "Basic Usage");
        assert_eq!(result.test_cases[0].generated_code, "print('ok')");
    }

    #[tokio::test]
    async fn test_validate_code_generator_fails() {
        let patterns = vec![basic_pattern()];
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::failing("LLM unavailable")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 1);
        assert!(!result.all_passed());
        // The test case should have empty generated_code and Fail result
        let tc = &result.test_cases[0];
        assert!(tc.generated_code.is_empty());
        assert!(tc.result.is_fail());
        assert!(tc.result.error_message().contains("Code generation failed"));
    }

    #[tokio::test]
    async fn test_validate_executor_run_error_returns_err() {
        let patterns = vec![basic_pattern()];
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("print('ok')")),
            Box::new(MockExecutor::erroring("container crashed")),
            ValidationMode::Minimal,
            InstallSource::Registry,
        );
        let err = validator.validate("# SKILL.md", &[]).await.unwrap_err();
        assert!(err.to_string().contains("container crashed"));
    }

    #[tokio::test]
    async fn test_validate_executor_returns_fail_result() {
        let patterns = vec![basic_pattern()];
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("print('ok')")),
            Box::new(MockExecutor::failing_execution(
                "ImportError: no module named foo",
            )),
            ValidationMode::Minimal,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 1);
        assert!(result.test_cases[0].result.is_fail());
    }

    #[tokio::test]
    async fn test_validate_executor_returns_timeout() {
        let patterns = vec![basic_pattern()];
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("while True: pass")),
            Box::new(MockExecutor::timing_out()),
            ValidationMode::Minimal,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        // Timeout is not is_pass(), so it counts as failed
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 1);
    }

    #[tokio::test]
    async fn test_validate_local_install_source_sets_local_package() {
        let patterns = vec![basic_pattern()];
        let code_gen = MockCodeGenerator::succeeding("print('ok')");
        let lp_handle = code_gen.local_package_handle();
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(code_gen),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            InstallSource::LocalInstall, // non-registry
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        assert!(result.all_passed());
        // Verify set_local_package was actually called
        let lp = lp_handle.lock().unwrap();
        assert!(
            lp.is_some(),
            "set_local_package should have been called for non-registry source"
        );
    }

    #[tokio::test]
    async fn test_validate_with_mode() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![basic_pattern()])),
            Box::new(MockCodeGenerator::succeeding("code")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Thorough,
            InstallSource::Registry,
        );
        let validator = validator.with_mode(ValidationMode::Minimal);
        assert_eq!(validator.mode, ValidationMode::Minimal);
    }

    #[tokio::test]
    async fn test_validate_registry_does_not_set_local_package() {
        let patterns = vec![basic_pattern()];
        let code_gen = MockCodeGenerator::succeeding("print('ok')");
        let lp_handle = code_gen.local_package_handle();
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(code_gen),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        assert!(result.all_passed());
        // Verify set_local_package was NOT called for registry source
        let lp = lp_handle.lock().unwrap();
        assert!(
            lp.is_none(),
            "set_local_package should not be called for registry source"
        );
    }

    // --- TestResult edge case tests ---

    #[test]
    fn test_all_passed_with_zero_passed_and_zero_failed() {
        let result = TestResult {
            passed: 0,
            failed: 0,
            test_cases: vec![],
        };
        // passed=0 means all_passed() is false (need at least 1 pass)
        assert!(!result.all_passed());
    }

    #[test]
    fn test_generate_feedback_timeout_is_actionable() {
        // Timeouts should be treated as failures so the patch loop can act on them.
        let result = TestResult {
            passed: 0,
            failed: 1,
            test_cases: vec![TestCase {
                pattern_name: "Slow Pattern".to_string(),
                result: ExecutionResult::Timeout,
                generated_code: "while True: pass".to_string(),
            }],
        };
        assert!(!result.all_passed());
        let feedback = result.generate_feedback(&Language::Python).unwrap();
        assert!(feedback.contains("Slow Pattern"));
        assert!(feedback.contains("timed out"));
    }

    #[test]
    fn test_generate_feedback_all_failed_no_passed_section() {
        let result = TestResult {
            passed: 0,
            failed: 2,
            test_cases: vec![
                TestCase {
                    pattern_name: "Pattern A".to_string(),
                    result: ExecutionResult::Fail("error A".to_string()),
                    generated_code: "code_a()".to_string(),
                },
                TestCase {
                    pattern_name: "Pattern B".to_string(),
                    result: ExecutionResult::Fail("error B".to_string()),
                    generated_code: "code_b()".to_string(),
                },
            ],
        };
        let feedback = result.generate_feedback(&Language::Python).unwrap();
        // No passed patterns, so the "PATTERNS THAT PASSED" section should be absent
        assert!(!feedback.contains("PATTERNS THAT PASSED"));
        assert!(feedback.contains("PATTERNS THAT FAILED"));
        assert!(feedback.contains("Pattern A"));
        assert!(feedback.contains("Pattern B"));
        assert!(feedback.contains("error A"));
        assert!(feedback.contains("error B"));
        assert!(feedback.contains("SKILL.md PATCH REQUIRED"));
    }

    #[test]
    fn test_generate_feedback_mixed_has_passed_section() {
        let result = TestResult {
            passed: 1,
            failed: 1,
            test_cases: vec![
                TestCase {
                    pattern_name: "Good Pattern".to_string(),
                    result: ExecutionResult::Pass("ok".to_string()),
                    generated_code: "good()".to_string(),
                },
                TestCase {
                    pattern_name: "Bad Pattern".to_string(),
                    result: ExecutionResult::Fail("broken".to_string()),
                    generated_code: "bad()".to_string(),
                },
            ],
        };
        let feedback = result.generate_feedback(&Language::Python).unwrap();
        assert!(feedback.contains("PATTERNS THAT PASSED"));
        assert!(feedback.contains("Good Pattern"));
        assert!(feedback.contains("PATTERNS THAT FAILED"));
        assert!(feedback.contains("Bad Pattern"));
        assert!(feedback.contains("broken"));
        // Verify the passed pattern appears in the keep-as-is section
        assert!(feedback.contains("keep these EXACTLY as-is"));
    }

    #[test]
    fn test_generate_feedback_contains_code_block() {
        let result = TestResult {
            passed: 0,
            failed: 1,
            test_cases: vec![TestCase {
                pattern_name: "Broken".to_string(),
                result: ExecutionResult::Fail("SyntaxError".to_string()),
                generated_code: "def broken(:\n  pass".to_string(),
            }],
        };
        let feedback = result.generate_feedback(&Language::Python).unwrap();
        assert!(feedback.contains("```python"));
        assert!(feedback.contains("def broken(:\n  pass"));
        assert!(feedback.contains("```"));
    }

    // --- ValidationMode tests ---

    #[test]
    fn test_validation_mode_default_is_thorough() {
        assert_eq!(ValidationMode::default(), ValidationMode::Thorough);
    }

    // --- TestCase construction ---

    #[test]
    fn test_test_case_fields_accessible() {
        let tc = TestCase {
            pattern_name: "My Pattern".to_string(),
            result: ExecutionResult::Pass("output".to_string()),
            generated_code: "print('hello')".to_string(),
        };
        assert_eq!(tc.pattern_name, "My Pattern");
        assert!(tc.result.is_pass());
        assert_eq!(tc.generated_code, "print('hello')");
    }

    // --- Parser returning None for name/version ---

    #[tokio::test]
    async fn test_validate_with_no_package_name() {
        let mut parser = MockParser::new(vec![basic_pattern()]);
        parser.name = None;
        parser.version = None;
        let validator = make_validator(
            Box::new(parser),
            Box::new(MockCodeGenerator::succeeding("code")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            InstallSource::LocalInstall,
        );
        // When name is None, local package path is skipped
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        assert!(result.all_passed());
    }

    #[tokio::test]
    async fn test_validate_multiple_patterns_mixed_results() {
        // Use a code generator that always succeeds, but executor returns Fail
        // This tests the loop over selected_patterns and result counting
        let patterns = vec![basic_pattern(), config_pattern(), error_pattern()];
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("code")),
            Box::new(MockExecutor::failing_execution("import error")),
            ValidationMode::Thorough,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 3);
        assert_eq!(result.test_cases.len(), 3);
    }

    #[tokio::test]
    async fn test_validate_failed_names_collected_for_failed_patterns() {
        // Exercises the `failed_names` collection path in the test summary:
        // when tests fail, the warn! branch collects their names.
        let patterns = vec![basic_pattern(), config_pattern()];
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("code")),
            Box::new(MockExecutor::failing_execution("module not found")),
            ValidationMode::Thorough,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        assert_eq!(result.failed, 2);
        // Verify the pattern names are preserved in the failed test cases —
        // these are the same names that feed into the warn! `failed_names` output.
        let failed_names: Vec<&str> = result
            .test_cases
            .iter()
            .filter(|tc| !tc.result.is_pass())
            .map(|tc| tc.pattern_name.as_str())
            .collect();
        assert!(
            failed_names.contains(&"Basic Usage"),
            "expected Basic Usage in failed names"
        );
        assert!(
            failed_names.contains(&"Configuration"),
            "expected Configuration in failed names"
        );
    }

    #[test]
    fn test_new_go_validator_constructs() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig::default();
        let validator = TestCodeValidator::new(&Language::Go, &client, config, None);
        assert!(
            validator.is_ok(),
            "Go validator should construct successfully"
        );
    }

    #[test]
    fn test_new_go_validator_with_custom_instructions() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig::default();
        let validator = TestCodeValidator::new(
            &Language::Go,
            &client,
            config,
            Some("Use table-driven tests".to_string()),
        );
        assert!(validator.is_ok());
    }

    #[test]
    fn test_new_go_baremetal_uses_go_executor() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::BareMetal,
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Go, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
    }

    #[test]
    fn test_new_go_container_uses_container_executor() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Go, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::Container);
    }

    #[test]
    fn test_new_javascript_validator_constructs() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig::default();
        let validator = TestCodeValidator::new(&Language::JavaScript, &client, config, None);
        assert!(
            validator.is_ok(),
            "JavaScript validator should construct successfully"
        );
    }

    #[test]
    fn test_new_java_validator_bare_metal() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig::default();
        let validator = TestCodeValidator::new(&Language::Java, &client, config, None);
        assert!(
            validator.is_ok(),
            "Java validator should construct successfully"
        );
    }

    #[test]
    fn test_new_java_validator_container_mode() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let mut config = ContainerConfig::default();
        config.execution_mode = crate::config::ExecutionMode::Container;
        let validator = TestCodeValidator::new(&Language::Java, &client, config, None);
        assert!(
            validator.is_ok(),
            "Java container validator should construct successfully"
        );
    }

    #[test]
    fn test_new_javascript_validator_with_custom_instructions() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig::default();
        let validator = TestCodeValidator::new(
            &Language::JavaScript,
            &client,
            config,
            Some("Use async/await patterns".to_string()),
        );
        assert!(validator.is_ok());
    }

    #[test]
    fn test_new_javascript_baremetal_uses_node_executor() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::BareMetal,
            ..Default::default()
        };
        let validator =
            TestCodeValidator::new(&Language::JavaScript, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
    }

    #[test]
    fn test_new_javascript_container_uses_container_executor() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            ..Default::default()
        };
        let validator =
            TestCodeValidator::new(&Language::JavaScript, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::Container);
    }

    #[test]
    fn test_new_rust_creates_validator() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig::default();
        let result = TestCodeValidator::new(&Language::Rust, &client, config, None);
        assert!(result.is_ok(), "Rust validator should be constructable");
    }

    #[test]
    fn test_new_rust_container_stays_container() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            ..ContainerConfig::default()
        };
        let validator = TestCodeValidator::new(&Language::Rust, &client, config, None).unwrap();
        // Rust container mode is now supported (Cargo.toml + cargo run)
        assert_eq!(validator.execution_mode, ExecutionMode::Container);
    }

    #[test]
    fn test_generate_feedback_go_language() {
        let result = TestResult {
            passed: 1,
            failed: 1,
            test_cases: vec![
                TestCase {
                    pattern_name: "Basic".to_string(),
                    result: ExecutionResult::Pass("ok".to_string()),
                    generated_code: "fmt.Println(\"ok\")".to_string(),
                },
                TestCase {
                    pattern_name: "Config".to_string(),
                    result: ExecutionResult::Fail("undefined: viper".to_string()),
                    generated_code: "viper.Get(\"key\")".to_string(),
                },
            ],
        };

        let feedback = result.generate_feedback(&Language::Go).unwrap();
        assert!(
            feedback.contains("go"),
            "Go feedback should reference go language"
        );
        assert!(feedback.contains("Config"));
    }

    #[test]
    fn test_generate_feedback_javascript_language() {
        let result = TestResult {
            passed: 1,
            failed: 1,
            test_cases: vec![
                TestCase {
                    pattern_name: "Basic".to_string(),
                    result: ExecutionResult::Pass("ok".to_string()),
                    generated_code: "console.log(\"ok\")".to_string(),
                },
                TestCase {
                    pattern_name: "Middleware".to_string(),
                    result: ExecutionResult::Fail(
                        "ReferenceError: express is not defined".to_string(),
                    ),
                    generated_code: "app.use(express.json())".to_string(),
                },
            ],
        };

        let feedback = result.generate_feedback(&Language::JavaScript).unwrap();
        assert!(
            feedback.contains("javascript"),
            "JS feedback should reference javascript language"
        );
        assert!(feedback.contains("Middleware"));
    }

    // --- Stateful mocks for retry path coverage ---

    /// Executor that returns Fail on the first run_code, then Err on the second.
    /// Covers the retry-execution-error path (line 402-405).
    struct FailThenErrorExecutor {
        call_count: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FailThenErrorExecutor {
        fn new() -> (Self, Arc<std::sync::atomic::AtomicUsize>) {
            let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            (
                Self {
                    call_count: Arc::clone(&counter),
                },
                counter,
            )
        }
    }

    #[async_trait::async_trait]
    impl LanguageExecutor for FailThenErrorExecutor {
        async fn setup_environment(&self, _deps: &[String]) -> Result<ExecutionEnv> {
            let temp_dir = tempfile::TempDir::new()?;
            Ok(ExecutionEnv {
                temp_dir,
                interpreter_path: None,
                container_name: None,
                dependencies: vec![],
            })
        }

        async fn run_code(&self, _env: &ExecutionEnv, _code: &str) -> Result<ExecutionResult> {
            let n = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 0 {
                Ok(ExecutionResult::Fail("first run failed".to_string()))
            } else {
                Err(anyhow::anyhow!("container crashed on retry"))
            }
        }

        async fn cleanup(&self, _env: &ExecutionEnv) -> Result<()> {
            Ok(())
        }
    }

    /// Code generator that succeeds on generate_test_code but fails on retry_test_code.
    /// Covers the retry-code-generation-failure path (lines 409-411).
    struct RetryFailingCodeGenerator {
        retry_count: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl RetryFailingCodeGenerator {
        fn new() -> (Self, Arc<std::sync::atomic::AtomicUsize>) {
            let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            (
                Self {
                    retry_count: Arc::clone(&counter),
                },
                counter,
            )
        }
    }

    #[async_trait::async_trait]
    impl LanguageCodeGenerator for RetryFailingCodeGenerator {
        async fn generate_test_code(&self, _pattern: &CodePattern) -> Result<String> {
            Ok("print('initial')".to_string())
        }

        async fn retry_test_code(
            &self,
            _pattern: &CodePattern,
            _previous_code: &str,
            _error_output: &str,
        ) -> Result<String> {
            self.retry_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(anyhow::anyhow!("retry generation failed"))
        }
    }

    #[tokio::test]
    async fn test_validate_retry_execution_error_falls_through() {
        let patterns = vec![basic_pattern()];
        let (executor, call_count) = FailThenErrorExecutor::new();
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("print('ok')")),
            Box::new(executor),
            ValidationMode::Minimal,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        // First run_code returns Fail, retry run_code returns Err → falls through
        // with the original Fail result
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 1);
        assert!(result.test_cases[0].result.is_fail());
        assert_eq!(
            call_count.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "executor should be called twice (initial + retry)"
        );
        assert!(
            result.test_cases[0]
                .result
                .error_message()
                .contains("first run failed"),
            "should preserve original failure message"
        );
    }

    #[tokio::test]
    async fn test_validate_retry_code_generation_fails() {
        let patterns = vec![basic_pattern()];
        let (code_gen, retry_count) = RetryFailingCodeGenerator::new();
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(code_gen),
            Box::new(MockExecutor::failing_execution("import error")),
            ValidationMode::Minimal,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        // generate_test_code succeeds, run_code fails, retry_test_code fails
        // → falls through with original Fail result
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 1);
        assert!(result.test_cases[0].result.is_fail());
        assert_eq!(
            retry_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "retry_test_code should be called once"
        );
    }

    // --- Convenience wrapper coverage ---

    #[test]
    fn test_new_python_convenience_wrapper() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig::default();
        let validator = TestCodeValidator::new_python(&client, config);
        assert!(validator.is_ok());
    }

    #[test]
    fn test_new_python_with_custom_convenience_wrapper() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig::default();
        let validator = TestCodeValidator::new_python_with_custom(
            &client,
            config,
            Some("Use pytest style".to_string()),
        );
        assert!(validator.is_ok());
    }

    // --- generate_feedback edge case: failed > 0 but all test_cases pass ---

    #[test]
    fn test_generate_feedback_no_failing_test_cases_returns_none() {
        // Contradictory state: failed=1 in counts but all test_cases show Pass.
        // The filter produces empty failed_tests, hitting the early return at line 83.
        let result = TestResult {
            passed: 1,
            failed: 1,
            test_cases: vec![TestCase {
                pattern_name: "Only Pass".to_string(),
                result: ExecutionResult::Pass("ok".to_string()),
                generated_code: "code()".to_string(),
            }],
        };
        assert!(result.generate_feedback(&Language::Python).is_none());
    }

    // --- Container + local-install fallback (non-Python falls back to BareMetal) ---

    #[test]
    fn test_new_go_container_local_install_falls_back_to_bare_metal() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/go-project".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Go, &client, config, None).unwrap();
        // Container + non-Registry + non-Python should fall back to BareMetal
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
    }

    #[test]
    fn test_new_javascript_container_local_install_falls_back_to_bare_metal() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/js-project".to_string()),
            ..Default::default()
        };
        let validator =
            TestCodeValidator::new(&Language::JavaScript, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
    }

    #[test]
    fn test_new_java_container_local_install_falls_back_to_bare_metal() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/java-project".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Java, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
    }

    // --- BareMetal + local_source (with_local_source branch) ---

    #[test]
    fn test_new_go_baremetal_with_local_source() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::BareMetal,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/go-src".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Go, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
        assert_eq!(
            validator.local_source.as_deref(),
            Some("/tmp/go-src"),
            "local_source should be set from config"
        );
    }

    #[test]
    fn test_new_javascript_baremetal_with_local_source() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::BareMetal,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/js-src".to_string()),
            ..Default::default()
        };
        let validator =
            TestCodeValidator::new(&Language::JavaScript, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
        assert_eq!(validator.local_source.as_deref(), Some("/tmp/js-src"));
    }

    #[test]
    fn test_new_rust_baremetal_with_local_source() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::BareMetal,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/rust-src".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Rust, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
        assert_eq!(validator.local_source.as_deref(), Some("/tmp/rust-src"));
    }

    #[test]
    fn test_new_java_baremetal_with_local_source() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::BareMetal,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/java-src".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Java, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
        assert_eq!(validator.local_source.as_deref(), Some("/tmp/java-src"));
    }

    // --- Successful retry path (first run fails, retry succeeds) ---

    /// Executor that returns Fail on the first run_code, then Pass on the second.
    /// Covers the successful retry path (lines 542-544, 562-567).
    struct FailThenPassExecutor {
        call_count: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FailThenPassExecutor {
        fn new() -> Self {
            Self {
                call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait::async_trait]
    impl LanguageExecutor for FailThenPassExecutor {
        async fn setup_environment(&self, _deps: &[String]) -> Result<ExecutionEnv> {
            let temp_dir = tempfile::TempDir::new()?;
            Ok(ExecutionEnv {
                temp_dir,
                interpreter_path: None,
                container_name: None,
                dependencies: vec![],
            })
        }

        async fn run_code(&self, _env: &ExecutionEnv, _code: &str) -> Result<ExecutionResult> {
            let n = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 0 {
                Ok(ExecutionResult::Fail("first run compile error".to_string()))
            } else {
                Ok(ExecutionResult::Pass("retry succeeded".to_string()))
            }
        }

        async fn cleanup(&self, _env: &ExecutionEnv) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_validate_retry_succeeds_after_initial_failure() {
        let patterns = vec![basic_pattern()];
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("print('ok')")),
            Box::new(FailThenPassExecutor::new()),
            ValidationMode::Minimal,
            InstallSource::Registry,
        );
        let result = validator.validate("# SKILL.md", &[]).await.unwrap();
        // First run_code returns Fail, retry_test_code (default impl) succeeds,
        // second run_code returns Pass
        assert_eq!(result.passed, 1);
        assert_eq!(result.failed, 0);
        assert!(result.all_passed());
    }

    // --- Python container + local-install should NOT fall back ---

    #[test]
    fn test_new_python_container_local_install_stays_container() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/py-project".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Python, &client, config, None).unwrap();
        // Python is the exception: container + local-install should NOT fall back
        assert_eq!(validator.execution_mode(), ExecutionMode::Container);
    }

    // --- Python bare-metal with local source ---

    #[test]
    fn test_new_python_baremetal_with_local_source() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::BareMetal,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/py-src".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Python, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
        assert_eq!(validator.local_source.as_deref(), Some("/tmp/py-src"));
    }

    // --- Helper: make_validator with configurable language ---

    fn make_rust_validator<'a>(
        parser: Box<dyn LanguageParser>,
        code_generator: Box<dyn LanguageCodeGenerator + 'a>,
        executor: Box<dyn LanguageExecutor>,
        mode: ValidationMode,
        install_source: InstallSource,
        local_source: Option<String>,
    ) -> TestCodeValidator<'a> {
        TestCodeValidator {
            language: Language::Rust,
            parser,
            code_generator,
            executor,
            execution_mode: ExecutionMode::BareMetal,
            mode,
            install_source,
            executor_timeout: 30,
            local_source,
        }
    }

    // --- validate() Rust-specific structured deps path ---

    #[tokio::test]
    async fn test_validate_rust_uses_structured_deps_path() {
        // Exercises the Rust-specific env setup in validate() (lines 476-498).
        // Uses a SKILL.md with no deps so CargoExecutor skips `cargo fetch`.
        let patterns = vec![basic_pattern()];
        let validator = make_rust_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("fn main() {}")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            InstallSource::Registry,
            None,
        );
        // Minimal SKILL.md — RustParser will find no structured deps
        let result = validator
            .validate("# No deps SKILL.md\n", &[])
            .await
            .unwrap();
        assert_eq!(result.passed, 1);
        assert_eq!(result.failed, 0);
        assert!(result.all_passed());
    }

    #[tokio::test]
    async fn test_validate_rust_with_local_source_wires_into_cargo_executor() {
        // Covers the `if let Some(ref src) = self.local_source` branch at line 490
        let patterns = vec![basic_pattern()];
        let validator = make_rust_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("fn main() {}")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            InstallSource::LocalInstall,
            Some("/tmp/nonexistent-rust-src".to_string()),
        );
        let result = validator
            .validate("# No deps SKILL.md\n", &[])
            .await
            .unwrap();
        assert_eq!(result.passed, 1);
        assert!(result.all_passed());
    }

    #[tokio::test]
    async fn test_validate_rust_container_uses_executor_not_cargo() {
        // When execution_mode is Container, validate() should use self.executor
        // (the MockExecutor) instead of creating a fresh CargoExecutor.
        // This verifies the Codex P1 fix: container mode goes through the
        // container executor's setup/run/cleanup, not the bare-metal branch.
        let patterns = vec![basic_pattern()];
        let validator = TestCodeValidator {
            language: Language::Rust,
            parser: Box::new(MockParser::new(patterns)),
            code_generator: Box::new(MockCodeGenerator::succeeding("fn main() {}")),
            executor: Box::new(MockExecutor::passing("ok")),
            execution_mode: ExecutionMode::Container,
            mode: ValidationMode::Minimal,
            install_source: InstallSource::Registry,
            executor_timeout: 30,
            local_source: None,
        };
        // MockExecutor.setup_environment returns a valid env with no container_name.
        // If this used CargoExecutor, it would also return no container_name —
        // but the key difference is that run_code would fail on a real
        // ContainerExecutor because container_name is None.
        // With MockExecutor, it passes because MockExecutor doesn't check container_name.
        let result = validator.validate("# No deps\n", &[]).await.unwrap();
        assert_eq!(result.passed, 1);
        assert!(result.all_passed());
    }

    // --- LocalMount keeps container mode for non-Python ---

    #[test]
    fn test_new_go_container_local_mount_stays_container() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            install_source: InstallSource::LocalMount,
            source_path: Some("/tmp/go-mount".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Go, &client, config, None).unwrap();
        // LocalMount should NOT trigger the BareMetal fallback, even for non-Python
        assert_eq!(validator.execution_mode(), ExecutionMode::Container);
    }

    #[test]
    fn test_new_javascript_container_local_mount_stays_container() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            install_source: InstallSource::LocalMount,
            source_path: Some("/tmp/js-mount".to_string()),
            ..Default::default()
        };
        let validator =
            TestCodeValidator::new(&Language::JavaScript, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::Container);
    }

    #[test]
    fn test_new_java_container_local_mount_stays_container() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            install_source: InstallSource::LocalMount,
            source_path: Some("/tmp/java-mount".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Java, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::Container);
    }

    #[test]
    fn test_new_rust_container_local_mount_stays_container() {
        // Rust container now supported — LocalMount keeps container mode.
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            install_source: InstallSource::LocalMount,
            source_path: Some("/tmp/rust-mount".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Rust, &client, config, None).unwrap();
        assert_eq!(validator.execution_mode(), ExecutionMode::Container);
    }

    // --- Rust container + LocalInstall early fallback ---

    #[test]
    fn test_new_rust_container_local_install_uses_early_fallback() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::Container,
            install_source: InstallSource::LocalInstall,
            source_path: Some("/tmp/rust-local".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Rust, &client, config, None).unwrap();
        // Both the early fallback (non-Python + Container + LocalInstall) and the
        // Rust container fallback would apply; the early one fires first.
        assert_eq!(validator.execution_mode(), ExecutionMode::BareMetal);
        assert_eq!(validator.local_source.as_deref(), Some("/tmp/rust-local"));
    }

    // --- Registry install_source yields None local_source ---

    #[test]
    fn test_new_registry_source_has_no_local_source() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::BareMetal,
            install_source: InstallSource::Registry,
            source_path: Some("/tmp/should-be-ignored".to_string()),
            ..Default::default()
        };
        let validator = TestCodeValidator::new(&Language::Go, &client, config, None).unwrap();
        // Registry source: local_source should be None even if source_path is set
        assert!(
            validator.local_source.is_none(),
            "Registry install should not populate local_source"
        );
    }

    // --- LocalMount extracts local_source ---

    #[test]
    fn test_new_local_mount_extracts_local_source() {
        use crate::llm::client::MockLlmClient;

        let client = MockLlmClient;
        let config = ContainerConfig {
            execution_mode: ExecutionMode::BareMetal,
            install_source: InstallSource::LocalMount,
            source_path: Some("/tmp/mount-src".to_string()),
            ..Default::default()
        };
        let validator =
            TestCodeValidator::new(&Language::JavaScript, &client, config, None).unwrap();
        assert_eq!(
            validator.local_source.as_deref(),
            Some("/tmp/mount-src"),
            "LocalMount should extract local_source from config"
        );
    }

    #[tokio::test]
    async fn test_validate_rust_enriches_deps_from_collected() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        // SKILL.md has no deps (model omitted them). Collected deps fill the gap.
        let patterns = vec![basic_pattern()];
        let validator = make_rust_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("fn main() {}")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            InstallSource::Registry,
            None,
        );
        let collected = vec![
            StructuredDep {
                name: "serde_json".to_string(),
                raw_spec: Some("\"1\"".to_string()),
                source: DepSource::Manifest,
            },
            StructuredDep {
                name: "tokio".to_string(),
                raw_spec: Some("{ version = \"1\", features = [\"full\"] }".to_string()),
                source: DepSource::Manifest,
            },
        ];
        // Should pass — the collected deps enrich what the parser found (nothing)
        let result = validator
            .validate("# No deps SKILL.md\n", &collected)
            .await
            .unwrap();
        assert_eq!(result.passed, 1);
        assert!(result.all_passed());
    }

    #[tokio::test]
    async fn test_validate_rust_enrichment_upgrades_name_only_deps() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        // SKILL.md has `use tokio::*` — parser finds tokio as name-only (no spec).
        // Collected deps provide the real spec. Enrichment should upgrade, not skip.
        let skill_md = "---\nname: test\nmetadata:\n  version: \"1.0\"\n  ecosystem: rust\n---\n\n## Imports\n\n```rust\nuse tokio::runtime::Runtime;\nuse serde_json::Value;\n```\n\n## Core Patterns\n\n### Basic\n\n```rust\nfn main() {}\n```\n";
        let patterns = vec![basic_pattern()];
        let validator = make_rust_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("fn main() {}")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            InstallSource::Registry,
            None,
        );
        let collected = vec![
            // tokio: parser finds name-only, this provides the spec → upgrade path
            StructuredDep {
                name: "tokio".to_string(),
                raw_spec: Some("{ version = \"1\", features = [\"full\"] }".to_string()),
                source: DepSource::Manifest,
            },
            // serde_json: parser finds name-only, this provides the spec → upgrade path
            StructuredDep {
                name: "serde_json".to_string(),
                raw_spec: Some("\"1\"".to_string()),
                source: DepSource::Manifest,
            },
            // axum: not in SKILL.md at all → None path (add new)
            StructuredDep {
                name: "axum".to_string(),
                raw_spec: Some("\"0.8\"".to_string()),
                source: DepSource::Manifest,
            },
        ];
        let result = validator.validate(skill_md, &collected).await.unwrap();
        assert_eq!(result.passed, 1);
        assert!(result.all_passed());
    }

    #[tokio::test]
    async fn test_validate_rust_enrichment_skips_deps_with_existing_spec() {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        // SKILL.md has a [dependencies] TOML block with tokio spec.
        // Collected deps also have tokio spec. Enrichment should skip (not replace).
        let skill_md = "---\nname: test\nmetadata:\n  version: \"1.0\"\n  ecosystem: rust\n---\n\n## Imports\n\n```rust\nuse tokio::runtime::Runtime;\n```\n\n```toml\n[dependencies]\ntokio = { version = \"1\", features = [\"rt\"] }\n```\n\n## Core Patterns\n\n### Basic\n\n```rust\nfn main() {}\n```\n";
        let patterns = vec![basic_pattern()];
        let validator = make_rust_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(MockCodeGenerator::succeeding("fn main() {}")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            InstallSource::Registry,
            None,
        );
        let collected = vec![
            // tokio: parser already has spec from TOML block → skip path (Some(_))
            StructuredDep {
                name: "tokio".to_string(),
                raw_spec: Some("{ version = \"1\", features = [\"full\"] }".to_string()),
                source: DepSource::Manifest,
            },
        ];
        let result = validator.validate(skill_md, &collected).await.unwrap();
        assert_eq!(result.passed, 1);
        assert!(result.all_passed());
    }
}
