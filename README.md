# Skilldo

[![CI](https://github.com/SkillDoAI/skilldo/actions/workflows/ci.yml/badge.svg)](https://github.com/SkillDoAI/skilldo/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/SkillDoAI/skilldo/graph/badge.svg)](https://codecov.io/gh/SkillDoAI/skilldo)

*Pronounced "skill-do"* — The artificial skill generator.

Skilldo automatically generates `SKILL.md` agent rules files for open-source libraries. Point it at any repo and get structured rules that help AI coding assistants (Claude, Cursor, Copilot, Codex, etc.) use the library correctly — with validated, tested code examples.

- **6-agent pipeline** — Extract, Map, Learn, Create, Review, and Test stages with automatic retry loops on failure
- **Container-validated code** — Generated patterns are compiled and executed in Docker/Podman containers to catch hallucinated APIs before they ship
- **3-layer security scanning** — Regex patterns + prompt injection detection + 41 YARA rules (including [Cisco AI Defense](https://github.com/cisco-ai-defense/skill-scanner)) catch malicious content
- **Multi-provider** — Anthropic, OpenAI, Google Gemini, or any OpenAI-compatible endpoint (Ollama, DeepSeek, Groq, vLLM, etc.)
- **Per-stage model mixing** — Use a cheap local model for extraction and a frontier cloud model for review. One config, multiple providers.
- **OAuth 2.0 authentication** — Use your ChatGPT Plus/Pro or Google Workspace subscription instead of paying per-token API rates
- **CLI provider mode** — Shell out to vendor CLIs (Claude Code, Codex, Gemini CLI) for subscription-based access without API keys
- **Python + Go ecosystems** — Full pipeline support with language-specific parsers, code generators, and container images
- **Free with local models** — Run the entire pipeline on Ollama with zero API cost
- **28+ pre-generated skills** — Browse [`examples/skills/`](examples/skills/) for ready-to-use rules for popular Python libraries

The goal: make agent rules a standard part of every open-source package — like README.md or .gitignore.

> **Config file vs CLI:** Skilldo supports both a TOML config file and CLI flags. The config file is more powerful — it supports per-stage model overrides, OAuth, custom headers, extra request body fields, and prompt customization. CLI flags are great for quick runs or overriding a single setting. See [Configuration](#configuration) for details.

## Table of Contents

- [How It Works](#how-it-works)
- [The Agents](#the-agents)
- [Install](#install)
- [Quick Start](#quick-start)
- [Usage](#usage)
  - [Debugging](#debugging)
  - [Examples](#examples)
  - [Lint](#lint)
  - [Security Scanner](#security-scanner)
  - [Config Check](#config-check)
- [Configuration](#configuration)
  - [Minimal Config](#minimal-config-get-started-fast)
  - [Full Documented Config](#full-documented-config)
  - [Supported Providers](#supported-providers)
  - [Example Config Files](#example-config-files)
  - [Authentication (OAuth 2.0)](#authentication-oauth-20)
  - [CLI Provider Mode](#cli-provider-mode)
- [Example Output](#example-output)
- [Tips and Model Experience](#tips-and-model-experience)
- [Language Support](#language-support)
- [Building from Source](#building-from-source)
- [Requirements](#requirements)
- [License](#license)

## How It Works

Skilldo reads a library's source directory and runs a 6-stage pipeline to extract knowledge and synthesize it into a single `SKILL.md` file:

```text
Source Code ──→ Extract (API Surface)       ──┐
Test Files  ──→ Map (Pattern Extraction)    ──┤──→ Create ──→ Review ──→ Test ──→ SKILL.md
Docs/README ──→ Learn (Context Extraction)  ──┘      ↑          │         │
                                                     └──────────┴─────────┘
                                                      (retry on failure)
```

1. **Collect** — Discovers source files, tests, documentation, and changelogs from the local directory
2. **Extract** — Three stages work in parallel to pull out the API surface, usage patterns, and conventions/pitfalls
3. **Create** — Combines everything into a formatted SKILL.md
4. **Review** — Verifies accuracy (dates, signatures, consistency) and safety (prompt injection, nefarious content)
5. **Test** — Generates test code from the patterns and runs it in a container to verify correctness
6. **Iterate** — If review or test fails, feedback loops back for regeneration (configurable retries)

The output is a structured Markdown file with YAML frontmatter, ready to drop into any repository.

## The Agents

| Stage | Role | What It Does |
|-------|------|-------------|
| **Extract** | API Surface | Reads source code to identify public functions, classes, methods, parameters, return types, and deprecations |
| **Map** | Usage Patterns | Reads test files to extract real-world usage examples showing how the library is actually used |
| **Learn** | Conventions & Pitfalls | Reads docs, README, and changelogs to find common mistakes, migration notes, and best practices |
| **Create** | SKILL.md Composition | Combines output from extract/map/learn into a structured SKILL.md with sections for imports, patterns, pitfalls, and more |
| **Review** | Accuracy & Safety | Verifies dates, API signatures, and content consistency; checks for prompt injection, destructive commands, and credential leaks |
| **Test** | Code Validation | Generates runnable test code from the SKILL.md patterns and executes it in a Docker/Podman container to verify they actually work |

Test is optional but recommended. When enabled, it catches hallucinated APIs, wrong parameter names, and broken code examples before they ship. If validation fails, Create regenerates with the error feedback.

## Install

```bash
# macOS / Linux via Homebrew
brew install skilldoai/tap/skilldo

# Or build from source
cargo build --release
```

Binaries for macOS (ARM) and Linux (x86_64, ARM) are also available on the [Releases](https://github.com/SkillDoAI/skilldo/releases) page.

## Quick Start

Skilldo works on any local directory containing library source code. Clone a repo (or point it at one you already have) and generate:

```bash
# Clone a library and generate its SKILL.md
git clone --depth 1 https://github.com/pallets/click.git /tmp/click
skilldo generate /tmp/click --config my-config.toml --output click-SKILL.md
```

The input path is just a directory — it doesn't need to be a git repo. Git is only used as a fallback for version detection (reading the latest tag) when package metadata files don't contain a version.

You'll need a config file with your LLM provider credentials. See [Configuration](#configuration) below.

## Usage

```
skilldo generate [PATH] [OPTIONS]
```

**Arguments:**

| Argument | Default | Description |
|----------|---------|-------------|
| `PATH` | `.` (current dir) | Path to the library's source directory |

**Options:**

| Option | Description |
|--------|-------------|
| `--language <LANG>` | Force language detection (`python`, `javascript`, `rust`, `go`). Auto-detected if omitted. |
| `-o, --output <PATH>` | Output file path. Default: `SKILL.md` |
| `-i, --input <PATH>` | Existing SKILL.md to update (enables update mode) |
| `--version <VER>` | Explicit version override, e.g. `"2.1.0"` |
| `--version-from <STRATEGY>` | Version extraction strategy: `git-tag`, `package`, `branch`, `commit` |
| `--config <PATH>` | Path to config file |
| `--provider <PROVIDER>` | LLM provider: `anthropic`, `openai`, `chatgpt`, `gemini`, `openai-compatible` |
| `--model <MODEL>` | Override LLM model (e.g., `gpt-5.2`, `claude-sonnet-4-6`) |
| `--base-url <URL>` | Base URL for openai-compatible providers |
| `--max-retries <N>` | Override max generation retries |
| `--no-test` | Disable test stage container validation |
| `--test-mode <MODE>` | Test validation mode: `thorough`, `adaptive`, `minimal` |
| `--test-model <MODEL>` | Override test stage LLM model |
| `--test-provider <PROVIDER>` | Override test stage LLM provider |
| `--runtime <RUNTIME>` | Container runtime: `docker` or `podman` |
| `--timeout <SECONDS>` | Container execution timeout |
| `--no-parallel` | Run extract/map/learn sequentially instead of in parallel (useful for local models) |
| `-q, --quiet` | Suppress info messages, show warnings and errors only |
| `-v, --verbose` | Show debug output (generated code, container commands, LLM calls) |
| `--dry-run` | Use a mock LLM client (no API key needed, for testing) |

### Debugging

```bash
# Show what's happening during a run
skilldo -v generate /path/to/repo

# Inspect the exact prompts sent to the LLM
skilldo show-prompts --language go
skilldo show-prompts --language python --stage test

# For granular module-level tracing (power users)
RUST_LOG=skilldo::test_agent=trace skilldo generate /path/to/repo
```

### Examples

```bash
# Quick start — no config file needed (uses env vars for API key)
skilldo generate /path/to/repo --provider openai --model gpt-5.2

# With Anthropic
skilldo generate /path/to/repo --provider anthropic --model claude-sonnet-4-6

# With local Ollama model
skilldo generate /path/to/repo --provider openai-compatible --model codestral:latest \
  --base-url http://localhost:11434/v1 --no-parallel

# Using a config file (recommended for repeated use)
skilldo generate /path/to/repo --config my-config.toml -o click-SKILL.md

# Force Python, custom output path
skilldo generate /path/to/repo --language python -o rules/SKILL.md

# Update an existing SKILL.md (reads it, regenerates, writes back)
skilldo generate /path/to/repo -i SKILL.md -o SKILL.md

# Use Podman instead of Docker for test validation
skilldo generate /path/to/repo --runtime podman

# Skip test validation (faster, no container needed)
skilldo generate /path/to/repo --no-test

# Run agents sequentially (for local models that can't handle parallel requests)
skilldo generate /path/to/repo --no-parallel

# Dry run to test without an LLM
skilldo generate /path/to/repo --dry-run
```

### Lint

Validate a generated SKILL.md for structural and security issues:

```bash
skilldo lint click-SKILL.md
```

Checks for: missing frontmatter, missing required sections, unclosed code blocks, degeneration patterns (repeated text, gibberish), prompt instruction leaks, and security violations (destructive commands, credential access, exfiltration URLs, reverse shells, obfuscated payloads).

The linter runs automatically after generation and also in CI.

### Security Scanner

Skilldo includes a three-layer security scanner that runs during the review stage:

1. **Regex patterns** — credential access, destructive commands, exfiltration URLs, reverse shells, obfuscated payloads
2. **Prompt injection detection** — instruction overrides, identity reassignment, secrecy demands, indirect injection
3. **YARA rules** — 24 SkillDo rules (SD-001 to SD-211) + 17 vendored [Cisco AI Defense](https://github.com/cisco-ai-defense/skill-scanner) rules (Apache 2.0)

YARA rules are evaluated at runtime via [boreal](https://github.com/vthib/boreal), a pure Rust YARA engine. All SkillDo and Cisco rules ship in `rules/`.

**YARA compatibility note:** The YARA specification (libyara) does not support non-capturing groups `(?:...)` in regex patterns. Cisco's rules use `(?:...)` because their scanner runs on [YARA-X](https://github.com/VirusTotal/yara-x), VirusTotal's Rust rewrite that delegates regex parsing to the `regex_syntax` crate — which accepts `(?:` as an extension beyond the spec. Boreal and libyara both reject this syntax. Skilldo includes a `patch_for_boreal()` preprocessor that rewrites `(?:...)` → `(...)` at load time, which is safe because YARA has no backreferences (capturing and non-capturing groups are functionally identical).

### Config Check

Validate your configuration file without running a generation:

```bash
skilldo config check --config my-config.toml
```

Reports missing fields, invalid provider names, unreachable base URLs, and malformed `extra_body_json` before you burn API credits.

## Configuration

Skilldo uses a TOML config file. It searches for config in this order:
1. `--config <path>` (explicit CLI argument)
2. `./skilldo.toml` (repository root)
3. `~/.config/skilldo/config.toml` (user config directory)

### Minimal Config (Get Started Fast)

If you have an OpenAI API key, this is all you need:

```toml
[llm]
provider_type = "openai"
model = "gpt-5.2"
# Name of the environment variable holding your API key.
# Skilldo reads the key from this env var at runtime — it never stores keys in config.
api_key_env = "OPENAI_API_KEY"
```

Set your API key before running: `export OPENAI_API_KEY="sk-your-key-here"`

This uses GPT-5.2 for all stages, with test validation enabled by default using Docker.

### Full Documented Config

```toml
# ── LLM Provider ──────────────────────────────────────────────
# Configures the model used for all stages (unless overridden
# per-stage via extract_llm, create_llm, test_llm, etc.).
[llm]
# Provider type: "anthropic", "openai", "chatgpt", "gemini", or "openai-compatible"
# "provider" also works (legacy alias, to be removed in 0.5.0).
# "chatgpt" uses the Responses API — for OAuth-based ChatGPT subscription access.
provider_type = "anthropic"

# Human-readable name for this provider instance.
# Used as a label in logs and as a token storage key for OAuth.
# Defaults to the provider_type value if not set.
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

# Number of automatic retries on network/API errors (default: 3)
# network_retries = 3

# Delay in seconds between retries (default: 2)
# retry_delay = 2

# Timeout for individual LLM requests in seconds (default: 120)
# request_timeout_secs = 120

# Extra HTTP headers injected into every LLM API request.
# Useful for provider-specific headers (e.g., ChatGPT-Account-ID, OpenAI-Beta).
# Format: "Header-Name: value" (colon-separated).
# extra_headers = ["ChatGPT-Account-ID: acct_abc123", "OpenAI-Beta: assistants=v2"]

# Extra fields merged into the LLM request body (for provider-specific params).
# TOML table style:
# [llm.extra_body]
# truncate = "END"
# Or raw JSON string:
# extra_body_json = '{"reasoning": {"effort": "high"}, "truncate": "END"}'

# ── Generation Settings ───────────────────────────────────────
[generation]
# Max retry attempts for the generate→validate loop (default: 5)
max_retries = 5

# Run extract/map/learn in parallel (default: true). CLI: --no-parallel
parallel_extraction = true

# Approximate token budget for source code sent to agents (default: 100000)
max_source_tokens = 100000

# Enable test stage code validation in containers (default: true). CLI: --no-test
enable_test = true

# Test validation mode (default: "thorough")
#   "thorough"  — test every extracted pattern
#   "adaptive"  — test patterns, reduce scope on repeated failures
#   "minimal"   — test only core import + one pattern
test_mode = "thorough"

# Enable review stage (default: true)
enable_review = true

# ── Per-Stage LLM Overrides (Optional) ────────────────────────
# Run individual stages on different providers/models.
# Each is optional — if not set, the stage uses [llm].
#
# [generation.test_llm]              # Test stage
# provider_type = "openai"
# model = "gpt-5.2"
# api_key_env = "OPENAI_API_KEY"
#
# Also available: extract_llm, map_llm, learn_llm, create_llm, review_llm
# See examples/configs/reference.toml for the full list.

# ── Container / Execution Settings ────────────────────────────
# The test stage runs generated test code inside containers or bare-metal.
[generation.container]
# Container runtime: "docker" or "podman" (default: "docker")
runtime = "docker"

# Execution mode: "container" (default) or "bare-metal"
# Bare-metal runs tests directly on the host — no Docker/Podman needed.
# Requires the language toolchain installed locally:
#   Python: uv + python3    Go: go toolchain
# execution_mode = "container"

# Timeout for test execution in seconds (default: 60)
# Increase for libraries with heavy dependencies (numpy, pytorch, etc.)
timeout = 60

# Auto-remove containers after execution (default: true)
cleanup = true

# Container images (override if you need specific versions)
# python_image = "ghcr.io/astral-sh/uv:python3.11-bookworm-slim"
# javascript_image = "node:20-slim"
# rust_image = "rust:1.75-slim"
# go_image = "golang:1.25-alpine"

# ── Custom Prompts (Advanced) ────────────────────────────────
# Override or extend the built-in stage prompts.
# Mode per stage: "append" (default) adds your text after the built-in
# prompt, "overwrite" replaces it entirely. Test stage is always append-only.
#
# [prompts]
# override_prompts = false            # Global default: false = append
# extract_mode = "append"             # Per-stage override
# extract_custom = "Also extract all class methods that start with 'get_'"
# create_mode = "overwrite"
# create_custom = "Your entirely custom create prompt here..."
```

### Supported Providers

| Provider | Config `provider_type` | Needs API Key | Notes |
|----------|------------------------|---------------|-------|
| **Anthropic** | `"anthropic"` | Yes (`ANTHROPIC_API_KEY`) | Claude models |
| **OpenAI** | `"openai"` | Yes (`OPENAI_API_KEY`) | GPT models. Handles `max_completion_tokens` for GPT-5+. |
| **ChatGPT** | `"chatgpt"` | Yes (OAuth or `OPENAI_API_KEY`) | Uses the Responses API. Models: `gpt-5.2-codex`, `gpt-5.1-codex-mini`, etc. See [Authentication](#authentication-oauth-20). |
| **Google Gemini** | `"gemini"` | Yes (`GEMINI_API_KEY`) | Gemini models |
| **OpenAI-compatible** | `"openai-compatible"` | Varies | Ollama, DeepSeek, Groq, Together, Fireworks, xAI, Mistral, vLLM, etc. Set `base_url`. |

### Example Config Files

Ready-to-use configs for common setups:

- [`examples/configs/anthropic.toml`](examples/configs/anthropic.toml) — Claude Sonnet (cloud)
- [`examples/configs/openai.toml`](examples/configs/openai.toml) — GPT-5.2 (cloud)
- [`examples/configs/ollama.toml`](examples/configs/ollama.toml) — Qwen3-Coder via Ollama (local, no API key)
- [`examples/configs/hybrid.toml`](examples/configs/hybrid.toml) — Local extraction + cloud validation (best of both worlds)
- [`examples/configs/deepseek.toml`](examples/configs/deepseek.toml) — DeepSeek (OpenAI-compatible cloud)
- [`examples/configs/per-agent-extra-body.toml`](examples/configs/per-agent-extra-body.toml) — Per-agent `extra_body` overrides (e.g., different reasoning effort per agent)
- [`examples/configs/github-models.toml`](examples/configs/github-models.toml) — GitHub Models free tier (for CI/testing)
- [`examples/configs/reference.toml`](examples/configs/reference.toml) — **Every field documented** (copy what you need)

### Authentication (OAuth 2.0)

Skilldo supports OAuth 2.0 + PKCE for providers that offer subscription-based API access. This lets you use your existing ChatGPT Plus/Pro or Google Workspace subscription instead of paying per-token API rates.

**Setup:** Add OAuth fields to your `skilldo.toml`:

```toml
[llm]
provider_type = "openai"
provider_name = "openai-sub"
model = "gpt-5.3"
oauth_auth_url = "https://auth.openai.com/oauth/authorize"
oauth_token_url = "https://auth.openai.com/oauth/token"
oauth_scopes = "openid profile email offline_access"
oauth_client_id_env = "OPENAI_CLIENT_ID"
```

**Credentials JSON shortcut** — any provider using Google's `client_secret_*.json` format (`{"installed": {...}}` or `{"web": {...}}`) can base64-encode the file into a single env var:

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

**Commands:**

```bash
skilldo auth login              # Browser-based OAuth login for all configured providers
skilldo auth status             # Show token expiry and validity
skilldo auth logout             # Remove all stored tokens
```

Tokens are stored at `~/.config/skilldo/tokens/{provider_name}.json` with restricted permissions (0600). Tokens auto-refresh when expired.

Per-stage OAuth is supported — each pipeline stage can use a different provider and OAuth config.

### CLI Provider Mode

Shell out to vendor CLIs (Claude Code, Codex, Gemini CLI) instead of making HTTP API calls. Useful when you have a subscription (Claude Pro, ChatGPT Plus) but direct API access isn't available or violates ToS.

```toml
[llm]
model_type = "cli"
cli_command = "claude"
cli_args = ["-p", "--output-format", "json"]
cli_json_path = "result"
model = "claude-sonnet-4-6"  # informational label
```

The prompt is piped to the CLI via stdin. If `cli_json_path` is set, stdout is parsed as JSON and that field is extracted as the response text.

Parallel extraction is automatically disabled for CLI providers (vendor CLIs typically share a single auth session).

Other CLI examples:

```toml
# Codex CLI
cli_command = "codex"
cli_args = ["-a", "never", "exec", "--json"]

# Gemini CLI
cli_command = "gemini"
cli_args = ["--output-format", "json", "-y"]
cli_json_path = "result"
```

## Example Output

We've generated SKILL.md files for **28 Python libraries** and counting. Browse them all in [`examples/skills/`](examples/skills/):

aiohttp, arrow, beautifulsoup4, boto3, celery, click, cryptography, django, fastapi, flask, httpx, jinja2, keras, matplotlib, numpy, pandas, pillow, pydantic, pytest, pytorch, requests, rich, scikit-learn, scipy, sqlalchemy, transformers, typer, unstructured

## Tips and Model Experience

Generation gets you 90-95% of the way to a good SKILL.md — a validated, well-structured starting point. Quality varies by model. Here's what we've found:

### Model Recommendations

**Best overall**: GPT-5.2 produces the cleanest output with fewest retries. It nailed the `click` library on the first try in under 3 minutes.

**Best value**: The **hybrid setup** (local model for extract/map/learn/create, cloud model for review+test) gives you the best of both worlds. Extraction doesn't need a frontier model — a 14B-30B local model handles it fine. Review and test are where model quality matters most — run those in the cloud for best results.

**Local-only**: Qwen3-Coder (30B) via Ollama works well for small-to-medium libraries but may struggle with complex ones. Expect more retries and occasional truncation from token limits. Totally usable for free, just slower.

**Claude Sonnet**: Strong extraction and synthesis. Works great across the board.

### Benchmarks (Feb 2026)

| Library | Model | Time | Test Result | Retries |
|---------|-------|------|-------------|---------|
| click | GPT-5.2 | ~3 min | 3/3 pass | 1 |
| requests | GPT-5.2 | ~4.5 min | 3/3 pass | 3 |
| scipy | Claude Sonnet + GPT-5.2 (hybrid) | ~3.5 min | 3/3 pass | — |
| pydantic | Claude Sonnet | ~3 min | 3/3 pass | — |
| click | Qwen3-Coder (30B, local) | ~49 min | 2/3 (gave up) | 10 |

### General Tips

- **Start with `--dry-run`** to make sure file collection and language detection work before burning API credits.
- **Use `--version-from git-tag`** when generating from a cloned release tag for accurate version metadata.
- **Increase `max_retries`** for local models (10+) — they sometimes need several passes to get formatting right.
- **Increase `container.timeout`** for libraries with heavy C dependencies (numpy, scipy, pytorch) — pip install takes time.
- **Review the output.** Generation is a starting point, not a finished product. Tweak patterns for your specific needs, remove anything that doesn't look right.
- **The linter runs automatically** after generation and will flag issues like unclosed code blocks, missing sections, or prompt instruction leaks. Security violations (e.g. prompt leaks, destructive commands) cause a hard failure; formatting issues are retried up to `max_retries`.
- **Update mode** (`-i SKILL.md -o SKILL.md`) preserves your manual edits where possible while regenerating from the latest source.

## Language Support

| Language | Status | Notes |
|----------|--------|-------|
| Python | Full support | PyPI, setup.py, pyproject.toml, uv environments, container or bare-metal (`uv` + `python3`) validation |
| Go | Full support | go.mod, Go modules, container (`golang:1.25-alpine`) or bare-metal (`go` toolchain) validation |
| JavaScript/TypeScript | Detection only | package.json detected, ecosystem handler planned |
| Rust | Detection only | Cargo.toml detected, ecosystem handler planned |

Both Python and Go support **bare-metal execution** — run test validation directly on the host without Docker/Podman. Set `execution_mode = "bare-metal"` in `[generation.container]`. This requires the language toolchain installed locally (`uv` + `python3` for Python, `go` for Go).

Full ecosystem handlers for JS/TS and Rust are planned.

## Building from Source

```bash
# Build release binary
cargo build --release

# Run tests (requires uv)
make test

# Generate HTML coverage report
make coverage

# Run linter + formatter + audit
make lint
make fmt-check
cargo audit
```

## Requirements

**Runtime:**
- Rust 1.70+
- Docker or Podman (for test validation)
- An LLM API key (or Ollama for local models)

**External commands** (invoked at runtime via shell):

| Command | Required? | Purpose |
|---------|-----------|---------|
| `git` | Optional | Version detection fallback (`git describe --tags`, `git rev-parse`) when package metadata doesn't contain a version |
| `docker` or `podman` | For test stage | Runs generated test code in isolated containers |
| `uv` | For test stage (local) | Sets up Python environments for local (non-container) validation |
| `python3` | For non-container validation | Direct Python execution when containers aren't available |

**Development / Testing:**
- [uv](https://docs.astral.sh/uv/) — required to run the full test suite (test stage executor tests use `uv` to create isolated Python environments)
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) — for coverage reports (`make coverage` will install it automatically)
- [pre-commit](https://pre-commit.com/) — for git hooks (`pre-commit install && pre-commit install --hook-type pre-push`)

Install dev dependencies:
```bash
# Check everything is available
make check-deps

# Install uv if needed
pip install uv    # or: brew install uv

# Install pre-commit hooks
pre-commit install && pre-commit install --hook-type pre-push
```

## License

[AGPL-3.0](LICENSE) — free to use, modify, and distribute. If you modify the source and distribute it or run it as a service, you must share your changes under the same license.
