use anyhow::{bail, Result};
use std::env;

use super::client::MockLlmClient;
use super::client::{LlmClient, RetryClient};
use super::client_impl::{AnthropicClient, GeminiClient, OpenAIClient};
use crate::config::{Config, LlmConfig};

/// Create an LLM client from LlmConfig (for Agent 5 or other specialized clients)
pub fn create_client_from_llm_config(
    llm_config: &LlmConfig,
    dry_run: bool,
) -> Result<Box<dyn LlmClient>> {
    if dry_run {
        return Ok(Box::new(MockLlmClient::new()));
    }

    // Get API key from environment variable if specified
    let api_key = if let Some(ref env_var) = llm_config.api_key_env {
        if env_var.to_lowercase() == "none" || llm_config.provider == "openai-compatible" {
            env::var(env_var).unwrap_or_default()
        } else {
            env::var(env_var).map_err(|_| {
                anyhow::anyhow!("API key not found in environment variable: {}", env_var)
            })?
        }
    } else {
        String::new() // No API key needed (e.g., for local models)
    };

    let max_tokens = llm_config.get_max_tokens();

    let client: Box<dyn LlmClient> = match llm_config.provider.as_str() {
        "anthropic" => Box::new(AnthropicClient::new(
            api_key,
            llm_config.model.clone(),
            max_tokens,
        )),

        "openai" => Box::new(OpenAIClient::new(
            api_key,
            llm_config.model.clone(),
            max_tokens,
        )),

        "openai-compatible" => {
            let base_url = llm_config
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434/v1".to_string());

            Box::new(OpenAIClient::with_base_url(
                api_key,
                llm_config.model.clone(),
                base_url,
                max_tokens,
            ))
        }

        "gemini" => Box::new(GeminiClient::new(
            api_key,
            llm_config.model.clone(),
            max_tokens,
        )),

        unknown => bail!("Unknown LLM provider: {}", unknown),
    };

    Ok(Box::new(RetryClient::new(
        client,
        llm_config.network_retries,
        llm_config.retry_delay,
    )))
}

/// Create an LLM client based on configuration
pub fn create_client(config: &Config, dry_run: bool) -> Result<Box<dyn LlmClient>> {
    if dry_run {
        return Ok(Box::new(MockLlmClient::new()));
    }

    let api_key = config.get_api_key()?;
    let max_tokens = config.llm.get_max_tokens();

    let client: Box<dyn LlmClient> = match config.llm.provider.as_str() {
        "anthropic" => Box::new(AnthropicClient::new(
            api_key,
            config.llm.model.clone(),
            max_tokens,
        )),

        "openai" => Box::new(OpenAIClient::new(
            api_key,
            config.llm.model.clone(),
            max_tokens,
        )),

        "openai-compatible" => {
            let base_url = config
                .llm
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434/v1".to_string());

            Box::new(OpenAIClient::with_base_url(
                api_key,
                config.llm.model.clone(),
                base_url,
                max_tokens,
            ))
        }

        "gemini" => Box::new(GeminiClient::new(
            api_key,
            config.llm.model.clone(),
            max_tokens,
        )),

        unknown => bail!("Unknown LLM provider: {}", unknown),
    };

    Ok(Box::new(RetryClient::new(
        client,
        config.llm.network_retries,
        config.llm.retry_delay,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_create_mock_client_for_dry_run() {
        let config = Config::default();
        // Succeeding without panic proves mock client was created
        create_client(&config, true).unwrap();
    }

    #[test]
    fn test_create_anthropic_client() {
        env::set_var("AI_API_KEY", "test_key");
        let config = Config::default(); // Defaults to anthropic
        let result = create_client(&config, false);
        assert!(result.is_ok());
        env::remove_var("AI_API_KEY");
    }

    #[test]
    fn test_create_openai_client() {
        env::set_var("AI_API_KEY", "test_key");
        let mut config = Config::default();
        config.llm.provider = "openai".to_string();
        let result = create_client(&config, false);
        assert!(result.is_ok());
        env::remove_var("AI_API_KEY");
    }

    #[test]
    fn test_create_openai_compatible_client() {
        env::set_var("AI_API_KEY", "test_key");
        let mut config = Config::default();
        config.llm.provider = "openai-compatible".to_string();
        config.llm.base_url = Some("http://localhost:11434/v1".to_string());
        let result = create_client(&config, false);
        assert!(result.is_ok());
        env::remove_var("AI_API_KEY");
    }

    #[test]
    fn test_create_client_with_unknown_provider() {
        env::set_var("AI_API_KEY", "test_key");
        let mut config = Config::default();
        config.llm.provider = "unknown_provider".to_string();
        let result = create_client(&config, false);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Unknown LLM provider"));
        }
        env::remove_var("AI_API_KEY");
    }

    #[test]
    fn test_create_gemini_client() {
        env::set_var("AI_API_KEY", "test_key");
        let mut config = Config::default();
        config.llm.provider = "gemini".to_string();
        config.llm.model = "gemini-pro".to_string();
        let result = create_client(&config, false);
        assert!(result.is_ok());
        env::remove_var("AI_API_KEY");
    }

    #[test]
    fn test_create_client_without_api_key() {
        // Use a unique nonexistent env var to avoid race conditions with parallel tests
        let mut config = Config::default();
        config.llm.api_key_env = Some("SKILLDO_TEST_NONEXISTENT_KEY_FACTORY_99999".to_string());
        let result = create_client(&config, false);
        assert!(
            result.is_err(),
            "Expected error when API key is missing, but got Ok(client)"
        );
        if let Err(e) = result {
            assert!(e.to_string().contains("API key not found"));
        }
    }

    // --- Tests for create_client_from_llm_config ---

    fn make_llm_config(
        provider: &str,
        api_key_env: Option<&str>,
        base_url: Option<&str>,
    ) -> LlmConfig {
        LlmConfig {
            provider: provider.to_string(),
            model: "test-model".to_string(),
            api_key_env: api_key_env.map(|s| s.to_string()),
            base_url: base_url.map(|s| s.to_string()),
            max_tokens: None,
            network_retries: 0,
            retry_delay: 1,
        }
    }

    #[test]
    fn test_create_client_from_llm_config_dry_run() {
        let config = make_llm_config("anthropic", None, None);
        let result = create_client_from_llm_config(&config, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_client_from_llm_config_anthropic() {
        env::set_var("SKILLDO_TEST_FACTORY_LLM_KEY_1", "test_key");
        let config = make_llm_config("anthropic", Some("SKILLDO_TEST_FACTORY_LLM_KEY_1"), None);
        let result = create_client_from_llm_config(&config, false);
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_LLM_KEY_1");
    }

    #[test]
    fn test_create_client_from_llm_config_openai() {
        env::set_var("SKILLDO_TEST_FACTORY_LLM_KEY_2", "test_key");
        let config = make_llm_config("openai", Some("SKILLDO_TEST_FACTORY_LLM_KEY_2"), None);
        let result = create_client_from_llm_config(&config, false);
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_LLM_KEY_2");
    }

    #[test]
    fn test_create_client_from_llm_config_gemini() {
        env::set_var("SKILLDO_TEST_FACTORY_LLM_KEY_3", "test_key");
        let config = make_llm_config("gemini", Some("SKILLDO_TEST_FACTORY_LLM_KEY_3"), None);
        let result = create_client_from_llm_config(&config, false);
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_LLM_KEY_3");
    }

    #[test]
    fn test_create_client_from_llm_config_openai_compatible() {
        let config = make_llm_config("openai-compatible", None, Some("http://localhost:11434/v1"));
        let result = create_client_from_llm_config(&config, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_client_from_llm_config_unknown_provider() {
        let config = make_llm_config("unknown_provider", None, None);
        let result = create_client_from_llm_config(&config, false);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Unknown LLM provider"));
        }
    }

    #[test]
    fn test_create_client_from_llm_config_no_api_key() {
        let config = make_llm_config(
            "anthropic",
            Some("SKILLDO_TEST_FACTORY_LLM_NONEXISTENT_KEY"),
            None,
        );
        let result = create_client_from_llm_config(&config, false);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("API key not found"));
        }
    }

    #[test]
    fn test_create_client_from_llm_config_api_key_none() {
        // When api_key_env is "none", the function should not error (unwrap_or_default path)
        let config = make_llm_config("anthropic", Some("none"), None);
        let result = create_client_from_llm_config(&config, false);
        assert!(result.is_ok());
    }
}
