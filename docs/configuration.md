# Configuration

Skilldo uses a TOML config file. It searches for config in this order:
1. `--config <path>` (explicit CLI argument)
2. `./skilldo.toml` (repository root)
3. `~/.config/skilldo/config.toml` (user config directory)

## Minimal Config

If you have an OpenAI API key, this is all you need:

```toml
[llm]
provider_type = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"
```

Set your API key before running: `export OPENAI_API_KEY="sk-your-key-here"`

This uses GPT-5.2 for all stages, with test validation enabled by default.

## Full Documented Config

```toml
# ── LLM Provider ──────────────────────────────────────────────
[llm]
# Provider type: "anthropic", "openai", "chatgpt", "gemini", or "openai-compatible"
provider_type = "anthropic"

# Human-readable name for this provider instance.
# Used as a label in logs and as a token storage key for OAuth.
# provider_name = "anthropic-main"

# Model name (provider-specific)
model = "claude-sonnet-4-6"

# Environment variable containing the API key.
# Set to "none" for local models (Ollama) that don't need a key.
api_key_env = "ANTHROPIC_API_KEY"

# Base URL — only needed for openai-compatible providers
# base_url = "http://localhost:11434/v1"

# Override max output tokens per LLM request.
# Defaults: anthropic=8192, openai=8192, openai-compatible=16384, gemini=8192
# max_tokens = 8192

# Number of automatic retries on network/API errors (default: 10)
# network_retries = 10

# Delay in seconds between retries (default: 120)
# retry_delay = 120

# Timeout for individual LLM requests in seconds (default: 120)
# request_timeout_secs = 120

# Extra HTTP headers injected into every LLM API request.
# extra_headers = ["ChatGPT-Account-ID: acct_abc123", "OpenAI-Beta: assistants=v2"]

# Extra fields merged into the LLM request body.
# [llm.extra_body]
# truncate = "END"
# Or raw JSON string:
# extra_body_json = '{"reasoning": {"effort": "high"}}'

# ── Generation Settings ───────────────────────────────────────
[generation]
# Max retry attempts for the generate→validate loop (default: 5)
max_retries = 5

# Run extract/map/learn in parallel (default: true). CLI: --no-parallel
parallel_extraction = true

# Approximate token budget for source code sent to agents (default: 100000)
max_source_tokens = 100000

# Enable test stage code validation (default: true). CLI: --no-test
enable_test = true

# Test validation mode (default: "thorough")
#   "thorough"  — test every extracted pattern
#   "adaptive"  — test patterns, reduce scope on repeated failures
#   "minimal"   — test only core import + one pattern
test_mode = "thorough"

# Enable review stage (default: true). CLI: --no-review
enable_review = true

# Enable telemetry logging to ~/.skilldo/runs.csv (default: false)
# telemetry = true

# ── Per-Stage LLM Overrides (Optional) ────────────────────────
# Run individual stages on different providers/models.
# Each is optional — if not set, the stage uses [llm].
#
# [generation.test_llm]
# provider_type = "openai"
# model = "gpt-5.2"
# api_key_env = "OPENAI_API_KEY"
#
# Also available: extract_llm, map_llm, learn_llm, create_llm, review_llm

# ── Container / Execution Settings ────────────────────────────
[generation.container]
# Container runtime: "podman" or "docker" (default: auto-detected, prefers podman)
runtime = "docker"

# Timeout for test execution in seconds (default: 60)
timeout = 60

# Auto-remove containers after execution (default: true)
cleanup = true

# Container images per language (defaults shown — override for specific versions)
# python_image = "ghcr.io/astral-sh/uv:python3.11-bookworm-slim"
# javascript_image = "node:24-alpine"
# rust_image = "rust:1.75-slim"
# go_image = "golang:1.25-alpine"

# ── Custom Prompts (Advanced) ────────────────────────────────
# Override or extend the built-in stage prompts.
# [prompts]
# override_prompts = false
# extract_mode = "append"
# extract_custom = "Also extract all class methods that start with 'get_'"
# create_mode = "overwrite"
# create_custom = "Your entirely custom create prompt here..."
```

## Supported Providers

| Provider | Config `provider_type` | Needs API Key | Notes |
|----------|------------------------|---------------|-------|
| **Anthropic** | `"anthropic"` | Yes (`ANTHROPIC_API_KEY`) | Claude models |
| **OpenAI** | `"openai"` | Yes (`OPENAI_API_KEY`) | GPT models. Handles `max_completion_tokens` for GPT-5+. |
| **ChatGPT** | `"chatgpt"` | Yes (OAuth or `OPENAI_API_KEY`) | Uses the Responses API. Models: `gpt-5.2-codex`, `gpt-5.1-codex-mini`, etc. See [Authentication](authentication.md). |
| **Google Gemini** | `"gemini"` | Yes (`GEMINI_API_KEY`) | Gemini models |
| **OpenAI-compatible** | `"openai-compatible"` | Varies | Ollama, DeepSeek, Groq, Together, Fireworks, xAI, Mistral, vLLM, etc. Set `base_url`. |

## CLI Provider Mode

Shell out to vendor CLIs (Claude Code, Codex, Gemini CLI) instead of making HTTP API calls. Useful when you have a subscription but direct API access isn't available.

```toml
[llm]
provider_type = "cli"
model = "claude-sonnet-4-6"  # informational label
cli_command = "claude"
cli_args = ["-p", "--output-format", "json"]
cli_json_path = "result"
```

The prompt is piped to the CLI via stdin. If `cli_json_path` is set, stdout is parsed as JSON and that path is extracted as the response text. Dot-notation is supported for nested fields.

Parallel extraction is automatically disabled for CLI providers (vendor CLIs typically share a single auth session).

Other CLI examples:

```toml
# Codex CLI
provider_type = "cli"
cli_command = "codex"
cli_args = ["-a", "never", "exec", "--json"]

# Gemini CLI
provider_type = "cli"
cli_command = "gemini"
cli_args = ["-p", "", "--output-format", "json", "-m", "gemini-3-pro-preview", "-y"]
cli_json_path = "response"
```

## Example Config Files

Ready-to-use configs for common setups:

- [`examples/configs/anthropic.toml`](../examples/configs/anthropic.toml) — Claude Sonnet (cloud)
- [`examples/configs/openai.toml`](../examples/configs/openai.toml) — GPT-5.2 (cloud)
- [`examples/configs/ollama.toml`](../examples/configs/ollama.toml) — Qwen3-Coder via Ollama (local, no API key)
- [`examples/configs/hybrid.toml`](../examples/configs/hybrid.toml) — Local extraction + cloud validation
- [`examples/configs/deepseek.toml`](../examples/configs/deepseek.toml) — DeepSeek (OpenAI-compatible cloud)
- [`examples/configs/reference.toml`](../examples/configs/reference.toml) — **Every field documented**

## Config Check

Validate your configuration file without running a generation:

```bash
skilldo config check --config my-config.toml
```

Reports missing fields, invalid provider names, unreachable base URLs, and malformed `extra_body_json` before you burn API credits.
