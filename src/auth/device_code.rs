//! Device Code OAuth 2.0 flow — used by providers like OpenAI that don't
//! support localhost redirect URIs for CLI tools.
//!
//! Flow: POST /deviceauth/usercode → display code → poll /deviceauth/token →
//! exchange code for tokens via standard token endpoint.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tracing::{debug, info};

use super::OAuthEndpoint;

const POLL_TIMEOUT_SECS: u64 = 900; // 15 minutes
const OAUTH_HTTP_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Deserialize)]
struct UserCodeResponse {
    device_auth_id: String,
    user_code: String,
    #[serde(
        default = "default_interval",
        deserialize_with = "deserialize_string_or_u64"
    )]
    interval: u64,
    url: Option<String>,
}

fn default_interval() -> u64 {
    5
}

/// Deserialize a value that may be either a string or u64.
fn deserialize_string_or_u64<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct StringOrU64;
    impl<'de> de::Visitor<'de> for StringOrU64 {
        type Value = u64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or integer")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<u64, E> {
            Ok(v)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<u64, E> {
            v.parse().map_err(de::Error::custom)
        }
    }
    deserializer.deserialize_any(StringOrU64)
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    authorization_code: Option<String>,
    code_verifier: Option<String>,
}

/// Run the device code authentication flow.
///
/// 1. Request a user code from the provider
/// 2. Display the code and URL to the user
/// 3. Poll until the user completes authentication
/// 4. Exchange the authorization code for tokens
pub async fn device_code_login(endpoint: &OAuthEndpoint) -> Result<super::TokenSet> {
    let client = reqwest::Client::builder()
        .user_agent(format!("skilldo-cli/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(OAUTH_HTTP_TIMEOUT_SECS))
        .build()
        .context("Failed to build HTTP client")?;
    // OpenAI device code endpoints live under {issuer}/api/accounts/deviceauth/
    // The issuer is derived from the auth_url by stripping /oauth/authorize
    let issuer = endpoint
        .auth_url
        .trim_end_matches("/authorize")
        .trim_end_matches("/oauth")
        .trim_end_matches('/');
    let auth_base = format!("{issuer}/api/accounts");

    // Step 1: Request user code
    let usercode_url = format!("{auth_base}/deviceauth/usercode");
    debug!("Requesting device code from {usercode_url}");

    let response = client
        .post(&usercode_url)
        .json(&serde_json::json!({ "client_id": endpoint.client_id }))
        .send()
        .await
        .context("Failed to request device code")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("Device code request failed ({status}): {body}");
    }

    let user_code_resp: UserCodeResponse = response
        .json()
        .await
        .context("Failed to parse device code response")?;

    // Step 2: Display code to user
    // The user-facing verification URL is {issuer}/codex/device (not the API endpoint)
    let verification_url = user_code_resp
        .url
        .clone()
        .unwrap_or_else(|| format!("{issuer}/codex/device"));

    println!();
    info!("Please visit: {verification_url}");
    info!("And enter this code: {}", user_code_resp.user_code);
    println!();

    // Open browser (skip for localhost URLs — used in tests)
    if verification_url.contains("localhost") || verification_url.contains("127.0.0.1") {
        debug!("Skipping browser open for localhost URL");
    } else if let Err(e) = open::that(&verification_url) {
        debug!("Could not open browser automatically: {e}");
    }

    // Step 3: Poll for token
    let token_url = format!("{auth_base}/deviceauth/token");
    let interval = std::time::Duration::from_secs(user_code_resp.interval);
    let deadline =
        tokio::time::Instant::now() + tokio::time::Duration::from_secs(POLL_TIMEOUT_SECS);

    let device_token = loop {
        tokio::time::sleep(interval).await;

        if tokio::time::Instant::now() > deadline {
            bail!("Device code authentication timed out after {POLL_TIMEOUT_SECS}s");
        }

        let poll_resp = client
            .post(&token_url)
            .json(&serde_json::json!({
                "device_auth_id": user_code_resp.device_auth_id,
                "user_code": user_code_resp.user_code,
            }))
            .send()
            .await
            .context("Failed to poll device token")?;

        let status = poll_resp.status();

        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
            // User hasn't completed auth yet — keep polling
            debug!("Still waiting for user to complete authentication...");
            continue;
        }

        if !status.is_success() {
            let body = poll_resp.text().await.unwrap_or_default();
            bail!("Device token poll failed ({status}): {body}");
        }

        break poll_resp
            .json::<DeviceTokenResponse>()
            .await
            .context("Failed to parse device token response")?;
    };

    // Step 4: Exchange code for tokens via standard token endpoint
    let authorization_code = device_token
        .authorization_code
        .context("No authorization_code in device token response")?;

    let code_verifier = device_token.code_verifier.unwrap_or_default();

    let redirect_uri = format!("{auth_base}/deviceauth/callback");

    debug!("Exchanging device code at {}", endpoint.token_url);

    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", &authorization_code),
        ("redirect_uri", &redirect_uri),
        ("client_id", &endpoint.client_id),
    ];

    if !code_verifier.is_empty() {
        params.push(("code_verifier", &code_verifier));
    }

    let secret_ref;
    if let Some(secret) = &endpoint.client_secret {
        secret_ref = secret.clone();
        params.push(("client_secret", &secret_ref));
    }

    let token_response = client
        .post(&endpoint.token_url)
        .form(&params)
        .send()
        .await
        .context("Failed to exchange device code for tokens")?;

    if !token_response.status().is_success() {
        let status = token_response.status();
        let body = token_response.text().await.unwrap_or_default();
        bail!("Device code token exchange failed ({status}): {body}");
    }

    super::oauth::parse_token_response(token_response).await
}

/// Check if an endpoint should use device code flow instead of PKCE redirect.
///
/// Currently disabled — the PKCE flow with the correct redirect URI
/// (port 1455, /auth/callback) works with OpenAI's registered client IDs.
/// Device code flow is available but OpenAI's implementation has restrictions
/// that cause failures for third-party tools.
pub fn should_use_device_code(_endpoint: &OAuthEndpoint) -> bool {
    false // Device code disabled for now — PKCE works with correct redirect URI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_code_currently_disabled() {
        // Device code is disabled in favor of PKCE with correct redirect URI
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
            token_url: "https://auth.openai.com/oauth/token".to_string(),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };
        assert!(!should_use_device_code(&endpoint));
    }

    #[tokio::test]
    async fn device_code_request_fails_gracefully() {
        let mut server = mockito::Server::new_async().await;
        // auth_base = {issuer}/api/accounts, issuer = server.url()
        let _mock = server
            .mock("POST", "/api/accounts/deviceauth/usercode")
            .with_status(400)
            .with_body(r#"{"error":"invalid_client"}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: format!("{}/oauth/authorize", server.url()),
            token_url: format!("{}/oauth/token", server.url()),
            scopes: "openid".to_string(),
            client_id: "bad-client".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };

        let result = device_code_login(&endpoint).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Device code request failed"));
    }

    #[tokio::test]
    async fn device_code_full_flow_with_mock() {
        let mut server = mockito::Server::new_async().await;

        // auth_base = {issuer}/api/accounts, issuer = server.url() (after stripping /oauth/authorize)
        let _usercode_mock = server
            .mock("POST", "/api/accounts/deviceauth/usercode")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"device_auth_id":"daid-123","user_code":"ABCD-1234","interval":1}"#)
            .create_async()
            .await;

        // Mock device token endpoint — return code on first poll
        let _token_poll_mock = server
            .mock("POST", "/api/accounts/deviceauth/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"authorization_code":"authcode-xyz","code_verifier":"verifier-abc"}"#)
            .create_async()
            .await;

        // Mock standard token exchange
        let _exchange_mock = server
            .mock("POST", "/oauth/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"access_token":"at-device","refresh_token":"rt-device","expires_in":7200}"#,
            )
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: format!("{}/oauth/authorize", server.url()),
            token_url: format!("{}/oauth/token", server.url()),
            scopes: "openid".to_string(),
            client_id: "test-client".to_string(),
            client_secret: None,
            provider_name: "test-device".to_string(),
        };

        let tokens = device_code_login(&endpoint).await.unwrap();
        assert_eq!(tokens.access_token, "at-device");
        assert_eq!(tokens.refresh_token, "rt-device");
        assert!(tokens.expires_at > 0);
    }

    #[test]
    fn deserialize_interval_as_string() {
        let json = r#"{"device_auth_id":"id","user_code":"CODE","interval":"10"}"#;
        let resp: UserCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.interval, 10);
    }

    #[test]
    fn deserialize_interval_as_integer() {
        let json = r#"{"device_auth_id":"id","user_code":"CODE","interval":5}"#;
        let resp: UserCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.interval, 5);
    }

    #[test]
    fn deserialize_interval_defaults_when_missing() {
        let json = r#"{"device_auth_id":"id","user_code":"CODE"}"#;
        let resp: UserCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.interval, 5); // default_interval()
    }

    #[test]
    fn deserialize_url_field_optional() {
        let json = r#"{"device_auth_id":"id","user_code":"CODE","interval":1,"url":"https://example.com/verify"}"#;
        let resp: UserCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.url, Some("https://example.com/verify".to_string()));

        let json_no_url = r#"{"device_auth_id":"id","user_code":"CODE","interval":1}"#;
        let resp2: UserCodeResponse = serde_json::from_str(json_no_url).unwrap();
        assert!(resp2.url.is_none());
    }

    #[tokio::test]
    async fn device_code_poll_retries_on_forbidden() {
        let mut server = mockito::Server::new_async().await;

        let _usercode_mock = server
            .mock("POST", "/api/accounts/deviceauth/usercode")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"device_auth_id":"daid","user_code":"CODE","interval":1}"#)
            .create_async()
            .await;

        // First poll returns 403 (user hasn't completed auth), second returns success
        let _poll_forbidden = server
            .mock("POST", "/api/accounts/deviceauth/token")
            .with_status(403)
            .expect(1)
            .create_async()
            .await;

        let _poll_success = server
            .mock("POST", "/api/accounts/deviceauth/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"authorization_code":"auth-code","code_verifier":"cv"}"#)
            .create_async()
            .await;

        let _exchange_mock = server
            .mock("POST", "/oauth/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: format!("{}/oauth/authorize", server.url()),
            token_url: format!("{}/oauth/token", server.url()),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: "test-poll-retry".to_string(),
        };

        let tokens = device_code_login(&endpoint).await.unwrap();
        assert_eq!(tokens.access_token, "at");
    }

    #[tokio::test]
    async fn device_code_exchange_fails_on_error() {
        let mut server = mockito::Server::new_async().await;

        let _usercode_mock = server
            .mock("POST", "/api/accounts/deviceauth/usercode")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"device_auth_id":"daid","user_code":"CODE","interval":1}"#)
            .create_async()
            .await;

        let _poll_mock = server
            .mock("POST", "/api/accounts/deviceauth/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"authorization_code":"auth-code","code_verifier":"cv"}"#)
            .create_async()
            .await;

        // Token exchange fails
        let _exchange_mock = server
            .mock("POST", "/oauth/token")
            .with_status(400)
            .with_body(r#"{"error":"invalid_grant"}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: format!("{}/oauth/authorize", server.url()),
            token_url: format!("{}/oauth/token", server.url()),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: "test-exchange-fail".to_string(),
        };

        let result = device_code_login(&endpoint).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn device_code_with_client_secret() {
        let mut server = mockito::Server::new_async().await;

        let _usercode_mock = server
            .mock("POST", "/api/accounts/deviceauth/usercode")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"device_auth_id":"daid","user_code":"CODE","interval":1}"#)
            .create_async()
            .await;

        let _poll_mock = server
            .mock("POST", "/api/accounts/deviceauth/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"authorization_code":"auth-code","code_verifier":"cv"}"#)
            .create_async()
            .await;

        let _exchange_mock = server
            .mock("POST", "/oauth/token")
            .match_body(mockito::Matcher::UrlEncoded(
                "client_secret".to_string(),
                "secret123".to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: format!("{}/oauth/authorize", server.url()),
            token_url: format!("{}/oauth/token", server.url()),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: Some("secret123".to_string()),
            provider_name: "test-with-secret".to_string(),
        };

        let tokens = device_code_login(&endpoint).await.unwrap();
        assert_eq!(tokens.access_token, "at");
        _exchange_mock.assert_async().await;
    }
}
