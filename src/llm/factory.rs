use anyhow::{bail, Result};
use std::env;

use super::client::LlmClient;
use super::client::MockLlmClient;
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

    match llm_config.provider.as_str() {
        "anthropic" => Ok(Box::new(AnthropicClient::new(
            api_key,
            llm_config.model.clone(),
            max_tokens,
        ))),

        "openai" => Ok(Box::new(OpenAIClient::new(
            api_key,
            llm_config.model.clone(),
            max_tokens,
        ))),

        "openai-compatible" => {
            let base_url = llm_config
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434/v1".to_string());

            Ok(Box::new(OpenAIClient::with_base_url(
                api_key,
                llm_config.model.clone(),
                base_url,
                max_tokens,
            )))
        }

        "gemini" => Ok(Box::new(GeminiClient::new(
            api_key,
            llm_config.model.clone(),
            max_tokens,
        ))),

        unknown => bail!("Unknown LLM provider: {}", unknown),
    }
}

/// Create an LLM client based on configuration
pub fn create_client(config: &Config, dry_run: bool) -> Result<Box<dyn LlmClient>> {
    if dry_run {
        return Ok(Box::new(MockLlmClient::new()));
    }

    let api_key = config.get_api_key()?;
    let max_tokens = config.llm.get_max_tokens();

    match config.llm.provider.as_str() {
        "anthropic" => Ok(Box::new(AnthropicClient::new(
            api_key,
            config.llm.model.clone(),
            max_tokens,
        ))),

        "openai" => Ok(Box::new(OpenAIClient::new(
            api_key,
            config.llm.model.clone(),
            max_tokens,
        ))),

        "openai-compatible" => {
            let base_url = config
                .llm
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434/v1".to_string());

            Ok(Box::new(OpenAIClient::with_base_url(
                api_key,
                config.llm.model.clone(),
                base_url,
                max_tokens,
            )))
        }

        "gemini" => Ok(Box::new(GeminiClient::new(
            api_key,
            config.llm.model.clone(),
            max_tokens,
        ))),

        unknown => bail!("Unknown LLM provider: {}", unknown),
    }
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
}
