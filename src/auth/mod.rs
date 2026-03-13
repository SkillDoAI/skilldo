//! OAuth 2.0 authentication — token storage, PKCE, and generic OAuth flows.
//!
//! Fully provider-agnostic: users configure OAuth endpoints + env var names
//! per stage in `skilldo.toml`. Any provider that speaks OAuth 2.0 + PKCE works.

pub mod device_code;
pub mod oauth;
pub mod pkce;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;

/// Stored OAuth token set, persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp (seconds) when the access token expires.
    pub expires_at: u64,
}

/// Resolved OAuth endpoint configuration — all env vars dereferenced.
#[derive(Debug, Clone)]
pub struct OAuthEndpoint {
    pub auth_url: String,
    pub token_url: String,
    pub scopes: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub provider_name: String,
}

impl TokenSet {
    /// Check whether the access token has expired (with 60s safety buffer).
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now + 60 >= self.expires_at
    }
}

/// Directory for token storage: `~/.config/skilldo/tokens/`
fn tokens_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("Could not determine config directory")?;
    Ok(config_dir.join("skilldo").join("tokens"))
}

/// Sanitize a provider name to a safe filename component.
/// Only allows `[A-Za-z0-9._-]`; rejects empty, `.`, `..`, or names with path separators.
fn sanitize_provider_name(name: &str) -> Result<&str> {
    if name.is_empty() || name == "." || name == ".." {
        anyhow::bail!("Invalid provider name: {:?}", name);
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        anyhow::bail!(
            "Provider name contains invalid characters (only A-Z, a-z, 0-9, '.', '_', '-' allowed): {:?}",
            name
        );
    }
    Ok(name)
}

/// Token file path for a given provider name.
fn token_path(provider_name: &str) -> Result<PathBuf> {
    let safe_name = sanitize_provider_name(provider_name)?;
    Ok(tokens_dir()?.join(format!("{safe_name}.json")))
}

/// Save tokens to disk with secure permissions.
pub fn save_tokens(provider_name: &str, tokens: &TokenSet) -> Result<()> {
    let dir = tokens_dir()?;
    ensure_secure_dir(&dir)?;
    let path = token_path(provider_name)?;
    let json = serde_json::to_string_pretty(tokens)?;
    write_secure_file(&path, &json)?;
    debug!("Saved tokens for {provider_name} to {}", path.display());
    Ok(())
}

/// Load tokens from disk. Returns None if file doesn't exist.
pub fn load_tokens(provider_name: &str) -> Result<Option<TokenSet>> {
    let path = token_path(provider_name)?;
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(anyhow::Error::new(e)
                .context(format!("Failed to read token file: {}", path.display())))
        }
    };
    let tokens: TokenSet = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse token file: {}", path.display()))?;
    Ok(Some(tokens))
}

/// Delete tokens for a provider. Returns Ok even if file doesn't exist.
pub fn delete_tokens(provider_name: &str) -> Result<()> {
    let path = token_path(provider_name)?;
    match std::fs::remove_file(&path) {
        Ok(()) => {
            debug!("Deleted tokens for {provider_name}");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(anyhow::Error::new(e)
                .context(format!("Failed to delete token file: {}", path.display())))
        }
    }
    Ok(())
}

/// Resolve an OAuth token for the given endpoint.
///
/// - Loads stored tokens
/// - If expired, attempts refresh
/// - Returns the access token string, or None if no tokens stored
pub async fn resolve_oauth_token(endpoint: &OAuthEndpoint) -> Result<Option<String>> {
    let tokens = match load_tokens(&endpoint.provider_name)? {
        Some(t) => t,
        None => return Ok(None),
    };

    if !tokens.is_expired() {
        return Ok(Some(tokens.access_token));
    }

    debug!(
        "Token for {} expired, attempting refresh",
        endpoint.provider_name
    );
    let refreshed = oauth::refresh_tokens(endpoint, &tokens.refresh_token).await?;
    save_tokens(&endpoint.provider_name, &refreshed)?;
    Ok(Some(refreshed.access_token))
}

/// Create directory with secure permissions (0o700 on Unix).
/// Uses `DirBuilder::mode()` to set permissions at creation time, avoiding
/// a TOCTOU race where the directory is briefly world-readable.
fn ensure_secure_dir(path: &PathBuf) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        use std::os::unix::fs::PermissionsExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .with_context(|| format!("Failed to create directory: {}", path.display()))?;
        // Enforce 0o700 on pre-existing directories (DirBuilder only sets mode at creation)
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("Failed to set directory permissions: {}", path.display()))?;
        Ok(())
    }

    #[cfg(not(unix))]
    std::fs::create_dir_all(path)
        .with_context(|| format!("Failed to create directory: {}", path.display()))?;

    #[cfg(not(unix))]
    Ok(())
}

/// Write file with secure permissions (0o600 on Unix).
/// Uses `OpenOptions::mode()` to set permissions at creation time, and
/// `set_permissions()` to re-harden existing files on update.
fn write_secure_file(path: &PathBuf, content: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::fs::PermissionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        // Re-harden permissions on pre-existing files (mode() only applies at creation)
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to set file permissions: {}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_set_not_expired_when_far_future() {
        let tokens = TokenSet {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: u64::MAX,
        };
        assert!(!tokens.is_expired());
    }

    #[test]
    fn token_set_expired_when_past() {
        let tokens = TokenSet {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: 0,
        };
        assert!(tokens.is_expired());
    }

    #[test]
    fn token_set_expired_within_buffer() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // Expires in 30s — within the 60s buffer
        let tokens = TokenSet {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: now + 30,
        };
        assert!(tokens.is_expired());
    }

    #[test]
    fn token_set_serialization_round_trip() {
        let tokens = TokenSet {
            access_token: "access123".to_string(),
            refresh_token: "refresh456".to_string(),
            expires_at: 1700000000,
        };
        let json = serde_json::to_string(&tokens).unwrap();
        let parsed: TokenSet = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.access_token, "access123");
        assert_eq!(parsed.refresh_token, "refresh456");
        assert_eq!(parsed.expires_at, 1700000000);
    }

    #[test]
    fn save_load_delete_round_trip() {
        let name = "test-oauth-roundtrip";
        let tokens = TokenSet {
            access_token: "a".to_string(),
            refresh_token: "r".to_string(),
            expires_at: 9999999999,
        };
        save_tokens(name, &tokens).unwrap();

        let loaded = load_tokens(name).unwrap().unwrap();
        assert_eq!(loaded.access_token, "a");
        assert_eq!(loaded.refresh_token, "r");

        delete_tokens(name).unwrap();
        assert!(load_tokens(name).unwrap().is_none());
    }

    #[test]
    fn load_tokens_returns_none_for_missing() {
        assert!(load_tokens("nonexistent-provider-xyz").unwrap().is_none());
    }

    #[test]
    fn delete_tokens_ok_when_missing() {
        assert!(delete_tokens("nonexistent-provider-xyz").is_ok());
    }

    #[test]
    fn sanitize_provider_name_accepts_valid() {
        assert_eq!(
            sanitize_provider_name("my-provider_1.0").unwrap(),
            "my-provider_1.0"
        );
    }

    #[test]
    fn sanitize_provider_name_rejects_path_traversal() {
        assert!(sanitize_provider_name("../etc/passwd").is_err());
        assert!(sanitize_provider_name("..").is_err());
        assert!(sanitize_provider_name(".").is_err());
        assert!(sanitize_provider_name("").is_err());
    }

    #[test]
    fn sanitize_provider_name_rejects_slashes() {
        assert!(sanitize_provider_name("foo/bar").is_err());
        assert!(sanitize_provider_name("foo\\bar").is_err());
    }

    #[test]
    fn sanitize_provider_name_rejects_special_chars() {
        assert!(sanitize_provider_name("foo bar").is_err());
        assert!(sanitize_provider_name("foo@bar").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn save_tokens_creates_secure_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let name = "test-oauth-perms";
        let tokens = TokenSet {
            access_token: "a".to_string(),
            refresh_token: "r".to_string(),
            expires_at: 9999999999,
        };
        save_tokens(name, &tokens).unwrap();

        let path = token_path(name).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);

        let dir = tokens_dir().unwrap();
        let dir_meta = std::fs::metadata(&dir).unwrap();
        assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700);

        delete_tokens(name).unwrap();
    }

    #[tokio::test]
    async fn resolve_oauth_token_returns_none_when_no_tokens() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: "test-resolve-no-tokens".to_string(),
        };
        let result = resolve_oauth_token(&endpoint).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn resolve_oauth_token_returns_valid_token() {
        let name = "test-resolve-valid";
        let tokens = TokenSet {
            access_token: "valid-at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: u64::MAX,
        };
        save_tokens(name, &tokens).unwrap();

        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: name.to_string(),
        };
        let result = resolve_oauth_token(&endpoint).await.unwrap();
        assert_eq!(result, Some("valid-at".to_string()));

        delete_tokens(name).unwrap();
    }

    #[tokio::test]
    async fn resolve_oauth_token_refreshes_expired_token() {
        let name = "test-resolve-refresh";
        let tokens = TokenSet {
            access_token: "old-at".to_string(),
            refresh_token: "old-rt".to_string(),
            expires_at: 0, // expired
        };
        save_tokens(name, &tokens).unwrap();

        // Set up mock server for refresh
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"access_token":"refreshed-at","refresh_token":"refreshed-rt","expires_in":3600}"#,
            )
            .create_async()
            .await;

        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: format!("{}/token", server.url()),
            scopes: "openid".to_string(),
            client_id: "test".to_string(),
            client_secret: None,
            provider_name: name.to_string(),
        };

        let result = resolve_oauth_token(&endpoint).await.unwrap();
        assert_eq!(result, Some("refreshed-at".to_string()));

        // Verify refreshed tokens were saved
        let loaded = load_tokens(name).unwrap().unwrap();
        assert_eq!(loaded.access_token, "refreshed-at");
        assert_eq!(loaded.refresh_token, "refreshed-rt");

        delete_tokens(name).unwrap();
    }

    #[test]
    fn load_tokens_returns_error_on_invalid_json() {
        let name = "test-oauth-invalid-json";
        let path = token_path(name).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        std::fs::write(&path, "{not valid json}").unwrap();

        let result = load_tokens(name);
        assert!(result.is_err(), "Invalid JSON should produce an error");
        let err = result.err().unwrap().to_string();
        assert!(
            err.contains("Failed to parse token file"),
            "Error should mention parsing: {err}"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn delete_tokens_succeeds_when_no_file() {
        let result = delete_tokens("test-oauth-nonexistent-provider");
        assert!(
            result.is_ok(),
            "delete_tokens should succeed even if no file exists"
        );
    }

    #[test]
    fn token_path_rejects_empty_name() {
        assert!(token_path("").is_err());
    }

    #[test]
    fn token_path_rejects_dotdot() {
        assert!(token_path("..").is_err());
    }

    #[test]
    fn token_path_produces_json_extension() {
        let path = token_path("my-provider").unwrap();
        assert!(path.to_str().unwrap().ends_with("my-provider.json"));
    }
}
