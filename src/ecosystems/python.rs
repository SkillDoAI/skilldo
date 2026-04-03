//! Python ecosystem handler — discovers source/test/doc/example files, extracts
//! metadata (name, version, license, URLs) from pyproject.toml and setup.cfg,
//! and detects dependencies. Used by the collector for Python projects.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Extract a clean version string from pyproject.toml `[project]` section content.
/// Strips bracket/brace wrappers and rejects dynamic versions (attr, file references).
pub(crate) fn pyproject_version(content: &str) -> Option<String> {
    let raw = pyproject_project_field(content, "version")?;
    let version = raw
        .trim_matches('[')
        .trim_matches(']')
        .trim_matches('{')
        .trim_matches('}');
    if !version.is_empty()
        && !version.contains('=')
        && !version.contains("attr")
        && !version.contains("file")
        && !version.contains("\"")
    {
        Some(version.to_string())
    } else {
        None
    }
}

/// Extract the raw value of a field from the `[project]` section of a pyproject.toml string.
/// Returns the value trimmed of whitespace and outer quotes, or `None` if not found.
/// Skips lines starting with "dynamic" (PEP 621 dynamic fields).
pub(crate) fn pyproject_project_field(content: &str, key: &str) -> Option<String> {
    let mut in_project = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_project = trimmed == "[project]";
            continue;
        }
        if in_project && trimmed.contains('=') && !trimmed.starts_with("dynamic") {
            // Split on first '=' and match key exactly (not prefix)
            if let Some(eq_pos) = trimmed.find('=') {
                let lhs = trimmed[..eq_pos].trim();
                if lhs != key {
                    continue;
                }
                let val = trimmed[eq_pos + 1..]
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Check if a Python filename is a test file
fn is_test_filename(name: &str) -> bool {
    name.starts_with("test_")
        || name.starts_with("tests_")
        || name.ends_with("_test.py")
        || name == "conftest.py"
}

pub struct PythonHandler {
    repo_path: PathBuf,
}

impl PythonHandler {
    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    /// Find all Python source files (prioritized for large codebases)
    pub fn find_source_files(&self) -> Result<Vec<PathBuf>> {
        let mut source_files = Vec::new();

        // Try src/{package}/ first, then {package}/
        let candidates = vec![self.repo_path.join("src"), self.repo_path.clone()];

        for base in candidates {
            if !base.exists() {
                continue;
            }

            self.collect_py_files(&base, &mut source_files)?;
        }

        // Dedup files (src/ scan may overlap with root scan)
        source_files.sort();
        source_files.dedup();

        // Sort by priority for large codebases (read most important files first)
        source_files.sort_by_key(|path| self.file_priority(path));
        let source_files = crate::util::filter_within_boundary(source_files, &self.repo_path);

        if source_files.is_empty() {
            bail!(
                "No Python source files found in {}",
                self.repo_path.display()
            );
        }

        info!("Found {} Python source files", source_files.len());
        Ok(source_files)
    }

    /// Calculate file priority (lower = higher priority, read first)
    fn file_priority(&self, path: &Path) -> i32 {
        crate::util::calculate_file_priority(path, &self.repo_path)
    }

    /// Find all Python test files (supports recursive search and multiple patterns)
    pub fn find_test_files(&self) -> Result<Vec<PathBuf>> {
        let mut test_files = Vec::new();

        // Strategy: Recursively search entire repo for test files
        // Patterns supported:
        // - **/tests/**/*.py (nested test directories like numpy/linalg/tests/)
        // - **/*_test.py (TensorFlow, PyTorch pattern)
        // - **/test_*.py (pytest convention)
        self.collect_test_files(&self.repo_path, &mut test_files)?;

        let test_files = crate::util::filter_within_boundary(test_files, &self.repo_path);

        if test_files.is_empty() {
            warn!("No test files found in {}", self.repo_path.display());
        }

        info!("Found {} Python test files", test_files.len());
        Ok(test_files)
    }

    /// Recursively collect test files with multiple pattern support
    fn collect_test_files(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        self.collect_test_files_inner(dir, files, 0)
    }

    fn collect_test_files_inner(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
        depth: usize,
    ) -> Result<()> {
        if !dir.is_dir() || depth > Self::MAX_DEPTH {
            return Ok(());
        }

        let entries = fs::read_dir(dir)?;
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".py") && is_test_filename(name) {
                        files.push(path);
                    }
                }
            } else if path.is_dir() {
                // Skip common non-source directories but recurse into everything else
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !matches!(
                        name,
                        "venv"
                            | ".venv"
                            | "env"
                            | ".env"
                            | "__pycache__"
                            | ".git"
                            | "node_modules"
                            | ".tox"
                            | "build"
                            | "dist"
                            | ".eggs"
                    ) {
                        self.collect_test_files_inner(&path, files, depth + 1)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Find example files (highest value for learning API usage)
    pub fn find_examples(&self) -> Result<Vec<PathBuf>> {
        let mut example_files = Vec::new();

        // Look for common example directory names
        for example_dir_name in &["examples", "example", "samples", "sample", "demos", "demo"] {
            let examples_dir = self.repo_path.join(example_dir_name);
            if examples_dir.exists() && examples_dir.is_dir() {
                self.collect_py_files(&examples_dir, &mut example_files)?;
            }
        }

        let example_files = crate::util::filter_within_boundary(example_files, &self.repo_path);

        info!("Found {} Python example files", example_files.len());
        Ok(example_files)
    }

    /// Find documentation files
    pub fn find_docs(&self) -> Result<Vec<PathBuf>> {
        let mut docs = Vec::new();

        // README
        for name in &["README.md", "README.rst", "README.txt", "README"] {
            let path = self.repo_path.join(name);
            if path.exists() {
                docs.push(path);
                break;
            }
        }

        // Search both docs/ and doc/ directories recursively
        for docs_dirname in &["docs", "doc"] {
            let docs_dir = self.repo_path.join(docs_dirname);
            if docs_dir.exists() && docs_dir.is_dir() {
                self.collect_docs_recursive(&docs_dir, &mut docs, 0)?;
            }
        }

        let docs = crate::util::filter_within_boundary(docs, &self.repo_path);

        info!("Found {} documentation files", docs.len());
        Ok(docs)
    }

    /// Recursively collect documentation files from a directory
    fn collect_docs_recursive(
        &self,
        dir: &Path,
        docs: &mut Vec<PathBuf>,
        depth: usize,
    ) -> Result<()> {
        // Limit recursion depth to avoid performance issues
        if depth > 10 {
            return Ok(());
        }

        // Skip common non-documentation directories
        if let Some(dir_name) = dir.file_name().and_then(|n| n.to_str()) {
            if dir_name.starts_with('.')
                || dir_name == "node_modules"
                || dir_name == "__pycache__"
                || dir_name == "build"
                || dir_name == "dist"
                || dir_name == "_build"
                || dir_name == ".git"
            {
                return Ok(());
            }
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_file() {
                    // Collect .md and .rst documentation files
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "md" || ext == "rst" {
                            docs.push(path);
                        }
                    }
                } else if path.is_dir() {
                    // Recurse into subdirectories
                    self.collect_docs_recursive(&path, docs, depth + 1)?;
                }
            }
        }

        Ok(())
    }

    /// Find changelog
    pub fn find_changelog(&self) -> Option<PathBuf> {
        for name in &[
            "HISTORY.md",
            "CHANGELOG.md",
            "CHANGES.md",
            "CHANGES.rst",
            "CHANGELOG",
        ] {
            let path = self.repo_path.join(name);
            if path.exists() && path.is_file() {
                info!("Found changelog: {}", name);
                return Some(path);
            }
        }

        debug!("No changelog found");
        None
    }

    /// Get package version from pyproject.toml, __init__.py, or docs
    pub fn get_version(&self) -> Result<String> {
        // Strategy 1: Try pyproject.toml (most authoritative source, scoped to [project])
        let pyproject = self.repo_path.join("pyproject.toml");
        if let Ok(content) = fs::read_to_string(&pyproject) {
            if let Some(version) = pyproject_version(&content) {
                return Ok(version);
            }
        }

        // Strategy 2: Try package __init__.py (flat layout, src/ layout, and single-file modules)
        if let Some(pkg_name) = self.repo_path.file_name().and_then(|n| n.to_str()) {
            let candidates = [
                self.repo_path.join(pkg_name).join("__init__.py"),
                self.repo_path
                    .join("src")
                    .join(pkg_name)
                    .join("__init__.py"),
                // Single-file modules (e.g. six.py in repo root)
                self.repo_path.join(format!("{pkg_name}.py")),
            ];
            for init_path in &candidates {
                if let Ok(content) = fs::read_to_string(init_path) {
                    for line in content.lines() {
                        if line.contains("__version__") && line.contains('=') {
                            if let Some(version) = line.split('=').nth(1) {
                                let version = version.trim().trim_matches('"').trim_matches('\'');
                                if !version.is_empty()
                                    && version.chars().next().unwrap_or('x').is_numeric()
                                {
                                    return Ok(version.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Strategy 3: Try root-level changelog files (CHANGES, CHANGELOG, HISTORY, etc.)
        // Sort entries for deterministic results when multiple changelog files exist.
        if let Ok(entries) = fs::read_dir(&self.repo_path) {
            let mut changelog_files: Vec<_> = entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|n| {
                            let nl = n.to_lowercase();
                            (nl.starts_with("change")
                                || nl.starts_with("history")
                                || nl.starts_with("news"))
                                && e.file_type().is_ok_and(|t| t.is_file())
                        })
                        .unwrap_or(false)
                })
                .collect();
            changelog_files.sort_by_key(|e| e.file_name());
            for entry in changelog_files {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Some(name) = entry.file_name().to_str() {
                        let search_content = content.chars().take(1000).collect::<String>();
                        for line in search_content.lines() {
                            let line_lower = line.to_lowercase();
                            if let Some(version) = self.extract_version_number(&line_lower) {
                                debug!("Found version {} in root {}", version, name);
                                return Ok(version);
                            }
                        }
                    }
                }
            }
        }

        // Strategy 4: Try release/blog docs (fallback for dynamic-version packages)
        if let Ok(docs) = self.find_docs() {
            for doc_path in docs {
                if let Some(filename) = doc_path.file_name().and_then(|n| n.to_str()) {
                    if filename.contains("release")
                        || filename.contains("blog")
                        || filename.contains("whatsnew")
                        || filename.contains("changelog")
                    {
                        if let Ok(content) = fs::read_to_string(&doc_path) {
                            let search_content = content.chars().take(1000).collect::<String>();
                            for line in search_content.lines() {
                                let line_lower = line.to_lowercase();
                                if let Some(version) = self.extract_version_number(&line_lower) {
                                    debug!("Found version {} in {}", version, filename);
                                    return Ok(version);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Final fallback to "latest"
        Ok("latest".to_string())
    }

    /// Extract version number like "3.0.0" or "2.1.4" from text
    fn extract_version_number(&self, text: &str) -> Option<String> {
        // Look for patterns like "3.0.0", "2.1.4", "1.5.3"
        // Must have at least major.minor format
        let words: Vec<&str> = text.split_whitespace().collect();

        for word in words {
            // Clean up common prefixes/suffixes
            let clean = word.trim_matches(|c: char| !c.is_numeric() && c != '.');

            // Check if it looks like a version number (e.g., "3.0.0", "2.1")
            if clean.contains('.') {
                let parts: Vec<&str> = clean.split('.').collect();

                // Validate it's numeric parts
                if parts.len() >= 2 && parts.iter().all(|p| p.chars().all(|c| c.is_numeric())) {
                    // Valid version like "3.0" or "3.0.0"
                    return Some(clean.to_string());
                }
            }
        }

        None
    }

    /// Get package license from pyproject.toml or setup.py
    pub fn get_license(&self) -> Option<String> {
        // Try pyproject.toml first (scoped to [project] section)
        let pyproject = self.repo_path.join("pyproject.toml");
        if pyproject.exists() {
            if let Ok(content) = fs::read_to_string(&pyproject) {
                if let Some(raw) = pyproject_project_field(&content, "license") {
                    // Handle TOML table format: { text = "BSD-3-Clause" }
                    if raw.contains("text") && raw.starts_with('{') {
                        if let Some(eq_pos) = raw.find('=') {
                            let value_part = &raw[eq_pos + 1..];
                            let clean_value = value_part
                                .trim()
                                .trim_end_matches('}')
                                .trim()
                                .trim_matches('"')
                                .trim_matches('\'')
                                .trim();
                            if !clean_value.is_empty() {
                                info!(
                                    "Found license in pyproject.toml (table format): {}",
                                    clean_value
                                );
                                return Some(clean_value.to_string());
                            }
                        }
                    }
                    // Simple string format (already trimmed/unquoted by helper)
                    else if !raw.starts_with('[') && !raw.starts_with('{') {
                        info!("Found license in pyproject.toml: {}", raw);
                        return Some(raw);
                    }
                }
            }
        }

        // Try setup.py
        let setup_py = self.repo_path.join("setup.py");
        if setup_py.exists() {
            if let Ok(content) = fs::read_to_string(&setup_py) {
                for line in content.lines() {
                    if line.contains("license") && line.contains("=") {
                        if let Some(license_part) = line.split('=').nth(1) {
                            let license = license_part
                                .trim()
                                .trim_matches(',')
                                .trim_matches('"')
                                .trim_matches('\'')
                                .trim();
                            if !license.is_empty() && !license.contains("(") {
                                debug!("Found license in setup.py: {}", license);
                                return Some(license.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Try setup.cfg
        let setup_cfg = self.repo_path.join("setup.cfg");
        if setup_cfg.exists() {
            if let Ok(content) = fs::read_to_string(&setup_cfg) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("license") && trimmed.contains("=") {
                        if let Some(license) = trimmed.split('=').nth(1) {
                            let license = license.trim();
                            if !license.is_empty() {
                                debug!("Found license in setup.cfg: {}", license);
                                return Some(license.to_string());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Get project URLs from pyproject.toml or setup.py
    pub fn get_project_urls(&self) -> Vec<(String, String)> {
        let mut urls = Vec::new();

        // Try pyproject.toml first
        let pyproject = self.repo_path.join("pyproject.toml");
        if pyproject.exists() {
            if let Ok(content) = fs::read_to_string(&pyproject) {
                let mut in_urls_section = false;
                for line in content.lines() {
                    let trimmed = line.trim();

                    // Check for [project.urls] section
                    if trimmed == "[project.urls]" {
                        in_urls_section = true;
                        continue;
                    }

                    // Exit urls section on new section
                    if in_urls_section && trimmed.starts_with('[') {
                        break;
                    }

                    // Parse URL lines
                    if in_urls_section && trimmed.contains('=') {
                        if let Some((key, value)) = trimmed.split_once('=') {
                            let key = key.trim().trim_matches('"').trim_matches('\'');
                            let value = value.trim().trim_matches('"').trim_matches('\'');
                            if value.starts_with("http") {
                                info!("Found project URL: {} = {}", key, value);
                                urls.push((key.to_string(), value.to_string()));
                            }
                        }
                    }
                }
            }
        }

        // Try setup.py if no URLs found
        if urls.is_empty() {
            let setup_py = self.repo_path.join("setup.py");
            if setup_py.exists() {
                if let Ok(content) = fs::read_to_string(&setup_py) {
                    // Look for project_urls dict
                    let mut in_project_urls = false;
                    for line in content.lines() {
                        let trimmed = line.trim();

                        if trimmed.contains("project_urls") && trimmed.contains("{") {
                            in_project_urls = true;
                        }

                        if in_project_urls {
                            if trimmed.contains("}") {
                                break;
                            }

                            // Parse "Key": "http://..." lines
                            if trimmed.contains("http") && trimmed.contains(':') {
                                if let Some((key, value)) = trimmed.split_once(':') {
                                    let key = key
                                        .trim()
                                        .trim_matches('"')
                                        .trim_matches('\'')
                                        .trim_matches(',');
                                    let value = value
                                        .trim()
                                        .trim_matches('"')
                                        .trim_matches('\'')
                                        .trim_matches(',');
                                    if value.starts_with("http") {
                                        debug!(
                                            "Found project URL in setup.py: {} = {}",
                                            key, value
                                        );
                                        urls.push((key.to_string(), value.to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        urls
    }

    /// Maximum recursion depth for file discovery (guards against symlink loops).
    const MAX_DEPTH: usize = 20;

    /// Recursively collect .py files from a directory (Python only, no C++/C)
    fn collect_py_files(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        self.collect_py_files_inner(dir, files, 0)
    }

    fn collect_py_files_inner(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
        depth: usize,
    ) -> Result<()> {
        if !dir.is_dir() || depth > Self::MAX_DEPTH {
            return Ok(());
        }

        let entries = fs::read_dir(dir)?;
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "py" {
                        // Exclude test files from source collection
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if !is_test_filename(name) {
                                files.push(path);
                            }
                        }
                    }
                }
            } else if path.is_dir() {
                // Skip common non-source directories (including test dirs)
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !matches!(
                        name,
                        "venv"
                            | ".venv"
                            | "env"
                            | ".env"
                            | "__pycache__"
                            | ".git"
                            | "node_modules"
                            | ".tox"
                            | "build"
                            | "dist"
                            | ".eggs"
                            | "csrc"
                            | "cpp"
                            | "cuda"
                            | "tests"
                            | "test"
                            | "testing"
                            | "examples"
                            | "example"
                            | "samples"
                            | "sample"
                            | "demo"
                            | "demos"
                    ) {
                        self.collect_py_files_inner(&path, files, depth + 1)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Detect indicators of C/native extensions in Python packages.
    pub fn detect_native_deps(&self) -> Vec<String> {
        let mut indicators = Vec::new();
        // Check setup.py for ext_modules or Extension
        let setup_py = self.repo_path.join("setup.py");
        if let Ok(content) = fs::read_to_string(&setup_py) {
            if content.contains("ext_modules") || content.contains("Extension(") {
                indicators.push("ext_modules in setup.py".to_string());
            }
            if content.contains("cffi_modules") {
                indicators.push("cffi_modules in setup.py".to_string());
            }
        }
        // Check pyproject.toml for maturin/pyo3
        let pyproject = self.repo_path.join("pyproject.toml");
        if let Ok(content) = fs::read_to_string(&pyproject) {
            if content.contains("[tool.maturin]") {
                indicators.push("maturin build system".to_string());
            }
            if content.contains("[tool.pyo3]")
                || (content.contains("[build-system]") && content.contains("\"pyo3\""))
            {
                indicators.push("pyo3 binding".to_string());
            }
        }
        indicators
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // -- find_source_files --

    #[test]
    fn test_find_source_files_from_root() {
        let dir = TempDir::new().unwrap();
        let pkg = dir.path().join("mypackage");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("core.py"), "# core").unwrap();
        fs::write(pkg.join("utils.py"), "# utils").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_find_source_files_from_src_dir() {
        let dir = TempDir::new().unwrap();
        let src_pkg = dir.path().join("src").join("mypackage");
        fs::create_dir_all(&src_pkg).unwrap();
        fs::write(src_pkg.join("api.py"), "# api").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert!(files.iter().any(|p| p.ends_with("api.py")));
    }

    #[test]
    fn test_find_source_files_deduplicates() {
        let dir = TempDir::new().unwrap();
        // Put a .py directly in root — scanned by both src/ check (skipped) and root check
        fs::write(dir.path().join("app.py"), "# app").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        // After dedup, each file appears once
        let mut sorted = files.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(files.len(), sorted.len());
    }

    #[test]
    fn test_find_source_files_empty_repo_errors() {
        let dir = TempDir::new().unwrap();
        // No .py files at all
        let handler = PythonHandler::new(dir.path());
        let result = handler.find_source_files();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No Python source files"));
    }

    #[test]
    fn test_find_source_files_skips_excluded_dirs() {
        let dir = TempDir::new().unwrap();
        // Put files in excluded dirs — they should NOT be found
        for excluded in &[
            "venv",
            ".venv",
            "__pycache__",
            ".git",
            "build",
            "dist",
            "csrc",
        ] {
            let excl_dir = dir.path().join(excluded);
            fs::create_dir_all(&excl_dir).unwrap();
            fs::write(excl_dir.join("hidden.py"), "# hidden").unwrap();
        }
        // Put one real file
        fs::write(dir.path().join("real.py"), "# real").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("real.py"));
    }

    #[test]
    fn test_find_source_files_ignores_non_py() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("module.py"), "# py").unwrap();
        fs::write(dir.path().join("module.cpp"), "// cpp").unwrap();
        fs::write(dir.path().join("module.c"), "// c").unwrap();
        fs::write(dir.path().join("README.md"), "# readme").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("module.py"));
    }

    // -- find_test_files --

    #[test]
    fn test_find_test_files_test_prefix() {
        let dir = TempDir::new().unwrap();
        let tests = dir.path().join("tests");
        fs::create_dir_all(&tests).unwrap();
        fs::write(tests.join("test_core.py"), "# test").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("test_core.py"));
    }

    #[test]
    fn test_find_test_files_suffix_pattern() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("core_test.py"), "# test").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("core_test.py"));
    }

    #[test]
    fn test_find_test_files_tests_prefix() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("tests_helpers.py"), "# tqdm convention").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_find_test_files_empty_returns_empty_vec() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("core.py"), "# not a test").unwrap();

        let handler = PythonHandler::new(dir.path());
        let result = handler.find_test_files().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_test_files_skips_excluded_dirs() {
        let dir = TempDir::new().unwrap();
        let venv = dir.path().join("venv");
        fs::create_dir_all(&venv).unwrap();
        fs::write(venv.join("test_hidden.py"), "# hidden").unwrap();
        // One real test file
        fs::write(dir.path().join("test_real.py"), "# real").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("test_real.py"));
    }

    #[test]
    fn test_find_test_files_recursive() {
        let dir = TempDir::new().unwrap();
        let deep = dir.path().join("pkg").join("sub").join("tests");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("test_deep.py"), "# deep test").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("test_deep.py"));
    }

    #[test]
    fn test_collect_test_files_non_dir_returns_ok() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("not_a_dir.txt");
        fs::write(&file_path, "content").unwrap();

        let handler = PythonHandler::new(dir.path());
        let mut files = Vec::new();
        handler.collect_test_files(&file_path, &mut files).unwrap();
        assert!(files.is_empty());
    }

    // -- find_examples --

    #[test]
    fn test_find_examples_with_examples_dir() {
        let dir = TempDir::new().unwrap();
        let examples = dir.path().join("examples");
        fs::create_dir_all(&examples).unwrap();
        fs::write(examples.join("demo.py"), "# demo").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_examples().unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_find_examples_no_dir() {
        let dir = TempDir::new().unwrap();
        let handler = PythonHandler::new(dir.path());
        let files = handler.find_examples().unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_find_examples_multiple_dirs() {
        let dir = TempDir::new().unwrap();
        for name in &["examples", "demos"] {
            let d = dir.path().join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("run.py"), "# run").unwrap();
        }

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_examples().unwrap();
        assert_eq!(files.len(), 2);
    }

    // -- find_docs --

    #[test]
    fn test_find_docs_readme_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.md"), "# Readme").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert!(docs.iter().any(|p| p.ends_with("README.md")));
    }

    #[test]
    fn test_find_docs_readme_rst() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.rst"), "Readme").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert!(docs.iter().any(|p| p.ends_with("README.rst")));
    }

    #[test]
    fn test_find_docs_recursive_docs_dir() {
        let dir = TempDir::new().unwrap();
        let docs_dir = dir.path().join("docs").join("guide");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("intro.md"), "# Intro").unwrap();
        fs::write(docs_dir.join("api.rst"), "API").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_find_docs_skips_build_dirs() {
        let dir = TempDir::new().unwrap();
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("real.md"), "# real").unwrap();
        let build = docs_dir.join("_build");
        fs::create_dir_all(&build).unwrap();
        fs::write(build.join("hidden.md"), "# hidden").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].ends_with("real.md"));
    }

    #[test]
    fn test_find_docs_skips_dot_dirs() {
        let dir = TempDir::new().unwrap();
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("real.md"), "# real").unwrap();
        let hidden = docs_dir.join(".hidden");
        fs::create_dir_all(&hidden).unwrap();
        fs::write(hidden.join("secret.md"), "# secret").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_find_docs_empty() {
        let dir = TempDir::new().unwrap();
        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert!(docs.is_empty());
    }

    // -- find_changelog --

    #[test]
    fn test_find_changelog_history_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("HISTORY.md"), "# History").unwrap();

        let handler = PythonHandler::new(dir.path());
        assert!(handler.find_changelog().is_some());
    }

    #[test]
    fn test_find_changelog_changes_rst() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CHANGES.rst"), "Changes").unwrap();

        let handler = PythonHandler::new(dir.path());
        let result = handler.find_changelog();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("CHANGES.rst"));
    }

    #[test]
    fn test_find_changelog_none() {
        let dir = TempDir::new().unwrap();
        let handler = PythonHandler::new(dir.path());
        assert!(handler.find_changelog().is_none());
    }

    // -- get_version --

    #[test]
    fn test_get_version_from_pyproject_toml() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"mypkg\"\nversion = \"2.3.1\"\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "2.3.1");
    }

    #[test]
    fn test_get_version_skips_dynamic() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "dynamic = [\"version\"]\nname = \"pkg\"\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        // dynamic line skipped, no other version source => "latest"
        assert_eq!(version, "latest");
    }

    #[test]
    fn test_get_version_skips_attr() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nversion = {attr = \"pkg.__version__\"}\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "latest");
    }

    #[test]
    fn test_get_version_from_init_py() {
        let dir = TempDir::new().unwrap();
        let pkg_name = dir
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let pkg_dir = dir.path().join(&pkg_name);
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("__init__.py"), "__version__ = \"1.2.3\"\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "1.2.3");
    }

    #[test]
    fn test_get_version_from_src_layout_init_py() {
        let dir = TempDir::new().unwrap();
        let pkg_name = dir
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        // src/ layout: src/<pkg>/__init__.py
        let src_pkg_dir = dir.path().join("src").join(&pkg_name);
        fs::create_dir_all(&src_pkg_dir).unwrap();
        fs::write(src_pkg_dir.join("__init__.py"), "__version__ = \"4.5.6\"\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "4.5.6");
    }

    #[test]
    fn test_get_version_init_py_non_numeric_skipped() {
        let dir = TempDir::new().unwrap();
        let pkg_name = dir
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let pkg_dir = dir.path().join(&pkg_name);
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("__init__.py"), "__version__ = get_version()\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "latest");
    }

    #[test]
    fn test_get_version_from_release_doc() {
        let dir = TempDir::new().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(
            docs.join("release_notes.md"),
            "# Release 4.2.0\nNew features\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "4.2.0");
    }

    #[test]
    fn test_get_version_fallback_latest() {
        let dir = TempDir::new().unwrap();
        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "latest");
    }

    // -- extract_version_number --

    #[test]
    fn test_extract_version_number_semver() {
        let handler = PythonHandler::new(Path::new("/tmp"));
        assert_eq!(
            handler.extract_version_number("release 3.0.0"),
            Some("3.0.0".to_string())
        );
    }

    #[test]
    fn test_extract_version_number_major_minor() {
        let handler = PythonHandler::new(Path::new("/tmp"));
        assert_eq!(
            handler.extract_version_number("version 2.1"),
            Some("2.1".to_string())
        );
    }

    #[test]
    fn test_extract_version_number_none() {
        let handler = PythonHandler::new(Path::new("/tmp"));
        assert_eq!(handler.extract_version_number("no version here"), None);
    }

    #[test]
    fn test_extract_version_number_with_prefix() {
        let handler = PythonHandler::new(Path::new("/tmp"));
        assert_eq!(
            handler.extract_version_number("v1.5.3 released"),
            Some("1.5.3".to_string())
        );
    }

    #[test]
    fn test_extract_version_number_non_numeric_parts() {
        let handler = PythonHandler::new(Path::new("/tmp"));
        // "3.0.0a1" — the 'a1' part makes the last segment non-numeric
        assert_eq!(handler.extract_version_number("version 3.0.0a1"), None);
    }

    // -- get_license --

    #[test]
    fn test_get_license_pyproject_simple() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nlicense = \"MIT\"\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), Some("MIT".to_string()));
    }

    #[test]
    fn test_get_license_pyproject_table_format() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nlicense = { text = \"BSD-3-Clause\" }\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), Some("BSD-3-Clause".to_string()));
    }

    #[test]
    fn test_get_license_setup_py() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            "setup(\n    license=\"Apache-2.0\",\n)\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), Some("Apache-2.0".to_string()));
    }

    #[test]
    fn test_get_license_setup_cfg() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("setup.cfg"), "[metadata]\nlicense = PSF\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), Some("PSF".to_string()));
    }

    #[test]
    fn test_get_license_none() {
        let dir = TempDir::new().unwrap();
        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), None);
    }

    #[test]
    fn test_get_license_setup_py_with_parens_skipped() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            "setup(\n    license=get_license(),\n)\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), None);
    }

    #[test]
    fn test_get_license_pyproject_bracket_skipped() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nlicense = [\"MIT\"]\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), None);
    }

    // -- get_project_urls --

    #[test]
    fn test_get_project_urls_pyproject() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project.urls]\nHomepage = \"https://example.com\"\nDocs = \"https://docs.example.com\"\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].0, "Homepage");
        assert_eq!(urls[0].1, "https://example.com");
    }

    #[test]
    fn test_get_project_urls_stops_at_next_section() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project.urls]\nHomepage = \"https://example.com\"\n[tool.pytest]\nflag = true\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert_eq!(urls.len(), 1);
    }

    #[test]
    fn test_get_project_urls_setup_py_fallback() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            r#"setup(
    project_urls={
        "Source": "https://github.com/org/repo",
    },
)
"#,
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert_eq!(urls.len(), 1);
    }

    #[test]
    fn test_get_project_urls_empty() {
        let dir = TempDir::new().unwrap();
        let handler = PythonHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert!(urls.is_empty());
    }

    #[test]
    fn test_get_project_urls_no_http_value_skipped() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project.urls]\nHomepage = \"not-a-url\"\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert!(urls.is_empty());
    }

    // -- collect_py_files --

    #[test]
    fn test_collect_py_files_non_dir() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("not_dir.txt");
        fs::write(&file, "").unwrap();

        let handler = PythonHandler::new(dir.path());
        let mut files = Vec::new();
        handler.collect_py_files(&file, &mut files).unwrap();
        assert!(files.is_empty());
    }

    // -- get_version: additional edge cases --

    #[test]
    fn test_get_version_from_init_py_single_quotes() {
        // __init__.py with single-quoted version string
        let dir = TempDir::new().unwrap();
        let pkg_name = dir
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let pkg_dir = dir.path().join(&pkg_name);
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("__init__.py"), "__version__ = '5.6.7'\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "5.6.7");
    }

    #[test]
    fn test_get_version_pyproject_version_with_braces() {
        // version = {attr = ...} format — should be skipped (contains "attr")
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nversion = {attr = \"pkg.__version__\"}\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "latest");
    }

    #[test]
    fn test_get_version_pyproject_version_with_quotes_in_value() {
        // version value containing inner quotes — should be skipped
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nversion = \"some\"other\"\"\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        // Contains a double-quote after trimming, so it is skipped
        assert_eq!(version, "latest");
    }

    #[test]
    fn test_get_version_pyproject_empty_version_value() {
        // version = "" — empty after trimming, should be skipped
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nversion = \"\"\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "latest");
    }

    #[test]
    fn test_get_version_from_whatsnew_doc() {
        // Version extracted from a "whatsnew" doc file
        let dir = TempDir::new().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(
            docs.join("whatsnew_v2.md"),
            "# What's new in 2.5.0\nBug fixes\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "2.5.0");
    }

    #[test]
    fn test_get_version_from_blog_doc() {
        // Version extracted from a blog doc
        let dir = TempDir::new().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("blog_post.md"), "# MyLib 1.0.0 Released\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "1.0.0");
    }

    #[test]
    fn test_get_version_prefers_pyproject_over_docs() {
        // pyproject.toml is the most authoritative source — takes priority over docs
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("release.md"), "# Release 2.0.0\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "1.0.0");
    }

    #[test]
    fn test_get_version_doc_without_version_falls_through() {
        // A release doc that has no version number in first 1000 chars
        // (avoid trailing periods which extract_version_number treats as versions
        // due to vacuous truth on empty split parts)
        let dir = TempDir::new().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(
            docs.join("release_notes.md"),
            "# Release Notes\nNo version here\n",
        )
        .unwrap();
        // Falls through to pyproject.toml
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nversion = \"3.1.4\"\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "3.1.4");
    }

    #[test]
    fn test_get_version_init_py_no_equals() {
        // __init__.py with __version__ but no = sign — should be skipped
        let dir = TempDir::new().unwrap();
        let pkg_name = dir
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let pkg_dir = dir.path().join(&pkg_name);
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("__init__.py"), "__version__: str\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        let version = handler.get_version().unwrap();
        assert_eq!(version, "latest");
    }

    // -- extract_version_number: additional edge cases --

    #[test]
    fn test_extract_version_number_surrounded_by_punctuation() {
        let handler = PythonHandler::new(Path::new("/tmp"));
        // Version surrounded by parens
        assert_eq!(
            handler.extract_version_number("(v1.2.3)"),
            Some("1.2.3".to_string())
        );
    }

    #[test]
    fn test_extract_version_number_four_parts() {
        let handler = PythonHandler::new(Path::new("/tmp"));
        assert_eq!(
            handler.extract_version_number("version 1.2.3.4"),
            Some("1.2.3.4".to_string())
        );
    }

    #[test]
    fn test_extract_version_number_trailing_dot() {
        let handler = PythonHandler::new(Path::new("/tmp"));
        // "1." splits into ["1", ""] — empty strings pass chars().all(is_numeric) vacuously
        // so this is treated as a valid version (vacuous truth edge case)
        assert_eq!(
            handler.extract_version_number("version 1."),
            Some("1.".to_string())
        );
    }

    #[test]
    fn test_extract_version_number_empty_string() {
        let handler = PythonHandler::new(Path::new("/tmp"));
        assert_eq!(handler.extract_version_number(""), None);
    }

    // -- get_license: additional edge cases --

    #[test]
    fn test_get_license_pyproject_table_no_space() {
        // license = {text="GPL-2.0"} — compact table format (no space after brace)
        // The shared helper returns the full value, and the table parser handles it.
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nlicense = {text=\"GPL-2.0\"}\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let lic = handler.get_license();
        assert_eq!(lic, Some("GPL-2.0".to_string()));
    }

    #[test]
    fn test_get_license_pyproject_table_empty_value() {
        // license = { text = "" } — empty value after trimming
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nlicense = { text = \"\" }\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let lic = handler.get_license();
        // Empty clean_value is skipped, falls through to setup.py/setup.cfg (none exist)
        assert_eq!(lic, None);
    }

    #[test]
    fn test_get_license_pyproject_curly_brace_skipped() {
        // license = {file = "LICENSE"} — starts with { so simple format skips it,
        // and the table format check fails because no "{ text" substring
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nlicense = {file = \"LICENSE\"}\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let lic = handler.get_license();
        assert_eq!(lic, None);
    }

    #[test]
    fn test_pyproject_field_exact_key_match() {
        // "license-files" should NOT shadow "license"
        let content = "[project]\nlicense-files = [\"LICENSE\"]\nlicense = \"MIT\"\n";
        assert_eq!(
            pyproject_project_field(content, "license"),
            Some("MIT".to_string())
        );
        // "version_info" should NOT match "version"
        let content2 = "[project]\nversion_info = \"extra\"\nversion = \"1.0.0\"\n";
        assert_eq!(
            pyproject_project_field(content2, "version"),
            Some("1.0.0".to_string())
        );
    }

    #[test]
    fn test_get_license_setup_py_empty_after_trim() {
        // setup.py with license="" (empty string)
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            "setup(\n    license=\"\",\n)\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), None);
    }

    #[test]
    fn test_get_license_setup_cfg_empty() {
        // setup.cfg with license = (empty value)
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("setup.cfg"), "[metadata]\nlicense =\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), None);
    }

    #[test]
    fn test_get_license_fallback_chain_pyproject_to_setup_py() {
        // pyproject.toml with license = [dynamic], falls through to setup.py
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nlicense = [\"MIT\"]\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("setup.py"),
            "setup(\n    license=\"Apache-2.0\",\n)\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), Some("Apache-2.0".to_string()));
    }

    #[test]
    fn test_get_license_fallback_chain_to_setup_cfg() {
        // No pyproject.toml, setup.py has license with parens (skipped),
        // setup.cfg has valid license
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            "setup(\n    license=get_license(),\n)\n",
        )
        .unwrap();
        fs::write(dir.path().join("setup.cfg"), "[metadata]\nlicense = ISC\n").unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), Some("ISC".to_string()));
    }

    // -- get_project_urls: additional edge cases --

    #[test]
    fn test_get_project_urls_setup_py_multiple_urls() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            r#"setup(
    project_urls={
        "Source": "https://github.com/org/repo",
        "Docs": "https://docs.example.com",
    },
)
"#,
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert_eq!(urls.len(), 2);
    }

    #[test]
    fn test_get_project_urls_setup_py_no_http_value_skipped() {
        // setup.py with a URL line that doesn't start with "http" after split
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            r#"setup(
    project_urls={
        "Source": "not-a-url",
    },
)
"#,
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let urls = handler.get_project_urls();
        // "not-a-url" doesn't contain "http" so the line is skipped entirely
        assert!(urls.is_empty());
    }

    #[test]
    fn test_get_project_urls_pyproject_takes_priority_over_setup_py() {
        // When pyproject.toml has URLs, setup.py is not checked
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project.urls]\nHomepage = \"https://example.com\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("setup.py"),
            r#"setup(
    project_urls={
        "Source": "https://github.com/org/repo",
    },
)
"#,
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let urls = handler.get_project_urls();
        // Only pyproject.toml URL, not setup.py
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].0, "Homepage");
    }

    // -- find_changelog: additional variants --

    #[test]
    fn test_find_changelog_changelog_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CHANGELOG.md"), "# Changelog").unwrap();

        let handler = PythonHandler::new(dir.path());
        let result = handler.find_changelog();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("CHANGELOG.md"));
    }

    #[test]
    fn test_find_changelog_changes_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CHANGES.md"), "# Changes").unwrap();

        let handler = PythonHandler::new(dir.path());
        let result = handler.find_changelog();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("CHANGES.md"));
    }

    #[test]
    fn test_find_changelog_no_extension() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CHANGELOG"), "Changes").unwrap();

        let handler = PythonHandler::new(dir.path());
        let result = handler.find_changelog();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("CHANGELOG"));
    }

    #[test]
    fn test_find_changelog_priority_order() {
        // HISTORY.md takes priority over CHANGELOG.md
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("HISTORY.md"), "# History").unwrap();
        fs::write(dir.path().join("CHANGELOG.md"), "# Changelog").unwrap();

        let handler = PythonHandler::new(dir.path());
        let result = handler.find_changelog();
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("HISTORY.md"));
    }

    // -- find_docs: additional edge cases --

    #[test]
    fn test_find_docs_doc_directory() {
        // Some projects use "doc/" instead of "docs/"
        let dir = TempDir::new().unwrap();
        let doc_dir = dir.path().join("doc");
        fs::create_dir_all(&doc_dir).unwrap();
        fs::write(doc_dir.join("guide.md"), "# Guide").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert!(docs.iter().any(|p| p.ends_with("guide.md")));
    }

    #[test]
    fn test_find_docs_readme_txt() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.txt"), "Readme text").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert!(docs.iter().any(|p| p.ends_with("README.txt")));
    }

    #[test]
    fn test_find_docs_readme_no_ext() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README"), "Readme").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert!(docs.iter().any(|p| p.ends_with("README")));
    }

    #[test]
    fn test_find_docs_readme_priority() {
        // README.md takes priority (checked first); only one README is included
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.md"), "# Readme").unwrap();
        fs::write(dir.path().join("README.rst"), "Readme").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        // Only one readme (the first match, README.md)
        let readme_count = docs
            .iter()
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("README")
            })
            .count();
        assert_eq!(readme_count, 1);
        assert!(docs.iter().any(|p| p.ends_with("README.md")));
    }

    #[test]
    fn test_find_docs_ignores_non_doc_extensions() {
        // Files with non-.md/.rst extensions in docs/ are skipped
        let dir = TempDir::new().unwrap();
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("guide.md"), "# Guide").unwrap();
        fs::write(docs_dir.join("style.css"), "body {}").unwrap();
        fs::write(docs_dir.join("app.py"), "# python").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].ends_with("guide.md"));
    }

    #[test]
    fn test_collect_docs_recursive_depth_limit() {
        // Create a directory structure deeper than 10 levels
        let dir = TempDir::new().unwrap();
        let mut deepest = dir.path().join("docs");
        for i in 0..12 {
            deepest = deepest.join(format!("level_{}", i));
        }
        fs::create_dir_all(&deepest).unwrap();
        fs::write(deepest.join("deep.md"), "# Deep").unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        // The file at depth 12+ should NOT be found due to depth limit of 10
        assert!(
            !docs.iter().any(|p| p.ends_with("deep.md")),
            "File beyond depth 10 should not be collected"
        );
    }

    // -- find_examples: additional directory variants --

    #[test]
    fn test_find_examples_sample_dir() {
        let dir = TempDir::new().unwrap();
        let sample = dir.path().join("sample");
        fs::create_dir_all(&sample).unwrap();
        fs::write(sample.join("run.py"), "# sample").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_examples().unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_find_examples_demo_dir() {
        let dir = TempDir::new().unwrap();
        let demo = dir.path().join("demo");
        fs::create_dir_all(&demo).unwrap();
        fs::write(demo.join("app.py"), "# demo").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_examples().unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_find_examples_samples_dir() {
        let dir = TempDir::new().unwrap();
        let samples = dir.path().join("samples");
        fs::create_dir_all(&samples).unwrap();
        fs::write(samples.join("basic.py"), "# basic").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_examples().unwrap();
        assert_eq!(files.len(), 1);
    }

    // -- find_source_files: additional edge cases --

    #[test]
    fn test_find_source_files_skips_cuda_cpp_dirs() {
        // cuda/ and cpp/ dirs should be excluded
        let dir = TempDir::new().unwrap();
        for excluded in &["cuda", "cpp"] {
            let excl_dir = dir.path().join(excluded);
            fs::create_dir_all(&excl_dir).unwrap();
            fs::write(excl_dir.join("kernel.py"), "# kernel").unwrap();
        }
        fs::write(dir.path().join("main.py"), "# main").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("main.py"));
    }

    #[test]
    fn test_find_source_files_skips_tox_and_eggs() {
        let dir = TempDir::new().unwrap();
        for excluded in &[".tox", ".eggs"] {
            let excl_dir = dir.path().join(excluded);
            fs::create_dir_all(&excl_dir).unwrap();
            fs::write(excl_dir.join("something.py"), "# hidden").unwrap();
        }
        fs::write(dir.path().join("real.py"), "# real").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("real.py"));
    }

    // -- find_test_files: additional skip-dir patterns --

    #[test]
    fn test_find_test_files_skips_all_excluded_dirs() {
        let dir = TempDir::new().unwrap();
        for excluded in &[
            ".env",
            "env",
            "node_modules",
            ".tox",
            "build",
            "dist",
            ".eggs",
        ] {
            let excl_dir = dir.path().join(excluded);
            fs::create_dir_all(&excl_dir).unwrap();
            fs::write(excl_dir.join("test_hidden.py"), "# hidden test").unwrap();
        }
        fs::write(dir.path().join("test_real.py"), "# real").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("test_real.py"));
    }

    #[test]
    fn test_find_test_files_non_py_not_collected() {
        // A file named "test_foo.txt" should NOT be collected
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test_foo.txt"), "# not python").unwrap();
        fs::write(dir.path().join("test_bar.py"), "# python test").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("test_bar.py"));
    }

    // -- collect_py_files: recursive and edge cases --

    #[test]
    fn test_collect_py_files_recursive() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("pkg").join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("deep.py"), "# deep").unwrap();
        fs::write(dir.path().join("top.py"), "# top").unwrap();

        let handler = PythonHandler::new(dir.path());
        let mut files = Vec::new();
        handler.collect_py_files(dir.path(), &mut files).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_collect_py_files_empty_dir() {
        let dir = TempDir::new().unwrap();
        let handler = PythonHandler::new(dir.path());
        let mut files = Vec::new();
        handler.collect_py_files(dir.path(), &mut files).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_collect_py_files_excludes_test_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("core.py"), "# core").unwrap();
        fs::write(dir.path().join("test_core.py"), "# test").unwrap();
        fs::write(dir.path().join("core_test.py"), "# test").unwrap();
        fs::write(dir.path().join("conftest.py"), "# fixtures").unwrap();
        fs::write(dir.path().join("utils.py"), "# utils").unwrap();

        let handler = PythonHandler::new(dir.path());
        let mut files = Vec::new();
        handler.collect_py_files(dir.path(), &mut files).unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"core.py"));
        assert!(names.contains(&"utils.py"));
        assert!(!names.contains(&"test_core.py"));
        assert!(!names.contains(&"core_test.py"));
        assert!(!names.contains(&"conftest.py"));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_collect_py_files_excludes_test_dirs() {
        let dir = TempDir::new().unwrap();
        let tests = dir.path().join("tests");
        fs::create_dir_all(&tests).unwrap();
        fs::write(tests.join("test_api.py"), "# test").unwrap();
        let test_dir = dir.path().join("test");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(test_dir.join("test_utils.py"), "# test").unwrap();
        fs::write(dir.path().join("main.py"), "# main").unwrap();

        let handler = PythonHandler::new(dir.path());
        let mut files = Vec::new();
        handler.collect_py_files(dir.path(), &mut files).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].file_name().unwrap().to_str().unwrap() == "main.py");
    }

    #[test]
    fn test_collect_py_files_excludes_example_dirs() {
        let dir = TempDir::new().unwrap();
        for name in &["examples", "example", "samples", "sample", "demo", "demos"] {
            let sub = dir.path().join(name);
            fs::create_dir_all(&sub).unwrap();
            fs::write(sub.join("usage.py"), "# example").unwrap();
        }
        fs::write(dir.path().join("core.py"), "# core").unwrap();

        let handler = PythonHandler::new(dir.path());
        let mut files = Vec::new();
        handler.collect_py_files(dir.path(), &mut files).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].file_name().unwrap().to_str().unwrap() == "core.py");
    }

    // -- pyproject_project_field: direct edge cases --

    #[test]
    fn test_pyproject_project_field_empty_value_returns_none() {
        // An explicitly empty value (after quote trimming) should return None
        let content = "[project]\nname = \"\"\n";
        assert_eq!(pyproject_project_field(content, "name"), None);
    }

    #[test]
    fn test_pyproject_project_field_not_in_project_section() {
        // Field under a different section should not be found
        let content = "[tool.poetry]\nname = \"poetry-name\"\n";
        assert_eq!(pyproject_project_field(content, "name"), None);
    }

    // -- get_license: setup.py edge cases for empty license values --

    #[test]
    fn test_get_license_setup_py_no_equals_skipped() {
        // A setup.py line with "license" but no '=' should be skipped
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            "# this mentions license but has no assignment\nprint('license')\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), None);
    }

    // -- get_project_urls: setup.py closing brace on same line as project_urls --

    #[test]
    fn test_get_project_urls_setup_py_closing_brace_same_line() {
        // project_urls={} — opening and closing brace on same line
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            "setup(\n    project_urls={},\n)\n",
        )
        .unwrap();

        let handler = PythonHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert!(urls.is_empty());
    }

    // -- find_test_files: conftest.py pattern --

    #[test]
    fn test_find_test_files_conftest() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("conftest.py"), "# conftest").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("conftest.py"));
    }

    // -- is_test_filename edge cases --

    #[test]
    fn test_is_test_filename_patterns() {
        assert!(is_test_filename("test_core.py"));
        assert!(is_test_filename("tests_helpers.py"));
        assert!(is_test_filename("core_test.py"));
        assert!(is_test_filename("conftest.py"));
        assert!(!is_test_filename("core.py"));
        assert!(!is_test_filename("testutils.py"));
        assert!(!is_test_filename("contest.py"));
    }

    // -- collect_py_files: depth limit --

    #[test]
    fn test_collect_py_files_depth_limit() {
        let dir = TempDir::new().unwrap();
        // Create a directory structure deeper than MAX_DEPTH (20)
        let mut deepest = dir.path().to_path_buf();
        for i in 0..22 {
            deepest = deepest.join(format!("d{}", i));
        }
        fs::create_dir_all(&deepest).unwrap();
        fs::write(deepest.join("deep.py"), "# deep").unwrap();
        fs::write(dir.path().join("shallow.py"), "# shallow").unwrap();

        let handler = PythonHandler::new(dir.path());
        let mut files = Vec::new();
        handler.collect_py_files(dir.path(), &mut files).unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"shallow.py"));
        assert!(
            !names.contains(&"deep.py"),
            "should not find files beyond MAX_DEPTH"
        );
    }

    // -- symlink boundary tests --

    #[cfg(unix)]
    #[test]
    fn test_find_source_files_skips_symlink_escaping_repo() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();

        // Create a .py file outside the repo
        fs::write(outside.path().join("secret.py"), "# secret").unwrap();

        // Create a symlink inside the repo pointing outside
        std::os::unix::fs::symlink(outside.path(), dir.path().join("src")).unwrap();

        // Also create a real file so find_source_files doesn't bail
        fs::write(dir.path().join("real.py"), "# real").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"real.py"), "should keep real files");
        assert!(
            !names.contains(&"secret.py"),
            "should skip files via symlink escaping repo"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_find_test_files_skips_symlink_escaping_repo() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();

        // Create a test file outside the repo
        fs::write(outside.path().join("test_secret.py"), "# secret test").unwrap();

        // Symlink tests/ → outside dir
        std::os::unix::fs::symlink(outside.path(), dir.path().join("tests")).unwrap();

        // Create a real test file
        fs::write(dir.path().join("test_real.py"), "# real test").unwrap();

        let handler = PythonHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"test_real.py"));
        assert!(!names.contains(&"test_secret.py"));
    }

    #[cfg(unix)]
    #[test]
    fn test_find_docs_skips_symlink_escaping_repo() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();

        // Create a doc file outside
        fs::write(outside.path().join("secret.md"), "# secret").unwrap();

        // Symlink docs/ → outside dir
        std::os::unix::fs::symlink(outside.path(), dir.path().join("docs")).unwrap();

        let handler = PythonHandler::new(dir.path());
        let docs = handler.find_docs().unwrap();
        assert!(
            !docs.iter().any(|p| p.ends_with("secret.md")),
            "should skip docs via symlink escaping repo"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_find_examples_skips_symlink_escaping_repo() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();

        // Create an example file outside
        fs::write(outside.path().join("demo.py"), "# demo").unwrap();

        // Symlink examples/ → outside dir
        std::os::unix::fs::symlink(outside.path(), dir.path().join("examples")).unwrap();

        let handler = PythonHandler::new(dir.path());
        let examples = handler.find_examples().unwrap();
        assert!(
            !examples.iter().any(|p| p.ends_with("demo.py")),
            "should skip examples via symlink escaping repo"
        );
    }

    #[test]
    fn test_get_version_from_single_file_module() {
        // Libraries like `six` are a single .py file in the repo root
        let dir = TempDir::new().unwrap();
        // Repo dir name = package name
        let repo = dir.path().join("six");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("six.py"), "__version__ = \"1.17.0\"\n").unwrap();
        let handler = PythonHandler::new(&repo);
        assert_eq!(handler.get_version().unwrap(), "1.17.0");
    }

    #[test]
    fn test_get_version_from_root_changes_file() {
        // Libraries like `six` have a CHANGES file in the repo root
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("CHANGES"),
            "Changelog for six\n=================\n\n1.17.0\n------\n\n- Fixed stuff\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "1.17.0");
    }

    #[test]
    fn test_get_version_from_root_changelog_md() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("CHANGELOG.md"),
            "# Changelog\n\n## 2.5.1\n\n- Bug fix\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "2.5.1");
    }

    #[test]
    fn test_get_version_prefers_init_over_root_changelog() {
        // __version__ in source should win over changelog
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("mypkg");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("mypkg.py"), "__version__ = \"3.0.0\"\n").unwrap();
        fs::write(repo.join("CHANGES"), "2.9.0\n------\n").unwrap();
        let handler = PythonHandler::new(&repo);
        assert_eq!(handler.get_version().unwrap(), "3.0.0");
    }

    // ── detect_native_deps ──────────────────────────────────────────

    #[test]
    fn detect_native_deps_ext_modules_in_setup_py() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            "from setuptools import setup, Extension\next_modules=[Extension('foo', ['foo.c'])]\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("ext_modules")),
            "should detect ext_modules, got: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_cffi_modules_in_setup_py() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.py"),
            "from setuptools import setup\nsetup(cffi_modules=['build_ffi.py:ffi'])\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("cffi_modules")),
            "should detect cffi_modules, got: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_maturin_in_pyproject() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[build-system]\nrequires = [\"maturin\"]\n\n[tool.maturin]\nfeatures = [\"pyo3/extension-module\"]\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("maturin")),
            "should detect maturin, got: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_pyo3_in_pyproject() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[build-system]\nrequires = [\"pyo3\"]\n\n[tool.pyo3]\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("pyo3")),
            "should detect pyo3, got: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_pyo3_requires_only() {
        // Test pyo3 detection from requires alone (no [tool.pyo3] section)
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[build-system]\nrequires = [\"setuptools\", \"pyo3\"]\nbuild-backend = \"setuptools.build_meta\"\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("pyo3")),
            "should detect pyo3 from requires list, got: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_pyo3_no_false_positive() {
        // A pyproject.toml mentioning pyo3 only in a comment should NOT trigger
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"pure-python\"\n# Note: pyo3 is not used here\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        assert!(
            handler.detect_native_deps().is_empty(),
            "comment mentioning pyo3 should not trigger detection"
        );
    }

    #[test]
    fn detect_native_deps_clean_project() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"pure-python\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        assert!(handler.detect_native_deps().is_empty());
    }

    // ── is_test_filename edge cases ─────────────────────────────────

    #[test]
    fn is_test_filename_suffix_pattern() {
        assert!(is_test_filename("foo_test.py"));
        assert!(is_test_filename("conftest.py"));
        assert!(is_test_filename("tests_integration.py"));
        assert!(!is_test_filename("foo.py"));
        assert!(!is_test_filename("testutils.py"));
    }

    // ── extract_version_number ──────────────────────────────────────

    #[test]
    fn extract_version_number_basic() {
        let handler = PythonHandler::new(std::path::Path::new("/tmp"));
        assert_eq!(
            handler.extract_version_number("## 3.0.0"),
            Some("3.0.0".to_string())
        );
        assert_eq!(
            handler.extract_version_number("version 2.1"),
            Some("2.1".to_string())
        );
        assert_eq!(handler.extract_version_number("no version here"), None);
        assert_eq!(handler.extract_version_number("abc"), None);
    }

    // ── get_version from release docs fallback (Strategy 4) ─────────

    #[test]
    fn test_get_version_from_release_doc_strategy4() {
        let dir = TempDir::new().unwrap();
        // No pyproject.toml, no __init__.py, no root changelog
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(
            docs_dir.join("release-notes.md"),
            "# Release Notes\n\n## 4.2.0\n\n- New feature\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "4.2.0");
    }

    #[test]
    fn test_get_version_fallback_to_latest_no_sources() {
        // No version source at all => falls back to "latest"
        let dir = TempDir::new().unwrap();
        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "latest");
    }

    // ── get_license from setup.cfg ──────────────────────────────────

    #[test]
    fn get_license_from_setup_cfg() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("setup.cfg"),
            "[metadata]\nname = mylib\nlicense = BSD-3-Clause\n",
        )
        .unwrap();
        let handler = PythonHandler::new(dir.path());
        assert_eq!(handler.get_license(), Some("BSD-3-Clause".to_string()));
    }

    // ── collect_py_files excludes test dirs ─────────────────────────

    #[test]
    fn collect_py_files_excludes_tests_and_venv() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let pkg = root.join("pkg");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("api.py"), "# api").unwrap();
        // These should be excluded
        let tests_dir = root.join("tests");
        fs::create_dir_all(&tests_dir).unwrap();
        fs::write(tests_dir.join("test_api.py"), "# test").unwrap();
        let venv = root.join("venv");
        fs::create_dir_all(venv.join("lib")).unwrap();
        fs::write(venv.join("lib").join("site.py"), "# venv").unwrap();

        let handler = PythonHandler::new(root);
        let mut files = Vec::new();
        handler.collect_py_files(root, &mut files).unwrap();
        let names: Vec<_> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"api.py"));
        assert!(!names.contains(&"test_api.py"));
        assert!(!names.contains(&"site.py"));
    }

    // ── pyproject_project_field edge: empty value after '=' ─────────

    #[test]
    fn pyproject_project_field_empty_value_returns_none() {
        let content = "[project]\nname = \nversion = \"1.0\"\n";
        assert_eq!(pyproject_project_field(content, "name"), None);
    }

    // ── collect_docs_recursive into subdirectory ────────────────────

    #[test]
    fn collect_docs_recursive_enters_subdir() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let guides = root.join("docs").join("guides");
        fs::create_dir_all(&guides).unwrap();
        fs::write(guides.join("getting-started.md"), "# Guide").unwrap();
        // _build should be skipped
        let build = root.join("docs").join("_build");
        fs::create_dir_all(&build).unwrap();
        fs::write(build.join("output.md"), "# Build output").unwrap();

        let handler = PythonHandler::new(root);
        let mut docs = Vec::new();
        handler
            .collect_docs_recursive(&root.join("docs"), &mut docs, 0)
            .unwrap();
        let names: Vec<_> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"getting-started.md"));
        assert!(!names.contains(&"output.md"));
    }
}
