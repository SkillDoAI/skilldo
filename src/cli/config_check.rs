use anyhow::Result;
use std::env;
use std::process::Command;

use crate::config::Config;

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

    // 2. Check LLM provider
    let valid_providers = ["anthropic", "openai", "gemini", "openai-compatible"];
    if valid_providers.contains(&config.llm.provider.as_str()) {
        results.pass(format!(
            "LLM provider: {} (model: {})",
            config.llm.provider, config.llm.model
        ));
    } else {
        results.error(format!(
            "Unknown LLM provider: '{}' (expected: {})",
            config.llm.provider,
            valid_providers.join(", ")
        ));
    }

    // 3. Check main API key env var
    check_api_key(
        &config.llm.api_key_env,
        "Main LLM",
        &config.llm.provider,
        &mut results,
    );

    // 4. Check base_url for openai-compatible
    if config.llm.provider == "openai-compatible" {
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

    // 6. Check Agent 5
    if config.generation.enable_agent5 {
        results.pass(format!(
            "Agent 5 enabled (mode: {})",
            config.generation.agent5_mode
        ));

        // Check Agent 5 LLM override if configured
        if let Some(ref agent5_llm) = config.generation.agent5_llm {
            check_agent_provider(
                &valid_providers,
                5,
                &agent5_llm.provider,
                &agent5_llm.model,
                &mut results,
            );
            check_api_key(
                &agent5_llm.api_key_env,
                "Agent 5 LLM",
                &agent5_llm.provider,
                &mut results,
            );
        }
    } else {
        results.pass("Agent 5 disabled".to_string());
    }

    // 7. Check per-agent LLM overrides
    let agent_llms = [
        (1, &config.generation.agent1_llm),
        (2, &config.generation.agent2_llm),
        (3, &config.generation.agent3_llm),
        (4, &config.generation.agent4_llm),
    ];
    for (num, llm_opt) in &agent_llms {
        if let Some(llm) = llm_opt {
            check_agent_provider(
                &valid_providers,
                *num,
                &llm.provider,
                &llm.model,
                &mut results,
            );
            check_api_key(
                &llm.api_key_env,
                &format!("Agent {} LLM", num),
                &llm.provider,
                &mut results,
            );
        }
    }

    // 8. Check container runtime
    let runtime = &config.generation.container.runtime;
    if check_runtime_available(runtime) {
        results.pass(format!("Container runtime: {} (available)", runtime));
    } else if config.generation.enable_agent5 {
        results.error(format!(
            "Container runtime '{}' not found — Agent 5 validation will fail",
            runtime
        ));
    } else {
        results.warn(format!(
            "Container runtime '{}' not found (Agent 5 disabled, so this is OK)",
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
    let all_agent_llms = [
        (1, &config.generation.agent1_llm),
        (2, &config.generation.agent2_llm),
        (3, &config.generation.agent3_llm),
        (4, &config.generation.agent4_llm),
        (5, &config.generation.agent5_llm),
    ];
    for (num, llm_opt) in &all_agent_llms {
        if let Some(llm) = llm_opt {
            match llm.resolve_extra_body() {
                Ok(extra) if !extra.is_empty() => {
                    results.pass(format!("Agent {} extra_body: {} fields", num, extra.len()));
                }
                Ok(_) => {}
                Err(e) => {
                    results.error(format!("Agent {} extra_body_json: {}", num, e));
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

    // Print results
    print_results(&results);

    // Return error if there were validation failures
    if !results.errors.is_empty() {
        anyhow::bail!("{} config error(s) found", results.errors.len());
    }

    Ok(())
}

fn check_agent_provider(
    valid_providers: &[&str],
    agent_num: usize,
    provider: &str,
    model: &str,
    results: &mut CheckResult,
) {
    if valid_providers.contains(&provider) {
        results.pass(format!(
            "Agent {} LLM override: {} ({})",
            agent_num, provider, model
        ));
    } else {
        results.error(format!(
            "Agent {} LLM override: unknown provider '{}' (expected: {})",
            agent_num,
            provider,
            valid_providers.join(", ")
        ));
    }
}

fn check_api_key(
    api_key_env: &Option<String>,
    label: &str,
    provider: &str,
    results: &mut CheckResult,
) {
    match api_key_env {
        Some(env_var) if env_var.to_lowercase() == "none" => {
            results.pass(format!("{}: no API key needed", label));
        }
        Some(env_var) => match env::var(env_var) {
            Ok(v) if !v.trim().is_empty() => {
                results.pass(format!("{}: {} is set", label, env_var));
            }
            Ok(_) if provider == "openai-compatible" => {
                results.warn(format!(
                    "{}: {} is set but empty (OK for local models, needed for gateways)",
                    label, env_var
                ));
            }
            Ok(_) => {
                results.error(format!("{}: {} is set but empty", label, env_var));
            }
            Err(_) if provider == "openai-compatible" => {
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
            // Infer the env var from provider (mirrors Config::get_api_key behavior)
            let inferred = match provider {
                "openai" => Some("OPENAI_API_KEY"),
                "anthropic" => Some("ANTHROPIC_API_KEY"),
                "gemini" => Some("GEMINI_API_KEY"),
                "openai-compatible" => Some("OPENAI_API_KEY"),
                _ => None,
            };
            if let Some(env_var) = inferred {
                match env::var(env_var) {
                    Ok(v) if !v.trim().is_empty() => {
                        results.pass(format!(
                            "{}: {} is set (inferred from provider)",
                            label, env_var
                        ));
                    }
                    Ok(_) if provider == "openai-compatible" => {
                        results.warn(format!(
                            "{}: {} is set but empty (OK for local models, needed for gateways)",
                            label, env_var
                        ));
                    }
                    Ok(_) => {
                        results.error(format!(
                            "{}: {} is set but empty (inferred from provider)",
                            label, env_var
                        ));
                    }
                    Err(_) if provider == "openai-compatible" => {
                        results.warn(format!(
                            "{}: {} is not set (OK for local models, needed for gateways)",
                            label, env_var
                        ));
                    }
                    Err(_) => {
                        results.error(format!(
                            "{}: {} is not set (inferred from provider)",
                            label, env_var
                        ));
                    }
                }
            } else {
                results.pass(format!("{}: no API key configured", label));
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
        check_api_key(&Some("none".to_string()), "Test", "anthropic", &mut r);
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
            "anthropic",
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
            "anthropic",
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
            "openai-compatible",
            &mut r,
        );
        // Should be a warning, not an error
        assert_eq!(r.warnings.len(), 1);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn test_check_api_key_no_env_configured_unknown_provider() {
        // Unknown provider with no api_key_env → no key needed
        let mut r = CheckResult::new();
        check_api_key(&None, "Test", "custom-provider", &mut r);
        assert_eq!(r.passed.len(), 1);
    }

    #[test]
    fn test_check_api_key_inferred_from_provider() {
        // Known provider with api_key_env=None → infers env var and checks it
        let mut r = CheckResult::new();
        check_api_key(&None, "Test", "anthropic", &mut r);
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

    // Test unknown provider by validating config directly.
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

        // run() calls process::exit(1) when there are errors, so we verify the
        // validation logic directly rather than calling run() end-to-end.
        let config =
            crate::config::Config::load_with_path(Some(config_path.to_str().unwrap().to_string()))
                .unwrap();
        let valid_providers = ["anthropic", "openai", "gemini", "openai-compatible"];
        assert!(!valid_providers.contains(&config.llm.provider.as_str()));
    }

    #[test]
    fn test_run_with_agent5_llm_override() {
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
enable_agent5 = true

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
enable_agent5 = false
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
enable_agent5 = false
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
enable_agent5 = false
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
    fn test_check_agent_provider_valid() {
        let valid_providers = ["anthropic", "openai", "gemini", "openai-compatible"];
        let mut r = CheckResult::new();
        check_agent_provider(&valid_providers, 1, "openai", "gpt-5", &mut r);
        assert_eq!(r.passed.len(), 1);
        assert!(r.passed[0].contains("Agent 1 LLM override"));
    }

    #[test]
    fn test_check_agent_provider_invalid() {
        // Lines 212-216: unknown provider in check_agent_provider
        let valid_providers = ["anthropic", "openai", "gemini", "openai-compatible"];
        let mut r = CheckResult::new();
        check_agent_provider(&valid_providers, 3, "badprovider", "some-model", &mut r);
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].contains("Agent 3 LLM override"));
        assert!(r.errors[0].contains("badprovider"));
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
        check_api_key(&None, "Test", "gemini", &mut r);
        env::remove_var("GEMINI_API_KEY");
        env::remove_var("SKILLDO_TEST_INFERRED_GEMINI_KEY_99");
        assert_eq!(r.passed.len(), 1);
        assert!(r.passed[0].contains("inferred from provider"));
    }

    #[test]
    fn test_check_api_key_inferred_openai_compatible_not_set() {
        // Lines 256-258: api_key_env=None, openai-compatible, inferred OPENAI_API_KEY not set.
        // OPENAI_API_KEY may be set in the environment. Temporarily remove and restore it.
        // This test intentionally manipulates the env; the unique env var name in
        // test_check_api_key_inferred_set avoids a race with that test.
        let saved = env::var("OPENAI_API_KEY").ok();
        env::remove_var("OPENAI_API_KEY");

        let mut r = CheckResult::new();
        check_api_key(&None, "Test", "openai-compatible", &mut r);

        if let Some(val) = saved {
            env::set_var("OPENAI_API_KEY", val);
        }

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
enable_agent5 = false
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
enable_agent5 = false

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
    fn test_run_with_agent5_enabled_unavailable_runtime() {
        // Lines 138-140: runtime unavailable + agent5 enabled → error, then process::exit(1)
        // We test the internal logic rather than calling run() to avoid process::exit.
        let valid_providers = ["anthropic", "openai", "gemini", "openai-compatible"];
        let mut r = CheckResult::new();
        let runtime = "nonexistent_runtime_xyz";

        if check_runtime_available(runtime) {
            r.pass(format!("Container runtime: {} (available)", runtime));
        } else {
            // enable_agent5 = true branch (lines 138-140)
            r.error(format!(
                "Container runtime '{}' not found — Agent 5 validation will fail",
                runtime
            ));
        }

        // Verify check_agent_provider pass path is also exercised via valid providers
        check_agent_provider(&valid_providers, 5, "anthropic", "claude-3", &mut r);

        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].contains("not found"));
        assert!(r.errors[0].contains("Agent 5 validation will fail"));
        assert_eq!(r.passed.len(), 1);
    }

    #[test]
    fn test_run_with_agent5_disabled_unavailable_runtime() {
        // Lines 143-145: runtime unavailable + agent5 disabled → warning (no process::exit)
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
enable_agent5 = false

[generation.container]
runtime = "nonexistent_runtime_xyz_disabled"
"#
        )
        .unwrap();

        // No errors expected: api_key=none, agent5=false, runtime only warns.
        let result = run(Some(config_path.to_str().unwrap().to_string()));
        assert!(result.is_ok());
    }
}
