//! Unit tests for pipeline data collector
//! Tests data collection from Python packages including:
//! - Source file discovery
//! - Test file discovery
//! - Version detection
//! - License extraction
//! - Documentation collection

use anyhow::Result;
use skilldo::detector::Language;
use skilldo::pipeline::collector::{CollectedData, Collector};
use std::fs;
use tempfile::TempDir;

/// Helper to create a test Python package structure
fn create_test_package(temp_dir: &TempDir) -> Result<()> {
    let pkg_dir = temp_dir.path().join("test_pkg");
    fs::create_dir_all(&pkg_dir)?;

    // Create __init__.py
    fs::write(
        pkg_dir.join("__init__.py"),
        r#"
"""Test package for unit testing."""
__version__ = "1.2.3"

def hello():
    """Say hello."""
    print("Hello")
"#,
    )?;

    // Create pyproject.toml (preferred way to specify metadata)
    fs::write(
        temp_dir.path().join("pyproject.toml"),
        r#"[project]
name = "test-package"
version = "1.2.3"
license = { text = "MIT" }
readme = "README.md"

[project.urls]
Homepage = "https://example.com"
Documentation = "https://docs.example.com"
"#,
    )?;

    // Create README
    fs::write(
        temp_dir.path().join("README.md"),
        r#"
# Test Package

A test package for unit testing the collector.

## Installation

```bash
pip install test-package
```

## Usage

```python
from test_pkg import hello
hello()
```
"#,
    )?;

    // Create tests directory
    let tests_dir = temp_dir.path().join("tests");
    fs::create_dir_all(&tests_dir)?;
    fs::write(
        tests_dir.join("test_hello.py"),
        r#"
from test_pkg import hello

def test_hello():
    hello()
    assert True
"#,
    )?;

    // Create examples directory
    let examples_dir = temp_dir.path().join("examples");
    fs::create_dir_all(&examples_dir)?;
    fs::write(
        examples_dir.join("basic.py"),
        r#"
from test_pkg import hello

if __name__ == "__main__":
    hello()
"#,
    )?;

    Ok(())
}

#[tokio::test]
async fn test_collector_discovers_python_package() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_test_package(&temp_dir)?;

    let collector = Collector::new(temp_dir.path(), Language::Python);
    let data: CollectedData = collector.collect().await?;

    // Should detect package name
    assert_eq!(data.package_name, "test-package");

    // Should detect version
    assert_eq!(data.version, "1.2.3");

    // Should detect language
    assert_eq!(data.language.as_str(), "python");

    // Should have source content
    assert!(data.source_content.contains("def hello()"));

    // Should have test content
    assert!(data.test_content.contains("test_hello"));

    // Should have docs content (README)
    assert!(data.docs_content.contains("# Test Package"));

    // Should have examples content
    assert!(data.examples_content.contains("if __name__ =="));

    Ok(())
}

#[tokio::test]
async fn test_collector_extracts_license() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_test_package(&temp_dir)?;

    let collector = Collector::new(temp_dir.path(), Language::Python);
    let data = collector.collect().await?;

    // Should extract license from setup.py
    assert_eq!(data.license, Some("MIT".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_collector_extracts_project_urls() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_test_package(&temp_dir)?;

    let collector = Collector::new(temp_dir.path(), Language::Python);
    let data = collector.collect().await?;

    // Should extract URL from pyproject.toml
    eprintln!("project_urls: {:?}", data.project_urls);
    assert!(!data.project_urls.is_empty(), "URLs should not be empty");

    // Find homepage URL
    let homepage = data
        .project_urls
        .iter()
        .find(|(key, _)| key.to_lowercase().contains("home") || key.to_lowercase().contains("url"));

    assert!(
        homepage.is_some(),
        "Should have extracted Homepage URL: {:?}",
        data.project_urls
    );

    Ok(())
}

#[tokio::test]
async fn test_collector_requires_tests() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let pkg_dir = temp_dir.path().join("minimal_pkg");
    fs::create_dir_all(&pkg_dir)?;

    // Just __init__.py and pyproject.toml, no tests
    fs::write(
        pkg_dir.join("__init__.py"),
        r#"
__version__ = "0.1.0"

def main():
    pass
"#,
    )?;

    fs::write(
        temp_dir.path().join("pyproject.toml"),
        r#"
[project]
name = "minimal-pkg"
version = "0.1.0"
"#,
    )?;

    let collector = Collector::new(temp_dir.path(), Language::Python);
    let result = collector.collect().await;

    // Should error when no tests found
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No tests found"));

    Ok(())
}

#[tokio::test]
async fn test_collector_respects_max_source_tokens() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let pkg_dir = temp_dir.path().join("large_pkg");
    fs::create_dir_all(&pkg_dir)?;

    // Create a very large source file
    let large_content = "# ".repeat(100000); // ~200KB of comments
    fs::write(
        pkg_dir.join("__init__.py"),
        format!("__version__ = '1.0.0'\n{}", large_content),
    )?;

    // Add required pyproject.toml
    fs::write(
        temp_dir.path().join("pyproject.toml"),
        "[project]\nname = \"large-pkg\"\nversion = \"1.0.0\"\n",
    )?;

    // Add required tests directory
    let tests_dir = temp_dir.path().join("tests");
    fs::create_dir_all(&tests_dir)?;
    fs::write(
        tests_dir.join("test_simple.py"),
        "def test_pass():\n    assert True\n",
    )?;

    let collector = Collector::new(temp_dir.path(), Language::Python);
    let data = collector.collect().await?;

    // Source should be limited by token budget (15K chars for source)
    assert!(
        data.source_content.len() <= 20000,
        "Source should respect token budget of ~15K chars"
    );

    // Should still have version
    assert_eq!(data.version, "1.0.0");

    Ok(())
}

#[tokio::test]
async fn test_collector_prefers_examples_over_tests() -> Result<()> {
    let temp_dir = TempDir::new()?;
    create_test_package(&temp_dir)?;

    let collector = Collector::new(temp_dir.path(), Language::Python);
    let data = collector.collect().await?;

    // Examples should be populated
    assert!(!data.examples_content.is_empty());

    // Tests should also be populated
    assert!(!data.test_content.is_empty());

    // Both should contain Python code
    assert!(data.examples_content.contains("from test_pkg import"));
    assert!(data.test_content.contains("def test_"));

    Ok(())
}
