//! JS/TS ecosystem handler — discovers source/test/doc/example files, extracts
//! metadata from package.json.

use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

pub struct JsHandler {
    repo_path: PathBuf,
}

impl JsHandler {
    const MAX_DEPTH: usize = 20;

    pub fn new(repo_path: &Path) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
        }
    }

    // ── File discovery ──────────────────────────────────────────────────

    /// Find all JS/TS source files (excluding tests, excluded dirs, .d.ts, .min.js).
    pub fn find_source_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_js_files(&self.repo_path, &mut files, 0, false)?;

        if files.is_empty() {
            bail!(
                "No JS/TS source files found in {}",
                self.repo_path.display()
            );
        }

        files.sort_by_key(|p| self.file_priority(p));
        info!("Found {} JS/TS source files", files.len());
        Ok(files)
    }

    /// Find all JS/TS test files (*.test.js, *.spec.js, etc., and files in test dirs).
    pub fn find_test_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.collect_js_files(&self.repo_path, &mut files, 0, true)?;

        if files.is_empty() {
            bail!(
                "No tests found in {}. Tests are required for generating skills.",
                self.repo_path.display()
            );
        }

        info!("Found {} JS/TS test files", files.len());
        Ok(files)
    }

    /// Find documentation files (.md, excluding node_modules).
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

    /// Find example files (examples/, example/, demo/, demos/ dirs).
    pub fn find_examples(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for dir_name in &["examples", "example", "demo", "demos"] {
            let dir = self.repo_path.join(dir_name);
            if dir.is_dir() {
                self.collect_all_js_in_dir(&dir, &mut files, 0)?;
            }
        }

        files.sort();
        files.dedup();
        info!("Found {} JS/TS example files", files.len());
        Ok(files)
    }

    /// Find changelog file at repo root.
    pub fn find_changelog(&self) -> Option<PathBuf> {
        for name in &["CHANGELOG.md", "CHANGES.md", "HISTORY.md"] {
            let path = self.repo_path.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
        None
    }

    // ── Metadata extraction ────────────────────────────────────────────

    /// Extract package name from package.json `name` field.
    pub fn extract_package_name(&self) -> Result<String> {
        let pkg = self.read_package_json()?;
        pkg["name"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("No 'name' field in package.json"))
    }

    /// Extract version from package.json `version` field.
    pub fn extract_version(&self) -> Result<String> {
        let pkg = self.read_package_json()?;
        pkg["version"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("No 'version' field in package.json"))
    }

    /// Detect license from package.json `license` field, with fallback to LICENSE file.
    pub fn detect_license(&self) -> Option<String> {
        if let Ok(pkg) = self.read_package_json() {
            if let Some(license) = pkg["license"].as_str() {
                return Some(license.to_string());
            }
        }

        // Fallback: classify from LICENSE file content
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

    /// Extract project URLs from package.json (homepage, bugs.url, repository).
    pub fn extract_project_urls(&self) -> Vec<(String, String)> {
        let mut urls = Vec::new();
        let Ok(pkg) = self.read_package_json() else {
            return urls;
        };

        if let Some(homepage) = pkg["homepage"].as_str() {
            urls.push(("Homepage".into(), homepage.to_string()));
        }

        if let Some(bugs_url) = pkg["bugs"]["url"].as_str() {
            urls.push(("Issues".into(), bugs_url.to_string()));
        }

        // repository can be a string or an object with a `url` field
        if let Some(repo) = pkg["repository"].as_str() {
            urls.push(("Repository".into(), repo.to_string()));
        } else if let Some(repo_url) = pkg["repository"]["url"].as_str() {
            urls.push(("Repository".into(), repo_url.to_string()));
        }

        urls
    }

    /// Extract dependency names from package.json `dependencies` (not devDependencies).
    #[allow(dead_code)]
    pub fn extract_dependencies(&self) -> Result<Vec<String>> {
        let pkg = self.read_package_json()?;
        let deps = match pkg["dependencies"].as_object() {
            Some(obj) => obj.keys().cloned().collect(),
            None => Vec::new(),
        };
        Ok(deps)
    }

    // ── Private helpers ────────────────────────────────────────────────

    /// Read and parse package.json from repo root.
    fn read_package_json(&self) -> Result<serde_json::Value> {
        let path = self.repo_path.join("package.json");
        let content = fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Cannot read package.json: {e}"))?;
        let pkg: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Cannot parse package.json: {e}"))?;
        Ok(pkg)
    }

    /// Collect JS/TS source or test files depending on `test_mode`.
    fn collect_js_files(
        &self,
        dir: &Path,
        files: &mut Vec<PathBuf>,
        depth: usize,
        test_mode: bool,
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
                    if !is_js_file(name) {
                        continue;
                    }
                    if !test_mode {
                        // Source mode: exclude tests, .d.ts, .min.js
                        if name.ends_with(".d.ts") || name.ends_with(".min.js") {
                            continue;
                        }
                        if Self::is_test_file(&path) {
                            continue;
                        }
                        files.push(path);
                    } else {
                        // Test mode: only include test files
                        if Self::is_test_file(&path) {
                            files.push(path);
                        }
                    }
                }
            } else if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if Self::is_excluded_dir(name) {
                        continue;
                    }
                    self.collect_js_files(&path, files, depth + 1, test_mode)?;
                }
            }
        }

        Ok(())
    }

    /// Collect all JS/TS files in a directory (for examples).
    fn collect_all_js_in_dir(
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
                    if is_js_file(name) {
                        files.push(path);
                    }
                }
            } else if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !Self::is_excluded_dir(name) {
                        self.collect_all_js_in_dir(&path, files, depth + 1)?;
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
        if depth > Self::MAX_DEPTH {
            return Ok(());
        }

        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "node_modules" {
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

    /// Check if a file matches JS/TS test patterns.
    fn is_test_file(path: &Path) -> bool {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Name-based patterns: *.test.js, *.spec.js, *.test.ts, *.spec.ts, etc.
        if name.contains(".test.") || name.contains(".spec.") {
            return true;
        }

        // Directory-based patterns: __tests__/, test/, tests/
        path.components().any(|c| {
            let s = c.as_os_str().to_str().unwrap_or("");
            matches!(s, "__tests__" | "test" | "tests")
        })
    }

    /// Check if a directory name should be excluded from traversal.
    fn is_excluded_dir(name: &str) -> bool {
        name.starts_with('.') || matches!(name, "node_modules" | "dist" | "build" | "coverage")
    }

    /// File priority for sorting: index/main files first, then src/lib, then rest.
    fn file_priority(&self, path: &Path) -> i32 {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let stem = Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        // Priority 0: index.js, index.ts, main.js, main.ts (any extension combo)
        if matches!(stem, "index" | "main") {
            return 0;
        }

        // Priority 1: files in src/ or lib/ directories
        let relative = path.strip_prefix(&self.repo_path).unwrap_or(path);
        if relative.components().any(|c| {
            let s = c.as_os_str().to_str().unwrap_or("");
            matches!(s, "src" | "lib")
        }) {
            return 1;
        }

        // Priority 2: everything else
        2
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Check if a filename has a JS/TS extension.
fn is_js_file(name: &str) -> bool {
    name.ends_with(".js")
        || name.ends_with(".ts")
        || name.ends_with(".jsx")
        || name.ends_with(".tsx")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── File discovery tests ──────────────────────────────────────────

    #[test]
    fn test_find_source_files_basic() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("index.js"), "module.exports = {};\n").unwrap();
        fs::write(root.join("utils.ts"), "export function add() {}\n").unwrap();
        fs::write(root.join("app.jsx"), "export default function App() {}\n").unwrap();
        fs::write(root.join("page.tsx"), "export default function Page() {}\n").unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_source_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"index.js"), "should find .js files");
        assert!(names.contains(&"utils.ts"), "should find .ts files");
        assert!(names.contains(&"app.jsx"), "should find .jsx files");
        assert!(names.contains(&"page.tsx"), "should find .tsx files");
    }

    #[test]
    fn test_find_source_files_excludes_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("index.js"), "module.exports = {};\n").unwrap();
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::write(
            root.join("node_modules").join("dep.js"),
            "module.exports = {};\n",
        )
        .unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_source_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(!names.contains(&"dep.js"), "should exclude node_modules/");
    }

    #[test]
    fn test_find_source_files_excludes_min_js() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("index.js"), "module.exports = {};\n").unwrap();
        fs::write(root.join("bundle.min.js"), "!function(){}\n").unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_source_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(!names.contains(&"bundle.min.js"), "should exclude .min.js");
    }

    #[test]
    fn test_find_source_files_excludes_dts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("index.ts"), "export function hello() {}\n").unwrap();
        fs::write(root.join("index.d.ts"), "declare function hello(): void;\n").unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_source_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(!names.contains(&"index.d.ts"), "should exclude .d.ts");
    }

    #[test]
    fn test_find_test_files_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("index.js"), "module.exports = {};\n").unwrap();
        fs::write(root.join("index.test.js"), "test('works', () => {});\n").unwrap();
        fs::write(root.join("app.spec.ts"), "describe('app', () => {});\n").unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_test_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(names.contains(&"index.test.js"), "should find .test.js");
        assert!(names.contains(&"app.spec.ts"), "should find .spec.ts");
        assert!(!names.contains(&"index.js"), "should exclude source files");
    }

    #[test]
    fn test_find_test_files_by_directory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("__tests__")).unwrap();
        fs::write(
            root.join("__tests__").join("util.js"),
            "test('util', () => {});\n",
        )
        .unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_test_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert!(
            names.contains(&"util.js"),
            "should find files in __tests__/"
        );
    }

    // ── Metadata extraction tests ─────────────────────────────────────

    #[test]
    fn test_extract_package_name_simple() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name": "express"}"#).unwrap();

        let handler = JsHandler::new(dir.path());
        assert_eq!(handler.extract_package_name().unwrap(), "express");
    }

    #[test]
    fn test_extract_package_name_scoped() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "@types/node"}"#,
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        assert_eq!(handler.extract_package_name().unwrap(), "@types/node");
    }

    #[test]
    fn test_extract_version() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "foo", "version": "1.2.3"}"#,
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        assert_eq!(handler.extract_version().unwrap(), "1.2.3");
    }

    #[test]
    fn test_detect_license_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "foo", "license": "MIT"}"#,
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        assert_eq!(handler.detect_license().unwrap(), "MIT");
    }

    #[test]
    fn test_detect_license_from_file() {
        let dir = tempfile::tempdir().unwrap();
        // No package.json or one without license field
        fs::write(dir.path().join("package.json"), r#"{"name": "foo"}"#).unwrap();
        fs::write(
            dir.path().join("LICENSE"),
            "MIT License\n\nCopyright (c) 2024 Test\n\nPermission is hereby granted, free of charge...\n",
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        assert_eq!(handler.detect_license().unwrap(), "MIT");
    }

    #[test]
    fn test_extract_project_urls_homepage() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "foo", "homepage": "https://foo.dev"}"#,
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        let urls = handler.extract_project_urls();
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Homepage" && v == "https://foo.dev"));
    }

    #[test]
    fn test_extract_project_urls_repository_string() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "foo", "repository": "https://github.com/org/foo"}"#,
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        let urls = handler.extract_project_urls();
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Repository" && v == "https://github.com/org/foo"));
    }

    #[test]
    fn test_extract_project_urls_repository_object() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "foo", "repository": {"type": "git", "url": "https://github.com/org/foo.git"}}"#,
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        let urls = handler.extract_project_urls();
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Repository" && v == "https://github.com/org/foo.git"));
    }

    #[test]
    fn test_extract_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "foo", "dependencies": {"express": "^4.0", "lodash": "^4.17"}, "devDependencies": {"jest": "^29"}}"#,
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        let deps = handler.extract_dependencies().unwrap();
        assert!(deps.contains(&"express".to_string()));
        assert!(deps.contains(&"lodash".to_string()));
        assert!(
            !deps.contains(&"jest".to_string()),
            "should exclude devDependencies"
        );
    }

    #[test]
    fn test_no_package_json_errors() {
        let dir = tempfile::tempdir().unwrap();
        let handler = JsHandler::new(dir.path());
        assert!(handler.extract_package_name().is_err());
        assert!(handler.extract_version().is_err());
    }

    #[test]
    fn test_file_priority_index_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("utils.ts"), "export function add() {}\n").unwrap();
        fs::write(root.join("index.ts"), "export * from './utils';\n").unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_source_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();

        assert_eq!(names[0], "index.ts", "index.ts should be sorted first");
    }

    #[test]
    fn test_find_source_files_empty_dir_errors() {
        let dir = tempfile::tempdir().unwrap();
        let handler = JsHandler::new(dir.path());
        assert!(handler.find_source_files().is_err());
    }

    #[test]
    fn test_find_changelog() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("CHANGELOG.md"),
            "# Changelog\n\n## 1.0.0\n- Initial release\n",
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        let changelog = handler.find_changelog();
        assert!(changelog.is_some());
        assert_eq!(
            changelog.unwrap().file_name().unwrap().to_str().unwrap(),
            "CHANGELOG.md"
        );
    }

    #[test]
    fn test_extract_dependencies_empty() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "foo", "version": "1.0.0"}"#,
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        let deps = handler.extract_dependencies().unwrap();
        assert!(deps.is_empty());
    }

    // ── Coverage gap tests ────────────────────────────────────────────

    #[test]
    fn test_find_test_files_empty_fails() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("index.js"), "module.exports = {};\n").unwrap();

        let handler = JsHandler::new(dir.path());
        let result = handler.find_test_files();
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("No tests found"),
            "should bail with 'No tests found'"
        );
    }

    #[test]
    fn test_find_docs_with_readme_and_docs_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("README.md"), "# Hello\n").unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs").join("guide.md"), "# Guide\n").unwrap();

        let handler = JsHandler::new(root);
        let docs = handler.find_docs().unwrap();
        assert!(docs.len() >= 2, "should find README + docs/guide.md");
    }

    #[test]
    fn test_find_docs_recursive_skips_dot_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("docs/.hidden")).unwrap();
        fs::write(root.join("docs/.hidden/secret.md"), "secret\n").unwrap();
        fs::write(root.join("docs/public.md"), "public\n").unwrap();

        let handler = JsHandler::new(root);
        let docs = handler.find_docs().unwrap();
        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"public.md"));
        assert!(!names.contains(&"secret.md"), "should skip .hidden dir");
    }

    #[test]
    fn test_find_examples_collects_from_examples_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("examples/nested")).unwrap();
        fs::write(root.join("examples/basic.js"), "console.log('hi');\n").unwrap();
        fs::write(root.join("examples/nested/advanced.ts"), "export {};\n").unwrap();
        // Also put a non-JS file that should be ignored
        fs::write(root.join("examples/readme.txt"), "not JS\n").unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_examples().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"basic.js"));
        assert!(names.contains(&"advanced.ts"));
        assert!(!names.contains(&"readme.txt"));
    }

    #[test]
    fn test_find_examples_skips_excluded_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("examples/node_modules")).unwrap();
        fs::write(root.join("examples/demo.js"), "console.log('demo');\n").unwrap();
        fs::write(
            root.join("examples/node_modules/dep.js"),
            "module.exports = {};\n",
        )
        .unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_examples().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"demo.js"));
        assert!(!names.contains(&"dep.js"));
    }

    #[test]
    fn test_detect_license_fallback_to_first_line() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name": "foo"}"#).unwrap();
        fs::write(
            dir.path().join("LICENSE"),
            "Custom License v1.0\n\nDo whatever you want.\n",
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        let license = handler.detect_license();
        assert_eq!(license.unwrap(), "Custom License v1.0");
    }

    #[test]
    fn test_detect_license_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name": "foo"}"#).unwrap();

        let handler = JsHandler::new(dir.path());
        assert!(handler.detect_license().is_none());
    }

    #[test]
    fn test_extract_project_urls_bugs_url() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "foo", "bugs": {"url": "https://github.com/org/foo/issues"}}"#,
        )
        .unwrap();

        let handler = JsHandler::new(dir.path());
        let urls = handler.extract_project_urls();
        assert!(urls
            .iter()
            .any(|(k, v)| k == "Issues" && v.contains("issues")));
    }

    #[test]
    fn test_extract_project_urls_no_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let handler = JsHandler::new(dir.path());
        let urls = handler.extract_project_urls();
        assert!(urls.is_empty());
    }

    #[test]
    fn test_collect_js_files_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src/utils")).unwrap();
        fs::write(root.join("src/index.ts"), "export {};\n").unwrap();
        fs::write(root.join("src/utils/helpers.ts"), "export {};\n").unwrap();

        let handler = JsHandler::new(root);
        let files = handler.find_source_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"index.ts"));
        assert!(names.contains(&"helpers.ts"));
    }

    #[test]
    fn test_collect_js_files_excludes_build_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("index.js"), "module.exports = {};\n").unwrap();
        for d in &["dist", "build", "coverage", ".next"] {
            fs::create_dir_all(root.join(d)).unwrap();
            fs::write(root.join(d).join("output.js"), "compiled\n").unwrap();
        }

        let handler = JsHandler::new(root);
        let files = handler.find_source_files().unwrap();
        let names: Vec<&str> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"index.js"));
        assert!(!names.contains(&"output.js"), "should exclude build dirs");
    }

    #[test]
    fn test_collect_docs_recursive_nested() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("docs/api")).unwrap();
        fs::write(root.join("docs/intro.md"), "# Intro\n").unwrap();
        fs::write(root.join("docs/api/reference.md"), "# API\n").unwrap();
        fs::write(root.join("docs/api/notes.rst"), "Notes\n").unwrap();

        let handler = JsHandler::new(root);
        let docs = handler.find_docs().unwrap();
        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"intro.md"));
        assert!(names.contains(&"reference.md"));
        assert!(names.contains(&"notes.rst"));
    }

    #[test]
    fn test_collect_docs_recursive_skips_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("docs/node_modules")).unwrap();
        fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
        fs::write(root.join("docs/node_modules/dep.md"), "dep\n").unwrap();

        let handler = JsHandler::new(root);
        let docs = handler.find_docs().unwrap();
        let names: Vec<&str> = docs
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"guide.md"));
        assert!(!names.contains(&"dep.md"));
    }
}
