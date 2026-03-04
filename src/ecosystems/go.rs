//! Go ecosystem handler — discovers source/test/doc/example files, extracts
//! metadata (name, version, license, URLs) from go.mod, and detects the module
//! path. Used by the collector for Go projects.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

pub struct GoHandler {
    repo_path: PathBuf,
}

impl GoHandler {
    const MAX_DEPTH: usize = 20;

    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    // ── File discovery ──────────────────────────────────────────────────

    /// Find all Go source files (excluding tests, vendor, testdata).
    pub fn find_source_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_go_files(&self.repo_path, &mut files, 0, false)?;

        if files.is_empty() {
            bail!("No Go source files found in {}", self.repo_path.display());
        }

        files.sort_by_key(|p| self.file_priority(p));
        info!("Found {} Go source files", files.len());
        Ok(files)
    }

    /// Find all Go test files (*_test.go).
    pub fn find_test_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_go_files(&self.repo_path, &mut files, 0, true)?;

        if files.is_empty() {
            bail!(
                "No tests found in {}. Tests are required for generating skills.",
                self.repo_path.display()
            );
        }

        info!("Found {} Go test files", files.len());
        Ok(files)
    }

    /// Find example files (examples/, example/ dirs, example_*_test.go).
    pub fn find_examples(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for dir_name in &["examples", "example", "_examples"] {
            let dir = self.repo_path.join(dir_name);
            if dir.is_dir() {
                // Collect all .go files in example dirs (including tests — they're examples)
                self.collect_all_go_in_dir(&dir, &mut files, 0)?;
            }
        }

        // Go convention: example_*_test.go files at package root are testable examples
        self.collect_example_test_files(&self.repo_path, &mut files, 0)?;

        files.sort();
        files.dedup();
        info!("Found {} Go example files", files.len());
        Ok(files)
    }

    /// Find documentation files (README, docs/, doc.go).
    pub fn find_docs(&self) -> Result<Vec<PathBuf>> {
        let mut docs = Vec::new();

        for name in &["README.md", "README.rst", "README.txt", "README"] {
            let path = self.repo_path.join(name);
            if path.exists() {
                docs.push(path);
                break;
            }
        }

        // doc.go at repo root (Go convention for package documentation)
        let doc_go = self.repo_path.join("doc.go");
        if doc_go.is_file() {
            docs.push(doc_go);
        }

        for docs_dirname in &["docs", "doc"] {
            let docs_dir = self.repo_path.join(docs_dirname);
            if docs_dir.is_dir() {
                self.collect_docs_recursive(&docs_dir, &mut docs, 0)?;
            }
        }

        info!("Found {} documentation files", docs.len());
        Ok(docs)
    }

    /// Find changelog file.
    pub fn find_changelog(&self) -> Option<PathBuf> {
        for name in &[
            "CHANGELOG.md",
            "CHANGELOG",
            "CHANGES.md",
            "CHANGES",
            "HISTORY.md",
        ] {
            let path = self.repo_path.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
        None
    }

    // ── Metadata extraction ────────────────────────────────────────────

    /// Extract package name from go.mod module path (last path segment).
    pub fn get_package_name(&self) -> Result<String> {
        let module_path = self.read_module_path()?;
        let name = module_path
            .rsplit('/')
            .next()
            .unwrap_or(&module_path)
            .to_string();
        Ok(name)
    }

    /// Extract version. Tries git tags first, then falls back to "latest".
    pub fn get_version(&self) -> Result<String> {
        // Strategy 1: git describe for latest tag
        if let Some(v) = self.version_from_git_tags() {
            return Ok(v);
        }

        // Strategy 2: version.go or version constant
        if let Some(v) = self.version_from_source() {
            return Ok(v);
        }

        Ok("latest".to_string())
    }

    /// Extract license text from LICENSE file.
    pub fn get_license(&self) -> Option<String> {
        for name in &["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENCE", "COPYING"] {
            let path = self.repo_path.join(name);
            if let Ok(content) = fs::read_to_string(&path) {
                // Return first line (usually the license name) or classify
                if let Some(license) = classify_license(&content) {
                    return Some(license);
                }
                // Fallback: first non-empty line
                return content
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .map(String::from);
            }
        }
        None
    }

    /// Derive project URLs from module path.
    pub fn get_project_urls(&self) -> Vec<(String, String)> {
        let mut urls = Vec::new();

        if let Ok(module_path) = self.read_module_path() {
            // pkg.go.dev documentation
            urls.push((
                "Documentation".into(),
                format!("https://pkg.go.dev/{module_path}"),
            ));

            // If it's a GitHub/GitLab/Bitbucket module, add source URL
            if module_path.starts_with("github.com/")
                || module_path.starts_with("gitlab.com/")
                || module_path.starts_with("bitbucket.org/")
            {
                urls.push(("Source".into(), format!("https://{module_path}")));
            }
        }

        urls
    }

    /// Read the full module path from go.mod.
    pub fn read_module_path(&self) -> Result<String> {
        let go_mod = self.repo_path.join("go.mod");
        let content =
            fs::read_to_string(&go_mod).map_err(|e| anyhow::anyhow!("Cannot read go.mod: {e}"))?;

        parse_module_path(&content)
            .ok_or_else(|| anyhow::anyhow!("No module directive found in go.mod"))
    }

    // ── Private helpers ────────────────────────────────────────────────

    /// Collect .go source files (not tests) or test files depending on `tests_only`.
    fn collect_go_files(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
        depth: usize,
        tests_only: bool,
    ) -> Result<()> {
        if !dir.is_dir() || depth > Self::MAX_DEPTH {
            return Ok(());
        }

        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.path());

        for entry in entries {
            let path = entry.path();

            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !name.ends_with(".go") {
                        continue;
                    }
                    let is_test = name.ends_with("_test.go");
                    if tests_only == is_test {
                        files.push(path);
                    }
                }
            } else if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if Self::should_skip_dir(name) {
                        continue;
                    }
                    self.collect_go_files(&path, files, depth + 1, tests_only)?;
                }
            }
        }

        Ok(())
    }

    /// Collect all .go files in a directory (both source and test — for examples).
    fn collect_all_go_in_dir(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
        depth: usize,
    ) -> Result<()> {
        if !dir.is_dir() || depth > Self::MAX_DEPTH {
            return Ok(());
        }

        for entry in fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".go") {
                        files.push(path);
                    }
                }
            } else if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !Self::should_skip_dir(name) {
                        self.collect_all_go_in_dir(&path, files, depth + 1)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Collect example_*_test.go files (Go testable examples convention).
    fn collect_example_test_files(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
        depth: usize,
    ) -> Result<()> {
        if !dir.is_dir() || depth > Self::MAX_DEPTH {
            return Ok(());
        }

        for entry in fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("example_") && name.ends_with("_test.go") {
                        files.push(path);
                    }
                }
            } else if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !Self::should_skip_dir(name) {
                        self.collect_example_test_files(&path, files, depth + 1)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn collect_docs_recursive(
        &self,
        dir: &Path,
        docs: &mut Vec<PathBuf>,
        depth: usize,
    ) -> Result<()> {
        if depth > 10 {
            return Ok(());
        }

        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.')
                || name == "node_modules"
                || name == "_build"
                || name == "vendor"
            {
                return Ok(());
            }
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "md" || ext == "rst" {
                            docs.push(path);
                        }
                    }
                } else if path.is_dir() {
                    self.collect_docs_recursive(&path, docs, depth + 1)?;
                }
            }
        }
        Ok(())
    }

    fn should_skip_dir(name: &str) -> bool {
        matches!(
            name,
            "vendor"
                | "testdata"
                | ".git"
                | "node_modules"
                | ".github"
                | ".vscode"
                | "build"
                | "dist"
        ) || name.starts_with('_')
            || name.starts_with('.')
    }

    fn file_priority(&self, path: &Path) -> i32 {
        crate::util::calculate_file_priority(path, &self.repo_path)
    }

    fn version_from_git_tags(&self) -> Option<String> {
        // Run `git describe --tags --abbrev=0` to get latest tag
        let output = std::process::Command::new("git")
            .args(["describe", "--tags", "--abbrev=0"])
            .current_dir(&self.repo_path)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if tag.is_empty() {
            return None;
        }

        // Strip 'v' prefix: v1.2.3 → 1.2.3
        let version = tag.strip_prefix('v').unwrap_or(&tag);
        // Basic semver-ish validation
        if version.contains('.') && version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            debug!("Version from git tag: {version}");
            Some(version.to_string())
        } else {
            None
        }
    }

    fn version_from_source(&self) -> Option<String> {
        // Look for version.go or a Version constant
        for name in &["version.go", "VERSION"] {
            let path = self.repo_path.join(name);
            if let Ok(content) = fs::read_to_string(&path) {
                if let Some(v) = extract_version_constant(&content) {
                    debug!("Version from {name}: {v}");
                    return Some(v);
                }
            }
        }
        None
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Parse module path from go.mod content.
pub(crate) fn parse_module_path(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module") {
            let path = rest.trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// Extract a version string from Go source (e.g., `Version = "1.2.3"`).
fn extract_version_constant(content: &str) -> Option<String> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static VERSION_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?i)(?:Version|VERSION)\s*=\s*"(\d+\.\d+[^"]*)"#).unwrap());

    VERSION_RE.captures(content).map(|cap| cap[1].to_string())
}

/// Classify a license file by its content (first few hundred chars).
fn classify_license(content: &str) -> Option<String> {
    let lower = content.to_lowercase();
    let prefix: String = lower.chars().take(500).collect();

    if prefix.contains("mit license")
        || prefix.contains("permission is hereby granted, free of charge")
    {
        Some("MIT".into())
    } else if prefix.contains("apache license") && prefix.contains("version 2") {
        Some("Apache-2.0".into())
    } else if prefix.contains("bsd 3-clause")
        || prefix.contains("redistribution and use in source and binary")
    {
        Some("BSD-3-Clause".into())
    } else if prefix.contains("bsd 2-clause") {
        Some("BSD-2-Clause".into())
    } else if prefix.contains("mozilla public license") {
        Some("MPL-2.0".into())
    } else if prefix.contains("gnu general public license") {
        if prefix.contains("version 3") {
            Some("GPL-3.0".into())
        } else {
            Some("GPL-2.0".into())
        }
    } else if prefix.contains("the unlicense") || prefix.contains("unlicense") {
        Some("Unlicense".into())
    } else if prefix.contains("isc license") {
        Some("ISC".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_go_project(name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // go.mod
        fs::write(
            root.join("go.mod"),
            format!(
                "module github.com/testorg/{name}\n\ngo 1.21\n\nrequire (\n\tgithub.com/stretchr/testify v1.9.0\n)\n"
            ),
        )
        .unwrap();

        // Source files
        fs::write(
            root.join("lib.go"),
            "package mylib\n\nfunc Hello() string { return \"hello\" }\n",
        )
        .unwrap();
        fs::write(
            root.join("util.go"),
            "package mylib\n\nfunc Add(a, b int) int { return a + b }\n",
        )
        .unwrap();
        fs::write(
            root.join("doc.go"),
            "// Package mylib provides greeting utilities.\npackage mylib\n",
        )
        .unwrap();

        // Test files
        fs::write(
            root.join("lib_test.go"),
            "package mylib\n\nimport \"testing\"\n\nfunc TestHello(t *testing.T) {\n\tif Hello() != \"hello\" { t.Fatal() }\n}\n",
        )
        .unwrap();

        // Example test file (Go convention)
        fs::write(
            root.join("example_test.go"),
            "package mylib\n\nimport \"fmt\"\n\nfunc ExampleHello() {\n\tfmt.Println(Hello())\n\t// Output: hello\n}\n",
        )
        .unwrap();

        // Examples dir
        fs::create_dir(root.join("examples")).unwrap();
        fs::write(
            root.join("examples").join("basic.go"),
            "package main\n\nimport \"fmt\"\n\nfunc main() { fmt.Println(\"basic\") }\n",
        )
        .unwrap();

        // Docs
        fs::write(root.join("README.md"), "# MyLib\n\nA test library.\n").unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("usage.md"), "# Usage\n").unwrap();

        // Changelog
        fs::write(
            root.join("CHANGELOG.md"),
            "# Changelog\n\n## 1.0.0\n- Initial release\n",
        )
        .unwrap();

        // LICENSE
        fs::write(
            root.join("LICENSE"),
            "MIT License\n\nCopyright (c) 2024 Test\n\nPermission is hereby granted, free of charge...\n",
        )
        .unwrap();

        // vendor/ (should be skipped)
        fs::create_dir(root.join("vendor")).unwrap();
        fs::write(root.join("vendor").join("dep.go"), "package dep\n").unwrap();

        // testdata/ (should be skipped)
        fs::create_dir(root.join("testdata")).unwrap();
        fs::write(
            root.join("testdata").join("fixture.go"),
            "package testdata\n",
        )
        .unwrap();

        // Nested package
        fs::create_dir(root.join("internal")).unwrap();
        fs::write(
            root.join("internal").join("helper.go"),
            "package internal\n\nfunc doStuff() {}\n",
        )
        .unwrap();

        dir
    }

    // ── File discovery tests ──────────────────────────────────────────

    #[test]
    fn find_source_files_excludes_tests_and_vendor() {
        let project = create_test_go_project("testlib");
        let handler = GoHandler::new(project.path());
        let files = handler.find_source_files().unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"lib.go"), "should find lib.go");
        assert!(names.contains(&"util.go"), "should find util.go");
        assert!(names.contains(&"doc.go"), "should find doc.go");
        assert!(
            names.contains(&"helper.go"),
            "should find internal/helper.go"
        );
        assert!(!names.contains(&"lib_test.go"), "should exclude test files");
        assert!(!names.contains(&"dep.go"), "should exclude vendor/");
        assert!(!names.contains(&"fixture.go"), "should exclude testdata/");
    }

    #[test]
    fn find_test_files_finds_test_go() {
        let project = create_test_go_project("testlib");
        let handler = GoHandler::new(project.path());
        let files = handler.find_test_files().unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"lib_test.go"), "should find lib_test.go");
        assert!(
            names.contains(&"example_test.go"),
            "should find example_test.go"
        );
        assert!(!names.contains(&"lib.go"), "should exclude source files");
    }

    #[test]
    fn find_examples_finds_example_dirs_and_files() {
        let project = create_test_go_project("testlib");
        let handler = GoHandler::new(project.path());
        let files = handler.find_examples().unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"basic.go"), "should find examples/basic.go");
        assert!(
            names.contains(&"example_test.go"),
            "should find example_*_test.go"
        );
    }

    #[test]
    fn find_docs_finds_readme_and_doc_go() {
        let project = create_test_go_project("testlib");
        let handler = GoHandler::new(project.path());
        let docs = handler.find_docs().unwrap();

        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"README.md"));
        assert!(names.contains(&"doc.go"));
        assert!(names.contains(&"usage.md"));
    }

    #[test]
    fn find_changelog_returns_some() {
        let project = create_test_go_project("testlib");
        let handler = GoHandler::new(project.path());
        assert!(handler.find_changelog().is_some());
    }

    #[test]
    fn find_source_files_empty_repo_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(handler.find_source_files().is_err());
    }

    #[test]
    fn find_test_files_no_tests_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(dir.path().join("main.go"), "package main\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(handler.find_test_files().is_err());
    }

    // ── Metadata tests ────────────────────────────────────────────────

    #[test]
    fn get_package_name_from_module_path() {
        let project = create_test_go_project("testlib");
        let handler = GoHandler::new(project.path());
        assert_eq!(handler.get_package_name().unwrap(), "testlib");
    }

    #[test]
    fn get_package_name_nested_module() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module github.com/org/repo/v2\n\ngo 1.21\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_package_name().unwrap(), "v2");
    }

    #[test]
    fn get_package_name_no_go_mod_errors() {
        let dir = tempfile::tempdir().unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(handler.get_package_name().is_err());
    }

    #[test]
    fn get_version_falls_back_to_latest() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "latest");
    }

    #[test]
    fn get_version_from_version_go() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            dir.path().join("version.go"),
            "package mylib\n\nconst Version = \"2.3.1\"\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "2.3.1");
    }

    #[test]
    fn get_license_classifies_mit() {
        let project = create_test_go_project("testlib");
        let handler = GoHandler::new(project.path());
        assert_eq!(handler.get_license().unwrap(), "MIT");
    }

    #[test]
    fn get_license_classifies_apache() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("LICENSE"),
            "Apache License\nVersion 2.0, January 2004\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_license().unwrap(), "Apache-2.0");
    }

    #[test]
    fn get_license_no_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(handler.get_license().is_none());
    }

    #[test]
    fn get_project_urls_from_github_module() {
        let project = create_test_go_project("testlib");
        let handler = GoHandler::new(project.path());
        let urls = handler.get_project_urls();

        assert!(urls
            .iter()
            .any(|(k, v)| k == "Documentation"
                && v.contains("pkg.go.dev/github.com/testorg/testlib")));
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Source" && v == "https://github.com/testorg/testlib"));
    }

    // ── Free function tests ───────────────────────────────────────────

    #[test]
    fn parse_module_path_basic() {
        assert_eq!(
            parse_module_path("module github.com/foo/bar\n\ngo 1.21\n"),
            Some("github.com/foo/bar".into())
        );
    }

    #[test]
    fn parse_module_path_empty() {
        assert_eq!(parse_module_path("go 1.21\n"), None);
    }

    #[test]
    fn extract_version_constant_basic() {
        assert_eq!(
            extract_version_constant("const Version = \"1.2.3\"\n"),
            Some("1.2.3".into())
        );
    }

    #[test]
    fn extract_version_constant_uppercase() {
        assert_eq!(
            extract_version_constant("var VERSION = \"0.9.0-beta\"\n"),
            Some("0.9.0-beta".into())
        );
    }

    #[test]
    fn extract_version_constant_no_match() {
        assert_eq!(extract_version_constant("package main\n"), None);
    }

    #[test]
    fn classify_license_bsd() {
        assert_eq!(
            classify_license("BSD 3-Clause License\n\nCopyright..."),
            Some("BSD-3-Clause".into())
        );
    }

    #[test]
    fn classify_license_unknown() {
        assert_eq!(classify_license("Some random text"), None);
    }

    #[test]
    fn find_examples_empty_when_no_example_dirs() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(dir.path().join("main.go"), "package main\n").unwrap();
        let handler = GoHandler::new(dir.path());
        let examples = handler.find_examples().unwrap();
        assert!(examples.is_empty());
    }

    #[test]
    fn find_changelog_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(handler.find_changelog().is_none());
    }

    #[test]
    fn should_skip_dir_standard_exclusions() {
        assert!(GoHandler::should_skip_dir("vendor"));
        assert!(GoHandler::should_skip_dir("testdata"));
        assert!(GoHandler::should_skip_dir(".git"));
        assert!(GoHandler::should_skip_dir("_internal"));
        assert!(!GoHandler::should_skip_dir("internal"));
        assert!(!GoHandler::should_skip_dir("pkg"));
        assert!(!GoHandler::should_skip_dir("cmd"));
    }
}
