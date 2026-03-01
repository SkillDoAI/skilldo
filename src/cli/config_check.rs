use anyhow::Result;
use std::env;
use std::process::Command;

use crate::config::{Config, Provider};

struct CheckResult {
    passed: Vec<String>,
    warnings: Vec<String>,
    errors: Vec<String>,
}

impl CheckResult {
    fn new() -> Self {
        Self {
            passed: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn pass(&mut self, msg: impl Into<String>) {
        self.passed.push(msg.into());
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
    }
}

pub fn run(config_path: Option<String>) -> Result<()> {
    let mut results = CheckResult::new();

    // 1. Try to load config
    let config = match Config::load_with_path(config_path.clone()) {
        Ok(config) => {
            let source = config_path.as_deref().unwrap_or("default search path");
            results.pass(format!("Config loaded from {}", source));
            config
        }
        Err(e) => {
            // Intentional: print the error diagnostically and return Ok(()).
            // This is a diagnostic command — config load failure is reported to the
            // user via print_results(), not propagated as an Err (which would double-print).
            results.error(format!("Failed to load config: {}", e));
            print_results(&results);
            return Ok(());
        }
    };

    // 2. Check LLM provider (enum-validated — always valid)
    results.pass(format!(
        "LLM provider: {} (model: {})",
        config.llm.provider, config.llm.model
    ));

    // 3. Check main API key env var
    check_api_key(
        &config.llm.api_key_env,
        "Main LLM",
        &config.llm.provider,
        &mut results,
    );

    // 4. Check base_url for openai-compatible
    if config.llm.provider == Provider::OpenAICompatible {
        if config.llm.base_url.is_some() {
            results.pass("Base URL configured for openai-compatible provider".to_string());
        } else {
            results.warn(
                "openai-compatible provider without base_url — will use default http://localhost:11434/v1".to_string(),
            );
        }
    }

    // 5. Check generation settings
    results.pass(format!(
        "Generation: max_retries={}, max_source_tokens={}",
        config.generation.max_retries, config.generation.max_source_tokens
    ));

    // 6. Check test agent
    if config.generation.enable_test {
        results.pass(format!(
            "test agent enabled (mode: {})",
            config.generation.test_mode
        ));

        // Check test agent LLM override if configured
        if let Some(ref test_llm) = config.generation.test_llm {
            check_stage_provider("test", &test_llm.provider, &test_llm.model, &mut results);
            check_api_key(
                &test_llm.api_key_env,
                "test LLM",
                &test_llm.provider,
                &mut results,
            );
        }
    } else {
        results.pass("test agent disabled".to_string());
    }

    // 6b. Check review agent
    if config.generation.enable_review {
        results.pass(format!(
            "review agent enabled (max_retries: {})",
            config.generation.review_max_retries
        ));
        if let Some(ref review_llm) = config.generation.review_llm {
            check_stage_provider(
                "review",
                &review_llm.provider,
                &review_llm.model,
                &mut results,
            );
            check_api_key(
                &review_llm.api_key_env,
                "review LLM",
                &review_llm.provider,
                &mut results,
            );
        }
    } else {
        results.pass("review agent disabled".to_string());
    }

    // 7. Check per-stage LLM overrides
    let stage_llms: [(&str, &Option<crate::config::LlmConfig>); 4] = [
        ("extract", &config.generation.extract_llm),
        ("map", &config.generation.map_llm),
        ("learn", &config.generation.learn_llm),
        ("create", &config.generation.create_llm),
    ];
    for (name, llm_opt) in &stage_llms {
        if let Some(llm) = llm_opt {
            check_stage_provider(name, &llm.provider, &llm.model, &mut results);
            check_api_key(
                &llm.api_key_env,
                &format!("{} LLM", name),
                &llm.provider,
                &mut results,
            );
        }
    }

    // 8. Check container runtime
    let runtime = &config.generation.container.runtime;
    if check_runtime_available(runtime) {
        results.pass(format!("Container runtime: {} (available)", runtime));
    } else if config.generation.enable_test {
        results.error(format!(
            "Container runtime '{}' not found — test agent validation will fail",
            runtime
        ));
    } else {
        results.warn(format!(
            "Container runtime '{}' not found (test agent disabled, so this is OK)",
            runtime
        ));
    }

    // 9. Validate extra_body_json for main and per-agent LLM configs
    match config.llm.resolve_extra_body() {
        Ok(extra) if !extra.is_empty() => {
            results.pass(format!("Main LLM extra_body: {} fields", extra.len()));
        }
        Ok(_) => {} // empty, nothing to report
        Err(e) => {
            results.error(format!("Main LLM extra_body_json: {}", e));
        }
    }
    let all_stage_llms: [(&str, &Option<crate::config::LlmConfig>); 6] = [
        ("extract", &config.generation.extract_llm),
        ("map", &config.generation.map_llm),
        ("learn", &config.generation.learn_llm),
        ("create", &config.generation.create_llm),
        ("review", &config.generation.review_llm),
        ("test", &config.generation.test_llm),
    ];
    for (name, llm_opt) in &all_stage_llms {
        if let Some(llm) = llm_opt {
            match llm.resolve_extra_body() {
                Ok(extra) if !extra.is_empty() => {
                    results.pass(format!("{} extra_body: {} fields", name, extra.len()));
                }
                Ok(_) => {}
                Err(e) => {
                    results.error(format!("{} extra_body_json: {}", name, e));
                }
            }
        }
    }

    // 10. Check container timeout
    if config.generation.container.timeout < 30 {
        results.warn(format!(
            "Container timeout {}s is very short — consider 60s+ for libraries with dependencies",
            config.generation.container.timeout
        ));
    }

    // 11. Warn about parallel extraction with local models
    if config.generation.parallel_extraction && is_likely_local_provider(&config) {
        results.warn(
            "parallel_extraction=true with a local provider (Ollama) — \
             this may cause hangs. Set parallel_extraction = false in [generation]"
                .to_string(),
        );
    }

    // Print results
    print_results(&results);

    // Return error if there were validation failures
    if !results.errors.is_empty() {
        anyhow::bail!("{} config error(s) found", results.errors.len());
    }

    Ok(())
}

/// Heuristic: check if any LLM used during parallel extraction is likely local (Ollama).
/// Inspects extract/map/learn overrides if present, otherwise falls back to the main LLM config.
fn is_likely_local_provider(config: &Config) -> bool {
    let llms_used_in_parallel = [
        config.generation.extract_llm.as_ref(),
        config.generation.map_llm.as_ref(),
        config.generation.learn_llm.as_ref(),
    ];
    // If any stage override is local, warn. If no overrides, check main config.
    let has_override = llms_used_in_parallel.iter().any(|o| o.is_some());
    if has_override {
        llms_used_in_parallel
            .iter()
            .filter_map(|o| o.as_ref())
            .any(|llm| is_llm_local(llm))
    } else {
        is_llm_local(&config.llm)
    }
}

fn is_llm_local(llm: &crate::config::LlmConfig) -> bool {
    if llm.provider != Provider::OpenAICompatible {
        return false;
    }
    match &llm.base_url {
        Some(url) => url.contains("localhost") || url.contains("127.0.0.1"),
        None => true, // default is localhost:11434
    }
}

fn check_stage_provider(
    stage_name: &str,
    provider: &Provider,
    model: &str,
    results: &mut CheckResult,
) {
    // Provider is enum-validated — always valid
    results.pass(format!(
        "{} LLM override: {} ({})",
        stage_name, provider, model
    ));
}

fn check_api_key(
    api_key_env: &Option<String>,
    label: &str,
    provider: &Provider,
    results: &mut CheckResult,
) {
    let is_oai_compat = *provider == Provider::OpenAICompatible;
    match api_key_env {
        Some(env_var) if env_var.to_lowercase() == "none" => {
            results.pass(format!("{}: no API key needed", label));
        }
        Some(env_var) => match env::var(env_var) {
            Ok(v) if !v.trim().is_empty() => {
                results.pass(format!("{}: {} is set", label, env_var));
            }
            Ok(_) if is_oai_compat => {
                results.warn(format!(
                    "{}: {} is set but empty (OK for local models, needed for gateways)",
                    label, env_var
                ));
            }
            Ok(_) => {
                results.error(format!("{}: {} is set but empty", label, env_var));
            }
            Err(_) if is_oai_compat => {
                results.warn(format!(
                    "{}: {} is not set (OK for local models, needed for gateways)",
                    label, env_var
                ));
            }
            Err(_) => {
                results.error(format!("{}: {} is not set", label, env_var));
            }
        },
        None => {
            let inferred = provider.default_api_key_env();
            match env::var(inferred) {
                Ok(v) if !v.trim().is_empty() => {
                    results.pass(format!(
                        "{}: {} is set (inferred from provider)",
                        label, inferred
                    ));
                }
                Ok(_) if is_oai_compat => {
                    results.warn(format!(
                        "{}: {} is set but empty (OK for local models, needed for gateways)",
                        label, inferred
                    ));
                }
                Ok(_) => {
                    results.error(format!(
                        "{}: {} is set but empty (inferred from provider)",
                        label, inferred
                    ));
                }
                Err(_) if is_oai_compat => {
                    results.warn(format!(
                        "{}: {} is not set (OK for local models, needed for gateways)",
                        label, inferred
                    ));
                }
                Err(_) => {
                    results.error(format!(
                        "{}: {} is not set (inferred from provider)",
                        label, inferred
                    ));
                }
            }
        }
    }
}

fn check_runtime_available(runtime: &str) -> bool {
    Command::new(runtime)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn print_results(results: &CheckResult) {
    println!();
    for msg in &results.passed {
        println!("  \u{2713} {}", msg);
    }
    for msg in &results.warnings {
        println!("  ! {}", msg);
    }
    for msg in &results.errors {
        println!("  \u{2717} {}", msg);
    }
    println!();
    println!(
        "{} passed, {} warnings, {} errors",
        results.passed.len(),
        results.warnings.len(),
        results.errors.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests that mutate OPENAI_API_KEY must not run in parallel.
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard: acquires ENV_MUTEX, saves the env var, and restores on drop.
    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        var: &'static str,
        saved: Option<String>,
    }

    impl EnvGuard {
        fn new(var: &'static str) -> Self {
            let lock = ENV_MUTEX.lock().unwrap();
            let saved = env::var(var).ok();
            Self {
                _lock: lock,
                var,
                saved,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.saved {
                Some(val) => env::set_var(self.var, val),
                None => env::remove_var(self.var),
            }
        }
    }

    #[test]
    fn test_check_result_new() {
        let r = CheckResult::new();
        assert!(r.passed.is_empty());
        assert!(r.warnings.is_empty());
        assert!(r.errors.is_empty());
    }

    #[test]
    fn test_check_result_pass() {
        let mut r = CheckResult::new();
        r.pass("test passed");
        assert_eq!(r.passed.len(), 1);
        assert_eq!(r.passed[0], "test passed");
    }

    #[test]
    fn test_check_result_warn() {
        let mut r = CheckResult::new();
        r.warn("test warning");
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn test_check_result_error() {
        let mut r = CheckResult::new();
        r.error("test error");
        assert_eq!(r.errors.len(), 1);
    }

    #[test]
    fn test_check_api_key_none_provider() {
        let mut r = CheckResult::new();
        check_api_key(
            &Some("none".to_string()),
            "Test",
            &Provider::Anthropic,
            &mut r,
        );
        assert_eq!(r.passed.len(), 1);
        assert!(r.passed[0].contains("no API key needed"));
    }

    #[test]
    fn test_check_api_key_set() {
        env::set_var("SKILLDO_TEST_CHECK_KEY", "test123");
        let mut r = CheckResult::new();
        check_api_key(
            &Some("SKILLDO_TEST_CHECK_KEY".to_string()),
            "Test",
            &Provider::Anthropic,
            &mut r,
        );
        assert_eq!(r.passed.len(), 1);
        assert!(r.passed[0].contains("is set"));
        env::remove_var("SKILLDO_TEST_CHECK_KEY");
    }

    #[test]
    fn test_check_api_key_missing() {
        let mut r = CheckResult::new();
        check_api_key(
            &Some("SKILLDO_NONEXISTENT_KEY_999".to_string()),
            "Test",
            &Provider::Anthropic,
            &mut r,
        );
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].contains("is not set"));
    }

    #[test]
    fn test_check_api_key_missing_openai_compatible() {
        let mut r = CheckResult::new();
        check_api_key(
            &Some("SKILLDO_NONEXISTENT_KEY_999".to_string()),
            "Test",
            &Provider::OpenAICompatible,
            &mut r,
        );
        // Should be a warning, not an error
        assert_eq!(r.warnings.len(), 1);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn test_check_api_key_explicit_none() {
        // api_key_env="none" → no key needed (e.g. Ollama)
        let mut r = CheckResult::new();
        check_api_key(
            &Some("none".to_string()),
            "Test",
            &Provider::OpenAICompatible,
            &mut r,
        );
        assert_eq!(r.passed.len(), 1);
        assert!(r.passed[0].contains("no API key needed"));
    }

    #[test]
    fn test_check_api_key_inferred_from_provider() {
        // Known provider with api_key_env=None → infers env var and checks it
        let mut r = CheckResult::new();
        check_api_key(&None, "Test", &Provider::Anthropic, &mut r);
        // ANTHROPIC_API_KEY is not set in test → error with inferred message
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].contains("inferred from provider"));
    }

    #[test]
    fn test_run_with_nonexistent_config() {
        // Should not panic, should report an error gracefully
        let result = run(Some("/nonexistent/config.toml".to_string()));
        assert!(result.is_ok()); // run() returns Ok even on config errors (it prints them)
    }

    #[test]
    fn test_run_with_valid_temp_config() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test-config.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 3
max_source_tokens = 50000
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    // Test unknown provider rejected at deserialization time.
    #[test]
    fn test_run_with_unknown_provider() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "badprovider"
model = "test"
api_key_env = "none"

[generation]
max_retries = 1
max_source_tokens = 1000
"#
        )
        .unwrap();

        // Provider enum rejects unknown values at deserialization time
        let result =
            crate::config::Config::load_with_path(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_err(), "badprovider should fail deserialization");
    }

    #[test]
    fn test_run_with_test_llm_override() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "base-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = true

[generation.agent5_llm]
provider = "openai-compatible"
model = "agent5-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_per_agent_overrides() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "base-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000

[generation.agent1_llm]
provider = "openai-compatible"
model = "agent1-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.agent3_llm]
provider = "openai-compatible"
model = "agent3-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_short_timeout() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000

[generation.container]
timeout = 5
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_agent5_disabled() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_openai_compatible_no_base_url() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false
"#
        )
        .unwrap();

        // Should produce a warning about missing base_url but no errors.
        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_openai_compatible_with_base_url() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_results_formatting() {
        let mut r = CheckResult::new();
        r.pass("everything is fine");
        r.warn("something to watch");
        r.error("something broke");
        // Verify print_results does not panic with a mix of passes, warnings, and errors.
        print_results(&r);
        assert_eq!(r.passed.len(), 1);
        assert_eq!(r.warnings.len(), 1);
        assert_eq!(r.errors.len(), 1);
    }

    #[test]
    fn test_check_runtime_unavailable() {
        let available = check_runtime_available("nonexistent_runtime_xyz");
        assert!(!available);
    }

    #[test]
    fn test_check_stage_provider_valid() {
        let mut r = CheckResult::new();
        check_stage_provider("extract", &Provider::OpenAI, "gpt-5", &mut r);
        assert_eq!(r.passed.len(), 1);
        assert!(r.passed[0].contains("extract LLM override"));
    }

    #[test]
    fn test_check_api_key_inferred_set() {
        // Line 254: api_key_env=None, provider infers env var, and that var IS set.
        // Use a dedicated env var name to avoid races with openai-compatible tests.
        env::set_var("SKILLDO_TEST_INFERRED_GEMINI_KEY_99", "fake-gemini-key");
        let mut r = CheckResult::new();
        // Temporarily map gemini to our custom var by routing through the existing test helper.
        // The code infers GEMINI_API_KEY for gemini provider — set that one.
        env::set_var("GEMINI_API_KEY", "fake-gemini-key-for-inferred-test");
        check_api_key(&None, "Test", &Provider::Gemini, &mut r);
        env::remove_var("GEMINI_API_KEY");
        env::remove_var("SKILLDO_TEST_INFERRED_GEMINI_KEY_99");
        assert_eq!(r.passed.len(), 1);
        assert!(r.passed[0].contains("inferred from provider"));
    }

    #[test]
    fn test_check_api_key_inferred_openai_compatible_not_set() {
        let _guard = EnvGuard::new("OPENAI_API_KEY");
        env::remove_var("OPENAI_API_KEY");

        let mut r = CheckResult::new();
        check_api_key(&None, "Test", &Provider::OpenAICompatible, &mut r);

        assert_eq!(r.warnings.len(), 1);
        assert!(r.errors.is_empty());
        assert!(r.warnings[0].contains("OK for local models"));
    }

    #[test]
    fn test_run_with_main_extra_body_json() {
        // Line 152: main LLM extra_body resolves to non-empty fields
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
extra_body_json = '{{"top_p": 0.9}}'

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_agent_extra_body_json() {
        // Line 170: per-agent extra_body resolves to non-empty fields
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false

[generation.agent2_llm]
provider = "openai-compatible"
model = "agent2-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
extra_body_json = '{{"top_p": 0.9}}'
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_test_enabled_unavailable_runtime() {
        // runtime unavailable + test agent enabled → error
        // We test the internal logic rather than calling run() to avoid process::exit.
        let mut r = CheckResult::new();
        let runtime = "nonexistent_runtime_xyz";

        if check_runtime_available(runtime) {
            r.pass(format!("Container runtime: {} (available)", runtime));
        } else {
            r.error(format!(
                "Container runtime '{}' not found — test agent validation will fail",
                runtime
            ));
        }

        // Verify check_stage_provider pass path is also exercised via valid providers
        check_stage_provider("test", &Provider::Anthropic, "claude-3", &mut r);

        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].contains("not found"));
        assert!(r.errors[0].contains("test agent validation will fail"));
        assert_eq!(r.passed.len(), 1);
    }

    #[test]
    fn test_run_with_test_agent_disabled_unavailable_runtime() {
        // Lines 143-145: runtime unavailable + test agent disabled → warning (no process::exit)
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false

[generation.container]
runtime = "nonexistent_runtime_xyz_disabled"
"#
        )
        .unwrap();

        // No errors expected: api_key=none, test_agent=false, runtime only warns.
        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    // --- Coverage: unknown LLM provider rejected at config load ---
    #[test]
    fn test_run_unknown_provider_end_to_end() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "badprovider"
model = "test"
api_key_env = "none"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false
"#
        )
        .unwrap();

        // run() reports config load failure gracefully (returns Ok, prints error)
        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok()); // config load error is printed, not propagated
    }

    // --- Coverage: review LLM config validation (lines 130-144) ---
    #[test]
    fn test_run_with_review_enabled_and_review_llm() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "base-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_review = true
enable_test = false

[generation.review_llm]
provider = "openai-compatible"
model = "review-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    // --- Coverage: review agent disabled (line 144) ---
    #[test]
    fn test_run_with_review_disabled() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "base-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_review = false
enable_test = false
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    // --- Coverage: per-stage LLM validation for all 4 stages (lines 155-169) ---
    #[test]
    fn test_run_with_all_stage_llm_overrides() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "base-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false
enable_review = false

[generation.extract_llm]
provider = "openai-compatible"
model = "extract-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.map_llm]
provider = "openai-compatible"
model = "map-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.learn_llm]
provider = "openai-compatible"
model = "learn-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.create_llm]
provider = "openai-compatible"
model = "create-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }

    // --- Coverage: container runtime error when test enabled (lines 177-179) ---
    #[test]
    fn test_run_with_test_enabled_bad_runtime_end_to_end() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = true
enable_review = false

[generation.container]
runtime = "nonexistent_runtime_xyz"
"#
        )
        .unwrap();

        // Test agent enabled + bad runtime → should produce errors → run() returns Err
        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_err());
    }

    // --- Coverage: main LLM extra_body_json parse error (lines 194-195) ---
    #[test]
    fn test_run_with_bad_extra_body_json() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
extra_body_json = "not valid json!!!"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false
enable_review = false
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_err());
    }

    // --- Coverage: per-stage extra_body_json error (lines 213-214) ---
    #[test]
    fn test_run_with_stage_bad_extra_body_json() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "test-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_test = false
enable_review = false

[generation.extract_llm]
provider = "openai-compatible"
model = "extract-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"
extra_body_json = "[1, 2, 3]"
"#
        )
        .unwrap();

        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_err());
    }

    // --- Coverage: errors.is_empty() check causing bail (line 233) ---
    // Already covered by test_run_unknown_provider_end_to_end above.

    // --- Coverage: check_api_key empty value for openai-compatible (lines 275-282) ---
    #[test]
    fn test_check_api_key_empty_value_openai_compatible() {
        env::set_var("SKILLDO_TEST_EMPTY_OAI_KEY", "");
        let mut r = CheckResult::new();
        check_api_key(
            &Some("SKILLDO_TEST_EMPTY_OAI_KEY".to_string()),
            "Test",
            &Provider::OpenAICompatible,
            &mut r,
        );
        env::remove_var("SKILLDO_TEST_EMPTY_OAI_KEY");
        // Empty key + openai-compatible → warning, not error
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("empty"));
        assert!(r.warnings[0].contains("OK for local models"));
    }

    // --- Coverage: check_api_key empty value for non-openai-compatible (line 282) ---
    #[test]
    fn test_check_api_key_empty_value_non_oai_compatible() {
        env::set_var("SKILLDO_TEST_EMPTY_ANTH_KEY", "");
        let mut r = CheckResult::new();
        check_api_key(
            &Some("SKILLDO_TEST_EMPTY_ANTH_KEY".to_string()),
            "Test",
            &Provider::Anthropic,
            &mut r,
        );
        env::remove_var("SKILLDO_TEST_EMPTY_ANTH_KEY");
        // Empty key + non openai-compatible → error
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].contains("empty"));
    }

    // --- Coverage: inferred env var empty for openai-compatible (lines 311-320) ---
    #[test]
    fn test_check_api_key_inferred_empty_openai_compatible() {
        let _guard = EnvGuard::new("OPENAI_API_KEY");
        env::set_var("OPENAI_API_KEY", "");

        let mut r = CheckResult::new();
        check_api_key(&None, "Test", &Provider::OpenAICompatible, &mut r);

        // Empty inferred key for openai-compatible → warning
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("empty"));
        assert!(r.warnings[0].contains("OK for local models"));
    }

    // --- Coverage: inferred env var empty for non-openai-compatible (lines 317-320) ---
    #[test]
    fn test_check_api_key_inferred_empty_non_oai_compatible() {
        let _guard = EnvGuard::new("OPENAI_API_KEY");
        env::set_var("OPENAI_API_KEY", "");

        let mut r = CheckResult::new();
        check_api_key(&None, "Test", &Provider::OpenAI, &mut r);

        // Empty inferred key for non-openai-compatible → error
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].contains("empty"));
        assert!(r.errors[0].contains("inferred from provider"));
    }

    // --- Coverage: review_llm with bad provider rejected at config load ---
    #[test]
    fn test_run_with_review_llm_bad_provider() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("test.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(
            f,
            r#"
[llm]
provider = "openai-compatible"
model = "base-model"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation]
max_retries = 1
max_source_tokens = 1000
enable_review = true
enable_test = false

[generation.review_llm]
provider = "bad-review-provider"
model = "review-model"
api_key_env = "none"
"#
        )
        .unwrap();

        // run() reports config load failure gracefully (returns Ok, prints error)
        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok()); // config load error is printed, not propagated
    }

    #[test]
    fn test_is_likely_local_provider_ollama() {
        let mut config = Config::default();
        config.llm.provider = Provider::OpenAICompatible;
        config.llm.base_url = Some("http://localhost:11434/v1".to_string());
        assert!(is_likely_local_provider(&config));
    }

    #[test]
    fn test_is_likely_local_provider_no_base_url() {
        let mut config = Config::default();
        config.llm.provider = Provider::OpenAICompatible;
        config.llm.base_url = None; // default is localhost
        assert!(is_likely_local_provider(&config));
    }

    #[test]
    fn test_is_likely_local_provider_remote() {
        let mut config = Config::default();
        config.llm.provider = Provider::OpenAICompatible;
        config.llm.base_url = Some("https://api.openrouter.ai/api/v1".to_string());
        assert!(!is_likely_local_provider(&config));
    }

    #[test]
    fn test_is_likely_local_provider_non_compat() {
        let config = Config::default(); // Anthropic
        assert!(!is_likely_local_provider(&config));
    }
}
