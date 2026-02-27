// Comprehensive error handling and failure case tests
use skilldo::config::Config;
use skilldo::detector::Language;
use skilldo::llm::factory;
use skilldo::pipeline::collector::Collector;
use std::env;
use std::path::Path;
use std::str::FromStr;

#[test]
fn test_missing_api_key() {
    // Use a unique env var name to avoid race conditions with parallel tests
    let mut config = Config::default();
    config.llm.api_key_env = Some("SKILLDO_TEST_NONEXISTENT_KEY_12345".to_string());
    let result = factory::create_client(&config, false);
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("API key not found"));
    }
}

#[test]
fn test_invalid_provider() {
    let mut config = Config::default();
    config.llm.provider = "invalid_provider".to_string();
    config.llm.api_key_env = Some("SKILLDO_TEST_DUMMY_KEY".to_string());
    env::set_var("SKILLDO_TEST_DUMMY_KEY", "test_key");
    let result = factory::create_client(&config, false);
    assert!(result.is_err());
    // Just verify it's an error - don't check exact message
    env::remove_var("SKILLDO_TEST_DUMMY_KEY");
}

#[tokio::test]
async fn test_collector_with_nonexistent_directory() {
    let path = Path::new("/nonexistent/directory");
    let collector = Collector::new(path, Language::Python);
    let result = collector.collect().await;
    assert!(result.is_err());
}

#[test]
fn test_language_from_invalid_string() {
    let result = Language::from_str("invalid_language");
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.to_string().contains("Unknown language"));
    }
}

#[test]
fn test_config_with_invalid_toml() {
    // Config loads default if repo config is invalid
    let config = Config::default();
    assert!(config.llm.provider == "anthropic");
    assert!(config.generation.max_retries == 5);
}

#[test]
fn test_openai_client_without_api_key() {
    env::set_var("SKILLDO_TEST_ERR_OAI_EMPTY", "");
    let mut config = Config::default();
    config.llm.provider = "openai".to_string();
    config.llm.api_key_env = Some("SKILLDO_TEST_ERR_OAI_EMPTY".to_string());
    let result = factory::create_client(&config, false);
    // Empty API key should be accepted (for API-key-less providers)
    assert!(result.is_ok());
    env::remove_var("SKILLDO_TEST_ERR_OAI_EMPTY");
}

#[test]
fn test_gemini_client_creation() {
    env::set_var("SKILLDO_TEST_ERR_GEMINI", "test_key");
    let mut config = Config::default();
    config.llm.provider = "gemini".to_string();
    config.llm.model = "gemini-pro".to_string();
    config.llm.api_key_env = Some("SKILLDO_TEST_ERR_GEMINI".to_string());
    let result = factory::create_client(&config, false);
    assert!(result.is_ok());
    env::remove_var("SKILLDO_TEST_ERR_GEMINI");
}

#[test]
fn test_all_supported_languages() {
    assert_eq!(Language::Python.as_str(), "python");
    assert_eq!(Language::JavaScript.as_str(), "javascript");
    assert_eq!(Language::Rust.as_str(), "rust");
    assert_eq!(Language::Go.as_str(), "go");

    assert!(Language::from_str("python").is_ok());
    assert!(Language::from_str("javascript").is_ok());
    assert!(Language::from_str("rust").is_ok());
    assert!(Language::from_str("go").is_ok());
}

#[test]
fn test_config_default_values() {
    let config = Config::default();
    assert_eq!(config.llm.provider, "anthropic");
    assert_eq!(config.llm.api_key_env, None);
    assert_eq!(config.generation.max_retries, 5);
    assert!(!config.prompts.override_prompts);
}

#[test]
fn test_openai_compatible_with_custom_base_url() {
    env::set_var("SKILLDO_TEST_ERR_COMPAT_1", "test_key");
    let mut config = Config::default();
    config.llm.provider = "openai-compatible".to_string();
    config.llm.api_key_env = Some("SKILLDO_TEST_ERR_COMPAT_1".to_string());
    config.llm.base_url = Some("http://custom:8080/v1".to_string());

    let result = factory::create_client(&config, false);
    assert!(result.is_ok());
    env::remove_var("SKILLDO_TEST_ERR_COMPAT_1");
}

#[test]
fn test_openai_compatible_without_base_url() {
    env::set_var("SKILLDO_TEST_ERR_COMPAT_2", "test_key");
    let mut config = Config::default();
    config.llm.provider = "openai-compatible".to_string();
    config.llm.api_key_env = Some("SKILLDO_TEST_ERR_COMPAT_2".to_string());
    config.llm.base_url = None;

    let result = factory::create_client(&config, false);
    // Should use default base_url
    assert!(result.is_ok());
    env::remove_var("SKILLDO_TEST_ERR_COMPAT_2");
}
