// Comprehensive end-to-end integration tests
// Coverage: Full pipeline from repo clone → file collection → 5-agent generation → lint validation
// Tests multiple ecosystems, config loading, error recovery, and output validation

use anyhow::Result;
use skilldo::config::{Config, GenerationConfig, LlmConfig, PromptsConfig};
use skilldo::detector::{detect_language, Language};
use skilldo::ecosystems::python::PythonHandler;
use skilldo::lint::{Severity, SkillLinter};
use skilldo::llm::client::MockLlmClient;
use skilldo::llm::factory;
use skilldo::pipeline::collector::Collector;
use skilldo::pipeline::generator::Generator;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ============================================================================
// Test Utilities
// ============================================================================

/// Create a minimal Python project structure for testing
fn create_test_python_project(base: &Path, name: &str) -> Result<PathBuf> {
    let project_dir = base.join(name);
    fs::create_dir_all(&project_dir)?;

    // Create pyproject.toml
    let pyproject = format!(
        r#"[project]
name = "{}"
version = "1.0.0"
description = "Test package"
license = "MIT"

[project.urls]
Homepage = "https://github.com/test/{}"
Documentation = "https://test.readthedocs.io"
"#,
        name, name
    );
    fs::write(project_dir.join("pyproject.toml"), pyproject)?;

    // Create source directory
    let src_dir = project_dir.join(name);
    fs::create_dir_all(&src_dir)?;

    // Create __init__.py with public API
    let init_py = r#""""Test package for integration tests."""

__version__ = "1.0.0"

class TestClass:
    """Main test class."""

    def __init__(self, value: int):
        self.value = value

    def get_value(self) -> int:
        """Get the stored value."""
        return self.value

    def set_value(self, value: int) -> None:
        """Set a new value."""
        self.value = value

def helper_function(x: int, y: int) -> int:
    """Add two numbers."""
    return x + y
"#;
    fs::write(src_dir.join("__init__.py"), init_py)?;

    // Create tests directory
    let tests_dir = project_dir.join("tests");
    fs::create_dir_all(&tests_dir)?;

    // Create test file
    let test_main = r#""""Tests for main functionality."""
import pytest

def test_class_creation():
    """Test creating a TestClass instance."""
    pass

def test_helper_function():
    """Test the helper function."""
    pass
"#;
    fs::write(tests_dir.join("test_main.py"), test_main)?;

    // Create examples directory
    let examples_dir = project_dir.join("examples");
    fs::create_dir_all(&examples_dir)?;

    let example_basic = r#""""Basic usage example."""
print("Example usage")
"#;
    fs::write(examples_dir.join("basic_usage.py"), example_basic)?;

    // Create README
    let readme = format!(
        r#"# {}

A test package.
"#,
        name
    );
    fs::write(project_dir.join("README.md"), readme)?;

    // Create CHANGELOG
    let changelog = r#"# Changelog

## [1.0.0] - 2024-01-01
- Initial release
"#;
    fs::write(project_dir.join("CHANGELOG.md"), changelog)?;

    Ok(project_dir)
}

/// Validate SKILL.md format and required sections
fn validate_skill_md_format(content: &str) -> Vec<String> {
    let mut errors = Vec::new();

    if !content.starts_with("---") {
        errors.push("Missing frontmatter delimiter".to_string());
    }

    let required_fields = vec!["name:", "description:", "version:", "ecosystem:"];
    for field in required_fields {
        if !content.contains(field) {
            errors.push(format!("Missing required frontmatter field: {}", field));
        }
    }

    let required_sections = vec!["## Imports", "## Core Patterns", "## Pitfalls"];
    for section in required_sections {
        if !content.contains(section) {
            errors.push(format!("Missing required section: {}", section));
        }
    }

    if !content.contains("```python") && !content.contains("```") {
        errors.push("No code examples found".to_string());
    }

    errors
}

// ============================================================================
// Integration Tests: Full Pipeline
// ============================================================================

#[tokio::test]
async fn test_full_pipeline_python_project() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = create_test_python_project(temp_dir.path(), "testpkg").unwrap();

    // Step 1: Detect language
    let language = detect_language(&project_dir).unwrap();
    assert_eq!(language, Language::Python);

    // Step 2: Collect files
    let collector = Collector::new(&project_dir, language);
    let data = collector.collect().await.unwrap();

    // Verify collected data
    assert_eq!(data.package_name, "testpkg");
    assert_eq!(data.version, "1.0.0");
    assert_eq!(data.license, Some("MIT".to_string()));
    assert!(!data.source_content.is_empty());
    assert!(!data.test_content.is_empty());

    // Step 3: Generate with mock client
    let client = Box::new(MockLlmClient::new());
    let generator = Generator::new(client, 3);
    let skill_md = generator.generate(&data).await.unwrap();

    // Step 4: Validate output format
    let format_errors = validate_skill_md_format(&skill_md);
    assert!(
        format_errors.is_empty(),
        "SKILL.md format errors: {:?}",
        format_errors
    );

    // Step 5: Lint validation
    let linter = SkillLinter::new();
    let issues = linter.lint(&skill_md).unwrap();

    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "Linting errors found: {:?}", errors);
}

#[tokio::test]
async fn test_pipeline_with_custom_config() {
    let custom_config = Config {
        llm: LlmConfig {
            provider: "mock".to_string(),
            model: "test-model".to_string(),
            api_key_env: None,
            base_url: None,
            max_tokens: None,
        },
        generation: GenerationConfig {
            max_retries: 5,
            max_source_tokens: 50000,
            enable_agent5: true,
            agent5_mode: "thorough".to_string(),
            agent5_llm: None,
            container: skilldo::config::ContainerConfig::default(),
        },
        prompts: PromptsConfig {
            override_prompts: false,
            agent1_mode: None,
            agent2_mode: None,
            agent3_mode: None,
            agent4_mode: None,
            agent1_custom: Some("Custom Agent 1 instructions".to_string()),
            agent2_custom: None,
            agent3_custom: None,
            agent4_custom: None,
            agent5_custom: None,
        },
    };

    // Verify config serialization
    let toml_str = toml::to_string(&custom_config).unwrap();
    assert!(toml_str.contains("provider = \"mock\""));
    assert!(toml_str.contains("max_retries = 5"));

    // Test config deserialization
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.llm.provider, "mock");
    assert_eq!(parsed.generation.max_retries, 5);
}

// ============================================================================
// Error Recovery Tests
// ============================================================================

#[tokio::test]
async fn test_error_recovery_missing_tests() {
    let temp_dir = TempDir::new().unwrap();
    let project = temp_dir.path().join("notests");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"notests\"",
    )
    .unwrap();

    let src = project.join("notests");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("__init__.py"), "pass").unwrap();

    let collector = Collector::new(&project, Language::Python);
    let result = collector.collect().await;

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("No tests found"));
}

#[tokio::test]
async fn test_error_recovery_nonexistent_repo() {
    let fake_path = Path::new("/nonexistent/path/to/repo");
    let collector = Collector::new(fake_path, Language::Python);

    let result = collector.collect().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_error_recovery_llm_client_failure() {
    env::remove_var("AI_API_KEY");

    let config = Config::default();
    let result = factory::create_client(&config, false);

    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("API key"));
    }
}

// ============================================================================
// Multiple Ecosystem Tests
// ============================================================================

#[test]
fn test_ecosystem_detection_python() {
    let temp_dir = TempDir::new().unwrap();
    let project = temp_dir.path().join("python_project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("pyproject.toml"), "[project]\nname = \"test\"").unwrap();

    let language = detect_language(&project).unwrap();
    assert_eq!(language, Language::Python);
}

#[test]
fn test_ecosystem_detection_javascript() {
    let temp_dir = TempDir::new().unwrap();
    let project = temp_dir.path().join("js_project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("package.json"), "{\"name\": \"test\"}").unwrap();

    let language = detect_language(&project).unwrap();
    assert_eq!(language, Language::JavaScript);
}

#[test]
fn test_ecosystem_detection_rust() {
    let temp_dir = TempDir::new().unwrap();
    let project = temp_dir.path().join("rust_project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

    let language = detect_language(&project).unwrap();
    assert_eq!(language, Language::Rust);
}

#[test]
fn test_ecosystem_detection_go() {
    let temp_dir = TempDir::new().unwrap();
    let project = temp_dir.path().join("go_project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("go.mod"), "module test").unwrap();

    let language = detect_language(&project).unwrap();
    assert_eq!(language, Language::Go);
}

#[test]
fn test_ecosystem_detection_failure() {
    let temp_dir = TempDir::new().unwrap();
    let project = temp_dir.path().join("unknown_project");
    fs::create_dir_all(&project).unwrap();

    let result = detect_language(&project);
    assert!(result.is_err());
}

// ============================================================================
// Python Handler Tests
// ============================================================================

#[test]
fn test_python_handler_find_all_files() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = create_test_python_project(temp_dir.path(), "fulltest").unwrap();

    let handler = PythonHandler::new(&project_dir);

    let sources = handler.find_source_files().unwrap();
    assert!(!sources.is_empty());

    let tests = handler.find_test_files().unwrap();
    assert!(!tests.is_empty());

    let docs = handler.find_docs().unwrap();
    assert!(!docs.is_empty());

    let version = handler.get_version().unwrap();
    assert_eq!(version, "1.0.0");

    let license = handler.get_license();
    assert_eq!(license, Some("MIT".to_string()));
}

#[test]
fn test_python_handler_nested_tests() {
    let temp_dir = TempDir::new().unwrap();
    let project = temp_dir.path().join("nested");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"nested\"",
    )
    .unwrap();

    let nested_tests = project.join("module").join("tests");
    fs::create_dir_all(&nested_tests).unwrap();
    fs::write(nested_tests.join("test_nested.py"), "def test(): pass").unwrap();

    let handler = PythonHandler::new(&project);
    let tests = handler.find_test_files().unwrap();

    assert!(!tests.is_empty());
}

// ============================================================================
// Output Validation Tests
// ============================================================================

#[tokio::test]
async fn test_skill_md_structure_validation() {
    let valid_skill = r#"---
name: testpkg
description: Test package
version: 1.0.0
ecosystem: python
license: MIT
---

## Imports

```python
from testpkg import TestClass
```

## Core Patterns

### Basic Usage

```python
obj = TestClass(42)
```

## Pitfalls

### Wrong: Bad approach

```python
# Bad code
```

### Right: Good approach

```python
# Good code
```
"#;

    let linter = SkillLinter::new();
    let issues = linter.lint(valid_skill).unwrap();

    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty());
}

#[test]
fn test_skill_md_missing_sections() {
    let incomplete_skill = r#"---
name: test
description: Test
version: 1.0.0
ecosystem: python
---

## Imports

Content here.
"#;

    let linter = SkillLinter::new();
    let issues = linter.lint(incomplete_skill).unwrap();

    assert!(issues.iter().any(|i| i.severity == Severity::Error));
}

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_config_from_toml() {
    let toml_str = r#"
[llm]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"

[generation]
max_retries = 5
max_source_tokens = 150000

[prompts]
override_prompts = true
agent1_custom = "Custom instructions here"
"#;

    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.llm.provider, "anthropic");
    assert_eq!(config.generation.max_retries, 5);
    assert!(config.prompts.override_prompts);
}

#[test]
fn test_config_api_key_retrieval() {
    env::set_var("TEST_INTEGRATION_KEY", "test_key_value");

    let mut config = Config::default();
    config.llm.api_key_env = Some("TEST_INTEGRATION_KEY".to_string());

    let api_key = config.get_api_key().unwrap();
    assert_eq!(api_key, "test_key_value");

    env::remove_var("TEST_INTEGRATION_KEY");
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_empty_examples_directory() {
    let temp_dir = TempDir::new().unwrap();
    let project = temp_dir.path().join("no_examples");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"no_examples\"",
    )
    .unwrap();

    let src = project.join("no_examples");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("__init__.py"), "pass").unwrap();

    let tests = project.join("tests");
    fs::create_dir_all(&tests).unwrap();
    fs::write(tests.join("test_main.py"), "def test(): pass").unwrap();

    let handler = PythonHandler::new(&project);
    let examples = handler.find_examples().unwrap();

    assert!(examples.is_empty());
}

#[tokio::test]
async fn test_version_fallback() {
    let temp_dir = TempDir::new().unwrap();
    let project = temp_dir.path().join("no_version");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("pyproject.toml"),
        "[project]\nname = \"no_version\"",
    )
    .unwrap();

    let handler = PythonHandler::new(&project);
    let version = handler.get_version().unwrap();

    assert_eq!(version, "latest");
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_large_project_collection() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = create_test_python_project(temp_dir.path(), "large_project").unwrap();

    let src = project_dir.join("large_project");
    for i in 0..50 {
        let content = format!("def function_{}(): pass\n", i);
        fs::write(src.join(format!("module_{}.py", i)), content).unwrap();
    }

    let collector = Collector::new(&project_dir, Language::Python);
    let start = std::time::Instant::now();
    let result = collector.collect().await;
    let duration = start.elapsed();

    assert!(result.is_ok());
    assert!(duration.as_secs() < 30);
}

#[test]
fn test_lint_performance() {
    let large_skill = format!(
        r#"---
name: large
description: Large test
version: 1.0.0
ecosystem: python
license: MIT
---

## Imports

```python
import large
```

## Core Patterns

{}

## Pitfalls

### Wrong
Bad

### Right
Good
"#,
        "Pattern\n".repeat(1000)
    );

    let linter = SkillLinter::new();
    let start = std::time::Instant::now();
    let result = linter.lint(&large_skill);
    let duration = start.elapsed();

    assert!(result.is_ok());
    assert!(duration.as_millis() < 100);
}
