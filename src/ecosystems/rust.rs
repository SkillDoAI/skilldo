//! Rust ecosystem handler — discovers source/test/doc/example files, extracts
//! metadata (name, version, license, URLs) from Cargo.toml, and detects the
//! crate name. Used by the collector for Rust projects.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::ecosystems::{classify_license, LICENSE_FILENAMES};

/// Extract the raw value of a field from the `[package]` section of a Cargo.toml string.
/// Returns the value trimmed of whitespace and outer quotes, or `None` if not found.
/// Only matches fields within the `[package]` section (not `[dependencies]` etc.).
pub fn cargo_toml_field(content: &str, field: &str) -> Option<String> {
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package {
            if let Some(eq_pos) = trimmed.find('=') {
                let lhs = trimmed[..eq_pos].trim();
                if lhs != field {
                    continue;
                }
                let mut rhs = trimmed[eq_pos + 1..].trim();
                // Strip inline TOML comments: walk chars tracking quote state
                // to find the first ` #` outside any quoted string.
                let mut in_double = false;
                let mut in_single = false;
                let mut comment_pos = None;
                let bytes = rhs.as_bytes();
                for (i, &b) in bytes.iter().enumerate() {
                    match b {
                        b'"' if !in_single => in_double = !in_double,
                        b'\'' if !in_double => in_single = !in_single,
                        b'#' if !in_double && !in_single && i > 0 && bytes[i - 1] == b' ' => {
                            comment_pos = Some(i - 1);
                            break;
                        }
                        _ => {}
                    }
                }
                if let Some(pos) = comment_pos {
                    rhs = rhs[..pos].trim();
                }
                let val = rhs.trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

pub struct RustHandler {
    repo_path: PathBuf,
}

impl RustHandler {
    const MAX_DEPTH: usize = 20;

    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    // ── File discovery ──────────────────────────────────────────────────

    /// Find all Rust source files (excluding tests, target, benches, examples).
    pub fn find_source_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_rs_files(&self.repo_path, &mut files, 0)?;

        if files.is_empty() {
            bail!("No Rust source files found in {}", self.repo_path.display());
        }

        files.sort_by_key(|p| self.file_priority(p));
        info!("Found {} Rust source files", files.len());
        Ok(files)
    }

    /// Find all Rust test files (files in tests/ dir + *_test.rs files).
    pub fn find_test_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        // Strategy 1: files in the tests/ directory
        let tests_dir = self.repo_path.join("tests");
        if tests_dir.is_dir() {
            self.collect_all_rs_in_dir(&tests_dir, &mut files, 0)?;
        }

        // Strategy 2: *_test.rs files anywhere in the source tree
        self.collect_test_rs_files(&self.repo_path, &mut files, 0)?;

        files.sort();
        files.dedup();

        if files.is_empty() {
            // Not fatal for Rust: inline #[cfg(test)] modules are captured
            // in source_content and are visible to the LLM pipeline.
            info!(
                "No standalone test files in {}; inline #[cfg(test)] modules \
                 in source files will be used instead",
                self.repo_path.display()
            );
        } else {
            info!("Found {} Rust test files", files.len());
        }
        Ok(files)
    }

    /// Find example files (files in examples/ directory).
    pub fn find_examples(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        let examples_dir = self.repo_path.join("examples");
        if examples_dir.is_dir() {
            self.collect_all_rs_in_dir(&examples_dir, &mut files, 0)?;
        }

        files.sort();
        info!("Found {} Rust example files", files.len());
        Ok(files)
    }

    /// Find documentation files (README, *.md at root, docs/ directory).
    pub fn find_docs(&self) -> Result<Vec<PathBuf>> {
        let mut docs = Vec::new();

        // README at root
        for name in &["README.md", "README.rst", "README.txt", "README"] {
            let path = self.repo_path.join(name);
            if path.exists() {
                docs.push(path);
                break;
            }
        }

        // Other *.md files at root (excluding README already added, CHANGELOG handled separately)
        if let Ok(entries) = fs::read_dir(&self.repo_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "md" {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                // Skip files already added or changelog files
                                if !name.starts_with("README")
                                    && !name.starts_with("CHANGELOG")
                                    && !name.starts_with("CHANGES")
                                    && !name.starts_with("HISTORY")
                                {
                                    docs.push(path);
                                }
                            }
                        }
                    }
                }
            }
        }

        // docs/ and doc/ directories
        for docs_dirname in &["docs", "doc"] {
            let docs_dir = self.repo_path.join(docs_dirname);
            if docs_dir.is_dir() {
                self.collect_docs_recursive(&docs_dir, &mut docs, 0)?;
            }
        }

        docs.sort();
        docs.dedup();
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

    /// Extract package name from Cargo.toml `[package]` section.
    pub fn get_package_name(&self) -> Result<String> {
        let cargo_toml = self.repo_path.join("Cargo.toml");
        let content = fs::read_to_string(&cargo_toml)
            .map_err(|e| anyhow::anyhow!("Cannot read Cargo.toml: {e}"))?;

        cargo_toml_field(&content, "name")
            .ok_or_else(|| anyhow::anyhow!("No package name found in Cargo.toml"))
    }

    /// Extract version from Cargo.toml, falling back to git tags, then "latest".
    pub fn get_version(&self) -> Result<String> {
        // Strategy 1: Cargo.toml [package] version (most authoritative for Rust)
        let cargo_toml = self.repo_path.join("Cargo.toml");
        if let Ok(content) = fs::read_to_string(&cargo_toml) {
            if let Some(version) = cargo_toml_field(&content, "version") {
                // Reject workspace-inherited versions like { workspace = true }
                if !version.contains("workspace") && !version.contains('{') && version.contains('.')
                {
                    return Ok(version);
                }
            }
        }

        // Strategy 2: git tags
        if let Some(v) = self.version_from_git_tags() {
            return Ok(v);
        }

        Ok("latest".to_string())
    }

    /// Extract license from Cargo.toml `license` field, then fall back to LICENSE file.
    pub fn get_license(&self) -> Option<String> {
        // Strategy 1: Cargo.toml license field
        let cargo_toml = self.repo_path.join("Cargo.toml");
        if let Ok(content) = fs::read_to_string(&cargo_toml) {
            if let Some(license) = cargo_toml_field(&content, "license") {
                if !license.contains('{') && !license.contains("workspace") {
                    return Some(license);
                }
            }
        }

        // Strategy 2: LICENSE file classification
        for name in LICENSE_FILENAMES {
            let path = self.repo_path.join(name);
            if let Ok(content) = fs::read_to_string(&path) {
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

    /// Extract project URLs from Cargo.toml `[package]` section.
    pub fn get_project_urls(&self) -> Vec<(String, String)> {
        let mut urls = Vec::new();

        let cargo_toml = self.repo_path.join("Cargo.toml");
        let Ok(content) = fs::read_to_string(&cargo_toml) else {
            return urls;
        };

        if let Some(repo) = cargo_toml_field(&content, "repository") {
            urls.push(("Repository".into(), repo));
        }
        if let Some(homepage) = cargo_toml_field(&content, "homepage") {
            urls.push(("Homepage".into(), homepage));
        }
        if let Some(docs) = cargo_toml_field(&content, "documentation") {
            urls.push(("Documentation".into(), docs));
        }

        // If no documentation URL but we have a crate name, add docs.rs
        if !urls.iter().any(|(k, _)| k == "Documentation") {
            if let Some(name) = cargo_toml_field(&content, "name") {
                urls.push(("Documentation".into(), format!("https://docs.rs/{name}")));
            }
        }

        urls
    }

    // ── Private helpers ────────────────────────────────────────────────

    /// Collect .rs source files (not tests, not in excluded dirs).
    fn collect_rs_files(&self, dir: &Path, files: &mut Vec<PathBuf>, depth: usize) -> Result<()> {
        if !dir.is_dir() || depth > Self::MAX_DEPTH {
            return Ok(());
        }

        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.path());

        for entry in entries {
            let path = entry.path();
            let ft = entry.file_type()?;

            if ft.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !name.ends_with(".rs") {
                        continue;
                    }
                    // Exclude test files and build scripts from source collection
                    if name.ends_with("_test.rs") || name == "build.rs" {
                        continue;
                    }
                    files.push(path);
                }
            } else if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if Self::should_skip_dir_for_source(name) {
                        continue;
                    }
                    self.collect_rs_files(&path, files, depth + 1)?;
                }
            }
        }

        Ok(())
    }

    /// Collect all .rs files in a directory (for tests/ and examples/).
    fn collect_all_rs_in_dir(
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
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".rs") {
                        files.push(path);
                    }
                }
            } else if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !Self::should_skip_dir(name) {
                        self.collect_all_rs_in_dir(&path, files, depth + 1)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Collect *_test.rs files from the source tree (excluding target/, tests/, etc.).
    fn collect_test_rs_files(
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
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with("_test.rs") {
                        files.push(path);
                    }
                }
            } else if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Skip target/ and tests/ (tests/ handled separately)
                    if Self::should_skip_dir(name) || name == "tests" {
                        continue;
                    }
                    self.collect_test_rs_files(&path, files, depth + 1)?;
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
        if depth > Self::MAX_DEPTH {
            return Ok(());
        }

        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.')
                || name == "node_modules"
                || name == "_build"
                || name == "vendor"
                || name == "target"
            {
                return Ok(());
            }
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(ft) = entry.file_type() else { continue };
                if ft.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "md" || ext == "rst" {
                            docs.push(path);
                        }
                    }
                } else if ft.is_dir() {
                    self.collect_docs_recursive(&path, docs, depth + 1)?;
                }
            }
        }
        Ok(())
    }

    /// Directories to skip when collecting source files.
    /// Excludes target/, tests/, benches/, examples/ and standard hidden/build dirs.
    fn should_skip_dir_for_source(name: &str) -> bool {
        matches!(
            name,
            "target"
                | "tests"
                | "benches"
                | "examples"
                | "vendor"
                | ".git"
                | ".github"
                | ".vscode"
                | "node_modules"
                | "build"
                | "dist"
        ) || name.starts_with('.')
    }

    /// Directories to always skip (target, .git, vendor).
    fn should_skip_dir(name: &str) -> bool {
        matches!(name, "target" | ".git" | "vendor" | "node_modules") || name.starts_with('.')
    }

    fn file_priority(&self, path: &Path) -> i32 {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Rust-specific priority: lib.rs and mod.rs highest, then main.rs
        match file_name {
            "lib.rs" => -30,
            "mod.rs" => -25,
            "main.rs" => -20,
            _ => crate::util::calculate_file_priority(path, &self.repo_path),
        }
    }

    fn version_from_git_tags(&self) -> Option<String> {
        let repo = crate::git::Git2Repo::open(&self.repo_path).ok()?;

        if let Some(v) = repo
            .describe_tags()
            .ok()
            .and_then(|tag| parse_version_tag(&tag))
        {
            return Some(v);
        }

        if let Some(v) = repo
            .list_tags_sorted()
            .ok()
            .and_then(|tags| tags.iter().find_map(|tag| parse_version_tag(tag)))
        {
            return Some(v);
        }

        debug!("No local tags found, fetching tags from remote");
        if repo.fetch_tags(std::time::Duration::from_secs(30)).is_ok() {
            if let Some(v) = repo
                .list_tags_sorted()
                .ok()
                .and_then(|tags| tags.iter().find_map(|tag| parse_version_tag(tag)))
            {
                return Some(v);
            }
        }

        None
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Parse a git tag into a version string. Strips `v` prefix and validates semver shape.
fn parse_version_tag(tag: &str) -> Option<String> {
    if tag.is_empty() {
        return None;
    }
    let version = tag.strip_prefix('v').unwrap_or(tag);
    if version.contains('.') && version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        debug!("Version from git tag: {version}");
        Some(version.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_rust_project(name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Cargo.toml
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"1.2.3\"\nlicense = \"MIT\"\nrepository = \"https://github.com/testorg/{name}\"\nhomepage = \"https://{name}.example.com\"\n\n[dependencies]\nserde = \"1.0\"\n"
            ),
        )
        .unwrap();

        // Source files
        fs::create_dir(root.join("src")).unwrap();
        fs::write(
            root.join("src").join("lib.rs"),
            "//! Main library module\npub mod util;\n\npub fn hello() -> &'static str { \"hello\" }\n",
        )
        .unwrap();
        fs::write(
            root.join("src").join("util.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .unwrap();
        fs::write(
            root.join("src").join("main.rs"),
            "fn main() { println!(\"{}\", testcrate::hello()); }\n",
        )
        .unwrap();

        // Nested module
        fs::create_dir(root.join("src").join("internal")).unwrap();
        fs::write(
            root.join("src").join("internal").join("mod.rs"),
            "pub fn helper() {}\n",
        )
        .unwrap();

        // Test files
        fs::create_dir(root.join("tests")).unwrap();
        fs::write(
            root.join("tests").join("integration.rs"),
            "#[test]\nfn test_hello() { assert_eq!(testcrate::hello(), \"hello\"); }\n",
        )
        .unwrap();

        // Examples
        fs::create_dir(root.join("examples")).unwrap();
        fs::write(
            root.join("examples").join("basic.rs"),
            "fn main() { println!(\"basic example\"); }\n",
        )
        .unwrap();

        // Benches (should be excluded from source)
        fs::create_dir(root.join("benches")).unwrap();
        fs::write(root.join("benches").join("perf.rs"), "fn main() {}\n").unwrap();

        // Docs
        fs::write(root.join("README.md"), "# TestCrate\n\nA test crate.\n").unwrap();
        fs::write(root.join("CONTRIBUTING.md"), "# Contributing\n").unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("usage.md"), "# Usage\n").unwrap();

        // Changelog
        fs::write(
            root.join("CHANGELOG.md"),
            "# Changelog\n\n## 1.2.3\n- Initial release\n",
        )
        .unwrap();

        // LICENSE
        fs::write(
            root.join("LICENSE"),
            "MIT License\n\nCopyright (c) 2024 Test\n\nPermission is hereby granted, free of charge...\n",
        )
        .unwrap();

        // target/ (should be skipped)
        fs::create_dir(root.join("target")).unwrap();
        fs::write(
            root.join("target").join("build_artifact.rs"),
            "// artifact\n",
        )
        .unwrap();

        // vendor/ (should be skipped)
        fs::create_dir(root.join("vendor")).unwrap();
        fs::write(root.join("vendor").join("dep.rs"), "// vendor dep\n").unwrap();

        dir
    }

    // ── File discovery tests ──────────────────────────────────────────

    #[test]
    fn find_source_files_finds_rs_files() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        let files = handler.find_source_files().unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"lib.rs"), "should find lib.rs");
        assert!(names.contains(&"util.rs"), "should find util.rs");
        assert!(names.contains(&"main.rs"), "should find main.rs");
        assert!(names.contains(&"mod.rs"), "should find internal/mod.rs");
    }

    #[test]
    fn find_source_files_excludes_target_and_vendor() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        let files = handler.find_source_files().unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(
            !names.contains(&"build_artifact.rs"),
            "should exclude target/"
        );
        assert!(!names.contains(&"dep.rs"), "should exclude vendor/");
    }

    #[test]
    fn find_source_files_excludes_tests_benches_examples() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        let files = handler.find_source_files().unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(!names.contains(&"integration.rs"), "should exclude tests/");
        assert!(!names.contains(&"perf.rs"), "should exclude benches/");
        assert!(!names.contains(&"basic.rs"), "should exclude examples/");
    }

    #[test]
    fn find_source_files_prioritizes_lib_rs() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        let files = handler.find_source_files().unwrap();

        let first_name = files[0].file_name().and_then(|n| n.to_str()).unwrap();
        assert_eq!(
            first_name, "lib.rs",
            "lib.rs should be first (highest priority)"
        );
    }

    #[test]
    fn find_source_files_empty_repo_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"empty\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.find_source_files().is_err());
    }

    #[test]
    fn find_source_files_excludes_test_suffix_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn hello() {}\n").unwrap();
        fs::write(src.join("lib_test.rs"), "#[test] fn t() {}\n").unwrap();

        let handler = RustHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"lib.rs"));
        assert!(
            !names.contains(&"lib_test.rs"),
            "should exclude *_test.rs from source"
        );
    }

    #[test]
    fn find_test_files_finds_tests_dir_and_test_suffix() {
        let project = create_test_rust_project("testcrate");
        // Also add a _test.rs file in src/
        fs::write(
            project.path().join("src").join("util_test.rs"),
            "#[test] fn t() {}\n",
        )
        .unwrap();

        let handler = RustHandler::new(project.path());
        let files = handler.find_test_files().unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(
            names.contains(&"integration.rs"),
            "should find tests/integration.rs"
        );
        assert!(
            names.contains(&"util_test.rs"),
            "should find src/util_test.rs"
        );
    }

    #[test]
    fn find_test_files_returns_empty_for_inline_only() {
        // Projects with only #[cfg(test)] inline modules are valid —
        // find_test_files returns empty Vec, source_content has the tests.
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(
            src.join("lib.rs"),
            "pub fn hello() {}\n#[cfg(test)] mod tests {}\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn find_test_files_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        // A _test.rs file inside tests/ — found by both strategies
        let tests_dir = dir.path().join("tests");
        fs::create_dir(&tests_dir).unwrap();
        fs::write(tests_dir.join("foo_test.rs"), "#[test] fn t() {}\n").unwrap();

        let handler = RustHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();

        let mut sorted = files.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(files.len(), sorted.len(), "should deduplicate test files");
    }

    #[test]
    fn find_examples_finds_example_files() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        let files = handler.find_examples().unwrap();

        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"basic.rs"), "should find examples/basic.rs");
    }

    #[test]
    fn find_examples_empty_when_no_examples_dir() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn hello() {}\n").unwrap();
        let handler = RustHandler::new(dir.path());
        let examples = handler.find_examples().unwrap();
        assert!(examples.is_empty());
    }

    #[test]
    fn find_examples_recurses_into_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("examples").join("advanced");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("demo.rs"), "fn main() {}\n").unwrap();

        let handler = RustHandler::new(dir.path());
        let files = handler.find_examples().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(
            names.contains(&"demo.rs"),
            "should find nested example files"
        );
    }

    #[test]
    fn find_docs_finds_readme_and_docs_dir() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        let docs = handler.find_docs().unwrap();

        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"README.md"));
        assert!(names.contains(&"CONTRIBUTING.md"));
        assert!(names.contains(&"usage.md"));
    }

    #[test]
    fn find_docs_excludes_changelog_from_root_md() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        let docs = handler.find_docs().unwrap();

        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(
            !names.contains(&"CHANGELOG.md"),
            "should not include CHANGELOG in docs"
        );
    }

    #[test]
    fn find_changelog_returns_some() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        assert!(handler.find_changelog().is_some());
    }

    #[test]
    fn find_changelog_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.find_changelog().is_none());
    }

    #[test]
    fn find_changelog_finds_changes_md() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("CHANGES.md"), "# Changes\n").unwrap();
        let handler = RustHandler::new(dir.path());
        let changelog = handler.find_changelog().unwrap();
        assert!(changelog.ends_with("CHANGES.md"));
    }

    // ── Metadata tests ────────────────────────────────────────────────

    #[test]
    fn get_package_name_from_cargo_toml() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        assert_eq!(handler.get_package_name().unwrap(), "testcrate");
    }

    #[test]
    fn get_package_name_no_cargo_toml_errors() {
        let dir = tempfile::tempdir().unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.get_package_name().is_err());
    }

    #[test]
    fn get_package_name_no_name_field_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.get_package_name().is_err());
    }

    #[test]
    fn get_version_from_cargo_toml() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        assert_eq!(handler.get_version().unwrap(), "1.2.3");
    }

    #[test]
    fn get_version_falls_back_to_latest() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "latest");
    }

    #[test]
    fn get_version_rejects_workspace_inherited() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = { workspace = true }\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        // workspace = true is not a real version, should fall through to latest
        assert_eq!(handler.get_version().unwrap(), "latest");
    }

    #[test]
    fn get_license_from_cargo_toml() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        assert_eq!(handler.get_license().unwrap(), "MIT");
    }

    #[test]
    fn get_license_falls_back_to_license_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("LICENSE"),
            "Apache License\nVersion 2.0, January 2004\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_license().unwrap(), "Apache-2.0");
    }

    #[test]
    fn get_license_no_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.get_license().is_none());
    }

    #[test]
    fn get_license_rejects_workspace_inherited() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\nlicense = { workspace = true }\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("LICENSE"),
            "MIT License\n\nPermission is hereby granted, free of charge...\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        // Should skip workspace-inherited and fall back to LICENSE file
        assert_eq!(handler.get_license().unwrap(), "MIT");
    }

    #[test]
    fn get_project_urls_from_cargo_toml() {
        let project = create_test_rust_project("testcrate");
        let handler = RustHandler::new(project.path());
        let urls = handler.get_project_urls();

        assert!(urls
            .iter()
            .any(|(k, v)| k == "Repository" && v == "https://github.com/testorg/testcrate"));
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Homepage" && v == "https://testcrate.example.com"));
    }

    #[test]
    fn get_project_urls_adds_docs_rs_fallback() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\nrepository = \"https://github.com/org/my-crate\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        let urls = handler.get_project_urls();

        assert!(urls
            .iter()
            .any(|(k, v)| k == "Documentation" && v == "https://docs.rs/my-crate"));
    }

    #[test]
    fn get_project_urls_no_docs_rs_when_documentation_present() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\ndocumentation = \"https://my-crate.docs.example.com\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        let urls = handler.get_project_urls();

        // Should have the explicit documentation URL, not docs.rs
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Documentation" && v == "https://my-crate.docs.example.com"));
        assert!(
            !urls.iter().any(|(_, v)| v.contains("docs.rs")),
            "should not add docs.rs when documentation field is present"
        );
    }

    #[test]
    fn get_project_urls_no_cargo_toml_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.get_project_urls().is_empty());
    }

    // ── cargo_toml_field tests ────────────────────────────────────────

    #[test]
    fn cargo_toml_field_extracts_name() {
        let content = "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\n";
        assert_eq!(
            cargo_toml_field(content, "name"),
            Some("my-crate".to_string())
        );
    }

    #[test]
    fn cargo_toml_field_extracts_version() {
        let content = "[package]\nname = \"test\"\nversion = \"3.2.1\"\n";
        assert_eq!(
            cargo_toml_field(content, "version"),
            Some("3.2.1".to_string())
        );
    }

    #[test]
    fn cargo_toml_field_scoped_to_package_section() {
        let content = "[package]\nname = \"real-name\"\n\n[dependencies]\nname = \"dep-name\"\n";
        assert_eq!(
            cargo_toml_field(content, "name"),
            Some("real-name".to_string()),
            "should only match fields in [package] section"
        );
    }

    #[test]
    fn cargo_toml_field_returns_none_for_missing_field() {
        let content = "[package]\nname = \"test\"\n";
        assert_eq!(cargo_toml_field(content, "description"), None);
    }

    #[test]
    fn cargo_toml_field_returns_none_for_missing_section() {
        let content = "[dependencies]\nserde = \"1.0\"\n";
        assert_eq!(cargo_toml_field(content, "name"), None);
    }

    #[test]
    fn cargo_toml_field_handles_single_quotes() {
        let content = "[package]\nname = 'single-quoted'\n";
        assert_eq!(
            cargo_toml_field(content, "name"),
            Some("single-quoted".to_string())
        );
    }

    #[test]
    fn cargo_toml_field_handles_whitespace() {
        let content = "[package]\n  name  =  \"spaced-out\"  \n";
        assert_eq!(
            cargo_toml_field(content, "name"),
            Some("spaced-out".to_string())
        );
    }

    #[test]
    fn cargo_toml_field_stops_at_next_section() {
        let content = "[package]\nname = \"real\"\n\n[lib]\nname = \"lib-name\"\n";
        assert_eq!(
            cargo_toml_field(content, "name"),
            Some("real".to_string()),
            "should not match [lib] section"
        );
    }

    #[test]
    fn cargo_toml_field_extracts_license() {
        let content = "[package]\nname = \"test\"\nlicense = \"MIT OR Apache-2.0\"\n";
        assert_eq!(
            cargo_toml_field(content, "license"),
            Some("MIT OR Apache-2.0".to_string())
        );
    }

    #[test]
    fn cargo_toml_field_extracts_repository() {
        let content = "[package]\nname = \"test\"\nrepository = \"https://github.com/org/repo\"\n";
        assert_eq!(
            cargo_toml_field(content, "repository"),
            Some("https://github.com/org/repo".to_string())
        );
    }

    // ── parse_version_tag tests ──────────────────────────────────────

    #[test]
    fn parse_version_tag_strips_v_prefix() {
        assert_eq!(parse_version_tag("v1.0.0"), Some("1.0.0".into()));
    }

    #[test]
    fn parse_version_tag_no_prefix() {
        assert_eq!(parse_version_tag("2.3.4"), Some("2.3.4".into()));
    }

    #[test]
    fn parse_version_tag_empty() {
        assert_eq!(parse_version_tag(""), None);
    }

    #[test]
    fn parse_version_tag_non_version() {
        assert_eq!(parse_version_tag("release"), None);
        assert_eq!(parse_version_tag("v"), None);
    }

    #[test]
    fn parse_version_tag_prerelease() {
        assert_eq!(
            parse_version_tag("v0.5.0-alpha.1"),
            Some("0.5.0-alpha.1".into())
        );
    }

    // ── should_skip_dir tests ────────────────────────────────────────

    #[test]
    fn should_skip_dir_for_source_standard_exclusions() {
        assert!(RustHandler::should_skip_dir_for_source("target"));
        assert!(RustHandler::should_skip_dir_for_source("tests"));
        assert!(RustHandler::should_skip_dir_for_source("benches"));
        assert!(RustHandler::should_skip_dir_for_source("examples"));
        assert!(RustHandler::should_skip_dir_for_source("vendor"));
        assert!(RustHandler::should_skip_dir_for_source(".git"));
        assert!(RustHandler::should_skip_dir_for_source(".hidden"));
        assert!(!RustHandler::should_skip_dir_for_source("src"));
        assert!(!RustHandler::should_skip_dir_for_source("internal"));
    }

    #[test]
    fn should_skip_dir_always_skips_target_and_git() {
        assert!(RustHandler::should_skip_dir("target"));
        assert!(RustHandler::should_skip_dir(".git"));
        assert!(RustHandler::should_skip_dir("vendor"));
        assert!(!RustHandler::should_skip_dir("src"));
        assert!(!RustHandler::should_skip_dir("tests"));
    }

    // ── Depth limit tests ───────────────────────────────────────────

    #[test]
    fn find_docs_deeply_nested_stops_at_depth_limit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let mut path = root.join("docs");
        for i in 0..22 {
            path = path.join(format!("level{i}"));
        }
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("deep.md"), "# Deep doc\n").unwrap();

        let handler = RustHandler::new(root);
        let docs = handler.find_docs().unwrap();
        assert!(!docs
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n.to_str() == Some("deep.md"))));
    }

    #[test]
    fn find_docs_skips_hidden_and_target_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let hidden = root.join("docs").join(".hidden");
        fs::create_dir_all(&hidden).unwrap();
        fs::write(hidden.join("secret.md"), "# Secret\n").unwrap();

        let target = root.join("docs").join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("build.md"), "# Build\n").unwrap();

        fs::write(root.join("docs").join("guide.md"), "# Guide\n").unwrap();

        let handler = RustHandler::new(root);
        let docs = handler.find_docs().unwrap();
        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"guide.md"), "should find normal docs");
        assert!(!names.contains(&"secret.md"), "should skip .hidden/");
        assert!(!names.contains(&"build.md"), "should skip target/");
    }

    // ── Git tag version tests ────────────────────────────────────────

    #[test]
    fn get_version_from_git_tags() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();
        let src = root.join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn hello() {}\n").unwrap();

        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap()
        };
        git(&["init"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["add", "."]);
        git(&["commit", "-m", "init", "--no-gpg-sign"]);
        git(&["tag", "v2.0.0"]);

        let handler = RustHandler::new(root);
        // No version in Cargo.toml → falls through to git tags
        assert_eq!(handler.get_version().unwrap(), "2.0.0");
    }

    #[test]
    fn get_version_prefers_cargo_toml_over_git_tags() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"3.0.0\"\n",
        )
        .unwrap();
        let src = root.join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn hello() {}\n").unwrap();

        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .unwrap()
        };
        git(&["init"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["add", "."]);
        git(&["commit", "-m", "init", "--no-gpg-sign"]);
        git(&["tag", "v4.0.0"]);

        let handler = RustHandler::new(root);
        assert_eq!(
            handler.get_version().unwrap(),
            "3.0.0",
            "Cargo.toml version should take priority over git tags"
        );
    }

    // ── License fallback (unclassified) ──────────────────────────────

    #[test]
    fn get_license_unclassified_returns_first_line() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("LICENSE"),
            "Custom Proprietary License v1.0\n\nDo whatever you want.\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(
            handler.get_license().unwrap(),
            "Custom Proprietary License v1.0"
        );
    }

    #[test]
    fn get_license_from_licence_spelling() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("LICENCE"),
            "MIT License\n\nPermission is hereby granted...\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_license().unwrap(), "MIT");
    }

    // ── Collect files edge cases ─────────────────────────────────────

    #[test]
    fn collect_rs_files_skips_non_rs_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(src.join("notes.txt"), "not rust\n").unwrap();
        fs::write(src.join("config.json"), "{}\n").unwrap();

        let handler = RustHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert!(files
            .iter()
            .all(|p| p.extension().is_some_and(|e| e == "rs")));
    }

    #[test]
    fn collect_rs_files_recurses_into_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub mod deep;\n").unwrap();
        let deep = src.join("deep");
        fs::create_dir(&deep).unwrap();
        fs::write(deep.join("mod.rs"), "pub fn nested() {}\n").unwrap();

        let handler = RustHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert!(files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "mod.rs")));
    }

    // ── File priority tests ──────────────────────────────────────────

    #[test]
    fn file_priority_lib_rs_highest() {
        let dir = tempfile::tempdir().unwrap();
        let handler = RustHandler::new(dir.path());
        let lib_priority = handler.file_priority(&dir.path().join("src").join("lib.rs"));
        let mod_priority = handler.file_priority(&dir.path().join("src").join("mod.rs"));
        let main_priority = handler.file_priority(&dir.path().join("src").join("main.rs"));

        assert!(
            lib_priority < mod_priority,
            "lib.rs should have higher priority than mod.rs"
        );
        assert!(
            mod_priority < main_priority,
            "mod.rs should have higher priority than main.rs"
        );
    }

    // ── Extra coverage tests ─────────────────────────────────────────

    #[test]
    fn cargo_toml_field_returns_none_for_empty_value() {
        let content = "[package]\nname = \"\"\nversion = \"1.0.0\"\n";
        assert_eq!(cargo_toml_field(content, "name"), None);
    }

    #[test]
    fn get_license_workspace_inherited_no_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nlicense = { workspace = true }\n",
        )
        .unwrap();
        // No LICENSE file at all
        let handler = RustHandler::new(dir.path());
        assert!(handler.get_license().is_none());
    }

    #[test]
    fn find_docs_with_non_md_root_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("README.md"), "# Readme\n").unwrap();
        fs::write(root.join("build.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();

        let handler = RustHandler::new(root);
        let docs = handler.find_docs().unwrap();
        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"README.md"));
        assert!(!names.contains(&"build.rs"), "should not include .rs files");
    }

    #[test]
    fn find_test_files_in_nested_tests_dir() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("tests").join("integration");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("api_test.rs"), "#[test] fn t() {}\n").unwrap();

        let handler = RustHandler::new(dir.path());
        let files = handler.find_test_files().unwrap();
        assert!(files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "api_test.rs")));
    }

    #[test]
    fn cargo_toml_field_strips_inline_comment() {
        let content = "[package]\nname = \"my-crate\" # this is a comment\nversion = \"2.0.0\"\n";
        assert_eq!(
            cargo_toml_field(content, "name"),
            Some("my-crate".to_string())
        );
        assert_eq!(
            cargo_toml_field(content, "version"),
            Some("2.0.0".to_string())
        );
    }

    #[test]
    fn cargo_toml_field_preserves_hash_inside_quotes() {
        let content = "[package]\ndescription = \"A # symbol inside\"\n";
        assert_eq!(
            cargo_toml_field(content, "description"),
            Some("A # symbol inside".to_string())
        );
    }

    #[test]
    fn cargo_toml_field_apostrophe_in_double_quoted_value_with_comment() {
        // Regression: mixed quote counting must track " and ' separately
        let content = "[package]\ndescription = \"it's # great\" # comment\n";
        assert_eq!(
            cargo_toml_field(content, "description"),
            Some("it's # great".to_string())
        );
    }

    #[test]
    fn cargo_toml_field_preserves_hash_inside_single_quotes() {
        let content = "[package]\ndescription = 'has # inside'\n";
        assert_eq!(
            cargo_toml_field(content, "description"),
            Some("has # inside".to_string())
        );
    }

    #[test]
    fn cargo_toml_field_single_quoted_value_with_trailing_comment() {
        let content = "[package]\nname = 'my-crate' # comment\n";
        assert_eq!(
            cargo_toml_field(content, "name"),
            Some("my-crate".to_string())
        );
    }

    #[test]
    fn get_project_urls_no_name_no_docs_rs() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        let urls = handler.get_project_urls();
        // No name field means no docs.rs fallback
        assert!(!urls.iter().any(|(_, v)| v.contains("docs.rs")));
    }

    #[test]
    fn find_test_files_discovers_test_rs_in_src() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn x() {}\n").unwrap();
        fs::write(src.join("utils_test.rs"), "#[test] fn t() {}\n").unwrap();
        // Also put one in tests/ dir for the other strategy
        let tests = root.join("tests");
        fs::create_dir_all(&tests).unwrap();
        fs::write(tests.join("integration.rs"), "#[test] fn t() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_test_files().unwrap();
        assert!(
            files.iter().any(|p| p.ends_with("utils_test.rs")),
            "should discover *_test.rs files in src"
        );
        assert!(
            files.iter().any(|p| p.ends_with("integration.rs")),
            "should discover files in tests/ dir"
        );
    }

    #[test]
    fn find_docs_with_nested_docs_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs");
        let nested = docs.join("guides");
        fs::create_dir_all(&nested).unwrap();
        fs::write(docs.join("overview.md"), "# Overview\n").unwrap();
        fs::write(nested.join("quickstart.md"), "# Quickstart\n").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_docs().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(
            names.contains(&"overview.md"),
            "should find docs in docs/ dir"
        );
        assert!(
            names.contains(&"quickstart.md"),
            "should find docs in nested docs/ subdir"
        );
    }

    #[test]
    fn collect_rs_files_skips_deep_nesting() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn x() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(!files.is_empty());
    }

    #[test]
    fn find_docs_with_vendor_dir_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Create a docs dir with a vendor subdir that should be skipped
        let docs = root.join("docs");
        fs::create_dir_all(docs.join("vendor")).unwrap();
        fs::write(docs.join("readme.md"), "# Docs\n").unwrap();
        fs::write(docs.join("vendor").join("third_party.md"), "# Vendor\n").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_docs().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"readme.md"));
        // vendor dir should be excluded by collect_docs_recursive
        assert!(
            !names.contains(&"third_party.md"),
            "vendor dir should be excluded"
        );
    }

    #[test]
    fn find_source_files_in_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        let nested = src.join("utils");
        fs::create_dir_all(&nested).unwrap();
        fs::write(src.join("lib.rs"), "mod utils;\n").unwrap();
        fs::write(nested.join("helpers.rs"), "pub fn help() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(files.iter().any(|p| p.ends_with("helpers.rs")));
    }

    #[test]
    fn find_examples_with_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let examples = root.join("examples");
        let sub = examples.join("advanced");
        fs::create_dir_all(&sub).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(examples.join("basic.rs"), "fn main() {}\n").unwrap();
        fs::write(sub.join("demo.rs"), "fn main() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_examples().unwrap();
        assert!(files.iter().any(|p| p.ends_with("basic.rs")));
        assert!(files.iter().any(|p| p.ends_with("demo.rs")));
    }

    #[test]
    fn find_test_files_with_test_rs_in_nested_src() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src").join("net");
        fs::create_dir_all(&src).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(src.join("client_test.rs"), "fn test_it() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_test_files().unwrap();
        assert!(
            files.iter().any(|p| p.ends_with("client_test.rs")),
            "should find _test.rs in nested src dirs: {files:?}"
        );
    }

    #[test]
    fn collect_docs_recursive_with_nested_docs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs");
        let sub = docs.join("api");
        fs::create_dir_all(&sub).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(docs.join("guide.md"), "# Guide\n").unwrap();
        fs::write(sub.join("ref.md"), "# API Ref\n").unwrap();

        let handler = RustHandler::new(root);
        let found = handler.find_docs().unwrap();
        assert!(found.iter().any(|p| p.ends_with("guide.md")));
        assert!(found.iter().any(|p| p.ends_with("ref.md")));
    }

    #[test]
    fn collect_docs_recursive_skips_vendor_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs");
        let vendor = docs.join("vendor");
        fs::create_dir_all(&vendor).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(docs.join("guide.md"), "# Guide\n").unwrap();
        fs::write(vendor.join("third_party.md"), "# Vendor\n").unwrap();

        let handler = RustHandler::new(root);
        let found = handler.find_docs().unwrap();
        assert!(found.iter().any(|p| p.ends_with("guide.md")));
        assert!(
            !found.iter().any(|p| p.ends_with("third_party.md")),
            "should skip vendor/ inside docs/"
        );
    }

    #[test]
    fn find_docs_includes_rst_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("doc");
        fs::create_dir_all(&docs).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(docs.join("index.rst"), "Title\n=====\n").unwrap();

        let handler = RustHandler::new(root);
        let found = handler.find_docs().unwrap();
        assert!(found.iter().any(|p| p.ends_with("index.rst")));
    }

    #[test]
    fn collect_rs_files_excludes_test_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn x() {}\n").unwrap();
        fs::write(src.join("lib_test.rs"), "fn test_x() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(files.iter().any(|p| p.ends_with("lib.rs")));
        assert!(
            !files.iter().any(|p| p.ends_with("lib_test.rs")),
            "source files should exclude _test.rs"
        );
    }

    #[test]
    fn find_docs_filters_readme_and_changelog_at_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(root.join("README.md"), "# Readme\n").unwrap();
        fs::write(root.join("CHANGELOG.md"), "# Changes\n").unwrap();
        fs::write(root.join("CONTRIBUTING.md"), "# Contrib\n").unwrap();

        let handler = RustHandler::new(root);
        let found = handler.find_docs().unwrap();
        // README and CHANGELOG handled separately; CONTRIBUTING.md should be included
        assert!(
            found.iter().any(|p| p.ends_with("CONTRIBUTING.md")),
            "non-changelog root docs should be found"
        );
    }

    #[test]
    fn get_version_from_inline_commented_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"my-lib\"\nversion = \"3.2.1\" # stable\n",
        )
        .unwrap();

        let handler = RustHandler::new(root);
        let v = handler.get_version().unwrap();
        assert_eq!(v, "3.2.1");
    }

    #[test]
    fn get_license_workspace_inherited_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nlicense = { workspace = true }\n",
        )
        .unwrap();
        let handler = RustHandler::new(root);
        assert!(handler.get_license().is_none());
    }

    #[test]
    fn get_version_no_cargo_toml_falls_back() {
        // No Cargo.toml at all → falls back to "latest"
        let dir = tempfile::tempdir().unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "latest");
    }

    #[test]
    fn collect_rs_files_filters_non_rs_and_txt_json() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn x() {}\n").unwrap();
        fs::write(src.join("notes.txt"), "not rust\n").unwrap();
        fs::write(src.join("data.json"), "{}\n").unwrap();

        let handler = RustHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert!(files.iter().any(|p| p.ends_with("lib.rs")));
        assert!(!files.iter().any(|p| p.ends_with("notes.txt")));
        assert!(!files.iter().any(|p| p.ends_with("data.json")));
    }

    #[test]
    fn collect_rs_files_excludes_build_rs() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn x() {}\n").unwrap();
        fs::write(dir.path().join("build.rs"), "fn main() {}\n").unwrap();

        let handler = RustHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert!(files.iter().any(|p| p.ends_with("lib.rs")));
        assert!(
            !files.iter().any(|p| p.ends_with("build.rs")),
            "build.rs should be excluded from source collection"
        );
    }

    #[test]
    fn collect_rs_files_skips_excluded_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let target = src.join("target");
        let benches = src.join("benches");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&benches).unwrap();
        fs::write(src.join("lib.rs"), "pub fn x() {}\n").unwrap();
        fs::write(target.join("gen.rs"), "// generated\n").unwrap();
        fs::write(benches.join("bench.rs"), "fn bench() {}\n").unwrap();

        let handler = RustHandler::new(dir.path());
        let files = handler.find_source_files().unwrap();
        assert!(files.iter().any(|p| p.ends_with("lib.rs")));
        assert!(!files.iter().any(|p| p.ends_with("gen.rs")));
        assert!(!files.iter().any(|p| p.ends_with("bench.rs")));
    }

    #[test]
    fn find_examples_skips_non_rs_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let examples = root.join("examples");
        fs::create_dir(&examples).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(examples.join("demo.rs"), "fn main() {}\n").unwrap();
        fs::write(examples.join("config.toml"), "key = 1\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_examples().unwrap();
        assert!(files.iter().any(|p| p.ends_with("demo.rs")));
        assert!(!files.iter().any(|p| p.ends_with("config.toml")));
    }

    #[test]
    fn find_test_files_skips_excluded_dirs_in_test_search() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        let target = root.join("target");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(src.join("client_test.rs"), "fn test_it() {}\n").unwrap();
        fs::write(target.join("gen_test.rs"), "fn gen_test() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_test_files().unwrap();
        assert!(files.iter().any(|p| p.ends_with("client_test.rs")));
        assert!(
            !files.iter().any(|p| p.ends_with("gen_test.rs")),
            "should skip target/"
        );
    }

    #[test]
    fn collect_docs_skips_non_doc_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs");
        fs::create_dir(&docs).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(docs.join("guide.md"), "# Guide\n").unwrap();
        fs::write(docs.join("data.json"), "{}\n").unwrap();

        let handler = RustHandler::new(root);
        let found = handler.find_docs().unwrap();
        assert!(found.iter().any(|p| p.ends_with("guide.md")));
        assert!(!found.iter().any(|p| p.ends_with("data.json")));
    }

    #[test]
    fn version_from_git_tags_with_tagged_repo() {
        use std::process::Command;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Init a git repo with a tag
        Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(root)
            .output()
            .unwrap();

        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        let src = root.join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn x() {}\n").unwrap();

        Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init", "--no-gpg-sign"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["tag", "v2.5.0"])
            .current_dir(root)
            .output()
            .unwrap();

        let handler = RustHandler::new(root);
        // No version in Cargo.toml, so it falls back to git tags
        let v = handler.get_version().unwrap();
        assert_eq!(v, "2.5.0");
    }

    #[test]
    fn parse_version_tag_edge_cases() {
        assert_eq!(
            parse_version_tag("v0.1.0-rc1"),
            Some("0.1.0-rc1".to_string())
        );
        assert_eq!(parse_version_tag(""), None);
        assert_eq!(parse_version_tag("latest"), None);
        assert_eq!(parse_version_tag("release"), None);
    }

    #[test]
    fn get_license_unrecognized_returns_first_line() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("LICENSE"),
            "Custom License v42\n\nDo whatever you want.\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_license().unwrap(), "Custom License v42");
    }

    #[test]
    fn find_changelog_finds_plain_changelog() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("CHANGELOG"), "# Changes\n").unwrap();
        let handler = RustHandler::new(dir.path());
        let changelog = handler.find_changelog().unwrap();
        assert!(changelog.ends_with("CHANGELOG"));
    }

    #[test]
    fn find_changelog_finds_history_md() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("HISTORY.md"), "# History\n").unwrap();
        let handler = RustHandler::new(dir.path());
        let changelog = handler.find_changelog().unwrap();
        assert!(changelog.ends_with("HISTORY.md"));
    }

    #[test]
    fn get_project_urls_includes_documentation_field() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\ndocumentation = \"https://x.docs.io\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert!(
            urls.iter()
                .any(|(k, v)| k == "Documentation" && v == "https://x.docs.io"),
            "should find explicit documentation URL: {:?}",
            urls
        );
    }

    #[test]
    fn file_priority_mod_rs_between_lib_and_main() {
        let dir = tempfile::tempdir().unwrap();
        let handler = RustHandler::new(dir.path());
        let lib = handler.file_priority(Path::new("src/lib.rs"));
        let mod_rs = handler.file_priority(Path::new("src/mod.rs"));
        let main = handler.file_priority(Path::new("src/main.rs"));
        assert!(lib < mod_rs, "lib.rs should rank above mod.rs");
        assert!(mod_rs < main, "mod.rs should rank above main.rs");
    }
}
