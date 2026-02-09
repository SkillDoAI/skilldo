//! Unit tests for Python ecosystem handler
//! Tests Python-specific file detection and metadata extraction

use anyhow::Result;
use skilldo::ecosystems::python::PythonHandler;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_python_handler_finds_source_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let pkg_dir = temp_dir.path().join("mypkg");
    fs::create_dir_all(&pkg_dir)?;

    // Create some Python source files
    fs::write(pkg_dir.join("__init__.py"), "# Package init")?;
    fs::write(pkg_dir.join("module.py"), "def foo(): pass")?;

    let handler = PythonHandler::new(temp_dir.path());
    let sources = handler.find_source_files()?;

    assert!(!sources.is_empty(), "Should find source files");
    assert!(sources
        .iter()
        .any(|p| p.to_str().unwrap().contains("__init__.py")));

    Ok(())
}

#[test]
fn test_python_handler_finds_tests() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tests_dir = temp_dir.path().join("tests");
    fs::create_dir_all(&tests_dir)?;

    // Create test files
    fs::write(tests_dir.join("test_foo.py"), "def test_foo(): pass")?;
    fs::write(tests_dir.join("test_bar.py"), "def test_bar(): pass")?;

    let handler = PythonHandler::new(temp_dir.path());
    let tests = handler.find_test_files()?;

    assert_eq!(tests.len(), 2, "Should find 2 test files");

    Ok(())
}

#[test]
fn test_python_handler_finds_docs() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create README and docs
    fs::write(temp_dir.path().join("README.md"), "# My Package")?;
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir_all(&docs_dir)?;
    fs::write(docs_dir.join("guide.md"), "# Guide")?;

    let handler = PythonHandler::new(temp_dir.path());
    let docs = handler.find_docs()?;

    assert!(!docs.is_empty(), "Should find documentation files");
    assert!(docs.iter().any(|p| p.to_str().unwrap().contains("README")));

    Ok(())
}

#[test]
fn test_python_handler_finds_examples() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let examples_dir = temp_dir.path().join("examples");
    fs::create_dir_all(&examples_dir)?;

    fs::write(examples_dir.join("example1.py"), "print('hello')")?;
    fs::write(examples_dir.join("example2.py"), "print('world')")?;

    let handler = PythonHandler::new(temp_dir.path());
    let examples = handler.find_examples()?;

    assert_eq!(examples.len(), 2, "Should find 2 example files");

    Ok(())
}

#[test]
fn test_python_handler_extracts_version_from_pyproject() -> Result<()> {
    let temp_dir = TempDir::new()?;

    fs::write(
        temp_dir.path().join("pyproject.toml"),
        r#"[project]
name = "my-package"
version = "2.3.4"
"#,
    )?;

    let handler = PythonHandler::new(temp_dir.path());
    let version = handler.get_version()?;

    assert_eq!(version, "2.3.4");

    Ok(())
}

#[test]
fn test_python_handler_extracts_version_from_init() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Handler looks for <repo_name>/<repo_name>/__init__.py
    // So if repo is /tmp/abc, it looks for /tmp/abc/abc/__init__.py
    let repo_name = temp_dir.path().file_name().unwrap().to_str().unwrap();
    let pkg_dir = temp_dir.path().join(repo_name);
    fs::create_dir_all(&pkg_dir)?;

    fs::write(
        pkg_dir.join("__init__.py"),
        r#"
"""My package"""
__version__ = "1.2.3"
"#,
    )?;

    let handler = PythonHandler::new(temp_dir.path());
    let version = handler.get_version()?;

    assert_eq!(version, "1.2.3");

    Ok(())
}

#[test]
fn test_python_handler_extracts_license() -> Result<()> {
    let temp_dir = TempDir::new()?;

    fs::write(
        temp_dir.path().join("pyproject.toml"),
        r#"[project]
name = "my-package"
version = "1.0.0"
license = { text = "Apache-2.0" }
"#,
    )?;

    let handler = PythonHandler::new(temp_dir.path());
    let license = handler.get_license();

    assert_eq!(license, Some("Apache-2.0".to_string()));

    Ok(())
}

#[test]
fn test_python_handler_extracts_project_urls() -> Result<()> {
    let temp_dir = TempDir::new()?;

    fs::write(
        temp_dir.path().join("pyproject.toml"),
        r#"[project]
name = "my-package"
version = "1.0.0"

[project.urls]
Homepage = "https://example.com"
Repository = "https://github.com/example/repo"
"#,
    )?;

    let handler = PythonHandler::new(temp_dir.path());
    let urls = handler.get_project_urls();

    assert_eq!(urls.len(), 2);
    assert!(urls
        .iter()
        .any(|(k, v)| k == "Homepage" && v == "https://example.com"));
    assert!(urls
        .iter()
        .any(|(k, v)| k == "Repository" && v.contains("github.com")));

    Ok(())
}

#[test]
fn test_python_handler_finds_changelog() -> Result<()> {
    let temp_dir = TempDir::new()?;

    fs::write(
        temp_dir.path().join("CHANGELOG.md"),
        "# Changelog\n\n## 1.0.0",
    )?;

    let handler = PythonHandler::new(temp_dir.path());
    let changelog = handler.find_changelog();

    assert!(changelog.is_some(), "Should find CHANGELOG.md");

    Ok(())
}
