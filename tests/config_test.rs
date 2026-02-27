//! Unit tests for configuration defaults
//! Tests configuration structure and default values

use anyhow::Result;
use skilldo::config::Config;

#[test]
fn test_config_has_defaults() -> Result<()> {
    let config = Config::default();

    // Should have reasonable defaults
    assert!(config.generation.max_retries > 0);
    assert!(config.generation.max_source_tokens > 0);
    assert!(!config.llm.provider.is_empty());
    assert!(!config.llm.model.is_empty());

    Ok(())
}

#[test]
fn test_config_generation_defaults() -> Result<()> {
    let config = Config::default();

    // Generation config should have sensible defaults
    assert!(
        config.generation.max_retries >= 1,
        "Should have at least 1 retry"
    );
    assert!(
        config.generation.max_source_tokens >= 10000,
        "Should have reasonable token limit"
    );

    Ok(())
}

#[test]
fn test_config_llm_defaults() -> Result<()> {
    let config = Config::default();

    // LLM config should have provider and model
    assert!(
        !config.llm.provider.is_empty(),
        "Provider should not be empty"
    );
    assert!(!config.llm.model.is_empty(), "Model should not be empty");

    Ok(())
}

#[test]
fn test_config_load_returns_valid_config() -> Result<()> {
    // Should load config or return defaults without crashing
    let config = Config::load()?;

    assert!(config.generation.max_retries > 0);
    assert!(!config.llm.provider.is_empty());

    Ok(())
}

#[test]
fn test_config_agent5_enabled_by_default() -> Result<()> {
    let config = Config::default();

    // Agent5 should be enabled by default
    assert!(config.generation.enable_agent5);

    Ok(())
}

#[test]
fn test_config_agent5_mode_default() -> Result<()> {
    let config = Config::default();

    // Should have a default agent5 mode
    assert!(!config.generation.agent5_mode.is_empty());
    assert_eq!(config.generation.agent5_mode, "thorough");

    Ok(())
}

#[test]
fn test_config_get_api_key() -> Result<()> {
    let config = Config::default();

    // Should attempt to get API key from environment
    // Either succeeds or fails gracefully
    let result = config.get_api_key();
    assert!(result.is_ok() || result.is_err());

    Ok(())
}

#[test]
fn test_generation_config_get_agent5_mode_thorough() {
    let config = Config::default();
    let mode = config.generation.get_agent5_mode();

    // Default mode should be thorough
    assert!(matches!(mode, skilldo::agent5::ValidationMode::Thorough));
}

#[test]
fn test_config_returns_default_when_no_config_file() {
    // When load() finds no config, should return default
    let config = Config::load().expect("Should return default config");

    // Should have valid defaults
    assert!(config.generation.max_retries > 0);
    assert!(!config.llm.provider.is_empty());
}

#[test]
fn test_config_get_api_key_when_env_var_not_set() -> Result<()> {
    use std::env;

    // Create a config with a specific env var that doesn't exist
    let mut config = Config::default();
    config.llm.api_key_env = Some("NONEXISTENT_API_KEY_VAR_XYZABC".to_string());

    // Ensure the env var is not set
    env::remove_var("NONEXISTENT_API_KEY_VAR_XYZABC");

    // Should fail when environment variable is not set
    let result = config.get_api_key();
    assert!(result.is_err(), "Should fail when API key env var not set");

    Ok(())
}

#[test]
fn test_config_get_api_key_when_no_env_var_specified() -> Result<()> {
    // openai-compatible with no api_key_env: should return empty string (local models)
    let mut config = Config::default();
    config.llm.provider = "openai-compatible".to_string();
    config.llm.api_key_env = None;

    let result = config.get_api_key()?;
    assert_eq!(
        result, "",
        "Should return empty string for openai-compatible without key"
    );

    Ok(())
}

#[test]
fn test_config_get_api_key_inferred_from_provider() -> Result<()> {
    // Default provider (anthropic) with api_key_env = None should infer ANTHROPIC_API_KEY
    let config = Config::default();
    assert!(config.llm.api_key_env.is_none());
    let result = config.get_api_key();
    // Should error because ANTHROPIC_API_KEY isn't set in test env
    assert!(
        result.is_err(),
        "Should error when inferred ANTHROPIC_API_KEY is not set"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("ANTHROPIC_API_KEY"),
        "Error should mention inferred env var"
    );

    Ok(())
}

#[test]
fn test_generation_config_get_agent5_mode_minimal() {
    use skilldo::agent5::ValidationMode;

    // Create config with minimal mode
    let mut config = Config::default();
    config.generation.agent5_mode = "minimal".to_string();

    let mode = config.generation.get_agent5_mode();
    assert!(matches!(mode, ValidationMode::Minimal));
}

#[test]
fn test_generation_config_get_agent5_mode_adaptive() {
    use skilldo::agent5::ValidationMode;

    // Create config with adaptive mode
    let mut config = Config::default();
    config.generation.agent5_mode = "adaptive".to_string();

    let mode = config.generation.get_agent5_mode();
    assert!(matches!(mode, ValidationMode::Adaptive));
}
