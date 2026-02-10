use skilldo::detector::Language;
use skilldo::pipeline::collector::{CollectedData, Collector};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to create a minimal Python project structure
fn create_test_python_project(name: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    // Create pyproject.toml
    fs::write(
        base.join("pyproject.toml"),
        format!(
            r#"[project]
name = "{}"
version = "1.2.3"
license = "MIT"

[project.urls]
Homepage = "https://example.com"
Documentation = "https://docs.example.com"
"#,
            name
        ),
    )
    .unwrap();

    // Create package directory
    let pkg_dir = base.join(name);
    fs::create_dir_all(&pkg_dir).unwrap();

    // Create __init__.py
    fs::write(
        pkg_dir.join("__init__.py"),
        r#""""Example package."""
__version__ = "1.2.3"

def hello():
    """Say hello."""
    return "Hello, World!"
"#,
    )
    .unwrap();

    // Create a module file
    fs::write(
        pkg_dir.join("core.py"),
        r#""""Core functionality."""

class Calculator:
    """A simple calculator."""

    def add(self, a, b):
        """Add two numbers."""
        return a + b

    def multiply(self, a, b):
        """Multiply two numbers."""
        return a * b
"#,
    )
    .unwrap();

    // Create tests directory
    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();

    fs::write(
        tests_dir.join("test_core.py"),
        r#""""Tests for core module."""
import pytest
from testpkg.core import Calculator

def test_calculator_add():
    calc = Calculator()
    assert calc.add(2, 3) == 5

def test_calculator_multiply():
    calc = Calculator()
    assert calc.multiply(4, 5) == 20
"#,
    )
    .unwrap();

    // Create examples directory
    let examples_dir = base.join("examples");
    fs::create_dir_all(&examples_dir).unwrap();

    fs::write(
        examples_dir.join("basic_usage.py"),
        r#"""Basic usage example."""
from testpkg import hello
from testpkg.core import Calculator

# Simple hello
print(hello())

# Calculator example
calc = Calculator()
result = calc.add(10, 20)
print(f"10 + 20 = {result}")
"#,
    )
    .unwrap();

    // Create docs
    fs::write(
        base.join("README.md"),
        r#"# Test Package

This is a test package for demonstrating the collector.

## Installation

```bash
pip install testpkg
```

## Usage

```python
from testpkg import hello
print(hello())
```
"#,
    )
    .unwrap();

    // Create changelog
    fs::write(
        base.join("CHANGELOG.md"),
        r#"# Changelog

## [1.2.3] - 2024-01-01

### Added
- Initial release
- Basic calculator functionality

### Fixed
- Bug in multiply function
"#,
    )
    .unwrap();

    dir
}

#[tokio::test]
async fn test_collect_successful_with_all_file_types() {
    // Arrange
    let project = create_test_python_project("testpkg");
    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok(), "Collection should succeed");
    let data = result.unwrap();

    // Validate package metadata
    assert_eq!(data.package_name, "testpkg");
    assert_eq!(data.version, "1.2.3");
    assert_eq!(data.license, Some("MIT".to_string()));
    assert_eq!(data.language, Language::Python);

    // Validate project URLs
    assert_eq!(data.project_urls.len(), 2);
    assert!(data
        .project_urls
        .iter()
        .any(|(k, v)| k == "Homepage" && v == "https://example.com"));
    assert!(data
        .project_urls
        .iter()
        .any(|(k, v)| k == "Documentation" && v == "https://docs.example.com"));

    // Validate content was collected
    assert!(
        !data.examples_content.is_empty(),
        "Should have examples content"
    );
    assert!(
        data.examples_content.contains("basic_usage.py"),
        "Should include example filename"
    );
    assert!(
        data.examples_content.contains("Calculator example"),
        "Should include example code"
    );

    assert!(!data.test_content.is_empty(), "Should have test content");
    assert!(
        data.test_content.contains("test_core.py"),
        "Should include test filename"
    );
    assert!(
        data.test_content.contains("test_calculator_add"),
        "Should include test code"
    );

    assert!(!data.docs_content.is_empty(), "Should have docs content");
    assert!(
        data.docs_content.contains("Test Package"),
        "Should include README content"
    );

    assert!(
        !data.source_content.is_empty(),
        "Should have source content"
    );
    assert!(
        data.source_content.contains("public API") || data.source_content.contains("__init__.py"),
        "Should prioritize public API files"
    );

    assert!(
        !data.changelog_content.is_empty(),
        "Should have changelog content"
    );
    assert!(
        data.changelog_content.contains("1.2.3"),
        "Should include version in changelog"
    );
}

#[tokio::test]
async fn test_collect_handles_missing_examples() {
    // Arrange
    let project = create_test_python_project("nolixpkg");
    // Don't create examples directory - already done in helper, so remove it
    fs::remove_dir_all(project.path().join("examples")).unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok(), "Should succeed even without examples");
    let data = result.unwrap();
    assert_eq!(
        data.examples_content, "",
        "Examples content should be empty"
    );
    assert!(
        !data.test_content.is_empty(),
        "Should still have test content"
    );
    assert!(
        !data.source_content.is_empty(),
        "Should still have source content"
    );
}

#[tokio::test]
async fn test_collect_handles_missing_changelog() {
    // Arrange
    let project = create_test_python_project("nochangelog");
    fs::remove_file(project.path().join("CHANGELOG.md")).unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok(), "Should succeed without changelog");
    let data = result.unwrap();
    assert_eq!(
        data.changelog_content, "",
        "Changelog content should be empty"
    );
}

#[tokio::test]
async fn test_collect_handles_missing_docs() {
    // Arrange
    let project = create_test_python_project("nodocs");
    fs::remove_file(project.path().join("README.md")).unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok(), "Should succeed without docs");
    let data = result.unwrap();
    assert_eq!(data.docs_content, "", "Docs content should be empty");
}

#[tokio::test]
async fn test_collect_handles_no_license() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    // Create minimal project without license
    fs::write(
        base.join("pyproject.toml"),
        r#"[project]
name = "nolicense"
version = "1.0.0"
"#,
    )
    .unwrap();

    let pkg_dir = base.join("nolicense");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "pass").unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(data.license, None, "License should be None");
}

#[tokio::test]
async fn test_collect_handles_no_project_urls() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    fs::write(
        base.join("pyproject.toml"),
        r#"[project]
name = "nourls"
version = "1.0.0"
"#,
    )
    .unwrap();

    let pkg_dir = base.join("nourls");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "pass").unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();
    assert!(data.project_urls.is_empty(), "Project URLs should be empty");
}

#[tokio::test]
async fn test_collect_fails_with_no_source_files() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    // Create only non-Python files - this will fail to find source files
    // Note: The PythonHandler.find_source_files() looks in src/ and repo root
    // for .py files, so we need to ensure there are none
    fs::write(base.join("README.md"), "# No Python here").unwrap();
    fs::write(base.join("config.toml"), "key = 'value'").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_err(), "Should fail without source files");
    let err = result.unwrap_err();
    // Could fail on either "no tests" or "no source" depending on order
    assert!(
        err.to_string().contains("No Python source files found")
            || err.to_string().contains("No tests found"),
        "Error should mention missing Python files: {}",
        err
    );
}

#[tokio::test]
async fn test_collect_fails_with_no_tests() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    // Create only source, no tests
    let pkg_dir = base.join("pkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_err(), "Should fail without tests");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("No tests found"),
        "Error should mention missing tests: {}",
        err
    );
}

#[tokio::test]
async fn test_collect_handles_invalid_path() {
    // Arrange
    let invalid_path = Path::new("/nonexistent/path/that/does/not/exist");
    let collector = Collector::new(invalid_path, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_err(), "Should fail with invalid path");
}

#[tokio::test]
async fn test_collect_respects_character_limits() {
    // Arrange
    let project = create_test_python_project("limitpkg");

    // Create many large example files to test truncation
    let examples_dir = project.path().join("examples");
    for i in 0..100 {
        let content = format!("# Example {}\n{}\n", i, "x".repeat(1000));
        fs::write(examples_dir.join(format!("example_{}.py", i)), content).unwrap();
    }

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();

    // Examples should be limited to ~30K chars
    assert!(
        data.examples_content.len() <= 35_000,
        "Examples content should be limited (got {} chars)",
        data.examples_content.len()
    );

    // Tests should be limited to ~30K chars
    assert!(
        data.test_content.len() <= 35_000,
        "Test content should be limited (got {} chars)",
        data.test_content.len()
    );
}

#[tokio::test]
async fn test_collect_handles_empty_files() {
    // Arrange
    let project = create_test_python_project("emptypkg");

    // Create empty files
    let pkg_dir = project.path().join("emptypkg");
    fs::write(pkg_dir.join("empty.py"), "").unwrap();

    let examples_dir = project.path().join("examples");
    fs::write(examples_dir.join("empty_example.py"), "").unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok(), "Should handle empty files gracefully");
}

#[tokio::test]
async fn test_collect_prioritizes_public_api_files() {
    // Arrange
    let project = create_test_python_project("apipkg");
    let pkg_dir = project.path().join("apipkg");

    // Create nested structure to test prioritization
    let submodule_dir = pkg_dir.join("internal").join("deep");
    fs::create_dir_all(&submodule_dir).unwrap();

    // Deep file (low priority)
    fs::write(
        submodule_dir.join("impl.py"),
        "# Deep implementation detail\n".repeat(1000),
    )
    .unwrap();

    // Top-level __init__.py should be prioritized
    fs::write(
        pkg_dir.join("__init__.py"),
        r#""""Public API - should be read first."""
__version__ = "1.0.0"

from .core import Calculator

__all__ = ['Calculator', 'hello']

def hello():
    """Public hello function."""
    return "Hello"
"#,
    )
    .unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();

    // Should contain __init__.py content
    assert!(
        data.source_content.contains("Public API"),
        "Should include high-priority public API file"
    );
    assert!(
        data.source_content.contains("public API") || data.source_content.contains("__init__.py"),
        "Should label high-priority files"
    );
}

#[tokio::test]
async fn test_collected_data_struct_validation() {
    // Arrange
    let project = create_test_python_project("validpkg");
    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let data = collector.collect().await.unwrap();

    // Assert - validate all fields
    assert!(
        !data.package_name.is_empty(),
        "Package name should not be empty"
    );
    assert!(!data.version.is_empty(), "Version should not be empty");
    assert_eq!(data.language, Language::Python);

    // All content fields should be initialized (may be empty)
    assert_ne!(data.examples_content, "uninitialized");
    assert_ne!(data.test_content, "uninitialized");
    assert_ne!(data.docs_content, "uninitialized");
    assert_ne!(data.source_content, "uninitialized");
    assert_ne!(data.changelog_content, "uninitialized");
}

#[tokio::test]
async fn test_collect_package_name_from_pyproject() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    fs::write(
        base.join("pyproject.toml"),
        r#"[project]
name = "my-awesome-package"
version = "2.0.0"
"#,
    )
    .unwrap();

    let pkg_dir = base.join("my_awesome_package");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "pass").unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(data.package_name, "my-awesome-package");
}

#[tokio::test]
async fn test_collect_package_name_from_setup_py() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    fs::write(
        base.join("setup.py"),
        r#"from setuptools import setup

setup(
    name='setup-package',
    version='3.0.0',
)
"#,
    )
    .unwrap();

    let pkg_dir = base.join("setup_package");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "pass").unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(data.package_name, "setup-package");
}

#[tokio::test]
async fn test_collect_package_name_from_setup_py_double_quotes() {
    // Arrange - setup.py with double-quoted name (no pyproject.toml)
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    fs::write(
        base.join("setup.py"),
        r#"from setuptools import setup

setup(
    name="my-package",
    version="2.0.0",
)
"#,
    )
    .unwrap();

    let pkg_dir = base.join("my_package");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "pass").unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(data.package_name, "my-package");
}

#[tokio::test]
async fn test_collect_package_name_from_directory() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    // Create a subdirectory with a known name
    let project_dir = base.join("my-project-dir");
    fs::create_dir_all(&project_dir).unwrap();

    let pkg_dir = project_dir.join("src");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("main.py"), "pass").unwrap();

    let tests_dir = project_dir.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(&project_dir, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(data.package_name, "my-project-dir");
}

#[tokio::test]
async fn test_collect_version_from_pyproject() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    fs::write(
        base.join("pyproject.toml"),
        r#"[project]
name = "verpkg"
version = "4.5.6"
"#,
    )
    .unwrap();

    let pkg_dir = base.join("verpkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "pass").unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(data.version, "4.5.6");
}

#[tokio::test]
async fn test_collect_handles_truncated_files() {
    // Arrange
    let project = create_test_python_project("truncpkg");

    // Create a very large changelog
    let changelog = "# Change\n".repeat(10_000);
    fs::write(project.path().join("CHANGELOG.md"), changelog).unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();

    // Changelog should be truncated to 5K chars
    assert!(
        data.changelog_content.len() <= 5_000,
        "Changelog should be limited to 5K chars (got {})",
        data.changelog_content.len()
    );
}

#[tokio::test]
async fn test_collector_new() {
    // Arrange
    let path = Path::new("/tmp/test");
    let language = Language::Python;

    // Act
    let collector = Collector::new(path, language.clone());

    // Assert - verify collector is created (implicit by no panic)
    // This tests the constructor works correctly
    let _ = collector;
}

#[test]
fn test_collected_data_clone() {
    // Arrange
    let data = CollectedData {
        package_name: "test".to_string(),
        version: "1.0.0".to_string(),
        license: Some("MIT".to_string()),
        project_urls: vec![("Homepage".to_string(), "http://example.com".to_string())],
        language: Language::Python,
        examples_content: "examples".to_string(),
        test_content: "tests".to_string(),
        docs_content: "docs".to_string(),
        source_content: "source".to_string(),
        changelog_content: "changelog".to_string(),
        source_file_count: 1,
    };

    // Act
    let cloned = data.clone();

    // Assert
    assert_eq!(cloned.package_name, data.package_name);
    assert_eq!(cloned.version, data.version);
    assert_eq!(cloned.license, data.license);
    assert_eq!(cloned.language, data.language);
}

#[test]
fn test_collected_data_debug() {
    // Arrange
    let data = CollectedData {
        package_name: "debugtest".to_string(),
        version: "1.0.0".to_string(),
        license: None,
        project_urls: vec![],
        language: Language::Python,
        examples_content: String::new(),
        test_content: String::new(),
        docs_content: String::new(),
        source_content: String::new(),
        changelog_content: String::new(),
        source_file_count: 0,
    };

    // Act
    let debug_str = format!("{:?}", data);

    // Assert
    assert!(debug_str.contains("debugtest"));
    assert!(debug_str.contains("1.0.0"));
}

#[tokio::test]
async fn test_collect_with_multiple_file_types_in_examples() {
    // Arrange
    let project = create_test_python_project("multipkg");
    let examples_dir = project.path().join("examples");

    // Add multiple example files
    for i in 1..=5 {
        fs::write(
            examples_dir.join(format!("example_{}.py", i)),
            format!("# Example {}\nprint('Example {}')", i, i),
        )
        .unwrap();
    }

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();

    // Should have content from multiple example files
    assert!(data.examples_content.contains("example_1.py"));
    assert!(
        data.examples_content.contains("Example 1") || data.examples_content.contains("Example 2")
    );
}

#[tokio::test]
async fn test_collect_file_truncation_in_middle_of_file() {
    // Arrange
    let project = create_test_python_project("truncmid");

    // Create a single very large file that will be truncated
    let examples_dir = project.path().join("examples");
    let large_content = "# Line\n".repeat(5000);
    fs::write(examples_dir.join("large.py"), large_content).unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();

    // File should be marked as truncated
    assert!(data.examples_content.contains("large.py"));
}

#[tokio::test]
async fn test_collect_handles_nested_source_structure() {
    // Arrange
    let project = create_test_python_project("nested");
    let pkg_dir = project.path().join("nested");

    // Create deep nested structure
    let deep_dir = pkg_dir.join("level1").join("level2").join("level3");
    fs::create_dir_all(&deep_dir).unwrap();

    fs::write(
        deep_dir.join("deep_module.py"),
        "# Deep module\ndef deep_function(): pass",
    )
    .unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();

    // Should include nested files but prioritize top-level
    assert!(!data.source_content.is_empty());
}

#[tokio::test]
async fn test_collect_package_name_fallback_to_unknown() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    // Create an unusual structure where we can't detect package name
    // Use a "." path which is excluded from package name detection
    let pkg_dir = base.join("src");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("main.py"), "pass").unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();
    // Should fall back to directory name or "unknown"
    assert!(!data.package_name.is_empty());
}

#[tokio::test]
async fn test_collect_with_unreadable_file() {
    // Arrange
    let project = create_test_python_project("unreadable");
    let examples_dir = project.path().join("examples");

    // Create a file with good content
    fs::write(
        examples_dir.join("good.py"),
        "# Good example\nprint('works')",
    )
    .unwrap();

    // Create a directory with .py extension (can't be read as file)
    fs::create_dir_all(examples_dir.join("baddir.py")).unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok(), "Should handle unreadable files gracefully");
    let data = result.unwrap();

    // Should still have readable content
    assert!(data.examples_content.contains("good.py"));
}

#[tokio::test]
async fn test_collect_smart_file_reading_priority() {
    // Arrange
    let project = create_test_python_project("priority");
    let pkg_dir = project.path().join("priority");

    // Create files at different levels to test prioritization
    // High priority: __init__.py
    fs::write(
        pkg_dir.join("__init__.py"),
        "# High priority public API\n".repeat(100),
    )
    .unwrap();

    // Medium priority: top-level module
    fs::write(
        pkg_dir.join("api.py"),
        "# Medium priority module\n".repeat(100),
    )
    .unwrap();

    // Low priority: deep implementation
    let impl_dir = pkg_dir.join("internal").join("impl").join("deep");
    fs::create_dir_all(&impl_dir).unwrap();
    fs::write(
        impl_dir.join("details.py"),
        "# Low priority implementation\n".repeat(1000),
    )
    .unwrap();

    let collector = Collector::new(project.path(), Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();

    // Should prioritize high-priority files
    assert!(
        data.source_content.contains("public API") || data.source_content.contains("__init__.py")
    );
}

#[tokio::test]
async fn test_collect_version_fallback_to_latest() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    // Create project without version info
    fs::write(base.join("setup.py"), "# No version here").unwrap();

    let pkg_dir = base.join("pkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "# No version").unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(
        data.version, "latest",
        "Should fallback to 'latest' version"
    );
}

#[tokio::test]
async fn test_collect_handles_all_optional_content_missing() {
    // Arrange
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    // Minimal project: just source and tests
    let pkg_dir = base.join("minimal");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "pass").unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_basic.py"), "def test_pass(): pass").unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let result = collector.collect().await;

    // Assert
    assert!(result.is_ok());
    let data = result.unwrap();

    // All optional fields should be empty but present
    assert_eq!(data.examples_content, "");
    assert_eq!(data.docs_content, "");
    assert_eq!(data.changelog_content, "");
    assert_eq!(data.license, None);
    assert!(data.project_urls.is_empty());

    // Required fields should have content
    assert!(!data.source_content.is_empty());
    assert!(!data.test_content.is_empty());
    assert!(!data.package_name.is_empty());
    assert!(!data.version.is_empty());
}

#[tokio::test]
async fn test_collect_returns_package_metadata() {
    // Arrange: minimal Python project with pyproject.toml name/version and one .py file
    let dir = TempDir::new().unwrap();
    let base = dir.path();

    fs::write(
        base.join("pyproject.toml"),
        r#"[project]
name = "metadata-pkg"
version = "3.2.1"
license = "Apache-2.0"

[project.urls]
Repository = "https://github.com/example/metadata-pkg"
"#,
    )
    .unwrap();

    let pkg_dir = base.join("metadata_pkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(
        pkg_dir.join("__init__.py"),
        r#""""metadata_pkg init."""
__version__ = "3.2.1"

def greet():
    return "hi"
"#,
    )
    .unwrap();

    let tests_dir = base.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    fs::write(
        tests_dir.join("test_greet.py"),
        "def test_greet(): assert True",
    )
    .unwrap();

    let collector = Collector::new(base, Language::Python);

    // Act
    let data = collector.collect().await.expect("collect() should succeed");

    // Assert: verify all CollectedData fields
    assert_eq!(data.package_name, "metadata-pkg");
    assert_eq!(data.version, "3.2.1");
    assert_eq!(data.license, Some("Apache-2.0".to_string()));
    assert_eq!(data.language, Language::Python);
    assert!(data.source_file_count > 0, "Should count source files");
    assert!(
        data.project_urls
            .iter()
            .any(|(k, v)| k == "Repository" && v == "https://github.com/example/metadata-pkg"),
        "Should include project URL"
    );
    assert!(
        !data.source_content.is_empty(),
        "Should have source content from __init__.py"
    );
    assert!(
        !data.test_content.is_empty(),
        "Should have test content from test_greet.py"
    );
}
