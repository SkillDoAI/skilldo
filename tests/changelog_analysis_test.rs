//! Tests for changelog analysis and regeneration decisions
//! Tests ChangelogAnalyzer pattern detection and decision logic

use anyhow::Result;
use skilldo::changelog::{ChangeSignificance, ChangelogAnalyzer};

#[test]
fn test_changelog_detects_behavior_changes() -> Result<()> {
    let changelog = r#"
## 1.2.4

- API now returns dictionary instead of list
- Timeout default changed from 30s to 60s

## 1.2.3

- Initial release
"#;

    let analyzer = ChangelogAnalyzer::new(changelog.to_string());
    let analysis = analyzer.analyze_between_versions("1.2.3", "1.2.4")?;

    // Should detect behavior changes
    assert_eq!(
        analysis.significance,
        ChangeSignificance::Regenerate,
        "Should regenerate for behavior changes"
    );
    assert!(
        analysis.reason.contains("behavior change"),
        "Reason should mention behavior changes: {}",
        analysis.reason
    );

    Ok(())
}

#[test]
fn test_changelog_detects_new_api_methods() -> Result<()> {
    let changelog = r#"
## 1.1.0

- Added new method `calculate()` to User class
- Introduced function `process_data()` for batch operations

## 1.0.0

- Initial API
"#;

    let analyzer = ChangelogAnalyzer::new(changelog.to_string());
    let analysis = analyzer.analyze_between_versions("1.0.0", "1.1.0")?;

    // Should detect new API additions
    assert_eq!(
        analysis.significance,
        ChangeSignificance::Regenerate,
        "Should regenerate for new methods"
    );
    assert!(
        analysis.reason.contains("new feature"),
        "Should mention new features: {}",
        analysis.reason
    );

    Ok(())
}

#[test]
fn test_changelog_detects_new_module() -> Result<()> {
    let changelog = r#"
## 2.1.0

- New module `auth` for authentication

## 2.0.0

- Major refactor
"#;

    let analyzer = ChangelogAnalyzer::new(changelog.to_string());
    let analysis = analyzer.analyze_between_versions("2.0.0", "2.1.0")?;

    // Should detect new module as new feature
    assert_eq!(
        analysis.significance,
        ChangeSignificance::Regenerate,
        "Should regenerate for new module"
    );

    Ok(())
}

#[test]
fn test_changelog_detects_new_class() -> Result<()> {
    let changelog = r#"
## 3.1.0

- Added new class `DataProcessor` for handling datasets

## 3.0.0

- Bug fixes
"#;

    let analyzer = ChangelogAnalyzer::new(changelog.to_string());
    let analysis = analyzer.analyze_between_versions("3.0.0", "3.1.0")?;

    // Should detect new class as new feature
    assert_eq!(
        analysis.significance,
        ChangeSignificance::Regenerate,
        "Should regenerate for new class"
    );
    assert!(
        analysis.reason.contains("new feature"),
        "Should mention new features: {}",
        analysis.reason
    );

    Ok(())
}

#[test]
fn test_changelog_behavior_change_with_multiple_types() -> Result<()> {
    let changelog = r#"
## 4.1.0

- API now accepts timeout parameter
- Function now returns Promise instead of callback
- New function `configure_timeout()` to customize behavior

## 4.0.0

- Initial release
"#;

    let analyzer = ChangelogAnalyzer::new(changelog.to_string());
    let analysis = analyzer.analyze_between_versions("4.0.0", "4.1.0")?;

    // Should detect both behavior changes and new features
    assert_eq!(
        analysis.significance,
        ChangeSignificance::Regenerate,
        "Should regenerate"
    );
    // Should mention both types
    assert!(
        analysis.reason.contains("behavior") || analysis.reason.contains("feature"),
        "Should mention changes: {}",
        analysis.reason
    );

    Ok(())
}

#[test]
fn test_changelog_counts_multiple_behavior_changes() -> Result<()> {
    let changelog = r#"
## 5.1.0

- Method now returns tuple instead of list
- Function changed to async implementation
- API default changed to use SSL

## 5.0.0

- Initial release
"#;

    let analyzer = ChangelogAnalyzer::new(changelog.to_string());
    let analysis = analyzer.analyze_between_versions("5.0.0", "5.1.0")?;

    // Should detect multiple behavior changes
    assert_eq!(
        analysis.significance,
        ChangeSignificance::Regenerate,
        "Should regenerate for behavior changes"
    );
    assert!(
        analysis.reason.contains("behavior change"),
        "Should mention behavior changes in reason: {}",
        analysis.reason
    );

    Ok(())
}

#[test]
fn test_changelog_skips_only_bug_fixes() -> Result<()> {
    let changelog = r#"
## 6.0.1

- Fixed null pointer exception in data loader
- Resolved memory leak in cache
- Corrected calculation error in stats

## 6.0.0

- Initial release
"#;

    let analyzer = ChangelogAnalyzer::new(changelog.to_string());
    let analysis = analyzer.analyze_between_versions("6.0.0", "6.0.1")?;

    // Should NOT regenerate for only bug fixes
    assert_eq!(
        analysis.significance,
        ChangeSignificance::Skip,
        "Should skip regeneration for only bug fixes"
    );

    Ok(())
}

#[test]
fn test_changelog_with_breaking_changes_and_behavior_changes() -> Result<()> {
    let changelog = r#"
## 8.0.0

- BREAKING: Removed deprecated API endpoints
- API now returns JSON instead of XML

## 7.0.0

- Initial release
"#;

    let analyzer = ChangelogAnalyzer::new(changelog.to_string());
    let analysis = analyzer.analyze_between_versions("7.0.0", "8.0.0")?;

    // Should regenerate for both breaking changes and behavior changes
    assert_eq!(
        analysis.significance,
        ChangeSignificance::Regenerate,
        "Should regenerate"
    );
    assert!(
        analysis.reason.contains("breaking") || analysis.reason.contains("behavior"),
        "Should mention change types: {}",
        analysis.reason
    );

    Ok(())
}
