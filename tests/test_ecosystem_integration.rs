//! Integration tests for the ecosystem detection → collection pipeline.
//! Tests the full flow: detect_language → Collector → CollectedData.

use anyhow::Result;
use skilldo::detector::{detect_language, Language};
use skilldo::pipeline::collector::Collector;
use std::fs;
use tempfile::TempDir;

// ── Detection → Collection flow ──────────────────────────────────────────

#[tokio::test]
async fn test_detect_then_collect_python_project() -> Result<()> {
    let tmp = TempDir::new()?;

    // Create a minimal Python project
    fs::write(
        tmp.path().join("pyproject.toml"),
        r#"[project]
name = "my-package"
version = "1.0.0"
license = { text = "MIT" }
"#,
    )?;

    let pkg_dir = tmp.path().join("my_package");
    fs::create_dir_all(&pkg_dir)?;
    fs::write(pkg_dir.join("__init__.py"), "__version__ = '1.0.0'")?;
    fs::write(pkg_dir.join("core.py"), "def hello(): return 'world'")?;

    let tests_dir = tmp.path().join("tests");
    fs::create_dir_all(&tests_dir)?;
    fs::write(
        tests_dir.join("test_core.py"),
        "from my_package.core import hello\ndef test_hello():\n    assert hello() == 'world'",
    )?;

    // Step 1: Detect language
    let language = detect_language(tmp.path())?;
    assert_eq!(language, Language::Python);

    // Step 2: Collect data
    let collector = Collector::new(tmp.path(), language);
    let data = collector.collect().await?;

    // Verify collected data
    assert_eq!(data.version, "1.0.0");
    assert_eq!(data.license, Some("MIT".to_string()));
    assert_eq!(data.language, Language::Python);
    assert!(data.source_file_count >= 2);
    assert!(data.source_content.contains("def hello"));
    assert!(data.test_content.contains("test_hello"));

    Ok(())
}

#[tokio::test]
async fn test_detect_then_collect_with_budget() -> Result<()> {
    let tmp = TempDir::new()?;

    fs::write(
        tmp.path().join("pyproject.toml"),
        "[project]\nname = \"test\"\nversion = \"0.1.0\"",
    )?;

    let pkg_dir = tmp.path().join("test_pkg");
    fs::create_dir_all(&pkg_dir)?;
    fs::write(pkg_dir.join("__init__.py"), "")?;
    // Create a large source file
    let large_content = "x = 1\n".repeat(10_000);
    fs::write(pkg_dir.join("big.py"), &large_content)?;

    let tests_dir = tmp.path().join("tests");
    fs::create_dir_all(&tests_dir)?;
    fs::write(tests_dir.join("test_it.py"), "def test(): pass")?;

    let language = detect_language(tmp.path())?;
    let collector = Collector::new(tmp.path(), language).with_max_source_chars(500);
    let data = collector.collect().await?;

    // With a 500 char budget, total collected content must not exceed it
    let total = data.source_content.len() + data.test_content.len() + data.examples_content.len();
    assert!(
        total <= 500,
        "Total content should respect budget of 500 (got {total} chars)"
    );

    Ok(())
}

// ── Unsupported languages bail cleanly ───────────────────────────────────

#[tokio::test]
async fn test_collect_javascript_not_yet_supported() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("package.json"), "{}").unwrap();

    let language = detect_language(tmp.path()).unwrap();
    assert_eq!(language, Language::JavaScript);

    let collector = Collector::new(tmp.path(), language);
    let result = collector.collect().await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not yet implemented"),
        "Expected 'not yet implemented' error, got: {}",
        err
    );
}

#[tokio::test]
async fn test_collect_rust_not_yet_supported() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

    let language = detect_language(tmp.path()).unwrap();
    assert_eq!(language, Language::Rust);

    let collector = Collector::new(tmp.path(), language);
    let result = collector.collect().await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("not yet implemented"));
}

#[tokio::test]
async fn test_collect_go_not_yet_supported() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("go.mod"), "module test").unwrap();

    let language = detect_language(tmp.path()).unwrap();
    assert_eq!(language, Language::Go);

    let collector = Collector::new(tmp.path(), language);
    let result = collector.collect().await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("not yet implemented"));
}

// ── Python edge cases ───────────────────────────────────────────────────

#[tokio::test]
async fn test_python_no_source_files_errors() {
    let tmp = TempDir::new().unwrap();
    // pyproject.toml exists but no .py files at all
    fs::write(
        tmp.path().join("pyproject.toml"),
        "[project]\nname = \"empty\"\nversion = \"0.1.0\"",
    )
    .unwrap();

    let collector = Collector::new(tmp.path(), Language::Python);
    let result = collector.collect().await;

    // Should fail because no source files found
    assert!(result.is_err());
}

#[tokio::test]
async fn test_python_no_tests_errors() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("pyproject.toml"),
        "[project]\nname = \"test\"\nversion = \"0.1.0\"",
    )
    .unwrap();

    let pkg_dir = tmp.path().join("test_pkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("__init__.py"), "").unwrap();
    fs::write(pkg_dir.join("core.py"), "x = 1").unwrap();

    // No tests directory
    let collector = Collector::new(tmp.path(), Language::Python);
    let result = collector.collect().await;

    // Should fail because no test files found
    assert!(result.is_err());
}

#[tokio::test]
async fn test_python_with_examples_and_docs() -> Result<()> {
    let tmp = TempDir::new()?;

    fs::write(
        tmp.path().join("pyproject.toml"),
        "[project]\nname = \"mylib\"\nversion = \"2.0.0\"",
    )?;

    let pkg_dir = tmp.path().join("mylib");
    fs::create_dir_all(&pkg_dir)?;
    fs::write(pkg_dir.join("__init__.py"), "__version__ = '2.0.0'")?;
    fs::write(
        pkg_dir.join("api.py"),
        "def create(): pass\ndef read(): pass",
    )?;

    let tests_dir = tmp.path().join("tests");
    fs::create_dir_all(&tests_dir)?;
    fs::write(tests_dir.join("test_api.py"), "def test_create(): pass")?;

    let examples_dir = tmp.path().join("examples");
    fs::create_dir_all(&examples_dir)?;
    fs::write(
        examples_dir.join("basic.py"),
        "from mylib import create\ncreate()",
    )?;

    fs::write(
        tmp.path().join("README.md"),
        "# mylib\nA library for things.",
    )?;
    fs::write(
        tmp.path().join("CHANGELOG.md"),
        "## 2.0.0\n- Initial release",
    )?;

    let collector = Collector::new(tmp.path(), Language::Python);
    let data = collector.collect().await?;

    assert_eq!(data.version, "2.0.0");
    assert!(!data.examples_content.is_empty(), "Should have examples");
    assert!(!data.docs_content.is_empty(), "Should have docs");
    assert!(!data.changelog_content.is_empty(), "Should have changelog");
    assert!(data.source_content.contains("def create"));
    assert!(data.examples_content.contains("from mylib import create"));

    Ok(())
}

#[tokio::test]
async fn test_python_project_urls_flow() -> Result<()> {
    let tmp = TempDir::new()?;

    fs::write(
        tmp.path().join("pyproject.toml"),
        r#"[project]
name = "urltest"
version = "1.0.0"

[project.urls]
Homepage = "https://example.com"
Documentation = "https://docs.example.com"
"#,
    )?;

    let pkg_dir = tmp.path().join("urltest");
    fs::create_dir_all(&pkg_dir)?;
    fs::write(pkg_dir.join("__init__.py"), "")?;
    fs::write(pkg_dir.join("main.py"), "x = 1")?;

    let tests_dir = tmp.path().join("tests");
    fs::create_dir_all(&tests_dir)?;
    fs::write(tests_dir.join("test_main.py"), "def test(): pass")?;

    let collector = Collector::new(tmp.path(), Language::Python);
    let data = collector.collect().await?;

    assert_eq!(data.project_urls.len(), 2);
    assert!(data.project_urls.iter().any(|(k, _)| k == "Homepage"));
    assert!(data.project_urls.iter().any(|(k, _)| k == "Documentation"));

    Ok(())
}

// ── Detection precedence integration (detect + verify language field) ──

#[tokio::test]
async fn test_collected_data_has_correct_language_field() -> Result<()> {
    let tmp = TempDir::new()?;

    fs::write(
        tmp.path().join("pyproject.toml"),
        "[project]\nname = \"test\"\nversion = \"1.0.0\"",
    )?;

    let pkg_dir = tmp.path().join("test_pkg");
    fs::create_dir_all(&pkg_dir)?;
    fs::write(pkg_dir.join("__init__.py"), "")?;
    fs::write(pkg_dir.join("core.py"), "x = 1")?;

    let tests_dir = tmp.path().join("tests");
    fs::create_dir_all(&tests_dir)?;
    fs::write(tests_dir.join("test_core.py"), "def test(): pass")?;

    let language = detect_language(tmp.path())?;
    let collector = Collector::new(tmp.path(), language);
    let data = collector.collect().await?;

    assert_eq!(data.language, Language::Python);
    assert_eq!(data.language.as_str(), "python");

    Ok(())
}
