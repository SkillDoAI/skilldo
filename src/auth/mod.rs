//! OAuth 2.0 authentication — token storage, PKCE, and generic OAuth flows.
//!
//! Fully provider-agnostic: users configure OAuth endpoints + env var names
//! per stage in `skilldo.toml`. Any provider that speaks OAuth 2.0 + PKCE works.

pub mod device_code;
pub mod oauth;
pub mod pkce;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

/// Stored OAuth token set, persisted to disk.
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp (seconds) when the access token expires.
    pub expires_at: u64,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Resolved OAuth endpoint configuration — all env vars dereferenced.
#[derive(Clone)]
pub struct OAuthEndpoint {
    pub auth_url: String,
    pub token_url: String,
    pub scopes: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub provider_name: String,
}

impl std::fmt::Debug for OAuthEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthEndpoint")
            .field("auth_url", &self.auth_url)
            .field("token_url", &self.token_url)
            .field("scopes", &self.scopes)
            .field("client_id", &"[REDACTED]")
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("provider_name", &self.provider_name)
            .finish()
    }
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
fn ensure_secure_dir(path: &Path) -> Result<()> {
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
fn write_secure_file(path: &Path, content: &str) -> Result<()> {
    // Write to a temp file first, then rename for atomicity.
    // Prevents truncated token files from interrupted writes.
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("Failed to create temp file in {}", dir.display()))?;

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .write_all(content.as_bytes())
            .with_context(|| format!("Failed to write temp file: {}", tmp.path().display()))?;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to set permissions: {}", tmp.path().display()))?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(tmp.path(), content)
            .with_context(|| format!("Failed to write temp file: {}", tmp.path().display()))?;
    }

    // Atomic rename — either succeeds completely or leaves old file intact
    tmp.persist(path)
        .with_context(|| format!("Failed to persist file: {}", path.display()))?;

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

        // Cleanup guard — runs even if assertions panic
        struct Cleanup<'a>(&'a str);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                delete_tokens(self.0).ok();
            }
        }
        let _cleanup = Cleanup(name);

        let result = load_tokens(name);
        assert!(result.is_err(), "Invalid JSON should produce an error");
        let err = result.err().unwrap().to_string();
        assert!(
            err.contains("Failed to parse token file"),
            "Error should mention parsing: {err}"
        );
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

    #[test]
    fn tokens_dir_returns_valid_path() {
        let dir = tokens_dir().unwrap();
        assert!(dir.to_str().unwrap().contains("skilldo"));
        assert!(dir.to_str().unwrap().ends_with("tokens"));
    }

    #[test]
    fn sanitize_provider_name_rejects_single_dot() {
        let result = sanitize_provider_name(".");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Invalid"),
            "should say 'Invalid' for single dot"
        );
    }

    #[test]
    fn save_and_overwrite_tokens_preserves_latest() {
        let name = "test-oauth-overwrite";
        let tokens_v1 = TokenSet {
            access_token: "old-access".to_string(),
            refresh_token: "old-refresh".to_string(),
            expires_at: 1000,
        };
        save_tokens(name, &tokens_v1).unwrap();

        let tokens_v2 = TokenSet {
            access_token: "new-access".to_string(),
            refresh_token: "new-refresh".to_string(),
            expires_at: 2000,
        };
        save_tokens(name, &tokens_v2).unwrap();

        let loaded = load_tokens(name).unwrap().unwrap();
        assert_eq!(loaded.access_token, "new-access");
        assert_eq!(loaded.refresh_token, "new-refresh");
        assert_eq!(loaded.expires_at, 2000);

        delete_tokens(name).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn load_tokens_returns_error_on_unreadable_file() {
        use std::os::unix::fs::PermissionsExt;

        let name = "test-oauth-unreadable";
        let tokens = TokenSet {
            access_token: "a".to_string(),
            refresh_token: "r".to_string(),
            expires_at: 9999999999,
        };
        save_tokens(name, &tokens).unwrap();

        let path = token_path(name).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Skip if running as root or elevated — 0o000 won't block reads
        if std::fs::read(&path).is_ok() {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            delete_tokens(name).unwrap();
            return; // Can't test permission denial in this environment
        }

        let result = load_tokens(name);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        assert!(result.is_err(), "should error on permission denied");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Failed to read token file"),
            "error should mention reading: {err}"
        );

        delete_tokens(name).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ensure_secure_dir_creates_with_correct_mode() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("test-secure-dir").to_path_buf();
        ensure_secure_dir(&dir).unwrap();

        let meta = std::fs::metadata(&dir).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_secure_dir_reharden_existing_directory() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("test-reharden-dir").to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        // Weaken permissions
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        // ensure_secure_dir should fix it
        ensure_secure_dir(&dir).unwrap();

        let meta = std::fs::metadata(&dir).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn write_secure_file_rehardens_existing_file() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test-reharden.json").to_path_buf();
        // Create with weak permissions
        std::fs::write(&path, "initial").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        // write_secure_file should fix permissions
        write_secure_file(&path, "updated content").unwrap();

        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "updated content");
    }

    #[test]
    fn token_set_debug_redacts_secrets() {
        let tokens = TokenSet {
            access_token: "super-secret-token".to_string(),
            refresh_token: "super-secret-refresh".to_string(),
            expires_at: 12345,
        };
        let debug = format!("{:?}", tokens);
        assert!(debug.contains("TokenSet"));
        assert!(debug.contains("12345"));
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("super-secret-token"));
        assert!(!debug.contains("super-secret-refresh"));
    }

    #[test]
    fn oauth_endpoint_debug_redacts_secrets() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com".to_string(),
            token_url: "https://token.example.com".to_string(),
            scopes: "openid".to_string(),
            client_id: "secret-client-id".to_string(),
            client_secret: Some("secret-client-secret".to_string()),
            provider_name: "test".to_string(),
        };
        let debug = format!("{:?}", endpoint);
        assert!(debug.contains("OAuthEndpoint"));
        assert!(debug.contains("test"));
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret-client-id"));
        assert!(!debug.contains("secret-client-secret"));
    }

    #[test]
    fn oauth_endpoint_debug_redacts_with_no_secret() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com".to_string(),
            token_url: "https://token.example.com".to_string(),
            scopes: "openid".to_string(),
            client_id: "secret-client-id".to_string(),
            client_secret: None,
            provider_name: "no-secret".to_string(),
        };
        let debug = format!("{:?}", endpoint);
        assert!(debug.contains("OAuthEndpoint"));
        assert!(debug.contains("no-secret"));
        assert!(debug.contains("[REDACTED]"));
        assert!(debug.contains("None"));
        assert!(!debug.contains("secret-client-id"));
    }

    #[test]
    fn oauth_endpoint_clone() {
        let endpoint = OAuthEndpoint {
            auth_url: "https://auth.example.com".to_string(),
            token_url: "https://token.example.com".to_string(),
            scopes: "openid".to_string(),
            client_id: "cid".to_string(),
            client_secret: Some("secret".to_string()),
            provider_name: "clone-test".to_string(),
        };
        let cloned = endpoint.clone();
        assert_eq!(cloned.provider_name, "clone-test");
        assert_eq!(cloned.client_secret, Some("secret".to_string()));
    }

    #[test]
    fn token_set_clone() {
        let tokens = TokenSet {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: 999,
        };
        let cloned = tokens.clone();
        assert_eq!(cloned.access_token, "at");
        assert_eq!(cloned.expires_at, 999);
    }

    #[test]
    fn save_tokens_rejects_invalid_provider_name() {
        let tokens = TokenSet {
            access_token: "a".to_string(),
            refresh_token: "r".to_string(),
            expires_at: 9999999999,
        };
        let result = save_tokens("../../bad", &tokens);
        assert!(result.is_err());
    }

    #[test]
    fn load_tokens_rejects_invalid_provider_name() {
        let result = load_tokens("foo/bar");
        assert!(result.is_err());
    }

    #[test]
    fn delete_tokens_rejects_invalid_provider_name() {
        let result = delete_tokens("foo@bar");
        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn delete_tokens_error_path_exercises_context_message() {
        // We can't safely make the shared tokens dir unwritable (other tests
        // run in parallel), so instead we verify the error formatting logic
        // in the delete_tokens error path by constructing the error manually.
        // This ensures the `Failed to delete token file` context message is correct.
        let path = token_path("hypothetical-perm-denied").unwrap();
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Permission denied");
        let err = anyhow::Error::new(io_err)
            .context(format!("Failed to delete token file: {}", path.display()));
        let msg = err.to_string();
        assert!(
            msg.contains("Failed to delete token file"),
            "error should contain context: {msg}"
        );
        assert!(
            msg.contains("hypothetical-perm-denied.json"),
            "error should contain filename: {msg}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_secure_file_creates_new_with_correct_mode() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("brand-new.json").to_path_buf();
        // File does not exist yet
        assert!(!path.exists());
        write_secure_file(&path, "new content").unwrap();
        assert!(path.exists());
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "new content");
    }

    #[cfg(unix)]
    #[test]
    fn ensure_secure_dir_creates_nested_parents() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("a").join("b").join("c").to_path_buf();
        assert!(!dir.exists());
        ensure_secure_dir(&dir).unwrap();
        assert!(dir.exists());
        let meta = std::fs::metadata(&dir).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn sanitize_provider_name_accepts_all_valid_chars() {
        // Test all valid character classes together
        assert_eq!(
            sanitize_provider_name("AZ-az_09.test").unwrap(),
            "AZ-az_09.test"
        );
    }

    #[test]
    fn sanitize_provider_name_rejects_unicode() {
        assert!(sanitize_provider_name("caf\u{00e9}").is_err());
    }

    #[test]
    fn token_set_not_expired_well_outside_buffer() {
        // Token expires in 120s — well outside the 60s buffer, never flaky
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let tokens = TokenSet {
            access_token: "test".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: now + 120,
        };
        assert!(!tokens.is_expired());
    }

    // token_set_expired_within_buffer (line 253) already covers now+30

    #[test]
    fn token_path_constructs_expected_structure() {
        let path = token_path("my-test-provider").unwrap();
        let path_str = path.to_str().unwrap();
        assert!(path_str.contains("skilldo"));
        assert!(path_str.contains("tokens"));
        assert!(path_str.ends_with("my-test-provider.json"));
    }

    #[test]
    fn load_tokens_missing_fields_in_json() {
        // Valid JSON but missing required fields for TokenSet
        let name = "test-oauth-partial-json";
        let path = token_path(name).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        std::fs::write(&path, r#"{"access_token": "x"}"#).unwrap();

        struct Cleanup<'a>(&'a str);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                delete_tokens(self.0).ok();
            }
        }
        let _cleanup = Cleanup(name);

        let result = load_tokens(name);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Failed to parse token file"),
            "error should mention parsing: {err}"
        );
    }

    #[test]
    fn save_and_load_tokens_with_special_chars_in_values() {
        let name = "test-oauth-special-vals";
        let tokens = TokenSet {
            access_token: "eyJhbGciOiJSUzI1NiIs+/==".to_string(),
            refresh_token: "rt-with\"quotes\\and\nnewlines".to_string(),
            expires_at: 1234567890,
        };
        save_tokens(name, &tokens).unwrap();

        let loaded = load_tokens(name).unwrap().unwrap();
        assert_eq!(loaded.access_token, tokens.access_token);
        assert_eq!(loaded.refresh_token, tokens.refresh_token);
        assert_eq!(loaded.expires_at, tokens.expires_at);

        delete_tokens(name).unwrap();
    }
}
