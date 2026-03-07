//! Generic OAuth 2.0 + PKCE flow — fully provider-agnostic.
//!
//! Handles authorization URL construction, local callback server,
//! code exchange, and token refresh. Zero provider-specific code.

use anyhow::{bail, Context, Result};
use tracing::{debug, info};

use super::{OAuthEndpoint, TokenSet};

const DEFAULT_REDIRECT_PORT: u16 = 8085;
const DEFAULT_REDIRECT_URI: &str = "http://localhost:8085/callback";
// OpenAI's registered client IDs use port 1455 + /auth/callback
const OPENAI_REDIRECT_PORT: u16 = 1455;
const OPENAI_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const CALLBACK_TIMEOUT_SECS: u64 = 120;
const OAUTH_HTTP_TIMEOUT_SECS: u64 = 30;

/// Escape HTML special characters to prevent injection in error pages.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build a reqwest client with a timeout for OAuth token operations.
fn oauth_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(OAUTH_HTTP_TIMEOUT_SECS))
        .build()
        .context("Failed to build OAuth HTTP client")
}

/// Get the redirect URI and port for the given endpoint.
/// Uses host-level matching (not substring) to identify OpenAI's auth server.
fn redirect_config(endpoint: &OAuthEndpoint) -> (u16, &'static str) {
    if url_has_host(&endpoint.auth_url, "auth.openai.com") {
        (OPENAI_REDIRECT_PORT, OPENAI_REDIRECT_URI)
    } else {
        (DEFAULT_REDIRECT_PORT, DEFAULT_REDIRECT_URI)
    }
}

/// Check if a URL's host matches exactly (not a substring match).
/// Parses the host from the URL without adding a `url` crate dependency.
fn url_has_host(url: &str, expected_host: &str) -> bool {
    // Format: scheme://host[:port]/path
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .and_then(|host_port| host_port.split(':').next())
        .map(|host| host == expected_host)
        .unwrap_or(false)
}

/// Build the authorization URL for the browser.
pub fn build_auth_url(endpoint: &OAuthEndpoint, challenge: &str, state: &str) -> String {
    let (_, redirect_uri) = redirect_config(endpoint);
    let separator = if endpoint.auth_url.contains('?') {
        "&"
    } else {
        "?"
    };
    let mut url = format!(
        "{}{}response_type=code&client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256",
        endpoint.auth_url,
        separator,
        urlencoding::encode(&endpoint.client_id),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(state),
        urlencoding::encode(challenge),
    );
    if !endpoint.scopes.is_empty() {
        url.push_str(&format!("&scope={}", urlencoding::encode(&endpoint.scopes)));
    }

    // Google requires access_type=offline for refresh tokens.
    // Per RFC 6749, authorization servers MUST ignore unrecognized params,
    // so this is safe to send to non-Google providers.
    if url_has_host(&endpoint.auth_url, "accounts.google.com") {
        url.push_str("&access_type=offline");
    }
    url
}

/// Start a local HTTP server, wait for the OAuth callback, and return the authorization code.
///
/// Listens on the appropriate port for the provider, validates the state parameter,
/// serves a success HTML page, and returns the `code` query parameter.
pub async fn start_callback_server(
    expected_state: &str,
    endpoint: &OAuthEndpoint,
) -> Result<String> {
    let (port, _) = redirect_config(endpoint);
    start_callback_server_on_port(expected_state, port).await
}

/// Internal: callback server on a specified port (used by tests to avoid port collisions).
async fn start_callback_server_on_port(expected_state: &str, port: u16) -> Result<String> {
    // Always bind IPv4 (redirect_uri uses "localhost" which resolves to 127.0.0.1).
    let v4_listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .with_context(|| format!("Failed to bind to 127.0.0.1:{port} for OAuth callback"))?;

    // Also try IPv6 — on some systems localhost resolves to ::1.
    // If it fails (IPv6 not enabled, port conflict), just ignore it.
    let v6_listener = tokio::net::TcpListener::bind(format!("[::1]:{port}"))
        .await
        .ok();
    if v6_listener.is_some() {
        debug!("OAuth callback server listening on port {port} (IPv4 + IPv6)");
    } else {
        debug!("OAuth callback server listening on port {port} (IPv4 only)");
    }

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let deadline =
        tokio::time::Instant::now() + tokio::time::Duration::from_secs(CALLBACK_TIMEOUT_SECS);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            bail!("OAuth callback timed out after {CALLBACK_TIMEOUT_SECS}s — did you complete login in the browser?");
        }

        // Accept from whichever listener gets a connection first
        let accept_result = match &v6_listener {
            Some(v6) => {
                tokio::time::timeout(remaining, async {
                    tokio::select! {
                        r = v4_listener.accept() => r,
                        r = v6.accept() => r,
                    }
                })
                .await
            }
            None => tokio::time::timeout(remaining, v4_listener.accept()).await,
        };
        let (mut stream, _) = match accept_result {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => {
                debug!("Failed to accept connection, retrying: {e}");
                continue;
            }
            Err(_) => bail!("OAuth callback timed out after {CALLBACK_TIMEOUT_SECS}s — did you complete login in the browser?"),
        };

        // Read the HTTP request
        let mut buf = vec![0u8; 4096];
        let n = match stream.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                debug!("Failed to read from connection, retrying: {e}");
                continue;
            }
        };
        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse the request line: GET /callback?code=...&state=... HTTP/1.1
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");

        let query = path.split('?').nth(1).unwrap_or("");
        let params: std::collections::HashMap<String, String> = query
            .split('&')
            .filter_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                Some((key.to_string(), urlencoding::decode(value)))
            })
            .collect();

        // Validate state first — ignore connections with wrong/missing state
        let state = params.get("state").map(|s| s.as_str()).unwrap_or("");
        if state != expected_state {
            debug!("Ignoring connection with wrong state, waiting for correct callback");
            let body = "<html><body><h1>Authentication Failed</h1><p>State mismatch — please try again.</p><p>You can close this tab.</p></body></html>";
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            continue;
        }

        // Check for error response (only after state is validated)
        if let Some(error) = params.get("error") {
            let description = params
                .get("error_description")
                .map(|s| s.as_str())
                .unwrap_or("");
            let error_escaped = html_escape(error);
            let desc_escaped = html_escape(description);
            let body = format!("<html><body><h1>Authentication Failed</h1><p>{error_escaped}: {desc_escaped}</p><p>You can close this tab.</p></body></html>");
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            bail!("OAuth error: {error} — {description}");
        }

        // Extract code
        let code = match params.get("code") {
            Some(c) => c.clone(),
            None => {
                let body = "<html><body><h1>Authentication Failed</h1><p>No authorization code received. Please try again.</p><p>You can close this tab.</p></body></html>";
                let response = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                bail!("No authorization code in callback");
            }
        };

        // Send success response
        let body = "<html><body><h1>Authentication Successful</h1><p>You can close this tab and return to the terminal.</p></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        let _ = stream.write_all(response.as_bytes()).await;

        return Ok(code);
    }
}

/// Exchange an authorization code for tokens.
pub async fn exchange_code(
    endpoint: &OAuthEndpoint,
    code: &str,
    verifier: &str,
) -> Result<TokenSet> {
    let client = oauth_http_client()?;

    let (_, redirect_uri) = redirect_config(endpoint);
    let mut params = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", &endpoint.client_id),
        ("code_verifier", verifier),
    ];

    let secret_ref;
    if let Some(secret) = &endpoint.client_secret {
        secret_ref = secret.clone();
        params.push(("client_secret", &secret_ref));
    }

    debug!("Exchanging authorization code at {}", endpoint.token_url);

    let response = client
        .post(&endpoint.token_url)
        .form(&params)
        .send()
        .await
        .context("Failed to exchange authorization code")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("Token exchange failed ({}): {}", status, body);
    }

    parse_token_response(response).await
}

/// Refresh an expired access token using a refresh token.
pub async fn refresh_tokens(endpoint: &OAuthEndpoint, refresh_token: &str) -> Result<TokenSet> {
    let client = oauth_http_client()?;

    let mut params = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", &endpoint.client_id),
    ];

    let secret_ref;
    if let Some(secret) = &endpoint.client_secret {
        secret_ref = secret.clone();
        params.push(("client_secret", &secret_ref));
    }

    debug!("Refreshing token at {}", endpoint.token_url);

    let response = client
        .post(&endpoint.token_url)
        .form(&params)
        .send()
        .await
        .context("Failed to refresh token")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!(
            "Token refresh failed ({}): {} — you may need to run `skilldo auth login` again",
            status,
            body
        );
    }

    let mut tokens = parse_token_response(response).await?;

    // Some providers don't return a new refresh token — keep the old one
    if tokens.refresh_token.is_empty() {
        tokens.refresh_token = refresh_token.to_string();
    }

    Ok(tokens)
}

/// Open the authorization URL in the user's browser and print it to stdout.
pub fn open_auth_url(url: &str) {
    info!("Opening browser for authentication...");
    info!("If the browser doesn't open, visit this URL:\n  {url}");
    if let Err(e) = open::that(url) {
        info!("Could not open browser automatically: {e}");
        info!("Please open this URL manually:\n  {url}");
    }
}

/// Parse the token response JSON into a TokenSet.
pub(super) async fn parse_token_response(response: reqwest::Response) -> Result<TokenSet> {
    let body: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse token response")?;

    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No access_token in token response"))?
        .to_string();

    let refresh_token = body["refresh_token"].as_str().unwrap_or("").to_string();

    // Calculate expiry: use `expires_in` seconds from now.
    // Some providers return expires_in as a string instead of integer (RFC 6749 says integer,
    // but Facebook, some enterprise IdPs deviate). Handle both.
    let expires_in = body["expires_in"]
        .as_u64()
        .or_else(|| {
            body["expires_in"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
        })
        .unwrap_or(3600);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(TokenSet {
        access_token,
        refresh_token,
        expires_at: now + expires_in,
    })
}

// URL encoding helper — minimal, no extra dependency
mod urlencoding {
    pub fn encode(input: &str) -> String {
        let mut result = String::with_capacity(input.len() * 3);
        for byte in input.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                _ => {
                    result.push('%');
                    result.push_str(&format!("{byte:02X}"));
                }
            }
        }
        result
    }

    /// Decode percent-encoded strings (e.g., query parameter values).
    /// Also decodes `+` as space (application/x-www-form-urlencoded).
    pub fn decode(input: &str) -> String {
        let mut result = Vec::with_capacity(input.len());
        let mut bytes = input.bytes();
        while let Some(b) = bytes.next() {
            match b {
                b'%' => {
                    let Some(hi) = bytes.next() else {
                        result.push(b'%');
                        break;
                    };
                    let Some(lo) = bytes.next() else {
                        result.push(b'%');
                        result.push(hi);
                        break;
                    };
                    let hex = [hi, lo];
                    if let Ok(s) = std::str::from_utf8(&hex) {
                        if let Ok(byte) = u8::from_str_radix(s, 16) {
                            result.push(byte);
                            continue;
                        }
                    }
                    // Malformed hex — pass through literally
                    result.push(b'%');
                    result.push(hi);
                    result.push(lo);
                }
                b'+' => result.push(b' '),
                _ => result.push(b),
            }
        }
        String::from_utf8_lossy(&result).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_auth_url_includes_all_params() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: "openid email".to_string(),
            client_id: "my-client".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };

        let url = build_auth_url(&endpoint, "challenge123", "state456");

        assert!(url.starts_with("https://auth.example.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=my-client"));
        assert!(url.contains("code_challenge=challenge123"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=state456"));
        assert!(url.contains("scope=openid%20email"));
        // access_type=offline only added for Google URLs
        assert!(!url.contains("access_type=offline"));
    }

    #[test]
    fn build_auth_url_uses_ampersand_when_query_exists() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com/authorize?prompt=consent".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: "openid".to_string(),
            client_id: "c".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };
        let url = build_auth_url(&endpoint, "ch", "st");
        // Should use & not ? since auth_url already has query params
        assert!(
            url.starts_with("https://auth.example.com/authorize?prompt=consent&response_type=code")
        );
    }

    #[test]
    fn build_auth_url_adds_access_type_for_google() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            scopes: "openid".to_string(),
            client_id: "google-client".to_string(),
            client_secret: None,
            provider_name: "google".to_string(),
        };
        let url = build_auth_url(&endpoint, "c", "s");
        assert!(url.contains("access_type=offline"));
    }

    #[test]
    fn build_auth_url_encodes_special_chars() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: "https://www.googleapis.com/auth/generative-language".to_string(),
            client_id: "client+id".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };

        let url = build_auth_url(&endpoint, "c", "s");
        // + should be percent-encoded
        assert!(url.contains("client_id=client%2Bid"));
    }

    #[test]
    fn urlencoding_encodes_correctly() {
        assert_eq!(urlencoding::encode("hello world"), "hello%20world");
        assert_eq!(urlencoding::encode("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(urlencoding::encode("safe-_.~"), "safe-_.~");
    }

    #[test]
    fn urlencoding_decodes_correctly() {
        assert_eq!(urlencoding::decode("hello%20world"), "hello world");
        assert_eq!(urlencoding::decode("a%2Bb%3Dc"), "a+b=c");
        assert_eq!(urlencoding::decode("safe-_.~"), "safe-_.~");
        assert_eq!(urlencoding::decode("form+encoded"), "form encoded");
        assert_eq!(urlencoding::decode("no%encoding"), "no%encoding"); // malformed passthrough
    }

    #[tokio::test]
    async fn callback_server_extracts_code() {
        let port = 18085u16;
        let state = "test-state-123";

        let server = tokio::spawn(async move { start_callback_server_on_port(state, port).await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let _ = client
            .get(format!(
                "http://127.0.0.1:{port}/callback?code=auth-code-xyz&state={state}"
            ))
            .send()
            .await;

        let code = server.await.unwrap().unwrap();
        assert_eq!(code, "auth-code-xyz");
    }

    #[tokio::test]
    async fn exchange_code_with_mock_server() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"at-123","refresh_token":"rt-456","expires_in":3600}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: "https://unused".to_string(),
            token_url: server.url() + "/token",
            scopes: "openid".to_string(),
            client_id: "cid".to_string(),
            client_secret: Some("csecret".to_string()),
            provider_name: "test".to_string(),
        };

        let tokens = exchange_code(&endpoint, "code123", "verifier456")
            .await
            .unwrap();
        assert_eq!(tokens.access_token, "at-123");
        assert_eq!(tokens.refresh_token, "rt-456");
        assert!(tokens.expires_at > 0);

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn exchange_code_fails_on_error_response() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/token")
            .with_status(400)
            .with_body(r#"{"error":"invalid_grant"}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: "https://unused".to_string(),
            token_url: server.url() + "/token",
            scopes: "openid".to_string(),
            client_id: "cid".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };

        let result = exchange_code(&endpoint, "bad-code", "v").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Token exchange failed"));
    }

    #[tokio::test]
    async fn exchange_code_parses_string_expires_in() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            // Some providers return expires_in as a string instead of integer
            .with_body(r#"{"access_token":"at-str","refresh_token":"rt-str","expires_in":"7200"}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: "https://unused".to_string(),
            token_url: server.url() + "/token",
            scopes: "openid".to_string(),
            client_id: "cid".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };

        let tokens = exchange_code(&endpoint, "code", "verifier").await.unwrap();
        assert_eq!(tokens.access_token, "at-str");
        // Verify expires_at is roughly now + 7200 (not the default 3600)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(tokens.expires_at >= now + 7100);
        assert!(tokens.expires_at <= now + 7300);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn refresh_tokens_with_mock_server() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"new-at","refresh_token":"new-rt","expires_in":7200}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: "https://unused".to_string(),
            token_url: server.url() + "/token",
            scopes: "openid".to_string(),
            client_id: "cid".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };

        let tokens = refresh_tokens(&endpoint, "old-rt").await.unwrap();
        assert_eq!(tokens.access_token, "new-at");
        assert_eq!(tokens.refresh_token, "new-rt");

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn refresh_tokens_keeps_old_refresh_when_missing() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"new-at","expires_in":3600}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: "https://unused".to_string(),
            token_url: server.url() + "/token",
            scopes: "openid".to_string(),
            client_id: "cid".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };

        let tokens = refresh_tokens(&endpoint, "keep-me").await.unwrap();
        assert_eq!(tokens.access_token, "new-at");
        assert_eq!(tokens.refresh_token, "keep-me");
    }

    #[test]
    fn redirect_config_openai_uses_port_1455() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
            token_url: "https://auth.openai.com/oauth/token".to_string(),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };
        let (port, uri) = redirect_config(&endpoint);
        assert_eq!(port, 1455);
        assert!(uri.contains("1455"));
        assert!(uri.contains("/auth/callback"));
    }

    #[test]
    fn redirect_config_generic_uses_port_8085() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };
        let (port, uri) = redirect_config(&endpoint);
        assert_eq!(port, 8085);
        assert!(uri.contains("8085"));
        assert!(uri.contains("/callback"));
    }

    #[test]
    fn build_auth_url_uses_openai_redirect_for_openai() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.openai.com/oauth/authorize".to_string(),
            token_url: "https://auth.openai.com/oauth/token".to_string(),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };
        let url = build_auth_url(&endpoint, "c", "s");
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"));
    }

    #[tokio::test]
    async fn refresh_tokens_fails_on_error_response() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/token")
            .with_status(401)
            .with_body(r#"{"error":"invalid_grant"}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: "https://unused".to_string(),
            token_url: server.url() + "/token",
            scopes: "openid".to_string(),
            client_id: "cid".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };

        let result = refresh_tokens(&endpoint, "bad-rt").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Token refresh failed"));
    }

    #[tokio::test]
    async fn refresh_tokens_with_client_secret() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("client_secret".to_string(), "my-secret".to_string()),
                mockito::Matcher::UrlEncoded("grant_type".to_string(), "refresh_token".to_string()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"new-at","refresh_token":"new-rt","expires_in":3600}"#)
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: "https://unused".to_string(),
            token_url: server.url() + "/token",
            scopes: "openid".to_string(),
            client_id: "cid".to_string(),
            client_secret: Some("my-secret".to_string()),
            provider_name: "test".to_string(),
        };

        let tokens = refresh_tokens(&endpoint, "old-rt").await.unwrap();
        assert_eq!(tokens.access_token, "new-at");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn callback_server_handles_error_response() {
        let port = 18087u16;

        let server =
            tokio::spawn(async move { start_callback_server_on_port("test-state", port).await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let _ = client
            .get(format!(
                "http://127.0.0.1:{port}/callback?error=access_denied&error_description=User+denied&state=test-state"
            ))
            .send()
            .await;

        let result = server.await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("access_denied"));
    }

    #[tokio::test]
    async fn callback_server_ignores_wrong_state_then_accepts_correct() {
        let port = 18088u16;

        let server =
            tokio::spawn(async move { start_callback_server_on_port("correct-state", port).await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        // First request with wrong state — should be ignored
        let _ = client
            .get(format!(
                "http://127.0.0.1:{port}/callback?code=wrong&state=bad-state"
            ))
            .send()
            .await;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Second request with correct state — should succeed
        let _ = client
            .get(format!(
                "http://127.0.0.1:{port}/callback?code=good-code&state=correct-state"
            ))
            .send()
            .await;

        let result = server.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "good-code");
    }

    #[test]
    fn build_auth_url_omits_scope_when_empty() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://example.com/auth".to_string(),
            token_url: "https://example.com/token".to_string(),
            scopes: String::new(),
            client_id: "cid".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };
        let url = build_auth_url(&endpoint, "challenge", "state");
        assert!(!url.contains("scope="));
    }

    #[test]
    fn urlencoding_malformed_trailing_percent() {
        assert_eq!(urlencoding::decode("hello%"), "hello%");
    }

    #[test]
    fn urlencoding_malformed_single_hex_digit() {
        assert_eq!(urlencoding::decode("hello%2"), "hello%2");
    }

    #[test]
    fn urlencoding_invalid_hex_chars() {
        assert_eq!(urlencoding::decode("hello%ZZ"), "hello%ZZ");
    }

    #[test]
    fn url_has_host_exact_match() {
        assert!(url_has_host(
            "https://auth.openai.com/oauth/authorize",
            "auth.openai.com"
        ));
        assert!(url_has_host(
            "https://accounts.google.com/o/oauth2/v2/auth",
            "accounts.google.com"
        ));
    }

    #[test]
    fn url_has_host_rejects_substring() {
        // Must NOT match substrings — prevents spoofing
        assert!(!url_has_host(
            "https://not-auth.openai.com.evil.com/authorize",
            "auth.openai.com"
        ));
        assert!(!url_has_host(
            "https://fake-accounts.google.com/auth",
            "accounts.google.com"
        ));
    }

    #[test]
    fn url_has_host_handles_port() {
        assert!(url_has_host(
            "https://auth.openai.com:443/authorize",
            "auth.openai.com"
        ));
    }

    #[test]
    fn url_has_host_handles_invalid() {
        assert!(!url_has_host("not-a-url", "auth.openai.com"));
        assert!(!url_has_host("", "auth.openai.com"));
    }

    #[tokio::test]
    async fn callback_server_returns_error_when_no_code() {
        let port = 18089u16;

        let server =
            tokio::spawn(async move { start_callback_server_on_port("test-state", port).await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        // Send a request with correct state but no code parameter
        let _ = client
            .get(format!("http://127.0.0.1:{port}/callback?state=test-state"))
            .send()
            .await;

        let result = server.await.unwrap();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No authorization code"));
    }

    #[tokio::test]
    async fn callback_server_public_wrapper_uses_correct_port() {
        // Test start_callback_server (the public wrapper) with a non-OpenAI endpoint
        // to cover the public API path (lines 91-97)
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: "test".to_string(),
        };

        let server =
            tokio::spawn(async move { start_callback_server("my-state", &endpoint).await });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        // Default port is 8085 for non-OpenAI endpoints
        let _ = client
            .get("http://127.0.0.1:8085/callback?code=pub-code&state=my-state")
            .send()
            .await;

        let result = server.await.unwrap().unwrap();
        assert_eq!(result, "pub-code");
    }

    #[test]
    fn html_escape_prevents_injection() {
        assert_eq!(
            html_escape("<script>alert(1)</script>"),
            "&lt;script&gt;alert(1)&lt;/script&gt;"
        );
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape("safe text"), "safe text");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }
}
