//! Java ecosystem handler — discovers source/test/doc/example files, extracts
//! metadata (name, version, license, URLs) from pom.xml or build.gradle, and
//! detects the package coordinates. Used by the collector for Java projects.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

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

        if files.is_empty() {
            bail!("No Java source files found in {}", self.repo_path.display());
        }

        files.sort();
        info!("Found {} Java source files", files.len());
        Ok(files)
    }

    /// Find all Java test files.
    pub fn find_test_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_java_files(&self.repo_path, &mut files, 0, true)?;

        if files.is_empty() {
            bail!(
                "No tests found in {}. Tests are required for generating skills.",
                self.repo_path.display()
            );
        }

        info!("Found {} Java test files", files.len());
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
        info!("Found {} Java example files", files.len());
        Ok(files)
    }

    /// Find documentation files (README, docs/).
    pub fn find_docs(&self) -> Result<Vec<PathBuf>> {
        let mut docs = Vec::new();

        for name in &["README.md", "README.rst", "README.txt", "README"] {
            let path = self.repo_path.join(name);
            if path.exists() {
                docs.push(path);
                break;
            }
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

    /// Extract package name from pom.xml or build.gradle.
    pub fn get_package_name(&self) -> Result<String> {
        // Try pom.xml first
        let pom = self.repo_path.join("pom.xml");
        if pom.is_file() {
            if let Ok(content) = fs::read_to_string(&pom) {
                if let Some(name) = parse_pom_artifact_id(&content) {
                    return Ok(name);
                }
            }
        }

        // Try build.gradle / build.gradle.kts
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

        // Fallback: directory name
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

        Ok("latest".to_string())
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
                    let in_test_dir = Self::is_test_path(&path);
                    let is_test_file = name.ends_with("Test.java")
                        || name.ends_with("Tests.java")
                        || name.starts_with("Test");
                    let is_test = in_test_dir || is_test_file;
                    if tests_only == is_test {
                        files.push(path);
                    }
                }
            } else if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if Self::should_skip_dir(name) {
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

    /// Collect documentation files recursively.
    fn collect_docs_recursive(
        &self,
        dir: &Path,
        docs: &mut Vec<PathBuf>,
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
                    let lower = name.to_lowercase();
                    if lower.ends_with(".md")
                        || lower.ends_with(".rst")
                        || lower.ends_with(".txt")
                        || lower.ends_with(".adoc")
                    {
                        docs.push(path);
                    }
                }
            } else if ft.is_dir() {
                self.collect_docs_recursive(&path, docs, depth + 1)?;
            }
        }
        Ok(())
    }

    /// Check if a path is inside a test directory.
    fn is_test_path(path: &Path) -> bool {
        path.components().any(|c| {
            let s = c.as_os_str().to_str().unwrap_or("");
            s == "test" || s == "tests" || s == "src/test"
        })
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

/// Extract `<artifactId>` from pom.xml (top-level, not inside `<parent>` or `<dependency>`).
fn parse_pom_artifact_id(content: &str) -> Option<String> {
    // Find the top-level <artifactId> that's not inside <parent> or <dependencies>
    // Simple heuristic: take the first <artifactId> that appears before <dependencies>
    let deps_pos = content.find("<dependencies>");
    let parent_end = content.find("</parent>").map(|p| p + 9).unwrap_or(0);
    let search_region = if let Some(dp) = deps_pos {
        &content[parent_end..dp]
    } else {
        &content[parent_end..]
    };

    extract_xml_tag(search_region, "artifactId")
}

/// Extract `<version>` from pom.xml (top-level).
fn parse_pom_version(content: &str) -> Option<String> {
    let deps_pos = content.find("<dependencies>");
    let parent_end = content.find("</parent>").map(|p| p + 9).unwrap_or(0);
    let search_region = if let Some(dp) = deps_pos {
        &content[parent_end..dp]
    } else {
        &content[parent_end..]
    };

    extract_xml_tag(search_region, "version")
}

/// Extract license name from pom.xml `<licenses>` section.
fn parse_pom_license(content: &str) -> Option<String> {
    let start = content.find("<licenses>")?;
    let end = content[start..].find("</licenses>")?;
    let section = &content[start..start + end];
    extract_xml_tag(section, "name")
}

/// Extract `<url>` from pom.xml (top-level).
fn parse_pom_url(content: &str) -> Option<String> {
    // Only match top-level <url>, before <dependencies> or <build>
    let end_pos = content
        .find("<dependencies>")
        .or_else(|| content.find("<build>"))
        .unwrap_or(content.len());
    let parent_end = content.find("</parent>").map(|p| p + 9).unwrap_or(0);
    let search = &content[parent_end..end_pos];
    extract_xml_tag(search, "url")
}

/// Extract SCM URL from pom.xml `<scm>` section.
fn parse_pom_scm_url(content: &str) -> Option<String> {
    let start = content.find("<scm>")?;
    let end = content[start..].find("</scm>")?;
    let section = &content[start..start + end];
    extract_xml_tag(section, "url")
        .or_else(|| extract_xml_tag(section, "connection"))
        .map(|url| url.trim_start_matches("scm:git:").to_string())
}

/// Extract `group` from build.gradle.
fn parse_gradle_group(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        // group = 'com.example' or group = "com.example"
        if trimmed.starts_with("group") {
            let rhs = trimmed.split_once('=')?.1.trim();
            let name = rhs.trim_matches(|c: char| c == '\'' || c == '"' || c.is_whitespace());
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Extract `version` from build.gradle.
fn parse_gradle_version(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("version") && !trimmed.starts_with("versions") {
            if let Some((_, rhs)) = trimmed.split_once('=') {
                let v = rhs
                    .trim()
                    .trim_matches(|c: char| c == '\'' || c == '"' || c.is_whitespace());
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Simple XML tag value extraction — finds `<tag>value</tag>`.
fn extract_xml_tag(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = content.find(&open)?;
    let value_start = start + open.len();
    let end = content[value_start..].find(&close)?;
    let value = content[value_start..value_start + end].trim();
    if value.is_empty() {
        None
    } else {
        debug!("Extracted <{}> = {}", tag, value);
        Some(value.to_string())
    }
}

/// Parse dependencies from pom.xml `<dependency>` elements.
#[cfg_attr(not(test), allow(dead_code))]
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
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn parse_gradle_dependencies(content: &str) -> Vec<String> {
    let mut deps = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Match: implementation 'group:artifact:version' or api "group:artifact:version"
        for keyword in &["implementation", "api", "compile", "testImplementation"] {
            if let Some(stripped) = trimmed.strip_prefix(keyword) {
                let rest = stripped.trim();
                // Strip parentheses if present: implementation("...")
                let rest = rest.trim_start_matches('(').trim_end_matches(')').trim();
                let dep = rest
                    .trim_matches(|c: char| c == '\'' || c == '"')
                    .to_string();
                // Must look like a Maven coordinate (contains at least one colon)
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

    // ── Parsing unit tests ──

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
    fn no_test_files_errors() {
        let tmp = TempDir::new().unwrap();
        let handler = JavaHandler::new(tmp.path());
        assert!(handler.find_test_files().is_err());
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
        assert_eq!(handler.get_version().unwrap(), "latest");
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
}
