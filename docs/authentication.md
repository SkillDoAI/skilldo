# Authentication

Skilldo supports multiple authentication methods depending on your LLM provider.

## API Keys (Default)

The simplest method. Set an environment variable and reference it in your config:

```bash
export OPENAI_API_KEY="sk-your-key-here"
```

```toml
[llm]
provider_type = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"
```

For local models (Ollama), set `api_key_env = "none"`.

## OAuth 2.0 + PKCE

Use your existing ChatGPT Plus/Pro or Google Workspace subscription instead of paying per-token API rates.

### Setup

Add OAuth fields to your `skilldo.toml`:

```toml
[llm]
provider_type = "chatgpt"
provider_name = "openai-sub"
model = "gpt-5.3"
oauth_auth_url = "https://auth.openai.com/oauth/authorize"
oauth_token_url = "https://auth.openai.com/oauth/token"
oauth_scopes = "openid profile email offline_access"
oauth_client_id_env = "OPENAI_CLIENT_ID"
```

### Google Credentials JSON Shortcut

Any provider using Google's `client_secret_*.json` format can base64-encode the file into a single env var:

```bash
export GOOGLE_OAUTH_CREDENTIALS="$(base64 < client_secret_123.json)"
```

```toml
[llm]
provider_type = "gemini"
provider_name = "google-workspace"
model = "gemini-2.5-pro"
oauth_credentials_env = "GOOGLE_OAUTH_CREDENTIALS"
```

### Commands

```bash
skilldo auth login              # Browser-based OAuth login for all configured providers
skilldo auth login --config x   # Login for providers in a specific config
skilldo auth status             # Show token expiry and validity
skilldo auth logout             # Remove all stored tokens
```

### Token Storage

Tokens are stored at `~/.config/skilldo/tokens/{provider_name}.json` with restricted permissions (0600). Tokens auto-refresh when expired.

### Per-Stage OAuth

Each pipeline stage can use a different provider and OAuth config:

```toml
[llm]
provider_type = "anthropic"
model = "claude-sonnet-4-6"
api_key_env = "ANTHROPIC_API_KEY"

[generation.review_llm]
provider_type = "chatgpt"
model = "gpt-5.3-codex"
oauth_auth_url = "https://auth.openai.com/oauth/authorize"
oauth_token_url = "https://auth.openai.com/oauth/token"
oauth_scopes = "openid profile email offline_access"
oauth_client_id_env = "OPENAI_CLIENT_ID"
```

Run `skilldo auth login --config your-config.toml` to authenticate all OAuth providers in one go.

## CLI Provider Mode

No API key or OAuth needed — shell out to vendor CLIs that handle their own auth:

```toml
[llm]
provider_type = "cli"
cli_command = "claude"
cli_args = ["-p", "--output-format", "json"]
cli_json_path = "result"
```

See [Configuration](configuration.md#cli-provider-mode) for details.
