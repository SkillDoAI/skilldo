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

    /// Extract package name from go.mod module path.
    /// Skips major-version suffixes (v2, v3, etc.) per Go module conventions.
    pub fn get_package_name(&self) -> Result<String> {
        let module_path = self.read_module_path()?;
        let segments: Vec<&str> = module_path.rsplit('/').collect();
        // If the last segment is a major-version suffix (v2, v3, ...), use the parent
        let name = if segments.len() > 1 && is_major_version_suffix(segments[0]) {
            segments[1]
        } else {
            segments[0]
        };
        Ok(name.to_string())
    }

    /// Extract version from local repo state. Strategies in order:
    ///
    /// 1. **Git tags** — `git describe --tags`, then `git tag -l --sort=-v:refname`,
    ///    then fetch + retry. Covers the ~38% of Go projects that version only via tags.
    /// 2. **Root version.go** — `Version = "1.2.3"` string constant at repo root.
    /// 3. **VERSION files** — Plain-text `VERSION` or `VERSION.txt` at repo root
    ///    (prometheus, rclone, tailscale style).
    /// 4. **Version subdirs** — `version/`, `internal/version/`, `pkg/version/`,
    ///    `version/rawversion/`. Also parses Major/Minor/Patch integer constants
    ///    (geth, hugo style). Dev placeholders are filtered out.
    ///
    /// **Not detectable:** ldflags injection (`-X main.Version=...` at build time),
    /// used by ~20% of top Go projects (kubernetes, docker, ollama, caddy, istio, helm).
    /// For these projects, `git describe --tags` still works if they tag releases (most do).
    /// Library owners can specify version via custom instructions for unusual patterns.
    pub fn get_version(&self) -> Result<String> {
        // Strategy 1: git tags (primary — Go modules version via tags)
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
            let ft = entry.file_type()?;

            if ft.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !name.ends_with(".go") {
                        continue;
                    }
                    let is_test = name.ends_with("_test.go");
                    if tests_only == is_test {
                        files.push(path);
                    }
                }
            } else if ft.is_dir() {
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
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".go") {
                        files.push(path);
                    }
                }
            } else if ft.is_dir() {
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
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("example_") && name.ends_with("_test.go") {
                        files.push(path);
                    }
                }
            } else if ft.is_dir() {
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
        if depth > Self::MAX_DEPTH {
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
        // Strategy 1: git describe (fast, works for full clones with reachable tags)
        if let Some(v) = self.git_describe_version() {
            return Some(v);
        }

        // Strategy 2: git tag -l sorted by version (works when tags exist but
        // aren't reachable from HEAD, e.g. shallow clones after tag fetch)
        if let Some(v) = self.latest_version_tag() {
            return Some(v);
        }

        // Strategy 3: fetch tags then retry (shallow clones start with no tags)
        debug!("No local tags found, fetching tags from remote");
        let fetch = std::process::Command::new("git")
            .args(["fetch", "--tags", "--quiet"])
            .current_dir(&self.repo_path)
            .output();
        if fetch.is_ok_and(|o| o.status.success()) {
            if let Some(v) = self.latest_version_tag() {
                return Some(v);
            }
        }

        None
    }

    /// Try `git describe --tags --abbrev=0` for the tag reachable from HEAD.
    fn git_describe_version(&self) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["describe", "--tags", "--abbrev=0"])
            .current_dir(&self.repo_path)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
        parse_version_tag(&tag)
    }

    /// Get the latest semver tag via `git tag -l --sort=-v:refname`.
    fn latest_version_tag(&self) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["tag", "-l", "--sort=-v:refname"])
            .current_dir(&self.repo_path)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|tag| parse_version_tag(tag.trim()))
    }

    fn version_from_source(&self) -> Option<String> {
        // 1. Root-level Go source files
        let path = self.repo_path.join("version.go");
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some(v) = extract_version_constant(&content) {
                debug!("Version from version.go: {v}");
                return Some(v);
            }
        }

        // 2. Plain-text VERSION / VERSION.txt files (just a version string, not Go source)
        for name in &["VERSION", "VERSION.txt"] {
            let path = self.repo_path.join(name);
            if let Ok(content) = fs::read_to_string(&path) {
                if let Some(first_line) = content.lines().next() {
                    if let Some(v) = parse_version_tag(first_line.trim()) {
                        debug!("Version from {name}: {v}");
                        return Some(v);
                    }
                }
            }
        }

        // 3. Common version subdirectories
        for subpath in &[
            "version/version.go",
            "internal/version/version.go",
            "pkg/version/version.go",
            "version/rawversion/version.go",
        ] {
            let path = self.repo_path.join(subpath);
            if let Ok(content) = fs::read_to_string(&path) {
                if let Some(v) = extract_version_constant(&content) {
                    debug!("Version from {subpath}: {v}");
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

/// Extract a version string from Go source. Tries three patterns in order:
/// 1. `Version = "1.2.3"` (simple string constant)
/// 2. `Major = 1; Minor = 17; Patch = 2` (integer constants, geth/hugo style)
///
/// Returns `None` for dev placeholders like "dev", "(untracked)", "library-import".
fn extract_version_constant(content: &str) -> Option<String> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static VERSION_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"(?i)(?:Version|VERSION)\s*=\s*"(\d+\.\d+[^"]*)"#).unwrap());

    // Try simple string constant first
    if let Some(cap) = VERSION_RE.captures(content) {
        let v = cap[1].to_string();
        if !is_dev_placeholder(&v) {
            return Some(v);
        }
    }

    // Fallback: Major/Minor/Patch integer constants
    extract_version_parts(content)
}

/// Check if a version string is a dev/build placeholder that shouldn't be used.
fn is_dev_placeholder(version: &str) -> bool {
    let normalized = version.trim().to_lowercase();
    let normalized = normalized.strip_prefix('v').unwrap_or(&normalized);
    matches!(
        normalized,
        "dev"
            | "development"
            | "unversioned"
            | "unknown"
            | "unknown-dev"
            | "unknown-version"
            | "(untracked)"
            | "library-import"
            | "0.0.0-unset"
            | "0.0.0"
    )
}

/// Extract version from separate Major/Minor/Patch integer constants.
/// Handles both `const` assignment (`Major = 1`) and struct init (`Major: 1`).
/// Also matches `PatchLevel` as an alias for Patch (used by hugo).
fn extract_version_parts(content: &str) -> Option<String> {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static MAJOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bMajor\s*[=:]\s*(\d+)").unwrap());
    static MINOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bMinor\s*[=:]\s*(\d+)").unwrap());
    static PATCH_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)\b(?:Patch|PatchLevel)\s*[=:]\s*(\d+)").unwrap());

    let major: u32 = MAJOR_RE.captures(content)?.get(1)?.as_str().parse().ok()?;
    let minor: u32 = MINOR_RE.captures(content)?.get(1)?.as_str().parse().ok()?;
    let patch: u32 = PATCH_RE
        .captures(content)
        .and_then(|c| c.get(1)?.as_str().parse().ok())
        .unwrap_or(0);

    let version = format!("{major}.{minor}.{patch}");
    debug!("Version from Major/Minor/Patch constants: {version}");
    Some(version)
}

/// Check if a path segment is a Go major-version suffix (v2, v3, ...).
fn is_major_version_suffix(segment: &str) -> bool {
    segment.starts_with('v')
        && segment.len() >= 2
        && segment[1..].chars().all(|c| c.is_ascii_digit())
        && segment[1..].parse::<u32>().is_ok_and(|n| n >= 2)
}

/// Classify a license file by its content (first few hundred chars).
fn classify_license(content: &str) -> Option<String> {
    // Only lowercase the prefix we actually inspect (avoid full-file allocation)
    let byte_end = content.len().min(600);
    let mut end = byte_end;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = content[..end].to_lowercase();

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
    fn get_package_name_skips_major_version_suffix() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module github.com/org/repo/v2\n\ngo 1.21\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(
            handler.get_package_name().unwrap(),
            "repo",
            "Should skip v2 suffix and use parent segment"
        );
    }

    #[test]
    fn get_package_name_v5_suffix() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module github.com/go-chi/chi/v5\n\ngo 1.21\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_package_name().unwrap(), "chi");
    }

    #[test]
    fn get_package_name_v1_not_skipped() {
        // v1 is not a major version suffix in Go conventions (v0, v1 = no suffix)
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module github.com/org/v1\n\ngo 1.21\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(
            handler.get_package_name().unwrap(),
            "v1",
            "v1 is not a major version suffix, should be kept"
        );
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

    // ── Dev placeholder filtering ────────────────────────────────────

    #[test]
    fn extract_version_constant_rejects_dev_placeholder() {
        assert_eq!(
            extract_version_constant("var Version = \"dev\"\n"),
            None,
            "should reject 'dev' placeholder"
        );
    }

    #[test]
    fn extract_version_constant_rejects_untracked() {
        assert_eq!(
            extract_version_constant("const Version = \"(untracked)\"\n"),
            None
        );
    }

    #[test]
    fn extract_version_constant_rejects_library_import() {
        assert_eq!(
            extract_version_constant("var Version = \"library-import\"\n"),
            None,
        );
    }

    #[test]
    fn extract_version_constant_rejects_v0_0_0_unset() {
        assert_eq!(
            extract_version_constant("const Version = \"v0.0.0-unset\"\n"),
            None,
        );
    }

    #[test]
    fn is_dev_placeholder_rejects_known_values() {
        assert!(is_dev_placeholder("dev"));
        assert!(is_dev_placeholder("development"));
        assert!(is_dev_placeholder("unknown-dev"));
        assert!(is_dev_placeholder("(untracked)"));
        assert!(is_dev_placeholder("library-import"));
        assert!(is_dev_placeholder("v0.0.0-unset"));
        assert!(is_dev_placeholder("0.0.0"));
    }

    #[test]
    fn is_dev_placeholder_accepts_real_versions() {
        assert!(!is_dev_placeholder("1.2.3"));
        assert!(!is_dev_placeholder("0.9.0-beta"));
        assert!(!is_dev_placeholder("3.7.0-alpha.0"));
        assert!(!is_dev_placeholder("v1.12.0"));
    }

    // ── Major/Minor/Patch integer constants ──────────────────────────

    #[test]
    fn extract_version_parts_basic() {
        let content = "const (\n\tMajor = 1\n\tMinor = 17\n\tPatch = 2\n)\n";
        assert_eq!(extract_version_parts(content), Some("1.17.2".into()));
    }

    #[test]
    fn extract_version_parts_missing_patch() {
        let content = "const Major = 3\nconst Minor = 5\n";
        assert_eq!(extract_version_parts(content), Some("3.5.0".into()));
    }

    #[test]
    fn extract_version_parts_hugo_style_struct() {
        // Hugo uses struct init with colons and PatchLevel
        let content = "var CurrentVersion = Version{\n\tMajor:      0,\n\tMinor:      158,\n\tPatchLevel: 0,\n}\n";
        assert_eq!(extract_version_parts(content), Some("0.158.0".into()));
    }

    #[test]
    fn extract_version_parts_geth_style() {
        let content =
            "const (\n\tMajor = 1\n\tMinor = 17\n\tPatch = 2\n\tMeta  = \"unstable\"\n)\n";
        assert_eq!(extract_version_parts(content), Some("1.17.2".into()));
    }

    #[test]
    fn extract_version_parts_no_match() {
        assert_eq!(extract_version_parts("package main\n"), None);
    }

    #[test]
    fn extract_version_constant_falls_back_to_parts() {
        // No string Version constant, but has Major/Minor/Patch
        let content = "package version\n\nconst (\n\tMajor = 2\n\tMinor = 8\n\tPatch = 1\n)\n";
        assert_eq!(extract_version_constant(content), Some("2.8.1".into()));
    }

    // ── VERSION file handling ────────────────────────────────────────

    #[test]
    fn get_version_from_version_file_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(dir.path().join("VERSION"), "3.10.0\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "3.10.0");
    }

    #[test]
    fn get_version_from_version_txt_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(dir.path().join("VERSION.txt"), "1.95.0\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "1.95.0");
    }

    #[test]
    fn get_version_from_version_file_with_v_prefix() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(dir.path().join("VERSION"), "v2.16.0\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "2.16.0");
    }

    // ── Version subdirectory search ──────────────────────────────────

    #[test]
    fn get_version_from_version_subdir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir(dir.path().join("version")).unwrap();
        fs::write(
            dir.path().join("version").join("version.go"),
            "package version\n\nvar Version = \"3.7.0-alpha.0\"\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "3.7.0-alpha.0");
    }

    #[test]
    fn get_version_from_internal_version_subdir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir_all(dir.path().join("internal").join("version")).unwrap();
        fs::write(
            dir.path()
                .join("internal")
                .join("version")
                .join("version.go"),
            "package version\n\nconst Version = \"0.26.2\"\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "0.26.2");
    }

    #[test]
    fn get_version_subdir_dev_placeholder_skipped() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir(dir.path().join("version")).unwrap();
        fs::write(
            dir.path().join("version").join("version.go"),
            "package version\n\nvar Version = \"dev\"\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        // "dev" is a placeholder — should fall through to "latest" (no git tags in temp dir)
        assert_eq!(handler.get_version().unwrap(), "latest");
    }

    // ── License classifier tests ────────────────────────────────────

    #[test]
    fn classify_license_bsd_3_clause() {
        assert_eq!(
            classify_license("BSD 3-Clause License\n\nCopyright..."),
            Some("BSD-3-Clause".into())
        );
    }

    #[test]
    fn classify_license_bsd_3_clause_via_redistribution() {
        assert_eq!(
            classify_license("Redistribution and use in source and binary forms..."),
            Some("BSD-3-Clause".into())
        );
    }

    #[test]
    fn classify_license_bsd_2_clause() {
        assert_eq!(
            classify_license("BSD 2-Clause License\n\nCopyright (c) 2024"),
            Some("BSD-2-Clause".into())
        );
    }

    #[test]
    fn classify_license_mpl() {
        assert_eq!(
            classify_license("Mozilla Public License Version 2.0\n\n1. Definitions"),
            Some("MPL-2.0".into())
        );
    }

    #[test]
    fn classify_license_gpl3() {
        assert_eq!(
            classify_license("GNU General Public License\nVersion 3, 29 June 2007"),
            Some("GPL-3.0".into())
        );
    }

    #[test]
    fn classify_license_gpl2() {
        assert_eq!(
            classify_license("GNU General Public License\nVersion 2, June 1991"),
            Some("GPL-2.0".into())
        );
    }

    #[test]
    fn classify_license_gpl_no_version_defaults_to_gpl2() {
        // GNU GPL without a specific version 3 marker → defaults to GPL-2.0
        assert_eq!(
            classify_license("GNU General Public License as published by the FSF"),
            Some("GPL-2.0".into())
        );
    }

    #[test]
    fn classify_license_unlicense() {
        assert_eq!(
            classify_license("This is free and unencumbered software released into the public domain.\n\nThe Unlicense"),
            Some("Unlicense".into())
        );
    }

    #[test]
    fn classify_license_unlicense_bare() {
        assert_eq!(
            classify_license("Unlicense\n\nAnyone is free to copy..."),
            Some("Unlicense".into())
        );
    }

    #[test]
    fn classify_license_isc() {
        assert_eq!(
            classify_license("ISC License\n\nCopyright (c) 2024 Test"),
            Some("ISC".into())
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

    #[test]
    fn is_major_version_suffix_recognizes_v2_plus() {
        assert!(is_major_version_suffix("v2"));
        assert!(is_major_version_suffix("v3"));
        assert!(is_major_version_suffix("v5"));
        assert!(is_major_version_suffix("v10"));
    }

    #[test]
    fn is_major_version_suffix_rejects_v0_v1() {
        assert!(!is_major_version_suffix("v0"));
        assert!(!is_major_version_suffix("v1"));
    }

    #[test]
    fn is_major_version_suffix_rejects_non_versions() {
        assert!(!is_major_version_suffix("vendor"));
        assert!(!is_major_version_suffix("v"));
        assert!(!is_major_version_suffix("2"));
        assert!(!is_major_version_suffix("v2a"));
        assert!(!is_major_version_suffix("version"));
    }

    // ── parse_version_tag tests ──────────────────────────────────────

    #[test]
    fn parse_version_tag_strips_v_prefix() {
        assert_eq!(parse_version_tag("v1.18.0"), Some("1.18.0".into()));
    }

    #[test]
    fn parse_version_tag_no_prefix() {
        assert_eq!(parse_version_tag("1.2.3"), Some("1.2.3".into()));
    }

    #[test]
    fn parse_version_tag_two_part() {
        assert_eq!(parse_version_tag("v1.2"), Some("1.2".into()));
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
        // "0.9.0-beta" contains '.' and starts with digit — valid
        assert_eq!(parse_version_tag("v0.9.0-beta"), Some("0.9.0-beta".into()));
    }

    // ── License fallback (unclassified) ──────────────────────────────

    #[test]
    fn get_license_unclassified_returns_first_line() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("LICENSE"),
            "Custom Proprietary License v1.0\n\nDo whatever you want.\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(
            handler.get_license().unwrap(),
            "Custom Proprietary License v1.0"
        );
    }

    #[test]
    fn get_license_unclassified_skips_blank_lines() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("LICENSE"), "\n\n  \nActual License Text\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_license().unwrap(), "Actual License Text");
    }

    #[test]
    fn get_license_from_licence_spelling() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("LICENCE"),
            "MIT License\n\nPermission is hereby granted...\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_license().unwrap(), "MIT");
    }

    #[test]
    fn get_license_from_copying_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("COPYING"),
            "GNU General Public License\nVersion 3, 29 June 2007",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_license().unwrap(), "GPL-3.0");
    }

    // ── URL generation edge cases ────────────────────────────────────

    #[test]
    fn get_project_urls_gitlab_module() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module gitlab.com/myorg/mylib\n\ngo 1.21\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Source" && v == "https://gitlab.com/myorg/mylib"));
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Documentation" && v.contains("pkg.go.dev/gitlab.com/myorg/mylib")));
    }

    #[test]
    fn get_project_urls_bitbucket_module() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module bitbucket.org/team/repo\n\ngo 1.21\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Source" && v == "https://bitbucket.org/team/repo"));
    }

    #[test]
    fn get_project_urls_custom_domain_no_source_url() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("go.mod"),
            "module go.uber.org/zap\n\ngo 1.21\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        let urls = handler.get_project_urls();
        // Should have Documentation but NOT Source (custom domain)
        assert!(urls.iter().any(|(k, _)| k == "Documentation"));
        assert!(!urls.iter().any(|(k, _)| k == "Source"));
    }

    #[test]
    fn get_project_urls_no_go_mod_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(handler.get_project_urls().is_empty());
    }

    // ── Docs recursive depth limit ───────────────────────────────────

    #[test]
    fn find_docs_deeply_nested_stops_at_depth_limit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();

        // Create docs/a/b/c/.../deep.md at depth > MAX_DEPTH (20)
        let mut path = root.join("docs");
        for i in 0..22 {
            path = path.join(format!("level{i}"));
        }
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("deep.md"), "# Deep doc\n").unwrap();

        let handler = GoHandler::new(root);
        let docs = handler.find_docs().unwrap();
        // The deeply nested file should NOT be found (depth > MAX_DEPTH)
        assert!(!docs
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n.to_str() == Some("deep.md"))));
    }

    #[test]
    fn find_docs_skips_hidden_and_vendor_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();

        // Hidden dir inside docs/
        let hidden = root.join("docs").join(".hidden");
        fs::create_dir_all(&hidden).unwrap();
        fs::write(hidden.join("secret.md"), "# Secret\n").unwrap();

        // vendor dir inside docs/
        let vendor = root.join("docs").join("vendor");
        fs::create_dir_all(&vendor).unwrap();
        fs::write(vendor.join("vendored.md"), "# Vendored\n").unwrap();

        // _build dir inside docs/
        let build = root.join("docs").join("_build");
        fs::create_dir_all(&build).unwrap();
        fs::write(build.join("built.md"), "# Built\n").unwrap();

        // Normal doc should be found
        fs::write(root.join("docs").join("guide.md"), "# Guide\n").unwrap();

        let handler = GoHandler::new(root);
        let docs = handler.find_docs().unwrap();
        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"guide.md"), "should find normal docs");
        assert!(!names.contains(&"secret.md"), "should skip .hidden/");
        assert!(!names.contains(&"vendored.md"), "should skip vendor/");
        assert!(!names.contains(&"built.md"), "should skip _build/");
    }

    // ── Version from git tags (using real git repos) ─────────────────

    #[test]
    fn get_version_from_git_describe() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();

        // Initialize a real git repo with a tag
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
        git(&["tag", "v1.5.0"]);

        let handler = GoHandler::new(root);
        assert_eq!(handler.get_version().unwrap(), "1.5.0");
    }

    #[test]
    fn get_version_from_git_tag_list() {
        // Test the latest_version_tag fallback (tag not reachable from HEAD)
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();

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
        // Make another commit so HEAD is ahead of the tag
        fs::write(root.join("extra.go"), "package main\n").unwrap();
        git(&["add", "extra.go"]);
        git(&["commit", "-m", "extra", "--no-gpg-sign"]);

        let handler = GoHandler::new(root);
        let version = handler.get_version().unwrap();
        // git describe may return "2.0.0" directly since the tag is still reachable,
        // but either way the version should be parseable
        assert!(
            version == "2.0.0" || version.starts_with("2.0.0"),
            "expected version from tag, got: {version}"
        );
    }

    #[test]
    fn get_version_prefers_git_tags_over_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            root.join("version.go"),
            "package mylib\n\nconst Version = \"1.0.0\"\n",
        )
        .unwrap();

        // Git tag should win over source constant
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
        git(&["tag", "v2.5.0"]);

        let handler = GoHandler::new(root);
        assert_eq!(
            handler.get_version().unwrap(),
            "2.5.0",
            "git tag should take priority over source constant"
        );
    }

    // ── Version source priority tests ────────────────────────────────

    #[test]
    fn get_version_version_go_over_version_file() {
        // version.go should be checked before VERSION file
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            dir.path().join("version.go"),
            "package mylib\n\nconst Version = \"3.0.0\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("VERSION"), "4.0.0\n").unwrap();

        let handler = GoHandler::new(dir.path());
        // No git repo → falls through to source → version.go first
        assert_eq!(handler.get_version().unwrap(), "3.0.0");
    }

    #[test]
    fn get_version_version_file_over_subdirs() {
        // VERSION file should be checked before version subdirectories
        // Note: macOS is case-insensitive, so VERSION file and version/ dir conflict.
        // Use VERSION.txt instead to test priority over subdirs.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(dir.path().join("VERSION.txt"), "5.0.0\n").unwrap();
        fs::create_dir_all(dir.path().join("version")).unwrap();
        fs::write(
            dir.path().join("version").join("version.go"),
            "package version\n\nconst Version = \"6.0.0\"\n",
        )
        .unwrap();

        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "5.0.0");
    }

    #[test]
    fn get_version_from_pkg_version_subdir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir_all(dir.path().join("pkg").join("version")).unwrap();
        fs::write(
            dir.path().join("pkg").join("version").join("version.go"),
            "package version\n\nvar Version = \"1.8.3\"\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "1.8.3");
    }

    #[test]
    fn get_version_from_rawversion_subdir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir_all(dir.path().join("version").join("rawversion")).unwrap();
        fs::write(
            dir.path()
                .join("version")
                .join("rawversion")
                .join("version.go"),
            "package rawversion\n\nconst Version = \"4.2.1\"\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "4.2.1");
    }

    // ── Example collection with nested subdirectories ─────────────────

    #[test]
    fn find_examples_recurses_into_example_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();

        // examples/advanced/demo.go — should be found via collect_all_go_in_dir recursion
        fs::create_dir_all(root.join("examples").join("advanced")).unwrap();
        fs::write(
            root.join("examples").join("advanced").join("demo.go"),
            "package main\n",
        )
        .unwrap();
        // Non-Go file in examples — should be skipped
        fs::write(root.join("examples").join("README.md"), "# Examples\n").unwrap();

        let handler = GoHandler::new(root);
        let files = handler.find_examples().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(
            names.contains(&"demo.go"),
            "should find nested example .go file"
        );
        assert!(!names.contains(&"README.md"), "should skip non-.go files");
    }

    #[test]
    fn find_examples_finds_nested_example_test_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();

        // example_*_test.go in a subdirectory
        fs::create_dir(root.join("pkg")).unwrap();
        fs::write(
            root.join("pkg").join("example_usage_test.go"),
            "package pkg\n",
        )
        .unwrap();

        let handler = GoHandler::new(root);
        let files = handler.find_examples().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(
            names.contains(&"example_usage_test.go"),
            "should find example_*_test.go in subdirs"
        );
    }

    #[test]
    fn find_examples_skips_vendor_in_example_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();

        // examples/vendor/dep.go — should be skipped
        fs::create_dir_all(root.join("examples").join("vendor")).unwrap();
        fs::write(
            root.join("examples").join("vendor").join("dep.go"),
            "package dep\n",
        )
        .unwrap();
        // examples/real.go — should be found
        fs::write(root.join("examples").join("real.go"), "package main\n").unwrap();

        let handler = GoHandler::new(root);
        let files = handler.find_examples().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"real.go"));
        assert!(
            !names.contains(&"dep.go"),
            "should skip vendor/ in examples"
        );
    }

    // ── read_module_path error case ──────────────────────────────────

    #[test]
    fn read_module_path_no_module_directive_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "go 1.21\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(handler.read_module_path().is_err());
    }

    // ── Recursive file collection edge cases ─────────────────────────

    #[test]
    fn collect_go_files_skips_non_go_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();
        fs::write(root.join("notes.txt"), "not a go file\n").unwrap();
        fs::write(root.join("config.json"), "{}").unwrap();

        let handler = GoHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(files
            .iter()
            .all(|p| p.extension().is_some_and(|e| e == "go")));
    }

    #[test]
    fn collect_go_files_recurses_into_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();
        fs::create_dir_all(root.join("pkg").join("deep")).unwrap();
        fs::write(
            root.join("pkg").join("deep").join("util.go"),
            "package deep\n",
        )
        .unwrap();

        let handler = GoHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "util.go")));
    }
}
