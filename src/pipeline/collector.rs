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
    /// Based on python.rs file_priority logic but language-agnostic
    fn calculate_file_priority(path: &Path, repo_path: &Path) -> i32 {
        let path_str = path.to_str().unwrap_or("");
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let relative = path.strip_prefix(repo_path).unwrap_or(path);
        let depth = relative.components().count();

        // Priority 0: Top-level package __init__.py (tensorflow/__init__.py)
        if file_name == "__init__.py" && depth == 2 {
            return 0;
        }

        // Priority 10: Subpackage __init__.py files (tensorflow/keras/__init__.py)
        if file_name == "__init__.py" && depth > 2 {
            return 10;
        }

        // Priority 100: Skip internal/private files (read last if at all)
        if file_name.starts_with('_')
            || path_str.contains("/_internal/")
            || path_str.contains("/_impl/")
            || path_str.contains("/testing/")
            || path_str.contains("/tests/")
            || path_str.contains("/benchmarks/")
            || path_str.contains("/tools/")
            || path_str.contains("/scripts/")
        {
            return 100;
        }

        // Priority 20: Public top-level modules (tensorflow/nn.py)
        if !file_name.starts_with('_') && depth == 2 {
            return 20;
        }

        // Priority 30: Public subpackage modules (tensorflow/keras/layers.py)
        if !file_name.starts_with('_') && depth == 3 {
            return 30;
        }

        // Priority 50: Everything else (deeper submodules)
        50
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
