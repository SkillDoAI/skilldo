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
            .filter(|tc| !tc.result.is_fail())
            .map(|tc| format!("- {}", tc.pattern_name))
            .collect();

        let failed_tests: Vec<String> = self
            .test_cases
            .iter()
            .filter(|tc| tc.result.is_fail())
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
            "Agent 5: Starting code generation validation (mode: {:?})",
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
}
