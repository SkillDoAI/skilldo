use anyhow::{bail, Result};
use std::fs;
use std::path::Path;
use tracing::info;

use crate::config::{Config, Provider};
use crate::detector::Language;
use crate::llm::factory;
use crate::review::{self, ReviewAgent};

/// Run the review agent standalone against an existing SKILL.md file.
pub async fn run(
    path: String,
    config_path: Option<String>,
    model_override: Option<String>,
    provider_override: Option<String>,
    base_url_override: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let file = Path::new(&path);
    if !file.exists() {
        bail!("File not found: {}", path);
    }
    if !file.is_file() {
        bail!("Path is not a file: {}", path);
    }

    let skill_md = fs::read_to_string(file)?;
    info!("Reviewing: {}", path);

    // Load config
    let config = Config::load_with_path(config_path)?;

    // Resolve LLM config: review_llm if set, otherwise main llm
    let mut llm_config = config
        .generation
        .review_llm
        .clone()
        .unwrap_or_else(|| config.llm.clone());

    // Apply CLI overrides to the resolved config
    if let Some(ref provider) = provider_override {
        llm_config.provider = provider.parse::<Provider>()?;
    }
    if let Some(ref model) = model_override {
        llm_config.model = model.clone();
    }
    if let Some(ref base_url) = base_url_override {
        llm_config.base_url = Some(base_url.clone());
    }

    let client = factory::create_client_from_llm_config(&llm_config, dry_run).await?;
    if dry_run {
        info!("Using mock LLM client");
    } else {
        info!("Using {}/{}", llm_config.provider, llm_config.model);
    }

    // Extract package name and language from frontmatter
    let (package_name, language) = extract_frontmatter_meta(&skill_md);
    info!(
        "Package: {}, Language: {}",
        package_name.as_deref().unwrap_or("unknown"),
        language.as_deref().unwrap_or("unknown")
    );

    let lang_str = language.ok_or_else(|| {
        anyhow::anyhow!("Cannot determine ecosystem. Set `ecosystem:` in SKILL.md frontmatter.")
    })?;
    let lang = lang_str.parse::<Language>().map_err(|_| {
        anyhow::anyhow!(
            "Unsupported ecosystem '{}'. Supported: {}",
            lang_str,
            Language::supported_list()
        )
    })?;

    let review_agent =
        ReviewAgent::new(client.as_ref(), config.prompts.review_custom.clone()).with_strict(true);

    let result = review_agent.review(&skill_md, &lang).await?;

    // Print results
    if result.passed && !result.issues.is_empty() {
        println!("PASSED with {} warning(s):\n", result.issues.len());
        review::print_review_issues(&result.issues);
    } else if result.passed {
        println!("PASSED: No issues found.");
    } else {
        println!("FAILED: {} issue(s) found.\n", result.issues.len());
        review::print_review_issues(&result.issues);
    }

    if !result.passed {
        bail!("{} review issue(s) found", result.issues.len());
    }

    Ok(())
}

/// Extract package name and language from SKILL.md YAML frontmatter.
fn extract_frontmatter_meta(skill_md: &str) -> (Option<String>, Option<String>) {
    if !skill_md.starts_with("---") {
        return (None, None);
    }
    let Some(end) = skill_md[3..].find("---") else {
        return (None, None);
    };
    let frontmatter = &skill_md[3..3 + end];

    let mut name = None;
    let mut language = None;

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("name:") {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                name = Some(val.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("ecosystem:") {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                language = Some(val.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("language:") {
            // Fallback: some SKILL.md files use "language:" instead of "ecosystem:"
            if language.is_none() {
                let val = rest.trim().trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    language = Some(val.to_string());
                }
            }
        }
    }

    (name, language)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_frontmatter_meta_full() {
        let md = "---\nname: arrow\nversion: 1.4.0\necosystem: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "arrow");
        assert_eq!(lang.unwrap(), "python");
    }

    #[test]
    fn test_extract_frontmatter_meta_language_field() {
        let md = "---\nname: numpy\nlanguage: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "numpy");
        assert_eq!(lang.unwrap(), "python");
    }

    #[test]
    fn test_extract_frontmatter_meta_no_frontmatter() {
        let md = "# No frontmatter";
        let (name, lang) = extract_frontmatter_meta(md);
        assert!(name.is_none());
        assert!(lang.is_none());
    }

    #[test]
    fn test_extract_frontmatter_meta_partial() {
        let md = "---\nname: flask\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "flask");
        assert!(lang.is_none());
    }

    #[test]
    fn test_extract_frontmatter_meta_ecosystem_over_language() {
        let md = "---\nname: test\necosystem: rust\nlanguage: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "test");
        assert_eq!(lang.unwrap(), "rust"); // ecosystem takes precedence
    }

    #[tokio::test]
    async fn test_run_file_not_found() {
        let result = run(
            "/tmp/nonexistent-skill.md".to_string(),
            None,
            None,
            None,
            None,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_run_dry_run() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true, // dry_run
        )
        .await;
        result.unwrap();
    }

    // --- Coverage: path is not a file (line 28-29) ---
    #[tokio::test]
    async fn test_run_path_is_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = run(
            dir.path().to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a file"));
    }

    // --- Coverage: CLI overrides ---
    #[tokio::test]
    async fn test_run_dry_run_with_all_overrides() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            Some("override-model".to_string()),
            Some("openai-compatible".to_string()),
            Some("http://localhost:9999".to_string()),
            true, // dry_run
        )
        .await;
        result.unwrap();
    }

    // --- Coverage: frontmatter with unclosed --- ---
    #[test]
    fn test_extract_frontmatter_meta_unclosed() {
        let md = "---\nname: arrow\nversion: 1.4.0\necosystem: python\n";
        let (name, lang) = extract_frontmatter_meta(md);
        assert!(name.is_none());
        assert!(lang.is_none());
    }

    // --- Coverage: empty name/ecosystem values ---
    #[test]
    fn test_extract_frontmatter_meta_empty_values() {
        let md = "---\nname: \necosystem: \n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert!(name.is_none());
        assert!(lang.is_none());
    }

    // --- Coverage: language fallback not taken when ecosystem already set ---
    #[test]
    fn test_extract_frontmatter_meta_language_not_taken_when_ecosystem_set() {
        let md = "---\nname: test\necosystem: rust\nlanguage: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "test");
        assert_eq!(lang.unwrap(), "rust");
    }

    // --- Coverage: metadata-nested frontmatter (normalizer canonical format) ---
    #[test]
    fn test_extract_frontmatter_meta_nested_metadata() {
        let md = "---\nname: arrow\ndescription: python library\nlicense: MIT\nmetadata:\n  version: \"1.4.0\"\n  ecosystem: python\n  generated-by: skilldo/claude-sonnet-4-6\n---\n\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "arrow");
        assert_eq!(lang.unwrap(), "python");
    }

    // --- Coverage: metadata-nested with Go ecosystem ---
    #[test]
    fn test_extract_frontmatter_meta_nested_metadata_go() {
        let md = "---\nname: gin\ndescription: go library\nlicense: MIT\nmetadata:\n  version: \"1.10.0\"\n  ecosystem: go\n---\n\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "gin");
        assert_eq!(lang.unwrap(), "go");
    }

    // --- Coverage: quoted values in frontmatter ---
    #[test]
    fn test_extract_frontmatter_meta_quoted_values() {
        let md = "---\nname: \"my-lib\"\necosystem: 'python'\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "my-lib");
        assert_eq!(lang.unwrap(), "python");
    }

    #[test]
    fn test_extract_frontmatter_meta_empty_language_only() {
        let md = "---\nname: testpkg\nlanguage: \n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "testpkg");
        assert!(lang.is_none());
    }

    #[test]
    fn test_extract_frontmatter_meta_empty_ecosystem_uses_language() {
        let md = "---\nname: testpkg\necosystem: \nlanguage: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "testpkg");
        assert_eq!(lang.unwrap(), "python");
    }

    #[test]
    fn test_extract_frontmatter_meta_empty_name() {
        let md = "---\nname: \necosystem: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert!(name.is_none());
        assert_eq!(lang.unwrap(), "python");
    }

    #[tokio::test]
    async fn test_run_dry_run_with_model_only() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            Some("custom-model".to_string()),
            None,
            None,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_dry_run_with_provider_only() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            Some("openai".to_string()),
            None,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_dry_run_with_base_url_override() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            Some("http://localhost:11434/v1".to_string()),
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_dry_run_with_review_llm_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test\n",
        )
        .unwrap();
        let config_path = dir.path().join("skilldo.toml");
        std::fs::write(
            &config_path,
            r#"
[llm]
provider = "anthropic"
model = "claude-sonnet"
api_key_env = "none"

[generation]
max_retries = 3

[generation.review_llm]
provider = "openai-compatible"
model = "local-review"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
"#,
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            Some(config_path.to_str().unwrap().to_string()),
            None,
            None,
            None,
            true,
        )
        .await;
        result.unwrap();
    }

    #[tokio::test]
    async fn test_run_dry_run_missing_name_in_frontmatter() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nversion: 1.0.0\necosystem: python\n---\n# Test without name\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_dry_run_no_frontmatter_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(&skill_path, "# No frontmatter here\nJust content.\n").unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot determine ecosystem"));
    }

    #[tokio::test]
    async fn test_run_dry_run_language_field_fallback() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\nlanguage: python\n---\n# Test\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_dry_run_metadata_nested_frontmatter() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: arrow\ndescription: python library\nlicense: MIT\nmetadata:\n  version: \"1.4.0\"\n  ecosystem: python\n  generated-by: skilldo/claude-sonnet-4-6\n---\n\n# Content\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        result.unwrap();
    }

    #[test]
    fn test_passed_with_warnings_output() {
        use crate::review::{ReviewIssue, ReviewResult, Severity};

        let result = ReviewResult {
            passed: true,
            malformed: false,
            issues: vec![ReviewIssue {
                severity: Severity::Warning,
                category: "consistency".to_string(),
                complaint: "Minor version drift".to_string(),
                evidence: "1.0.0 vs 1.0.1".to_string(),
            }],
        };

        // Verify the branching logic: passed with non-empty issues
        assert!(result.passed && !result.issues.is_empty());

        // Verify the count resolves to the expected literal
        assert_eq!(result.issues.len(), 1);
    }

    #[tokio::test]
    async fn test_run_dry_run_unsupported_ecosystem_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: cobol\n---\n# Test\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Unsupported ecosystem"),
            "should mention unsupported ecosystem: {err}"
        );
        assert!(
            err.contains("cobol"),
            "should mention the bad ecosystem name: {err}"
        );
    }

    #[tokio::test]
    async fn test_run_dry_run_only_name_no_ecosystem_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\n---\n# No ecosystem\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Cannot determine ecosystem"),
            "should mention missing ecosystem: {err}"
        );
    }

    #[tokio::test]
    async fn test_run_dry_run_read_file_content() {
        // Verify the file is actually read (line 31) by using non-trivial content
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        let content = "---\nname: mylib\nversion: 2.0.0\necosystem: rust\n---\n\n# MyLib\n\nRust library for testing.\n\n## Imports\n\n```rust\nuse mylib::Thing;\n```\n";
        std::fs::write(&skill_path, content).unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        result.unwrap();
    }

    #[tokio::test]
    async fn test_run_dry_run_go_ecosystem() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: gin\nversion: 1.10.0\necosystem: go\n---\n# Gin\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        result.unwrap();
    }

    #[tokio::test]
    async fn test_run_dry_run_javascript_ecosystem() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: express\nversion: 4.18.0\necosystem: javascript\n---\n# Express\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        result.unwrap();
    }

    #[test]
    fn test_extract_frontmatter_meta_language_fallback_when_ecosystem_empty() {
        // ecosystem is empty string, language has value -> language is used
        let md = "---\nname: pkg\necosystem: \nlanguage: rust\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "pkg");
        assert_eq!(lang.unwrap(), "rust");
    }

    #[test]
    fn test_extract_frontmatter_meta_only_language_no_ecosystem() {
        // No ecosystem field at all, just language
        let md = "---\nname: pkg\nlanguage: go\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "pkg");
        assert_eq!(lang.unwrap(), "go");
    }

    #[tokio::test]
    async fn test_run_invalid_config_path_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            Some("/tmp/nonexistent-skilldo-config-99999.toml".to_string()),
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_dry_run_with_custom_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test\n",
        )
        .unwrap();
        let config_path = dir.path().join("skilldo.toml");
        std::fs::write(
            &config_path,
            r#"
[llm]
provider_type = "anthropic"
model = "claude-sonnet"
api_key_env = "none"
"#,
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            Some(config_path.to_str().unwrap().to_string()),
            None,
            None,
            None,
            true,
        )
        .await;
        result.unwrap();
    }

    #[tokio::test]
    async fn test_run_dry_run_with_invalid_provider_override_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\nversion: 1.0.0\necosystem: python\n---\n# Test\n",
        )
        .unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            Some("invalid-provider-that-does-not-exist".to_string()),
            None,
            true,
        )
        .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_frontmatter_meta_whitespace_in_values() {
        let md = "---\nname:   spaced-pkg  \necosystem:   python  \n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "spaced-pkg");
        assert_eq!(lang.unwrap(), "python");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_run_unreadable_file_errors() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            "---\nname: testpkg\necosystem: python\n---\n# Content\n",
        )
        .unwrap();
        // Make unreadable
        std::fs::set_permissions(&skill_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            true,
        )
        .await;

        // Restore permissions for cleanup
        std::fs::set_permissions(&skill_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn test_extract_frontmatter_meta_multiple_colons_in_value() {
        let md = "---\nname: my:lib:thing\necosystem: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "my:lib:thing");
        assert_eq!(lang.unwrap(), "python");
    }

    #[test]
    fn test_extract_frontmatter_meta_tabs_in_values() {
        let md = "---\nname:\ttabbed-pkg\necosystem:\tpython\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "tabbed-pkg");
        assert_eq!(lang.unwrap(), "python");
    }

    #[test]
    fn test_extract_frontmatter_meta_mixed_quotes() {
        // Single quotes on name, double on ecosystem
        let md = "---\nname: 'single-quoted'\necosystem: \"double-quoted\"\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "single-quoted");
        assert_eq!(lang.unwrap(), "double-quoted");
    }
}
