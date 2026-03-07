use anyhow::Result;
use std::collections::HashSet;
use tracing::info;

use crate::auth::{self, OAuthEndpoint};
use crate::config::Config;

/// Run the OAuth login flow for all configured OAuth providers.
pub async fn login(config_path: Option<String>) -> Result<()> {
    let config = Config::load_with_path(config_path)?;
    let endpoints = collect_unique_endpoints(&config)?;

    if endpoints.is_empty() {
        anyhow::bail!(
            "No OAuth endpoints configured. Add oauth_auth_url, oauth_token_url, and \
             oauth_client_id_env to your skilldo.toml config."
        );
    }

    for endpoint in &endpoints {
        info!("Authenticating with {}...", endpoint.provider_name);

        let tokens = if auth::device_code::should_use_device_code(endpoint) {
            auth::device_code::device_code_login(endpoint).await?
        } else {
            let (verifier, challenge) = auth::pkce::generate_pkce();
            let state = auth::pkce::generate_state();

            let url = auth::oauth::build_auth_url(endpoint, &challenge, &state);
            auth::oauth::open_auth_url(&url);

            let code = auth::oauth::start_callback_server(&state, endpoint).await?;
            auth::oauth::exchange_code(endpoint, &code, &verifier).await?
        };

        auth::save_tokens(&endpoint.provider_name, &tokens)?;
        info!(
            "Authenticated with {} successfully.",
            endpoint.provider_name
        );
    }

    Ok(())
}

/// Show OAuth token status for all configured providers.
pub fn status(config_path: Option<String>) -> Result<()> {
    let config = Config::load_with_path(config_path)?;
    let endpoints = collect_unique_endpoints(&config)?;

    if endpoints.is_empty() {
        println!("No OAuth endpoints configured.");
        return Ok(());
    }

    for endpoint in &endpoints {
        match auth::load_tokens(&endpoint.provider_name)? {
            Some(tokens) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                if tokens.is_expired() {
                    let ago = now.saturating_sub(tokens.expires_at);
                    println!(
                        "{}: EXPIRED (expired {}s ago, has refresh token: {})",
                        endpoint.provider_name,
                        ago,
                        !tokens.refresh_token.is_empty()
                    );
                } else {
                    let remaining = tokens.expires_at.saturating_sub(now);
                    println!(
                        "{}: VALID (expires in {}s)",
                        endpoint.provider_name, remaining
                    );
                }
            }
            None => {
                println!("{}: NOT LOGGED IN", endpoint.provider_name);
            }
        }
    }

    Ok(())
}

/// Delete all stored OAuth tokens for configured providers.
pub fn logout(config_path: Option<String>) -> Result<()> {
    let config = Config::load_with_path(config_path)?;
    let endpoints = collect_unique_endpoints(&config)?;

    if endpoints.is_empty() {
        println!("No OAuth endpoints configured.");
        return Ok(());
    }

    for endpoint in &endpoints {
        auth::delete_tokens(&endpoint.provider_name)?;
        info!("Logged out of {}.", endpoint.provider_name);
    }

    Ok(())
}

/// Collect unique OAuth endpoints from the config (dedup by token_url + client_id).
fn collect_unique_endpoints(config: &Config) -> Result<Vec<OAuthEndpoint>> {
    let mut seen = HashSet::new();
    let mut endpoints = Vec::new();

    // Collect from all LlmConfig sources: global + per-stage overrides
    let llm_configs: Vec<&crate::config::LlmConfig> = std::iter::once(&config.llm)
        .chain(config.generation.extract_llm.as_ref())
        .chain(config.generation.map_llm.as_ref())
        .chain(config.generation.learn_llm.as_ref())
        .chain(config.generation.create_llm.as_ref())
        .chain(config.generation.review_llm.as_ref())
        .chain(config.generation.test_llm.as_ref())
        .collect();

    for llm_config in llm_configs {
        if let Some(endpoint) = llm_config.resolve_oauth_endpoint()? {
            let key = format!("{}|{}", endpoint.token_url, endpoint.client_id);
            if seen.insert(key) {
                endpoints.push(endpoint);
            }
        }
    }

    Ok(endpoints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn collect_unique_endpoints_empty_by_default() {
        let config = Config::default();
        let endpoints = collect_unique_endpoints(&config).unwrap();
        assert!(endpoints.is_empty());
    }

    #[test]
    fn collect_unique_endpoints_deduplicates() {
        let mut config = Config::default();
        // Set OAuth on both global and extract stage with same endpoint
        config.llm.oauth_auth_url = Some("https://auth.example.com/authorize".to_string());
        config.llm.oauth_token_url = Some("https://auth.example.com/token".to_string());
        config.llm.oauth_client_id_env = Some("SKILLDO_TEST_AUTH_CID".to_string());

        std::env::set_var("SKILLDO_TEST_AUTH_CID", "same-client");

        let mut extract_llm = config.llm.clone();
        extract_llm.provider_name = Some("extract-provider".to_string());
        config.generation.extract_llm = Some(extract_llm);

        let endpoints = collect_unique_endpoints(&config).unwrap();
        // Same token_url + client_id → deduplicated to 1
        assert_eq!(endpoints.len(), 1);

        std::env::remove_var("SKILLDO_TEST_AUTH_CID");
    }

    #[test]
    fn collect_unique_endpoints_different_providers() {
        let mut config = Config::default();
        config.llm.oauth_auth_url = Some("https://auth1.example.com/authorize".to_string());
        config.llm.oauth_token_url = Some("https://auth1.example.com/token".to_string());
        config.llm.oauth_client_id_env = Some("SKILLDO_TEST_AUTH_CID1".to_string());

        std::env::set_var("SKILLDO_TEST_AUTH_CID1", "client1");
        std::env::set_var("SKILLDO_TEST_AUTH_CID2", "client2");

        let mut test_llm = config.llm.clone();
        test_llm.oauth_auth_url = Some("https://auth2.example.com/authorize".to_string());
        test_llm.oauth_token_url = Some("https://auth2.example.com/token".to_string());
        test_llm.oauth_client_id_env = Some("SKILLDO_TEST_AUTH_CID2".to_string());
        test_llm.provider_name = Some("test-provider".to_string());
        config.generation.test_llm = Some(test_llm);

        let endpoints = collect_unique_endpoints(&config).unwrap();
        assert_eq!(endpoints.len(), 2);

        std::env::remove_var("SKILLDO_TEST_AUTH_CID1");
        std::env::remove_var("SKILLDO_TEST_AUTH_CID2");
    }

    #[test]
    fn status_no_endpoints_prints_message() {
        // Default config has no OAuth endpoints
        let result = status(None);
        assert!(result.is_ok());
    }

    #[test]
    fn logout_no_endpoints_prints_message() {
        let result = logout(None);
        assert!(result.is_ok());
    }

    #[test]
    fn login_no_endpoints_errors() {
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(login(None));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No OAuth endpoints configured"));
    }

    #[test]
    fn status_shows_valid_token() {
        let provider = "test-cli-auth-status-valid";
        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: u64::MAX,
        };
        auth::save_tokens(provider, &tokens).unwrap();

        // Create a config with OAuth pointing to this provider
        let mut config = Config::default();
        config.llm.oauth_auth_url = Some("https://auth.example.com/authorize".to_string());
        config.llm.oauth_token_url = Some("https://auth.example.com/token".to_string());
        config.llm.oauth_client_id_env = Some("SKILLDO_TEST_STATUS_CID".to_string());
        config.llm.provider_name = Some(provider.to_string());

        std::env::set_var("SKILLDO_TEST_STATUS_CID", "test-client");
        let endpoints = collect_unique_endpoints(&config).unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].provider_name, provider);

        // Verify token loads correctly
        let loaded = auth::load_tokens(provider).unwrap().unwrap();
        assert!(!loaded.is_expired());

        auth::delete_tokens(provider).unwrap();
        std::env::remove_var("SKILLDO_TEST_STATUS_CID");
    }

    #[test]
    fn status_shows_expired_token() {
        let provider = "test-cli-auth-status-expired";
        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: 0,
        };
        auth::save_tokens(provider, &tokens).unwrap();

        let loaded = auth::load_tokens(provider).unwrap().unwrap();
        assert!(loaded.is_expired());

        auth::delete_tokens(provider).unwrap();
    }

    #[test]
    fn status_shows_not_logged_in() {
        let provider = "test-cli-auth-status-missing";
        let loaded = auth::load_tokens(provider).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn logout_deletes_existing_token() {
        let provider = "test-cli-auth-logout";
        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: u64::MAX,
        };
        auth::save_tokens(provider, &tokens).unwrap();
        assert!(auth::load_tokens(provider).unwrap().is_some());

        auth::delete_tokens(provider).unwrap();
        assert!(auth::load_tokens(provider).unwrap().is_none());
    }

    /// Helper: write a temp config with OAuth fields and return the path.
    fn write_temp_oauth_config(provider_name: &str, env_var: &str) -> String {
        let dir = std::env::temp_dir().join(format!("skilldo-test-{}", provider_name));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("skilldo.toml");
        std::fs::write(
            &path,
            format!(
                r#"
[llm]
provider = "openai"
model = "gpt-4o"
oauth_auth_url = "https://auth.example.com/authorize"
oauth_token_url = "https://auth.example.com/token"
oauth_client_id_env = "{env_var}"
provider_name = "{provider_name}"
"#
            ),
        )
        .unwrap();
        path.to_str().unwrap().to_string()
    }

    #[test]
    fn status_with_valid_token() {
        let provider = "test-cli-status-valid-tok";
        let env_var = "SKILLDO_TEST_CLI_STATUS_VALID";
        std::env::set_var(env_var, "client-id");

        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: u64::MAX,
        };
        auth::save_tokens(provider, &tokens).unwrap();

        let config_path = write_temp_oauth_config(provider, env_var);
        let result = status(Some(config_path));
        assert!(result.is_ok());

        auth::delete_tokens(provider).unwrap();
        std::env::remove_var(env_var);
    }

    #[test]
    fn status_with_expired_token() {
        let provider = "test-cli-status-expired-tok";
        let env_var = "SKILLDO_TEST_CLI_STATUS_EXP";
        std::env::set_var(env_var, "client-id");

        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: 0,
        };
        auth::save_tokens(provider, &tokens).unwrap();

        let config_path = write_temp_oauth_config(provider, env_var);
        let result = status(Some(config_path));
        assert!(result.is_ok());

        auth::delete_tokens(provider).unwrap();
        std::env::remove_var(env_var);
    }

    #[test]
    fn status_with_no_token() {
        let provider = "test-cli-status-no-tok";
        let env_var = "SKILLDO_TEST_CLI_STATUS_NONE";
        std::env::set_var(env_var, "client-id");

        let config_path = write_temp_oauth_config(provider, env_var);
        let result = status(Some(config_path));
        assert!(result.is_ok());

        std::env::remove_var(env_var);
    }

    #[test]
    fn logout_with_config_deletes_tokens() {
        let provider = "test-cli-logout-cfg";
        let env_var = "SKILLDO_TEST_CLI_LOGOUT";
        std::env::set_var(env_var, "client-id");

        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "".to_string(),
            expires_at: u64::MAX,
        };
        auth::save_tokens(provider, &tokens).unwrap();

        let config_path = write_temp_oauth_config(provider, env_var);
        let result = logout(Some(config_path));
        assert!(result.is_ok());

        assert!(auth::load_tokens(provider).unwrap().is_none());
        std::env::remove_var(env_var);
    }
}
