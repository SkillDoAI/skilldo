// Test TOML table format license parsing (PyTorch style)
use skilldo::ecosystems::python::PythonHandler;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_license_toml_table_format() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // PyTorch-style TOML table format
    let pyproject_content = r#"
[project]
name = "torch"
license = { text = "BSD-3-Clause" }
"#;
    fs::write(repo_path.join("pyproject.toml"), pyproject_content).unwrap();

    let handler = PythonHandler::new(repo_path);
    let license = handler.get_license();

    assert!(license.is_some());
    assert_eq!(license.unwrap(), "BSD-3-Clause");
}

#[test]
fn test_license_simple_string_format() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Simple string format
    let pyproject_content = r#"
[project]
name = "test"
license = "MIT"
"#;
    fs::write(repo_path.join("pyproject.toml"), pyproject_content).unwrap();

    let handler = PythonHandler::new(repo_path);
    let license = handler.get_license();

    assert!(license.is_some());
    assert_eq!(license.unwrap(), "MIT");
}
