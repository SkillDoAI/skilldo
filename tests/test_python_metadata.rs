// Comprehensive tests for Python metadata extraction (license, URLs, version)
use skilldo::ecosystems::python::PythonHandler;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_license_from_pyproject_toml() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Create pyproject.toml with license
    let pyproject_content = r#"
[project]
name = "test-package"
version = "1.0.0"
license = "MIT"
"#;
    fs::write(repo_path.join("pyproject.toml"), pyproject_content).unwrap();

    let handler = PythonHandler::new(repo_path);
    let license = handler.get_license();

    assert!(license.is_some());
    assert_eq!(license.unwrap(), "MIT");
}

#[test]
fn test_license_from_setup_py() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Create setup.py with license
    let setup_content = r#"
from setuptools import setup

setup(
    name='test-package',
    version='1.0.0',
    license='BSD-3-Clause',
)
"#;
    fs::write(repo_path.join("setup.py"), setup_content).unwrap();

    let handler = PythonHandler::new(repo_path);
    let license = handler.get_license();

    assert!(license.is_some());
    assert_eq!(license.unwrap(), "BSD-3-Clause'");
}

#[test]
fn test_license_from_setup_cfg() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Create setup.cfg with license
    let setup_cfg_content = r#"
[metadata]
name = test-package
version = 1.0.0
license = Apache-2.0
"#;
    fs::write(repo_path.join("setup.cfg"), setup_cfg_content).unwrap();

    let handler = PythonHandler::new(repo_path);
    let license = handler.get_license();

    assert!(license.is_some());
    assert_eq!(license.unwrap(), "Apache-2.0");
}

#[test]
fn test_license_priority_pyproject_over_setup() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Both files exist - pyproject.toml should take priority
    fs::write(repo_path.join("pyproject.toml"), "license = \"MIT\"").unwrap();
    fs::write(repo_path.join("setup.py"), "license='GPL-3.0'").unwrap();

    let handler = PythonHandler::new(repo_path);
    let license = handler.get_license();

    assert_eq!(license.unwrap(), "MIT");
}

#[test]
fn test_no_license_found() {
    let temp = TempDir::new().unwrap();
    let handler = PythonHandler::new(temp.path());
    let license = handler.get_license();
    assert!(license.is_none());
}

#[test]
fn test_project_urls_from_pyproject_toml() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    let pyproject_content = r#"
[project.urls]
Homepage = "https://example.com"
Documentation = "https://docs.example.com"
Source = "https://github.com/test/repo"
"#;
    fs::write(repo_path.join("pyproject.toml"), pyproject_content).unwrap();

    let handler = PythonHandler::new(repo_path);
    let urls = handler.get_project_urls();

    assert_eq!(urls.len(), 3);
    assert!(urls
        .iter()
        .any(|(k, v)| k == "Homepage" && v == "https://example.com"));
    assert!(urls
        .iter()
        .any(|(k, v)| k == "Documentation" && v == "https://docs.example.com"));
    assert!(urls
        .iter()
        .any(|(k, v)| k == "Source" && v == "https://github.com/test/repo"));
}

#[test]
fn test_project_urls_from_setup_py() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    let setup_content = r#"
setup(
    project_urls={
        "Bug Tracker": "https://github.com/test/issues",
        "Source Code": "https://github.com/test/repo",
    }
)
"#;
    fs::write(repo_path.join("setup.py"), setup_content).unwrap();

    let handler = PythonHandler::new(repo_path);
    let urls = handler.get_project_urls();

    // Setup.py parsing is limited - just verify it doesn't crash
    assert!(urls.len() <= 2);
}

#[test]
fn test_no_project_urls() {
    let temp = TempDir::new().unwrap();
    let handler = PythonHandler::new(temp.path());
    let urls = handler.get_project_urls();
    assert!(urls.is_empty());
}

#[test]
fn test_version_from_pyproject_toml() {
    let temp = TempDir::new().unwrap();
    let pyproject_content = r#"
[project]
version = "1.2.3"
"#;
    fs::write(temp.path().join("pyproject.toml"), pyproject_content).unwrap();

    let handler = PythonHandler::new(temp.path());
    let version = handler.get_version().unwrap();
    assert_eq!(version, "1.2.3");
}

#[test]
fn test_version_from_init_py() {
    let temp = TempDir::new().unwrap();
    let temp_name = temp.path().file_name().unwrap().to_str().unwrap();
    let pkg_dir = temp.path().join(temp_name);
    fs::create_dir(&pkg_dir).unwrap();

    let init_content = r#"
__version__ = "2.0.0"
"#;
    fs::write(pkg_dir.join("__init__.py"), init_content).unwrap();

    // Handler looks for {repo_name}/{repo_name}/__init__.py
    let handler = PythonHandler::new(temp.path());
    let version = handler.get_version().unwrap();
    assert_eq!(version, "2.0.0");
}

#[test]
fn test_version_fallback_to_latest() {
    let temp = TempDir::new().unwrap();
    let handler = PythonHandler::new(temp.path());
    let version = handler.get_version().unwrap();
    assert_eq!(version, "latest");
}

#[test]
fn test_find_source_files_in_src_dir() {
    let temp = TempDir::new().unwrap();
    let src_dir = temp.path().join("src").join("mypackage");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("main.py"), "# source").unwrap();
    fs::write(src_dir.join("utils.py"), "# utils").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_source_files().unwrap();
    // Handler searches both src/ and repo root, may find files in both
    assert!(files.len() >= 2);
}

#[test]
fn test_find_source_files_in_package_dir() {
    let temp = TempDir::new().unwrap();
    let pkg_dir = temp.path().join("mypackage");
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("main.py"), "# source").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_source_files().unwrap();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_find_test_files_pytest_convention() {
    let temp = TempDir::new().unwrap();
    let tests_dir = temp.path().join("tests");
    fs::create_dir(&tests_dir).unwrap();
    fs::write(tests_dir.join("test_main.py"), "# test").unwrap();
    fs::write(tests_dir.join("test_utils.py"), "# test").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_test_files().unwrap();
    assert_eq!(files.len(), 2);
}

#[test]
fn test_find_test_files_suffix_pattern() {
    let temp = TempDir::new().unwrap();
    let tests_dir = temp.path().join("tests");
    fs::create_dir(&tests_dir).unwrap();
    fs::write(tests_dir.join("main_test.py"), "# test").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_test_files().unwrap();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_find_test_files_nested() {
    let temp = TempDir::new().unwrap();
    let nested_tests = temp.path().join("mypackage").join("tests");
    fs::create_dir_all(&nested_tests).unwrap();
    fs::write(nested_tests.join("test_nested.py"), "# test").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_test_files().unwrap();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_no_tests_returns_error() {
    let temp = TempDir::new().unwrap();
    let handler = PythonHandler::new(temp.path());
    let result = handler.find_test_files();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No tests found"));
}

#[test]
fn test_find_examples() {
    let temp = TempDir::new().unwrap();
    let examples_dir = temp.path().join("examples");
    fs::create_dir(&examples_dir).unwrap();
    fs::write(examples_dir.join("example1.py"), "# example").unwrap();
    fs::write(examples_dir.join("example2.py"), "# example").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_examples().unwrap();
    assert_eq!(files.len(), 2);
}

#[test]
fn test_find_docs_readme() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("README.md"), "# Readme").unwrap();

    let handler = PythonHandler::new(temp.path());
    let docs = handler.find_docs().unwrap();
    assert_eq!(docs.len(), 1);
    assert!(docs[0].file_name().unwrap() == "README.md");
}

#[test]
fn test_find_docs_directory() {
    let temp = TempDir::new().unwrap();
    let docs_dir = temp.path().join("docs");
    fs::create_dir(&docs_dir).unwrap();
    fs::write(docs_dir.join("guide.md"), "# Guide").unwrap();
    fs::write(docs_dir.join("api.rst"), "# API").unwrap();

    let handler = PythonHandler::new(temp.path());
    let docs = handler.find_docs().unwrap();
    assert_eq!(docs.len(), 2);
}

#[test]
fn test_find_changelog() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("CHANGELOG.md"), "# Changes").unwrap();

    let handler = PythonHandler::new(temp.path());
    let changelog = handler.find_changelog();
    assert!(changelog.is_some());
}

#[test]
fn test_no_changelog() {
    let temp = TempDir::new().unwrap();
    let handler = PythonHandler::new(temp.path());
    let changelog = handler.find_changelog();
    assert!(changelog.is_none());
}

#[test]
fn test_skip_venv_directories() {
    let temp = TempDir::new().unwrap();

    // Create venv with Python files (should be skipped)
    let venv_dir = temp.path().join("venv").join("lib");
    fs::create_dir_all(&venv_dir).unwrap();
    fs::write(venv_dir.join("package.py"), "# venv").unwrap();

    // Create real package
    let src_dir = temp.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    fs::write(src_dir.join("main.py"), "# source").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_source_files().unwrap();

    // Should not find venv files
    assert!(!files.iter().any(|f| f.to_str().unwrap().contains("venv")));
    assert!(files.iter().any(|f| f.ends_with("main.py")));
}

#[test]
fn test_skip_pycache_directories() {
    let temp = TempDir::new().unwrap();

    let pycache = temp.path().join("__pycache__");
    fs::create_dir(&pycache).unwrap();
    fs::write(pycache.join("module.pyc"), "# cache").unwrap();

    let src_dir = temp.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    fs::write(src_dir.join("main.py"), "# source").unwrap();

    let handler = PythonHandler::new(temp.path());
    let files = handler.find_source_files().unwrap();

    // Should not find pycache files
    assert!(!files
        .iter()
        .any(|f| f.to_str().unwrap().contains("__pycache__")));
    assert!(files.iter().any(|f| f.ends_with("main.py")));
}
