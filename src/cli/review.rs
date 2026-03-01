use anyhow::{bail, Result};
use std::fs;
use std::path::Path;
use tracing::info;

use crate::config::Config;
use crate::llm::factory;
use crate::review::ReviewAgent;

/// Run the review agent standalone against an existing SKILL.md file.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    path: String,
    config_path: Option<String>,
    model_override: Option<String>,
    provider_override: Option<String>,
    base_url_override: Option<String>,
    runtime_override: Option<String>,
    timeout_override: Option<u64>,
    no_container: bool,
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
    let mut config = Config::load_with_path(config_path)?;

    // Resolve LLM config: review_llm if set, otherwise main llm
    let mut llm_config = config
        .generation
        .review_llm
        .clone()
        .unwrap_or_else(|| config.llm.clone());

    // Apply CLI overrides to the resolved config
    if let Some(ref provider) = provider_override {
        llm_config.provider = provider.clone();
    }
    if let Some(ref model) = model_override {
        llm_config.model = model.clone();
    }
    if let Some(ref base_url) = base_url_override {
        llm_config.base_url = Some(base_url.clone());
    }
    if let Some(ref runtime) = runtime_override {
        config.generation.container.runtime = runtime.clone();
    }
    if let Some(timeout) = timeout_override {
        config.generation.container.timeout = timeout;
    }

    let client = factory::create_client_from_llm_config(&llm_config, dry_run)?;
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

    let lang = language.as_deref().unwrap_or("python");
    let pkg = package_name.as_deref().unwrap_or("unknown");

    // If --no-container, force non-python path (skips container introspection)
    let effective_lang = if no_container { "unknown" } else { lang };

    let review_agent = ReviewAgent::new(
        client.as_ref(),
        config.generation.container.clone(),
        config.prompts.review_custom.clone(),
    )
    .with_strict(true);

    let result = review_agent.review(&skill_md, pkg, effective_lang).await?;

    // Print results
    if result.passed && !result.issues.is_empty() {
        println!("PASSED with {} warning(s):\n", result.issues.len());
        for (i, issue) in result.issues.iter().enumerate() {
            println!(
                "  {}. [{}][{}] {}",
                i + 1,
                issue.severity,
                issue.category,
                issue.complaint
            );
            if !issue.evidence.is_empty() {
                println!("     Evidence: {}", issue.evidence);
            }
        }
    } else if result.passed {
        println!("PASSED: No issues found.");
    } else {
        println!("FAILED: {} issue(s) found.\n", result.issues.len());
        for (i, issue) in result.issues.iter().enumerate() {
            println!(
                "  {}. [{}][{}] {}",
                i + 1,
                issue.severity,
                issue.category,
                issue.complaint
            );
            if !issue.evidence.is_empty() {
                println!("     Evidence: {}", issue.evidence);
            }
        }
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
            None,
            None,
            false,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_run_dry_run() {
        // Create a temp SKILL.md
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
            None,
            None,
            true, // no_container: no container runtime in test env
            true, // dry_run
        )
        .await;
        // Mock client returns passed=true, so this should succeed
        assert!(result.is_ok(), "dry run review failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_run_dry_run_no_container() {
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
            None,
            None,
            true, // no_container
            true,
        )
        .await;
        assert!(result.is_ok());
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
            None,
            None,
            false,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a file"));
    }

    // --- Coverage: CLI overrides (lines 39, 42, 45, 48, 51) ---
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
            Some("override-model".to_string()), // model_override
            Some("openai-compatible".to_string()), // provider_override
            Some("http://localhost:9999".to_string()), // base_url_override
            Some("podman".to_string()),         // runtime_override
            Some(120),                          // timeout_override
            true,                               // no_container: no container runtime in test env
            true,                               // dry_run
        )
        .await;
        assert!(
            result.is_ok(),
            "dry run with overrides failed: {:?}",
            result.err()
        );
    }

    // --- Coverage: frontmatter with unclosed --- (line 119) ---
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
        // ecosystem is set first; language: should be ignored
        let md = "---\nname: test\necosystem: rust\nlanguage: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "test");
        assert_eq!(lang.unwrap(), "rust");
    }

    // --- Coverage: quoted values in frontmatter ---
    #[test]
    fn test_extract_frontmatter_meta_quoted_values() {
        let md = "---\nname: \"my-lib\"\necosystem: 'python'\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "my-lib");
        assert_eq!(lang.unwrap(), "python");
    }

    // --- Coverage: empty language value with non-empty name ---
    #[test]
    fn test_extract_frontmatter_meta_empty_language_only() {
        let md = "---\nname: testpkg\nlanguage: \n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "testpkg");
        assert!(lang.is_none());
    }

    // --- Coverage: language field used when ecosystem is empty ---
    #[test]
    fn test_extract_frontmatter_meta_empty_ecosystem_uses_language() {
        let md = "---\nname: testpkg\necosystem: \nlanguage: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert_eq!(name.unwrap(), "testpkg");
        assert_eq!(lang.unwrap(), "python");
    }

    // --- Coverage: frontmatter with only name: empty ---
    #[test]
    fn test_extract_frontmatter_meta_empty_name() {
        let md = "---\nname: \necosystem: python\n---\n# Content";
        let (name, lang) = extract_frontmatter_meta(md);
        assert!(name.is_none());
        assert_eq!(lang.unwrap(), "python");
    }

    // --- Coverage: CLI overrides (model only, provider only) ---
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
            None,
            None,
            true, // no_container: no container runtime in test env
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
            None,
            None,
            true, // no_container: no container runtime in test env
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_dry_run_with_runtime_override() {
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
            Some("podman".to_string()),
            None,
            true, // no_container: no container runtime in test env
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_dry_run_with_timeout_override() {
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
            None,
            Some(120),
            true, // no_container: no container runtime in test env
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
            None,
            None,
            true, // no_container: no container runtime in test env
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    // --- Coverage: review_llm config takes precedence over main llm ---
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
            None,
            None,
            true, // no_container: no container runtime in test env
            true,
        )
        .await;
        assert!(
            result.is_ok(),
            "review_llm config failed: {:?}",
            result.err()
        );
    }

    // --- Coverage: no name in frontmatter -> defaults to "unknown" package ---
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
            None,
            None,
            true, // no_container: no container runtime in test env
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    // --- Coverage: no frontmatter defaults to "unknown" package and "python" language ---
    #[tokio::test]
    async fn test_run_dry_run_no_frontmatter() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(&skill_path, "# No frontmatter here\nJust content.\n").unwrap();

        let result = run(
            skill_path.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            true, // no_container: no container runtime in test env
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    // --- Coverage: language field only (no ecosystem) triggers fallback ---
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
            None,
            None,
            true, // no_container: no container runtime in test env
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_passed_with_warnings_output() {
        use crate::review::{ReviewIssue, ReviewResult};

        let result = ReviewResult {
            passed: true,
            issues: vec![ReviewIssue {
                severity: "warning".to_string(),
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
}
