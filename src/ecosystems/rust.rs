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
            // Strip inline comments on section headers: `[package] # metadata`
            let header = trimmed
                .split_once('#')
                .map_or(trimmed, |(before, _)| before.trim());
            in_package = header == "[package]";
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

    /// Check if a Cargo.toml field value is a workspace-inherited placeholder
    /// like `{ workspace = true }`. More precise than substring matching on
    /// "workspace" which could false-positive on URLs containing the word.
    fn is_workspace_placeholder(value: &str) -> bool {
        let t = value.trim();
        t.starts_with('{') && t.contains("workspace") && t.contains("true")
    }

    /// Check if a Cargo.toml `[package]` section contains a dotted workspace
    /// key like `version.workspace = true` (as opposed to the inline-table
    /// form `version = { workspace = true }`).
    fn has_dotted_workspace_key(content: &str, field: &str) -> bool {
        let dotted = format!("{field}.workspace");
        let mut in_package = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                let header = trimmed
                    .split_once('#')
                    .map_or(trimmed, |(before, _)| before.trim());
                in_package = header == "[package]";
                continue;
            }
            if in_package {
                if let Some(eq_pos) = trimmed.find('=') {
                    let lhs = trimmed[..eq_pos].trim();
                    let rhs = trimmed[eq_pos + 1..].trim();
                    let rhs_clean = rhs.split('#').next().unwrap_or("").trim();
                    if lhs == dotted && rhs_clean == "true" {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Walk up from `self.repo_path` looking for the nearest workspace-root Cargo.toml.
    /// Per Cargo semantics, `version.workspace = true` resolves from the NEAREST
    /// ancestor with `[workspace]` — not higher. Stops at .git or 10 levels.
    fn version_from_workspace_root(&self) -> Option<String> {
        let mut dir = self.repo_path.clone();
        for _ in 0..10 {
            if !dir.pop() {
                break;
            }
            let candidate = dir.join("Cargo.toml");
            if let Ok(content) = fs::read_to_string(&candidate) {
                if let Ok(parsed) = content.parse::<toml::Table>() {
                    // Found a [workspace] section — this is THE workspace root per Cargo.
                    // Extract version if present, but either way stop walking.
                    if parsed.contains_key("workspace") {
                        return parsed
                            .get("workspace")
                            .and_then(|w| w.get("package"))
                            .and_then(|p| p.get("version"))
                            .and_then(|v| v.as_str())
                            .filter(|ver| ver.contains('.'))
                            .map(|ver| {
                                debug!("resolved workspace version {ver} from workspace root");
                                ver.to_string()
                            });
                    }
                }
            }
            // Stop at repo root — workspace Cargo.toml won't be above it
            if dir.join(".git").exists() {
                break;
            }
        }
        None
    }

    // ── File discovery ──────────────────────────────────────────────────

    /// Find all Rust source files (excluding tests, target, benches, examples).
    /// Prefers `src/` when it exists to avoid picking up root-level .rs files
    /// (build scripts, workspace shims, etc.).
    pub fn find_source_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let src_dir = self.repo_path.join("src");
        let start = if src_dir.is_dir() {
            &src_dir
        } else {
            &self.repo_path
        };
        self.collect_rs_files(start, &mut files, 0)?;

        files.sort_by_key(|p| self.file_priority(p));
        let files = crate::util::filter_within_boundary(files, &self.repo_path);

        if files.is_empty() {
            bail!("No Rust source files found in {}", self.repo_path.display());
        }
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
        let files = crate::util::filter_within_boundary(files, &self.repo_path);

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
        let files = crate::util::filter_within_boundary(files, &self.repo_path);
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

    /// Extract package name from Cargo.toml `[package]` section.
    /// For workspace roots (no `[package]`), finds the first member crate's name.
    pub fn get_package_name(&self) -> Result<String> {
        let cargo_toml = self.repo_path.join("Cargo.toml");
        let content = fs::read_to_string(&cargo_toml)
            .map_err(|e| anyhow::anyhow!("Cannot read Cargo.toml: {e}"))?;

        // Try direct [package].name first
        if let Some(name) = cargo_toml_field(&content, "name") {
            return Ok(name);
        }

        // Workspace root — find the first member with a [package] section
        if let Ok(parsed) = content.parse::<toml::Table>() {
            if let Some(workspace) = parsed.get("workspace").and_then(|v| v.as_table()) {
                if let Some(members) = workspace.get("members").and_then(|v| v.as_array()) {
                    for member in members {
                        if let Some(member_path) = member.as_str() {
                            // Reject paths that escape the repo root
                            let member_rel = Path::new(member_path);
                            if member_rel.is_absolute()
                                || member_rel
                                    .components()
                                    .any(|c| c == std::path::Component::ParentDir)
                            {
                                continue;
                            }
                            // Expand glob patterns like "crates/*"
                            let resolved_paths =
                                expand_workspace_member(&self.repo_path, member_path);
                            for resolved in &resolved_paths {
                                let member_cargo = resolved.join("Cargo.toml");
                                if let Ok(member_content) = fs::read_to_string(&member_cargo) {
                                    if let Some(name) = cargo_toml_field(&member_content, "name") {
                                        tracing::info!(
                                            "Workspace root — using first member crate: {}",
                                            name
                                        );
                                        return Ok(name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        anyhow::bail!("No package name found in Cargo.toml (not a package or workspace)")
    }

    /// Extract version from Cargo.toml, falling back to workspace, git tags, then "latest".
    pub fn get_version(&self) -> Result<String> {
        // Strategy 1: Cargo.toml [package] version (most authoritative for Rust)
        let cargo_toml = self.repo_path.join("Cargo.toml");
        let mut has_workspace_version = false;
        if let Ok(content) = fs::read_to_string(&cargo_toml) {
            if let Some(version) = cargo_toml_field(&content, "version") {
                // Reject workspace-inherited versions like { workspace = true }
                if !Self::is_workspace_placeholder(&version) && version.contains('.') {
                    return Ok(version);
                }
                if Self::is_workspace_placeholder(&version) {
                    has_workspace_version = true;
                }
            }

            // Strategy 1b: workspace.package.version (for workspace root Cargo.toml)
            if let Ok(parsed) = content.parse::<toml::Table>() {
                if let Some(ver) = parsed
                    .get("workspace")
                    .and_then(|w| w.get("package"))
                    .and_then(|p| p.get("version"))
                    .and_then(|v| v.as_str())
                {
                    if ver.contains('.') {
                        return Ok(ver.to_string());
                    }
                }
            }

            // Detect `version.workspace = true` (dotted-key form)
            if !has_workspace_version && Self::has_dotted_workspace_key(&content, "version") {
                has_workspace_version = true;
            }
        }

        // Strategy 1c: walk up to workspace root for inherited version
        if has_workspace_version {
            if let Some(ver) = self.version_from_workspace_root() {
                return Ok(ver);
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
        let cargo_toml = self.repo_path.join("Cargo.toml");
        let cargo_content = fs::read_to_string(&cargo_toml).ok();

        // Strategy 1: Cargo.toml license field
        if let Some(ref content) = cargo_content {
            if let Some(license) = cargo_toml_field(content, "license") {
                if !Self::is_workspace_placeholder(&license) {
                    return Some(license);
                }
            }
        }

        // Strategy 2: Cargo.toml `license-file` field
        if let Some(ref content) = cargo_content {
            if let Some(license_file) = cargo_toml_field(content, "license-file") {
                // Guard against path traversal (absolute paths or ".." segments)
                let lf = std::path::Path::new(&license_file);
                if lf.is_absolute()
                    || lf
                        .components()
                        .any(|c| c == std::path::Component::ParentDir)
                {
                    return None;
                }
                let path = self.repo_path.join(&license_file);
                if let Ok(file_content) = fs::read_to_string(&path) {
                    if let Some(license) = classify_license(&file_content) {
                        return Some(license);
                    }
                    // Fallback: first non-empty line
                    return file_content
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .map(String::from);
                }
            }
        }

        // Strategy 3: LICENSE file classification
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

        // Helper: skip workspace-inherited placeholders like `{ workspace = true }`
        let explicit_url = |field: &str| -> Option<String> {
            cargo_toml_field(&content, field).filter(|v| !Self::is_workspace_placeholder(v))
        };

        if let Some(repo) = explicit_url("repository") {
            urls.push(("Repository".into(), repo));
        }
        if let Some(homepage) = explicit_url("homepage") {
            urls.push(("Homepage".into(), homepage));
        }
        if let Some(docs) = explicit_url("documentation") {
            urls.push(("Documentation".into(), docs));
        }

        // If no documentation URL but we have a crate name, add docs.rs
        if !urls.iter().any(|(k, _)| k == "Documentation") {
            if let Some(name) = explicit_url("name") {
                urls.push(("Documentation".into(), format!("https://docs.rs/{name}")));
            }
        }

        urls
    }

    /// Extract structured dependencies from Cargo.toml [dependencies] section.
    /// Preserves raw TOML value specs losslessly. Drops path deps (non-portable).
    /// Resolves `workspace = true` against root [workspace.dependencies] if available.
    pub fn get_dependencies(&self) -> Vec<crate::pipeline::collector::StructuredDep> {
        use crate::pipeline::collector::{DepSource, StructuredDep};

        let cargo_toml = self.repo_path.join("Cargo.toml");
        let Ok(content) = fs::read_to_string(&cargo_toml) else {
            return Vec::new();
        };

        let Ok(parsed) = content.parse::<toml::Table>() else {
            debug!("Failed to parse Cargo.toml as TOML");
            return Vec::new();
        };

        // Try to load workspace deps for resolving { workspace = true }
        let workspace_deps = self.load_workspace_deps();

        let Some(deps_table) = parsed.get("dependencies").and_then(|v| v.as_table()) else {
            return Vec::new();
        };

        let mut result = Vec::new();
        for (name, value) in deps_table {
            let raw = match value {
                toml::Value::String(s) => format!("\"{}\"", s),
                other => other.to_string(),
            };

            // Drop ALL path deps — non-portable for SKILL.md and temp Cargo projects.
            // The target crate's path dep is handled by local-install in the executor.
            if let Some(tbl) = value.as_table() {
                if tbl.contains_key("path") {
                    debug!("Dropping path dep: {}", name);
                    continue;
                }
            }

            // Resolve { workspace = true } — check structurally, not by substring
            let resolved_raw = if let Some(child_tbl) = value.as_table() {
                if child_tbl.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
                    if let Some(ws_spec) = workspace_deps.get(name.as_str()) {
                        debug!("Resolved workspace dep {}: {}", name, ws_spec);
                        // Merge child overrides (features, default-features, optional)
                        // into the workspace-resolved spec.
                        let child_overrides: Vec<(&str, &toml::Value)> = child_tbl
                            .iter()
                            .filter(|(k, _)| *k != "workspace")
                            .map(|(k, v)| (k.as_str(), v))
                            .collect();
                        if child_overrides.is_empty() {
                            ws_spec.clone()
                        } else if let Ok(ws_tbl) = ws_spec.parse::<toml::Value>() {
                            // Wrap simple "version" string as { version = "..." } table
                            // so child overrides can be merged in.
                            let mut tbl = match ws_tbl {
                                toml::Value::String(ref s) => {
                                    let mut t = toml::map::Map::new();
                                    t.insert("version".to_string(), toml::Value::String(s.clone()));
                                    t
                                }
                                toml::Value::Table(t) => t,
                                _ => {
                                    // Unexpected shape (int, bool, etc.) — skip merge
                                    debug!("Workspace dep {} has non-table shape, skipping override merge", name);
                                    let mut t = toml::map::Map::new();
                                    t.insert("version".to_string(), ws_tbl);
                                    t
                                }
                            };
                            for (k, v) in child_overrides {
                                if k == "features" {
                                    // Cargo: inherited features are additive — union, don't replace
                                    if let (Some(ws_arr), Some(child_arr)) = (
                                        tbl.get("features").and_then(|f| f.as_array()).cloned(),
                                        v.as_array(),
                                    ) {
                                        let mut merged = ws_arr;
                                        for f in child_arr {
                                            if !merged.contains(f) {
                                                merged.push(f.clone());
                                            }
                                        }
                                        tbl.insert(k.to_string(), toml::Value::Array(merged));
                                    } else {
                                        tbl.insert(k.to_string(), v.clone());
                                    }
                                } else {
                                    tbl.insert(k.to_string(), v.clone());
                                }
                            }
                            toml::Value::Table(tbl).to_string()
                        } else {
                            // ws_spec didn't parse — use it as-is
                            ws_spec.clone()
                        }
                    } else {
                        debug!("Unresolvable workspace dep {}, degrading to wildcard", name);
                        "\"*\"".to_string()
                    }
                } else {
                    raw
                }
            } else {
                raw
            };

            // Drop path deps that survived workspace resolution
            // (workspace entry itself could be a path dep)
            let is_path_dep = resolved_raw
                .parse::<toml::Value>()
                .ok()
                .and_then(|v| v.as_table().map(|t| t.contains_key("path")))
                .unwrap_or(false);
            if is_path_dep {
                debug!("Dropping resolved path dep: {}", name);
                continue;
            }

            result.push(StructuredDep {
                name: name.clone(),
                raw_spec: Some(resolved_raw),
                source: DepSource::Manifest,
            });
        }

        debug!("Extracted {} dependencies from Cargo.toml", result.len());
        result
    }

    /// Load [workspace.dependencies] from the root Cargo.toml for resolving
    /// `{ workspace = true }` entries. Returns name → raw TOML spec pairs.
    fn load_workspace_deps(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();

        // Walk up to find the workspace root (has [workspace] section).
        // No artificial depth limit — stop when we hit the filesystem root.
        let mut dir = self.repo_path.as_path();
        loop {
            let cargo_path = dir.join("Cargo.toml");
            if let Ok(content) = fs::read_to_string(&cargo_path) {
                if let Ok(parsed) = content.parse::<toml::Table>() {
                    if let Some(ws) = parsed.get("workspace").and_then(|v| v.as_table()) {
                        // Stop here — this is the workspace root regardless of
                        // whether [workspace.dependencies] exists.
                        if let Some(ws_deps) = ws.get("dependencies").and_then(|v| v.as_table()) {
                            for (name, value) in ws_deps {
                                let raw = match value {
                                    toml::Value::String(s) => format!("\"{}\"", s),
                                    other => other.to_string(),
                                };
                                map.insert(name.clone(), raw);
                            }
                        }
                        return map;
                    }
                }
            }
            match dir.parent() {
                Some(p) if p != dir => dir = p,
                _ => break,
            }
        }
        map
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

    /// Detect indicators that this crate has native/C dependencies.
    /// Returns a list of human-readable reasons (empty = no native deps detected).
    /// For workspace roots, also scans member crates.
    pub fn detect_native_deps(&self) -> Vec<String> {
        let mut indicators = Vec::new();
        let cargo_toml = self.repo_path.join("Cargo.toml");
        if let Ok(content) = fs::read_to_string(&cargo_toml) {
            if let Ok(parsed) = content.parse::<toml::Table>() {
                // Check this crate's own deps
                Self::check_native_deps_in_manifest(&parsed, &mut indicators);
                // Workspace root — also check member crates
                if parsed.contains_key("workspace") {
                    if let Some(members) = parsed
                        .get("workspace")
                        .and_then(|w| w.get("members"))
                        .and_then(|m| m.as_array())
                    {
                        for member in members {
                            if let Some(member_path) = member.as_str() {
                                // Reject paths that escape the repo root
                                let member_rel = Path::new(member_path);
                                if member_rel.is_absolute()
                                    || member_rel
                                        .components()
                                        .any(|c| c == std::path::Component::ParentDir)
                                {
                                    continue;
                                }
                                for resolved in
                                    expand_workspace_member(&self.repo_path, member_path)
                                {
                                    if let Ok(member_content) =
                                        fs::read_to_string(resolved.join("Cargo.toml"))
                                    {
                                        if let Ok(member_parsed) =
                                            member_content.parse::<toml::Table>()
                                        {
                                            Self::check_native_deps_in_manifest(
                                                &member_parsed,
                                                &mut indicators,
                                            );
                                        }
                                    }
                                    if resolved.join("build.rs").exists() {
                                        let name = resolved
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy();
                                        indicators.push(format!("build.rs in member {name}"));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if self.repo_path.join("build.rs").exists() {
            indicators.push("build.rs present".to_string());
        }
        indicators
    }

    /// Check a single Cargo.toml manifest for native dep indicators.
    fn check_native_deps_in_manifest(parsed: &toml::Table, indicators: &mut Vec<String>) {
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(deps) = parsed.get(section).and_then(|v| v.as_table()) {
                for key in deps.keys() {
                    if key.ends_with("-sys") {
                        indicators.push(format!("{key} crate ({section})"));
                    }
                }
            }
        }
        if let Some(links) = parsed
            .get("package")
            .and_then(|p| p.get("links"))
            .and_then(|l| l.as_str())
        {
            indicators.push(format!("links = \"{links}\""));
        }
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Expand a workspace member path, handling full glob patterns like "crates/*-macros".
/// Returns a list of resolved directory paths. For literal paths, returns a single entry.
fn expand_workspace_member(repo_root: &Path, member: &str) -> Vec<std::path::PathBuf> {
    if member.contains('*') || member.contains('?') || member.contains('[') {
        let pattern = repo_root.join(member);
        let pattern_str = pattern.to_string_lossy();
        match glob::glob(&pattern_str) {
            Ok(entries) => {
                let mut paths: Vec<_> = entries
                    .filter_map(|r| r.ok())
                    .filter(|p| p.is_dir())
                    .collect();
                paths.sort();
                paths
            }
            Err(e) => {
                tracing::warn!("Invalid glob pattern '{}': {}", member, e);
                Vec::new()
            }
        }
    } else {
        vec![repo_root.join(member)]
    }
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
    fn get_package_name_from_workspace_root() {
        let dir = tempfile::tempdir().unwrap();
        // Workspace root with no [package]
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/mylib\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        // Member crate with [package]
        let member_dir = dir.path().join("crates/mylib");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_package_name().unwrap(), "mylib");
    }

    #[test]
    fn get_version_from_workspace_package() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\"]\n\n[workspace.package]\nversion = \"2.0.0\"\n",
        )
        .unwrap();
        let member_dir = dir.path().join("crates/a");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            "[package]\nname = \"a\"\nversion.workspace = true\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "2.0.0");
    }

    #[test]
    fn get_version_member_crate_resolves_workspace_root() {
        // Handler points at the MEMBER crate, not the workspace root.
        // version.workspace = true should walk up to find the root version.
        // Tracing subscriber ensures debug!() format args execute for coverage.
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\"]\n\n[workspace.package]\nversion = \"3.1.0\"\n",
        )
        .unwrap();
        let member_dir = dir.path().join("crates/a");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            "[package]\nname = \"a\"\nversion.workspace = true\n",
        )
        .unwrap();
        // Point handler at the member, not the root
        let handler = RustHandler::new(&member_dir);
        assert_eq!(handler.get_version().unwrap(), "3.1.0");
    }

    #[test]
    fn get_version_member_crate_inline_table_resolves_workspace_root() {
        // Same as above but using `version = { workspace = true }` form.
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/b\"]\n\n[workspace.package]\nversion = \"4.2.0\"\n",
        )
        .unwrap();
        let member_dir = dir.path().join("crates/b");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            "[package]\nname = \"b\"\nversion = { workspace = true }\n",
        )
        .unwrap();
        let handler = RustHandler::new(&member_dir);
        assert_eq!(handler.get_version().unwrap(), "4.2.0");
    }

    #[test]
    fn get_version_member_crate_no_workspace_root_falls_back() {
        // Member crate with version.workspace = true but no workspace root above.
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"orphan\"\nversion.workspace = true\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        // No workspace root to resolve — falls back to "latest"
        assert_eq!(handler.get_version().unwrap(), "latest");
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
    fn is_workspace_placeholder_detection() {
        // True positives: workspace-inherited values
        assert!(RustHandler::is_workspace_placeholder(
            "{ workspace = true }"
        ));
        assert!(RustHandler::is_workspace_placeholder("{workspace = true}"));
        assert!(RustHandler::is_workspace_placeholder(
            "  { workspace = true }  "
        ));

        // True negatives: real values that happen to contain "workspace"
        assert!(!RustHandler::is_workspace_placeholder(
            "https://github.com/my-workspace/repo"
        ));
        assert!(!RustHandler::is_workspace_placeholder("MIT"));
        assert!(!RustHandler::is_workspace_placeholder("1.2.3"));
        assert!(!RustHandler::is_workspace_placeholder(""));
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
    fn cargo_toml_field_handles_commented_section_header() {
        let content = "[package] # metadata\nname = \"test\"\nversion = \"1.0.0\"\n";
        assert_eq!(
            cargo_toml_field(content, "name"),
            Some("test".to_string()),
            "[package] with inline comment should still match"
        );
    }

    #[test]
    fn get_project_urls_skips_workspace_inherited() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nrepository = { workspace = true }\nhomepage = { workspace = true }\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        let urls = handler.get_project_urls();
        assert!(
            !urls.iter().any(|(k, _)| k == "Repository"),
            "workspace-inherited repository should be skipped: {:?}",
            urls
        );
        assert!(
            !urls.iter().any(|(k, _)| k == "Homepage"),
            "workspace-inherited homepage should be skipped: {:?}",
            urls
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

    #[test]
    fn find_test_files_empty_repo_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn x() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_test_files().unwrap();
        assert!(files.is_empty(), "no test files in this repo");
    }

    #[test]
    fn find_test_files_from_tests_dir_with_nested_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let tests = root.join("tests");
        let sub = tests.join("integration");
        fs::create_dir_all(&sub).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(tests.join("smoke.rs"), "fn test_smoke() {}\n").unwrap();
        fs::write(sub.join("api.rs"), "fn test_api() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_test_files().unwrap();
        assert!(files.iter().any(|p| p.ends_with("smoke.rs")));
        assert!(
            files.iter().any(|p| p.ends_with("api.rs")),
            "should recurse into tests/ subdirectories: {files:?}"
        );
    }

    #[test]
    fn find_examples_with_nested_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let examples = root.join("examples");
        let sub = examples.join("advanced");
        fs::create_dir_all(&sub).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(examples.join("basic.rs"), "fn main() {}\n").unwrap();
        fs::write(sub.join("complex.rs"), "fn main() {}\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_examples().unwrap();
        assert!(files.iter().any(|p| p.ends_with("basic.rs")));
        assert!(
            files.iter().any(|p| p.ends_with("complex.rs")),
            "should recurse into examples/ subdirs: {files:?}"
        );
    }

    #[test]
    fn get_license_no_license_anywhere_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let handler = RustHandler::new(dir.path());
        assert!(handler.get_license().is_none());
    }

    #[test]
    fn collect_rs_files_with_nested_src_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        let models = src.join("models");
        fs::create_dir_all(&models).unwrap();
        fs::write(src.join("lib.rs"), "pub mod models;\n").unwrap();
        fs::write(models.join("user.rs"), "pub struct User;\n").unwrap();
        fs::write(models.join("post.rs"), "pub struct Post;\n").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(files.iter().any(|p| p.ends_with("lib.rs")));
        assert!(files.iter().any(|p| p.ends_with("user.rs")));
        assert!(files.iter().any(|p| p.ends_with("post.rs")));
    }

    #[test]
    fn find_docs_root_md_files_included() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        fs::write(root.join("CONTRIBUTING.md"), "# Contributing\n").unwrap();
        fs::write(root.join("SECURITY.md"), "# Security\n").unwrap();

        let handler = RustHandler::new(root);
        let found = handler.find_docs().unwrap();
        assert!(found.iter().any(|p| p.ends_with("CONTRIBUTING.md")));
        assert!(found.iter().any(|p| p.ends_with("SECURITY.md")));
    }

    #[test]
    fn version_from_git_tags_with_commit_after_tag() {
        use std::process::Command;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

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
        Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "first", "--no-gpg-sign"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(root)
            .output()
            .unwrap();

        // Make a second commit so HEAD is ahead of the tag
        fs::write(root.join("README.md"), "# X\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "second", "--no-gpg-sign"])
            .current_dir(root)
            .output()
            .unwrap();

        let handler = RustHandler::new(root);
        // describe_tags may include "-N-gHASH" suffix; list_tags_sorted
        // gives the clean tag. Either way we should get 1.0.0.
        let v = handler.get_version().unwrap();
        assert!(
            v.starts_with("1.0.0"),
            "should find version from git tags: {v}"
        );
    }

    #[test]
    fn version_from_git_tags_list_tags_fallback() {
        use std::process::Command;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create repo and tag on main branch
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
        Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "tagged", "--no-gpg-sign"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["tag", "v3.0.0"])
            .current_dir(root)
            .output()
            .unwrap();

        // Create an orphan branch (no shared history with tagged commit).
        // describe_tags will fail because no tag is reachable from HEAD,
        // but list_tags_sorted will find v3.0.0.
        Command::new("git")
            .args(["checkout", "--orphan", "orphan"])
            .current_dir(root)
            .output()
            .unwrap();
        fs::write(root.join("README.md"), "# orphan\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "orphan commit", "--no-gpg-sign"])
            .current_dir(root)
            .output()
            .unwrap();

        let handler = RustHandler::new(root);
        let v = handler.get_version().unwrap();
        assert_eq!(v, "3.0.0", "should fall back to list_tags_sorted");
    }

    #[test]
    fn version_falls_back_to_latest_with_non_version_tags() {
        use std::process::Command;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

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

        // Cargo.toml without a version field
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
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
        // Tag with a non-version name (no dots, doesn't start with digit)
        Command::new("git")
            .args(["tag", "release-candidate"])
            .current_dir(root)
            .output()
            .unwrap();

        let handler = RustHandler::new(root);
        // describe_tags returns "release-candidate", parse_version_tag → None
        // list_tags_sorted returns ["release-candidate"], parse_version_tag → None
        // fetch_tags on a local-only repo is a no-op
        // Falls through to "latest"
        let v = handler.get_version().unwrap();
        assert_eq!(
            v, "latest",
            "non-version tags should fall through to latest"
        );
    }

    // ── Coverage: license-file support ─────────────────────────────────

    #[test]
    fn get_license_from_license_file_field() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nlicense-file = \"COPYING\"\n",
        )
        .unwrap();
        fs::write(root.join("COPYING"), "MIT License\n\nCopyright...").unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();

        let handler = RustHandler::new(root);
        assert_eq!(handler.get_license().unwrap(), "MIT");
    }

    #[test]
    fn get_license_file_fallback_first_line() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nlicense-file = \"LICENSE.custom\"\n",
        )
        .unwrap();
        fs::write(
            root.join("LICENSE.custom"),
            "Custom License v42\nDetails...",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();

        let handler = RustHandler::new(root);
        assert_eq!(handler.get_license().unwrap(), "Custom License v42");
    }

    #[test]
    fn get_license_file_missing_falls_through() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nlicense-file = \"NONEXISTENT\"\n",
        )
        .unwrap();
        // No LICENSE files either
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();

        let handler = RustHandler::new(root);
        assert!(handler.get_license().is_none());
    }

    #[test]
    fn get_license_file_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nlicense-file = \"../../../etc/passwd\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();

        let handler = RustHandler::new(root);
        assert!(handler.get_license().is_none());
    }

    #[test]
    fn get_license_file_rejects_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nlicense-file = \"/etc/passwd\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();

        let handler = RustHandler::new(root);
        assert!(handler.get_license().is_none());
    }

    // ── Coverage: find_source_files prefers src/ ───────────────────────

    #[test]
    fn find_source_files_prefers_src_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        // Root-level .rs file (should be excluded when src/ exists)
        fs::write(root.join("build_helper.rs"), "fn helper() {}").unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "pub fn main() {}").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(
            !files
                .iter()
                .any(|p| p.file_name().unwrap() == "build_helper.rs"),
            "root-level .rs should be excluded when src/ exists: {:?}",
            files
        );
        assert!(files.iter().any(|p| p.file_name().unwrap() == "lib.rs"));
    }

    #[test]
    fn find_source_files_falls_back_to_root_without_src() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        // No src/ dir, just root .rs files
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(files.iter().any(|p| p.file_name().unwrap() == "main.rs"));
    }

    // ── Coverage: collect_test_rs_files ─────────────────────────────────

    #[test]
    fn find_test_files_includes_test_rs_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();
        // *_test.rs file in src/
        fs::write(root.join("src").join("parser_test.rs"), "#[test] fn t() {}").unwrap();

        let handler = RustHandler::new(root);
        let test_files = handler.find_test_files().unwrap();
        assert!(
            test_files
                .iter()
                .any(|p| p.file_name().unwrap() == "parser_test.rs"),
            "should find *_test.rs files: {:?}",
            test_files
        );
    }

    #[test]
    fn find_test_files_skips_target_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();
        // target/ should be skipped
        fs::create_dir_all(root.join("target").join("debug")).unwrap();
        fs::write(
            root.join("target").join("debug").join("something_test.rs"),
            "",
        )
        .unwrap();

        let handler = RustHandler::new(root);
        let test_files = handler.find_test_files().unwrap();
        assert!(
            !test_files
                .iter()
                .any(|p| p.to_str().unwrap().contains("target")),
            "should skip target/: {:?}",
            test_files
        );
    }

    #[test]
    fn find_test_files_recurses_into_nested_tests_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();
        // Nested test dirs: tests/helpers/ with a .rs file
        fs::create_dir_all(root.join("tests").join("helpers")).unwrap();
        fs::write(root.join("tests").join("smoke.rs"), "#[test] fn s() {}").unwrap();
        fs::write(
            root.join("tests").join("helpers").join("common.rs"),
            "pub fn setup() {}",
        )
        .unwrap();
        // Also a non-.rs file to ensure filtering works
        fs::write(root.join("tests").join("data.json"), "{}").unwrap();

        let handler = RustHandler::new(root);
        let test_files = handler.find_test_files().unwrap();
        assert!(test_files
            .iter()
            .any(|p| p.file_name().unwrap() == "smoke.rs"));
        assert!(
            test_files
                .iter()
                .any(|p| p.file_name().unwrap() == "common.rs"),
            "should recurse into tests/helpers/: {:?}",
            test_files
        );
        assert!(
            !test_files
                .iter()
                .any(|p| p.file_name().unwrap() == "data.json"),
            "should only include .rs files: {:?}",
            test_files
        );
    }

    // ── Coverage: collect_docs_recursive ────────────────────────────────

    #[test]
    fn find_docs_includes_docs_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();
        fs::write(root.join("README.md"), "# Hello").unwrap();
        fs::create_dir_all(root.join("docs").join("guides")).unwrap();
        fs::write(root.join("docs").join("intro.md"), "# Intro").unwrap();
        fs::write(root.join("docs").join("guides").join("setup.rst"), "Setup").unwrap();

        let handler = RustHandler::new(root);
        let docs = handler.find_docs().unwrap();
        assert!(
            docs.iter().any(|p| p.file_name().unwrap() == "intro.md"),
            "should find docs/intro.md: {:?}",
            docs
        );
        assert!(
            docs.iter().any(|p| p.file_name().unwrap() == "setup.rst"),
            "should find nested .rst files: {:?}",
            docs
        );
    }

    #[test]
    fn find_docs_skips_hidden_and_vendor_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();
        // docs/ with hidden subdir and vendor subdir
        fs::create_dir_all(root.join("docs").join(".hidden")).unwrap();
        fs::write(root.join("docs").join(".hidden").join("secret.md"), "").unwrap();
        fs::create_dir_all(root.join("docs").join("vendor")).unwrap();
        fs::write(root.join("docs").join("vendor").join("third.md"), "").unwrap();
        fs::write(root.join("docs").join("real.md"), "# Real").unwrap();

        let handler = RustHandler::new(root);
        let docs = handler.find_docs().unwrap();
        assert!(
            !docs.iter().any(|p| p.to_str().unwrap().contains(".hidden")),
            "should skip hidden dirs: {:?}",
            docs
        );
        assert!(
            !docs.iter().any(|p| p.to_str().unwrap().contains("vendor")),
            "should skip vendor dirs: {:?}",
            docs
        );
        assert!(docs.iter().any(|p| p.file_name().unwrap() == "real.md"));
    }

    // ── Coverage: no standalone test files log ─────────────────────────

    #[test]
    fn find_test_files_returns_empty_without_tests_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "pub fn f() {}").unwrap();
        // No tests/ dir, no *_test.rs files

        let handler = RustHandler::new(root);
        let test_files = handler.find_test_files().unwrap();
        assert!(
            test_files.is_empty(),
            "should be empty when no test files exist"
        );
    }

    // ── Coverage: depth limit on collect_rs_files ──────────────────────

    #[test]
    fn collect_rs_files_respects_max_depth() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Create nested dirs deeper than MAX_DEPTH (20)
        let mut deep = root.join("src");
        fs::create_dir(&deep).unwrap();
        for i in 0..22 {
            deep = deep.join(format!("d{i}"));
            fs::create_dir(&deep).unwrap();
        }
        fs::write(deep.join("deep.rs"), "fn deep() {}").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(root.join("src").join("lib.rs"), "pub fn f() {}").unwrap();

        let handler = RustHandler::new(root);
        let files = handler.find_source_files().unwrap();
        assert!(
            !files.iter().any(|p| p.file_name().unwrap() == "deep.rs"),
            "should not descend past MAX_DEPTH: {:?}",
            files
        );
    }

    // ── Coverage: root-level .md files in find_docs ────────────────────

    #[test]
    fn find_docs_includes_root_md_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "").unwrap();
        fs::write(root.join("README.md"), "# Readme").unwrap();
        fs::write(root.join("CONTRIBUTING.md"), "# Contributing").unwrap();
        // CHANGELOG, CHANGES, HISTORY should be excluded
        fs::write(root.join("CHANGELOG.md"), "# Changelog").unwrap();
        fs::write(root.join("CHANGES.md"), "# Changes").unwrap();
        fs::write(root.join("HISTORY.md"), "# History").unwrap();

        let handler = RustHandler::new(root);
        let docs = handler.find_docs().unwrap();
        assert!(
            docs.iter()
                .any(|p| p.file_name().unwrap() == "CONTRIBUTING.md"),
            "should include root .md files: {:?}",
            docs
        );
        assert!(
            !docs.iter().any(|p| p.file_name().unwrap() == "CHANGES.md"),
            "should exclude CHANGES.md: {:?}",
            docs
        );
        assert!(
            !docs.iter().any(|p| p.file_name().unwrap() == "HISTORY.md"),
            "should exclude HISTORY.md: {:?}",
            docs
        );
        assert!(
            !docs
                .iter()
                .any(|p| p.file_name().unwrap() == "CHANGELOG.md"),
            "should exclude CHANGELOG.md: {:?}",
            docs
        );
    }

    // ── get_dependencies tests ──────────────────────────────────────────

    #[test]
    fn get_dependencies_with_string_deps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[dependencies]\ntokio = \"1\"\nserde = \"1.0\"\n",
        )
        .unwrap();

        let handler = RustHandler::new(root);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 2);
        assert!(deps.iter().any(|d| d.name == "tokio"));
        assert!(deps.iter().any(|d| d.name == "serde"));
        let tokio_dep = deps.iter().find(|d| d.name == "tokio").unwrap();
        assert_eq!(tokio_dep.raw_spec.as_deref(), Some("\"1\""));
    }

    #[test]
    fn get_dependencies_with_table_deps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[dependencies]\ntokio = { version = \"1\", features = [\"full\"] }\n",
        )
        .unwrap();

        let handler = RustHandler::new(root);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);
        let tokio_dep = &deps[0];
        assert_eq!(tokio_dep.name, "tokio");
        let raw = tokio_dep.raw_spec.as_ref().unwrap();
        assert!(
            raw.contains("version"),
            "raw_spec should contain version: {raw}"
        );
        assert!(
            raw.contains("features"),
            "raw_spec should contain features: {raw}"
        );
    }

    #[test]
    fn get_dependencies_drops_path_deps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[dependencies]\nmy-local = { path = \"../my-local\" }\nserde = \"1.0\"\n",
        )
        .unwrap();

        let handler = RustHandler::new(root);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1, "path dep should be dropped: {:?}", deps);
        assert_eq!(deps[0].name, "serde");
    }

    #[test]
    fn get_dependencies_resolves_workspace_true() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root with [workspace.dependencies]
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\ntokio = \"1.35\"\nserde = { version = \"1.0\", features = [\"derive\"] }\n",
        )
        .unwrap();

        // Child crate
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\ntokio = { workspace = true }\nserde = { workspace = true }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 2, "both deps should resolve: {:?}", deps);

        let tokio_dep = deps.iter().find(|d| d.name == "tokio").unwrap();
        assert_eq!(
            tokio_dep.raw_spec.as_deref(),
            Some("\"1.35\""),
            "tokio should resolve to workspace version"
        );

        let serde_dep = deps.iter().find(|d| d.name == "serde").unwrap();
        let serde_raw = serde_dep.raw_spec.as_ref().unwrap();
        assert!(
            serde_raw.contains("version") && serde_raw.contains("derive"),
            "serde should resolve to workspace table spec: {serde_raw}"
        );
    }

    #[test]
    fn get_dependencies_workspace_true_with_child_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\nreqwest = \"0.12\"\n",
        )
        .unwrap();

        // Child crate adds features on top of workspace version
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\nreqwest = { workspace = true, features = [\"json\"] }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);

        let reqwest_dep = &deps[0];
        let raw = reqwest_dep.raw_spec.as_ref().unwrap();
        assert!(
            raw.contains("version") && raw.contains("json"),
            "should merge workspace version with child features: {raw}"
        );
    }

    #[test]
    fn get_dependencies_workspace_features_union_not_replace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root: reqwest with features = ["rustls-tls"]
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\nreqwest = { version = \"0.12\", features = [\"rustls-tls\"] }\n",
        )
        .unwrap();

        // Child crate adds "json" feature on top of workspace features
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\nreqwest = { workspace = true, features = [\"json\"] }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);

        let raw = deps[0].raw_spec.as_ref().unwrap();
        // Both workspace ("rustls-tls") AND child ("json") features must be present
        assert!(
            raw.contains("rustls-tls") && raw.contains("json"),
            "features must be additive (union), not replaced: {raw}"
        );
    }

    #[test]
    fn get_dependencies_drops_resolved_path_deps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root where the dep itself is a path dep
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\nmy-local = { path = \"../my-local\" }\n",
        )
        .unwrap();

        // Child crate inherits the path dep via workspace
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\nmy-local = { workspace = true }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert!(
            deps.is_empty(),
            "resolved path dep should be dropped: {:?}",
            deps
        );
    }

    #[test]
    fn load_workspace_deps_returns_empty_when_no_workspace_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // A plain crate with no [workspace] section anywhere up the tree
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"standalone\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let handler = RustHandler::new(root);
        let ws_deps = handler.load_workspace_deps();
        assert!(
            ws_deps.is_empty(),
            "should return empty map without workspace root: {:?}",
            ws_deps
        );
    }

    #[test]
    fn get_dependencies_returns_empty_when_no_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        // No Cargo.toml at all
        let handler = RustHandler::new(dir.path());
        let deps = handler.get_dependencies();
        assert!(deps.is_empty());
    }

    #[test]
    fn get_dependencies_returns_empty_for_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Write invalid TOML that cannot be parsed
        fs::write(root.join("Cargo.toml"), "this is [not valid {{{ toml").unwrap();

        let handler = RustHandler::new(root);
        let deps = handler.get_dependencies();
        assert!(deps.is_empty(), "invalid TOML should return empty deps");
    }

    #[test]
    fn get_dependencies_unresolvable_workspace_dep_degrades_to_wildcard() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root WITHOUT the dep in [workspace.dependencies]
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\n# no entries\n",
        )
        .unwrap();

        // Child references a workspace dep that doesn't exist in root
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\nmissing-dep = { workspace = true }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "missing-dep");
        assert_eq!(
            deps[0].raw_spec.as_deref(),
            Some("\"*\""),
            "unresolvable workspace dep should degrade to wildcard"
        );
    }

    #[test]
    fn get_dependencies_workspace_child_default_features_override() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root: serde with default features
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\n",
        )
        .unwrap();

        // Child crate overrides with default-features = false
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = { workspace = true, default-features = false }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);

        let raw = deps[0].raw_spec.as_ref().unwrap();
        assert!(
            raw.contains("default-features") && raw.contains("false"),
            "should include default-features = false override: {raw}"
        );
        assert!(
            raw.contains("version") && raw.contains("1.0"),
            "should preserve workspace version: {raw}"
        );
    }

    #[test]
    fn get_dependencies_workspace_child_optional_override() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root: simple version string
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\ntokio = \"1.35\"\n",
        )
        .unwrap();

        // Child marks it optional
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\ntokio = { workspace = true, optional = true }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);

        let raw = deps[0].raw_spec.as_ref().unwrap();
        assert!(
            raw.contains("optional") && raw.contains("true"),
            "should include optional = true override: {raw}"
        );
        assert!(
            raw.contains("version") && raw.contains("1.35"),
            "should preserve workspace version: {raw}"
        );
    }

    #[test]
    fn get_dependencies_workspace_child_features_and_default_features() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root: serde with features
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\n",
        )
        .unwrap();

        // Child adds features AND default-features = false simultaneously
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = { workspace = true, features = [\"alloc\"], default-features = false }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);

        let raw = deps[0].raw_spec.as_ref().unwrap();
        // Features should be unioned: derive (from ws) + alloc (from child)
        assert!(
            raw.contains("derive") && raw.contains("alloc"),
            "features should be unioned: {raw}"
        );
        // default-features override should be present
        assert!(
            raw.contains("default-features") && raw.contains("false"),
            "default-features override should be merged: {raw}"
        );
    }

    #[test]
    fn get_dependencies_workspace_string_dep_child_adds_features_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root: simple string version (no features array)
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\ntokio = \"1.35\"\n",
        )
        .unwrap();

        // Child adds features to a workspace dep that has no features
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\ntokio = { workspace = true, features = [\"full\"] }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);

        let raw = deps[0].raw_spec.as_ref().unwrap();
        // The ws dep had no features, child sets features = ["full"]
        // This hits the else branch at line 429-430 (ws has no features array)
        assert!(
            raw.contains("full"),
            "child features should be added to string ws dep: {raw}"
        );
        assert!(
            raw.contains("version") && raw.contains("1.35"),
            "workspace version should be preserved: {raw}"
        );
    }

    #[test]
    fn get_dependencies_workspace_non_table_non_string_shape() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root: dep with integer value (unusual but valid TOML)
        // This hits the `_` arm in the match on ws_tbl shape (lines 407-413)
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\nweird-dep = 42\n",
        )
        .unwrap();

        // Child overrides with features — forces merge path through the `_` arm
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\nweird-dep = { workspace = true, optional = true }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);

        let raw = deps[0].raw_spec.as_ref().unwrap();
        // The integer 42 gets wrapped as { version = 42 } and optional = true merged in
        assert!(
            raw.contains("optional"),
            "child override should be merged even with unexpected ws shape: {raw}"
        );
    }

    #[test]
    fn get_dependencies_workspace_features_dedup_on_union() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Workspace root: reqwest with "json" feature
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"child\"]\n\n[workspace.dependencies]\nreqwest = { version = \"0.12\", features = [\"json\", \"rustls-tls\"] }\n",
        )
        .unwrap();

        // Child also requests "json" (duplicate) plus "stream"
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join("Cargo.toml"),
            "[package]\nname = \"child\"\nversion = \"0.1.0\"\n\n[dependencies]\nreqwest = { workspace = true, features = [\"json\", \"stream\"] }\n",
        )
        .unwrap();

        let handler = RustHandler::new(&child);
        let deps = handler.get_dependencies();
        assert_eq!(deps.len(), 1);

        let raw = deps[0].raw_spec.as_ref().unwrap();
        // All three unique features should be present
        assert!(raw.contains("json"), "should contain json: {raw}");
        assert!(
            raw.contains("rustls-tls"),
            "should contain rustls-tls: {raw}"
        );
        assert!(raw.contains("stream"), "should contain stream: {raw}");

        // "json" should appear only once (deduped)
        let json_count = raw.matches("json").count();
        assert_eq!(
            json_count, 1,
            "json should appear exactly once (deduped), found {json_count} times in: {raw}"
        );
    }

    #[test]
    fn get_package_name_workspace_skips_member_without_package() {
        let dir = tempfile::tempdir().unwrap();
        // Workspace root with two members
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/no-pkg\", \"crates/has-pkg\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        // First member: Cargo.toml exists but has no [package] section (just dependencies)
        let no_pkg_dir = dir.path().join("crates/no-pkg");
        fs::create_dir_all(&no_pkg_dir).unwrap();
        fs::write(
            no_pkg_dir.join("Cargo.toml"),
            "[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        // Second member: has a valid [package] with name
        let has_pkg_dir = dir.path().join("crates/has-pkg");
        fs::create_dir_all(&has_pkg_dir).unwrap();
        fs::write(
            has_pkg_dir.join("Cargo.toml"),
            "[package]\nname = \"real-crate\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_package_name().unwrap(), "real-crate");
    }

    #[test]
    fn get_version_workspace_package_no_dot_falls_through() {
        let dir = tempfile::tempdir().unwrap();
        // workspace.package.version exists but has no '.' — should not be accepted
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\"]\n\n[workspace.package]\nversion = \"beta\"\n",
        )
        .unwrap();
        let member_dir = dir.path().join("crates/a");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            "[package]\nname = \"a\"\nversion.workspace = true\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        // "beta" has no dot, so it falls through to "latest"
        assert_eq!(handler.get_version().unwrap(), "latest");
    }

    #[test]
    fn get_package_name_workspace_member_dir_missing() {
        // Workspace member listed but its directory doesn't exist on disk.
        // fs::read_to_string fails → loop continues → eventually errors.
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/ghost\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        // Don't create the member directory at all
        let handler = RustHandler::new(dir.path());
        assert!(handler.get_package_name().is_err());
    }

    #[test]
    fn get_package_name_workspace_non_string_member() {
        // Workspace members array contains a non-string value (integer).
        // member.as_str() returns None → loop continues → eventually errors.
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [42]\nresolver = \"2\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.get_package_name().is_err());
    }

    #[test]
    fn get_package_name_workspace_no_members_key() {
        // Workspace table exists but has no `members` key.
        // Falls through the members check → errors.
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.get_package_name().is_err());
    }

    #[test]
    fn get_package_name_invalid_toml_no_package() {
        // Content has no [package] name and is not valid TOML.
        // cargo_toml_field returns None, content.parse fails → errors.
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "this is not valid toml {{{\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.get_package_name().is_err());
    }

    #[test]
    fn get_version_invalid_toml_falls_through() {
        // Cargo.toml exists but is not valid TOML and has no [package] version.
        // cargo_toml_field returns None, TOML parse fails → falls through to "latest".
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "this is not valid toml {{{\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_version().unwrap(), "latest");
    }

    #[test]
    fn get_package_name_workspace_rejects_dotdot_member_path() {
        // Member paths containing ".." should be skipped (path traversal guard).
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"../escape\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        // The ".." member is skipped, no valid members remain → error
        assert!(handler.get_package_name().is_err());
    }

    #[test]
    fn get_package_name_workspace_glob_members() {
        // Workspace with glob pattern "crates/*" should expand and find member
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        let member_dir = dir.path().join("crates/mylib");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(
            member_dir.join("Cargo.toml"),
            "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert_eq!(handler.get_package_name().unwrap(), "mylib");
    }

    #[test]
    fn expand_workspace_member_literal_path() {
        let dir = tempfile::tempdir().unwrap();
        let member = dir.path().join("crates/foo");
        fs::create_dir_all(&member).unwrap();
        let result = super::expand_workspace_member(dir.path(), "crates/foo");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], dir.path().join("crates/foo"));
    }

    #[test]
    fn expand_workspace_member_glob_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("crates/alpha");
        let b = dir.path().join("crates/beta");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        // Also create a file (should be filtered out — only dirs)
        fs::write(dir.path().join("crates/README.md"), "hi").unwrap();
        let result = super::expand_workspace_member(dir.path(), "crates/*");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn expand_workspace_member_glob_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("crates")).unwrap();
        // Empty directory — no subdirs
        let result = super::expand_workspace_member(dir.path(), "crates/*");
        assert!(result.is_empty());
    }

    #[test]
    fn expand_workspace_member_glob_with_suffix() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("crates/foo-macros")).unwrap();
        fs::create_dir_all(dir.path().join("crates/foo-core")).unwrap();
        fs::create_dir_all(dir.path().join("crates/bar-macros")).unwrap();
        let result = super::expand_workspace_member(dir.path(), "crates/*-macros");
        assert_eq!(
            result.len(),
            2,
            "should only match -macros dirs: {:?}",
            result
        );
        let names: Vec<_> = result
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"foo-macros".to_string()));
        assert!(names.contains(&"bar-macros".to_string()));
        assert!(!names.contains(&"foo-core".to_string()));
    }

    #[test]
    fn expand_workspace_member_glob_question_mark() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("pkg-a")).unwrap();
        fs::create_dir_all(dir.path().join("pkg-b")).unwrap();
        fs::create_dir_all(dir.path().join("pkg-cc")).unwrap(); // won't match ? (single char)
        let result = super::expand_workspace_member(dir.path(), "pkg-?");
        assert_eq!(
            result.len(),
            2,
            "? should match single char only: {:?}",
            result
        );
    }

    #[test]
    fn expand_workspace_member_glob_nested() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("services/api/crate-a")).unwrap();
        fs::create_dir_all(dir.path().join("services/api/crate-b")).unwrap();
        fs::create_dir_all(dir.path().join("services/web/crate-c")).unwrap();
        let result = super::expand_workspace_member(dir.path(), "services/*/crate-*");
        assert_eq!(result.len(), 3, "nested globs should match: {:?}", result);
    }

    #[test]
    fn detect_native_deps_sys_crate() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mylib\"\nversion = \"1.0.0\"\n\n[dependencies]\nopenssl-sys = \"0.9\"\nserde = \"1\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("openssl-sys")),
            "should detect -sys crate: {:?}",
            indicators
        );
        assert!(
            !indicators.iter().any(|i| i.contains("serde")),
            "should not flag normal crates"
        );
    }

    #[test]
    fn detect_native_deps_build_rs() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mylib\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("build.rs"), "fn main() {}").unwrap();
        let handler = RustHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("build.rs")),
            "should detect build.rs: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_links_field() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"mylib\"\nlinks = \"z\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("links")),
            "should detect links field: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_workspace_member() {
        let dir = tempfile::tempdir().unwrap();
        // Workspace root with no [package]
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/native\"]\n",
        )
        .unwrap();
        // Member crate with -sys dep, links field, and build.rs
        let member = dir.path().join("crates").join("native");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"native\"\nversion = \"1.0.0\"\nlinks = \"z\"\n\n[dependencies]\nopenssl-sys = \"0.9\"\n",
        )
        .unwrap();
        fs::write(member.join("build.rs"), "fn main() {}").unwrap();
        let handler = RustHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.iter().any(|i| i.contains("openssl-sys")),
            "should detect -sys crate in workspace member: {:?}",
            indicators
        );
        assert!(
            indicators.iter().any(|i| i.contains("build.rs in member")),
            "should detect build.rs in workspace member: {:?}",
            indicators
        );
        assert!(
            indicators.iter().any(|i| i.contains("links")),
            "should detect links field in workspace member: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_workspace_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        // Workspace with a ".." member and an absolute path member — both should be skipped
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"../escape\", \"/etc/evil\"]\n",
        )
        .unwrap();
        // Create the escape dir so it would match if traversal weren't blocked
        let escape_dir = dir.path().parent().unwrap().join("escape");
        fs::create_dir_all(&escape_dir).ok();
        fs::write(
            escape_dir.join("Cargo.toml"),
            "[package]\nname = \"evil\"\nversion = \"1.0.0\"\n\n[dependencies]\nopenssl-sys = \"0.9\"\n",
        )
        .ok();
        let handler = RustHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert!(
            indicators.is_empty(),
            "path traversal members should be skipped: {:?}",
            indicators
        );
        // Cleanup the escape dir we created outside the tempdir
        fs::remove_dir_all(&escape_dir).ok();
    }

    #[test]
    fn detect_native_deps_workspace_member_build_rs_only() {
        // Workspace member that has build.rs but no -sys deps or links
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/buildcrate\"]\n",
        )
        .unwrap();
        let member = dir.path().join("crates").join("buildcrate");
        fs::create_dir_all(&member).unwrap();
        fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"buildcrate\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(member.join("build.rs"), "fn main() {}").unwrap();
        let handler = RustHandler::new(dir.path());
        let indicators = handler.detect_native_deps();
        assert_eq!(indicators.len(), 1);
        assert!(
            indicators[0].contains("build.rs in member buildcrate"),
            "should detect build.rs in member: {:?}",
            indicators
        );
    }

    #[test]
    fn detect_native_deps_clean() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"pure-rust\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();
        let handler = RustHandler::new(dir.path());
        assert!(handler.detect_native_deps().is_empty());
    }

    #[test]
    fn has_dotted_workspace_key_falls_through_non_matching_lines() {
        // Lines in [package] with '=' that don't match the dotted key
        // should fall through (exercises line 107 closing brace).
        let content = "[package]\nname = \"test\"\nversion = \"1.0.0\"\nlicense = \"MIT\"\n";
        assert!(!RustHandler::has_dotted_workspace_key(content, "version"));
    }

    #[test]
    fn has_dotted_workspace_key_returns_true_when_present() {
        let content = "[package]\nname = \"test\"\nversion.workspace = true\n";
        assert!(RustHandler::has_dotted_workspace_key(content, "version"));
    }

    #[test]
    fn has_dotted_workspace_key_returns_true_with_trailing_comment() {
        let content = "[package]\nversion.workspace = true # inherited\n";
        assert!(RustHandler::has_dotted_workspace_key(content, "version"));
    }

    #[test]
    fn has_dotted_workspace_key_skips_lines_without_equals() {
        // A comment-only line inside [package] has no '=' and exercises the
        // implicit else of `if let Some(eq_pos)`.
        let content = "[package]\n# a comment\nversion.workspace = true\n";
        assert!(RustHandler::has_dotted_workspace_key(content, "version"));
    }

    #[test]
    fn version_from_workspace_root_at_filesystem_root() {
        // When repo_path is "/" (filesystem root), dir.pop() returns false
        // immediately, exercising line 119.
        let handler = RustHandler::new(std::path::Path::new("/"));
        assert!(handler.version_from_workspace_root().is_none());
    }

    #[test]
    fn version_from_workspace_root_finds_version() {
        let dir = tempfile::tempdir().unwrap();
        // Create workspace root Cargo.toml with [workspace.package].version
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"2.3.4\"\n",
        )
        .unwrap();
        // Create a sub-crate directory
        let sub = dir.path().join("crates").join("mycrate");
        std::fs::create_dir_all(&sub).unwrap();
        let handler = RustHandler::new(&sub);
        let ver = handler.version_from_workspace_root();
        assert_eq!(ver, Some("2.3.4".to_string()));
    }

    #[test]
    fn version_from_workspace_root_rejects_version_without_dot() {
        // A workspace version like "dev" (no '.') should be rejected,
        // exercising the else branch of `if ver.contains('.')`.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"dev\"\n",
        )
        .unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let handler = RustHandler::new(&sub);
        assert!(handler.version_from_workspace_root().is_none());
    }

    #[test]
    fn version_from_workspace_root_stops_at_git_boundary() {
        let dir = tempfile::tempdir().unwrap();
        // workspace root at top level
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"9.9.9\"\n",
        )
        .unwrap();
        // .git at level 1 — should prevent walking above level 1
        let level1 = dir.path().join("level1");
        std::fs::create_dir_all(&level1).unwrap();
        std::fs::create_dir(level1.join(".git")).unwrap();
        // sub-crate at level 2
        let sub = level1.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let handler = RustHandler::new(&sub);
        // Should NOT find the workspace root above .git boundary
        assert!(handler.version_from_workspace_root().is_none());
    }

    #[test]
    fn version_from_workspace_root_deep_nesting() {
        let dir = tempfile::tempdir().unwrap();
        // workspace root at top level
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"5.6.7\"\n",
        )
        .unwrap();
        // 5 levels deep — previously capped at 3
        let deep = dir.path().join("a/b/c/d/e");
        std::fs::create_dir_all(&deep).unwrap();
        let handler = RustHandler::new(&deep);
        assert_eq!(
            handler.version_from_workspace_root(),
            Some("5.6.7".to_string())
        );
    }

    #[test]
    fn version_from_workspace_root_stops_at_workspace_without_version() {
        // Per Cargo semantics: if nearest [workspace] has no [workspace.package].version,
        // don't walk higher — return None.
        let dir = tempfile::tempdir().unwrap();
        // Higher ancestor has version
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nversion = \"9.9.9\"\n[workspace]\nmembers = [\"inner/*\"]\n",
        )
        .unwrap();
        // Inner workspace has [workspace] but no version
        let inner = dir.path().join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(
            inner.join("Cargo.toml"),
            "[workspace]\nmembers = [\"sub\"]\n",
        )
        .unwrap();
        let sub = inner.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let handler = RustHandler::new(&sub);
        // Should return None — stops at inner [workspace], doesn't walk to outer
        assert!(
            handler.version_from_workspace_root().is_none(),
            "should stop at nearest [workspace] even without version"
        );
    }

    #[test]
    fn expand_workspace_member_glob_nonexistent_prefix() {
        let dir = tempfile::tempdir().unwrap();
        // Don't create the "crates" directory — read_dir should fail,
        // falling through to Vec::new()
        let result = super::expand_workspace_member(dir.path(), "crates/*");
        assert!(result.is_empty());
    }

    #[test]
    fn expand_workspace_member_glob_matches_directories() {
        let dir = tempfile::tempdir().unwrap();
        let crates = dir.path().join("crates");
        std::fs::create_dir_all(crates.join("foo")).unwrap();
        std::fs::create_dir_all(crates.join("bar")).unwrap();
        // A file should not match (glob only returns dirs)
        std::fs::write(crates.join("baz.txt"), "not a dir").unwrap();
        let result = super::expand_workspace_member(dir.path(), "crates/*");
        assert_eq!(result.len(), 2, "should match 2 dirs, got: {:?}", result);
        // Should be sorted
        let names: Vec<_> = result
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert_eq!(names, vec!["bar", "foo"]);
    }

    #[test]
    fn expand_workspace_member_glob_with_suffix_filter() {
        // Test "crates/*-macros" style glob
        let dir = tempfile::tempdir().unwrap();
        let crates = dir.path().join("crates");
        std::fs::create_dir_all(crates.join("foo-macros")).unwrap();
        std::fs::create_dir_all(crates.join("bar-macros")).unwrap();
        std::fs::create_dir_all(crates.join("baz-core")).unwrap();
        let result = super::expand_workspace_member(dir.path(), "crates/*-macros");
        assert_eq!(result.len(), 2, "should match only -macros dirs");
    }

    #[test]
    fn expand_workspace_member_literal_no_glob() {
        let dir = tempfile::tempdir().unwrap();
        let result = super::expand_workspace_member(dir.path(), "subcrate");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], dir.path().join("subcrate"));
    }

    #[test]
    fn expand_workspace_member_question_mark_glob() {
        // '?' glob character
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("a1")).unwrap();
        std::fs::create_dir_all(root.join("a2")).unwrap();
        std::fs::create_dir_all(root.join("bb")).unwrap();
        let result = super::expand_workspace_member(root, "a?");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn expand_workspace_member_bracket_glob() {
        // '[' glob character
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("xa")).unwrap();
        std::fs::create_dir_all(root.join("xb")).unwrap();
        std::fs::create_dir_all(root.join("xc")).unwrap();
        let result = super::expand_workspace_member(root, "x[ab]");
        assert_eq!(result.len(), 2, "should match xa and xb");
    }

    // ── collect_rs_files / collect_all_rs_in_dir ────────────────────

    #[test]
    fn collect_rs_files_excludes_tests_and_build_rs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "pub fn hello() {}").unwrap();
        std::fs::write(src.join("build.rs"), "fn main() {}").unwrap();
        std::fs::write(src.join("foo_test.rs"), "#[test] fn t() {}").unwrap();
        let handler = RustHandler::new(root);
        let mut files = Vec::new();
        handler.collect_rs_files(&src, &mut files, 0).unwrap();
        let names: Vec<_> = files
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"lib.rs"));
        assert!(!names.contains(&"build.rs"));
        assert!(!names.contains(&"foo_test.rs"));
    }

    #[test]
    fn collect_all_rs_in_dir_includes_all_rs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let tests = root.join("tests");
        std::fs::create_dir_all(&tests).unwrap();
        std::fs::write(tests.join("integration.rs"), "fn main() {}").unwrap();
        std::fs::write(tests.join("not_rs.txt"), "nope").unwrap();
        let handler = RustHandler::new(root);
        let mut files = Vec::new();
        handler
            .collect_all_rs_in_dir(&tests, &mut files, 0)
            .unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("integration.rs"));
    }

    #[test]
    fn collect_test_rs_files_finds_test_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "pub fn x() {}").unwrap();
        std::fs::write(src.join("foo_test.rs"), "#[test] fn t() {}").unwrap();
        let handler = RustHandler::new(root);
        let mut files = Vec::new();
        handler.collect_test_rs_files(&src, &mut files, 0).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("foo_test.rs"));
    }

    // ── collect_docs_recursive ──────────────────────────────────────

    #[test]
    fn collect_docs_recursive_rust_enters_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let docs = root.join("docs").join("api");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("reference.md"), "# Ref\n").unwrap();
        // target/ should be skipped
        let target = root.join("docs").join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("output.md"), "# Build").unwrap();

        let handler = RustHandler::new(root);
        let mut found = Vec::new();
        handler
            .collect_docs_recursive(&root.join("docs"), &mut found, 0)
            .unwrap();
        let names: Vec<_> = found
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"reference.md"));
        assert!(!names.contains(&"output.md"));
    }
}
