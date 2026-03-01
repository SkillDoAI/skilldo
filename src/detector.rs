use anyhow::{bail, Result};
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    Python,
    JavaScript,
    Rust,
    Go,
}

impl Language {
    pub fn as_str(&self) -> &str {
        match self {
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::Rust => "rust",
            Language::Go => "go",
        }
    }

    pub fn ecosystem_term(&self) -> &str {
        match self {
            Language::Python => "package",
            Language::Go => "module",
            Language::Rust => "crate",
            Language::JavaScript => "package",
        }
    }
}

impl FromStr for Language {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "python" | "py" => Ok(Language::Python),
            "javascript" | "js" | "node" | "npm" => Ok(Language::JavaScript),
            "rust" | "rs" => Ok(Language::Rust),
            "go" | "golang" => Ok(Language::Go),
            _ => bail!("Unknown language: {}", s),
        }
    }
}

pub fn detect_language(path: &Path) -> Result<Language> {
    // Check for language-specific files in order of confidence
    if path.join("pyproject.toml").exists() || path.join("setup.py").exists() {
        return Ok(Language::Python);
    }

    if path.join("Cargo.toml").exists() {
        return Ok(Language::Rust);
    }

    if path.join("package.json").exists() {
        return Ok(Language::JavaScript);
    }

    if path.join("go.mod").exists() {
        return Ok(Language::Go);
    }

    bail!(
        "Could not detect language in {}. Please specify with --language",
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_from_str_aliases() {
        // Python aliases
        assert_eq!(Language::from_str("python").unwrap(), Language::Python);
        assert_eq!(Language::from_str("py").unwrap(), Language::Python);
        assert_eq!(Language::from_str("PYTHON").unwrap(), Language::Python);

        // JavaScript aliases
        assert_eq!(
            Language::from_str("javascript").unwrap(),
            Language::JavaScript
        );
        assert_eq!(Language::from_str("js").unwrap(), Language::JavaScript);
        assert_eq!(Language::from_str("node").unwrap(), Language::JavaScript);
        assert_eq!(Language::from_str("npm").unwrap(), Language::JavaScript);

        // Rust aliases
        assert_eq!(Language::from_str("rust").unwrap(), Language::Rust);
        assert_eq!(Language::from_str("rs").unwrap(), Language::Rust);

        // Go aliases
        assert_eq!(Language::from_str("go").unwrap(), Language::Go);
        assert_eq!(Language::from_str("golang").unwrap(), Language::Go);
    }

    #[test]
    fn test_from_str_invalid() {
        assert!(Language::from_str("ruby").is_err());
        assert!(Language::from_str("").is_err());
    }

    #[test]
    fn test_as_str_roundtrip() {
        for lang in &[
            Language::Python,
            Language::JavaScript,
            Language::Rust,
            Language::Go,
        ] {
            assert_eq!(Language::from_str(lang.as_str()).unwrap(), *lang);
        }
    }

    #[test]
    fn test_ecosystem_term() {
        assert_eq!(Language::Python.ecosystem_term(), "package");
        assert_eq!(Language::Go.ecosystem_term(), "module");
        assert_eq!(Language::Rust.ecosystem_term(), "crate");
        assert_eq!(Language::JavaScript.ecosystem_term(), "package");
    }

    #[test]
    fn test_detect_python_pyproject() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pyproject.toml"),
            "[project]\nname = \"test\"",
        )
        .unwrap();
        assert_eq!(detect_language(tmp.path()).unwrap(), Language::Python);
    }

    #[test]
    fn test_detect_python_setup_py() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("setup.py"), "from setuptools import setup").unwrap();
        assert_eq!(detect_language(tmp.path()).unwrap(), Language::Python);
    }

    #[test]
    fn test_detect_rust() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        assert_eq!(detect_language(tmp.path()).unwrap(), Language::Rust);
    }

    #[test]
    fn test_detect_javascript() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_language(tmp.path()).unwrap(), Language::JavaScript);
    }

    #[test]
    fn test_detect_go() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("go.mod"), "module test").unwrap();
        assert_eq!(detect_language(tmp.path()).unwrap(), Language::Go);
    }

    #[test]
    fn test_detect_unknown() {
        let tmp = TempDir::new().unwrap();
        let result = detect_language(tmp.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Could not detect language"));
    }
}
