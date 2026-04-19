use anyhow::Result;
use std::collections::HashSet;
use tracing::info;

use crate::auth::{self, OAuthEndpoint};
use crate::config::Config;

/// Run the OAuth login flow for all configured OAuth providers.
pub async fn login(config_path: Option<String>) -> Result<()> {
    let config = Config::load_with_path(config_path)?;
    let endpoints = collect_all_endpoints(&config)?;

    if endpoints.is_empty() {
        anyhow::bail!(
            "No OAuth endpoints configured. Add oauth_auth_url, oauth_token_url, and \
             oauth_client_id_env to your skilldo.toml config."
        );
    }

    // Group by OAuth app so we only auth once per unique server+client_id,
    // but save tokens under ALL provider_names that share the app.
    let groups = group_by_oauth_app(&endpoints);

    for (endpoint, provider_names) in &groups {
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

        // Save under all provider_names sharing this OAuth app
        for name in provider_names {
            auth::save_tokens(name, &tokens)?;
        }
        info!(
            "Authenticated with {} successfully.",
            endpoint.provider_name
        );
    }

    Ok(())
}

/// Show OAuth token status for all configured providers.
pub fn status(config_path: Option<String>) -> Result<()> {
    status_to(config_path, &mut std::io::stdout())
}

/// Write OAuth token status to the given writer (testable variant).
pub fn status_to(config_path: Option<String>, out: &mut dyn std::io::Write) -> Result<()> {
    let config = Config::load_with_path(config_path)?;
    let endpoints = collect_all_endpoints(&config)?;

    if endpoints.is_empty() {
        writeln!(out, "No OAuth endpoints configured.")?;
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
                    if tokens.expires_at <= now {
                        let ago = now - tokens.expires_at;
                        writeln!(
                            out,
                            "{}: EXPIRED (expired {}s ago, has refresh token: {})",
                            endpoint.provider_name,
                            ago,
                            !tokens.refresh_token.is_empty()
                        )?;
                    } else {
                        let remaining = tokens.expires_at.saturating_sub(now);
                        writeln!(
                            out,
                            "{}: EXPIRING SOON (expires in {}s, will auto-refresh)",
                            endpoint.provider_name, remaining
                        )?;
                    }
                } else {
                    let remaining = tokens.expires_at.saturating_sub(now);
                    writeln!(
                        out,
                        "{}: VALID (expires in {}s)",
                        endpoint.provider_name, remaining
                    )?;
                }
            }
            None => {
                writeln!(out, "{}: NOT LOGGED IN", endpoint.provider_name)?;
            }
        }
    }

    Ok(())
}

/// Delete all stored OAuth tokens for configured providers.
pub fn logout(config_path: Option<String>) -> Result<()> {
    logout_to(config_path, &mut std::io::stdout())
}

/// Delete tokens with output to the given writer (testable variant).
pub fn logout_to(config_path: Option<String>, out: &mut dyn std::io::Write) -> Result<()> {
    let config = Config::load_with_path(config_path)?;
    let endpoints = collect_all_endpoints(&config)?;

    if endpoints.is_empty() {
        writeln!(out, "No OAuth endpoints configured.")?;
        return Ok(());
    }

    for endpoint in &endpoints {
        auth::delete_tokens(&endpoint.provider_name)?;
        writeln!(out, "Logged out of {}.", endpoint.provider_name)?;
    }

    Ok(())
}

/// Collect all OAuth endpoints from the config, deduplicating by provider_name.
///
/// Multiple stages may share the same OAuth server (same token_url + client_id)
/// but have different provider_names. All are returned so that token storage
/// works correctly for each provider_name.
fn collect_all_endpoints(config: &Config) -> Result<Vec<OAuthEndpoint>> {
    let mut seen_names = HashSet::new();
    let mut endpoints = Vec::new();

    let llm_configs: Vec<&crate::config::LlmConfig> = std::iter::once(&config.llm)
        .chain(config.generation.extract_llm.as_ref())
        .chain(config.generation.map_llm.as_ref())
        .chain(config.generation.learn_llm.as_ref())
        .chain(config.generation.fact_llm.as_ref())
        .chain(config.generation.create_llm.as_ref())
        .chain(config.generation.review_llm.as_ref())
        .chain(config.generation.test_llm.as_ref())
        .collect();

    for llm_config in llm_configs {
        if let Some(endpoint) = llm_config.resolve_oauth_endpoint()? {
            // Dedup by provider_name (same name = same token file)
            if seen_names.insert(endpoint.provider_name.clone()) {
                endpoints.push(endpoint);
            }
        }
    }

    Ok(endpoints)
}

/// Group endpoints by OAuth app (token_url + client_id) for login dedup.
/// Scopes are unioned across all endpoints sharing the same app so a single
/// login covers all stages.
/// Returns: Vec<(merged_endpoint, all_provider_names_sharing_this_app)>
fn group_by_oauth_app(endpoints: &[OAuthEndpoint]) -> Vec<(OAuthEndpoint, Vec<&str>)> {
    let mut groups: std::collections::HashMap<(&str, &str), (OAuthEndpoint, Vec<&str>)> =
        std::collections::HashMap::new();
    for ep in endpoints {
        groups
            .entry((&ep.token_url, &ep.client_id))
            .and_modify(|(merged, names)| {
                names.push(&ep.provider_name);
                // Union scopes: merge space-delimited scope strings
                if !ep.scopes.is_empty() {
                    let existing: HashSet<&str> = merged.scopes.split_whitespace().collect();
                    let new_scopes: Vec<&str> = ep
                        .scopes
                        .split_whitespace()
                        .filter(|s| !existing.contains(s))
                        .collect();
                    for scope in new_scopes {
                        if !merged.scopes.is_empty() {
                            merged.scopes.push(' ');
                        }
                        merged.scopes.push_str(scope);
                    }
                }
            })
            .or_insert((ep.clone(), vec![&ep.provider_name]));
    }
    let mut result: Vec<_> = groups.into_values().collect();
    result.sort_by(|a, b| a.0.provider_name.cmp(&b.0.provider_name));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use serial_test::serial;

    #[test]
    fn collect_all_endpoints_empty_by_default() {
        let config = Config::default();
        let endpoints = collect_all_endpoints(&config).unwrap();
        assert!(endpoints.is_empty());
    }

    #[test]
    #[serial]
    fn collect_all_endpoints_deduplicates_same_provider_name() {
        let mut config = Config::default();
        // Set OAuth on both global and extract stage with same endpoint AND same provider_name
        config.llm.oauth_auth_url = Some("https://auth.example.com/authorize".to_string());
        config.llm.oauth_token_url = Some("https://auth.example.com/token".to_string());
        config.llm.oauth_client_id_env = Some("SKILLDO_TEST_AUTH_CID".to_string());
        config.llm.provider_name = Some("shared-provider".to_string());

        std::env::set_var("SKILLDO_TEST_AUTH_CID", "same-client");

        let extract_llm = config.llm.clone();
        // Same provider_name → deduped to 1
        config.generation.extract_llm = Some(extract_llm);

        let endpoints = collect_all_endpoints(&config).unwrap();
        assert_eq!(endpoints.len(), 1);

        std::env::remove_var("SKILLDO_TEST_AUTH_CID");
    }

    #[test]
    #[serial]
    fn collect_all_endpoints_keeps_different_provider_names() {
        let mut config = Config::default();
        config.llm.oauth_auth_url = Some("https://auth.example.com/authorize".to_string());
        config.llm.oauth_token_url = Some("https://auth.example.com/token".to_string());
        config.llm.oauth_client_id_env = Some("SKILLDO_TEST_AUTH_CID_MULTI".to_string());
        config.llm.provider_name = Some("global-provider".to_string());

        std::env::set_var("SKILLDO_TEST_AUTH_CID_MULTI", "same-client");

        let mut extract_llm = config.llm.clone();
        extract_llm.provider_name = Some("extract-provider".to_string());
        config.generation.extract_llm = Some(extract_llm);

        let endpoints = collect_all_endpoints(&config).unwrap();
        // Same OAuth app but different provider_names → both kept
        assert_eq!(endpoints.len(), 2);

        std::env::remove_var("SKILLDO_TEST_AUTH_CID_MULTI");
    }

    #[test]
    fn group_by_oauth_app_groups_shared_server() {
        let endpoints = vec![
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "cid".to_string(),
                client_secret: None,
                provider_name: "provider-a".to_string(),
            },
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "cid".to_string(),
                client_secret: None,
                provider_name: "provider-b".to_string(),
            },
        ];
        let groups = group_by_oauth_app(&endpoints);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 2);
        assert!(groups[0].1.contains(&"provider-a"));
        assert!(groups[0].1.contains(&"provider-b"));
    }

    #[test]
    #[serial]
    fn collect_all_endpoints_different_providers() {
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

        let endpoints = collect_all_endpoints(&config).unwrap();
        assert_eq!(endpoints.len(), 2);

        std::env::remove_var("SKILLDO_TEST_AUTH_CID1");
        std::env::remove_var("SKILLDO_TEST_AUTH_CID2");
    }

    /// Create a minimal temp config file with no OAuth endpoints for testing.
    /// Returns NamedTempFile so the file is auto-deleted when dropped.
    fn empty_config_file() -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        write!(
            f,
            "[llm]\nprovider_type = \"anthropic\"\nmodel = \"test\"\n"
        )
        .unwrap();
        f
    }

    #[test]
    fn status_no_endpoints_prints_message() {
        let f = empty_config_file();
        let result = status(Some(f.path().to_string_lossy().into_owned()));
        assert!(result.is_ok());
    }

    #[test]
    fn logout_no_endpoints_prints_message() {
        let f = empty_config_file();
        let result = logout(Some(f.path().to_string_lossy().into_owned()));
        assert!(result.is_ok());
    }

    #[test]
    fn login_no_endpoints_errors() {
        let f = empty_config_file();
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(login(Some(f.path().to_string_lossy().into_owned())));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No OAuth endpoints configured"));
    }

    #[test]
    #[serial]
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
        let endpoints = collect_all_endpoints(&config).unwrap();
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
    #[serial]
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
    #[serial]
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
    #[serial]
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
    #[serial]
    fn status_with_expiring_soon_token() {
        let provider = "test-cli-status-expiring-soon";
        let env_var = "SKILLDO_TEST_CLI_STATUS_SOON";
        std::env::set_var(env_var, "client-id");

        // Set expires_at to 30s from now — within the 60s safety buffer
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: now + 30,
        };
        assert!(tokens.is_expired()); // within safety buffer
        auth::save_tokens(provider, &tokens).unwrap();

        let config_path = write_temp_oauth_config(provider, env_var);
        let result = status(Some(config_path));
        assert!(result.is_ok());

        auth::delete_tokens(provider).unwrap();
        std::env::remove_var(env_var);
    }

    #[test]
    #[serial]
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

    #[test]
    #[serial]
    fn collect_all_endpoints_from_map_llm() {
        let mut config = Config::default();
        config.llm.oauth_auth_url = Some("https://auth.example.com/authorize".to_string());
        config.llm.oauth_token_url = Some("https://auth.example.com/token".to_string());
        config.llm.oauth_client_id_env = Some("SKILLDO_TEST_MAP_CID".to_string());
        config.llm.provider_name = Some("global-map".to_string());

        std::env::set_var("SKILLDO_TEST_MAP_CID", "map-client");

        let mut map_llm = config.llm.clone();
        map_llm.provider_name = Some("map-provider".to_string());
        config.generation.map_llm = Some(map_llm);

        let endpoints = collect_all_endpoints(&config).unwrap();
        assert_eq!(endpoints.len(), 2);
        let names: Vec<_> = endpoints.iter().map(|e| e.provider_name.as_str()).collect();
        assert!(names.contains(&"global-map"));
        assert!(names.contains(&"map-provider"));

        std::env::remove_var("SKILLDO_TEST_MAP_CID");
    }

    #[test]
    #[serial]
    fn collect_all_endpoints_from_learn_and_create_llm() {
        let mut config = Config::default();
        config.llm.oauth_auth_url = Some("https://auth.example.com/authorize".to_string());
        config.llm.oauth_token_url = Some("https://auth.example.com/token".to_string());
        config.llm.oauth_client_id_env = Some("SKILLDO_TEST_LC_CID".to_string());
        config.llm.provider_name = Some("global-lc".to_string());

        std::env::set_var("SKILLDO_TEST_LC_CID", "lc-client");

        let mut learn_llm = config.llm.clone();
        learn_llm.provider_name = Some("learn-provider".to_string());
        config.generation.learn_llm = Some(learn_llm);

        let mut create_llm = config.llm.clone();
        create_llm.provider_name = Some("create-provider".to_string());
        config.generation.create_llm = Some(create_llm);

        let endpoints = collect_all_endpoints(&config).unwrap();
        assert_eq!(endpoints.len(), 3);
        let names: Vec<_> = endpoints.iter().map(|e| e.provider_name.as_str()).collect();
        assert!(names.contains(&"global-lc"));
        assert!(names.contains(&"learn-provider"));
        assert!(names.contains(&"create-provider"));

        std::env::remove_var("SKILLDO_TEST_LC_CID");
    }

    #[test]
    #[serial]
    fn collect_all_endpoints_from_review_llm() {
        let mut config = Config::default();
        config.llm.oauth_auth_url = Some("https://auth.example.com/authorize".to_string());
        config.llm.oauth_token_url = Some("https://auth.example.com/token".to_string());
        config.llm.oauth_client_id_env = Some("SKILLDO_TEST_REV_CID".to_string());
        config.llm.provider_name = Some("global-rev".to_string());

        std::env::set_var("SKILLDO_TEST_REV_CID", "rev-client");

        let mut review_llm = config.llm.clone();
        review_llm.provider_name = Some("review-provider".to_string());
        config.generation.review_llm = Some(review_llm);

        let endpoints = collect_all_endpoints(&config).unwrap();
        assert_eq!(endpoints.len(), 2);
        let names: Vec<_> = endpoints.iter().map(|e| e.provider_name.as_str()).collect();
        assert!(names.contains(&"global-rev"));
        assert!(names.contains(&"review-provider"));

        std::env::remove_var("SKILLDO_TEST_REV_CID");
    }

    #[test]
    fn group_by_oauth_app_separates_distinct_apps() {
        let endpoints = vec![
            OAuthEndpoint {
                auth_url: "https://auth1.example.com/authorize".to_string(),
                token_url: "https://auth1.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "cid1".to_string(),
                client_secret: None,
                provider_name: "provider-x".to_string(),
            },
            OAuthEndpoint {
                auth_url: "https://auth2.example.com/authorize".to_string(),
                token_url: "https://auth2.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "cid2".to_string(),
                client_secret: None,
                provider_name: "provider-y".to_string(),
            },
        ];
        let groups = group_by_oauth_app(&endpoints);
        assert_eq!(groups.len(), 2);
        // Each group has exactly 1 provider name
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(groups[1].1.len(), 1);
    }

    #[test]
    fn group_by_oauth_app_groups_same_token_url_and_client_id() {
        let endpoints = vec![
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "alpha".to_string(),
            },
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "shared-cid".to_string(),
                client_secret: Some("secret".to_string()),
                provider_name: "beta".to_string(),
            },
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "gamma".to_string(),
            },
        ];
        let groups = group_by_oauth_app(&endpoints);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 3);
    }

    #[test]
    fn group_by_oauth_app_unions_scopes() {
        let endpoints = vec![
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid profile".to_string(),
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "stage-a".to_string(),
            },
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid email offline_access".to_string(),
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "stage-b".to_string(),
            },
        ];
        let groups = group_by_oauth_app(&endpoints);
        assert_eq!(groups.len(), 1);
        let merged_scopes: HashSet<&str> = groups[0].0.scopes.split_whitespace().collect();
        assert!(merged_scopes.contains("openid"));
        assert!(merged_scopes.contains("profile"));
        assert!(merged_scopes.contains("email"));
        assert!(merged_scopes.contains("offline_access"));
        assert_eq!(merged_scopes.len(), 4, "should have 4 unique scopes");
    }

    #[test]
    fn group_by_oauth_app_empty_input() {
        let endpoints: Vec<OAuthEndpoint> = vec![];
        let groups = group_by_oauth_app(&endpoints);
        assert!(groups.is_empty());
    }

    #[test]
    fn group_by_oauth_app_results_sorted_by_provider_name() {
        let endpoints = vec![
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "cid-z".to_string(),
                client_secret: None,
                provider_name: "z-provider".to_string(),
            },
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "cid-a".to_string(),
                client_secret: None,
                provider_name: "a-provider".to_string(),
            },
        ];
        let groups = group_by_oauth_app(&endpoints);
        assert_eq!(groups.len(), 2);
        // Results should be sorted by provider_name
        assert_eq!(groups[0].0.provider_name, "a-provider");
        assert_eq!(groups[1].0.provider_name, "z-provider");
    }

    #[test]
    #[serial]
    fn status_with_multiple_providers_shows_all() {
        let providers = [
            ("test-multi-status-valid", u64::MAX, "valid-at"),
            ("test-multi-status-expired", 0, "expired-at"),
        ];
        let env_var = "SKILLDO_TEST_MULTI_STATUS";
        std::env::set_var(env_var, "client-id");

        for (name, expires_at, access_token) in &providers {
            let tokens = auth::TokenSet {
                access_token: access_token.to_string(),
                refresh_token: "rt".to_string(),
                expires_at: *expires_at,
            };
            auth::save_tokens(name, &tokens).unwrap();
        }

        // Create config with two different provider names
        let dir = std::env::temp_dir().join("skilldo-test-multi-status");
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
provider_name = "test-multi-status-valid"

[generation.extract_llm]
provider = "openai"
model = "gpt-4o"
oauth_auth_url = "https://auth2.example.com/authorize"
oauth_token_url = "https://auth2.example.com/token"
oauth_client_id_env = "{env_var}"
provider_name = "test-multi-status-expired"
"#
            ),
        )
        .unwrap();

        let result = status(Some(path.to_str().unwrap().to_string()));
        assert!(result.is_ok());

        for (name, _, _) in &providers {
            auth::delete_tokens(name).unwrap();
        }
        std::env::remove_var(env_var);
    }

    #[test]
    fn status_with_invalid_config_path_errors() {
        let result = status(Some(
            "/tmp/nonexistent-skilldo-config-12345.toml".to_string(),
        ));
        assert!(result.is_err());
    }

    #[test]
    fn logout_with_invalid_config_path_errors() {
        let result = logout(Some(
            "/tmp/nonexistent-skilldo-config-12345.toml".to_string(),
        ));
        assert!(result.is_err());
    }

    #[test]
    fn login_with_invalid_config_path_errors() {
        let result = tokio::runtime::Runtime::new().unwrap().block_on(login(Some(
            "/tmp/nonexistent-skilldo-config-12345.toml".to_string(),
        )));
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn collect_all_endpoints_skips_non_oauth_stages() {
        let mut config = Config::default();
        // Global LLM has no OAuth
        config.llm.provider_name = Some("no-oauth-global".to_string());

        // extract_llm has OAuth
        let mut extract_llm = config.llm.clone();
        extract_llm.oauth_auth_url = Some("https://auth.example.com/authorize".to_string());
        extract_llm.oauth_token_url = Some("https://auth.example.com/token".to_string());
        extract_llm.oauth_client_id_env = Some("SKILLDO_TEST_SKIP_CID".to_string());
        extract_llm.provider_name = Some("oauth-extract".to_string());
        config.generation.extract_llm = Some(extract_llm);

        // map_llm has no OAuth
        let mut map_llm = config.llm.clone();
        map_llm.provider_name = Some("no-oauth-map".to_string());
        config.generation.map_llm = Some(map_llm);

        std::env::set_var("SKILLDO_TEST_SKIP_CID", "skip-client");
        let endpoints = collect_all_endpoints(&config).unwrap();
        // Only extract_llm has OAuth, so only 1 endpoint
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].provider_name, "oauth-extract");
        std::env::remove_var("SKILLDO_TEST_SKIP_CID");
    }

    #[test]
    #[serial]
    fn logout_with_multiple_providers_deletes_all() {
        let providers = ["test-multi-logout-a", "test-multi-logout-b"];
        let env_var = "SKILLDO_TEST_MULTI_LOGOUT";
        std::env::set_var(env_var, "client-id");

        for name in &providers {
            let tokens = auth::TokenSet {
                access_token: "at".to_string(),
                refresh_token: "rt".to_string(),
                expires_at: u64::MAX,
            };
            auth::save_tokens(name, &tokens).unwrap();
        }

        let dir = std::env::temp_dir().join("skilldo-test-multi-logout");
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
provider_name = "test-multi-logout-a"

[generation.extract_llm]
provider = "openai"
model = "gpt-4o"
oauth_auth_url = "https://auth2.example.com/authorize"
oauth_token_url = "https://auth2.example.com/token"
oauth_client_id_env = "{env_var}"
provider_name = "test-multi-logout-b"
"#
            ),
        )
        .unwrap();

        let result = logout(Some(path.to_str().unwrap().to_string()));
        assert!(result.is_ok());

        for name in &providers {
            assert!(
                auth::load_tokens(name).unwrap().is_none(),
                "token for {} should have been deleted",
                name
            );
        }
        std::env::remove_var(env_var);
    }

    #[test]
    fn status_to_captures_no_endpoints_message() {
        let mut buf = Vec::new();
        // Empty config → no OAuth endpoints
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            "[llm]\nprovider_type = \"anthropic\"\nmodel = \"test\"\n",
        )
        .unwrap();
        let result = status_to(Some(tmp.path().to_string_lossy().to_string()), &mut buf);
        assert!(result.is_ok());
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("No OAuth endpoints configured"),
            "output: {output}"
        );
    }

    #[test]
    fn logout_to_captures_no_endpoints_message() {
        let mut buf = Vec::new();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            "[llm]\nprovider_type = \"anthropic\"\nmodel = \"test\"\n",
        )
        .unwrap();
        let result = logout_to(Some(tmp.path().to_string_lossy().to_string()), &mut buf);
        assert!(result.is_ok());
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("No OAuth endpoints configured"),
            "output: {output}"
        );
    }

    #[test]
    #[serial]
    fn status_to_valid_token_output() {
        let provider = "test-cli-status-to-valid";
        let env_var = "SKILLDO_TEST_STATUS_TO_VALID";
        std::env::set_var(env_var, "client-id");

        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: u64::MAX,
        };
        auth::save_tokens(provider, &tokens).unwrap();

        let config_path = write_temp_oauth_config(provider, env_var);
        let mut buf = Vec::new();
        let result = status_to(Some(config_path), &mut buf);
        assert!(result.is_ok());
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("VALID"), "expected VALID in: {output}");
        assert!(
            output.contains(provider),
            "expected provider name in: {output}"
        );
        assert!(
            output.contains("expires in"),
            "expected 'expires in' in: {output}"
        );

        auth::delete_tokens(provider).unwrap();
        std::env::remove_var(env_var);
    }

    #[test]
    #[serial]
    fn status_to_expired_token_output() {
        let provider = "test-cli-status-to-expired";
        let env_var = "SKILLDO_TEST_STATUS_TO_EXP";
        std::env::set_var(env_var, "client-id");

        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: 0,
        };
        auth::save_tokens(provider, &tokens).unwrap();

        let config_path = write_temp_oauth_config(provider, env_var);
        let mut buf = Vec::new();
        let result = status_to(Some(config_path), &mut buf);
        assert!(result.is_ok());
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("EXPIRED"), "expected EXPIRED in: {output}");
        assert!(
            output.contains("has refresh token: true"),
            "expected 'has refresh token: true' in: {output}"
        );

        auth::delete_tokens(provider).unwrap();
        std::env::remove_var(env_var);
    }

    #[test]
    #[serial]
    fn status_to_expired_no_refresh_token_output() {
        let provider = "test-cli-status-to-exp-norefresh";
        let env_var = "SKILLDO_TEST_STATUS_TO_EXPNR";
        std::env::set_var(env_var, "client-id");

        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "".to_string(),
            expires_at: 0,
        };
        auth::save_tokens(provider, &tokens).unwrap();

        let config_path = write_temp_oauth_config(provider, env_var);
        let mut buf = Vec::new();
        let result = status_to(Some(config_path), &mut buf);
        assert!(result.is_ok());
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("EXPIRED"), "expected EXPIRED in: {output}");
        assert!(
            output.contains("has refresh token: false"),
            "expected 'has refresh token: false' in: {output}"
        );

        auth::delete_tokens(provider).unwrap();
        std::env::remove_var(env_var);
    }

    #[test]
    #[serial]
    fn status_to_expiring_soon_output() {
        let provider = "test-cli-status-to-soon";
        let env_var = "SKILLDO_TEST_STATUS_TO_SOON";
        std::env::set_var(env_var, "client-id");

        // 30s from now — within the 60s safety buffer so is_expired() == true
        // but expires_at > now so it hits the EXPIRING SOON branch
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: now + 30,
        };
        assert!(tokens.is_expired());
        auth::save_tokens(provider, &tokens).unwrap();

        let config_path = write_temp_oauth_config(provider, env_var);
        let mut buf = Vec::new();
        let result = status_to(Some(config_path), &mut buf);
        assert!(result.is_ok());
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("EXPIRING SOON"),
            "expected EXPIRING SOON in: {output}"
        );
        assert!(
            output.contains("will auto-refresh"),
            "expected 'will auto-refresh' in: {output}"
        );

        auth::delete_tokens(provider).unwrap();
        std::env::remove_var(env_var);
    }

    #[test]
    #[serial]
    fn status_to_not_logged_in_output() {
        let provider = "test-cli-status-to-nologin";
        let env_var = "SKILLDO_TEST_STATUS_TO_NOLOG";
        std::env::set_var(env_var, "client-id");

        // Ensure no token file exists
        let _ = auth::delete_tokens(provider);

        let config_path = write_temp_oauth_config(provider, env_var);
        let mut buf = Vec::new();
        let result = status_to(Some(config_path), &mut buf);
        assert!(result.is_ok());
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("NOT LOGGED IN"),
            "expected NOT LOGGED IN in: {output}"
        );
        assert!(
            output.contains(provider),
            "expected provider name in: {output}"
        );

        std::env::remove_var(env_var);
    }

    #[test]
    #[serial]
    fn logout_to_with_endpoints_deletes_tokens() {
        let provider = "test-cli-logout-to-ep";
        let env_var = "SKILLDO_TEST_LOGOUT_TO_EP";
        std::env::set_var(env_var, "client-id");

        let tokens = auth::TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: u64::MAX,
        };
        auth::save_tokens(provider, &tokens).unwrap();
        assert!(auth::load_tokens(provider).unwrap().is_some());

        let config_path = write_temp_oauth_config(provider, env_var);
        let mut buf = Vec::new();
        let result = logout_to(Some(config_path), &mut buf);
        assert!(result.is_ok());

        // Token should be deleted
        assert!(
            auth::load_tokens(provider).unwrap().is_none(),
            "token should have been deleted"
        );
        // logout_to writes "Logged out of <provider>." to the buffer
        let output = String::from_utf8(buf).unwrap();
        assert!(
            output.contains("Logged out of"),
            "expected logout confirmation in output: {output}"
        );
        std::env::remove_var(env_var);
    }

    #[test]
    fn status_to_with_invalid_config_errors() {
        let mut buf = Vec::new();
        let result = status_to(
            Some("/tmp/nonexistent-skilldo-cfg-99999.toml".to_string()),
            &mut buf,
        );
        assert!(result.is_err());
    }

    #[test]
    fn logout_to_with_invalid_config_errors() {
        let mut buf = Vec::new();
        let result = logout_to(
            Some("/tmp/nonexistent-skilldo-cfg-99999.toml".to_string()),
            &mut buf,
        );
        assert!(result.is_err());
    }

    #[test]
    fn group_by_oauth_app_skips_empty_scopes_on_grouped_endpoint() {
        // When a second endpoint in the same group has empty scopes,
        // the scope-union block should be skipped (exercising the `if !ep.scopes.is_empty()` false branch).
        let endpoints = vec![
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid".to_string(),
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "first".to_string(),
            },
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "".to_string(), // empty scopes
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "second".to_string(),
            },
        ];
        let groups = group_by_oauth_app(&endpoints);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 2);
        // Scopes should remain unchanged — only "openid" from the first endpoint
        let merged_scopes: HashSet<&str> = groups[0].0.scopes.split_whitespace().collect();
        assert_eq!(merged_scopes.len(), 1);
        assert!(merged_scopes.contains("openid"));
    }

    #[test]
    fn group_by_oauth_app_both_empty_scopes() {
        // When all endpoints in a group have empty scopes, the merged scopes stay empty.
        let endpoints = vec![
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "".to_string(),
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "ep-a".to_string(),
            },
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "".to_string(),
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "ep-b".to_string(),
            },
        ];
        let groups = group_by_oauth_app(&endpoints);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].0.scopes.is_empty());
    }

    #[test]
    fn group_by_oauth_app_no_duplicate_scopes_on_overlap() {
        // When scopes fully overlap, no duplicates should appear in the merged result.
        let endpoints = vec![
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid profile email".to_string(),
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "ep-x".to_string(),
            },
            OAuthEndpoint {
                auth_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: "openid profile email".to_string(), // exact same scopes
                client_id: "shared-cid".to_string(),
                client_secret: None,
                provider_name: "ep-y".to_string(),
            },
        ];
        let groups = group_by_oauth_app(&endpoints);
        assert_eq!(groups.len(), 1);
        let merged_scopes: HashSet<&str> = groups[0].0.scopes.split_whitespace().collect();
        assert_eq!(merged_scopes.len(), 3);
        // Verify the raw string has no extra spaces or duplicates
        let parts: Vec<&str> = groups[0].0.scopes.split_whitespace().collect();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    #[serial]
    fn status_to_multiple_providers_mixed_states() {
        let env_var = "SKILLDO_TEST_STATUS_TO_MIX";
        std::env::set_var(env_var, "client-id");

        // Provider A: valid token
        let provider_a = "test-status-to-mix-valid";
        auth::save_tokens(
            provider_a,
            &auth::TokenSet {
                access_token: "at".to_string(),
                refresh_token: "rt".to_string(),
                expires_at: u64::MAX,
            },
        )
        .unwrap();

        // Provider B: no token (NOT LOGGED IN)
        let provider_b = "test-status-to-mix-nologin";
        let _ = auth::delete_tokens(provider_b);

        let dir = std::env::temp_dir().join("skilldo-test-status-to-mix");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("skilldo.toml");
        std::fs::write(
            &path,
            format!(
                r#"
[llm]
provider_type = "openai"
model = "gpt-4o"
oauth_auth_url = "https://auth.example.com/authorize"
oauth_token_url = "https://auth.example.com/token"
oauth_client_id_env = "{env_var}"
provider_name = "{provider_a}"

[generation.extract_llm]
provider_type = "openai"
model = "gpt-4o"
oauth_auth_url = "https://auth2.example.com/authorize"
oauth_token_url = "https://auth2.example.com/token"
oauth_client_id_env = "{env_var}"
provider_name = "{provider_b}"
"#
            ),
        )
        .unwrap();

        let mut buf = Vec::new();
        let result = status_to(Some(path.to_str().unwrap().to_string()), &mut buf);
        assert!(result.is_ok());
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("VALID"), "expected VALID in: {output}");
        assert!(
            output.contains("NOT LOGGED IN"),
            "expected NOT LOGGED IN in: {output}"
        );

        auth::delete_tokens(provider_a).unwrap();
        std::env::remove_var(env_var);
    }
}
