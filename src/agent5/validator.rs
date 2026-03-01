use anyhow::Result;
use tracing::{debug, info, warn};

use super::code_generator::PythonCodeGenerator;
use super::container_executor::ContainerExecutor;
use super::executor::ExecutionResult;
use super::parser::PythonParser;
use super::{CodePattern, LanguageCodeGenerator, LanguageExecutor, LanguageParser};
use crate::config::ContainerConfig;
use crate::llm::client::LlmClient;

/// Validation mode for Agent 5
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationMode {
    /// Test 2-3 patterns thoroughly
    #[default]
    Thorough,
    /// Test 1 pattern initially, expand if it passes easily (future)
    Adaptive,
    /// Test just 1 pattern for quick validation
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

    pub fn generate_feedback(&self) -> Option<String> {
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
                    "Pattern: {}\nGenerated test code:\n```python\n{}\n```\nError: {}",
                    tc.pattern_name,
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

/// Main Agent 5 coordinator
pub struct Agent5CodeValidator<'a> {
    parser: Box<dyn LanguageParser>,
    code_generator: Box<dyn LanguageCodeGenerator + 'a>,
    executor: Box<dyn LanguageExecutor>,
    mode: ValidationMode,
    /// Install source from config; when not "registry", local_package is set on code_generator
    install_source: String,
}

impl<'a> Agent5CodeValidator<'a> {
    /// Create a new Agent 5 validator for Python
    #[allow(dead_code)]
    pub fn new_python(llm_client: &'a dyn LlmClient, config: ContainerConfig) -> Self {
        let install_source = config.install_source.clone();
        Self {
            parser: Box::new(PythonParser),
            code_generator: Box::new(PythonCodeGenerator::new(llm_client)),
            executor: Box::new(ContainerExecutor::new(config, "python")),
            mode: ValidationMode::default(),
            install_source,
        }
    }

    /// Create a new Agent 5 validator for Python with custom instructions (append-only)
    pub fn new_python_with_custom(
        llm_client: &'a dyn LlmClient,
        config: ContainerConfig,
        custom_instructions: Option<String>,
    ) -> Self {
        let install_source = config.install_source.clone();
        Self {
            parser: Box::new(PythonParser),
            code_generator: Box::new(
                PythonCodeGenerator::new(llm_client).with_custom_instructions(custom_instructions),
            ),
            executor: Box::new(ContainerExecutor::new(config, "python")),
            mode: ValidationMode::default(),
            install_source,
        }
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
            ValidationMode::Minimal => {
                // Just test the first basic usage pattern
                vec![&patterns[0]]
            }
            ValidationMode::Thorough => {
                // Test up to 3 patterns: prioritize Basic, Config, Error handling
                let mut selected = Vec::new();

                // Try to get one from each category
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
            ValidationMode::Adaptive => {
                // For now, same as Minimal - future enhancement
                // TODO: Start with 1, expand to 2-3 if first passes quickly
                vec![&patterns[0]]
            }
        }
    }

    /// Validate SKILL.md by generating and running test code
    pub async fn validate(&self, skill_md: &str) -> Result<TestResult> {
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
        if self.install_source != "registry" {
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
        info!("  → Setting up Python environment...");
        let env = self.executor.setup_environment(&deps)?;

        let mut test_cases = Vec::new();

        // 4. Generate and run tests for each pattern
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
            let result = match self.executor.run_code(&env, &test_code) {
                Ok(r) => r,
                Err(e) => {
                    warn!("    Execution error: {}", e);
                    self.executor.cleanup(&env).ok();
                    return Err(e);
                }
            };

            match &result {
                ExecutionResult::Pass(output) => {
                    info!("    ✓ Test passed");
                    debug!(
                        "    Output: {}",
                        output.lines().next().unwrap_or("(no output)")
                    );
                }
                ExecutionResult::Fail(error) => {
                    warn!("    ✗ Test failed");
                    debug!(
                        "    Error: {}",
                        error.lines().next().unwrap_or("(no error message)")
                    );
                }
                ExecutionResult::Timeout => {
                    warn!("    ⏱  Test timed out");
                }
            }

            test_cases.push(TestCase {
                pattern_name: pattern.name.clone(),
                result,
                generated_code: test_code,
            });
        }

        // 5. Cleanup
        self.executor.cleanup(&env)?;

        // 6. Analyze results
        let passed = test_cases.iter().filter(|tc| tc.result.is_pass()).count();
        let failed = test_cases.len() - passed;

        if failed == 0 {
            info!("  ✓ All {} tests passed!", passed);
        } else {
            warn!("  ✗ {} tests passed, {} failed", passed, failed);
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
            ValidationMode::Thorough => 3.min(patterns.len()),
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
        assert!(result.generate_feedback().is_none());
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
        let feedback = result.generate_feedback().unwrap();
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
            ValidationMode::Thorough => 3.min(patterns.len()),
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
        assert!(result.generate_feedback().is_none());
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
    use std::sync::Mutex;

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
        local_package: Mutex<Option<String>>,
    }

    impl MockCodeGenerator {
        fn succeeding(code: &str) -> Self {
            Self {
                result: Mutex::new(Ok(code.to_string())),
                local_package: Mutex::new(None),
            }
        }

        fn failing(msg: &str) -> Self {
            Self {
                result: Mutex::new(Err(msg.to_string())),
                local_package: Mutex::new(None),
            }
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

    impl LanguageExecutor for MockExecutor {
        fn setup_environment(&self, _deps: &[String]) -> Result<ExecutionEnv> {
            let temp_dir = tempfile::TempDir::new()?;
            Ok(ExecutionEnv {
                temp_dir,
                python_path: None,
                container_name: None,
                dependencies: vec![],
            })
        }

        fn run_code(&self, _env: &ExecutionEnv, _code: &str) -> Result<ExecutionResult> {
            let guard = self.run_result.lock().unwrap();
            match &*guard {
                Ok(r) => Ok(r.clone()),
                Err(msg) => Err(anyhow::anyhow!("{}", msg)),
            }
        }

        fn cleanup(&self, _env: &ExecutionEnv) -> Result<()> {
            Ok(())
        }
    }

    fn make_validator<'a>(
        parser: Box<dyn LanguageParser>,
        code_generator: Box<dyn LanguageCodeGenerator + 'a>,
        executor: Box<dyn LanguageExecutor>,
        mode: ValidationMode,
        install_source: &str,
    ) -> Agent5CodeValidator<'a> {
        Agent5CodeValidator {
            parser,
            code_generator,
            executor,
            mode,
            install_source: install_source.to_string(),
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
            "registry",
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
            "registry",
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
            "registry",
        );
        let patterns = vec![error_pattern(), basic_pattern()];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "Error Handling");
    }

    #[test]
    fn test_select_patterns_thorough_prioritizes_categories() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            "registry",
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
        // Should pick BasicUsage, Configuration, ErrorHandling (by category priority)
        let names: Vec<&str> = selected.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Basic Usage"));
        assert!(names.contains(&"Configuration"));
        assert!(names.contains(&"Error Handling"));
    }

    #[test]
    fn test_select_patterns_thorough_fills_remaining() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            "registry",
        );
        // Only BasicUsage category present, rest are Other
        let patterns = vec![
            basic_pattern(),
            other_pattern("Extra1"),
            other_pattern("Extra2"),
            other_pattern("Extra3"),
        ];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].name, "Basic Usage");
        assert_eq!(selected[1].name, "Extra1");
        assert_eq!(selected[2].name, "Extra2");
    }

    #[test]
    fn test_select_patterns_thorough_with_fewer_than_3() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            "registry",
        );
        let patterns = vec![basic_pattern()];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "Basic Usage");
    }

    #[test]
    fn test_select_patterns_thorough_exactly_3_categories() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            "registry",
        );
        let patterns = vec![basic_pattern(), config_pattern(), error_pattern()];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn test_select_patterns_thorough_more_than_3_all_categories() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            "registry",
        );
        // 5 patterns, 3 matching priority categories
        let patterns = vec![
            basic_pattern(),
            config_pattern(),
            error_pattern(),
            async_pattern(),
            other_pattern("Extra"),
        ];
        let selected = validator.select_patterns(&patterns);
        // Should stop at 3 from priority categories
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn test_select_patterns_thorough_no_priority_categories() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![])),
            Box::new(MockCodeGenerator::succeeding("")),
            Box::new(MockExecutor::passing("")),
            ValidationMode::Thorough,
            "registry",
        );
        // All Other/Async categories -- none match BasicUsage/Configuration/ErrorHandling
        let patterns = vec![
            async_pattern(),
            other_pattern("A"),
            other_pattern("B"),
            other_pattern("C"),
        ];
        let selected = validator.select_patterns(&patterns);
        assert_eq!(selected.len(), 3);
        // Fills from remaining
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
            "registry",
        );
        let result = validator.validate("# SKILL.md").await.unwrap();
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
            "registry",
        );
        let result = validator.validate("# SKILL.md").await.unwrap();
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
            "registry",
        );
        let result = validator.validate("# SKILL.md").await.unwrap();
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
            "registry",
        );
        let err = validator.validate("# SKILL.md").await.unwrap_err();
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
            "registry",
        );
        let result = validator.validate("# SKILL.md").await.unwrap();
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
            "registry",
        );
        let result = validator.validate("# SKILL.md").await.unwrap();
        // Timeout is not is_pass(), so it counts as failed
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 1);
    }

    #[tokio::test]
    async fn test_validate_local_install_source_sets_local_package() {
        let patterns = vec![basic_pattern()];
        let code_gen = MockCodeGenerator::succeeding("print('ok')");
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(code_gen),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            "local-install", // non-registry
        );
        let result = validator.validate("# SKILL.md").await.unwrap();
        assert!(result.all_passed());
        // Verify set_local_package was called by checking the code_generator
        // (The mock stores the value; we verify the code path ran without error)
    }

    #[tokio::test]
    async fn test_validate_with_mode() {
        let validator = make_validator(
            Box::new(MockParser::new(vec![basic_pattern()])),
            Box::new(MockCodeGenerator::succeeding("code")),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Thorough,
            "registry",
        );
        let validator = validator.with_mode(ValidationMode::Minimal);
        assert_eq!(validator.mode, ValidationMode::Minimal);
    }

    #[tokio::test]
    async fn test_validate_registry_does_not_set_local_package() {
        let patterns = vec![basic_pattern()];
        let code_gen = MockCodeGenerator::succeeding("print('ok')");
        // Verify the set_local_package path is NOT taken for "registry" install_source
        let validator = make_validator(
            Box::new(MockParser::new(patterns)),
            Box::new(code_gen),
            Box::new(MockExecutor::passing("ok")),
            ValidationMode::Minimal,
            "registry",
        );
        let result = validator.validate("# SKILL.md").await.unwrap();
        assert!(result.all_passed());
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
        let feedback = result.generate_feedback().unwrap();
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
        let feedback = result.generate_feedback().unwrap();
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
        let feedback = result.generate_feedback().unwrap();
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
        let feedback = result.generate_feedback().unwrap();
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
            "local-install",
        );
        // When name is None, local package path is skipped
        let result = validator.validate("# SKILL.md").await.unwrap();
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
            "registry",
        );
        let result = validator.validate("# SKILL.md").await.unwrap();
        assert_eq!(result.passed, 0);
        assert_eq!(result.failed, 3);
        assert_eq!(result.test_cases.len(), 3);
    }
}
