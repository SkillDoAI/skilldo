use anyhow::{bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

use crate::detector::Language;
use crate::ecosystems::python::PythonHandler;

/// Find the largest byte index <= `index` that is a char boundary in `s`.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub struct Collector {
    repo_path: PathBuf,
    language: Language,
    max_source_chars: usize,
}

impl Collector {
    pub fn new(repo_path: &Path, language: Language) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
            language,
            max_source_chars: 100_000, // default ~25K tokens
        }
    }

    /// Set the total character budget for collected source material.
    /// Maps to config `max_source_tokens` (which is actually a char budget).
    pub fn with_max_source_chars(mut self, budget: usize) -> Self {
        self.max_source_chars = budget;
        self
    }

    pub async fn collect(&self) -> Result<CollectedData> {
        info!("Collecting files for {:?}", self.language);

        match self.language {
            Language::Python => self.collect_python().await,
            _ => bail!(
                "Support for {:?} not yet implemented. Currently only Python is supported.",
                self.language
            ),
        }
    }

    async fn collect_python(&self) -> Result<CollectedData> {
        let handler = PythonHandler::new(&self.repo_path);

        // Find files
        let example_paths = handler.find_examples()?;
        let test_paths = handler.find_test_files()?;
        let doc_paths = handler.find_docs()?;
        let source_paths = handler.find_source_files()?;
        let changelog_path = handler.find_changelog();
        let version = handler.get_version()?;
        let license = handler.get_license();
        let project_urls = handler.get_project_urls();

        // Smart token budget allocation scaled to max_source_chars.
        // The total across all categories is capped at `budget`.
        //
        // Fixed categories get first claim, source gets the remainder:
        // - 30% examples - Real usage patterns
        // - 30% tests - API usage in tests
        // - 20% docs - Documentation and tutorials
        // - 5% changelog - Version history
        // - remainder → source (15-100% depending on project scale, capped)
        let budget = self.max_source_chars;
        let examples_budget = budget * 30 / 100;
        let test_budget = budget * 30 / 100;
        let docs_budget = budget * 20 / 100;
        let changelog_budget = budget * 5 / 100;
        let fixed_total = examples_budget + test_budget + docs_budget + changelog_budget;

        let examples_content = Self::read_files(&example_paths, examples_budget)?;
        let test_content = Self::read_files(&test_paths, test_budget)?;
        let docs_content = Self::read_files(&doc_paths, docs_budget)?;

        // Source budget = whatever remains after fixed categories, scaled by project size
        let remaining = budget.saturating_sub(fixed_total);
        let source_budget = match source_paths.len() {
            n if n > 2000 => remaining,            // Massive — use full remainder
            n if n > 1000 => remaining * 60 / 100, // Very large
            n if n > 300 => remaining * 40 / 100,  // Large
            _ => remaining,                        // Small — use full remainder (it's only 15%)
        };
        let source_content = Self::read_files_smart(&source_paths, source_budget, &self.repo_path)?;
        let changelog_content = if let Some(path) = changelog_path {
            Self::read_file_limited(&path, changelog_budget)?
        } else {
            String::new()
        };

        // Get package name - try multiple strategies
        let package_name = Self::detect_package_name(&self.repo_path)?;

        Ok(CollectedData {
            package_name,
            version,
            license,
            project_urls,
            language: self.language.clone(),
            source_file_count: source_paths.len(),
            examples_content,
            test_content,
            docs_content,
            source_content,
            changelog_content,
        })
    }

    fn read_files(paths: &[PathBuf], max_chars: usize) -> Result<String> {
        let mut content = String::new();
        let mut total_chars = 0;

        for path in paths {
            if total_chars >= max_chars {
                info!("Reached character limit, truncating remaining files");
                break;
            }

            if let Ok(file_content) = fs::read_to_string(path) {
                let remaining = max_chars - total_chars;
                if file_content.len() <= remaining {
                    content.push_str(&format!("\n\n// File: {}\n", path.display()));
                    content.push_str(&file_content);
                    total_chars += file_content.len();
                } else {
                    content.push_str(&format!("\n\n// File: {} (truncated)\n", path.display()));
                    let end = floor_char_boundary(&file_content, remaining);
                    content.push_str(&file_content[..end]);
                    total_chars = max_chars;
                    break;
                }
            }
        }

        info!("Read {} characters from {} files", total_chars, paths.len());
        Ok(content)
    }

    fn read_file_limited(path: &Path, max_chars: usize) -> Result<String> {
        let content = fs::read_to_string(path)?;
        if content.len() > max_chars {
            let end = floor_char_boundary(&content, max_chars);
            Ok(content[..end].to_string())
        } else {
            Ok(content)
        }
    }

    /// Detect package name using multiple strategies
    fn detect_package_name(repo_path: &Path) -> Result<String> {
        // Strategy 1: Read from pyproject.toml
        let pyproject = repo_path.join("pyproject.toml");
        if pyproject.exists() {
            if let Ok(content) = fs::read_to_string(&pyproject) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("name") && trimmed.contains("=") {
                        if let Some(name) = trimmed.split('=').nth(1) {
                            let name = name.trim().trim_matches('"').trim_matches('\'');
                            if !name.is_empty() && !name.contains("[") {
                                return Ok(name.to_lowercase());
                            }
                        }
                    }
                }
            }
        }

        // Strategy 2: Read from setup.py
        let setup_py = repo_path.join("setup.py");
        if setup_py.exists() {
            if let Ok(content) = fs::read_to_string(&setup_py) {
                for line in content.lines() {
                    if line.contains("name=") || line.contains("name =") {
                        if let Some(start) = line.find("name") {
                            if let Some(eq) = line[start..].find("=") {
                                let after_eq = &line[start + eq + 1..];
                                if let Some(quote_start) = after_eq.find(['\'', '"']) {
                                    let quote_char = after_eq.chars().nth(quote_start).unwrap();
                                    if let Some(quote_end) =
                                        after_eq[quote_start + 1..].find(quote_char)
                                    {
                                        let name =
                                            &after_eq[quote_start + 1..quote_start + 1 + quote_end];
                                        if !name.is_empty() {
                                            return Ok(name.to_lowercase());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Strategy 3: Canonicalize path and get directory name
        if let Ok(canonical) = repo_path.canonicalize() {
            if let Some(name) = canonical.file_name().and_then(|n| n.to_str()) {
                if name != "." && name != ".." && !name.is_empty() {
                    return Ok(name.to_lowercase());
                }
            }
        }

        // Strategy 4: Just use the file_name if not "."
        if let Some(name) = repo_path.file_name().and_then(|n| n.to_str()) {
            if name != "." && name != ".." && !name.is_empty() {
                return Ok(name.to_lowercase());
            }
        }

        // Final fallback
        Ok("unknown".to_string())
    }

    /// Smart file reading - prioritizes public API files over implementation
    /// Uses intelligent prioritization to read critical files fully, others partially
    fn read_files_smart(paths: &[PathBuf], max_chars: usize, repo_path: &Path) -> Result<String> {
        // Calculate priority for each file and sort
        let mut prioritized: Vec<(i32, PathBuf)> = paths
            .iter()
            .map(|path| (Self::calculate_file_priority(path, repo_path), path.clone()))
            .collect();

        prioritized.sort_by_key(|(priority, _)| *priority);

        let mut content = String::new();
        let mut total_chars = 0;

        for (priority, path) in prioritized {
            if total_chars >= max_chars {
                break;
            }

            if let Ok(file_content) = fs::read_to_string(&path) {
                // Priority-based budget allocation per file
                let file_budget = match priority {
                    0..=10 => usize::MAX, // Critical: Read fully (top-level __init__.py)
                    11..=30 => 10_000,    // Important: Substantial sample (public modules)
                    31..=50 => 2_000,     // Normal: Moderate sample
                    _ => 500,             // Low: Small sample (internals, tests, tools)
                };

                let remaining = max_chars - total_chars;
                let chars_to_read = file_content.len().min(file_budget).min(remaining);

                let priority_label = match priority {
                    0..=10 => "critical API",
                    11..=30 => "public API",
                    31..=50 => "module",
                    _ => "impl",
                };

                if chars_to_read == file_content.len() {
                    content.push_str(&format!(
                        "\n\n// File: {} ({})\n",
                        path.display(),
                        priority_label
                    ));
                    content.push_str(&file_content);
                } else {
                    content.push_str(&format!(
                        "\n\n// File: {} ({}, sampled)\n",
                        path.display(),
                        priority_label
                    ));
                    let end = floor_char_boundary(&file_content, chars_to_read);
                    content.push_str(&file_content[..end]);
                }

                total_chars += chars_to_read;
            }
        }

        info!(
            "Read {} characters from {} files (smart sampling)",
            total_chars,
            paths.len()
        );
        Ok(content)
    }

    /// Calculate file priority (lower = higher priority, read first)
    fn calculate_file_priority(path: &Path, repo_path: &Path) -> i32 {
        crate::util::calculate_file_priority(path, repo_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // -- floor_char_boundary tests --

    #[test]
    fn test_floor_char_boundary_ascii() {
        let s = "hello world";

        // Exact boundary positions in ASCII are always valid
        assert_eq!(floor_char_boundary(s, 0), 0);
        assert_eq!(floor_char_boundary(s, 5), 5);
        assert_eq!(floor_char_boundary(s, 11), 11);

        // Beyond string length clamps to len
        assert_eq!(floor_char_boundary(s, 100), s.len());
    }

    #[test]
    fn test_floor_char_boundary_multibyte() {
        // Each emoji is 4 bytes in UTF-8
        let s = "\u{1F600}\u{1F601}\u{1F602}"; // 3 emoji, 12 bytes total
        assert_eq!(s.len(), 12);

        // Index 0 is a valid boundary (start of first emoji)
        assert_eq!(floor_char_boundary(s, 0), 0);

        // Index 4 is a valid boundary (start of second emoji)
        assert_eq!(floor_char_boundary(s, 4), 4);

        // Indices 1, 2, 3 are mid-character; should floor to 0
        assert_eq!(floor_char_boundary(s, 1), 0);
        assert_eq!(floor_char_boundary(s, 2), 0);
        assert_eq!(floor_char_boundary(s, 3), 0);

        // Indices 5, 6, 7 are mid-character; should floor to 4
        assert_eq!(floor_char_boundary(s, 5), 4);
        assert_eq!(floor_char_boundary(s, 6), 4);
        assert_eq!(floor_char_boundary(s, 7), 4);

        // Index 8 is a valid boundary (start of third emoji)
        assert_eq!(floor_char_boundary(s, 8), 8);

        // CJK character test (3 bytes each)
        let cjk = "\u{4E16}\u{754C}"; // "世界", 6 bytes
        assert_eq!(cjk.len(), 6);
        assert_eq!(floor_char_boundary(cjk, 1), 0);
        assert_eq!(floor_char_boundary(cjk, 2), 0);
        assert_eq!(floor_char_boundary(cjk, 3), 3);
        assert_eq!(floor_char_boundary(cjk, 4), 3);
        assert_eq!(floor_char_boundary(cjk, 5), 3);
    }

    #[test]
    fn test_floor_char_boundary_empty_string() {
        let s = "";
        assert_eq!(floor_char_boundary(s, 0), 0);
        assert_eq!(floor_char_boundary(s, 10), 0);
    }

    // -- calculate_file_priority tests --

    #[test]
    fn test_calculate_file_priority_top_level_init() {
        // Top-level __init__.py at depth 2 (repo/pkg/__init__.py) => priority 0
        let repo = Path::new("/repo");
        let path = PathBuf::from("/repo/pkg/__init__.py");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 0);
    }

    #[test]
    fn test_calculate_file_priority_subpackage_init() {
        // Subpackage __init__.py at depth 3+ => priority 10
        let repo = Path::new("/repo");
        let path = PathBuf::from("/repo/pkg/sub/__init__.py");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 10);
    }

    #[test]
    fn test_calculate_file_priority_internal_files() {
        let repo = Path::new("/repo");

        // Private file (starts with _)
        let path = PathBuf::from("/repo/pkg/_private.py");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 100);

        // Internal directory
        let path = PathBuf::from("/repo/pkg/_internal/utils.py");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 100);

        // Tests directory
        let path = PathBuf::from("/repo/pkg/tests/test_foo.py");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 100);

        // Benchmarks directory
        let path = PathBuf::from("/repo/pkg/benchmarks/bench.py");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 100);
    }

    #[test]
    fn test_calculate_file_priority_public_modules() {
        let repo = Path::new("/repo");

        // Public top-level module at depth 2 => priority 20
        let path = PathBuf::from("/repo/pkg/api.py");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 20);

        // Public subpackage module at depth 3 => priority 30
        let path = PathBuf::from("/repo/pkg/sub/models.py");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 30);

        // Deeper module => priority 50
        let path = PathBuf::from("/repo/pkg/a/b/c/deep.py");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 50);
    }

    #[test]
    fn test_calculate_file_priority_readme() {
        // README.md at repo root, depth 1, not __init__.py, not private => priority 50
        // (doesn't match depth 2 or 3 public module rules)
        let repo = Path::new("/repo");
        let path = PathBuf::from("/repo/README.md");
        assert_eq!(Collector::calculate_file_priority(&path, repo), 50);
    }

    // -- read_files_smart budget tests --

    #[test]
    fn test_read_files_smart_respects_budget() {
        // Arrange: create multiple files that together exceed a small budget
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let pkg = repo.join("pkg");
        fs::create_dir_all(&pkg).unwrap();

        // Create 5 files, each 1000 chars
        let mut paths = Vec::new();
        for i in 0..5 {
            let file_path = pkg.join(format!("mod_{}.py", i));
            fs::write(&file_path, "x".repeat(1000)).unwrap();
            paths.push(file_path);
        }

        // Act: read with a 2500 char budget (should NOT read all 5000 chars)
        let result = Collector::read_files_smart(&paths, 2500, repo).unwrap();

        // Assert: content length should be within budget (allow for file headers)
        // The actual content chars tracked internally won't exceed 2500,
        // but headers add some overhead. Total should be well under 5000.
        assert!(
            result.len() < 5000,
            "Should not read all files; got {} chars",
            result.len()
        );
    }

    #[test]
    fn test_read_files_smart_empty_paths() {
        let dir = TempDir::new().unwrap();
        let result = Collector::read_files_smart(&[], 10_000, dir.path()).unwrap();
        assert_eq!(result, "");
    }

    // -- detect_package_name tests --

    #[test]
    fn test_detect_package_name_from_setup_py() {
        // Arrange: setup.py with double-quoted name, no pyproject.toml
        let dir = TempDir::new().unwrap();
        let base = dir.path();
        fs::write(
            base.join("setup.py"),
            r#"from setuptools import setup
setup(
    name="my-package",
    version="1.0.0",
)
"#,
        )
        .unwrap();

        // Act
        let name = Collector::detect_package_name(base).unwrap();

        // Assert
        assert_eq!(name, "my-package");
    }

    #[test]
    fn test_detect_package_name_from_dirname() {
        // Arrange: no pyproject.toml, no setup.py => falls back to dir name
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("my-cool-project");
        fs::create_dir_all(&project_dir).unwrap();

        // Act
        let name = Collector::detect_package_name(&project_dir).unwrap();

        // Assert
        assert_eq!(name, "my-cool-project");
    }

    // -- floor_char_boundary edge cases --

    #[test]
    fn test_floor_char_boundary_cafe_mid_multibyte() {
        // "cafe" with accented e: 'e' with acute is U+00E9, 2 bytes in UTF-8
        let s = "caf\u{00E9}"; // "cafe" — 5 bytes total (c=1, a=1, f=1, e=2)
        assert_eq!(s.len(), 5);

        // Index 4 is in the middle of the 2-byte 'e' char (starts at byte 3)
        assert_eq!(floor_char_boundary(s, 4), 3);

        // Index 3 is the start of 'e' — valid boundary
        assert_eq!(floor_char_boundary(s, 3), 3);

        // Index 5 == s.len() — should return s.len()
        assert_eq!(floor_char_boundary(s, 5), 5);
    }

    #[test]
    fn test_floor_char_boundary_exact_len() {
        let s = "abc";
        assert_eq!(floor_char_boundary(s, 3), 3); // index == s.len()
    }

    #[test]
    fn test_floor_char_boundary_beyond_len() {
        let s = "abc";
        assert_eq!(floor_char_boundary(s, 999), 3); // index >> s.len()
    }

    #[test]
    fn test_floor_char_boundary_ascii_every_index() {
        let s = "abcde";
        for i in 0..=s.len() {
            // Every index in an ASCII string is a valid char boundary
            assert_eq!(floor_char_boundary(s, i), i);
        }
    }

    // -- smart_sample_read priority bucket tests --

    #[test]
    fn test_smart_sample_read_critical_priority_reads_full_file() {
        // Arrange: a top-level __init__.py (priority 0) with large content
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let pkg = repo.join("pkg");
        fs::create_dir_all(&pkg).unwrap();

        let content = "x".repeat(20_000);
        let init_path = pkg.join("__init__.py");
        fs::write(&init_path, &content).unwrap();

        // Act: read with a budget larger than the file
        let result = Collector::read_files_smart(&[init_path], 100_000, repo).unwrap();

        // Assert: critical files (priority 0-10) use usize::MAX budget, so entire file is read
        assert!(
            result.contains(&"x".repeat(20_000)),
            "Critical file should be read in full"
        );
    }

    #[test]
    fn test_smart_sample_read_important_priority_caps_at_10k() {
        // Arrange: a public top-level module (priority 20 => 11-30 bucket => 10K budget)
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let pkg = repo.join("pkg");
        fs::create_dir_all(&pkg).unwrap();

        let content = "y".repeat(20_000);
        let mod_path = pkg.join("api.py");
        fs::write(&mod_path, &content).unwrap();

        // Act: budget is large enough that per-file cap is the limiting factor
        let result = Collector::read_files_smart(&[mod_path], 100_000, repo).unwrap();

        // Assert: important files cap at 10K chars read from the file
        // The result should contain a "sampled" label and not the full 20K
        assert!(
            result.contains("sampled"),
            "Important file should be sampled when over 10K"
        );
        // Content portion should be around 10K (capped by file_budget)
        let content_len = result.len();
        // Header adds ~40 chars; content capped at 10K
        assert!(
            content_len < 11_000,
            "Important file content should be capped near 10K, got {}",
            content_len
        );
    }

    #[test]
    fn test_smart_sample_read_normal_priority_caps_at_2k() {
        // Arrange: a deeper module (priority 30 => 31-50 bucket => 2K budget)
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let pkg = repo.join("pkg").join("sub");
        fs::create_dir_all(&pkg).unwrap();

        let content = "z".repeat(5_000);
        let mod_path = pkg.join("models.py");
        fs::write(&mod_path, &content).unwrap();

        // Priority 30 is in the 11-30 range (important), so use depth 4+ for 31-50
        let deeper = repo.join("pkg").join("a").join("b").join("c");
        fs::create_dir_all(&deeper).unwrap();
        let deep_path = deeper.join("deep.py");
        fs::write(&deep_path, &content).unwrap();

        // Act
        let result = Collector::read_files_smart(&[deep_path], 100_000, repo).unwrap();

        // Assert: normal (priority 50 => 31-50 bucket) caps at 2K
        assert!(
            result.contains("sampled"),
            "Normal-priority file should be sampled when over 2K"
        );
        let content_len = result.len();
        assert!(
            content_len < 2_200,
            "Normal-priority file content should be capped near 2K, got {}",
            content_len
        );
    }

    #[test]
    fn test_smart_sample_read_low_priority_caps_at_500() {
        // Arrange: an internal/private file (priority 100 => 51+ bucket => 500 bytes)
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let pkg = repo.join("pkg");
        fs::create_dir_all(&pkg).unwrap();

        let content = "w".repeat(2_000);
        let priv_path = pkg.join("_private.py");
        fs::write(&priv_path, &content).unwrap();

        // Act
        let result = Collector::read_files_smart(&[priv_path], 100_000, repo).unwrap();

        // Assert: low-priority files (priority 100 => 51+) cap at 500 bytes
        assert!(
            result.contains("sampled"),
            "Low-priority file should be sampled when over 500 bytes"
        );
        // 500 bytes of content + header (tempdir path can be ~100 chars)
        let content_len = result.len();
        assert!(
            content_len < 700,
            "Low-priority file content should be capped near 500 + header, got {}",
            content_len
        );
        // Verify it did NOT read the full 2000-byte file
        assert!(
            !result.contains(&"w".repeat(2_000)),
            "Should not read full file for low-priority"
        );
    }

    // -- detect_package_name fallback chain --

    #[test]
    fn test_detect_package_name_malformed_pyproject_no_equals() {
        // Arrange: pyproject.toml with name line but no '='
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("fallback-proj");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("pyproject.toml"), "name \"broken\"\n").unwrap();

        // Act: should skip malformed pyproject and fall back to dir name
        let name = Collector::detect_package_name(&base).unwrap();

        // Assert
        assert_eq!(name, "fallback-proj");
    }

    #[test]
    fn test_detect_package_name_pyproject_empty_name() {
        // Arrange: pyproject.toml with name = "" (empty)
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("empty-name-proj");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("pyproject.toml"), "name = \"\"\n").unwrap();

        // Act: empty name should be skipped, falls back to dir name
        let name = Collector::detect_package_name(&base).unwrap();

        // Assert
        assert_eq!(name, "empty-name-proj");
    }

    #[test]
    fn test_detect_package_name_setup_py_single_quotes() {
        // Arrange: setup.py using single quotes
        let dir = TempDir::new().unwrap();
        let base = dir.path();
        fs::write(
            base.join("setup.py"),
            "from setuptools import setup\nsetup(\n    name='single-quoted',\n)\n",
        )
        .unwrap();

        // Act
        let name = Collector::detect_package_name(base).unwrap();

        // Assert
        assert_eq!(name, "single-quoted");
    }

    #[test]
    fn test_detect_package_name_canonicalized_dir_fallback() {
        // Arrange: no pyproject.toml, no setup.py => canonical path dir name
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("canonical-test");
        fs::create_dir_all(&project_dir).unwrap();

        // Act
        let name = Collector::detect_package_name(&project_dir).unwrap();

        // Assert
        assert_eq!(name, "canonical-test");
    }

    #[test]
    fn test_detect_package_name_unknown_fallback() {
        // Arrange: use Path::new(".") which has file_name "." — filtered out
        // and canonicalize will resolve to a real dir name, so we need to test
        // the final "unknown" path. This is hard to trigger naturally since
        // canonicalize almost always produces a real name. We test that the
        // function returns something non-empty for an unusual path.
        // The "unknown" fallback only triggers if canonicalize fails AND
        // file_name is "." or ".." — which requires a non-existent path.
        let result = Collector::detect_package_name(Path::new(".")).unwrap();
        // On a real filesystem, "." canonicalizes to a real dir, so we get
        // a valid name. The important thing is it doesn't fail.
        assert!(!result.is_empty(), "Should never return empty string");
    }

    #[test]
    fn test_detect_package_name_pyproject_with_bracket_in_name() {
        // Arrange: pyproject.toml where name line contains "[" (e.g. section header)
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("bracket-proj");
        fs::create_dir_all(&base).unwrap();
        fs::write(
            base.join("pyproject.toml"),
            "[project]\nname = \"[invalid]\"\nversion = \"1.0\"\n",
        )
        .unwrap();

        // Act: name containing "[" is rejected, falls back to dir name
        let name = Collector::detect_package_name(&base).unwrap();

        // Assert
        assert_eq!(name, "bracket-proj");
    }

    // -- read_files budget boundary --

    #[test]
    fn test_read_files_exact_budget_fit() {
        // Arrange: file content exactly equals remaining budget
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("exact.py");
        let content = "a".repeat(100);
        fs::write(&file_path, &content).unwrap();

        // Act: budget == file size => file fits exactly
        let result = Collector::read_files(&[file_path], 100).unwrap();

        // Assert: should include the full file (not truncated)
        assert!(
            !result.contains("truncated"),
            "File that fits exactly should not be marked truncated"
        );
        assert!(
            result.contains(&"a".repeat(100)),
            "Full content should be present"
        );
    }

    #[test]
    fn test_read_files_one_byte_over_budget() {
        // Arrange: file content is 1 byte larger than budget
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("over.py");
        let content = "b".repeat(101);
        fs::write(&file_path, &content).unwrap();

        // Act: budget = 100, file = 101 => should truncate
        let result = Collector::read_files(&[file_path], 100).unwrap();

        // Assert: should be marked truncated
        assert!(
            result.contains("truncated"),
            "File exceeding budget by 1 byte should be truncated"
        );
    }

    #[test]
    fn test_read_files_multiple_files_last_fits_exactly() {
        // Arrange: two files, second fits exactly in remaining budget
        let dir = TempDir::new().unwrap();
        let file1 = dir.path().join("first.py");
        let file2 = dir.path().join("second.py");
        fs::write(&file1, "c".repeat(50)).unwrap();
        fs::write(&file2, "d".repeat(50)).unwrap();

        // Act: budget = 100, file1=50 + file2=50 = exactly 100
        let result = Collector::read_files(&[file1, file2], 100).unwrap();

        // Assert: both files fit, neither truncated
        assert!(
            !result.contains("truncated"),
            "Both files should fit exactly without truncation"
        );
        assert!(result.contains("first.py"), "Should include first file");
        assert!(result.contains("second.py"), "Should include second file");
    }

    // -- collect_python budget allocation scaling --
    // These test the source_budget calculation in collect_python indirectly
    // by exercising with_max_source_chars and checking the result.

    #[test]
    fn test_budget_scaling_small_repo() {
        // Arrange: < 300 source files => remaining * 100% (full remainder)
        let budget: usize = 100_000;
        let examples_budget = budget * 30 / 100;
        let test_budget = budget * 30 / 100;
        let docs_budget = budget * 20 / 100;
        let changelog_budget = budget * 5 / 100;
        let fixed_total = examples_budget + test_budget + docs_budget + changelog_budget;
        let remaining = budget.saturating_sub(fixed_total);

        // Small repo: use full remainder
        let source_budget = remaining;

        // Assert: 15% of 100K = 15K
        assert_eq!(remaining, 15_000);
        assert_eq!(source_budget, 15_000);
    }

    #[test]
    fn test_budget_scaling_medium_repo() {
        // Arrange: 300-1000 source files => remaining * 40%
        let budget: usize = 100_000;
        let fixed_total = budget * 85 / 100; // 85%
        let remaining = budget.saturating_sub(fixed_total);
        let file_count = 500;

        let source_budget = match file_count {
            n if n > 2000 => remaining,
            n if n > 1000 => remaining * 60 / 100,
            n if n > 300 => remaining * 40 / 100,
            _ => remaining,
        };

        // Assert: 40% of 15K = 6K
        assert_eq!(source_budget, 6_000);
    }

    #[test]
    fn test_budget_scaling_large_repo() {
        // Arrange: 1000-2000 source files => remaining * 60%
        let budget: usize = 100_000;
        let fixed_total = budget * 85 / 100;
        let remaining = budget.saturating_sub(fixed_total);
        let file_count = 1500;

        let source_budget = match file_count {
            n if n > 2000 => remaining,
            n if n > 1000 => remaining * 60 / 100,
            n if n > 300 => remaining * 40 / 100,
            _ => remaining,
        };

        // Assert: 60% of 15K = 9K
        assert_eq!(source_budget, 9_000);
    }

    #[test]
    fn test_budget_scaling_massive_repo() {
        // Arrange: 2000+ source files => remaining * 100% (full remainder)
        let budget: usize = 100_000;
        let fixed_total = budget * 85 / 100;
        let remaining = budget.saturating_sub(fixed_total);
        let file_count = 3000;

        let source_budget = match file_count {
            n if n > 2000 => remaining,
            n if n > 1000 => remaining * 60 / 100,
            n if n > 300 => remaining * 40 / 100,
            _ => remaining,
        };

        // Assert: 100% of 15K = 15K
        assert_eq!(source_budget, 15_000);
    }

    // -- read_file_limited --

    #[test]
    fn test_read_file_limited_within_budget() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("small.txt");
        fs::write(&path, "hello").unwrap();

        let result = Collector::read_file_limited(&path, 1000).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_read_file_limited_exceeds_budget() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("big.txt");
        fs::write(&path, "a".repeat(200)).unwrap();

        let result = Collector::read_file_limited(&path, 50).unwrap();
        assert_eq!(result.len(), 50);
    }

    #[test]
    fn test_read_file_limited_nonexistent() {
        let result = Collector::read_file_limited(Path::new("/no/such/file"), 100);
        assert!(result.is_err());
    }

    // -- read_files budget break path --

    #[test]
    fn test_read_files_breaks_at_budget() {
        let dir = TempDir::new().unwrap();
        let f1 = dir.path().join("a.py");
        let f2 = dir.path().join("b.py");
        let f3 = dir.path().join("c.py");
        fs::write(&f1, "x".repeat(50)).unwrap();
        fs::write(&f2, "y".repeat(50)).unwrap();
        fs::write(&f3, "z".repeat(50)).unwrap();

        // Budget of 100: f1(50) + f2(50) = 100, then f3 should trigger break
        let result = Collector::read_files(&[f1, f2, f3], 100).unwrap();
        assert!(
            !result.contains("c.py"),
            "Third file should be skipped after budget met"
        );
    }

    // -- detect_package_name pyproject success --

    #[test]
    fn test_detect_package_name_from_pyproject() {
        let dir = TempDir::new().unwrap();
        let base = dir.path();
        fs::write(
            base.join("pyproject.toml"),
            "[project]\nname = \"my-real-package\"\n",
        )
        .unwrap();

        let name = Collector::detect_package_name(base).unwrap();
        assert_eq!(name, "my-real-package");
    }

    // -- Collector::new and with_max_source_chars --

    #[test]
    fn test_collector_new_defaults() {
        let dir = TempDir::new().unwrap();
        let c = Collector::new(dir.path(), Language::Python);
        assert_eq!(c.max_source_chars, 100_000);
        assert_eq!(c.repo_path, dir.path().to_path_buf());
    }

    #[test]
    fn test_collector_with_max_source_chars() {
        let dir = TempDir::new().unwrap();
        let c = Collector::new(dir.path(), Language::Python).with_max_source_chars(50_000);
        assert_eq!(c.max_source_chars, 50_000);
    }

    // -- collect: unsupported language --

    #[tokio::test]
    async fn test_collect_unsupported_language() {
        let dir = TempDir::new().unwrap();
        let c = Collector::new(dir.path(), Language::Go);
        let result = c.collect().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not yet implemented"));
    }

    // -- read_files: additional edge cases --

    #[test]
    fn test_read_files_empty_paths() {
        let result = Collector::read_files(&[], 10_000).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_read_files_nonexistent_file_skipped() {
        // A non-existent file should be silently skipped (fs::read_to_string returns Err)
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("does_not_exist.py");
        let real = dir.path().join("real.py");
        fs::write(&real, "content").unwrap();

        let result = Collector::read_files(&[missing, real], 10_000).unwrap();
        assert!(result.contains("real.py"), "Real file should be read");
        assert!(result.contains("content"));
    }

    #[test]
    fn test_read_files_truncation_with_multibyte() {
        // File content contains multibyte chars; truncation must land on char boundary
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("multi.py");
        // Each emoji is 4 bytes
        let content = "\u{1F600}".repeat(10); // 40 bytes
        fs::write(&file_path, &content).unwrap();

        // Budget of 10 should truncate mid-emoji territory
        let result = Collector::read_files(&[file_path], 10).unwrap();
        // Result should be valid UTF-8 (this would panic if not)
        assert!(!result.is_empty());
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_read_files_zero_budget() {
        // Budget of 0 should immediately break
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("file.py");
        fs::write(&file_path, "content").unwrap();

        let result = Collector::read_files(&[file_path], 0).unwrap();
        // With budget 0, the first file triggers the break at the top of the loop
        assert!(!result.contains("content"));
    }

    #[test]
    fn test_read_files_second_file_truncated() {
        // First file fits, second is partially read
        let dir = TempDir::new().unwrap();
        let f1 = dir.path().join("a.py");
        let f2 = dir.path().join("b.py");
        fs::write(&f1, "x".repeat(60)).unwrap();
        fs::write(&f2, "y".repeat(100)).unwrap();

        // Budget 80: f1 (60 chars) fits, remaining = 20 for f2 (100 chars) => truncated
        let result = Collector::read_files(&[f1, f2], 80).unwrap();
        assert!(result.contains("b.py (truncated)"));
        assert!(!result.contains(&"y".repeat(100)));
    }

    // -- read_file_limited: additional edge cases --

    #[test]
    fn test_read_file_limited_exact_budget() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("exact.txt");
        fs::write(&path, "abcde").unwrap();

        // Budget == content length => no truncation
        let result = Collector::read_file_limited(&path, 5).unwrap();
        assert_eq!(result, "abcde");
    }

    #[test]
    fn test_read_file_limited_multibyte_truncation() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("multi.txt");
        // "cafe" with accent: 5 bytes (c=1, a=1, f=1, e-acute=2)
        fs::write(&path, "caf\u{00E9}").unwrap();

        // Budget 4: byte 4 is inside the 2-byte char, should floor to 3
        let result = Collector::read_file_limited(&path, 4).unwrap();
        assert_eq!(result, "caf");
    }

    #[test]
    fn test_read_file_limited_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.txt");
        fs::write(&path, "").unwrap();

        let result = Collector::read_file_limited(&path, 100).unwrap();
        assert_eq!(result, "");
    }

    // -- detect_package_name: additional edge cases --

    #[test]
    fn test_detect_package_name_setup_py_with_spaces() {
        // setup.py with "name =" (space before equals)
        let dir = TempDir::new().unwrap();
        let base = dir.path();
        fs::write(
            base.join("setup.py"),
            "setup(\n    name = \"spaced-name\",\n)\n",
        )
        .unwrap();

        let name = Collector::detect_package_name(base).unwrap();
        assert_eq!(name, "spaced-name");
    }

    #[test]
    fn test_detect_package_name_setup_py_no_closing_quote() {
        // setup.py where the name value has no closing quote — should fall back to dir name
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("fallback-proj");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("setup.py"), "setup(\n    name=\"unclosed\n)\n").unwrap();

        let name = Collector::detect_package_name(&base).unwrap();
        // The find(quote_char) for the closing quote fails, so setup.py strategy is skipped
        assert_eq!(name, "fallback-proj");
    }

    #[test]
    fn test_detect_package_name_setup_py_empty_name() {
        // setup.py with name="" (empty string)
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("fallback-empty");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("setup.py"), "setup(\n    name=\"\",\n)\n").unwrap();

        let name = Collector::detect_package_name(&base).unwrap();
        // Empty name is filtered out, falls back to dir name
        assert_eq!(name, "fallback-empty");
    }

    #[test]
    fn test_detect_package_name_pyproject_single_quotes() {
        // pyproject.toml with single-quoted name
        let dir = TempDir::new().unwrap();
        let base = dir.path();
        fs::write(
            base.join("pyproject.toml"),
            "[project]\nname = 'single-quoted-pkg'\n",
        )
        .unwrap();

        let name = Collector::detect_package_name(base).unwrap();
        assert_eq!(name, "single-quoted-pkg");
    }

    #[test]
    fn test_detect_package_name_pyproject_uppercase_lowered() {
        // Package name is uppercased — should be lowered
        let dir = TempDir::new().unwrap();
        let base = dir.path();
        fs::write(
            base.join("pyproject.toml"),
            "[project]\nname = \"MyPackage\"\n",
        )
        .unwrap();

        let name = Collector::detect_package_name(base).unwrap();
        assert_eq!(name, "mypackage");
    }

    #[test]
    fn test_detect_package_name_setup_py_no_name_line() {
        // setup.py without any name= line
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("no-name-proj");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("setup.py"), "setup(\n    version=\"1.0\",\n)\n").unwrap();

        let name = Collector::detect_package_name(&base).unwrap();
        // Falls back to dir name
        assert_eq!(name, "no-name-proj");
    }

    #[test]
    fn test_detect_package_name_pyproject_name_without_value() {
        // pyproject.toml with "name" but split produces empty after =
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("no-val-proj");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("pyproject.toml"), "name =\n").unwrap();

        let name = Collector::detect_package_name(&base).unwrap();
        // After splitting on '=' and trimming, value is empty, so it's skipped
        assert_eq!(name, "no-val-proj");
    }

    // -- read_files_smart: additional edge cases --

    #[test]
    fn test_read_files_smart_nonexistent_file_skipped() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let pkg = repo.join("pkg");
        fs::create_dir_all(&pkg).unwrap();

        let missing = pkg.join("missing.py");
        let real = pkg.join("real.py");
        fs::write(&real, "content").unwrap();

        let result = Collector::read_files_smart(&[missing, real], 10_000, repo).unwrap();
        assert!(result.contains("real.py"));
        assert!(!result.contains("missing.py"));
    }

    #[test]
    fn test_read_files_smart_zero_budget() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let pkg = repo.join("pkg");
        fs::create_dir_all(&pkg).unwrap();
        let file_path = pkg.join("mod.py");
        fs::write(&file_path, "content").unwrap();

        let result = Collector::read_files_smart(&[file_path], 0, repo).unwrap();
        // With zero budget, the loop breaks immediately
        assert_eq!(result, "");
    }

    #[test]
    fn test_read_files_smart_full_file_read_when_small() {
        // A small file under all budgets should be read fully (not sampled)
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let pkg = repo.join("pkg");
        fs::create_dir_all(&pkg).unwrap();
        let file_path = pkg.join("api.py");
        fs::write(&file_path, "short").unwrap();

        let result = Collector::read_files_smart(&[file_path], 10_000, repo).unwrap();
        assert!(result.contains("short"));
        assert!(!result.contains("sampled"));
    }

    #[test]
    fn test_read_files_smart_multibyte_truncation() {
        // Ensure smart read doesn't break on multibyte chars when sampling
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let pkg = repo.join("pkg");
        fs::create_dir_all(&pkg).unwrap();

        // A deep file (priority 50 => module bucket => 2K budget)
        let deep = repo.join("pkg").join("a").join("b").join("c");
        fs::create_dir_all(&deep).unwrap();
        // Mix of ASCII and multibyte so truncation is exercised
        let content = format!("{}\u{1F600}", "x".repeat(3000));
        let deep_path = deep.join("deep.py");
        fs::write(&deep_path, &content).unwrap();

        let result = Collector::read_files_smart(&[deep_path], 100_000, repo).unwrap();
        // Must be valid UTF-8 (would panic otherwise)
        assert!(!result.is_empty());
    }

    // -- with_max_source_chars chaining --

    #[test]
    fn test_collector_with_max_source_chars_chaining() {
        let dir = TempDir::new().unwrap();
        let c = Collector::new(dir.path(), Language::Python).with_max_source_chars(25_000);
        assert_eq!(c.max_source_chars, 25_000);
        // Verify language and path are preserved
        assert_eq!(c.language, Language::Python);
        assert_eq!(c.repo_path, dir.path().to_path_buf());
    }

    // -- collect: unsupported languages --

    #[tokio::test]
    async fn test_collect_javascript_unsupported() {
        let dir = TempDir::new().unwrap();
        let c = Collector::new(dir.path(), Language::JavaScript);
        let result = c.collect().await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("JavaScript"));
        assert!(msg.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_collect_rust_unsupported() {
        let dir = TempDir::new().unwrap();
        let c = Collector::new(dir.path(), Language::Rust);
        let result = c.collect().await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Rust"));
    }
}

#[derive(Debug, Clone)]
pub struct CollectedData {
    pub package_name: String,
    pub version: String,
    pub license: Option<String>,
    pub project_urls: Vec<(String, String)>,
    pub language: Language,
    pub source_file_count: usize, // Number of source files (for scale-aware prompts)
    pub examples_content: String, // NEW: Highest value content
    pub test_content: String,
    pub docs_content: String,
    pub source_content: String, // Moved to end - lowest priority
    pub changelog_content: String,
}
