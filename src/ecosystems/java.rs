//! Java ecosystem handler — discovers source/test/doc/example files, extracts
//! metadata (name, version, license, URLs) from pom.xml or build.gradle, and
//! detects the package coordinates. Used by the collector for Java projects.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

pub struct JavaHandler {
    repo_path: PathBuf,
}

impl JavaHandler {
    const MAX_DEPTH: usize = 20;

    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    // ── File discovery ──────────────────────────────────────────────────

    /// Find all Java source files (excluding test dirs, build output).
    pub fn find_source_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_java_files(&self.repo_path, &mut files, 0, false)?;

        files.sort();
        let files = crate::util::filter_within_boundary(files, &self.repo_path);

        if files.is_empty() {
            bail!("No Java source files found in {}", self.repo_path.display());
        }
        debug!("Found {} Java source files", files.len());
        Ok(files)
    }

    /// Find all Java test files.
    pub fn find_test_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_java_files(&self.repo_path, &mut files, 0, true)?;

        let mut files = crate::util::filter_within_boundary(files, &self.repo_path);

        if files.is_empty() {
            warn!("No test files found in {}", self.repo_path.display());
        }

        files.sort();
        debug!("Found {} Java test files", files.len());
        Ok(files)
    }

    /// Find example files (examples/, example/ dirs).
    pub fn find_examples(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for dir_name in &["examples", "example", "samples", "sample"] {
            let dir = self.repo_path.join(dir_name);
            if dir.is_dir() {
                self.collect_all_java_in_dir(&dir, &mut files, 0)?;
            }
        }

        files.sort();
        files.dedup();
        let files = crate::util::filter_within_boundary(files, &self.repo_path);
        debug!("Found {} Java example files", files.len());
        Ok(files)
    }

    /// Find documentation files (README, docs/).
    pub fn find_docs(&self) -> Result<Vec<PathBuf>> {
        let mut docs = Vec::new();

        for name in &["README.md", "README.rst", "README.txt", "README"] {
            let path = self.repo_path.join(name);
            if path.is_file() {
                docs.push(path);
                break;
            }
        }

        let skip = &["target", "build", "out", "bin", "node_modules", "buildSrc"];
        for docs_dirname in &["docs", "doc"] {
            let docs_dir = self.repo_path.join(docs_dirname);
            if docs_dir.is_dir() {
                docs.extend(super::walk_files(
                    &docs_dir,
                    &["md", "rst", "txt", "adoc"],
                    skip,
                    Some(Self::MAX_DEPTH),
                ));
            }
        }

        docs.sort();
        docs.dedup();
        let docs = crate::util::filter_within_boundary(docs, &self.repo_path);
        debug!("Found {} documentation files", docs.len());
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

    /// Extract package name from pom.xml or build.gradle.
    pub fn get_package_name(&self) -> Result<String> {
        // Try pom.xml first
        let pom = self.repo_path.join("pom.xml");
        if pom.is_file() {
            if let Ok(content) = fs::read_to_string(&pom) {
                if let Some(name) = parse_pom_artifact_id(&content) {
                    // Strip -parent suffix for parent POMs (the real artifact is a submodule)
                    let cleaned = name.strip_suffix("-parent").unwrap_or(&name);
                    if !cleaned.is_empty() {
                        return Ok(cleaned.to_string());
                    }
                }
            }
        }

        // Try settings.gradle for rootProject.name first (project name, not namespace)
        for settings_name in &["settings.gradle", "settings.gradle.kts"] {
            let settings = self.repo_path.join(settings_name);
            if settings.is_file() {
                if let Ok(content) = fs::read_to_string(&settings) {
                    if let Some(name) = parse_settings_gradle_name(&content) {
                        // Strip -root suffix (common convention)
                        let cleaned = name.strip_suffix("-root").unwrap_or(&name);
                        if !cleaned.is_empty() {
                            return Ok(cleaned.to_string());
                        }
                    }
                }
            }
        }

        // Try build.gradle group as fallback (namespace, not project name)
        for gradle_name in &["build.gradle", "build.gradle.kts"] {
            let gradle = self.repo_path.join(gradle_name);
            if gradle.is_file() {
                if let Ok(content) = fs::read_to_string(&gradle) {
                    if let Some(name) = parse_gradle_group(&content) {
                        return Ok(name);
                    }
                }
            }
        }

        // Last fallback: directory name
        let name = self
            .repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        Ok(name.to_string())
    }

    /// Extract the artifact-level identity for local-install dep filtering.
    ///
    /// Unlike `get_package_name()` which may return a Gradle `group` namespace
    /// (e.g., `com.example`), this returns the actual artifact name that appears
    /// in Maven coordinates as the artifactId component. Falls back to the
    /// directory name rather than the group namespace.
    pub fn get_artifact_id(&self) -> Result<String> {
        // pom.xml artifactId — authoritative
        let pom_path = self.repo_path.join("pom.xml");
        let pom_name = pom_path
            .is_file()
            .then(|| fs::read_to_string(&pom_path).ok())
            .flatten()
            .and_then(|c| parse_pom_artifact_id(&c))
            .and_then(|n| {
                let cleaned = n.strip_suffix("-parent").unwrap_or(&n).to_string();
                (!cleaned.is_empty()).then_some(cleaned)
            });
        if let Some(name) = pom_name {
            return Ok(name);
        }

        // settings.gradle rootProject.name — project name, not namespace
        // Convention: multi-module projects name the root "foo-root"
        // but the artifact is "foo". Only strip trailing "-root".
        for settings_name in &["settings.gradle", "settings.gradle.kts"] {
            let settings_path = self.repo_path.join(settings_name);
            let name = settings_path
                .is_file()
                .then(|| fs::read_to_string(&settings_path).ok())
                .flatten()
                .and_then(|c| parse_settings_gradle_name(&c))
                .and_then(|n| {
                    let cleaned = n.strip_suffix("-root").unwrap_or(&n).to_string();
                    (!cleaned.is_empty()).then_some(cleaned)
                });
            if let Some(name) = name {
                return Ok(name);
            }
        }

        // build.gradle archivesBaseName — the actual jar filename base
        for gradle_name in &["build.gradle", "build.gradle.kts"] {
            let gradle_path = self.repo_path.join(gradle_name);
            let name = gradle_path
                .is_file()
                .then(|| fs::read_to_string(&gradle_path).ok())
                .flatten()
                .and_then(|c| parse_gradle_archives_base_name(&c));
            if let Some(name) = name {
                return Ok(name);
            }
        }

        // Directory name — better than group for artifact identity
        let name = self
            .repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        Ok(name.to_string())
    }

    /// Extract version from pom.xml or build.gradle.
    pub fn get_version(&self) -> Result<String> {
        // Try pom.xml
        let pom = self.repo_path.join("pom.xml");
        if pom.is_file() {
            if let Ok(content) = fs::read_to_string(&pom) {
                if let Some(v) = parse_pom_version(&content) {
                    return Ok(v);
                }
            }
        }

        // Try build.gradle / build.gradle.kts
        for gradle_name in &["build.gradle", "build.gradle.kts"] {
            let gradle = self.repo_path.join(gradle_name);
            if gradle.is_file() {
                if let Ok(content) = fs::read_to_string(&gradle) {
                    if let Some(v) = parse_gradle_version(&content) {
                        return Ok(v);
                    }
                }
            }
        }

        warn!(
            "No version found in pom.xml or build.gradle for {} (may be parent-inherited)",
            self.repo_path.display()
        );
        Ok("unknown".to_string())
    }

    /// Extract license from pom.xml or LICENSE file.
    pub fn get_license(&self) -> Option<String> {
        // Try pom.xml <licenses> section
        let pom = self.repo_path.join("pom.xml");
        if pom.is_file() {
            if let Ok(content) = fs::read_to_string(&pom) {
                if let Some(license) = parse_pom_license(&content) {
                    return Some(license);
                }
            }
        }

        // Fallback: LICENSE file
        for name in super::LICENSE_FILENAMES {
            let path = self.repo_path.join(name);
            if let Ok(content) = fs::read_to_string(&path) {
                if let Some(license) = super::classify_license(&content) {
                    return Some(license);
                }
                return content
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .map(String::from);
            }
        }
        None
    }

    /// Derive project URLs from pom.xml or build.gradle.
    pub fn get_project_urls(&self) -> Vec<(String, String)> {
        let mut urls = Vec::new();

        let pom = self.repo_path.join("pom.xml");
        if pom.is_file() {
            if let Ok(content) = fs::read_to_string(&pom) {
                if let Some(url) = parse_pom_url(&content) {
                    urls.push(("Homepage".into(), url));
                }
                if let Some(scm_url) = parse_pom_scm_url(&content) {
                    urls.push(("Source".into(), scm_url));
                }
            }
        }

        urls
    }

    // ── Private helpers ────────────────────────────────────────────────

    /// Collect .java files. If `tests_only`, collect only from test directories.
    /// Otherwise, collect only from non-test directories.
    fn collect_java_files(
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
                    if !name.ends_with(".java") {
                        continue;
                    }
                    let rel_path = path.strip_prefix(&self.repo_path).unwrap_or(&path);
                    let is_test = Self::is_test_path(rel_path);
                    if tests_only == is_test {
                        files.push(path);
                    }
                }
            } else if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if Self::should_skip_dir(name) {
                        continue;
                    }
                    // Skip example directories at repo root — they're collected separately.
                    // Don't skip deep "example" package components (e.g., com.example).
                    if depth == 0 && matches!(name, "examples" | "example" | "samples" | "sample") {
                        continue;
                    }
                    self.collect_java_files(&path, files, depth + 1, tests_only)?;
                }
            }
        }

        Ok(())
    }

    /// Collect all .java files in a directory (for examples).
    fn collect_all_java_in_dir(
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
                    if name.ends_with(".java") {
                        files.push(path);
                    }
                }
            } else if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !Self::should_skip_dir(name) {
                        self.collect_all_java_in_dir(&path, files, depth + 1)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Check if a path is inside a test directory.
    /// Matches standard Maven/Gradle layouts (`src/test/`, `tests/`) and
    /// common test-named files, but NOT deep package components like
    /// `org.springframework.test.context` which are production code.
    fn is_test_path(path: &Path) -> bool {
        let components: Vec<&str> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        // Match src/test/* (Maven/Gradle standard layout)
        for window in components.windows(2) {
            if window[0] == "src" && (window[1] == "test" || window[1] == "tests") {
                return true;
            }
        }

        // Match top-level test/ or tests/ directory.
        // Skip root/prefix components on any OS: "/" on Unix, "C:" / "\" on Windows.
        for comp in &components {
            if *comp == "/" || *comp == "." || *comp == "\\" || comp.ends_with(':') {
                continue;
            }
            // First real directory component
            if *comp == "test" || *comp == "tests" {
                return true;
            }
            break; // Only check the first real component
        }

        // File-name heuristic: only apply *Test.java/*Tests.java suffix check
        // when NOT under src/main/ — files in src/main are production code even
        // if named FooTest.java (e.g., AbstractContextTest, AssertionTest).
        let under_src_main = components
            .windows(2)
            .any(|w| w[0] == "src" && w[1] == "main");
        if !under_src_main {
            if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                if fname.ends_with("Test.java")
                    || fname.ends_with("Tests.java")
                    || fname.ends_with("Spec.java")
                    || fname.ends_with("IT.java")
                {
                    return true;
                }
            }
        }

        false
    }

    /// Directories to skip during traversal.
    fn should_skip_dir(name: &str) -> bool {
        matches!(
            name,
            ".git"
                | ".svn"
                | ".hg"
                | ".idea"
                | ".gradle"
                | ".mvn"
                | "buildSrc"
                | "target"
                | "build"
                | "out"
                | "bin"
                | "node_modules"
                | ".settings"
                | ".classpath"
                | ".project"
        )
    }
}

// ── Parsing helpers (simple string-based, no XML crate) ──────────────

/// Find the earliest POM section boundary after `start_pos`.
/// Returns the position of the first opening tag among `<dependencies>`,
/// `<dependencyManagement>`, `<build>`, `<profiles>`, and `<reporting>`.
/// These sections contain their own `<version>` and `<artifactId>` tags
/// that are NOT the project-level values we want.
fn pom_section_boundary(content: &str, start_pos: usize) -> Option<usize> {
    let boundaries = [
        "<dependencies>",
        "<dependencyManagement>",
        "<build>",
        "<profiles>",
        "<reporting>",
    ];
    boundaries
        .iter()
        .filter_map(|tag| content[start_pos..].find(tag).map(|p| start_pos + p))
        .min()
}

/// Extract `<artifactId>` from pom.xml (top-level, not inside `<parent>` or `<dependency>`).
fn parse_pom_artifact_id(content: &str) -> Option<String> {
    // Strip comments before boundary detection
    let content = strip_xml_comments(content);
    let parent_start = content.find("<parent>");
    let parent_close = content.find("</parent>");
    let parent_end = parent_close.map(|p| p + 9).unwrap_or(0);

    // If <parent> exists but </parent> is missing, XML is malformed — bail
    if parent_start.is_some() && parent_close.is_none() {
        return None;
    }

    // Try before <parent> first (handles non-standard but valid ordering).
    // Limit search to before the earliest section boundary so we don't
    // pick up a dependency's <artifactId> from <dependencies>.
    if let Some(ps) = parent_start {
        let search_end = pom_section_boundary(&content, 0)
            .map(|bp| bp.min(ps))
            .unwrap_or(ps);
        let before_boundary = &content[..search_end];
        if let Some(v) = extract_xml_tag(before_boundary, "artifactId") {
            if !v.starts_with("${") {
                return Some(v);
            }
        }
    }

    // Check if any section boundary appears before parent_end (malformed ordering)
    let boundary = pom_section_boundary(&content, 0);
    if let Some(bp) = boundary {
        if parent_end > bp {
            return None;
        }
    }
    let search_boundary = pom_section_boundary(&content, parent_end);
    let search_region = if let Some(bp) = search_boundary {
        &content[parent_end..bp]
    } else {
        &content[parent_end..]
    };

    extract_xml_tag(search_region, "artifactId").filter(|v| !v.starts_with("${"))
}

/// Extract `<version>` from pom.xml (top-level).
fn parse_pom_version(content: &str) -> Option<String> {
    // Strip comments before boundary detection
    let content = strip_xml_comments(content);
    // Bail on malformed XML: <parent> opened but never closed
    if content.contains("<parent>") && !content.contains("</parent>") {
        return None;
    }
    let parent_start = content.find("<parent>");
    let parent_end = content.find("</parent>").map(|p| p + 9).unwrap_or(0);
    // Check if any section boundary appears before parent_end (malformed ordering)
    let boundary = pom_section_boundary(&content, 0);
    if let Some(bp) = boundary {
        if parent_end > bp {
            return None;
        }
    }
    // Try before <parent> first (mirrors parse_pom_artifact_id)
    if let Some(ps) = parent_start {
        let search_end = boundary.map(|bp| bp.min(ps)).unwrap_or(ps);
        if let Some(v) = extract_xml_tag(&content[..search_end], "version") {
            if !v.starts_with("${") {
                return Some(v);
            }
        }
    }
    // Separate boundary computation: `boundary` (from 0) guards malformed ordering,
    // `search_boundary` (from parent_end) limits the after-parent search region.
    let search_boundary = pom_section_boundary(&content, parent_end);
    let search_region = if let Some(bp) = search_boundary {
        &content[parent_end..bp]
    } else {
        &content[parent_end..]
    };

    extract_xml_tag(search_region, "version").filter(|v| !v.starts_with("${"))
}

/// Extract license name from pom.xml `<licenses>` section.
fn parse_pom_license(content: &str) -> Option<String> {
    let content = strip_xml_comments(content);
    let start = content.find("<licenses>")?;
    let end = content[start..].find("</licenses>")?;
    let section = &content[start..start + end + "</licenses>".len()];
    extract_xml_tag(section, "name")
}

/// Extract `<url>` from pom.xml (top-level).
fn parse_pom_url(content: &str) -> Option<String> {
    // Strip comments before boundary detection
    let content = strip_xml_comments(content);
    // Bail on malformed XML: <parent> opened but never closed
    if content.contains("<parent>") && !content.contains("</parent>") {
        return None;
    }
    let parent_end = content.find("</parent>").map(|p| p + 9).unwrap_or(0);
    // Check if any section boundary appears before parent_end (malformed ordering)
    if let Some(bp) = pom_section_boundary(&content, 0) {
        if parent_end > bp {
            return None;
        }
    }
    let section_end = pom_section_boundary(&content, parent_end).unwrap_or(content.len());
    // Also exclude metadata blocks that contain their own nested <url>
    let nested_url_start = [
        "<organization>",
        "<licenses>",
        "<developers>",
        "<contributors>",
        "<mailingLists>",
        "<issueManagement>",
        "<ciManagement>",
        "<distributionManagement>",
        "<properties>",
        "<repositories>",
        "<pluginRepositories>",
        "<scm>",
    ]
    .iter()
    .filter_map(|tag| content[parent_end..].find(tag).map(|p| parent_end + p))
    .min()
    .unwrap_or(content.len());
    let end_pos = section_end.min(nested_url_start);
    let search = &content[parent_end..end_pos];
    extract_xml_tag(search, "url")
}

/// Extract SCM URL from pom.xml `<scm>` section.
fn parse_pom_scm_url(content: &str) -> Option<String> {
    let content = strip_xml_comments(content);
    let start = content.find("<scm>")?;
    let end = content[start..].find("</scm>")?;
    let section = &content[start..start + end];
    extract_xml_tag(section, "url")
        .or_else(|| extract_xml_tag(section, "connection"))
        .map(|url| {
            // Strip Maven SCM prefix (scm:git:, scm:svn:, scm:hg:, etc.)
            if let Some(stripped) = url.strip_prefix("scm:") {
                if let Some(pos) = stripped.find(':') {
                    return stripped[pos + 1..].to_string();
                }
            }
            url
        })
}

/// Extract a quoted string value from a Gradle RHS, rejecting computed/concatenated
/// expressions like `"1.0" + suffix`. Returns `None` if the tail after the closing
/// quote contains non-comment content (e.g., `+`, method calls).
fn extract_gradle_quoted(rhs: &str) -> Option<String> {
    if (rhs.starts_with('\'') || rhs.starts_with('"')) && rhs.len() > 1 {
        let quote = rhs.chars().next().unwrap();
        if let Some(end) = rhs[1..].find(quote) {
            let value = &rhs[1..1 + end];
            if value.is_empty() {
                return None;
            }
            // Strip one optional closing ')' for function-call syntax like version("1.0")
            let tail = rhs[1 + end + 1..].trim();
            let tail = tail.strip_prefix(')').map(|t| t.trim()).unwrap_or(tail);
            // Accept if tail is empty, a line comment, or a block comment that
            // runs to end-of-input (nothing but whitespace after the closing */).
            // Rejects `"1.0" /* old */ + suffix` — a computed version expression
            // that happens to start with /* but is followed by more code.
            if tail.is_empty() || tail.starts_with("//") {
                return Some(value.to_string());
            }
            if let Some(rest) = tail.strip_prefix("/*") {
                if let Some(close_idx) = rest.find("*/") {
                    if rest[close_idx + 2..].trim().is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Extract `archivesBaseName` from build.gradle — the actual jar filename base.
/// This is more reliable than `group` for artifact identity since it directly
/// corresponds to the jar name and Maven artifactId convention.
fn parse_gradle_archives_base_name(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }
        // archivesBaseName = 'my-lib' or archivesBaseName = "my-lib"
        // Also: base.archivesName.set("my-lib") (Gradle 7+ convention)
        if trimmed.starts_with("archivesBaseName") {
            if let Some((_, rhs)) = trimmed.split_once('=') {
                if let Some(v) = extract_gradle_quoted(rhs.trim()) {
                    return Some(v);
                }
            }
        } else if trimmed.starts_with("archivesName") || trimmed.contains(".archivesName") {
            // archivesName = "my-lib" or base.archivesName.set("my-lib")
            if let Some((_, rhs)) = trimmed.split_once('=') {
                if let Some(v) = extract_gradle_quoted(rhs.trim()) {
                    return Some(v);
                }
            }
            // base.archivesName.set("my-lib") — extract the arg inside .set(...)
            if let Some(set_pos) = trimmed.find(".set(") {
                let inner = &trimmed[set_pos + 5..]; // after ".set("
                if let Some(v) = extract_gradle_quoted(inner) {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Extract `group` from build.gradle.
/// Only accepts quoted string values — skips constants like `JavaBasePlugin.DOCUMENTATION_GROUP`.
fn parse_gradle_group(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        // Exact "group" keyword — reject groupId, grouping, etc.
        if trimmed.starts_with("group") && trimmed[5..].starts_with([' ', '=', '\t', '.', '(']) {
            // Handle "group = 'x'", "group 'x'", "group.set('x')", "group('x')"
            let rhs = match trimmed.split_once('=') {
                Some((lhs, r)) if lhs.trim() == "group" => r.trim(),
                Some(_) => continue, // group.release = ... etc.
                None => trimmed[5..]
                    .trim()
                    .trim_start_matches(".set")
                    .trim()
                    .trim_start_matches('(')
                    .strip_suffix(')')
                    .unwrap_or(trimmed[5..].trim())
                    .trim(),
            };
            if let Some(v) = extract_gradle_quoted(rhs) {
                return Some(v);
            }
        }
    }
    None
}

/// Extract `rootProject.name` from settings.gradle as a fallback package name.
fn parse_settings_gradle_name(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        // Exact match — reject rootProject.nameSuffix etc.
        if trimmed.starts_with("rootProject.name") && trimmed[16..].starts_with([' ', '=', '\t']) {
            // Handle both "rootProject.name = 'x'" and "rootProject.name 'x'" (method-call)
            let rhs = match trimmed.split_once('=') {
                Some((_, r)) => r.trim(),
                None => trimmed[16..].trim(), // skip "rootProject.name" keyword
            };
            if let Some(v) = extract_gradle_quoted(rhs) {
                return Some(v);
            }
        }
    }
    None
}

/// Extract `version` from build.gradle.
/// Only matches `version = '...'` or `version '...'`, NOT `versionCode`, `versionName`, etc.
fn parse_gradle_version(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        // Must be exactly "version" followed by = or space+quote
        if !trimmed.starts_with("version") {
            continue;
        }
        let rest = &trimmed["version".len()..];
        if rest.is_empty() {
            continue;
        }
        let first = rest.chars().next().unwrap();
        // Accept version=, version , version\t, version', version", version(, version.
        // Reject "versionCode", "versionName" (first char is alphanumeric).
        if first != '='
            && first != ' '
            && first != '\t'
            && first != '\''
            && first != '"'
            && first != '('
            && first != '.'
        {
            continue;
        }
        if let Some((lhs, rhs)) = trimmed.split_once('=') {
            // Only match "version = ..." — reject "version.release = ...", etc.
            if lhs.trim() != "version" {
                continue;
            }
            let rhs = rhs.trim();
            if let Some(v) = extract_gradle_quoted(rhs) {
                return Some(v);
            }
        } else {
            // version '1.0.0', version("1.0.0"), or version.set("1.0.0") (Kotlin DSL)
            let rest_inner = rest
                .trim()
                .trim_start_matches(".set")
                .trim()
                .trim_start_matches('(');
            // Strip exactly one trailing ')' — not all (trim_end_matches strips all)
            let rest_trimmed = rest_inner.strip_suffix(')').unwrap_or(rest_inner).trim();
            if let Some(v) = extract_gradle_quoted(rest_trimmed) {
                return Some(v);
            }
            // No quoted value found — skip
            continue;
        }
    }
    None
}

/// Alias for the shared XML comment stripper in util.rs.
fn strip_xml_comments(content: &str) -> String {
    crate::util::strip_xml_comments(content)
}

/// Simple XML tag value extraction — finds `<tag>value</tag>`.
/// Strips XML comments first to avoid matching commented-out tags.
/// Note: some callers (parse_pom_artifact_id, parse_pom_version) pre-strip for
/// boundary computation — the double-strip here is a no-op in those cases.
fn extract_xml_tag(content: &str, tag: &str) -> Option<String> {
    let clean = strip_xml_comments(content);
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = clean.find(&open)?;
    let value_start = start + open.len();
    let end = clean[value_start..].find(&close)?;
    let value = clean[value_start..value_start + end].trim();
    if value.is_empty() {
        None
    } else {
        debug!("Extracted <{}> = {}", tag, value);
        Some(value.to_string())
    }
}

/// Parse dependencies from pom.xml `<dependency>` elements.
/// Currently only used in tests — will be wired into collect_java when
/// dependency context is added to LLM prompts.
#[cfg(test)]
pub(crate) fn parse_pom_dependencies(content: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut search_from = 0;
    while let Some(start) = content[search_from..].find("<dependency>") {
        let abs_start = search_from + start;
        if let Some(end) = content[abs_start..].find("</dependency>") {
            let block = &content[abs_start..abs_start + end];
            if let (Some(group), Some(artifact)) = (
                extract_xml_tag(block, "groupId"),
                extract_xml_tag(block, "artifactId"),
            ) {
                let dep = format!("{group}:{artifact}");
                if !deps.contains(&dep) {
                    deps.push(dep);
                }
            }
            search_from = abs_start + end + "</dependency>".len();
        } else {
            break;
        }
    }
    deps
}

/// Parse dependencies from build.gradle `implementation`/`api` lines.
#[cfg(test)]
pub(crate) fn parse_gradle_dependencies(content: &str) -> Vec<String> {
    let mut deps = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Match: implementation 'group:artifact:version' or api "group:artifact:version"
        for keyword in &["implementation", "api", "compile", "testImplementation"] {
            if let Some(stripped) = trimmed.strip_prefix(keyword) {
                // Ensure word boundary — reject "compileOnly", "implementationClass" etc.
                if let Some(next_ch) = stripped.chars().next() {
                    if next_ch.is_alphanumeric() {
                        continue;
                    }
                }
                let rest = stripped.trim();
                // Strip parentheses if present: implementation("...")
                let rest = rest.trim_start_matches('(').trim_end_matches(')').trim();
                let dep = rest
                    .trim_matches(|c: char| c == '\'' || c == '"')
                    .to_string();
                // Skip non-Maven notations (project refs, file deps)
                if dep.starts_with("project(")
                    || dep.starts_with("files(")
                    || dep.starts_with("fileTree(")
                    || dep.starts_with(':')
                {
                    continue;
                }
                // Must look like a Maven coordinate (group:artifact[:version])
                if dep.contains(':') && !deps.contains(&dep) {
                    deps.push(dep);
                }
            }
        }
    }
    deps
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_maven_project(tmp: &TempDir) {
        fs::write(
            tmp.path().join("pom.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>my-library</artifactId>
    <version>1.2.3</version>
    <url>https://example.com/my-library</url>
    <licenses>
        <license>
            <name>Apache-2.0</name>
        </license>
    </licenses>
    <scm>
        <url>https://github.com/example/my-library</url>
    </scm>
    <dependencies>
        <dependency>
            <groupId>org.slf4j</groupId>
            <artifactId>slf4j-api</artifactId>
            <version>2.0.9</version>
        </dependency>
    </dependencies>
</project>"#,
        )
        .unwrap();

        let src = tmp.path().join("src/main/java/com/example");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("App.java"), "public class App {}").unwrap();

        let test = tmp.path().join("src/test/java/com/example");
        fs::create_dir_all(&test).unwrap();
        fs::write(test.join("AppTest.java"), "public class AppTest {}").unwrap();
    }

    fn make_gradle_project(tmp: &TempDir) {
        fs::write(
            tmp.path().join("build.gradle"),
            r#"plugins {
    id 'java'
}

group = 'com.example'
version = '2.0.0'

dependencies {
    implementation 'org.apache.commons:commons-lang3:3.12.0'
    api 'com.google.guava:guava:31.1-jre'
}
"#,
        )
        .unwrap();

        let src = tmp.path().join("src/main/java/com/example");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("Main.java"), "public class Main {}").unwrap();

        let test = tmp.path().join("src/test/java/com/example");
        fs::create_dir_all(&test).unwrap();
        fs::write(test.join("MainTest.java"), "public class MainTest {}").unwrap();
    }

    // ── Maven project tests ──

    #[test]
    fn find_source_files_maven() {
        let tmp = TempDir::new().unwrap();
        make_maven_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].to_str().unwrap().contains("App.java"));
    }

    #[test]
    fn find_test_files_maven() {
        let tmp = TempDir::new().unwrap();
        make_maven_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].to_str().unwrap().contains("AppTest.java"));
    }

    #[test]
    fn get_package_name_pom() {
        let tmp = TempDir::new().unwrap();
        make_maven_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_package_name().unwrap(), "my-library");
    }

    #[test]
    fn get_version_pom() {
        let tmp = TempDir::new().unwrap();
        make_maven_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_version().unwrap(), "1.2.3");
    }

    #[test]
    fn get_license_pom() {
        let tmp = TempDir::new().unwrap();
        make_maven_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_license(), Some("Apache-2.0".to_string()));
    }

    #[test]
    fn get_project_urls_pom() {
        let tmp = TempDir::new().unwrap();
        make_maven_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        let urls = handler.get_project_urls();
        assert!(urls.iter().any(|(k, _)| k == "Homepage"));
        assert!(urls.iter().any(|(k, _)| k == "Source"));
    }

    // ── Gradle project tests ──

    #[test]
    fn find_source_files_gradle() {
        let tmp = TempDir::new().unwrap();
        make_gradle_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].to_str().unwrap().contains("Main.java"));
    }

    #[test]
    fn find_test_files_gradle() {
        let tmp = TempDir::new().unwrap();
        make_gradle_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_test_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].to_str().unwrap().contains("MainTest.java"));
    }

    #[test]
    fn get_package_name_gradle() {
        let tmp = TempDir::new().unwrap();
        make_gradle_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_package_name().unwrap(), "com.example");
    }

    #[test]
    fn get_version_gradle() {
        let tmp = TempDir::new().unwrap();
        make_gradle_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_version().unwrap(), "2.0.0");
    }

    #[test]
    fn get_package_name_strips_parent_suffix() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId>guava-parent</artifactId></project>",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_package_name().unwrap(), "guava");
    }

    #[test]
    fn get_package_name_gradle_skips_constants() {
        let tmp = TempDir::new().unwrap();
        // group = JavaBasePlugin.DOCUMENTATION_GROUP is not a quoted string
        fs::write(
            tmp.path().join("build.gradle"),
            "group = JavaBasePlugin.DOCUMENTATION_GROUP\nversion = '1.0'",
        )
        .unwrap();
        fs::write(
            tmp.path().join("settings.gradle"),
            "rootProject.name = 'retrofit-root'",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        // Should fall through to settings.gradle and strip -root
        assert_eq!(handler.get_package_name().unwrap(), "retrofit");
    }

    #[test]
    fn get_package_name_settings_gradle() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("build.gradle"), "apply plugin: 'java'").unwrap();
        fs::write(
            tmp.path().join("settings.gradle"),
            "rootProject.name = 'my-cool-lib'",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_package_name().unwrap(), "my-cool-lib");
    }

    #[test]
    fn parse_settings_gradle_name_basic() {
        assert_eq!(
            parse_settings_gradle_name("rootProject.name = 'my-lib'"),
            Some("my-lib".to_string())
        );
        assert_eq!(
            parse_settings_gradle_name("rootProject.name = \"my-lib\""),
            Some("my-lib".to_string())
        );
    }

    // ── Parsing unit tests ──

    #[test]
    fn strip_xml_comments_removes_single_comment() {
        let input = "before <!-- comment --> after";
        assert_eq!(strip_xml_comments(input), "before  after");
    }

    #[test]
    fn strip_xml_comments_removes_multiline() {
        let input = "<project>\n<!-- \n<artifactId>old</artifactId>\n-->\n<artifactId>real</artifactId>\n</project>";
        let clean = strip_xml_comments(input);
        assert!(!clean.contains("old"));
        assert!(clean.contains("real"));
    }

    #[test]
    fn extract_xml_tag_skips_commented_out_value() {
        let pom = "<!-- <artifactId>wrong</artifactId> -->\n<artifactId>correct</artifactId>";
        assert_eq!(
            extract_xml_tag(pom, "artifactId"),
            Some("correct".to_string())
        );
    }

    #[test]
    fn parse_pom_artifact_id_basic() {
        let pom = "<project><artifactId>foo</artifactId></project>";
        assert_eq!(parse_pom_artifact_id(pom), Some("foo".to_string()));
    }

    #[test]
    fn parse_pom_artifact_id_skips_parent() {
        let pom = r#"<project>
    <parent><artifactId>parent-art</artifactId></parent>
    <artifactId>child-art</artifactId>
</project>"#;
        assert_eq!(parse_pom_artifact_id(pom), Some("child-art".to_string()));
    }

    #[test]
    fn parse_pom_version_basic() {
        let pom = "<project><version>3.0.0</version></project>";
        assert_eq!(parse_pom_version(pom), Some("3.0.0".to_string()));
    }

    #[test]
    fn parse_pom_version_before_parent() {
        // Version declared before <parent> — should be found
        let pom =
            "<project><version>2.0</version><parent><version>1.0</version></parent></project>";
        assert_eq!(parse_pom_version(pom), Some("2.0".to_string()));
    }

    #[test]
    fn parse_pom_license_basic() {
        let pom = "<licenses><license><name>MIT</name></license></licenses>";
        assert_eq!(parse_pom_license(pom), Some("MIT".to_string()));
    }

    #[test]
    fn parse_gradle_group_basic() {
        let content = "group = 'com.example'\nversion = '1.0'";
        assert_eq!(parse_gradle_group(content), Some("com.example".to_string()));
    }

    #[test]
    fn parse_gradle_group_double_quotes() {
        let content = "group = \"org.test\"";
        assert_eq!(parse_gradle_group(content), Some("org.test".to_string()));
    }

    #[test]
    fn parse_gradle_version_basic() {
        let content = "group = 'com.example'\nversion = '3.2.1'";
        assert_eq!(parse_gradle_version(content), Some("3.2.1".to_string()));
    }

    #[test]
    fn parse_gradle_version_ignores_versions_plugin() {
        let content = "versions = '1.0'\nversion = '2.0'";
        assert_eq!(parse_gradle_version(content), Some("2.0".to_string()));
    }

    #[test]
    fn parse_pom_dependencies_basic() {
        let pom = r#"<dependencies>
    <dependency>
        <groupId>org.slf4j</groupId>
        <artifactId>slf4j-api</artifactId>
        <version>2.0</version>
    </dependency>
    <dependency>
        <groupId>junit</groupId>
        <artifactId>junit</artifactId>
    </dependency>
</dependencies>"#;
        let deps = parse_pom_dependencies(pom);
        assert_eq!(deps, vec!["org.slf4j:slf4j-api", "junit:junit"]);
    }

    #[test]
    fn parse_gradle_dependencies_basic() {
        let content = r#"
dependencies {
    implementation 'org.apache.commons:commons-lang3:3.12.0'
    api "com.google.guava:guava:31.1-jre"
    testImplementation 'junit:junit:4.13.2'
}
"#;
        let deps = parse_gradle_dependencies(content);
        assert_eq!(deps.len(), 3);
        assert!(deps.contains(&"org.apache.commons:commons-lang3:3.12.0".to_string()));
        assert!(deps.contains(&"com.google.guava:guava:31.1-jre".to_string()));
    }

    #[test]
    fn parse_pom_scm_url_basic() {
        let pom = "<scm><url>https://github.com/foo/bar</url></scm>";
        assert_eq!(
            parse_pom_scm_url(pom),
            Some("https://github.com/foo/bar".to_string())
        );
    }

    #[test]
    fn find_examples_empty() {
        let tmp = TempDir::new().unwrap();
        make_maven_project(&tmp);
        let handler = JavaHandler::new(tmp.path());
        let examples = handler.find_examples().unwrap();
        assert!(examples.is_empty());
    }

    #[test]
    fn find_examples_with_dir() {
        let tmp = TempDir::new().unwrap();
        make_maven_project(&tmp);
        let ex_dir = tmp.path().join("examples");
        fs::create_dir_all(&ex_dir).unwrap();
        fs::write(ex_dir.join("Example.java"), "class Example {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let examples = handler.find_examples().unwrap();
        assert_eq!(examples.len(), 1);
    }

    #[test]
    fn find_docs_with_readme() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("README.md"), "# Docs").unwrap();
        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn find_changelog_found() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("CHANGELOG.md"), "# Changes").unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert!(handler.find_changelog().is_some());
    }

    #[test]
    fn find_changelog_not_found() {
        let tmp = TempDir::new().unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert!(handler.find_changelog().is_none());
    }

    #[test]
    fn no_source_files_errors() {
        let tmp = TempDir::new().unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert!(handler.find_source_files().is_err());
    }

    #[test]
    fn no_test_files_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let handler = JavaHandler::new(tmp.path());
        let result = handler.find_test_files().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn get_license_from_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("LICENSE"),
            "MIT License\n\nCopyright (c) 2024",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_license(), Some("MIT".to_string()));
    }

    #[test]
    fn get_version_fallback() {
        let tmp = TempDir::new().unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_version().unwrap(), "unknown");
    }

    #[test]
    fn skip_build_output_dirs() {
        let tmp = TempDir::new().unwrap();
        make_maven_project(&tmp);
        // Add a .java file inside target/ — should be skipped
        let target = tmp.path().join("target/classes");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("Generated.java"), "class Generated {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_source_files().unwrap();
        assert!(!files.iter().any(|f| f.to_str().unwrap().contains("target")));
    }

    #[test]
    fn parse_gradle_dependencies_with_parens() {
        let content = r#"
dependencies {
    implementation("org.test:lib:1.0")
}
"#;
        let deps = parse_gradle_dependencies(content);
        assert_eq!(deps, vec!["org.test:lib:1.0"]);
    }

    #[test]
    fn parse_pom_url_basic() {
        let pom = "<project><url>https://example.com</url></project>";
        assert_eq!(parse_pom_url(pom), Some("https://example.com".to_string()));
    }

    #[test]
    fn extract_xml_tag_whitespace() {
        let content = "<tag>  value  </tag>";
        assert_eq!(extract_xml_tag(content, "tag"), Some("value".to_string()));
    }

    #[test]
    fn extract_xml_tag_empty() {
        let content = "<tag></tag>";
        assert_eq!(extract_xml_tag(content, "tag"), None);
    }

    #[test]
    fn pom_scm_connection_fallback() {
        let pom = "<scm><connection>scm:git:https://github.com/foo/bar.git</connection></scm>";
        assert_eq!(
            parse_pom_scm_url(pom),
            Some("https://github.com/foo/bar.git".to_string())
        );
    }

    #[test]
    fn pom_scm_strips_svn_prefix() {
        let pom = "<scm><url>scm:svn:https://svn.apache.org/repos/asf/foo</url></scm>";
        assert_eq!(
            parse_pom_scm_url(pom),
            Some("https://svn.apache.org/repos/asf/foo".to_string())
        );
    }

    // ── find_docs: README variants and docs/ directory ──

    #[test]
    fn find_docs_readme_rst() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("README.rst"), "Title\n=====").unwrap();
        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].to_str().unwrap().contains("README.rst"));
    }

    #[test]
    fn find_docs_readme_txt() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("README.txt"), "Hello").unwrap();
        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].to_str().unwrap().contains("README.txt"));
    }

    #[test]
    fn find_docs_readme_no_extension() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("README"), "Hello").unwrap();
        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].to_str().unwrap().contains("README"));
    }

    #[test]
    fn find_docs_with_docs_directory() {
        let tmp = TempDir::new().unwrap();
        let docs_dir = tmp.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        fs::write(docs_dir.join("guide.md"), "# Guide").unwrap();
        fs::write(docs_dir.join("api.rst"), "API docs").unwrap();
        fs::write(docs_dir.join("notes.txt"), "Notes").unwrap();
        fs::write(docs_dir.join("manual.adoc"), "= Manual").unwrap();
        // Non-doc file should be excluded
        fs::write(docs_dir.join("build.xml"), "<project/>").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 4);
    }

    #[test]
    fn find_docs_with_doc_directory() {
        let tmp = TempDir::new().unwrap();
        let doc_dir = tmp.path().join("doc");
        fs::create_dir_all(&doc_dir).unwrap();
        fs::write(doc_dir.join("overview.md"), "# Overview").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn find_docs_recursive_nested_subdirs() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("docs/sub/deep");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("deep.md"), "# Deep").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].to_str().unwrap().contains("deep.md"));
    }

    #[test]
    fn find_docs_readme_takes_priority_over_later_variants() {
        // When README.md exists, don't also add README.rst
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("README.md"), "# Main").unwrap();
        fs::write(tmp.path().join("README.rst"), "Fallback").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].to_str().unwrap().contains("README.md"));
    }

    // ── get_package_name: edge cases ──

    #[test]
    fn get_package_name_parent_only_empty_after_strip() {
        // artifactId is exactly "-parent" -> stripped to empty -> fallback to dir name
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId>-parent</artifactId></project>",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        let name = handler.get_package_name().unwrap();
        // Falls through to directory name fallback
        assert!(!name.is_empty());
    }

    #[test]
    fn get_package_name_gradle_kts() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("build.gradle.kts"),
            "group = \"io.ktor\"\nversion = \"2.3.0\"",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_package_name().unwrap(), "io.ktor");
    }

    #[test]
    fn get_package_name_settings_gradle_kts() {
        let tmp = TempDir::new().unwrap();
        // No pom.xml, no build.gradle group
        fs::write(
            tmp.path().join("build.gradle.kts"),
            "plugins { id(\"java\") }",
        )
        .unwrap();
        fs::write(
            tmp.path().join("settings.gradle.kts"),
            "rootProject.name = \"my-kts-lib\"",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_package_name().unwrap(), "my-kts-lib");
    }

    #[test]
    fn get_package_name_settings_gradle_kts_strips_root() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("settings.gradle.kts"),
            "rootProject.name = \"myapp-root\"",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_package_name().unwrap(), "myapp");
    }

    #[test]
    fn get_package_name_fallback_to_dir_name() {
        let tmp = TempDir::new().unwrap();
        // No pom.xml, no build.gradle, no settings.gradle
        let handler = JavaHandler::new(tmp.path());
        let name = handler.get_package_name().unwrap();
        // Should be the temp dir's directory name (non-empty)
        assert!(!name.is_empty());
        assert_ne!(name, "unknown");
    }

    #[test]
    fn get_package_name_pom_no_artifact_id() {
        // pom.xml exists but has no artifactId — fall through to gradle/settings/dir
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><modelVersion>4.0.0</modelVersion></project>",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        let name = handler.get_package_name().unwrap();
        // Falls to directory name
        assert!(!name.is_empty());
    }

    // ── get_version: handler-level paths ──

    #[test]
    fn get_version_from_pom_via_handler() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><version>5.1.0</version></project>",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_version().unwrap(), "5.1.0");
    }

    #[test]
    fn get_version_from_gradle_kts() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("build.gradle.kts"), "version = \"3.5.0\"").unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_version().unwrap(), "3.5.0");
    }

    #[test]
    fn get_version_pom_no_version_falls_through_to_gradle() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId>foo</artifactId></project>",
        )
        .unwrap();
        fs::write(tmp.path().join("build.gradle"), "version = '7.0.0'").unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_version().unwrap(), "7.0.0");
    }

    // ── get_license: handler-level paths ──

    #[test]
    fn get_license_from_pom_via_handler() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><licenses><license><name>GPL-3.0</name></license></licenses></project>",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_license(), Some("GPL-3.0".to_string()));
    }

    #[test]
    fn get_license_pom_no_licenses_falls_to_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId>foo</artifactId></project>",
        )
        .unwrap();
        fs::write(tmp.path().join("LICENSE"), "Apache License\nVersion 2.0").unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_license(), Some("Apache-2.0".to_string()));
    }

    #[test]
    fn get_license_file_unrecognized_returns_first_line() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("LICENSE"),
            "Custom Corporate License v1.0\n\nAll rights reserved.",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(
            handler.get_license(),
            Some("Custom Corporate License v1.0".to_string())
        );
    }

    #[test]
    fn get_license_file_blank_lines_only_returns_none_style() {
        // LICENSE file with only whitespace lines — first non-empty line is None
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("LICENSE"), "   \n  \n  ").unwrap();
        let handler = JavaHandler::new(tmp.path());
        // classify_license returns None, and no non-empty line -> returns None
        assert_eq!(handler.get_license(), None);
    }

    #[test]
    fn get_license_none_when_no_files() {
        let tmp = TempDir::new().unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_license(), None);
    }

    // ── get_project_urls: handler-level ──

    #[test]
    fn get_project_urls_no_pom() {
        let tmp = TempDir::new().unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert!(handler.get_project_urls().is_empty());
    }

    #[test]
    fn get_project_urls_pom_with_url_and_scm() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            r#"<project>
    <url>https://example.com</url>
    <scm><url>https://github.com/example/lib</url></scm>
</project>"#,
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        let urls = handler.get_project_urls();
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].0, "Homepage");
        assert_eq!(urls[1].0, "Source");
    }

    #[test]
    fn get_project_urls_pom_no_urls() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId>foo</artifactId></project>",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert!(handler.get_project_urls().is_empty());
    }

    // ── File discovery: nested examples ──

    #[test]
    fn find_examples_with_nested_subdirs() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("examples/advanced");
        fs::create_dir_all(&nested).unwrap();
        fs::write(tmp.path().join("examples/Simple.java"), "class Simple {}").unwrap();
        fs::write(nested.join("Advanced.java"), "class Advanced {}").unwrap();
        // Non-java file should be ignored
        fs::write(tmp.path().join("examples/README.md"), "# Examples").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let examples = handler.find_examples().unwrap();
        assert_eq!(examples.len(), 2);
    }

    #[test]
    fn find_examples_sample_dir() {
        let tmp = TempDir::new().unwrap();
        let sample_dir = tmp.path().join("sample");
        fs::create_dir_all(&sample_dir).unwrap();
        fs::write(sample_dir.join("Demo.java"), "class Demo {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let examples = handler.find_examples().unwrap();
        assert_eq!(examples.len(), 1);
    }

    #[test]
    fn find_examples_samples_dir() {
        let tmp = TempDir::new().unwrap();
        let samples_dir = tmp.path().join("samples");
        fs::create_dir_all(&samples_dir).unwrap();
        fs::write(samples_dir.join("Demo.java"), "class Demo {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let examples = handler.find_examples().unwrap();
        assert_eq!(examples.len(), 1);
    }

    #[test]
    fn find_examples_skips_build_output_dirs() {
        let tmp = TempDir::new().unwrap();
        let ex_dir = tmp.path().join("examples");
        fs::create_dir_all(ex_dir.join("target")).unwrap();
        fs::write(ex_dir.join("target/Generated.java"), "class Generated {}").unwrap();
        fs::write(ex_dir.join("Example.java"), "class Example {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let examples = handler.find_examples().unwrap();
        assert_eq!(examples.len(), 1);
        assert!(examples[0].to_str().unwrap().contains("Example.java"));
    }

    // ── File classification: test file naming patterns ──

    #[test]
    fn find_test_files_tests_suffix_in_test_dir() {
        // *Tests.java under src/test/ should be classified as test
        let tmp = TempDir::new().unwrap();
        let test_dir = tmp.path().join("src/test/java");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(test_dir.join("AppTests.java"), "class AppTests {}").unwrap();
        // Need at least one source file too
        let src = tmp.path().join("src/main/java");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("App.java"), "class App {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let tests = handler.find_test_files().unwrap();
        assert_eq!(tests.len(), 1);
        assert!(tests[0].to_str().unwrap().contains("AppTests.java"));
    }

    #[test]
    fn find_test_files_suffix_not_under_src_main() {
        // *Test.java suffix only triggers outside src/main/
        let tmp = TempDir::new().unwrap();
        // File under src/main — should NOT be classified as test
        let main_src = tmp.path().join("src/main/java");
        fs::create_dir_all(&main_src).unwrap();
        fs::write(main_src.join("AppTest.java"), "class AppTest {}").unwrap();
        // File under top-level — SHOULD be classified as test
        fs::write(tmp.path().join("FooTest.java"), "class FooTest {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let tests = handler.find_test_files().unwrap();
        assert_eq!(tests.len(), 1);
        assert!(tests[0].to_str().unwrap().contains("FooTest.java"));
    }

    #[test]
    fn find_source_files_keeps_all_under_src_main() {
        // Under src/main/, ALL .java files are production — even *Test.java names
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src/main/java");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("App.java"), "class App {}").unwrap();
        fs::write(src.join("AppTest.java"), "class AppTest {}").unwrap();
        fs::write(src.join("TestHelper.java"), "class TestHelper {}").unwrap();
        fs::write(src.join("AppTests.java"), "class AppTests {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_source_files().unwrap();
        // ALL 4 files are production code under src/main
        assert_eq!(files.len(), 4);
    }

    // ── Skipped directories ──

    #[test]
    fn find_source_files_skips_ide_and_vcs_dirs() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src/main/java");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("App.java"), "class App {}").unwrap();

        // Create files in various skip dirs
        for dir_name in &[".git", ".idea", ".gradle", ".mvn", "build", "out", "bin"] {
            let skip = tmp.path().join(dir_name);
            fs::create_dir_all(&skip).unwrap();
            fs::write(skip.join("Bad.java"), "class Bad {}").unwrap();
        }

        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].to_str().unwrap().contains("App.java"));
    }

    // ── Changelog variants ──

    #[test]
    fn find_changelog_changes_md() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("CHANGES.md"), "# Changes").unwrap();
        let handler = JavaHandler::new(tmp.path());
        let cl = handler.find_changelog();
        assert!(cl.is_some());
        assert!(cl.unwrap().to_str().unwrap().contains("CHANGES.md"));
    }

    #[test]
    fn find_changelog_changes_no_ext() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("CHANGES"), "changes").unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert!(handler.find_changelog().is_some());
    }

    #[test]
    fn find_changelog_history_md() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("HISTORY.md"), "# History").unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert!(handler.find_changelog().is_some());
    }

    // ── Parsing edge cases ──

    #[test]
    fn parse_gradle_group_empty_quotes() {
        // group = '' — empty after stripping quotes
        assert_eq!(parse_gradle_group("group = ''"), None);
        assert_eq!(parse_gradle_group("group = \"\""), None);
    }

    #[test]
    fn parse_gradle_group_no_equals() {
        // "group" appears but no '=' on line — but with a non-quoted value
        assert_eq!(parse_gradle_group("grouping stuff"), None);
    }

    #[test]
    fn parse_gradle_group_method_call() {
        // Groovy method-call syntax: group 'com.example' (no '=')
        assert_eq!(
            parse_gradle_group("group 'com.example'"),
            Some("com.example".into())
        );
    }

    #[test]
    fn parse_gradle_group_kotlin_dsl_set() {
        // Kotlin DSL: group.set("com.example")
        assert_eq!(
            parse_gradle_group(r#"group.set("com.example")"#),
            Some("com.example".into())
        );
    }

    #[test]
    fn parse_gradle_group_function_call() {
        // Groovy: group("com.example")
        assert_eq!(
            parse_gradle_group(r#"group("com.example")"#),
            Some("com.example".into())
        );
    }

    #[test]
    fn parse_settings_gradle_name_method_call() {
        // Groovy: rootProject.name 'my-lib' (no '=')
        assert_eq!(
            parse_settings_gradle_name("rootProject.name 'my-lib'"),
            Some("my-lib".into())
        );
    }

    #[test]
    fn parse_settings_gradle_name_empty() {
        assert_eq!(parse_settings_gradle_name("rootProject.name = ''"), None);
    }

    #[test]
    fn parse_settings_gradle_name_no_match() {
        assert_eq!(parse_settings_gradle_name("include ':submodule'"), None);
    }

    #[test]
    fn parse_gradle_version_no_version() {
        assert_eq!(parse_gradle_version("apply plugin: 'java'"), None);
    }

    #[test]
    fn parse_gradle_version_empty_value() {
        assert_eq!(parse_gradle_version("version = ''"), None);
    }

    #[test]
    fn parse_gradle_version_double_quotes() {
        assert_eq!(
            parse_gradle_version("version = \"4.0.0\""),
            Some("4.0.0".to_string())
        );
    }

    #[test]
    fn parse_pom_dependencies_duplicate_dedup() {
        let pom = r#"<dependencies>
    <dependency>
        <groupId>org.slf4j</groupId>
        <artifactId>slf4j-api</artifactId>
    </dependency>
    <dependency>
        <groupId>org.slf4j</groupId>
        <artifactId>slf4j-api</artifactId>
    </dependency>
</dependencies>"#;
        let deps = parse_pom_dependencies(pom);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0], "org.slf4j:slf4j-api");
    }

    #[test]
    fn parse_pom_dependencies_unclosed_dependency() {
        // <dependency> without closing tag — should break out
        let pom =
            "<dependencies><dependency><groupId>org.test</groupId><artifactId>lib</artifactId>";
        let deps = parse_pom_dependencies(pom);
        assert!(deps.is_empty());
    }

    #[test]
    fn parse_pom_dependencies_missing_group_or_artifact() {
        let pom = r#"<dependencies>
    <dependency>
        <groupId>org.test</groupId>
    </dependency>
</dependencies>"#;
        let deps = parse_pom_dependencies(pom);
        assert!(deps.is_empty());
    }

    #[test]
    fn parse_gradle_dependencies_compile_keyword() {
        let content = "compile 'old.dep:lib:1.0'";
        let deps = parse_gradle_dependencies(content);
        assert_eq!(deps, vec!["old.dep:lib:1.0"]);
    }

    #[test]
    fn parse_gradle_dependencies_no_colon_skipped() {
        // A bare string without ':' is not a maven coordinate
        let content = "implementation 'justAString'";
        let deps = parse_gradle_dependencies(content);
        assert!(deps.is_empty());
    }

    #[test]
    fn parse_gradle_dependencies_dedup() {
        let content = "implementation 'a:b:1'\nimplementation 'a:b:1'";
        let deps = parse_gradle_dependencies(content);
        assert_eq!(deps.len(), 1);
    }

    #[test]
    fn parse_pom_artifact_id_no_deps_section() {
        // No <dependencies> tag — search entire region after parent
        let pom = "<project><artifactId>simple</artifactId></project>";
        assert_eq!(parse_pom_artifact_id(pom), Some("simple".to_string()));
    }

    #[test]
    fn parse_pom_version_no_deps_section() {
        let pom = "<project><version>9.0</version></project>";
        assert_eq!(parse_pom_version(pom), Some("9.0".to_string()));
    }

    #[test]
    fn parse_pom_version_ignores_build_section_version() {
        // <version> inside <build> plugin config should NOT be picked up
        let pom = "<project><version>2.0</version><build><plugin><version>3.8.1</version></plugin></build></project>";
        assert_eq!(parse_pom_version(pom), Some("2.0".to_string()));
    }

    #[test]
    fn parse_pom_version_stops_at_dependency_management() {
        // <version> inside <dependencyManagement> should NOT be picked up
        let pom = "<project><version>1.5</version><dependencyManagement><dependency><version>2.0</version></dependency></dependencyManagement></project>";
        assert_eq!(parse_pom_version(pom), Some("1.5".to_string()));
    }

    #[test]
    fn parse_pom_artifact_id_ignores_build_section() {
        let pom = "<project><artifactId>myapp</artifactId><build><plugin><artifactId>maven-compiler</artifactId></plugin></build></project>";
        assert_eq!(parse_pom_artifact_id(pom), Some("myapp".to_string()));
    }

    // ── pom_section_boundary ──

    #[test]
    fn pom_boundary_finds_dependencies() {
        let pom = "<project><version>1.0</version><dependencies></dependencies></project>";
        assert_eq!(
            pom_section_boundary(pom, 0),
            Some(pom.find("<dependencies>").unwrap())
        );
    }

    #[test]
    fn pom_boundary_finds_build_before_dependencies() {
        let pom = "<project><build></build><dependencies></dependencies></project>";
        assert_eq!(
            pom_section_boundary(pom, 0),
            Some(pom.find("<build>").unwrap())
        );
    }

    #[test]
    fn pom_boundary_returns_none_when_no_sections() {
        let pom = "<project><version>1.0</version></project>";
        assert_eq!(pom_section_boundary(pom, 0), None);
    }

    #[test]
    fn pom_boundary_respects_start_pos() {
        let pom = "<dependencies></dependencies><build></build>";
        // Start search after </dependencies>
        let after_deps = pom.find("<build>").unwrap();
        assert_eq!(pom_section_boundary(pom, after_deps), Some(after_deps));
    }

    #[test]
    fn parse_pom_url_before_build() {
        // <url> should be found even when <build> exists but <dependencies> doesn't
        let pom = "<project><url>https://foo.com</url><build/></project>";
        assert_eq!(parse_pom_url(pom), Some("https://foo.com".to_string()));
    }

    #[test]
    fn parse_pom_url_no_url() {
        let pom = "<project><artifactId>foo</artifactId></project>";
        assert_eq!(parse_pom_url(pom), None);
    }

    #[test]
    fn parse_pom_url_ignores_license_url() {
        // No top-level <url>, only a <license><url> — should NOT be returned
        let pom = "<project><licenses><license><url>https://license.example.com</url></license></licenses></project>";
        assert_eq!(parse_pom_url(pom), None);
    }

    #[test]
    fn parse_pom_url_ignores_developer_url() {
        let pom = "<project><developers><developer><url>https://dev.example.com</url></developer></developers></project>";
        assert_eq!(parse_pom_url(pom), None);
    }

    #[test]
    fn parse_pom_scm_url_no_scm() {
        let pom = "<project><artifactId>foo</artifactId></project>";
        assert_eq!(parse_pom_scm_url(pom), None);
    }

    #[test]
    fn parse_pom_license_no_licenses() {
        let pom = "<project><artifactId>foo</artifactId></project>";
        assert_eq!(parse_pom_license(pom), None);
    }

    #[test]
    fn extract_xml_tag_missing() {
        assert_eq!(extract_xml_tag("<foo>bar</foo>", "baz"), None);
    }

    // ── Non-.java files mixed in source tree ──

    #[test]
    fn find_source_files_ignores_non_java() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src/main/java");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("App.java"), "class App {}").unwrap();
        fs::write(src.join("config.xml"), "<config/>").unwrap();
        fs::write(src.join("data.json"), "{}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].to_str().unwrap().contains("App.java"));
    }

    // ── Empty docs dir (no files) ──

    #[test]
    fn find_docs_empty_docs_dir() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert!(docs.is_empty());
    }

    // ── get_package_name: settings.gradle.kts with empty name after strip ──

    #[test]
    fn get_package_name_settings_kts_root_only() {
        // rootProject.name = "-root" -> stripped to empty -> fallback to dir name
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("settings.gradle.kts"),
            "rootProject.name = \"-root\"",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        let name = handler.get_package_name().unwrap();
        assert!(!name.is_empty());
    }

    // ── Malformed POM bounds-check tests ──

    #[test]
    fn parse_pom_artifact_id_parent_after_deps() {
        // Malformed: </parent> appears after <dependencies> — must not panic
        let pom = "<dependencies><dep/></dependencies></parent><artifactId>x</artifactId>";
        assert_eq!(parse_pom_artifact_id(pom), None);
    }

    #[test]
    fn parse_pom_artifact_id_deps_before_parent_narrows_search() {
        // <artifactId> before <dependencies> before <parent> — should find
        // the artifactId because it's before the section boundary
        let pom = "<artifactId>mylib</artifactId><dependencies></dependencies><parent><artifactId>parent-art</artifactId></parent>";
        assert_eq!(parse_pom_artifact_id(pom), Some("mylib".to_string()));
    }

    #[test]
    fn parse_pom_artifact_id_inside_deps_before_parent_rejected() {
        // artifactId only inside <dependencies>, before <parent> — should NOT match
        let pom = "<dependencies><dependency><artifactId>dep-lib</artifactId></dependency></dependencies><parent><artifactId>parent-art</artifactId></parent>";
        assert_eq!(parse_pom_artifact_id(pom), None);
    }

    #[test]
    fn parse_pom_version_parent_after_deps() {
        let pom = "<dependencies></dependencies></parent><version>1.0</version>";
        assert_eq!(parse_pom_version(pom), None);
    }

    #[test]
    fn parse_pom_url_parent_after_build() {
        let pom = "<build></build></parent><url>https://example.com</url>";
        assert_eq!(parse_pom_url(pom), None);
    }

    // ── parse_gradle_version: no-equals-sign path (version '1.0.0') ──

    #[test]
    fn parse_gradle_version_no_equals_sign() {
        // Gradle shorthand: version '1.0.0' without '='
        assert_eq!(
            parse_gradle_version("version '1.0.0'"),
            Some("1.0.0".to_string())
        );
    }

    #[test]
    fn parse_gradle_version_no_equals_double_quotes() {
        assert_eq!(
            parse_gradle_version("version \"2.3.4\""),
            Some("2.3.4".to_string())
        );
    }

    #[test]
    fn parse_gradle_version_bare_keyword_only() {
        // Line is exactly "version" with nothing after — should skip
        assert_eq!(parse_gradle_version("version"), None);
    }

    #[test]
    fn parse_gradle_version_rejects_version_code() {
        // "versionCode" starts with "version" but rest begins with 'C' (not =, space, or quote)
        assert_eq!(parse_gradle_version("versionCode 1"), None);
    }

    #[test]
    fn parse_gradle_version_tab_after_keyword() {
        // Tab between "version" and "=" should be accepted (common in some Gradle files)
        assert_eq!(
            parse_gradle_version("version\t= '3.2.1'"),
            Some("3.2.1".into())
        );
    }

    #[test]
    fn parse_gradle_version_kotlin_dsl_parens() {
        // version("1.0.0") — Kotlin DSL shorthand
        assert_eq!(
            parse_gradle_version(r#"version("1.0.0")"#),
            Some("1.0.0".into())
        );
    }

    #[test]
    fn parse_gradle_version_kotlin_dsl_set() {
        // version.set("1.0.0") — Kotlin DSL Property.set() API
        assert_eq!(
            parse_gradle_version(r#"version.set("1.0.0")"#),
            Some("1.0.0".into())
        );
    }

    #[test]
    fn parse_gradle_version_kotlin_dsl_set_with_space() {
        // version.set ("1.0.0") — space before paren
        assert_eq!(
            parse_gradle_version(r#"version.set ("1.0.0")"#),
            Some("1.0.0".into())
        );
    }

    #[test]
    fn parse_gradle_version_rejects_version_dot_release() {
        // "version.release" is a nebula-release plugin property, not the project version
        assert_eq!(parse_gradle_version("version.release = '2.0.0'"), None);
    }

    #[test]
    fn parse_gradle_version_rejects_version_dot_suffix() {
        // Any "version.xxx = ..." should be rejected
        assert_eq!(parse_gradle_version("version.catalog = '1.0'"), None);
    }

    // ── is_test_path: top-level test/ directory ──

    #[test]
    fn is_test_path_top_level_test_dir() {
        // Files in a top-level test/ directory should be classified as tests
        let path = Path::new("test/com/example/Foo.java");
        assert!(JavaHandler::is_test_path(path));
    }

    #[test]
    fn is_test_path_top_level_tests_dir() {
        let path = Path::new("tests/FooTest.java");
        assert!(JavaHandler::is_test_path(path));
    }

    #[test]
    fn is_test_path_deep_package_test_not_top_level() {
        // Deep "test" in package path should NOT be matched as top-level
        let path = Path::new("com/example/test/util/Helper.java");
        assert!(!JavaHandler::is_test_path(path));
    }

    // ── Depth limit tests ──

    #[test]
    fn collect_java_files_stops_at_max_depth() {
        let tmp = TempDir::new().unwrap();
        // Create a path deeper than MAX_DEPTH (20)
        let mut deep = tmp.path().to_path_buf();
        for i in 0..22 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("Deep.java"), "class Deep {}").unwrap();

        // Also create a shallow file so find_source_files doesn't error
        let shallow = tmp.path().join("src/main/java");
        fs::create_dir_all(&shallow).unwrap();
        fs::write(shallow.join("App.java"), "class App {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_source_files().unwrap();
        assert!(
            !files
                .iter()
                .any(|p| p.to_str().unwrap().contains("Deep.java")),
            "should not find files beyond MAX_DEPTH"
        );
    }

    #[test]
    fn collect_all_java_in_dir_stops_at_max_depth() {
        let tmp = TempDir::new().unwrap();
        let examples = tmp.path().join("examples");
        let mut deep = examples.clone();
        for i in 0..22 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("DeepExample.java"), "class DeepExample {}").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let files = handler.find_examples().unwrap();
        assert!(
            !files
                .iter()
                .any(|p| p.to_str().unwrap().contains("DeepExample.java")),
            "should not find example files beyond MAX_DEPTH"
        );
    }

    #[test]
    fn collect_docs_recursive_stops_at_max_depth() {
        let tmp = TempDir::new().unwrap();
        let mut deep = tmp.path().join("docs");
        for i in 0..22 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep.md"), "# Deep").unwrap();

        let handler = JavaHandler::new(tmp.path());
        let docs = handler.find_docs().unwrap();
        assert!(
            !docs.iter().any(|p| p.to_str().unwrap().contains("deep.md")),
            "should not find docs beyond MAX_DEPTH"
        );
    }

    // ── get_package_name: settings.gradle name empty after stripping -root ──

    #[test]
    fn get_package_name_settings_gradle_root_suffix_empty_name() {
        // rootProject.name = "-root" -> stripped to empty -> fallback to dir name
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("settings.gradle"),
            "rootProject.name = '-root'",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        let name = handler.get_package_name().unwrap();
        // Should fall through to directory name since stripped name is empty
        assert!(!name.is_empty());
    }

    // ── get_version: build.gradle.kts without pom.xml ──

    #[test]
    fn get_version_gradle_kts_no_pom() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("build.gradle.kts"), "version = '8.1.0'").unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_version().unwrap(), "8.1.0");
    }

    // ── is_test_path: absolute paths via TempDir ──

    #[test]
    fn is_test_path_works_with_absolute_paths_via_collect() {
        // Regression: collect_java_files passes absolute paths from entry.path().
        // is_test_path must still classify them correctly after strip_prefix.
        let tmp = TempDir::new().unwrap();

        // Create src/test/java/FooTest.java (should be a test)
        let test_dir = tmp.path().join("src/test/java");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(test_dir.join("FooTest.java"), "class FooTest {}").unwrap();

        // Create src/main/java/App.java (should NOT be a test)
        let main_dir = tmp.path().join("src/main/java");
        fs::create_dir_all(&main_dir).unwrap();
        fs::write(main_dir.join("App.java"), "class App {}").unwrap();

        let handler = JavaHandler::new(tmp.path());

        let tests = handler.find_test_files().unwrap();
        assert!(
            tests.iter().any(|p| p.ends_with("FooTest.java")),
            "should find test file"
        );

        let sources = handler.find_source_files().unwrap();
        assert!(
            sources.iter().any(|p| p.ends_with("App.java")),
            "should find source file"
        );
        assert!(
            !sources.iter().any(|p| p.ends_with("FooTest.java")),
            "test file should not appear in sources"
        );
    }

    // ── get_package_name with pom.xml having empty artifactId ──

    #[test]
    fn get_package_name_pom_empty_artifact_id() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId></artifactId></project>",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        let name = handler.get_package_name().unwrap();
        // Empty artifactId via extract_xml_tag returns None, falls through to dir name
        assert!(!name.is_empty());
    }

    // ── get_license: no pom, LICENSE file with classify_license match ──

    #[test]
    fn get_license_bsd_from_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("LICENSE"),
            "BSD 3-Clause License\n\nRedistribution and use",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_license(), Some("BSD-3-Clause".to_string()));
    }

    // ── get_artifact_id ──

    #[test]
    fn get_artifact_id_from_pom() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId>my-library</artifactId></project>",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_artifact_id().unwrap(), "my-library");
    }

    #[test]
    fn get_artifact_id_from_settings_gradle() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("build.gradle"),
            "group = 'com.example'\nversion = '1.0'\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("settings.gradle"),
            "rootProject.name = 'my-cool-lib'\n",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        // Should use rootProject.name, not group
        assert_eq!(handler.get_artifact_id().unwrap(), "my-cool-lib");
    }

    #[test]
    fn get_artifact_id_gradle_only_skips_group() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("build.gradle"),
            "group = 'com.example'\nversion = '1.0'\n",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        // No pom.xml, no settings.gradle, no archivesBaseName → falls back to dir name
        // Crucially, does NOT return "com.example" (the group namespace)
        let artifact = handler.get_artifact_id().unwrap();
        assert_ne!(
            artifact, "com.example",
            "get_artifact_id must not return group namespace"
        );
    }

    #[test]
    fn get_artifact_id_from_archives_base_name() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("build.gradle"),
            "group = 'com.example'\narchivesBaseName = 'my-artifact'\nversion = '1.0'\n",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_artifact_id().unwrap(), "my-artifact");
    }

    #[test]
    fn get_artifact_id_from_archives_name_set() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("build.gradle.kts"),
            "group = \"com.example\"\nbase.archivesName.set(\"my-kts-artifact\")\nversion = \"1.0\"\n",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_artifact_id().unwrap(), "my-kts-artifact");
    }

    // ── parse_gradle_archives_base_name ──

    #[test]
    fn parse_archives_base_name_basic() {
        let content = "archivesBaseName = 'my-lib'\n";
        assert_eq!(
            parse_gradle_archives_base_name(content),
            Some("my-lib".to_string())
        );
    }

    #[test]
    fn parse_archives_base_name_double_quotes() {
        let content = "archivesBaseName = \"my-lib\"\n";
        assert_eq!(
            parse_gradle_archives_base_name(content),
            Some("my-lib".to_string())
        );
    }

    #[test]
    fn parse_archives_name_set() {
        let content = "base.archivesName.set(\"my-lib\")\n";
        assert_eq!(
            parse_gradle_archives_base_name(content),
            Some("my-lib".to_string())
        );
    }

    #[test]
    fn parse_archives_name_equals() {
        let content = "archivesName = 'cool-project'\n";
        assert_eq!(
            parse_gradle_archives_base_name(content),
            Some("cool-project".to_string())
        );
    }

    #[test]
    fn parse_archives_base_name_not_present() {
        let content = "group = 'com.example'\nversion = '1.0'\n";
        assert_eq!(parse_gradle_archives_base_name(content), None);
    }

    #[test]
    fn parse_archives_base_name_skips_comments() {
        let content = "// archivesBaseName = 'old-name'\narchivesBaseName = 'real-name'\n";
        assert_eq!(
            parse_gradle_archives_base_name(content),
            Some("real-name".to_string())
        );
    }

    #[test]
    fn parse_archives_name_skips_block_comments() {
        let content = "/* archivesName = 'old' */\n* archivesName = 'star-prefix'\n";
        assert_eq!(parse_gradle_archives_base_name(content), None);
    }

    #[test]
    fn parse_archives_name_rejects_set_archives_name_prefix() {
        // Non-standard property — should not match our tightened pattern
        let content = "setArchivesNamePrefix = 'wrong'\n";
        assert_eq!(parse_gradle_archives_base_name(content), None);
    }

    // ── get_artifact_id edge cases ──

    #[test]
    fn get_artifact_id_pom_parent_suffix_stripped_to_empty() {
        // artifactId is exactly "-parent" → stripped to empty → falls through
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pom.xml"),
            "<project><artifactId>-parent</artifactId></project>",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        let id = handler.get_artifact_id().unwrap();
        // Should fall through to dir name, not return empty
        assert!(!id.is_empty());
    }

    #[test]
    fn get_artifact_id_settings_gradle_root_suffix_stripped_to_empty() {
        // rootProject.name = "-root" → stripped to empty → falls through
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("settings.gradle"),
            "rootProject.name = '-root'\n",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        let id = handler.get_artifact_id().unwrap();
        assert!(!id.is_empty());
    }

    #[test]
    fn get_artifact_id_settings_gradle_kts() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("settings.gradle.kts"),
            "rootProject.name = \"my-kts-artifact\"\n",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_artifact_id().unwrap(), "my-kts-artifact");
    }

    #[test]
    fn get_artifact_id_archives_base_name_kts() {
        // archivesBaseName in .kts file via the gradle_name loop second iteration
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("build.gradle.kts"),
            "archivesBaseName = \"my-kts-jar\"\n",
        )
        .unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert_eq!(handler.get_artifact_id().unwrap(), "my-kts-jar");
    }

    #[test]
    fn get_artifact_id_fallback_dir_name() {
        // No pom.xml, no settings.gradle, no build.gradle → dir name
        let tmp = TempDir::new().unwrap();
        let handler = JavaHandler::new(tmp.path());
        let id = handler.get_artifact_id().unwrap();
        assert!(!id.is_empty());
        assert_ne!(id, "unknown");
    }

    // ── parse_gradle_archives_base_name edge cases ──

    #[test]
    fn parse_archives_base_name_no_equals_sign() {
        // archivesBaseName without '=' should return None (no split_once match)
        let content = "archivesBaseName 'my-lib'\n";
        assert_eq!(parse_gradle_archives_base_name(content), None);
    }

    #[test]
    fn parse_archives_name_constant_not_quoted() {
        // archivesName = someVariable (not a quoted string) should return None
        let content = "archivesName = someVariable\n";
        assert_eq!(parse_gradle_archives_base_name(content), None);
    }

    #[test]
    fn parse_archives_name_set_with_variable() {
        // base.archivesName.set(someVariable) — no quotes, should return None
        let content = "base.archivesName.set(someVariable)\n";
        assert_eq!(parse_gradle_archives_base_name(content), None);
    }

    // --- Coverage: malformed parent in POM ---

    #[test]
    fn parse_pom_version_unclosed_parent() {
        // <parent> opened but never closed — should bail
        let pom = "<parent><groupId>com.foo</groupId><version>2.0</version>";
        assert_eq!(parse_pom_version(pom), None);
    }

    #[test]
    fn parse_pom_url_unclosed_parent() {
        // <parent> opened but never closed — should bail
        let pom = "<parent><url>https://example.com</url>";
        assert_eq!(parse_pom_url(pom), None);
    }

    // --- Coverage: extract_gradle_quoted with trailing comments ---

    #[test]
    fn extract_gradle_quoted_trailing_line_comment() {
        // Quoted value followed by // comment should be accepted
        assert_eq!(
            extract_gradle_quoted("'1.0.0' // some comment"),
            Some("1.0.0".to_string())
        );
    }

    #[test]
    fn extract_gradle_quoted_trailing_block_comment() {
        // Quoted value followed by /* comment should be accepted
        assert_eq!(
            extract_gradle_quoted("\"2.3.4\" /* block comment */"),
            Some("2.3.4".to_string())
        );
    }

    #[test]
    fn extract_gradle_quoted_trailing_code() {
        // Quoted value followed by code — should NOT be accepted
        assert_eq!(extract_gradle_quoted("'1.0.0' + suffix"), None);
    }

    #[test]
    fn extract_gradle_quoted_with_paren_and_comment() {
        // version("1.0") // Kotlin DSL with closing paren and comment
        assert_eq!(
            extract_gradle_quoted("\"1.0\") // version"),
            Some("1.0".to_string())
        );
    }

    #[test]
    fn extract_gradle_quoted_block_comment_then_code_rejected() {
        // `"1.0" /* old */ + suffix` — block comment followed by an
        // expression. Must be rejected so we don't treat a computed
        // version as a literal.
        assert_eq!(extract_gradle_quoted("\"1.0\" /* old */ + suffix"), None);
        assert_eq!(extract_gradle_quoted("'1.0' /* note */ more code"), None);
    }

    #[test]
    fn extract_gradle_quoted_unterminated_block_comment_rejected() {
        // `"1.0" /* unterminated` — `/*` with no closing `*/` must not be
        // mistaken for a valid trailing comment.
        assert_eq!(extract_gradle_quoted("\"1.0\" /* unterminated"), None);
    }
}
