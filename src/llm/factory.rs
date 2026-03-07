use anyhow::Result;
use std::env;
use tracing::{debug, warn};

use super::client::MockLlmClient;
use super::client::{LlmClient, RetryClient};
use super::client_impl::{AnthropicClient, ChatGPTClient, GeminiClient, OpenAIClient};
use crate::config::{Config, LlmConfig, Provider};

/// Create an LLM client from LlmConfig (for test agent or other specialized clients)
pub async fn create_client_from_llm_config(
    llm_config: &LlmConfig,
    dry_run: bool,
) -> Result<Box<dyn LlmClient>> {
    if dry_run {
        return Ok(Box::new(MockLlmClient::new()));
    }

    // Try OAuth token first — if configured and tokens are available
    let oauth_token = if llm_config.has_oauth() {
        if let Some(endpoint) = llm_config.resolve_oauth_endpoint()? {
            crate::auth::resolve_oauth_token(&endpoint).await?
        } else {
            None
        }
    } else {
        None
    };

    // Use OAuth token as API key, or fall back to env var
    let (api_key, use_bearer) = if let Some(token) = oauth_token {
        debug!(
            "Using OAuth token for {}",
            llm_config.resolved_provider_name()
        );
        (token, true)
    } else {
        // Get API key from environment variable (explicit or inferred from provider)
        let env_var = match &llm_config.api_key_env {
            Some(v) => v.clone(),
            None => llm_config.provider.default_api_key_env().to_string(),
        };
        // openai-compatible providers (Ollama, vLLM, etc.) often don't need API keys,
        // so missing/empty keys are silently accepted for that provider.
        let key = if env_var.is_empty() || env_var.to_lowercase() == "none" {
            String::new()
        } else if llm_config.provider == Provider::OpenAICompatible {
            env::var(&env_var).unwrap_or_default()
        } else {
            env::var(&env_var).map_err(|_| {
                if llm_config.has_oauth() {
                    anyhow::anyhow!(
                        "No OAuth tokens found for '{}'. Run `skilldo auth login` first, \
                         or set {} for API key auth.",
                        llm_config.resolved_provider_name(),
                        env_var
                    )
                } else {
                    anyhow::anyhow!("API key not found in environment variable: {}", env_var)
                }
            })?
        };
        (key, false)
    };

    debug!(
        "Creating LLM client: {} ({})",
        llm_config.resolved_provider_name(),
        llm_config.model
    );

    let max_tokens = llm_config.get_max_tokens();
    let extra_body = llm_config.resolve_extra_body()?;
    let extra_headers = llm_config.resolve_extra_headers()?;
    let timeout = llm_config.request_timeout_secs;

    let client: Box<dyn LlmClient> = match llm_config.provider {
        Provider::Anthropic => Box::new(
            AnthropicClient::new(api_key, llm_config.model.clone(), max_tokens, timeout)?
                .with_extra_headers(extra_headers),
        ),

        Provider::OpenAI => Box::new(
            OpenAIClient::new(api_key, llm_config.model.clone(), max_tokens, timeout)?
                .with_extra_body(extra_body)
                .with_extra_headers(extra_headers),
        ),

        Provider::ChatGPT => {
            if !extra_body.is_empty() {
                warn!("extra_body is ignored for ChatGPT provider (Responses API does not support it)");
            }
            Box::new(
                ChatGPTClient::new(
                    api_key,
                    llm_config.model.clone(),
                    max_tokens,
                    timeout,
                    use_bearer,
                    llm_config.base_url.clone(),
                )?
                .with_extra_headers(extra_headers),
            )
        }

        Provider::OpenAICompatible => {
            let base_url = llm_config
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434/v1".to_string());

            Box::new(
                OpenAIClient::with_base_url(
                    api_key,
                    llm_config.model.clone(),
                    base_url,
                    max_tokens,
                    timeout,
                )?
                .with_extra_body(extra_body)
                .with_extra_headers(extra_headers),
            )
        }

        Provider::Gemini => Box::new(
            GeminiClient::new(api_key, llm_config.model.clone(), max_tokens, timeout)?
                .with_bearer_auth(use_bearer)
                .with_extra_headers(extra_headers),
        ),
    };

    Ok(Box::new(RetryClient::new(
        client,
        llm_config.network_retries,
        llm_config.retry_delay,
    )))
}

/// Create an LLM client based on configuration
pub async fn create_client(config: &Config, dry_run: bool) -> Result<Box<dyn LlmClient>> {
    create_client_from_llm_config(&config.llm, dry_run).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    #[tokio::test]
    async fn test_create_mock_client_for_dry_run() {
        let config = Config::default();
        create_client(&config, true).await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_create_anthropic_client() {
        env::set_var("SKILLDO_TEST_FACTORY_ANTHRO", "test_key");
        let mut config = Config::default();
        config.llm.api_key_env = Some("SKILLDO_TEST_FACTORY_ANTHRO".to_string());
        let result = create_client(&config, false).await;
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_ANTHRO");
    }

    #[tokio::test]
    #[serial]
    async fn test_create_openai_client() {
        env::set_var("SKILLDO_TEST_FACTORY_OAI", "test_key");
        let mut config = Config::default();
        config.llm.provider = Provider::OpenAI;
        config.llm.api_key_env = Some("SKILLDO_TEST_FACTORY_OAI".to_string());
        let result = create_client(&config, false).await;
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_OAI");
    }

    #[tokio::test]
    async fn test_create_openai_compatible_client() {
        let mut config = Config::default();
        config.llm.provider = Provider::OpenAICompatible;
        config.llm.base_url = Some("http://localhost:11434/v1".to_string());
        let result = create_client(&config, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_create_gemini_client() {
        env::set_var("SKILLDO_TEST_FACTORY_GEMINI", "test_key");
        let mut config = Config::default();
        config.llm.provider = Provider::Gemini;
        config.llm.model = "gemini-pro".to_string();
        config.llm.api_key_env = Some("SKILLDO_TEST_FACTORY_GEMINI".to_string());
        let result = create_client(&config, false).await;
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_GEMINI");
    }

    #[tokio::test]
    async fn test_create_client_without_api_key() {
        let mut config = Config::default();
        config.llm.api_key_env = Some("SKILLDO_TEST_NONEXISTENT_KEY_FACTORY_99999".to_string());
        let result = create_client(&config, false).await;
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
        provider: Provider,
        api_key_env: Option<&str>,
        base_url: Option<&str>,
    ) -> LlmConfig {
        LlmConfig {
            provider,
            provider_name: None,
            model: "test-model".to_string(),
            api_key_env: api_key_env.map(|s| s.to_string()),
            base_url: base_url.map(|s| s.to_string()),
            max_tokens: None,
            network_retries: 0,
            retry_delay: 1,
            extra_body: std::collections::HashMap::new(),
            extra_body_json: None,
            request_timeout_secs: 120,
            oauth_auth_url: None,
            oauth_token_url: None,
            oauth_scopes: None,
            oauth_client_id_env: None,
            oauth_client_secret_env: None,
            oauth_credentials_env: None,
            extra_headers: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_create_client_from_llm_config_dry_run() {
        let config = make_llm_config(Provider::Anthropic, None, None);
        let result = create_client_from_llm_config(&config, true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_create_client_from_llm_config_anthropic() {
        env::set_var("SKILLDO_TEST_FACTORY_LLM_KEY_1", "test_key");
        let config = make_llm_config(
            Provider::Anthropic,
            Some("SKILLDO_TEST_FACTORY_LLM_KEY_1"),
            None,
        );
        let result = create_client_from_llm_config(&config, false).await;
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_LLM_KEY_1");
    }

    #[tokio::test]
    #[serial]
    async fn test_create_client_from_llm_config_openai() {
        env::set_var("SKILLDO_TEST_FACTORY_LLM_KEY_2", "test_key");
        let config = make_llm_config(
            Provider::OpenAI,
            Some("SKILLDO_TEST_FACTORY_LLM_KEY_2"),
            None,
        );
        let result = create_client_from_llm_config(&config, false).await;
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_LLM_KEY_2");
    }

    #[tokio::test]
    #[serial]
    async fn test_create_client_from_llm_config_gemini() {
        env::set_var("SKILLDO_TEST_FACTORY_LLM_KEY_3", "test_key");
        let config = make_llm_config(
            Provider::Gemini,
            Some("SKILLDO_TEST_FACTORY_LLM_KEY_3"),
            None,
        );
        let result = create_client_from_llm_config(&config, false).await;
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_LLM_KEY_3");
    }

    #[tokio::test]
    async fn test_create_client_from_llm_config_openai_compatible() {
        let config = make_llm_config(
            Provider::OpenAICompatible,
            None,
            Some("http://localhost:11434/v1"),
        );
        let result = create_client_from_llm_config(&config, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_create_client_from_llm_config_chatgpt() {
        env::set_var("SKILLDO_TEST_FACTORY_LLM_KEY_4", "test_key");
        let config = make_llm_config(
            Provider::ChatGPT,
            Some("SKILLDO_TEST_FACTORY_LLM_KEY_4"),
            None,
        );
        let result = create_client_from_llm_config(&config, false).await;
        assert!(result.is_ok());
        env::remove_var("SKILLDO_TEST_FACTORY_LLM_KEY_4");
    }

    #[tokio::test]
    async fn test_create_client_from_llm_config_no_api_key() {
        let config = make_llm_config(
            Provider::Anthropic,
            Some("SKILLDO_TEST_FACTORY_LLM_NONEXISTENT_KEY"),
            None,
        );
        let result = create_client_from_llm_config(&config, false).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("API key not found"));
        }
    }

    #[tokio::test]
    async fn test_create_client_from_llm_config_api_key_none() {
        let config = make_llm_config(Provider::Anthropic, Some("none"), None);
        let result = create_client_from_llm_config(&config, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_create_client_with_oauth_token() {
        let provider_name = "test-factory-oauth";
        let tokens = crate::auth::TokenSet {
            access_token: "oauth-test-token".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: u64::MAX,
        };
        crate::auth::save_tokens(provider_name, &tokens).unwrap();

        env::set_var("SKILLDO_TEST_FACTORY_OAUTH_CID", "client-id");

        let mut config = make_llm_config(Provider::OpenAI, Some("none"), None);
        config.provider_name = Some(provider_name.to_string());
        config.oauth_auth_url = Some("https://auth.example.com/authorize".to_string());
        config.oauth_token_url = Some("https://auth.example.com/token".to_string());
        config.oauth_client_id_env = Some("SKILLDO_TEST_FACTORY_OAUTH_CID".to_string());

        let result = create_client_from_llm_config(&config, false).await;
        assert!(result.is_ok());

        crate::auth::delete_tokens(provider_name).unwrap();
        env::remove_var("SKILLDO_TEST_FACTORY_OAUTH_CID");
    }

    #[tokio::test]
    #[serial]
    async fn test_create_gemini_client_with_bearer_auth() {
        let provider_name = "test-factory-gemini-oauth";
        let tokens = crate::auth::TokenSet {
            access_token: "gemini-oauth-token".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: u64::MAX,
        };
        crate::auth::save_tokens(provider_name, &tokens).unwrap();

        env::set_var("SKILLDO_TEST_FACTORY_GEMINI_OAUTH_CID", "gcid");

        let mut config = make_llm_config(Provider::Gemini, Some("none"), None);
        config.provider_name = Some(provider_name.to_string());
        config.oauth_auth_url = Some("https://accounts.google.com/o/oauth2/auth".to_string());
        config.oauth_token_url = Some("https://oauth2.googleapis.com/token".to_string());
        config.oauth_client_id_env = Some("SKILLDO_TEST_FACTORY_GEMINI_OAUTH_CID".to_string());

        let result = create_client_from_llm_config(&config, false).await;
        assert!(result.is_ok());

        crate::auth::delete_tokens(provider_name).unwrap();
        env::remove_var("SKILLDO_TEST_FACTORY_GEMINI_OAUTH_CID");
    }
}
