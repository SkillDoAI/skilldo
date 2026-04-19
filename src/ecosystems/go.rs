//! Go ecosystem handler — discovers source/test/doc/example files, extracts
//! metadata (name, version, license, URLs) from go.mod, and detects the module
//! path. Used by the collector for Go projects.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

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

        files.sort_by_key(|p| self.file_priority(p));
        let files = crate::util::filter_within_boundary(files, &self.repo_path);

        if files.is_empty() {
            bail!("No Go source files found in {}", self.repo_path.display());
        }
        info!("Found {} Go source files", files.len());
        Ok(files)
    }

    /// Find all Go test files (*_test.go).
    pub fn find_test_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_go_files(&self.repo_path, &mut files, 0, true)?;

        let files = crate::util::filter_within_boundary(files, &self.repo_path);

        if files.is_empty() {
            warn!("No test files found in {}", self.repo_path.display());
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
        let files = crate::util::filter_within_boundary(files, &self.repo_path);
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

        // Walk docs/ recursively, respecting .gitignore
        let skip = &["vendor", "node_modules", "_build"];
        for docs_dirname in &["docs", "doc"] {
            let docs_dir = self.repo_path.join(docs_dirname);
            if docs_dir.is_dir() {
                docs.extend(super::walk_files(
                    &docs_dir,
                    &["md", "rst"],
                    skip,
                    Some(Self::MAX_DEPTH),
                ));
            }
        }

        let docs = crate::util::filter_within_boundary(docs, &self.repo_path);
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
        for name in super::LICENSE_FILENAMES {
            let path = self.repo_path.join(name);
            if let Ok(content) = fs::read_to_string(&path) {
                // Return first line (usually the license name) or classify
                if let Some(license) = super::classify_license(&content) {
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

            // If it's a GitHub/GitLab/Bitbucket module, add source URL.
            // Strip Go semantic-import /vN suffix (e.g. github.com/org/repo/v5 → github.com/org/repo)
            if module_path.starts_with("github.com/")
                || module_path.starts_with("gitlab.com/")
                || module_path.starts_with("bitbucket.org/")
            {
                let url_path = strip_go_major_suffix(&module_path);
                urls.push(("Source".into(), format!("https://{url_path}")));
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
        let repo = crate::git::Git2Repo::open(&self.repo_path).ok()?;

        // Strategy 1: git describe (fast, works for full clones with reachable tags)
        if let Some(v) = repo
            .describe_tags()
            .ok()
            .and_then(|tag| parse_version_tag(&tag))
        {
            return Some(v);
        }

        // Strategy 2: tag list sorted by semver (works when tags exist but
        // aren't reachable from HEAD, e.g. shallow clones after tag fetch)
        if let Some(v) = repo
            .list_tags_sorted()
            .ok()
            .and_then(|tags| tags.iter().find_map(|tag| parse_version_tag(tag)))
        {
            return Some(v);
        }

        // Strategy 3: fetch tags then retry (shallow clones start with no tags).
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

    /// Detect CGo usage indicating native C dependencies.
    /// Recursively scans .go files including sub-packages.
    pub fn detect_native_deps(&self) -> Vec<String> {
        let mut indicators = Vec::new();
        self.scan_cgo_recursive(&self.repo_path, &mut indicators, 0);
        indicators
    }

    fn scan_cgo_recursive(&self, dir: &Path, indicators: &mut Vec<String>, depth: usize) {
        if depth > Self::MAX_DEPTH {
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            let path = entry.path();
            if ft.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !Self::should_skip_dir(&name) {
                    self.scan_cgo_recursive(&path, indicators, depth + 1);
                }
            } else if ft.is_file() && path.extension().and_then(|e| e.to_str()) == Some("go") {
                // Read only first 4KB — CGo markers appear in file headers
                if let Ok(file) = fs::File::open(&path) {
                    use std::io::Read;
                    let mut buf = [0u8; 4096];
                    let n = (&file).read(&mut buf).unwrap_or(0);
                    let content = String::from_utf8_lossy(&buf[..n]);
                    let rel = path
                        .strip_prefix(&self.repo_path)
                        .unwrap_or(&path)
                        .display()
                        .to_string();
                    // Match direct `import "C"` and grouped `import ( ... "C" ... )` forms.
                    // Check line-by-line to avoid false positives like `var x = "C"`.
                    let has_cgo = content.lines().any(|line| {
                        let trimmed = line.trim();
                        trimmed == "import \"C\""
                            || trimmed == "\"C\""
                            || trimmed.starts_with("import \"C\"")
                    });
                    if has_cgo {
                        indicators.push(format!("CGo import in {rel}"));
                    }
                    if content.contains("#cgo LDFLAGS") || content.contains("#cgo CFLAGS") {
                        indicators.push(format!("CGo build flags in {rel}"));
                    }
                }
            }
        }
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Strip Go semantic-import `/vN` major version suffix from a module path.
/// e.g. `github.com/org/repo/v5` → `github.com/org/repo`
fn strip_go_major_suffix(module_path: &str) -> &str {
    if let Some(idx) = module_path.rfind('/') {
        let suffix = &module_path[idx + 1..];
        if suffix.starts_with('v')
            && suffix.len() > 1
            && suffix[1..].chars().all(|c| c.is_ascii_digit())
        {
            let version: u32 = suffix[1..].parse().unwrap_or(0);
            if version >= 2 {
                return &module_path[..idx];
            }
        }
    }
    module_path
}

/// Parse module path from go.mod content.
pub(crate) fn parse_module_path(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module") {
            let path = rest.trim();
            // Strip trailing inline comments (e.g. "// indirect")
            let path = path.split("//").next().unwrap_or(path).trim();
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
        Lazy::new(|| Regex::new(r#"(?i)(?:Version|VERSION)\s*=\s*"(v?\d+\.\d+[^"]*)"#).unwrap());

    // Try simple string constant first
    if let Some(cap) = VERSION_RE.captures(content) {
        let v = cap[1].strip_prefix('v').unwrap_or(&cap[1]).to_string();
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

#[cfg(test)]
mod tests {
    use super::super::classify_license;
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
    fn find_test_files_no_tests_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(dir.path().join("main.go"), "package main\n").unwrap();
        let handler = GoHandler::new(dir.path());
        let result = handler.find_test_files().unwrap();
        assert!(result.is_empty());
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
    fn parse_module_path_strips_trailing_comment() {
        assert_eq!(
            parse_module_path("module github.com/foo/bar // indirect\n\ngo 1.21\n"),
            Some("github.com/foo/bar".into())
        );
    }

    #[test]
    fn parse_module_path_no_comment() {
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
    fn extract_version_constant_v_prefix() {
        assert_eq!(
            extract_version_constant("const Version = \"v1.2.3\"\n"),
            Some("1.2.3".into()),
            "should strip leading v prefix"
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
    fn classify_license_bsd2_via_redistribution_without_endorsement() {
        // Redistribution phrase without non-endorsement clause → BSD-2-Clause
        assert_eq!(
            classify_license("Redistribution and use in source and binary forms..."),
            Some("BSD-2-Clause".into())
        );
    }

    #[test]
    fn classify_license_bsd3_via_non_endorsement_clause() {
        // Redistribution phrase WITH non-endorsement clause → BSD-3-Clause
        assert_eq!(
            classify_license("Redistribution and use in source and binary forms, with or without modification, are permitted provided that neither the name of the copyright holder..."),
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
    fn get_version_from_orphan_tag_list() {
        // Strategy 2: describe fails (orphan tag) but list_tags_sorted finds it
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

        // Tag this commit
        git(&["tag", "v3.1.0"]);

        // Create a new orphan branch so HEAD has no reachable tags
        git(&["checkout", "--orphan", "orphan"]);
        fs::write(root.join("orphan.go"), "package main\n").unwrap();
        git(&["add", "orphan.go"]);
        git(&["commit", "-m", "orphan", "--no-gpg-sign"]);

        let handler = GoHandler::new(root);
        let version = handler.get_version().unwrap();
        // Strategy 1 (describe) should fail on orphan branch,
        // but Strategy 2 (list_tags_sorted) should find v3.1.0
        assert_eq!(version, "3.1.0");
    }

    #[test]
    fn get_version_no_tags_no_source() {
        // No tags, no source constants → version_from_git_tags returns None,
        // exercises the None path through all 3 strategies
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

        let handler = GoHandler::new(root);
        let version = handler.get_version().unwrap();
        // No tags, no version.go → falls through to "latest"
        assert_eq!(version, "latest");
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

    #[test]
    fn strip_major_suffix_v2() {
        assert_eq!(
            strip_go_major_suffix("github.com/org/repo/v5"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn strip_major_suffix_v1_kept() {
        // v1 is not a semantic-import suffix
        assert_eq!(
            strip_go_major_suffix("github.com/org/repo/v1"),
            "github.com/org/repo/v1"
        );
    }

    #[test]
    fn strip_major_suffix_no_suffix() {
        assert_eq!(
            strip_go_major_suffix("github.com/org/repo"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn strip_major_suffix_non_version_path() {
        assert_eq!(
            strip_go_major_suffix("github.com/org/repo/vendor"),
            "github.com/org/repo/vendor"
        );
    }

    #[test]
    fn parse_module_path_strips_inline_comment() {
        let content = "module github.com/acme/lib // note\n\ngo 1.21\n";
        assert_eq!(
            parse_module_path(content),
            Some("github.com/acme/lib".to_string())
        );
    }

    // ── strip_go_major_suffix edge cases ─────────────────────────────

    #[test]
    fn strip_major_suffix_no_slash() {
        // Module path with no slash at all (single segment)
        assert_eq!(strip_go_major_suffix("mymodule"), "mymodule");
    }

    #[test]
    fn strip_major_suffix_v0_kept() {
        // v0 is not a semantic-import suffix (< 2)
        assert_eq!(
            strip_go_major_suffix("github.com/org/repo/v0"),
            "github.com/org/repo/v0"
        );
    }

    // ── parse_module_path edge cases ─────────────────────────────────

    #[test]
    fn parse_module_path_empty_after_module() {
        // "module" keyword with nothing after it (just whitespace)
        assert_eq!(parse_module_path("module \n\ngo 1.21\n"), None);
    }

    // ── extract_version_constant edge cases ──────────────────────────

    #[test]
    fn extract_version_constant_rejects_zero_zero_zero() {
        assert_eq!(
            extract_version_constant("const Version = \"0.0.0\"\n"),
            None,
            "0.0.0 should be rejected as a dev placeholder"
        );
    }

    // ── is_dev_placeholder edge cases ────────────────────────────────

    #[test]
    fn is_dev_placeholder_case_insensitive() {
        assert!(is_dev_placeholder("DEV"));
        assert!(is_dev_placeholder("Development"));
        assert!(is_dev_placeholder("UNKNOWN"));
        assert!(is_dev_placeholder("V0.0.0"));
    }

    // ── find_test_files with nested packages ─────────────────────────

    #[test]
    fn find_test_files_finds_nested_package_tests() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir_all(root.join("internal").join("util")).unwrap();
        fs::write(
            root.join("internal").join("util").join("helper_test.go"),
            "package util\n",
        )
        .unwrap();

        let handler = GoHandler::new(root);
        let files = handler.find_test_files().unwrap();
        assert!(files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "helper_test.go")));
    }

    // ── find_docs with rst files ─────────────────────────────────────

    #[test]
    fn find_docs_includes_rst_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("guide.rst"), "Guide\n=====\n").unwrap();

        let handler = GoHandler::new(root);
        let docs = handler.find_docs().unwrap();
        assert!(docs
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "guide.rst")));
    }

    // ── get_license unclassified with blank leading lines ────────────

    #[test]
    fn get_license_empty_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("LICENSE"), "").unwrap();
        let handler = GoHandler::new(dir.path());
        // Empty file has no non-empty lines → None
        assert!(handler.get_license().is_none());
    }

    // ── parse_version_tag additional edge cases ──────────────────────

    #[test]
    fn parse_version_tag_no_dot_returns_none() {
        // "42" starts with digit but has no dot
        assert_eq!(parse_version_tag("42"), None);
    }

    #[test]
    fn parse_version_tag_starts_with_non_digit() {
        // "release.1.0" starts with 'r' (not a digit) after stripping v
        assert_eq!(parse_version_tag("release.1.0"), None);
    }

    #[test]
    fn collect_go_files_stops_at_max_depth() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();
        let mut deep = root.to_path_buf();
        for i in 0..22 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep.go"), "package deep\n").unwrap();
        let handler = GoHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(!files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "deep.go")));
    }

    #[test]
    fn collect_all_go_in_dir_stops_at_max_depth() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();
        let mut deep = root.join("examples");
        for i in 0..22 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep_example.go"), "package main\n").unwrap();
        let handler = GoHandler::new(root);
        let files = handler.find_examples().unwrap();
        assert!(!files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "deep_example.go")));
    }

    #[test]
    fn collect_example_test_files_stops_at_max_depth() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();
        let mut deep = root.to_path_buf();
        for i in 0..22 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("example_deep_test.go"), "package deep\n").unwrap();
        let handler = GoHandler::new(root);
        let files = handler.find_examples().unwrap();
        assert!(!files
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "example_deep_test.go")));
    }

    #[test]
    fn collect_docs_recursive_stops_at_max_depth_go() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        let mut deep = root.join("docs");
        for i in 0..22 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("unreachable.md"), "# Hidden\n").unwrap();
        let handler = GoHandler::new(root);
        let docs = handler.find_docs().unwrap();
        assert!(!docs
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "unreachable.md")));
    }

    #[test]
    fn get_version_from_internal_version_subdir_no_git() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir_all(dir.path().join("internal").join("version")).unwrap();
        fs::write(
            dir.path()
                .join("internal")
                .join("version")
                .join("version.go"),
            "package version\n\nvar Version = \"2.4.6\"\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "2.4.6");
    }

    #[test]
    fn extract_version_parts_missing_minor_returns_none() {
        assert_eq!(extract_version_parts("Major = 3\nPatch = 1\n"), None);
    }

    #[test]
    fn is_dev_placeholder_zero_zero_zero_unset() {
        assert!(is_dev_placeholder("0.0.0-unset"));
    }

    #[test]
    fn is_dev_placeholder_library_import() {
        assert!(is_dev_placeholder("library-import"));
    }

    #[test]
    fn is_dev_placeholder_untracked() {
        assert!(is_dev_placeholder("(untracked)"));
    }

    #[test]
    fn is_dev_placeholder_real_version_returns_false() {
        assert!(!is_dev_placeholder("1.2.3"));
        assert!(!is_dev_placeholder("v2.0.0"));
    }

    #[test]
    fn is_major_version_suffix_v2() {
        assert!(is_major_version_suffix("v2"));
        assert!(is_major_version_suffix("v100"));
    }

    #[test]
    fn is_major_version_suffix_v1_false() {
        assert!(!is_major_version_suffix("v1"));
        assert!(!is_major_version_suffix("v0"));
    }

    #[test]
    fn is_major_version_suffix_non_version_false() {
        assert!(!is_major_version_suffix("vendor"));
        assert!(!is_major_version_suffix("v"));
        assert!(!is_major_version_suffix("v2a"));
    }

    #[test]
    fn extract_version_constant_rejects_dev() {
        assert_eq!(extract_version_constant("const Version = \"dev\"\n"), None);
    }

    #[test]
    fn extract_version_constant_rejects_unversioned() {
        assert_eq!(
            extract_version_constant("const Version = \"unversioned\"\n"),
            None
        );
    }

    #[test]
    fn extract_version_constant_accepts_valid() {
        assert_eq!(
            extract_version_constant("const Version = \"v1.5.3\"\n"),
            Some("1.5.3".to_string()),
        );
    }

    #[test]
    fn extract_version_constant_parts_fallback_with_struct_syntax() {
        let content = "package version\n\nMajor: 2,\nMinor: 0,\nPatch: 1,\n";
        assert_eq!(extract_version_constant(content), Some("2.0.1".to_string()));
    }

    // ── Recursive traversal into subdirectories ─────────────────────

    #[test]
    fn collect_docs_recursive_enters_subdirectories() {
        // Exercises the `else if ft.is_dir()` branch in collect_docs_recursive
        // (line 362-364) with a nested subdir containing a doc file.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir_all(root.join("docs").join("guides")).unwrap();
        fs::write(
            root.join("docs").join("guides").join("quickstart.md"),
            "# Quick Start\n",
        )
        .unwrap();

        let handler = GoHandler::new(root);
        let docs = handler.find_docs().unwrap();
        assert!(
            docs.iter()
                .any(|p| p.file_name().is_some_and(|n| n == "quickstart.md")),
            "should find docs in nested subdirectory: {:?}",
            docs.iter().map(|p| p.file_name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn collect_docs_recursive_skips_non_doc_extensions() {
        // Exercises the branch where a file exists in docs/ but has an extension
        // that is neither .md nor .rst — it should be skipped.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("notes.txt"), "some notes\n").unwrap();
        fs::write(root.join("docs").join("data.json"), "{}").unwrap();
        fs::write(root.join("docs").join("real.md"), "# Real Doc\n").unwrap();

        let handler = GoHandler::new(root);
        let docs = handler.find_docs().unwrap();
        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"real.md"), "should find .md files");
        assert!(!names.contains(&"notes.txt"), "should skip .txt files");
        assert!(!names.contains(&"data.json"), "should skip .json files");
    }

    #[test]
    fn collect_all_go_in_dir_recurses_into_nested_example_dirs() {
        // Exercises the `else if ft.is_dir()` branch in collect_all_go_in_dir
        // (line 290-296) with a two-level deep example structure.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();
        fs::create_dir_all(root.join("examples").join("grpc").join("server")).unwrap();
        fs::write(
            root.join("examples")
                .join("grpc")
                .join("server")
                .join("main.go"),
            "package main\n",
        )
        .unwrap();

        let handler = GoHandler::new(root);
        let files = handler.find_examples().unwrap();
        assert!(
            files.iter().any(|p| p.ends_with("grpc/server/main.go")),
            "should find deeply nested example file: {:?}",
            files
        );
    }

    #[test]
    fn collect_example_test_files_recurses_into_nested_dirs() {
        // Exercises the recursive branch of collect_example_test_files
        // (lines 320-327) with a nested dir containing example_*_test.go.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();
        fs::create_dir_all(root.join("internal").join("api")).unwrap();
        fs::write(
            root.join("internal")
                .join("api")
                .join("example_handler_test.go"),
            "package api\n",
        )
        .unwrap();

        let handler = GoHandler::new(root);
        let files = handler.find_examples().unwrap();
        assert!(
            files.iter().any(|p| p
                .file_name()
                .is_some_and(|n| n == "example_handler_test.go")),
            "should find example_*_test.go in nested subdirectory: {:?}",
            files
        );
    }

    #[test]
    fn collect_go_files_recurses_into_multi_level_dirs() {
        // Exercises the recursive branch of collect_go_files (lines 256-264)
        // with a multi-level directory structure.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();
        fs::create_dir_all(root.join("cmd").join("server")).unwrap();
        fs::write(
            root.join("cmd").join("server").join("main.go"),
            "package main\n",
        )
        .unwrap();

        let handler = GoHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(
            files.iter().any(|p| p.ends_with("cmd/server/main.go")),
            "should find source files in nested dirs: {:?}",
            files
        );
    }

    #[test]
    fn version_from_source_version_txt_fallback() {
        // Exercises version_from_source lines 437-445:
        // VERSION.txt file should be detected when version.go is absent.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(dir.path().join("VERSION.txt"), "7.2.1\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(
            handler.get_version().unwrap(),
            "7.2.1",
            "should detect version from VERSION.txt"
        );
    }

    #[test]
    fn version_from_source_version_file_dev_placeholder_skipped() {
        // VERSION file containing a dev placeholder should be skipped,
        // falling through to version subdirs or "latest".
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(dir.path().join("VERSION"), "dev\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(
            handler.get_version().unwrap(),
            "latest",
            "dev placeholder in VERSION file should be skipped"
        );
    }

    #[test]
    fn version_from_source_version_go_dev_falls_to_version_file() {
        // version.go has a dev placeholder, but VERSION file has a real version.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            dir.path().join("version.go"),
            "package mylib\n\nvar Version = \"dev\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("VERSION"), "4.5.6\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert_eq!(
            handler.get_version().unwrap(),
            "4.5.6",
            "should fall through from dev version.go to VERSION file"
        );
    }

    // ── detect_native_deps (CGo detection) ──────────────────────────

    #[test]
    fn detect_native_deps_cgo_import() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            dir.path().join("cgo_wrapper.go"),
            "package main\n\n/*\n#include <stdlib.h>\n*/\nimport \"C\"\n\nfunc main() {}\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators
                .iter()
                .any(|i| i.contains("CGo import") && i.contains("cgo_wrapper.go")),
            "should detect CGo import, got: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_cgo_grouped_import() {
        // Go allows grouped imports: import ( "C" )
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            dir.path().join("bridge.go"),
            "package main\n\nimport (\n\t\"C\"\n\t\"fmt\"\n)\n\nfunc main() { fmt.Println(C.GoString(nil)) }\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("CGo import")),
            "should detect grouped import form: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_cgo_build_flags() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            dir.path().join("native.go"),
            "package main\n\n// #cgo LDFLAGS: -lm\n// #cgo CFLAGS: -I/usr/include\nimport \"C\"\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("CGo build flags")),
            "should detect CGo build flags, got: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_cgo_in_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        // CGo in a sub-package, not root
        let sub = dir.path().join("internal").join("native");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            sub.join("bridge.go"),
            "package native\n\n// #cgo LDFLAGS: -lz\nimport \"C\"\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("CGo import")),
            "should find CGo in subdirectory: {:?}",
            indicators
        );
        assert!(
            indicators.iter().any(|i| i.contains("CGo build flags")),
            "should find #cgo flags in subdirectory: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_cgo_no_false_positive_string_c() {
        // A Go file with `var lang = "C"` and `import "fmt"` should NOT trigger
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            dir.path().join("main.go"),
            "package main\n\nimport \"fmt\"\n\nvar lang = \"C\"\n\nfunc main() { fmt.Println(lang) }\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(
            handler.detect_native_deps().is_empty(),
            "string literal 'C' should not trigger CGo detection"
        );
    }

    #[test]
    fn detect_native_deps_skips_vendor_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        // CGo file inside vendor/ — should be skipped
        let vendor = dir.path().join("vendor").join("lib");
        fs::create_dir_all(&vendor).unwrap();
        fs::write(vendor.join("cgo.go"), "package lib\nimport \"C\"\n").unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(
            handler.detect_native_deps().is_empty(),
            "vendor/ CGo should be skipped"
        );
    }

    #[test]
    fn detect_native_deps_clean_go_project() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            dir.path().join("main.go"),
            "package main\n\nfunc main() {}\n",
        )
        .unwrap();
        let handler = GoHandler::new(dir.path());
        assert!(
            handler.detect_native_deps().is_empty(),
            "pure Go project should have no native dep indicators"
        );
    }

    // ── collect_example_test_files ──────────────────────────────────

    #[test]
    fn collect_example_test_files_finds_example_prefixed_tests() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("go.mod"), "module test\n\ngo 1.21\n").unwrap();
        fs::write(
            root.join("example_hello_test.go"),
            "package main\nfunc ExampleHello() {}\n",
        )
        .unwrap();
        // Regular test should not be collected
        fs::write(
            root.join("hello_test.go"),
            "package main\nfunc TestHello(t *testing.T) {}\n",
        )
        .unwrap();
        let handler = GoHandler::new(root);
        let mut files = Vec::new();
        handler
            .collect_example_test_files(root, &mut files, 0)
            .unwrap();
        let names: Vec<_> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"example_hello_test.go"));
        assert!(!names.contains(&"hello_test.go"));
    }

    // ── collect_all_go_in_dir ───────────────────────────────────────

    #[test]
    fn collect_all_go_in_dir_includes_both_source_and_test() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("lib.go"), "package lib\n").unwrap();
        fs::write(root.join("lib_test.go"), "package lib\n").unwrap();
        // vendor/ should be skipped
        let vendor = root.join("vendor");
        fs::create_dir_all(&vendor).unwrap();
        fs::write(vendor.join("dep.go"), "package dep\n").unwrap();

        let handler = GoHandler::new(root);
        let mut files = Vec::new();
        handler.collect_all_go_in_dir(root, &mut files, 0).unwrap();
        let names: Vec<_> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"lib.go"));
        assert!(names.contains(&"lib_test.go"));
        assert!(!names.contains(&"dep.go"));
    }
}
