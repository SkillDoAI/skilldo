use anyhow::Result;
use tracing::{debug, info};

/// Analyze changelog to determine if regeneration is needed
#[derive(Debug, Clone)]
pub struct ChangelogAnalyzer {
    changelog_content: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeSignificance {
    /// No API changes - skip regeneration
    Skip,
    /// API changes detected - regenerate needed
    Regenerate,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ChangelogAnalysis {
    pub significance: ChangeSignificance,
    pub reason: String,
    pub changes_found: Vec<String>,
}

impl ChangelogAnalyzer {
    pub fn new(changelog_content: String) -> Self {
        Self { changelog_content }
    }

    /// Annotate changelog lines with classification tags for LLM consumption.
    /// Returns the annotated content (empty string if changelog is empty).
    pub fn annotate_changelog(&self) -> String {
        if self.changelog_content.is_empty() {
            return String::new();
        }
        let mut annotated = Vec::new();
        for line in self.changelog_content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("---") {
                annotated.push(line.to_string());
                continue;
            }
            let lower = trimmed.to_lowercase();
            if self.is_breaking_change(&lower) {
                annotated.push(format!("[BREAKING] {}", trimmed));
            } else if self.is_new_feature(&lower) {
                annotated.push(format!("[NEW API] {}", trimmed));
            } else if self.is_deprecation(&lower) {
                annotated.push(format!("[DEPRECATED] {}", trimmed));
            } else if self.is_behavior_change(&lower) {
                annotated.push(format!("[BEHAVIOR CHANGE] {}", trimmed));
            } else {
                annotated.push(line.to_string());
            }
        }
        annotated.join("\n")
    }

    /// Analyze changelog between two versions
    #[allow(dead_code)]
    pub fn analyze_between_versions(
        &self,
        old_version: &str,
        new_version: &str,
    ) -> Result<ChangelogAnalysis> {
        info!(
            "Analyzing changelog from {} to {}",
            old_version, new_version
        );

        // Extract changes between versions
        let changes = self.extract_changes_between(old_version, new_version);

        if changes.is_empty() {
            return Ok(ChangelogAnalysis {
                significance: ChangeSignificance::Skip,
                reason: "No changelog entries found between versions".to_string(),
                changes_found: vec![],
            });
        }

        // Classify changes
        let mut breaking_changes = Vec::new();
        let mut new_features = Vec::new();
        let mut deprecations = Vec::new();
        let mut behavior_changes = Vec::new();
        let mut bug_fixes = Vec::new();

        for change in &changes {
            let lower = change.to_lowercase();

            // Breaking changes (highest priority)
            if self.is_breaking_change(&lower) {
                breaking_changes.push(change.clone());
            }
            // New APIs/features
            else if self.is_new_feature(&lower) {
                new_features.push(change.clone());
            }
            // Deprecations
            else if self.is_deprecation(&lower) {
                deprecations.push(change.clone());
            }
            // Behavior changes
            else if self.is_behavior_change(&lower) {
                behavior_changes.push(change.clone());
            }
            // Bug fixes (lowest priority)
            else if self.is_bug_fix(&lower) {
                bug_fixes.push(change.clone());
            }
        }

        // Decision logic
        let needs_regen = !breaking_changes.is_empty()
            || !new_features.is_empty()
            || !deprecations.is_empty()
            || !behavior_changes.is_empty();

        if needs_regen {
            let mut reasons = Vec::new();
            if !breaking_changes.is_empty() {
                reasons.push(format!("{} breaking change(s)", breaking_changes.len()));
            }
            if !new_features.is_empty() {
                reasons.push(format!("{} new feature(s)", new_features.len()));
            }
            if !deprecations.is_empty() {
                reasons.push(format!("{} deprecation(s)", deprecations.len()));
            }
            if !behavior_changes.is_empty() {
                reasons.push(format!("{} behavior change(s)", behavior_changes.len()));
            }

            info!("Regeneration needed: {}", reasons.join(", "));

            Ok(ChangelogAnalysis {
                significance: ChangeSignificance::Regenerate,
                reason: format!("API changes detected: {}", reasons.join(", ")),
                changes_found: changes,
            })
        } else {
            info!(
                "Only {} bug fix(es) found - skipping regeneration",
                bug_fixes.len()
            );

            Ok(ChangelogAnalysis {
                significance: ChangeSignificance::Skip,
                reason: format!(
                    "Only {} non-API changes (bug fixes, docs, internal)",
                    bug_fixes.len()
                ),
                changes_found: changes,
            })
        }
    }

    /// Extract changelog entries between two versions
    fn extract_changes_between(&self, old_version: &str, new_version: &str) -> Vec<String> {
        let mut changes = Vec::new();
        let mut in_version_section = false;
        let mut found_new_version = false;

        for line in self.changelog_content.lines() {
            let trimmed = line.trim();

            // Detect version headers (e.g., "## 2.2.0", "# v2.2.0", "[2.2.0]")
            if self.is_version_header(trimmed, new_version) {
                in_version_section = true;
                found_new_version = true;
                continue;
            }

            // Stop at old version
            if self.is_version_header(trimmed, old_version) {
                break;
            }

            // Collect changes in the version section
            if in_version_section && !trimmed.is_empty() && !trimmed.starts_with('#') {
                // Skip date lines and separator lines
                if !trimmed.starts_with("---") && !self.looks_like_date(trimmed) {
                    changes.push(trimmed.to_string());
                }
            }
        }

        if !found_new_version {
            debug!("Version {} not found in changelog", new_version);
        }

        changes
    }

    /// Check if line is a version header
    fn is_version_header(&self, line: &str, version: &str) -> bool {
        let patterns = [
            format!("## {}", version),
            format!("# {}", version),
            format!("## v{}", version),
            format!("# v{}", version),
            format!("[{}]", version),
            format!("Version {}", version),
        ];

        patterns.iter().any(|p| line.starts_with(p))
    }

    /// Check if line looks like a date
    fn looks_like_date(&self, line: &str) -> bool {
        line.contains("2024")
            || line.contains("2025")
            || line.contains("2026")
            || line.contains("Jan")
            || line.contains("Feb")
            || line.contains("Mar")
    }

    /// Detect breaking changes
    fn is_breaking_change(&self, text: &str) -> bool {
        text.contains("breaking")
            || text.contains("removed")
            || text.contains("incompatible")
            || text.contains("no longer")
            || text.contains("changed behavior")
            || text.contains("must now")
    }

    /// Detect new features/APIs
    fn is_new_feature(&self, text: &str) -> bool {
        (text.contains("added") || text.contains("new") || text.contains("introduce"))
            && (text.contains("api")
                || text.contains("function")
                || text.contains("method")
                || text.contains("class")
                || text.contains("module"))
    }

    /// Detect deprecations
    fn is_deprecation(&self, text: &str) -> bool {
        text.contains("deprecat") || text.contains("will be removed")
    }

    /// Detect behavior changes
    fn is_behavior_change(&self, text: &str) -> bool {
        text.contains("now returns")
            || text.contains("now accepts")
            || text.contains("changed to")
            || text.contains("default changed")
    }

    /// Detect bug fixes (skip these)
    fn is_bug_fix(&self, text: &str) -> bool {
        text.contains("fix")
            || text.contains("bug")
            || text.contains("issue")
            || text.contains("correct")
            || text.contains("patch")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breaking_change_detection() {
        let changelog = r#"
## 2.2.0

- BREAKING: Removed torch.jit.script API
- Fixed memory leak
- Added new torch.compile() function
"#;

        let analyzer = ChangelogAnalyzer::new(changelog.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.2.0").unwrap();

        assert_eq!(analysis.significance, ChangeSignificance::Regenerate);
        assert!(analysis.reason.contains("breaking"));
    }

    #[test]
    fn test_only_bug_fixes_skip() {
        let changelog = r#"
## 2.1.1

- Fixed memory leak in optimizer
- Corrected documentation typo
- Patched edge case bug
"#;

        let analyzer = ChangelogAnalyzer::new(changelog.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.1.1").unwrap();

        assert_eq!(analysis.significance, ChangeSignificance::Skip);
    }

    #[test]
    fn test_new_api_regenerate() {
        let changelog = r#"
## 2.2.0

- Added new torch.compile() API for optimization
- Fixed minor bugs
"#;

        let analyzer = ChangelogAnalyzer::new(changelog.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.2.0").unwrap();

        assert_eq!(analysis.significance, ChangeSignificance::Regenerate);
        assert!(analysis.reason.contains("new feature"));
    }

    #[test]
    fn test_deprecation_regenerate() {
        let changelog = r#"
## 2.2.0

- Deprecated torch.nn.functional.relu6 (use relu instead)
- Performance improvements
"#;

        let analyzer = ChangelogAnalyzer::new(changelog.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.2.0").unwrap();

        assert_eq!(analysis.significance, ChangeSignificance::Regenerate);
        assert!(analysis.reason.contains("deprecation"));
    }

    #[test]
    fn test_annotate_changelog_tags() {
        let changelog = r#"## 2.2.0

- BREAKING: Removed torch.jit.script API
- Added new torch.compile() function for optimization
- Deprecated torch.nn.functional.relu6 (use relu instead)
- The default now returns a dict instead of tuple
- Fixed memory leak in optimizer"#;
        let analyzer = ChangelogAnalyzer::new(changelog.to_string());
        let annotated = analyzer.annotate_changelog();
        assert!(annotated.contains("[BREAKING] - BREAKING: Removed"));
        assert!(annotated.contains("[NEW API] - Added new torch.compile() function"));
        assert!(annotated.contains("[DEPRECATED] - Deprecated torch.nn.functional"));
        assert!(annotated.contains("[BEHAVIOR CHANGE] - The default now returns"));
        // Bug fix lines pass through without tag
        assert!(annotated.contains("- Fixed memory leak in optimizer"));
        assert!(!annotated.contains("[BREAKING] - Fixed"));
    }

    #[test]
    fn test_annotate_changelog_empty() {
        let analyzer = ChangelogAnalyzer::new(String::new());
        assert_eq!(analyzer.annotate_changelog(), "");
    }

    #[test]
    fn test_annotate_changelog_unannotated_passthrough() {
        let changelog = "- Performance improvements\n- Internal refactoring";
        let analyzer = ChangelogAnalyzer::new(changelog.to_string());
        let annotated = analyzer.annotate_changelog();
        assert_eq!(annotated, changelog);
    }

    #[test]
    fn test_version_not_found() {
        let changelog = r#"
## 2.1.0

- Some changes
"#;

        let analyzer = ChangelogAnalyzer::new(changelog.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.2.0").unwrap();

        assert_eq!(analysis.significance, ChangeSignificance::Skip);
    }

    /// Covers line 108 (behavior_changes branch) and line 134 (behavior change reason string).
    #[test]
    fn test_behavior_change_regenerate() {
        let changelog = r#"
## 2.2.0

- The function now returns a list instead of a tuple
- Internal cleanup
"#;

        let analyzer = ChangelogAnalyzer::new(changelog.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.2.0").unwrap();

        assert_eq!(analysis.significance, ChangeSignificance::Regenerate);
        assert!(analysis.reason.contains("behavior change"));
    }

    /// Covers line 147 (bug_fixes.len() in Skip branch) by having recognized bug fix entries
    /// that do NOT match any higher-priority classification.
    #[test]
    fn test_bug_fix_count_in_skip_reason() {
        let changelog = r#"
## 2.1.1

- Fixed crash on empty input
- Corrected off-by-one bug in parser
"#;

        let analyzer = ChangelogAnalyzer::new(changelog.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.1.1").unwrap();

        assert_eq!(analysis.significance, ChangeSignificance::Skip);
        assert!(analysis.reason.contains("2 non-API changes"));
    }

    /// Covers lines 237-239 (is_new_feature: "method", "class", "module" branches).
    #[test]
    fn test_new_feature_method_class_module() {
        let changelog_method = r#"
## 2.2.0

- Added new .fit() method for streamlined training
"#;
        let analyzer = ChangelogAnalyzer::new(changelog_method.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.2.0").unwrap();
        assert_eq!(analysis.significance, ChangeSignificance::Regenerate);
        assert!(analysis.reason.contains("new feature"));

        let changelog_class = r#"
## 2.2.0

- Introduced DataFrameWriter class for batch output
"#;
        let analyzer = ChangelogAnalyzer::new(changelog_class.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.2.0").unwrap();
        assert_eq!(analysis.significance, ChangeSignificance::Regenerate);
        assert!(analysis.reason.contains("new feature"));

        let changelog_module = r#"
## 2.2.0

- Added new utilities module for helper functions
"#;
        let analyzer = ChangelogAnalyzer::new(changelog_module.to_string());
        let analysis = analyzer.analyze_between_versions("2.1.0", "2.2.0").unwrap();
        assert_eq!(analysis.significance, ChangeSignificance::Regenerate);
        assert!(analysis.reason.contains("new feature"));
    }
}
