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
    provider_override: Option<String>,
    base_url_override: Option<String>,
    max_retries_override: Option<usize>,
    test_model_override: Option<String>,
    test_provider_override: Option<String>,
    no_test: bool,
    test_mode_override: Option<String>,
    runtime_override: Option<String>,
    timeout_override: Option<u64>,
    install_source_override: Option<String>,
    source_path_override: Option<String>,
    no_parallel: bool,
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
    if let Some(ref provider) = provider_override {
        info!("CLI override: provider = {}", provider);
        config.llm.provider = provider.clone();
    }
    if let Some(ref model) = model_override {
        info!("CLI override: model = {}", model);
        config.llm.model = model.clone();
    }
    if let Some(ref base_url) = base_url_override {
        info!("CLI override: base_url = {}", base_url);
        config.llm.base_url = Some(base_url.clone());
    }
    if let Some(retries) = max_retries_override {
        info!("CLI override: max_retries = {}", retries);
        config.generation.max_retries = retries;
    }
    if no_test {
        info!("CLI override: test agent disabled");
        config.generation.enable_test = false;
        if test_model_override.is_some()
            || test_provider_override.is_some()
            || test_mode_override.is_some()
        {
            tracing::warn!(
                "--no-test is set; --test-model/--test-provider/--test-mode will have no effect"
            );
        }
    } else if let Some(ref mode) = test_mode_override {
        info!("CLI override: test_mode = {}", mode);
        config.generation.test_mode = mode.clone();
    }
    if let Some(ref runtime) = runtime_override {
        info!("CLI override: runtime = {}", runtime);
        config.generation.container.runtime = runtime.clone();
    }
    if let Some(timeout) = timeout_override {
        info!("CLI override: timeout = {}s", timeout);
        config.generation.container.timeout = timeout;
    }
    if let Some(ref source) = install_source_override {
        info!("CLI override: install_source = {}", source);
        config.generation.container.install_source = source.clone();
    }
    if let Some(ref path) = source_path_override {
        info!("CLI override: source_path = {}", path);
        config.generation.container.source_path = Some(path.clone());
    }
    if no_parallel {
        info!("CLI override: parallel_extraction = false");
        config.generation.parallel_extraction = false;
    }

    // Default source_path to the repo path (not CWD) for local install/mount modes
    if config.generation.container.source_path.is_none()
        && config.generation.container.install_source != "registry"
    {
        let abs_path = repo_path
            .canonicalize()
            .unwrap_or_else(|_| repo_path.to_path_buf());
        info!(
            "Defaulting source_path to repo path: {}",
            abs_path.display()
        );
        config.generation.container.source_path = Some(abs_path.to_string_lossy().to_string());
    }

    // Test agent model/provider CLI overrides (skip if test agent is disabled)
    if config.generation.enable_test
        && (test_model_override.is_some() || test_provider_override.is_some())
    {
        let mut test_llm = config.generation.test_llm.take().unwrap_or_else(|| {
            // Start from main LLM config if no test_llm configured
            config.llm.clone()
        });
        if let Some(ref model) = test_model_override {
            info!("CLI override: test model = {}", model);
            test_llm.model = model.clone();
        }
        if let Some(ref provider) = test_provider_override {
            info!("CLI override: test provider = {}", provider);
            test_llm.provider = provider.clone();
        }
        config.generation.test_llm = Some(test_llm);
    }

    // Log per-stage override info so users know what's being used
    if model_override.is_some() || provider_override.is_some() {
        let stage_overrides: Vec<String> = [
            config
                .generation
                .extract_llm
                .as_ref()
                .map(|c| format!("extract: {}/{}", c.provider, c.model)),
            config
                .generation
                .map_llm
                .as_ref()
                .map(|c| format!("map: {}/{}", c.provider, c.model)),
            config
                .generation
                .learn_llm
                .as_ref()
                .map(|c| format!("learn: {}/{}", c.provider, c.model)),
            config
                .generation
                .create_llm
                .as_ref()
                .map(|c| format!("create: {}/{}", c.provider, c.model)),
            config
                .generation
                .review_llm
                .as_ref()
                .map(|c| format!("review: {}/{}", c.provider, c.model)),
            config
                .generation
                .test_llm
                .as_ref()
                .map(|c| format!("test: {}/{}", c.provider, c.model)),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !stage_overrides.is_empty() {
            info!("Per-stage LLM overrides from config (not affected by --model/--provider):");
            for o in &stage_overrides {
                info!("  {}", o);
            }
        }
    }

    // Collect files
    info!("Collecting files...");
    let collector = Collector::new(repo_path, detected_language)
        .with_max_source_chars(config.generation.max_source_tokens);
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

    // Create per-stage LLM clients if configured
    let mut generator = Generator::new(client, config.generation.max_retries);
    if let Some(ref extract_config) = config.generation.extract_llm {
        let client = factory::create_client_from_llm_config(extract_config, dry_run)?;
        info!(
            "Using {} for extract: {}",
            extract_config.provider, extract_config.model
        );
        generator = generator.with_extract_client(client);
    }
    if let Some(ref map_config) = config.generation.map_llm {
        let client = factory::create_client_from_llm_config(map_config, dry_run)?;
        info!(
            "Using {} for map: {}",
            map_config.provider, map_config.model
        );
        generator = generator.with_map_client(client);
    }
    if let Some(ref learn_config) = config.generation.learn_llm {
        let client = factory::create_client_from_llm_config(learn_config, dry_run)?;
        info!(
            "Using {} for learn: {}",
            learn_config.provider, learn_config.model
        );
        generator = generator.with_learn_client(client);
    }
    if let Some(ref create_config) = config.generation.create_llm {
        let client = factory::create_client_from_llm_config(create_config, dry_run)?;
        info!(
            "Using {} for create: {}",
            create_config.provider, create_config.model
        );
        generator = generator.with_create_client(client);
    }
    if config.generation.enable_review {
        if let Some(ref review_config) = config.generation.review_llm {
            let client = factory::create_client_from_llm_config(review_config, dry_run)?;
            info!(
                "Using {} for review: {}",
                review_config.provider, review_config.model
            );
            generator = generator.with_review_client(client);
        }
    }
    if config.generation.enable_test {
        if let Some(ref test_config) = config.generation.test_llm {
            let client = factory::create_client_from_llm_config(test_config, dry_run)?;
            info!(
                "Using {} for test: {}",
                test_config.provider, test_config.model
            );
            generator = generator.with_test_client(client);
        }
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
    let model_name = {
        let mut name = config.llm.model.clone();
        let overrides: Vec<String> = [
            config
                .generation
                .extract_llm
                .as_ref()
                .map(|c| format!("extract:{}", c.model)),
            config
                .generation
                .map_llm
                .as_ref()
                .map(|c| format!("map:{}", c.model)),
            config
                .generation
                .learn_llm
                .as_ref()
                .map(|c| format!("learn:{}", c.model)),
            config
                .generation
                .create_llm
                .as_ref()
                .map(|c| format!("create:{}", c.model)),
            config
                .generation
                .review_llm
                .as_ref()
                .map(|c| format!("review:{}", c.model)),
            config
                .generation
                .test_llm
                .as_ref()
                .map(|c| format!("test:{}", c.model)),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !overrides.is_empty() {
            name = format!("{} + {}", name, overrides.join(", "));
        }
        name
    };

    // Generate SKILL.md
    info!("Generating SKILL.md...");
    let mut generator = generator
        .with_model_name(model_name)
        .with_prompts_config(config.prompts.clone())
        .with_test(config.generation.enable_test)
        .with_test_mode(config.generation.get_test_mode())
        .with_review(config.generation.enable_review)
        .with_review_max_retries(config.generation.review_max_retries)
        .with_container_config(config.generation.container.clone())
        .with_parallel_extraction(config.generation.parallel_extraction);

    if let Some(ref skill) = existing_skill {
        generator = generator.with_existing_skill(skill.clone());
    }

    let output_result = generator.generate(&collected_data).await?;

    // Write output
    fs::write(&output, &output_result.skill_md)?;
    info!("✓ Generated SKILL.md written to {}", output);

    // Lint the generated file
    info!("Running linter...");
    let linter = crate::lint::SkillLinter::new();
    let issues = linter.lint(&output_result.skill_md)?;
    linter.print_issues(&issues);

    // Surface unresolved review warnings to the user
    if !output_result.unresolved_warnings.is_empty() {
        println!(
            "\n⚠ Review completed with {} unresolved issue(s):",
            output_result.unresolved_warnings.len()
        );
        for (i, issue) in output_result.unresolved_warnings.iter().enumerate() {
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
        println!("\nThese could not be automatically verified or fixed.");
        println!("Consider adjusting your review prompts via the review_custom config option.");
    }

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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
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
            None,
            None,
            Some(10),
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_err(), "should reject unknown language");
    }

    #[tokio::test]
    async fn test_run_with_provider_override() {
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
            Some("openai".to_string()), // provider
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_base_url_override() {
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
            Some("http://localhost:11434/v1".to_string()), // base_url
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_no_agent5() {
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
            None,
            None,
            None,
            None,
            true, // no_agent5
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_test_mode_override() {
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
            None,
            None,
            None,
            None,
            false,
            Some("minimal".to_string()), // test_mode
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_runtime_override() {
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
            None,
            None,
            None,
            None,
            false,
            None,
            Some("podman".to_string()), // runtime
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_timeout_override() {
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            Some(300), // timeout
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_install_source_override() {
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            Some("local-mount".to_string()), // install_source
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_source_path_override() {
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            Some("/tmp/test".to_string()), // source_path
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_test_model_provider_override() {
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
            None,
            None,
            Some("gpt-5.2".to_string()), // test_model
            Some("openai".to_string()),  // test_provider
            false,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_all_overrides() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            None,
            output.to_str().unwrap().to_string(),
            Some("1.2.3".to_string()),                     // version
            None,                                          // version_from
            None,                                          // config
            Some("gpt-4".to_string()),                     // model
            Some("openai".to_string()),                    // provider
            Some("http://localhost:11434/v1".to_string()), // base_url
            Some(5),                                       // max_retries
            Some("gpt-5.2".to_string()),                   // test_model
            Some("openai".to_string()),                    // test_provider
            true,                                          // no_test
            Some("minimal".to_string()),                   // test_mode
            Some("podman".to_string()),                    // runtime
            Some(300),                                     // timeout
            Some("local-mount".to_string()),               // install_source
            Some("/tmp/test".to_string()),                 // source_path
            true,                                          // no_parallel
            true,                                          // dry_run
        )
        .await;
        assert!(result.is_ok());
    }

    /// Helper to write a config file with per-stage LLM overrides
    fn write_per_stage_config(dir: &Path) -> String {
        let config_path = dir.join("skilldo.toml");
        fs::write(
            &config_path,
            r#"
[llm]
provider = "anthropic"
model = "claude-sonnet"
api_key_env = "none"

[generation]
max_retries = 3
max_source_tokens = 50000
install_source = "registry"

[generation.extract_llm]
provider = "openai-compatible"
model = "local-extract"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.map_llm]
provider = "openai-compatible"
model = "local-map"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.learn_llm]
provider = "openai-compatible"
model = "local-learn"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.create_llm]
provider = "openai-compatible"
model = "local-create"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.review_llm]
provider = "openai-compatible"
model = "local-review"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.test_llm]
provider = "openai-compatible"
model = "local-test"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
"#,
        )
        .unwrap();
        config_path.to_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn test_run_with_config_path() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let config_path = write_per_stage_config(repo.path());
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            None,
            output.to_str().unwrap().to_string(),
            None,
            None,
            Some(config_path),
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(
            result.is_ok(),
            "config path dry run failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_with_per_stage_llm_configs() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let config_path = write_per_stage_config(repo.path());
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            None,
            output.to_str().unwrap().to_string(),
            None,
            None,
            Some(config_path),
            Some("override-model".to_string()),
            Some("openai".to_string()),
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(
            result.is_ok(),
            "per-stage config with model/provider override failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_with_install_source_registry_skips_source_path_default() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        // Config with install_source = "registry" — source_path default should be skipped
        let config_path = repo.path().join("skilldo.toml");
        fs::write(
            &config_path,
            r#"
[llm]
provider = "anthropic"
model = "claude-sonnet"
api_key_env = "none"

[generation]
max_retries = 3
max_source_tokens = 50000

[generation.container]
install_source = "registry"
"#,
        )
        .unwrap();
        let result = run(
            repo.path().to_str().unwrap().to_string(),
            Some("python".to_string()),
            None,
            output.to_str().unwrap().to_string(),
            None,
            None,
            Some(config_path.to_str().unwrap().to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_no_test_and_test_model_only() {
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
            None,
            None,
            Some("gpt-5.2".to_string()), // test_model only, no test_provider
            None,
            true, // no_test — should warn about test_model having no effect
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_test_model_only_no_provider() {
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
            None,
            None,
            Some("gpt-5.2".to_string()), // test_model only
            None,                        // no test_provider
            false,                       // test enabled
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_test_provider_only_no_model() {
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
            None,
            None,
            None,                       // no test_model
            Some("openai".to_string()), // test_provider only
            false,                      // test enabled
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_dry_run_nonexistent_path() {
        let result = run(
            "/tmp/skilldo-nonexistent-path-xyz".to_string(),
            None, // auto-detect should fail on missing path
            None,
            "/tmp/skilldo-nonexistent-output.md".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_err(), "nonexistent repo path should error");
    }

    #[tokio::test]
    async fn test_generate_dry_run_with_output_path() {
        let repo = make_test_repo();
        // Write output to a separate temp dir (not inside the repo)
        let output_dir = TempDir::new().unwrap();
        let output = output_dir.path().join("custom-output/SKILL.md");
        fs::create_dir_all(output.parent().unwrap()).unwrap();
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(
            result.is_ok(),
            "custom output path failed: {:?}",
            result.err()
        );
        assert!(
            output.exists(),
            "SKILL.md should exist at custom output path"
        );
    }

    #[tokio::test]
    async fn test_generate_dry_run_with_no_test_and_test_mode() {
        // Covers the warning branch: no_test=true + test_mode_override set
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
            None,
            None,
            None,
            None,
            true,                        // no_test
            Some("minimal".to_string()), // test_mode — should trigger warning
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_dry_run_with_no_parallel() {
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
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            true, // no_parallel
            true,
        )
        .await;
        assert!(result.is_ok());
    }
}
