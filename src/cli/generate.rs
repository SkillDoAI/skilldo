use anyhow::Result;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use tracing::info;

use crate::cli::version;
use crate::config::Config;
use crate::detector::{self, Language};
use crate::llm::factory;
use crate::pipeline::collector::Collector;
use crate::pipeline::generator::Generator;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    path: String,
    language: Option<String>,
    input: Option<String>,
    output: String,
    version_override: Option<String>,
    version_from: Option<String>,
    config_path: Option<String>,
    model_override: Option<String>,
    max_retries_override: Option<usize>,
    dry_run: bool,
) -> Result<()> {
    let repo_path = Path::new(&path);

    // Detect or validate language
    let detected_language = if let Some(lang_str) = language {
        info!("Using specified language: {}", lang_str);
        Language::from_str(&lang_str)?
    } else {
        info!("Auto-detecting language...");
        let lang = detector::detect_language(repo_path)?;
        info!("Detected language: {}", lang.as_str());
        lang
    };

    info!("Repository path: {}", repo_path.display());
    info!("Output: {}", output);
    if let Some(ref cfg) = config_path {
        info!("Config: {}", cfg);
    }
    info!("Dry run: {}", dry_run);

    // Load config (explicit path, repo root, or user config dir)
    let mut config = Config::load_with_path(config_path)?;

    // Apply CLI overrides
    if let Some(ref model) = model_override {
        info!("CLI override: model = {}", model);
        config.llm.model = model.clone();
    }
    if let Some(retries) = max_retries_override {
        info!("CLI override: max_retries = {}", retries);
        config.generation.max_retries = retries;
    }

    // Collect files
    info!("Collecting files...");
    let collector = Collector::new(repo_path, detected_language);
    let mut collected_data = collector.collect().await?;

    // Override version if CLI args provided
    let final_version = version::extract_version(repo_path, version_override, version_from)?;
    collected_data.version = final_version;

    info!(
        "Collected data for package: {} v{}",
        collected_data.package_name, collected_data.version
    );

    // Create LLM client via factory
    let client = factory::create_client(&config, dry_run)?;
    if dry_run {
        info!("Using mock LLM client");
    } else {
        info!("Using {} LLM provider", config.llm.provider);
    }

    // Create separate Agent 5 client if configured
    let mut generator = Generator::new(client, config.generation.max_retries);
    if let Some(ref agent5_config) = config.generation.agent5_llm {
        let agent5_client = factory::create_client_from_llm_config(agent5_config, dry_run)?;
        info!(
            "Using separate {} LLM for Agent 5: {}",
            agent5_config.provider, agent5_config.model
        );
        generator = generator.with_agent5_client(agent5_client);
    }

    // Detect existing SKILL.md for update mode
    let existing_skill = if let Some(ref input_path) = input {
        info!("Update mode: reading existing SKILL.md from {}", input_path);
        Some(fs::read_to_string(input_path)?)
    } else if Path::new(&output).exists() {
        info!("Existing SKILL.md found at {}, updating in-place", output);
        Some(fs::read_to_string(&output)?)
    } else {
        None
    };

    // Build model name for generated_with metadata
    let model_name = if let Some(ref agent5_config) = config.generation.agent5_llm {
        format!("{} + {} (agent5)", config.llm.model, agent5_config.model)
    } else {
        config.llm.model.clone()
    };

    // Generate SKILL.md
    info!("Generating SKILL.md...");
    let mut generator = generator
        .with_model_name(model_name)
        .with_prompts_config(config.prompts.clone())
        .with_agent5(config.generation.enable_agent5)
        .with_agent5_mode(config.generation.get_agent5_mode())
        .with_container_config(config.generation.container.clone());

    if let Some(ref skill) = existing_skill {
        generator = generator.with_existing_skill(skill.clone());
    }

    let skill_md = generator.generate(&collected_data).await?;

    // Write output
    fs::write(&output, &skill_md)?;
    info!("✓ Generated SKILL.md written to {}", output);

    // Lint the generated file
    info!("Running linter...");
    let linter = crate::lint::SkillLinter::new();
    let issues = linter.lint(&skill_md)?;
    linter.print_issues(&issues);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Create a minimal Python repo in a temp dir for dry-run tests
    fn make_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let setup_py = dir.path().join("setup.py");
        fs::write(
            &setup_py,
            r#"from setuptools import setup
setup(name="testpkg", version="1.0.0")
"#,
        )
        .unwrap();
        let pkg_dir = dir.path().join("testpkg");
        fs::create_dir(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join("__init__.py"),
            "def hello():\n    return 'world'\n",
        )
        .unwrap();
        let tests_dir = dir.path().join("tests");
        fs::create_dir(&tests_dir).unwrap();
        fs::write(
            tests_dir.join("test_hello.py"),
            "def test_hello():\n    assert True\n",
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn test_run_dry_run_defaults() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            None, // auto-detect
            None,
            output.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            None,
            true, // dry_run
        )
        .await;
        assert!(result.is_ok(), "dry run failed: {:?}", result.err());
        assert!(output.exists(), "SKILL.md should be written");
        let content = fs::read_to_string(&output).unwrap();
        assert!(content.contains("---"), "should contain frontmatter");
        assert!(!content.is_empty());
    }

    #[tokio::test]
    async fn test_run_explicit_language() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            None,
            output.to_str().unwrap().to_string(),
            None,
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
    async fn test_run_with_version_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            None,
            output.to_str().unwrap().to_string(),
            Some("9.9.9".to_string()),
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(result.is_ok());
        assert!(output.exists());
    }

    #[tokio::test]
    async fn test_run_with_model_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            None,
            output.to_str().unwrap().to_string(),
            None,
            None,
            None,
            Some("custom-model-v1".to_string()),
            None,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_max_retries_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            None,
            output.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            Some(10),
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_update_mode_with_input() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        // Create an existing SKILL.md as input
        let input_path = repo.path().join("old-SKILL.md");
        let mut f = fs::File::create(&input_path).unwrap();
        writeln!(
            f,
            "---\npackage: testpkg\nversion: 0.9.0\n---\n# Old content"
        )
        .unwrap();

        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            Some(input_path.to_str().unwrap().to_string()),
            output.to_str().unwrap().to_string(),
            None,
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
    async fn test_run_update_mode_existing_output() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        // Pre-create output file — run() should detect and use update mode
        fs::write(
            &output,
            "---\npackage: testpkg\nversion: 0.5.0\n---\n# Existing",
        )
        .unwrap();

        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            None, // no explicit input
            output.to_str().unwrap().to_string(),
            None,
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
    async fn test_run_invalid_language() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("brainfuck".to_string()),
            None,
            output.to_str().unwrap().to_string(),
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await;
        assert!(result.is_err(), "should reject unknown language");
    }
}
