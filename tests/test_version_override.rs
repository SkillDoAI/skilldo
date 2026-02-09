// TDD: Version override CLI argument tests
// Test FIRST, then implement!

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

// Test helper to create a fake Python repo with version in pyproject.toml
fn create_test_repo_with_version(version: &str) -> Result<TempDir> {
    let temp = TempDir::new()?;
    let pyproject = temp.path().join("pyproject.toml");
    fs::write(
        &pyproject,
        format!(
            r#"
[project]
name = "testpkg"
version = "{}"
"#,
            version
        ),
    )?;

    // Add a minimal source file
    let src_dir = temp.path().join("testpkg");
    fs::create_dir(&src_dir)?;
    fs::write(src_dir.join("__init__.py"), "# Test package\n")?;

    Ok(temp)
}

// Test helper to create a Git repo with tags
fn create_git_repo_with_tag(tag: &str) -> Result<TempDir> {
    let temp = create_test_repo_with_version("0.1.0")?;

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(temp.path())
        .output()?;

    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp.path())
        .output()?;

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp.path())
        .output()?;

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(temp.path())
        .output()?;

    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(temp.path())
        .output()?;

    std::process::Command::new("git")
        .args(["tag", tag])
        .current_dir(temp.path())
        .output()?;

    Ok(temp)
}

#[test]
fn test_version_explicit_override() {
    // GIVEN: A repo with version 1.0.0 in pyproject.toml
    let repo = create_test_repo_with_version("1.0.0").unwrap();

    // WHEN: We pass --version "2.5.0" explicitly
    // THEN: The extracted version should be "2.5.0", NOT "1.0.0"

    // This test will fail until we implement the feature!
    let extracted =
        skilldo::cli::version::extract_version(repo.path(), Some("2.5.0".to_string()), None)
            .unwrap();

    assert_eq!(
        extracted, "2.5.0",
        "Explicit version should override package metadata"
    );
}

#[test]
fn test_version_from_git_tag() {
    // GIVEN: A Git repo with tag "v1.2.3"
    let repo = create_git_repo_with_tag("v1.2.3").unwrap();

    // WHEN: We pass --version-from git-tag
    // THEN: Should extract "1.2.3" from the tag (strip 'v' prefix)

    let extracted =
        skilldo::cli::version::extract_version(repo.path(), None, Some("git-tag".to_string()))
            .unwrap();

    assert_eq!(extracted, "1.2.3", "Should extract version from git tag");
}

#[test]
fn test_version_from_package_default() {
    // GIVEN: A repo with version 3.4.5 in pyproject.toml
    let repo = create_test_repo_with_version("3.4.5").unwrap();

    // WHEN: We don't pass any version args (default behavior)
    // THEN: Should extract from package metadata

    let extracted = skilldo::cli::version::extract_version(repo.path(), None, None).unwrap();

    assert_eq!(extracted, "3.4.5", "Should extract from package by default");
}

#[test]
fn test_version_from_branch() {
    // GIVEN: A Git repo on branch "feature/awesome-stuff"
    let repo = create_git_repo_with_tag("v1.0.0").unwrap();

    std::process::Command::new("git")
        .args(["checkout", "-b", "feature/awesome-stuff"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    // WHEN: We pass --version-from branch
    // THEN: Should return "branch-feature-awesome-stuff"

    let extracted =
        skilldo::cli::version::extract_version(repo.path(), None, Some("branch".to_string()))
            .unwrap();

    assert_eq!(
        extracted, "branch-feature-awesome-stuff",
        "Should sanitize branch name"
    );
}

#[test]
fn test_version_from_commit() {
    // GIVEN: A Git repo with commits
    let repo = create_git_repo_with_tag("v1.0.0").unwrap();

    // WHEN: We pass --version-from commit
    // THEN: Should return "dev-<short-sha>"

    let extracted =
        skilldo::cli::version::extract_version(repo.path(), None, Some("commit".to_string()))
            .unwrap();

    assert!(extracted.starts_with("dev-"), "Should start with 'dev-'");
    assert_eq!(extracted.len(), 11, "Should be 'dev-' + 7-char SHA");
}

#[test]
fn test_explicit_version_overrides_version_from() {
    // GIVEN: A Git repo with tag "v5.0.0"
    let repo = create_git_repo_with_tag("v5.0.0").unwrap();

    // WHEN: We pass BOTH --version "9.9.9" AND --version-from git-tag
    // THEN: Explicit --version should win

    let extracted = skilldo::cli::version::extract_version(
        repo.path(),
        Some("9.9.9".to_string()),
        Some("git-tag".to_string()),
    )
    .unwrap();

    assert_eq!(extracted, "9.9.9", "Explicit version should always win");
}

#[test]
fn test_version_from_package_when_specified() {
    // GIVEN: A repo with version in package metadata
    let repo = create_test_repo_with_version("7.8.9").unwrap();

    // WHEN: We explicitly pass --version-from package
    // THEN: Should extract from package metadata

    let extracted =
        skilldo::cli::version::extract_version(repo.path(), None, Some("package".to_string()))
            .unwrap();

    assert_eq!(extracted, "7.8.9", "Should extract from package");
}

#[test]
fn test_version_from_invalid_source() {
    // GIVEN: A repo
    let repo = create_test_repo_with_version("1.0.0").unwrap();

    // WHEN: We pass --version-from invalid-source
    // THEN: Should return an error

    let result = skilldo::cli::version::extract_version(
        repo.path(),
        None,
        Some("invalid-source".to_string()),
    );

    assert!(result.is_err(), "Should error on invalid version source");
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Unknown version source"));
}

#[test]
fn test_fallback_to_unknown_when_no_package_metadata() {
    // GIVEN: A repo with NO version metadata
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src/lib.rs"), "// Empty\n").unwrap();

    // WHEN: We try to extract version (default = from package)
    // THEN: Should fall back to "unknown"

    let extracted = skilldo::cli::version::extract_version(temp.path(), None, None).unwrap();

    assert_eq!(extracted, "unknown", "Should fallback to 'unknown'");
}
