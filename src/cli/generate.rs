use anyhow::Result;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::time::Instant;
use tracing::{info, warn};

use crate::cli::version;
use crate::config::{Config, InstallSource, Provider};
use crate::detector::{self, Language};
use crate::llm::factory;
use crate::pipeline::collector::Collector;
use crate::pipeline::generator::Generator;
use crate::review;

/// Options for the `generate` command — replaces 26 positional parameters.
#[derive(Debug, Default)]
pub struct GenerateOptions {
    pub path: String,
    pub language: Option<String>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub version_override: Option<String>,
    pub version_from: Option<crate::config::VersionStrategy>,
    pub config_path: Option<String>,
    pub model_override: Option<String>,
    pub provider_override: Option<String>,
    pub base_url_override: Option<String>,
    pub max_retries_override: Option<usize>,
    pub test_model_override: Option<String>,
    pub test_provider_override: Option<String>,
    pub no_test: bool,
    pub test_mode_override: Option<String>,
    pub no_review: bool,
    pub no_security_scan: bool,
    pub review_model_override: Option<String>,
    pub review_provider_override: Option<String>,
    pub runtime_override: Option<String>,
    pub timeout_override: Option<u64>,
    pub install_source_override: Option<String>,
    pub source_path_override: Option<String>,
    pub container: bool,
    pub request_timeout_override: Option<u64>,
    pub no_parallel: bool,
    pub best_effort: bool,
    pub telemetry: bool,
    pub no_telemetry: bool,
    pub dry_run: bool,
    pub debug_stage_files: Option<String>,
    /// Directory containing cached `1-extract.md`, `2-map.md`, `3-learn.md`
    /// from a prior run. When set, those stages are skipped and their cached
    /// content is used instead — lets you iterate on the create prompt
    /// without repaying for the upstream LLM calls.
    pub replay_from: Option<String>,
}

pub async fn run(opts: GenerateOptions) -> Result<()> {
    let GenerateOptions {
        path,
        language,
        input,
        output,
        version_override,
        version_from,
        config_path,
        model_override,
        provider_override,
        base_url_override,
        max_retries_override,
        test_model_override,
        test_provider_override,
        no_test,
        test_mode_override,
        no_review,
        no_security_scan,
        review_model_override,
        review_provider_override,
        runtime_override,
        timeout_override,
        install_source_override,
        source_path_override,
        container,
        request_timeout_override,
        no_parallel,
        best_effort,
        telemetry,
        no_telemetry,
        dry_run,
        debug_stage_files,
        replay_from,
    } = opts;
    let repo_path = Path::new(&path);

    // If --replay-from is set, load the cached extract/map/learn outputs now
    // so we can fail fast if any are missing.
    let replay_stages: Option<(String, String, String, Option<String>)> =
        if let Some(dir) = replay_from.as_deref() {
            let dir = Path::new(dir);
            if !dir.is_dir() {
                anyhow::bail!("--replay-from: directory does not exist: {}", dir.display());
            }
            let load = |name: &str| -> Result<String> {
                let path = dir.join(name);
                fs::read_to_string(&path).map_err(|e| {
                    anyhow::anyhow!(
                        "--replay-from: failed to read {} — run `skilldo generate` with \
                     `--debug-stage-files {}` first to populate it: {e}",
                        path.display(),
                        dir.display()
                    )
                })
            };
            let api_surface = load("1-extract.md")?;
            let patterns = load("2-map.md")?;
            let context = load("3-learn.md")?;
            // Fact ledger is optional — may not exist if the cache was created
            // before the ledger feature was added.
            let cached_ledger = dir.join("facts.md");
            let fact_ledger = if cached_ledger.exists() {
                let l = fs::read_to_string(&cached_ledger).map_err(|e| {
                    anyhow::anyhow!(
                        "--replay-from: failed to read {}: {e}",
                        cached_ledger.display()
                    )
                })?;
                if !l.trim().is_empty() {
                    info!("Replay mode: loaded fact ledger ({} chars)", l.len());
                    Some(l)
                } else {
                    None
                }
            } else {
                None
            };
            info!(
                "Replay mode: loaded {} + {} + {} chars from {}",
                api_surface.len(),
                patterns.len(),
                context.len(),
                dir.display()
            );
            Some((api_surface, patterns, context, fact_ledger))
        } else {
            None
        };

    // Load config (explicit path, CWD, target repo, git root, or user config dir)
    let mut config = Config::load_with_path_and_repo(config_path, Some(repo_path))?;

    // Resolve defaults: CLI > config > hardcoded
    let output = output
        .or(config.generation.output.clone())
        .unwrap_or_else(|| "SKILL.md".to_string());
    let input = input.or(config.generation.input.clone());
    let language = language.or(config.generation.language.clone());

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
    info!("Dry run: {}", dry_run);

    // Apply CLI overrides
    if let Some(ref provider) = provider_override {
        info!("CLI override: provider = {}", provider);
        let new_provider: Provider = provider.parse()?;
        // When switching providers via CLI, reset api_key_env to the new provider's
        // default if the current value is "none" or empty. Prevents a local config's
        // api_key_env = "none" from blocking a CLI-specified provider that needs a key.
        let current_env = config.llm.api_key_env.as_deref().unwrap_or("");
        if current_env.is_empty() || current_env.eq_ignore_ascii_case("none") {
            config.llm.api_key_env = Some(new_provider.default_api_key_env().to_string());
            info!(
                "CLI override: api_key_env reset to {} (provider changed)",
                new_provider.default_api_key_env()
            );
        }
        config.llm.provider = new_provider;
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
    if let Some(timeout) = request_timeout_override {
        info!("CLI override: request_timeout = {}s", timeout);
        config.llm.request_timeout_secs = timeout;
        // Also apply to stage-specific LLM configs so create_client_from_llm_config picks it up
        for stage_llm in [
            &mut config.generation.extract_llm,
            &mut config.generation.map_llm,
            &mut config.generation.learn_llm,
            &mut config.generation.create_llm,
            &mut config.generation.review_llm,
            &mut config.generation.test_llm,
        ] {
            if let Some(llm) = stage_llm.as_mut() {
                llm.request_timeout_secs = timeout;
            }
        }
    }
    if no_test {
        warn!("CLI override: test agent disabled");
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
        config.generation.container.install_source = source.parse()?;
    }
    if let Some(ref path) = source_path_override {
        info!("CLI override: source_path = {}", path);
        config.generation.container.source_path = Some(path.clone());
    }
    if container {
        info!("CLI override: execution_mode = container");
        config.generation.container.execution_mode = crate::config::ExecutionMode::Container;
    }
    // local-install/local-mount now works with both bare-metal and container modes.
    // No auto-switch needed — bare-metal executors handle path deps natively.
    // Warn if --runtime passed without --container
    if runtime_override.is_some()
        && config.generation.container.execution_mode == crate::config::ExecutionMode::BareMetal
    {
        tracing::warn!("--runtime has no effect without --container");
    }
    if no_parallel {
        info!("CLI override: parallel_extraction = false");
        config.generation.parallel_extraction = false;
    }
    // Auto-disable parallel extraction for CLI providers (single auth session)
    if config.has_cli_provider() && config.generation.parallel_extraction {
        warn!("Auto-disabling parallel extraction (CLI provider detected)");
        config.generation.parallel_extraction = false;
    }

    // Default source_path to the repo path (not CWD) for local install/mount modes
    if config.generation.container.source_path.is_none()
        && config.generation.container.install_source != InstallSource::Registry
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

    // Local-install/local-mount requires source_path to be set (done above).
    // All languages now support local-install for bare-metal execution.

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
            test_llm.provider = provider.parse::<Provider>()?;
        }
        config.generation.test_llm = Some(test_llm);
    }

    // Security scan CLI override
    if no_security_scan {
        warn!("CLI override: security scan disabled");
        config.generation.enable_security_scan = false;
    }
    // Telemetry CLI overrides: --telemetry enables, --no-telemetry disables (escape hatch for config)
    if telemetry {
        config.generation.telemetry = true;
    } else if no_telemetry {
        config.generation.telemetry = false;
    }

    // Review agent CLI overrides
    if no_review {
        warn!("CLI override: review agent disabled");
        config.generation.enable_review = false;
        if review_model_override.is_some() || review_provider_override.is_some() {
            tracing::warn!(
                "--no-review is set; --review-model/--review-provider will have no effect"
            );
        }
    }
    if config.generation.enable_review
        && (review_model_override.is_some() || review_provider_override.is_some())
    {
        let mut review_llm = config
            .generation
            .review_llm
            .take()
            .unwrap_or_else(|| config.llm.clone());
        if let Some(ref model) = review_model_override {
            info!("CLI override: review model = {}", model);
            review_llm.model = model.clone();
        }
        if let Some(ref provider) = review_provider_override {
            info!("CLI override: review provider = {}", provider);
            review_llm.provider = provider.parse::<Provider>()?;
        }
        config.generation.review_llm = Some(review_llm);
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
    let language_str = detected_language.as_str().to_string();
    let collector = Collector::new(repo_path, detected_language)
        .with_max_source_chars(config.generation.max_source_tokens);
    let mut collected_data = collector.collect().await?;

    // Override version only when the user explicitly requests it (CLI flag or config).
    // Otherwise, keep the language-specific version from the collector (GoHandler, etc.).
    let version_strategy = version_from.or(config.generation.version_from);
    if version_override.is_some() || version_strategy.is_some() {
        // For --version-from package: prefer the ecosystem handler's version (already in
        // collected_data.version) over the Python-specific fallback chain in version.rs.
        // Only call extract_version for non-package strategies or explicit overrides.
        let skip_extract = version_override.is_none()
            && version_strategy == Some(crate::config::VersionStrategy::Package)
            && collected_data.version != "unknown";
        if !skip_extract {
            let final_version =
                version::extract_version(repo_path, version_override, version_strategy)?;
            collected_data.version = final_version;
        }
    }

    info!(
        "Collected data for package: {} v{}",
        collected_data.package_name, collected_data.version
    );

    // Warn about native dependencies if not running in container mode
    if !collected_data.native_dep_indicators.is_empty() && !container {
        let indicators = collected_data.native_dep_indicators.join(", ");
        warn!(
            "Native dependencies detected ({}). Consider using --container for reliable test execution.",
            indicators
        );
    }

    // Create LLM client via factory
    let client = factory::create_client(&config, dry_run).await?;
    if dry_run {
        info!("Using mock LLM client");
    } else {
        info!("Using {} LLM provider", config.llm.provider);
    }

    // Create per-stage LLM clients if configured
    let mut generator = Generator::new(client, config.generation.max_retries);
    if let Some(ref extract_config) = config.generation.extract_llm {
        let client = factory::create_client_from_llm_config(extract_config, dry_run).await?;
        info!(
            "Using {} for extract: {}",
            extract_config.provider, extract_config.model
        );
        generator = generator.with_extract_client(client);
    }
    if let Some(ref map_config) = config.generation.map_llm {
        let client = factory::create_client_from_llm_config(map_config, dry_run).await?;
        info!(
            "Using {} for map: {}",
            map_config.provider, map_config.model
        );
        generator = generator.with_map_client(client);
    }
    if let Some(ref learn_config) = config.generation.learn_llm {
        let client = factory::create_client_from_llm_config(learn_config, dry_run).await?;
        info!(
            "Using {} for learn: {}",
            learn_config.provider, learn_config.model
        );
        generator = generator.with_learn_client(client);
    }
    if let Some(ref create_config) = config.generation.create_llm {
        let client = factory::create_client_from_llm_config(create_config, dry_run).await?;
        info!(
            "Using {} for create: {}",
            create_config.provider, create_config.model
        );
        generator = generator.with_create_client(client);
    }
    if config.generation.enable_review {
        if let Some(ref review_config) = config.generation.review_llm {
            let client = factory::create_client_from_llm_config(review_config, dry_run).await?;
            info!(
                "Using {} for review: {}",
                review_config.provider, review_config.model
            );
            generator = generator.with_review_client(client);
        }
    }
    if config.generation.enable_test {
        if let Some(ref test_config) = config.generation.test_llm {
            let client = factory::create_client_from_llm_config(test_config, dry_run).await?;
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
    let start = Instant::now();
    let mut generator = generator
        .with_model_name(model_name)
        .with_prompts_config(config.prompts.clone())
        .with_test(config.generation.enable_test)
        .with_test_mode(config.generation.get_test_mode())
        .with_review(config.generation.enable_review)
        .with_security_scan(config.generation.enable_security_scan)
        .with_review_max_retries(config.generation.review_max_retries)
        .with_container_config(config.generation.container.clone())
        .with_parallel_extraction(config.generation.parallel_extraction)
        .with_debug_stage_dir(debug_stage_files)
        .with_security_context(config.generation.security_context);

    if let Some(ref skill) = existing_skill {
        generator = generator.with_existing_skill(skill.clone());
    }

    if let Some((api_surface, patterns, context, cached_ledger)) = replay_stages {
        generator = generator.with_replay_stages(api_surface, patterns, context, cached_ledger);
    }

    // Set up secret redaction for test agent output.
    crate::test_agent::executor::set_redact_vars(config.generation.redact_env_vars.clone());

    let output_result = generator.generate(&collected_data).await?;

    // Write output — skip entirely in dry-run (non-destructive: no files written).
    let output_path = Path::new(&output);
    let output_dir = output_path.parent().unwrap_or(Path::new("."));
    if !dry_run {
        let mut tmp = tempfile::NamedTempFile::new_in(output_dir)?;
        std::io::Write::write_all(&mut tmp, output_result.skill_md.as_bytes())?;

        // Only promote to final path if the run succeeded (or --best-effort)
        // Clean up stale temp files from previous failed runs before writing new output
        cleanup_stale_tmp_files(output_dir, output_path);

        if !output_result.has_unresolved_errors || best_effort {
            tmp.persist(output_path).map_err(|e| e.error)?;
            info!("✓ Generated SKILL.md written to {}", output);
        } else {
            // Keep the temp file around for inspection instead of auto-deleting
            let kept_path = tmp.into_temp_path().keep().map_err(|e| e.error)?;
            info!(
                "⚠ Output written to {} (unresolved errors — original preserved)",
                kept_path.display()
            );
        }
    }

    // Lint the generated file (skip in dry-run — mock output produces false warnings)
    let issues: Vec<crate::lint::LintIssue> = if dry_run {
        info!("Dry run complete — skipping lint (mock LLM output is not real content)");
        info!("Validated: file collection, config loading, language detection, provider setup");
        Vec::new()
    } else {
        info!("Running linter...");
        let linter = crate::lint::SkillLinter::new();
        let issues = linter.lint(&output_result.skill_md)?;
        linter.print_issues(&issues);
        issues
    };

    // Surface unresolved review warnings to the user
    if !output_result.unresolved_warnings.is_empty() {
        println!(
            "\n⚠ Review completed with {} unresolved issue(s):",
            output_result.unresolved_warnings.len()
        );
        review::print_review_issues(&output_result.unresolved_warnings);
        println!("\nThese could not be automatically verified or fixed.");
        println!("Consider adjusting your review prompts via the review_custom config option.");
    }

    // Record telemetry (non-fatal — warn on failure). Skip in dry-run.
    if config.generation.telemetry && !dry_run {
        let duration = start.elapsed();
        let test_llm = config.generation.test_llm.as_ref();
        let review_llm = config.generation.review_llm.as_ref();
        let record = crate::telemetry::RunRecord {
            language: language_str,
            library: collected_data.package_name.clone(),
            library_version: collected_data.version.clone(),
            provider: config.llm.provider.to_string(),
            model: config.llm.model.clone(),
            test_provider: test_llm.map(|c| c.provider.to_string()),
            test_model: test_llm.map(|c| c.model.clone()),
            review_provider: review_llm.map(|c| c.provider.to_string()),
            review_model: review_llm.map(|c| c.model.clone()),
            max_retries: config.generation.max_retries,
            retries_used: output_result.retries_used,
            review_retries_used: output_result.review_retries_used,
            passed: !output_result.has_unresolved_errors,
            failed_stage: output_result.failed_stage.map(|s| s.to_string()),
            failure_reason: output_result.failure_reason.clone(),
            duration_secs: duration.as_secs_f64(),
            timestamp: crate::telemetry::iso8601_now(),
            skilldo_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        if let Err(e) = crate::telemetry::append_run(&record, None) {
            tracing::warn!("Failed to write telemetry: {}", e);
        }
    }

    // Log run-complete summary for structured log consumers
    let duration = start.elapsed();
    let lint_issues = issues.len();
    let status = if output_result.has_unresolved_errors {
        "errors"
    } else {
        "ok"
    };
    info!(
        library = %collected_data.package_name,
        version = %collected_data.version,
        status,
        lint_issues,
        retries = output_result.retries_used,
        review_retries = output_result.review_retries_used,
        duration_secs = format!("{:.1}", duration.as_secs_f64()),
        "Run complete"
    );

    // Exit non-zero when unresolved errors remain (unless --best-effort)
    if output_result.has_unresolved_errors && !best_effort {
        anyhow::bail!(
            "Pipeline completed with unresolved errors. Use --best-effort to exit 0 anyway."
        );
    }

    Ok(())
}

/// Remove stale `.SKILL.md.*.tmp` files from prior failed runs.
fn cleanup_stale_tmp_files(dir: &Path, output_path: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let prefix = format!(
        ".{}.",
        output_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".tmp") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Create a minimal Python repo in a temp dir for dry-run tests.
    /// Includes a minimal skilldo.toml so tests don't pick up CWD config.
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
        // Minimal config to isolate from CWD/user config
        fs::write(
            dir.path().join("skilldo.toml"),
            "[llm]\nprovider = \"openai-compatible\"\nmodel = \"mock\"\napi_key_env = \"none\"\nbase_url = \"http://localhost:0/v1\"\n\n[generation]\nmax_retries = 1\nmax_source_tokens = 1000\n",
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

    /// Helper to build common test opts: path + output + dry_run + language=python.
    /// Uses the config from make_test_repo() to isolate from CWD/user config.
    fn test_opts(repo: &TempDir, output: &Path) -> GenerateOptions {
        GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            config_path: Some(
                repo.path()
                    .join("skilldo.toml")
                    .to_str()
                    .unwrap()
                    .to_string(),
            ),
            dry_run: true,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_run_dry_run_defaults() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            output: Some(output.to_str().unwrap().to_string()),
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(result.is_ok(), "dry run failed: {:?}", result.err());
        // Dry-run is non-destructive — no files should be written
        assert!(!output.exists(), "dry-run should not write files");
    }

    #[tokio::test]
    async fn test_run_explicit_language() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(test_opts(&repo, &output)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_version_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            version_override: Some("9.9.9".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
        // dry-run — no file written
        assert!(!output.exists(), "dry-run should not write files");
    }

    #[tokio::test]
    async fn test_run_with_model_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            model_override: Some("custom-model-v1".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_max_retries_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            max_retries_override: Some(10),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_update_mode_with_input() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let input_path = repo.path().join("old-SKILL.md");
        let mut f = fs::File::create(&input_path).unwrap();
        writeln!(
            f,
            "---\npackage: testpkg\nversion: 0.9.0\n---\n# Old content"
        )
        .unwrap();

        let result = run(GenerateOptions {
            input: Some(input_path.to_str().unwrap().to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_update_mode_existing_output() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        fs::write(
            &output,
            "---\npackage: testpkg\nversion: 0.5.0\n---\n# Existing",
        )
        .unwrap();

        let result = run(test_opts(&repo, &output)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_invalid_language() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            language: Some("brainfuck".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_err(), "should reject unknown language");
    }

    #[tokio::test]
    async fn test_run_with_provider_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            provider_override: Some("openai".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_base_url_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            base_url_override: Some("http://localhost:11434/v1".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_no_test_agent() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            no_test: true,
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_no_security_scan() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            no_security_scan: true,
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_test_mode_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            test_mode_override: Some("minimal".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_runtime_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            runtime_override: Some("podman".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_timeout_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            timeout_override: Some(300),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_request_timeout_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            request_timeout_override: Some(120),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_request_timeout_override_with_per_stage_config() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let config_path = write_per_stage_config(repo.path());
        let result = run(GenerateOptions {
            config_path: Some(config_path),
            request_timeout_override: Some(90),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "request_timeout with per-stage config failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_with_install_source_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            install_source_override: Some("local-mount".to_string()),
            best_effort: true, // mock output may trigger lint — we're testing the override threads through
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "install_source_override failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_accepts_local_install_for_non_python() {
        // local-install is now supported for all languages (v0.5.1)
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            language: Some("go".to_string()),
            install_source_override: Some("local-install".to_string()),
            dry_run: true,
            ..test_opts(&repo, &output)
        })
        .await;
        // Dry run should succeed (or fail for unrelated reasons like missing LLM)
        // but NOT fail with "install_source not supported"
        if let Err(ref e) = result {
            assert!(
                !e.to_string().contains("install_source"),
                "local-install should be accepted for Go: {e}"
            );
        }
    }

    #[tokio::test]
    async fn test_run_allows_local_install_non_python_when_no_test() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            language: Some("go".to_string()),
            install_source_override: Some("local-mount".to_string()),
            no_test: true,
            dry_run: true,
            best_effort: true,
            ..test_opts(&repo, &output)
        })
        .await;
        // May fail for other reasons (no Go files in test repo), but should NOT
        // fail with the install_source guard — that's what we're testing.
        if let Err(e) = &result {
            assert!(
                !e.to_string().contains("install_source"),
                "install_source guard should be skipped with --no-test: {e}"
            );
        }
    }

    #[tokio::test]
    async fn test_run_with_source_path_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            source_path_override: Some("/tmp/test".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_test_model_provider_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            test_model_override: Some("gpt-5.2".to_string()),
            test_provider_override: Some("openai".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_all_overrides() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            version_override: Some("1.2.3".to_string()),
            model_override: Some("gpt-4".to_string()),
            provider_override: Some("openai".to_string()),
            base_url_override: Some("http://localhost:11434/v1".to_string()),
            max_retries_override: Some(5),
            test_model_override: Some("gpt-5.2".to_string()),
            test_provider_override: Some("openai".to_string()),
            no_test: true,
            test_mode_override: Some("minimal".to_string()),
            no_review: true,
            review_model_override: Some("gpt-5.2".to_string()),
            review_provider_override: Some("openai".to_string()),
            runtime_override: Some("podman".to_string()),
            timeout_override: Some(300),
            install_source_override: Some("local-mount".to_string()),
            source_path_override: Some("/tmp/test".to_string()),
            no_parallel: true,
            ..test_opts(&repo, &output)
        })
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
provider_type = "openai-compatible"
model = "local-extract"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.map_llm]
provider_type = "openai-compatible"
model = "local-map"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.learn_llm]
provider_type = "openai-compatible"
model = "local-learn"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.create_llm]
provider_type = "openai-compatible"
model = "local-create"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.review_llm]
provider_type = "openai-compatible"
model = "local-review"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.test_llm]
provider_type = "openai-compatible"
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
        let result = run(GenerateOptions {
            config_path: Some(config_path),
            ..test_opts(&repo, &output)
        })
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
        let result = run(GenerateOptions {
            config_path: Some(config_path),
            model_override: Some("override-model".to_string()),
            provider_override: Some("openai".to_string()),
            ..test_opts(&repo, &output)
        })
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
        let result = run(GenerateOptions {
            config_path: Some(config_path.to_str().unwrap().to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_no_test_and_test_model_only() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            test_model_override: Some("gpt-5.2".to_string()),
            no_test: true,
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_test_model_only_no_provider() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            test_model_override: Some("gpt-5.2".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_test_provider_only_no_model() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            test_provider_override: Some("openai".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_dry_run_nonexistent_path() {
        let result = run(GenerateOptions {
            path: "/tmp/skilldo-nonexistent-path-xyz".to_string(),
            output: Some("/tmp/skilldo-nonexistent-output.md".to_string()),
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(result.is_err(), "nonexistent repo path should error");
    }

    #[tokio::test]
    async fn test_generate_dry_run_with_output_path() {
        let repo = make_test_repo();
        let output_dir = TempDir::new().unwrap();
        let output = output_dir.path().join("custom-output/SKILL.md");
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        let result = run(test_opts(&repo, &output)).await;
        assert!(
            result.is_ok(),
            "custom output path failed: {:?}",
            result.err()
        );
        // Dry-run is non-destructive — no files should be written
        assert!(!output.exists(), "dry-run should not write files");
    }

    #[tokio::test]
    async fn test_generate_dry_run_with_no_test_and_test_mode() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            no_test: true,
            test_mode_override: Some("minimal".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_dry_run_with_no_parallel() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            no_parallel: true,
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_best_effort_flag() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            output: Some(output.to_str().unwrap().to_string()),
            best_effort: true,
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "best_effort=true should always succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn cleanup_stale_tmp_files_removes_matching() {
        let dir = TempDir::new().unwrap();
        let output = dir.path().join("click-SKILL.md");

        // Create stale tmp files from "prior runs"
        fs::write(dir.path().join(".click-SKILL.md.12345.tmp"), "stale1").unwrap();
        fs::write(dir.path().join(".click-SKILL.md.67890.tmp"), "stale2").unwrap();
        // Unrelated file should survive
        fs::write(dir.path().join("other.txt"), "keep").unwrap();

        cleanup_stale_tmp_files(dir.path(), &output);

        assert!(!dir.path().join(".click-SKILL.md.12345.tmp").exists());
        assert!(!dir.path().join(".click-SKILL.md.67890.tmp").exists());
        assert!(dir.path().join("other.txt").exists());
    }

    #[test]
    fn cleanup_stale_tmp_files_nonexistent_dir() {
        // Should not panic on a missing directory
        cleanup_stale_tmp_files(Path::new("/nonexistent/dir"), Path::new("SKILL.md"));
    }

    #[tokio::test]
    async fn test_generate_dry_run_with_no_telemetry() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            no_telemetry: true,
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_dry_run_with_telemetry() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            telemetry: true,
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_container_flag() {
        // Verifies --container flag threads through pipeline options.
        // Test agent disabled — we're testing flag wiring, not Docker.
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            container: true,
            no_test: true,
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_ok(), "container flag failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_run_with_container_and_runtime_override() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            container: true,
            runtime_override: Some("docker".to_string()),
            best_effort: true,
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "container+runtime override failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_cli_provider_auto_disables_parallel() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let config_path = repo.path().join("skilldo.toml");
        fs::write(
            &config_path,
            r#"
[llm]
provider = "cli"
model = "gemini-2.5-pro"
command = "echo"

[generation]
max_retries = 0
max_source_tokens = 50000
parallel_extraction = true
"#,
        )
        .unwrap();
        let result = run(GenerateOptions {
            config_path: Some(config_path.to_str().unwrap().to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "CLI provider auto-disable parallel failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_with_review_model_override_enabled() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            review_model_override: Some("gpt-5.2".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "review_model_override with review enabled failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_with_review_provider_override_enabled() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            review_provider_override: Some("openai".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "review_provider_override with review enabled failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_with_review_model_and_provider_override_enabled() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            review_model_override: Some("gpt-5.2".to_string()),
            review_provider_override: Some("openai".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "review model+provider override failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_no_review_with_review_overrides_warns() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            no_review: true,
            review_model_override: Some("gpt-5.2".to_string()),
            review_provider_override: Some("openai".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "no_review with review overrides failed: {:?}",
            result.err()
        );
    }

    /// Create a minimal Java repo in a temp dir for dry-run tests
    fn make_java_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        // pom.xml
        fs::write(
            dir.path().join("pom.xml"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
    <modelVersion>4.0.0</modelVersion>
    <groupId>com.example</groupId>
    <artifactId>testpkg</artifactId>
    <version>1.0.0</version>
</project>"#,
        )
        .unwrap();
        // Source (use com/example to avoid "test" component triggering is_test_path)
        let src = dir
            .path()
            .join("src")
            .join("main")
            .join("java")
            .join("com")
            .join("example");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("App.java"),
            "package com.example;\npublic class App {\n    public static String hello() { return \"world\"; }\n}\n",
        )
        .unwrap();
        // Test
        let test_dir = dir
            .path()
            .join("src")
            .join("test")
            .join("java")
            .join("com")
            .join("example");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(
            test_dir.join("AppTest.java"),
            "package com.example;\npublic class AppTest {\n    public void testHello() {}\n}\n",
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn test_run_dry_run_java_language() {
        let repo = make_java_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("java".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            best_effort: true,
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(result.is_ok(), "Java dry run failed: {:?}", result.err());
        assert!(!output.exists(), "dry-run should not write files");
    }

    #[tokio::test]
    async fn test_run_java_with_model_and_provider_override() {
        let repo = make_java_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("java".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            model_override: Some("custom-model".to_string()),
            provider_override: Some("openai".to_string()),
            best_effort: true,
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "Java model/provider override failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_java_accepts_local_install() {
        // local-install is now supported for all languages (v0.5.1)
        let repo = make_java_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("java".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            install_source_override: Some("local-install".to_string()),
            dry_run: true,
            ..Default::default()
        })
        .await;
        if let Err(ref e) = result {
            assert!(
                !e.to_string().contains("install_source"),
                "local-install should be accepted for Java: {e}"
            );
        }
    }

    #[tokio::test]
    async fn test_run_java_with_no_test_and_local_install() {
        let repo = make_java_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("java".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            install_source_override: Some("local-mount".to_string()),
            no_test: true,
            best_effort: true,
            dry_run: true,
            ..Default::default()
        })
        .await;
        // Should NOT fail with install_source guard since --no-test is set
        if let Err(e) = &result {
            assert!(
                !e.to_string().contains("install_source"),
                "install_source guard should be skipped with --no-test: {e}"
            );
        }
    }

    #[tokio::test]
    async fn test_run_java_with_review_model_provider_override() {
        let repo = make_java_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("java".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            review_model_override: Some("gpt-5.2".to_string()),
            review_provider_override: Some("openai".to_string()),
            best_effort: true,
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "Java review override failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_java_with_per_stage_config() {
        let repo = make_java_test_repo();
        let output = repo.path().join("SKILL.md");
        let config_path = write_per_stage_config(repo.path());
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("java".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            config_path: Some(config_path),
            best_effort: true,
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "Java per-stage config failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_java_with_telemetry_enabled() {
        let repo = make_java_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("java".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            telemetry: true,
            best_effort: true,
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "Java telemetry run failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_java_with_version_override() {
        let repo = make_java_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("java".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            version_override: Some("9.9.9".to_string()),
            best_effort: true,
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "Java version override failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_with_no_review_and_review_model_only() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            no_review: true,
            review_model_override: Some("gpt-5.2".to_string()),
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_no_review_and_review_provider_only() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            no_review: true,
            review_provider_override: Some("openai".to_string()),
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_auto_detect_language_python() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            output: Some(output.to_str().unwrap().to_string()),
            dry_run: true,
            language: None,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "auto-detect language failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_telemetry_with_test_review_llm_metadata() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let config_path = repo.path().join("skilldo.toml");
        fs::write(
            &config_path,
            r#"
[llm]
provider = "anthropic"
model = "claude-sonnet"
api_key_env = "none"

[generation]
max_retries = 0
max_source_tokens = 50000
telemetry = true

[generation.test_llm]
provider_type = "openai-compatible"
model = "local-test"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.review_llm]
provider_type = "openai-compatible"
model = "local-review"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
"#,
        )
        .unwrap();
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            config_path: Some(config_path.to_str().unwrap().to_string()),
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "telemetry with test/review LLM failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_output_defaults_to_skill_md_path() {
        let repo = make_test_repo();
        // Use a temp-dir output so this test never overwrites the repo's SKILL.md
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            output: Some(output.to_str().unwrap().to_string()),
            language: Some("python".to_string()),
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "default output path failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_runtime_no_container_warns_only() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            runtime_override: Some("docker".to_string()),
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_install_source_auto_container_mode() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            install_source_override: Some("local-mount".to_string()),
            best_effort: true,
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(
            result.is_ok(),
            "local-mount auto-container failed: {:?}",
            result.err()
        );
    }

    /// Create a minimal Rust repo with a -sys dependency to trigger native dep warning
    fn make_rust_test_repo_with_sys() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"testpkg\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n[dependencies]\nopenssl-sys = \"0.9\"\n",
        )
        .unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("lib.rs"), "pub fn hello() {}\n").unwrap();
        let tests_dir = dir.path().join("tests");
        fs::create_dir(&tests_dir).unwrap();
        fs::write(tests_dir.join("test_lib.rs"), "fn test_it() {}\n").unwrap();
        // Config for isolation
        fs::write(
            dir.path().join("skilldo.toml"),
            "[llm]\nprovider = \"openai-compatible\"\nmodel = \"mock\"\napi_key_env = \"none\"\nbase_url = \"http://localhost:0/v1\"\n\n[generation]\nmax_retries = 1\nmax_source_tokens = 1000\n",
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn test_run_rust_with_native_deps_warns() {
        let repo = make_rust_test_repo_with_sys();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("rust".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            config_path: Some(
                repo.path()
                    .join("skilldo.toml")
                    .to_str()
                    .unwrap()
                    .to_string(),
            ),
            dry_run: true,
            best_effort: true,
            ..Default::default()
        })
        .await;
        // Should succeed (dry run) but exercise the native dep warning path
        assert!(
            result.is_ok(),
            "Rust with -sys dep dry run failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_with_no_test_and_test_provider_only2() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            no_test: true,
            test_provider_override: Some("openai".to_string()),
            dry_run: true,
            ..Default::default()
        })
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_with_version_from_git_tag() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            version_from: Some(crate::config::VersionStrategy::GitTag),
            dry_run: true,
            ..Default::default()
        })
        .await;
        // May fail if no git tags, but should not panic
        let _ = result;
    }

    /// Valid SKILL.md fixture response for mock LLM server.
    /// Must pass basic frontmatter + section linting so the file-write path executes.
    const MOCK_SKILL_MD: &str = r#"---
name: testpkg
description: A test package
license: MIT
metadata:
  version: "1.0.0"
  ecosystem: python
---

## Imports

```python
import testpkg
```

## Core Patterns

### Basic Usage

```python
from testpkg import hello
result = hello()
```

## Configuration

No special configuration required.

## Pitfalls

### Wrong: Missing import

```python
hello()  # NameError
```

### Right: Import first

```python
from testpkg import hello
hello()
```

## References

- https://example.com/testpkg
"#;

    /// Write a config TOML pointing all LLM stages at a local mock server.
    fn write_mock_server_config(dir: &Path, server_url: &str) -> String {
        let config_path = dir.join("skilldo-mock.toml");
        let base_url = format!("{}/v1", server_url);
        let toml = format!(
            r#"
[llm]
provider_type = "openai-compatible"
model = "mock-model"
api_key_env = "none"
base_url = "{url}"

[generation]
max_retries = 0
max_source_tokens = 50000
install_source = "registry"
enable_test = false
enable_review = false
enable_security_scan = false
"#,
            url = base_url
        );
        fs::write(&config_path, toml).unwrap();
        config_path.to_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn test_run_non_dry_run_writes_file() {
        // Exercises the non-dry-run code paths:
        // - File write via tempfile + persist (lines 481-488)
        // - Linter execution on real output (lines 505-509)
        let server = llmposter::ServerBuilder::new()
            .fixture(llmposter::Fixture::new().respond_with_content(MOCK_SKILL_MD))
            .build()
            .await
            .expect("failed to start mock server");

        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let config_path = write_mock_server_config(repo.path(), &server.url());

        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            config_path: Some(config_path),
            best_effort: true,
            no_parallel: true,
            dry_run: false,
            ..Default::default()
        })
        .await;

        assert!(result.is_ok(), "non-dry-run failed: {:?}", result.err());
        // Non-dry-run should write the SKILL.md file
        assert!(output.exists(), "non-dry-run should write the output file");
        let content = fs::read_to_string(&output).unwrap();
        assert!(
            content.contains("name: testpkg"),
            "output should contain frontmatter"
        );
    }

    /// SKILL.md missing required sections — triggers lint errors, NOT security errors.
    /// With max_retries=0 the generator exhausts retries immediately → has_unresolved_errors.
    const BAD_SKILL_MD: &str = r#"---
name: testpkg
description: A test package
license: MIT
metadata:
  version: "1.0.0"
  ecosystem: python
---

# testpkg

This content has no required sections and no code blocks.
"#;

    #[tokio::test]
    async fn test_run_non_dry_run_unresolved_errors_keeps_temp_file() {
        // Exercises the `has_unresolved_errors && !best_effort` branch (lines 490-494):
        // temp file is kept for inspection, original output is NOT overwritten.
        //
        // Activate a tracing subscriber so info!() format args are evaluated by llvm-cov.
        // try_init() is idempotent — safe to call from multiple tests.
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();

        let server = llmposter::ServerBuilder::new()
            .fixture(llmposter::Fixture::new().respond_with_content(BAD_SKILL_MD))
            .build()
            .await
            .expect("failed to start mock server");

        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        // Pre-create an "original" file that should be preserved
        fs::write(&output, "original content").unwrap();
        let config_path = write_mock_server_config(repo.path(), &server.url());

        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            config_path: Some(config_path),
            best_effort: false,
            no_parallel: true,
            dry_run: false,
            ..Default::default()
        })
        .await;

        // Should fail with "unresolved errors" bail
        assert!(
            result.is_err(),
            "expected error when has_unresolved_errors && !best_effort"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("unresolved errors"),
            "error message should mention unresolved errors, got: {err_msg}"
        );
        // Original file should be preserved (not overwritten)
        let preserved = fs::read_to_string(&output).unwrap();
        assert_eq!(
            preserved, "original content",
            "original output file should not be overwritten when errors are unresolved"
        );
        // A kept temp file should exist in the output dir (the kept temp path from
        // `into_temp_path().keep()`). Tempfile names are opaque, but we verify at least
        // one non-SKILL.md file was created by the pipeline in the output dir.
        let extra_files: Vec<_> = fs::read_dir(repo.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name != "SKILL.md"
                    && !name.starts_with("skilldo-mock")
                    && !name.starts_with("setup.py")
                    && name != "testpkg"
                    && name != "tests"
            })
            .collect();
        assert!(
            !extra_files.is_empty(),
            "a kept temp file should exist when errors are unresolved"
        );
    }

    /// Write a config TOML with redact_env_vars set.
    fn write_mock_server_config_with_redact(dir: &Path, server_url: &str) -> String {
        let config_path = dir.join("skilldo-mock-redact.toml");
        let base_url = format!("{}/v1", server_url);
        let toml = format!(
            r#"
[llm]
provider_type = "openai-compatible"
model = "mock-model"
api_key_env = "none"
base_url = "{url}"

[generation]
max_retries = 0
max_source_tokens = 50000
install_source = "registry"
enable_test = true
enable_review = false
enable_security_scan = false
redact_env_vars = ["FAKE_API_KEY", "ANOTHER_SECRET"]
"#,
            url = base_url
        );
        fs::write(&config_path, toml).unwrap();
        config_path.to_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn test_run_non_dry_run_with_redact_env_vars() {
        // Exercises line 472: set_redact_vars when config has non-empty redact_env_vars.
        let server = llmposter::ServerBuilder::new()
            .fixture(llmposter::Fixture::new().respond_with_content(MOCK_SKILL_MD))
            .build()
            .await
            .expect("failed to start mock server");

        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");
        let config_path = write_mock_server_config_with_redact(repo.path(), &server.url());

        let result = run(GenerateOptions {
            path: repo.path().to_str().unwrap().to_string(),
            language: Some("python".to_string()),
            output: Some(output.to_str().unwrap().to_string()),
            config_path: Some(config_path),
            best_effort: true,
            no_parallel: true,
            dry_run: false,
            ..Default::default()
        })
        .await;

        assert!(
            result.is_ok(),
            "non-dry-run with redact_env_vars failed: {:?}",
            result.err()
        );
        assert!(output.exists(), "output file should be written");
    }

    // ========================================================================
    // --replay-from: loading cached stage files
    // ========================================================================

    #[tokio::test]
    async fn test_run_replay_from_loads_cached_stages() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");

        // Create a replay cache directory with the 3 required stage files
        let replay_dir = repo.path().join("replay-cache");
        fs::create_dir(&replay_dir).unwrap();
        fs::write(
            replay_dir.join("1-extract.md"),
            "# API Surface\nfn hello() -> str",
        )
        .unwrap();
        fs::write(
            replay_dir.join("2-map.md"),
            "# Usage Patterns\nhello() returns greeting",
        )
        .unwrap();
        fs::write(
            replay_dir.join("3-learn.md"),
            "# Conventions\nUse hello() for greetings",
        )
        .unwrap();

        let result = run(GenerateOptions {
            replay_from: Some(replay_dir.to_str().unwrap().to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "replay-from dry run failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_replay_from_with_fact_ledger() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");

        // Create replay cache with all 3 stages plus facts.md
        let replay_dir = repo.path().join("replay-cache-facts");
        fs::create_dir(&replay_dir).unwrap();
        fs::write(replay_dir.join("1-extract.md"), "extract content").unwrap();
        fs::write(replay_dir.join("2-map.md"), "map content").unwrap();
        fs::write(replay_dir.join("3-learn.md"), "learn content").unwrap();
        fs::write(
            replay_dir.join("facts.md"),
            "- Package name is testpkg\n- Version is 1.0.0",
        )
        .unwrap();

        let result = run(GenerateOptions {
            replay_from: Some(replay_dir.to_str().unwrap().to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "replay-from with facts.md failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_replay_from_with_empty_fact_ledger() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");

        // Create replay cache with empty facts.md (should be treated as None)
        let replay_dir = repo.path().join("replay-cache-empty-facts");
        fs::create_dir(&replay_dir).unwrap();
        fs::write(replay_dir.join("1-extract.md"), "extract content").unwrap();
        fs::write(replay_dir.join("2-map.md"), "map content").unwrap();
        fs::write(replay_dir.join("3-learn.md"), "learn content").unwrap();
        fs::write(replay_dir.join("facts.md"), "   \n  ").unwrap();

        let result = run(GenerateOptions {
            replay_from: Some(replay_dir.to_str().unwrap().to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_ok(),
            "replay-from with empty facts.md failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_run_replay_from_missing_dir_fails() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");

        let result = run(GenerateOptions {
            replay_from: Some("/tmp/nonexistent-replay-dir-xyz".to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(result.is_err(), "nonexistent replay dir should fail");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("--replay-from"),
            "error should mention --replay-from: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_run_replay_from_missing_stage_file_fails() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");

        // Create replay dir with only 2 of 3 required files
        let replay_dir = repo.path().join("replay-cache-incomplete");
        fs::create_dir(&replay_dir).unwrap();
        fs::write(replay_dir.join("1-extract.md"), "extract content").unwrap();
        fs::write(replay_dir.join("2-map.md"), "map content").unwrap();
        // 3-learn.md is intentionally missing

        let result = run(GenerateOptions {
            replay_from: Some(replay_dir.to_str().unwrap().to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_err(),
            "replay-from with missing stage file should fail"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("3-learn.md"),
            "error should mention missing file: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_run_replay_from_invalid_utf8_fact_ledger_fails() {
        let repo = make_test_repo();
        let output = repo.path().join("SKILL.md");

        // Create replay cache with valid stage files but invalid UTF-8 in facts.md
        let replay_dir = repo.path().join("replay-cache-bad-facts");
        fs::create_dir(&replay_dir).unwrap();
        fs::write(replay_dir.join("1-extract.md"), "extract content").unwrap();
        fs::write(replay_dir.join("2-map.md"), "map content").unwrap();
        fs::write(replay_dir.join("3-learn.md"), "learn content").unwrap();
        // Write invalid UTF-8 bytes
        std::fs::write(replay_dir.join("facts.md"), [0xFF, 0xFE, 0x80, 0x81]).unwrap();

        let result = run(GenerateOptions {
            replay_from: Some(replay_dir.to_str().unwrap().to_string()),
            ..test_opts(&repo, &output)
        })
        .await;
        assert!(
            result.is_err(),
            "replay-from with invalid UTF-8 facts.md should fail"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("--replay-from") && err_msg.contains("facts.md"),
            "error should mention --replay-from and facts.md: {err_msg}"
        );
    }
}
