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
}

impl Collector {
    pub fn new(repo_path: &Path, language: Language) -> Self {
        Self {
            repo_path: repo_path.to_path_buf(),
            language,
        }
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

        // Smart token budget allocation (total ~100K chars = ~25K tokens)
        // Priority: examples > tests > docs > source (read best stuff first)
        //
        // Budget allocation:
        // - 30% examples (30K chars) - Real usage patterns
        // - 30% tests (30K chars) - API usage in tests
        // - 20% docs (20K chars) - Documentation and tutorials
        // - 15-50% source (15K-100K chars) - Public API (__init__.py, main modules)
        //   * Scales dynamically based on project size
        // - 5% changelog (5K chars) - Version history
        //
        // This ensures large frameworks get their BEST content analyzed
        let examples_content = Self::read_files(&example_paths, 30_000)?;
        let test_content = Self::read_files(&test_paths, 30_000)?;
        let docs_content = Self::read_files(&doc_paths, 20_000)?;

        // Dynamic source budget based on project scale
        let source_budget = match source_paths.len() {
            n if n > 2000 => 100_000, // Massive (TensorFlow, PyTorch, Django)
            n if n > 1000 => 50_000,  // Very large
            n if n > 300 => 30_000,   // Large
            _ => 15_000,              // Medium/Small (current)
        };
        let source_content = Self::read_files_smart(&source_paths, source_budget, &self.repo_path)?;
        let changelog_content = if let Some(path) = changelog_path {
            Self::read_file_limited(&path, 5_000)?
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
