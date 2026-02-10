use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

/// Extract version from various sources
/// Priority: explicit > version_from strategy > package metadata
pub fn extract_version(
    repo_path: &Path,
    explicit_version: Option<String>,
    version_from: Option<String>,
) -> Result<String> {
    // Priority 1: Explicit version always wins
    if let Some(version) = explicit_version {
        return Ok(version);
    }

    // Priority 2: version_from strategy
    if let Some(strategy) = version_from {
        return match strategy.as_str() {
            "git-tag" => extract_from_git_tag(repo_path),
            "package" => extract_from_package(repo_path),
            "branch" => extract_from_branch(repo_path),
            "commit" => extract_from_commit(repo_path),
            _ => bail!(
                "Unknown version source: {}. Valid options: git-tag, package, branch, commit",
                strategy
            ),
        };
    }

    // Priority 3: Default to package metadata
    extract_from_package(repo_path)
}

/// Extract version from Git tag (e.g., "v1.2.3" -> "1.2.3")
fn extract_from_git_tag(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        bail!("No git tags found");
    }

    let tag = String::from_utf8(output.stdout)?.trim().to_string();

    // Strip 'v' prefix if present
    Ok(tag.strip_prefix('v').unwrap_or(&tag).to_string())
}

/// Extract version from package metadata
/// Priority: build system files > source code > changelog/docs > git tags
fn extract_from_package(repo_path: &Path) -> Result<String> {
    // Strategy 1: pyproject.toml — canonical source of truth for Python packages
    let pyproject = repo_path.join("pyproject.toml");
    if pyproject.exists() {
        if let Ok(content) = std::fs::read_to_string(&pyproject) {
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

    // Strategy 2: setup.cfg
    let setup_cfg = repo_path.join("setup.cfg");
    if setup_cfg.exists() {
        if let Ok(content) = std::fs::read_to_string(&setup_cfg) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("version") && trimmed.contains("=") {
                    if let Some(version) = trimmed.split('=').nth(1) {
                        let version = version.trim();
                        if let Some(v) = extract_version_pattern(version) {
                            return Ok(v);
                        }
                    }
                }
            }
        }
    }

    // Strategy 3: __version__ in Python source files
    if let Ok(version) = extract_version_from_python_source(repo_path) {
        return Ok(version);
    }

    // Strategy 4: version.txt (used by PyTorch and others)
    let version_txt = repo_path.join("version.txt");
    if version_txt.exists() {
        if let Ok(content) = std::fs::read_to_string(&version_txt) {
            let trimmed = content.trim();
            let cleaned: String = trimmed
                .chars()
                .take_while(|c| c.is_numeric() || *c == '.')
                .collect();
            if let Some(v) =
                extract_version_pattern(&cleaned).or_else(|| extract_version_pattern(trimmed))
            {
                return Ok(v);
            }
        }
    }

    // Strategy 5: Changelog files (may contain "Unreleased" — less reliable than build files)
    for changelog_name in &[
        "CHANGELOG.md",
        "CHANGELOG.rst",
        "CHANGELOG",
        "CHANGES.md",
        "CHANGES.rst",
        "CHANGES",
        "HISTORY.md",
        "HISTORY.rst",
        "HISTORY",
        "NEWS.md",
        "NEWS.rst",
        "NEWS",
    ] {
        let changelog_path = repo_path.join(changelog_name);
        if changelog_path.exists() {
            if let Ok(version) = extract_version_from_changelog(&changelog_path) {
                return Ok(version);
            }
        }
    }

    // Strategy 6: Documentation files (release notes, whatsnew — least reliable)
    for docs_dir in &["docs", "doc", "web"] {
        let dir_path = repo_path.join(docs_dir);
        if dir_path.exists() {
            if let Ok(version) = extract_version_from_docs(&dir_path) {
                return Ok(version);
            }
        }
    }

    // Strategy 7: Git tags as last resort
    if let Ok(version) = extract_from_git_tag(repo_path) {
        return Ok(version);
    }

    Ok("unknown".to_string())
}

/// Extract __version__ from Python source files
fn extract_version_from_python_source(repo_path: &Path) -> Result<String> {
    use std::fs;

    // Find likely package directories
    let candidates: Vec<std::path::PathBuf> = ["src", "."]
        .iter()
        .flat_map(|prefix| {
            let dir = repo_path.join(prefix);
            fs::read_dir(&dir)
                .into_iter()
                .flat_map(|entries| entries.flatten())
                .filter(|e| e.path().is_dir())
                .filter(|e| {
                    // Look for dirs containing __init__.py (Python packages)
                    e.path().join("__init__.py").exists()
                })
                .map(|e| e.path())
                .collect::<Vec<_>>()
        })
        .collect();

    for pkg_dir in candidates {
        // Check _version.py first (common pattern: sklearn/_version.py)
        for version_file in &["_version.py", "__init__.py", "version.py"] {
            let path = pkg_dir.join(version_file);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    // Look for __version__ = "X.Y.Z" or VERSION = "X.Y.Z"
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if (trimmed.starts_with("__version__") || trimmed.starts_with("VERSION"))
                            && trimmed.contains('=')
                        {
                            if let Some(rhs) = trimmed.split('=').nth(1) {
                                let cleaned = rhs
                                    .trim()
                                    .trim_matches(|c: char| c == '"' || c == '\'' || c == ' ');
                                if let Some(v) = extract_version_pattern(cleaned) {
                                    return Ok(v);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    bail!("No __version__ found in Python source")
}

/// Extract version from changelog file
/// Warning: Some repos maintain multiple versions in one changelog - we take the FIRST (most recent)
fn extract_version_from_changelog(changelog_path: &Path) -> Result<String> {
    use std::fs;

    let content = fs::read_to_string(changelog_path)?;

    // Only check first 3000 chars to avoid multi-version changelogs
    // First version in file is typically the latest
    let search_text = content.chars().take(3000).collect::<String>();

    // Look for version patterns in headings/sections
    // Common formats: "## 3.0.0", "# Version 3.0.0", "[3.0.0]", etc.
    for line in search_text.lines().take(100) {
        let line_lower = line.to_lowercase();

        // Check if line looks like a version heading
        // Includes RST-style: "2.32.5 (2025-08-18)" (starts with digit)
        if line_lower.starts_with("##")
            || line_lower.starts_with('#')
            || line_lower.starts_with('[')
            || line_lower.starts_with("version")
            || line_lower.starts_with(|c: char| c.is_ascii_digit())
        {
            // Extract version number from this line
            if let Some(version) = extract_version_pattern(line) {
                // Validate it's not a comparison or range (e.g., "2.0 - 3.0")
                if !line.contains(" - ") && !line.contains("..") {
                    return Ok(version);
                }
            }
        }
    }

    bail!("No version found in changelog")
}

/// Extract version from documentation files (release notes, blog posts, etc.)
fn extract_version_from_docs(docs_dir: &Path) -> Result<String> {
    use std::fs;

    // Walk through docs directory (limited depth to avoid performance issues)
    fn search_docs_recursive(dir: &Path, depth: usize) -> Option<String> {
        if depth > 5 {
            return None;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_file() {
                    // Check release-related files
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        let fname_lower = filename.to_lowercase();

                        if fname_lower.contains("release")
                            || fname_lower.contains("whatsnew")
                            || fname_lower.contains("changelog")
                            || fname_lower.contains("blog")
                        {
                            // Only process .md and .rst files
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                if ext == "md" || ext == "rst" {
                                    if let Ok(content) = fs::read_to_string(&path) {
                                        // Check first 2000 chars for version
                                        let search_text =
                                            content.chars().take(2000).collect::<String>();

                                        if let Some(version) = extract_version_pattern(&search_text)
                                        {
                                            return Some(version);
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if path.is_dir() {
                    // Recurse into subdirectories
                    if let Some(version) = search_docs_recursive(&path, depth + 1) {
                        return Some(version);
                    }
                }
            }
        }

        None
    }

    search_docs_recursive(docs_dir, 0).ok_or_else(|| anyhow::anyhow!("No version found in docs"))
}

/// Extract version pattern like "3.0.0" or "2.1.4" from text (generic, no hardcoded package names)
fn extract_version_pattern(text: &str) -> Option<String> {
    // Look for semantic version patterns: X.Y.Z where X, Y, Z are numbers
    // Common in: "version 3.0.0", "released 3.0.0", "## 3.0.0", "[3.0.0]"

    // Split into words and check each for version pattern
    let words: Vec<&str> = text.split_whitespace().collect();

    for word in words {
        // Clean up common prefixes/suffixes (brackets, colons, etc.)
        let clean = word.trim_matches(|c: char| !c.is_numeric() && c != '.');

        // Check if it looks like a semantic version (e.g., "3.0.0", "2.1.4", "1.0")
        if clean.contains('.') {
            let parts: Vec<&str> = clean.split('.').collect();

            // Must have at least major.minor format (2 parts minimum)
            // Maximum 4 parts (e.g., 1.2.3.4 for some versioning schemes)
            // All parts must be numeric
            if parts.len() >= 2
                && parts.len() <= 4
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_numeric()))
            {
                // Extra validation: First part shouldn't be unreasonably large
                // (Catches things like "192.168.1.1" IP addresses)
                if let Ok(major) = parts[0].parse::<u32>() {
                    if major < 100 {
                        // Reasonable major version
                        return Some(clean.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Extract version from current Git branch name
/// "feature/awesome-stuff" -> "branch-feature-awesome-stuff"
fn extract_from_branch(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        bail!("Not a git repository");
    }

    let branch = String::from_utf8(output.stdout)?.trim().to_string();

    // Sanitize branch name: replace / with - and remove special chars
    let sanitized = branch.replace(['/', '_'], "-");

    Ok(format!("branch-{}", sanitized))
}

/// Extract version from current Git commit SHA
/// Returns "dev-<short-sha>" (7 characters)
fn extract_from_commit(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        bail!("Not a git repository");
    }

    let sha = String::from_utf8(output.stdout)?.trim().to_string();

    Ok(format!("dev-{}", sha))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_explicit_version_priority() {
        let result = extract_version(
            Path::new("/tmp"),
            Some("explicit-1.0.0".to_string()),
            Some("git-tag".to_string()),
        );
        assert_eq!(result.unwrap(), "explicit-1.0.0");
    }

    #[test]
    fn test_unknown_version_strategy() {
        let result = extract_version(Path::new("/tmp"), None, Some("nonsense".to_string()));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown version source"));
    }

    #[test]
    fn test_extract_version_pattern_semver() {
        assert_eq!(
            extract_version_pattern("version 3.0.0"),
            Some("3.0.0".to_string())
        );
        assert_eq!(
            extract_version_pattern("## 2.1.4"),
            Some("2.1.4".to_string())
        );
        assert_eq!(
            extract_version_pattern("[1.0.0]"),
            Some("1.0.0".to_string())
        );
        assert_eq!(extract_version_pattern("v1.2"), Some("1.2".to_string()));
    }

    #[test]
    fn test_extract_version_pattern_rejects_ips() {
        assert_eq!(extract_version_pattern("192.168.1.1"), None);
    }

    #[test]
    fn test_extract_version_pattern_no_match() {
        assert_eq!(extract_version_pattern("no version here"), None);
        assert_eq!(extract_version_pattern(""), None);
    }

    #[test]
    fn test_extract_from_package_pyproject() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("pyproject.toml"),
            "[project]\nversion = \"1.5.3\"\n",
        )
        .unwrap();
        let result = extract_from_package(tmp.path()).unwrap();
        assert_eq!(result, "1.5.3");
    }

    #[test]
    fn test_extract_from_package_changelog() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("CHANGELOG.md"),
            "# Changelog\n\n## 4.2.0\n\n- Added stuff\n",
        )
        .unwrap();
        let result = extract_from_package(tmp.path()).unwrap();
        assert_eq!(result, "4.2.0");
    }

    #[test]
    fn test_extract_from_package_fallback_unknown() {
        let tmp = TempDir::new().unwrap();
        let result = extract_from_package(tmp.path()).unwrap();
        assert_eq!(result, "unknown");
    }

    #[test]
    fn test_extract_from_git_tag() {
        // Use our own repo — it has a 'working' tag
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = extract_from_git_tag(repo);
        // Should succeed since we tagged 'working'
        if let Ok(tag) = result {
            assert!(!tag.is_empty());
        }
        // If it fails, that's OK too (CI may not have tags)
    }

    #[test]
    fn test_extract_from_branch() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = extract_from_branch(repo).unwrap();
        assert!(result.starts_with("branch-"));
    }

    #[test]
    fn test_extract_from_commit() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = extract_from_commit(repo).unwrap();
        assert!(result.starts_with("dev-"));
        // Short SHA is 7 chars
        assert_eq!(result.len(), 4 + 7); // "dev-" + 7 hex chars
    }

    #[test]
    fn test_extract_from_setup_cfg() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("setup.cfg"),
            "[metadata]\nname = mylib\nversion = 2.8.1\n",
        )
        .unwrap();
        let result = extract_from_package(tmp.path()).unwrap();
        assert_eq!(result, "2.8.1");
    }

    #[test]
    fn test_extract_from_python_version_file() {
        let tmp = TempDir::new().unwrap();
        let pkg = tmp.path().join("mypackage");
        fs::create_dir(&pkg).unwrap();
        fs::write(pkg.join("__init__.py"), "").unwrap();
        fs::write(pkg.join("_version.py"), "__version__ = \"1.6.0\"\n").unwrap();
        let result = extract_from_package(tmp.path()).unwrap();
        assert_eq!(result, "1.6.0");
    }

    #[test]
    fn test_extract_from_python_init() {
        let tmp = TempDir::new().unwrap();
        let pkg = tmp.path().join("mylib");
        fs::create_dir(&pkg).unwrap();
        fs::write(
            pkg.join("__init__.py"),
            "# My library\n__version__ = '3.2.1'\n",
        )
        .unwrap();
        let result = extract_from_package(tmp.path()).unwrap();
        assert_eq!(result, "3.2.1");
    }

    #[test]
    fn test_extract_from_version_txt() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("version.txt"), "2.5.0a0\n").unwrap();
        // version.txt often has pre-release suffixes; extract_version_pattern gets the semver part
        let result = extract_from_package(tmp.path()).unwrap();
        // Should at least not be "unknown"
        assert_ne!(result, "unknown");
    }

    #[test]
    fn test_extract_from_changelog_rst() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("CHANGES.rst"),
            "2.32.5 (2025-08-18)\n===================\n\n* Bug fix\n",
        )
        .unwrap();
        let result = extract_from_package(tmp.path()).unwrap();
        assert_eq!(result, "2.32.5");
    }

    #[test]
    fn test_extract_version_pattern_rejects_single_part() {
        assert_eq!(extract_version_pattern("1"), None);
    }

    #[test]
    fn test_extract_version_pattern_accepts_two_part() {
        assert_eq!(extract_version_pattern("1.0"), Some("1.0".to_string()));
    }

    #[test]
    fn test_extract_version_pattern_accepts_three_part() {
        assert_eq!(extract_version_pattern("1.0.0"), Some("1.0.0".to_string()));
    }

    #[test]
    fn test_extract_from_git_tag_no_repo() {
        let tmp = TempDir::new().unwrap();
        let result = extract_from_git_tag(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_from_branch_no_repo() {
        let tmp = TempDir::new().unwrap();
        let result = extract_from_branch(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_from_commit_no_repo() {
        let tmp = TempDir::new().unwrap();
        let result = extract_from_commit(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_version_from_changelog_deep() {
        let tmp = TempDir::new().unwrap();
        let changelog_path = tmp.path().join("CHANGELOG.md");

        // Build a changelog with 200+ lines of filler before any version heading.
        // The function only searches the first ~100 lines, so the version on line 150+
        // should NOT be found.
        let mut content = String::from("# Changelog\n\n");
        for i in 0..200 {
            content.push_str(&format!("- Fix item number {}\n", i));
        }
        content.push_str("\n## 9.9.9\n\n- Late version entry\n");

        fs::write(&changelog_path, &content).unwrap();
        let result = extract_version_from_changelog(&changelog_path);
        // The version heading is past line 100, so the function should not find it.
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_version_from_docs_no_docs() {
        let tmp = TempDir::new().unwrap();
        // No docs/ directory exists at all.
        let result = extract_version_from_docs(&tmp.path().join("docs"));
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_version_from_python_source_no_files() {
        let tmp = TempDir::new().unwrap();
        // Empty directory — no Python source files.
        let result = extract_version_from_python_source(tmp.path());
        assert!(result.is_err());
    }
}
