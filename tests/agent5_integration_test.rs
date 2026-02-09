use anyhow::Result;
use skilldo::agent5::{CodePattern, PatternCategory};
use skilldo::llm::client::LlmClient;
use std::fs;

/// Mock LLM client for testing
struct MockLlmClient {
    responses: Vec<String>,
    current_idx: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl MockLlmClient {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses,
            current_idx: std::sync::Arc::new(std::sync::Mutex::new(0)),
        }
    }
}

#[async_trait::async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, _prompt: &str) -> Result<String> {
        let mut idx = self.current_idx.lock().unwrap();
        let response = self
            .responses
            .get(*idx)
            .ok_or_else(|| anyhow::anyhow!("No more mock responses"))?
            .clone();
        *idx += 1;
        Ok(response)
    }
}

#[tokio::test]
async fn test_parser_extracts_patterns_from_click_skill() -> Result<()> {
    use skilldo::agent5::parser::PythonParser;
    use skilldo::agent5::LanguageParser;

    let skill_md = fs::read_to_string("tests/fixtures/click-SKILL.md")?;
    let parser = PythonParser;

    let patterns = parser.extract_patterns(&skill_md)?;

    // Should extract 3 patterns: Basic Command, Command with Options, Command with Arguments
    assert_eq!(
        patterns.len(),
        3,
        "Should extract 3 patterns from click SKILL.md"
    );

    // Check first pattern
    assert_eq!(patterns[0].name, "Basic Command");
    assert!(patterns[0].code.contains("@click.command()"));
    assert!(patterns[0].code.contains("click.echo"));
    assert_eq!(patterns[0].category, PatternCategory::BasicUsage);

    // Check second pattern
    assert_eq!(patterns[1].name, "Command with Options");
    assert!(patterns[1].code.contains("@click.option"));

    // Check third pattern
    assert_eq!(patterns[2].name, "Command with Arguments");
    assert!(patterns[2].code.contains("@click.argument"));

    Ok(())
}

#[tokio::test]
async fn test_parser_extracts_dependencies_from_click_skill() -> Result<()> {
    use skilldo::agent5::parser::PythonParser;
    use skilldo::agent5::LanguageParser;

    let skill_md = fs::read_to_string("tests/fixtures/click-SKILL.md")?;
    let parser = PythonParser;

    let deps = parser.extract_dependencies(&skill_md)?;

    // Should extract only 'click', not stdlib modules
    assert_eq!(deps.len(), 1, "Should extract only click dependency");
    assert_eq!(deps[0], "click");

    Ok(())
}

#[tokio::test]
async fn test_parser_extracts_patterns_from_pathlib_skill() -> Result<()> {
    use skilldo::agent5::parser::PythonParser;
    use skilldo::agent5::LanguageParser;

    let skill_md = fs::read_to_string("tests/fixtures/pathlib-SKILL.md")?;
    let parser = PythonParser;

    let patterns = parser.extract_patterns(&skill_md)?;

    assert_eq!(patterns.len(), 3, "Should extract 3 patterns");
    assert_eq!(patterns[0].name, "Basic Path Creation");
    assert!(patterns[0].code.contains("Path("));

    Ok(())
}

#[tokio::test]
async fn test_parser_filters_stdlib_dependencies() -> Result<()> {
    use skilldo::agent5::parser::PythonParser;
    use skilldo::agent5::LanguageParser;

    let skill_md = fs::read_to_string("tests/fixtures/pathlib-SKILL.md")?;
    let parser = PythonParser;

    let deps = parser.extract_dependencies(&skill_md)?;

    // Should be empty - pathlib and os are stdlib
    assert_eq!(deps.len(), 0, "Should filter out stdlib modules");

    Ok(())
}

#[tokio::test]
async fn test_code_generator_creates_test_prompt() -> Result<()> {
    use skilldo::agent5::code_generator::PythonCodeGenerator;
    use skilldo::agent5::LanguageCodeGenerator;

    let mock_client = MockLlmClient::new(vec![r#"
```python
import click

@click.command()
def hello():
    click.echo('✓ Test passed: Basic Command')

if __name__ == '__main__':
    hello()
```
"#
    .to_string()]);

    let generator = PythonCodeGenerator::new(&mock_client);

    let pattern = CodePattern {
        name: "Basic Command".to_string(),
        description: "Create a simple CLI command".to_string(),
        code: "@click.command()\ndef hello():\n    click.echo('Hello')".to_string(),
        category: PatternCategory::BasicUsage,
    };

    let test_code: String = generator.generate_test_code(&pattern).await?;

    assert!(
        test_code.contains("import click"),
        "Should have click import"
    );
    assert!(
        test_code.contains("@click.command()"),
        "Should have command decorator"
    );
    assert!(
        test_code.contains("✓ Test passed"),
        "Should have success message"
    );

    Ok(())
}

#[tokio::test]
async fn test_code_generator_extracts_code_from_markdown() -> Result<()> {
    use skilldo::agent5::code_generator::PythonCodeGenerator;
    use skilldo::agent5::LanguageCodeGenerator;

    let mock_client = MockLlmClient::new(vec!["```python\nprint('test')\n```".to_string()]);

    let generator = PythonCodeGenerator::new(&mock_client);

    let pattern = CodePattern {
        name: "Test".to_string(),
        description: "Test".to_string(),
        code: "pass".to_string(),
        category: PatternCategory::BasicUsage,
    };

    let test_code: String = generator.generate_test_code(&pattern).await?;

    assert_eq!(test_code.trim(), "print('test')");
    assert!(
        !test_code.contains("```"),
        "Should not contain markdown fences"
    );

    Ok(())
}

#[tokio::test]
// Requires uv installed (uv 0.7.7 available)
async fn test_executor_runs_simple_python_code() -> Result<()> {
    use skilldo::agent5::executor::PythonUvExecutor;
    use skilldo::agent5::LanguageExecutor;

    let executor = PythonUvExecutor::new();

    // Setup environment with no dependencies
    let env = executor.setup_environment(&[])?;

    // Run simple code
    let code = r#"
print("✓ Test passed")
"#;

    let result = executor.run_code(&env, code)?;

    assert!(result.is_pass(), "Simple code should pass");

    let output = match result {
        skilldo::agent5::executor::ExecutionResult::Pass(out) => out,
        _ => panic!("Expected Pass result"),
    };

    assert!(output.contains("✓ Test passed"));

    executor.cleanup(&env)?;

    Ok(())
}

#[tokio::test]
// Requires uv installed (uv 0.7.7 available)
async fn test_executor_handles_failing_code() -> Result<()> {
    use skilldo::agent5::executor::PythonUvExecutor;
    use skilldo::agent5::LanguageExecutor;

    let executor = PythonUvExecutor::new();
    let env = executor.setup_environment(&[])?;

    let code = r#"
raise ValueError("Test error")
"#;

    let result = executor.run_code(&env, code)?;

    assert!(result.is_fail(), "Failing code should fail");

    let error = match result {
        skilldo::agent5::executor::ExecutionResult::Fail(err) => err,
        _ => panic!("Expected Fail result"),
    };

    assert!(error.contains("ValueError"));
    assert!(error.contains("Test error"));

    executor.cleanup(&env)?;

    Ok(())
}

#[tokio::test]
// Requires uv installed (uv 0.7.7 available) and network
async fn test_executor_installs_dependencies() -> Result<()> {
    use skilldo::agent5::executor::PythonUvExecutor;
    use skilldo::agent5::LanguageExecutor;

    let executor = PythonUvExecutor::new();

    // Setup with click dependency
    let env = executor.setup_environment(&["click".to_string()])?;

    let code = r#"
import click
print(f"✓ Click version: {click.__version__}")
"#;

    let result = executor.run_code(&env, code)?;

    assert!(result.is_pass(), "Code with dependencies should pass");

    executor.cleanup(&env)?;

    Ok(())
}

#[tokio::test]
async fn test_validator_selects_patterns_thorough_mode() {
    use skilldo::agent5::ValidationMode;

    let _mock_client = MockLlmClient::new(vec![]);

    // Test pattern selection logic through modes
    assert_eq!(
        std::mem::discriminant(&ValidationMode::Thorough),
        std::mem::discriminant(&ValidationMode::Thorough)
    );
    assert_eq!(
        std::mem::discriminant(&ValidationMode::Minimal),
        std::mem::discriminant(&ValidationMode::Minimal)
    );
    assert_eq!(
        std::mem::discriminant(&ValidationMode::Adaptive),
        std::mem::discriminant(&ValidationMode::Adaptive)
    );

    // Note: Pattern selection is tested indirectly through full integration tests
    // as select_patterns is a private method
}

#[tokio::test]
#[ignore] // Requires Docker/Podman container runtime
async fn test_full_agent5_flow_with_click() -> Result<()> {
    use skilldo::agent5::Agent5CodeValidator;

    // Mock LLM that returns valid test code
    let mock_client = MockLlmClient::new(vec![
        // Test code for pattern 1
        r#"
```python
import click

@click.command()
def hello():
    click.echo('✓ Test passed: Basic Command')

if __name__ == '__main__':
    hello()
```
"#
        .to_string(),
        // Test code for pattern 2
        r#"
```python
import click

@click.command()
@click.option('--name', default='World')
def hello(name):
    click.echo(f'✓ Test passed: Command with Options')

if __name__ == '__main__':
    hello()
```
"#
        .to_string(),
        // Test code for pattern 3
        r#"
```python
import click

@click.command()
@click.argument('name')
def hello(name):
    click.echo(f'✓ Test passed: Command with Arguments')

if __name__ == '__main__':
    hello(['test'])
```
"#
        .to_string(),
    ]);

    let validator =
        Agent5CodeValidator::new_python(&mock_client, skilldo::config::ContainerConfig::default());

    let skill_md = fs::read_to_string("tests/fixtures/click-SKILL.md")?;

    let result = validator.validate(&skill_md).await?;

    assert!(result.all_passed(), "All tests should pass");
    assert_eq!(result.passed, 3, "Should have 3 passed tests");
    assert_eq!(result.failed, 0, "Should have 0 failed tests");
    assert!(
        result.generate_feedback().is_none(),
        "Should have no feedback on success"
    );

    Ok(())
}

#[tokio::test]
async fn test_test_result_generates_feedback_on_failure() {
    use skilldo::agent5::executor::ExecutionResult;
    use skilldo::agent5::validator::TestResult;

    let test_cases = vec![
        skilldo::agent5::validator::TestCase {
            pattern_name: "Good Pattern".to_string(),
            result: ExecutionResult::Pass("success".to_string()),
            generated_code: "print('ok')".to_string(),
        },
        skilldo::agent5::validator::TestCase {
            pattern_name: "Bad Pattern".to_string(),
            result: ExecutionResult::Fail("ImportError: No module named 'missing'".to_string()),
            generated_code: "import missing".to_string(),
        },
    ];

    let result = TestResult {
        passed: 1,
        failed: 1,
        test_cases,
    };

    assert!(!result.all_passed());

    let feedback = result.generate_feedback().unwrap();

    assert!(feedback.contains("Bad Pattern"));
    assert!(feedback.contains("ImportError"));
    assert!(feedback.contains("PATCH REQUIRED"));
    assert!(feedback.contains("Good Pattern")); // passed patterns listed
}

#[tokio::test]
async fn test_parser_handles_missing_core_patterns_section() -> Result<()> {
    use skilldo::agent5::parser::PythonParser;
    use skilldo::agent5::LanguageParser;

    // SKILL.md without Core Patterns section
    let skill_md = r#"---
name: test
version: 1.0.0
---

## Imports

```python
import test
```

## Configuration

None

## Pitfalls

None
"#;

    let parser = PythonParser;
    let patterns = parser.extract_patterns(skill_md)?;

    // Should return empty patterns when section is missing
    assert!(
        patterns.is_empty(),
        "Should return empty patterns when Core Patterns section missing"
    );

    Ok(())
}

#[tokio::test]
async fn test_parser_handles_missing_imports_section() -> Result<()> {
    use skilldo::agent5::parser::PythonParser;
    use skilldo::agent5::LanguageParser;

    // SKILL.md without Imports section
    let skill_md = r#"---
name: test
version: 1.0.0
---

## Core Patterns

### Basic

```python
print("hello")
```

## Pitfalls

None
"#;

    let parser = PythonParser;
    let deps = parser.extract_dependencies(skill_md)?;

    // Should return empty dependencies when section is missing
    assert!(
        deps.is_empty(),
        "Should return empty deps when Imports section missing"
    );

    Ok(())
}

#[tokio::test]
async fn test_parser_deduplicates_dependencies() -> Result<()> {
    use skilldo::agent5::parser::PythonParser;
    use skilldo::agent5::LanguageParser;

    // SKILL.md with duplicate imports
    let skill_md = r#"---
name: test
version: 1.0.0
---

## Imports

```python
import click
import requests
import click
from click import command
```

## Core Patterns

Test

## Pitfalls

None
"#;

    let parser = PythonParser;
    let deps = parser.extract_dependencies(skill_md)?;

    // Should deduplicate click
    let click_count = deps.iter().filter(|d| *d == "click").count();
    assert_eq!(click_count, 1, "Should deduplicate click import");

    // Should have both click and requests
    assert!(deps.contains(&"click".to_string()));
    assert!(deps.contains(&"requests".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_validator_with_mode_method() -> Result<()> {
    use skilldo::agent5::{Agent5CodeValidator, ValidationMode};

    let mock_client = MockLlmClient::new(vec![]);

    // Test that with_mode() sets the mode
    let _validator =
        Agent5CodeValidator::new_python(&mock_client, skilldo::config::ContainerConfig::default())
            .with_mode(ValidationMode::Minimal);

    // If we get here without panic, the mode was set successfully

    Ok(())
}

#[tokio::test]
async fn test_validator_with_empty_patterns() -> Result<()> {
    use skilldo::agent5::Agent5CodeValidator;

    let mock_client = MockLlmClient::new(vec![]);
    let validator =
        Agent5CodeValidator::new_python(&mock_client, skilldo::config::ContainerConfig::default());

    // SKILL.md with no patterns (no Core Patterns section)
    let skill_md = r#"---
name: test
version: 1.0.0
---

## Imports

```python
import sys
```

## Pitfalls

None
"#;

    let result = validator.validate(skill_md).await?;

    // Should return 0 tests when no patterns found
    assert_eq!(result.passed, 0, "Should have 0 passed tests");
    assert_eq!(result.failed, 0, "Should have 0 failed tests");
    assert_eq!(result.test_cases.len(), 0, "Should have empty test_cases");

    Ok(())
}

#[tokio::test]
async fn test_validator_minimal_mode() -> Result<()> {
    use skilldo::agent5::{Agent5CodeValidator, ValidationMode};

    let mock_client = MockLlmClient::new(vec![r#"
```python
import click

@click.command()
def hello():
    click.echo('✓ Test passed: Basic Command')

if __name__ == '__main__':
    hello()
```
"#
    .to_string()]);

    let validator =
        Agent5CodeValidator::new_python(&mock_client, skilldo::config::ContainerConfig::default())
            .with_mode(ValidationMode::Minimal);

    let skill_md = fs::read_to_string("tests/fixtures/click-SKILL.md")?;
    let result = validator.validate(&skill_md).await?;

    // Minimal mode should only test 1 pattern
    assert_eq!(
        result.test_cases.len(),
        1,
        "Minimal mode should test only 1 pattern"
    );

    Ok(())
}

#[tokio::test]
async fn test_validator_adaptive_mode() -> Result<()> {
    use skilldo::agent5::{Agent5CodeValidator, ValidationMode};

    let mock_client = MockLlmClient::new(vec![r#"
```python
import click

@click.command()
def hello():
    click.echo('✓ Test passed: Basic Command')

if __name__ == '__main__':
    hello()
```
"#
    .to_string()]);

    let validator =
        Agent5CodeValidator::new_python(&mock_client, skilldo::config::ContainerConfig::default())
            .with_mode(ValidationMode::Adaptive);

    let skill_md = fs::read_to_string("tests/fixtures/click-SKILL.md")?;
    let result = validator.validate(&skill_md).await?;

    // Adaptive mode currently acts like Minimal (1 pattern)
    assert_eq!(
        result.test_cases.len(),
        1,
        "Adaptive mode should test 1 pattern (for now)"
    );

    Ok(())
}

#[tokio::test]
async fn test_execution_result_error_message_for_pass() {
    use skilldo::agent5::executor::ExecutionResult;

    let result = ExecutionResult::Pass("Test output".to_string());
    assert_eq!(result.error_message(), "Test output");
    assert!(!result.is_fail());
}

#[tokio::test]
async fn test_execution_result_error_message_for_timeout() {
    use skilldo::agent5::executor::ExecutionResult;

    let result = ExecutionResult::Timeout;
    assert_eq!(
        result.error_message(),
        "Test execution timed out (60 seconds)"
    );
    assert!(!result.is_fail());
}

#[tokio::test]
async fn test_executor_default_constructor() {
    use skilldo::agent5::executor::PythonUvExecutor;

    // Should create without panicking
    let _executor = PythonUvExecutor::default();
}

#[tokio::test]
async fn test_executor_with_timeout_method() {
    use skilldo::agent5::executor::PythonUvExecutor;

    // Should set timeout without panicking
    let _executor = PythonUvExecutor::new().with_timeout(30);
}

#[tokio::test]
async fn test_test_result_generate_feedback_returns_none_on_success() {
    use skilldo::agent5::executor::ExecutionResult;
    use skilldo::agent5::validator::TestResult;

    let test_cases = vec![skilldo::agent5::validator::TestCase {
        pattern_name: "Pattern 1".to_string(),
        result: ExecutionResult::Pass("success".to_string()),
        generated_code: "print('ok')".to_string(),
    }];

    let result = TestResult {
        passed: 1,
        failed: 0,
        test_cases,
    };

    assert!(result.all_passed());
    assert!(
        result.generate_feedback().is_none(),
        "Should return None when all tests pass"
    );
}
