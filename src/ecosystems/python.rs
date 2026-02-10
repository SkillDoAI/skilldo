use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

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

        if source_files.is_empty() {
            bail!(
                "No Python source files found in {}",
                self.repo_path.display()
            );
        }

        // Sort by priority for large codebases (read most important files first)
        source_files.sort_by_key(|path| self.file_priority(path));

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

        if test_files.is_empty() {
            bail!(
                "No tests found in {}. Tests are required for generating rules.",
                self.repo_path.display()
            );
        }

        info!("Found {} Python test files", test_files.len());
        Ok(test_files)
    }

    /// Recursively collect test files with multiple pattern support
    fn collect_test_files(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir)?;
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                // Check if it's a test file
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".py")
                        && (
                            name.starts_with("test_") ||   // test_*.py
                        name.starts_with("tests_") ||  // tests_*.py (tqdm convention)
                        name.ends_with("_test.py")
                            // *_test.py
                        )
                    {
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
                        self.collect_test_files(&path, files)?;
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

    /// Get package version from pyproject.toml or setup.py
    pub fn get_version(&self) -> Result<String> {
        // Strategy 1: Try to find version in release/blog docs
        // Look for patterns like "pandas 3.0.0", "version 3.0.0", "released 3.0"
        if let Ok(docs) = self.find_docs() {
            for doc_path in docs {
                if let Some(filename) = doc_path.file_name().and_then(|n| n.to_str()) {
                    // Check release notes, blog posts, whatsnew files
                    if filename.contains("release")
                        || filename.contains("blog")
                        || filename.contains("whatsnew")
                        || filename.contains("changelog")
                    {
                        if let Ok(content) = fs::read_to_string(&doc_path) {
                            // Look for version patterns in first 1000 chars
                            let search_content = content.chars().take(1000).collect::<String>();

                            // Pattern: "pandas 3.0.0", "version 3.0.0", "released!", etc.
                            for line in search_content.lines() {
                                let line_lower = line.to_lowercase();

                                // Extract version like "3.0.0" or "2.1.4"
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

        // Strategy 2: Try pyproject.toml
        let pyproject = self.repo_path.join("pyproject.toml");
        if pyproject.exists() {
            if let Ok(content) = fs::read_to_string(&pyproject) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("version")
                        && trimmed.contains("=")
                        && !trimmed.starts_with("dynamic")
                    {
                        if let Some(version) = trimmed.split('=').nth(1) {
                            let version = version
                                .trim()
                                .trim_matches('"')
                                .trim_matches('\'')
                                .trim_matches('[')
                                .trim_matches(']')
                                .trim_matches('{')
                                .trim_matches('}');

                            if !version.is_empty()
                                && !version.contains("attr")
                                && !version.contains("\"")
                            {
                                return Ok(version.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Strategy 3: Try package __init__.py
        if let Some(pkg_name) = self.repo_path.file_name().and_then(|n| n.to_str()) {
            let init_path = self.repo_path.join(pkg_name).join("__init__.py");
            if init_path.exists() {
                if let Ok(content) = fs::read_to_string(&init_path) {
                    for line in content.lines() {
                        if line.contains("__version__") && line.contains("=") {
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
        // Try pyproject.toml first
        let pyproject = self.repo_path.join("pyproject.toml");
        if pyproject.exists() {
            if let Ok(content) = fs::read_to_string(&pyproject) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("license")
                        && trimmed.contains("=")
                        && !trimmed.starts_with("dynamic")
                    {
                        // Handle TOML table format: license = { text = "BSD-3-Clause" }
                        if trimmed.contains("{ text =") || trimmed.contains("{text=") {
                            // Extract value from { text = "VALUE" }
                            if let Some(start) = trimmed.find("{ text") {
                                let after_brace = &trimmed[start..];
                                if let Some(eq_pos) = after_brace.find('=') {
                                    let value_part = &after_brace[eq_pos + 1..];
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
                        }
                        // Handle simple string format: license = "MIT"
                        else if let Some(license) = trimmed.split('=').nth(1) {
                            let license =
                                license.trim().trim_matches('"').trim_matches('\'').trim();
                            if !license.is_empty()
                                && !license.starts_with('[')
                                && !license.starts_with('{')
                            {
                                info!("Found license in pyproject.toml: {}", license);
                                return Some(license.to_string());
                            }
                        }
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
                                .trim_matches('"')
                                .trim_matches('\'')
                                .trim_matches(',')
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

    /// Recursively collect .py files from a directory (Python only, no C++/C)
    fn collect_py_files(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir)?;
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    // ONLY collect .py files (exclude C++/C for hybrid codebases like PyTorch)
                    if ext == "py" {
                        files.push(path);
                    }
                }
            } else if path.is_dir() {
                // Skip common non-source directories
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
                    ) {
                        self.collect_py_files(&path, files)?;
                    }
                }
            }
        }

        Ok(())
    }
}
