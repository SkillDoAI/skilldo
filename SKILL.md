---
name: skilldo
description: Generate SKILL.md agent rules files for software libraries. Use when you need to create, review, or lint SKILL.md documentation for Python, Go, JavaScript, Rust, or Java libraries, or when configuring skilldo.toml files.
license: AGPL-3.0
compatibility: Requires an LLM API key (Anthropic, OpenAI, Gemini, or OpenAI-compatible). Optional container runtime (docker/podman) for test validation.
metadata:
  author: SkillDoAI
  version: "0.5.14"
---

# Skilldo CLI

Skilldo is a 6-stage LLM pipeline that generates SKILL.md files for software libraries:
**extract** → **map** → **learn** → **create** → **review** → **test**

Stages 1-3 gather library metadata (source files, docs, dependencies, version).
Stage 4 generates the SKILL.md. Stage 5 reviews for accuracy/safety. Stage 6 validates
with generated test code.

## Installation

```bash
# Homebrew
brew install skilldoai/tap/skilldo

# From source
cargo install --path .
```

## Quick Start

```bash
# Generate a SKILL.md for the current repo
skilldo generate

# Generate for a specific repo path
skilldo generate /path/to/library

# Specify language explicitly
skilldo generate --language python

# Use a specific config
skilldo generate --config skilldo.toml
```

## Configuration (skilldo.toml)

Skilldo uses TOML config files. Place `skilldo.toml` in the repo root, or pass `--config`.

### Minimal config (single provider)

```toml
[llm]
provider_type = "anthropic"          # anthropic, openai, gemini, openai-compatible, chatgpt
model = "claude-sonnet-4-6"
api_key_env = "ANTHROPIC_API_KEY"  # env var name containing the key

[generation]
max_retries = 10                # retry on lint/test failures
# telemetry = true              # opt-in: log runs to ~/.skilldo/runs.csv
```

### Per-stage model overrides

Different models can be used for review and test stages:

```toml
[llm]
provider_type = "anthropic"
model = "claude-sonnet-4-6"
api_key_env = "ANTHROPIC_API_KEY"

[generation]
enable_test = true
enable_review = true
test_mode = "thorough"          # thorough, adaptive, minimal

[generation.review_llm]
provider_type = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"

[generation.test_llm]
provider_type = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"
```

### Local models (Ollama)

```toml
[llm]
provider_type = "openai-compatible"
model = "qwen3-coder:latest"
base_url = "http://localhost:11434/v1"
api_key_env = "none"            # Ollama doesn't need a key
```

### CLI provider (use existing CLI tools as LLM backend)

```toml
[llm]
provider_type = "cli"
model = "claude-sonnet-4-6"
cli_command = "claude"
cli_args = ["-p", "--no-session-persistence", "--output-format", "text", "--dangerously-skip-permissions"]
request_timeout_secs = 900
```

Supported CLI tools: `claude`, `codex`, `gemini`. The prompt is piped via stdin; response captured from stdout.

### Custom base URL (Bedrock, Vertex AI, proxies)

All providers support optional `base_url` for custom endpoints:

```toml
# Anthropic via AWS Bedrock
[llm]
provider_type = "anthropic"
base_url = "https://bedrock-runtime.us-east-1.amazonaws.com"

# Gemini via Google Vertex AI
[llm]
provider_type = "gemini"
base_url = "https://us-central1-aiplatform.googleapis.com"
```

### Container settings (optional)

```toml
[generation.container]
runtime = "podman"              # docker or podman
timeout = 120                   # seconds
cleanup = true
# setup_commands = ["apt-get update && apt-get install -y cmake"]  # run before deps/tests
```

## Commands

### `skilldo generate [PATH]`
Generate a SKILL.md for a library repository.

Key flags:
- `--language <LANG>` — force language (python, javascript, rust, go, java). Auto-detected if omitted
- `--config <PATH>` — config file path (defaults to `./skilldo.toml` or `~/.config/skilldo/config.toml`)
- `--model <MODEL>` — override LLM model
- `--provider <PROVIDER>` — LLM provider: anthropic, openai, chatgpt, gemini, openai-compatible
- `--base-url <URL>` — base URL for openai-compatible providers
- `-i, --input <PATH>` — existing SKILL.md to use as reference for updates
- `-o <PATH>` — output file (default: SKILL.md)
- `--debug-stage-files <DIR>` — dump each pipeline stage's raw output for debugging
- `--no-test` — skip test validation
- `--no-review` — skip review validation
- `--no-security-scan` — skip YARA/unicode/injection scanning
- `--no-parallel` — run extract/map/learn sequentially (for local models)
- `--best-effort` — exit 0 even with errors
- `--telemetry` / `--no-telemetry` — toggle run logging
- `--container` — run test agent in container (default: bare-metal with uv/cargo)
- `--install-source <MODE>` — how the test agent installs the library for code validation: `registry` (from crates.io/PyPI/npm, default), `local-install` (mounts local repo and builds via package manager), `local-mount` (mounts repo and sets module path directly, no build step)
- `--source-path <PATH>` — local source path for local-install/local-mount modes
- `--test-mode <MODE>` — `thorough` tests 3 patterns (default), `minimal` tests 1, `adaptive` starts with 1 and expands
- `--review-model <MODEL>` / `--review-provider <PROVIDER>` — override model/provider for the review stage only
- `--test-model <MODEL>` / `--test-provider <PROVIDER>` — override model/provider for the test code generation stage only
- `--max-retries <N>` — max create→validate retries before giving up (default: from config)
- `--skill-version <VER>` — force a specific library version in frontmatter (e.g., "2.1.0") instead of auto-detecting
- `--version-from <STRATEGY>` — how to detect version: `package` (Cargo.toml/pyproject.toml), `git-tag`, `branch`, `commit`
- `-q, --quiet` — suppress informational output (only warnings/errors)
- `-v, --verbose` — show detailed debug output (equivalent to RUST_LOG=debug)
- `--request-timeout <SECS>` — override LLM request timeout in seconds (default: 120)
- `--dry-run` — use mock LLM client for testing (no API calls)

### `skilldo lint <PATH>`
Lint a SKILL.md for structural errors (frontmatter, sections, code blocks).

### `skilldo review <PATH>`
LLM-powered review of an existing SKILL.md for accuracy and safety.

Key flags:
- `--config <PATH>` — config with LLM settings

### `skilldo config check --config <PATH>`
Validate a config file for correctness.

### `skilldo auth login|status|logout`
Manage OAuth tokens for providers that use OAuth (e.g., ChatGPT).

## Supported Languages

| Language | Ecosystem | Detection |
|----------|-----------|-----------|
| Python | pip/uv, pyproject.toml, setup.py | `*.py`, `pyproject.toml` |
| JavaScript/TypeScript | npm, package.json | `*.js`, `*.ts`, `package.json` |
| Go | go modules, go.mod | `*.go`, `go.mod` |
| Rust | cargo, Cargo.toml | `*.rs`, `Cargo.toml` |
| Java | Maven/Gradle, pom.xml, build.gradle | `*.java`, `pom.xml`, `build.gradle` |

## Configuration (skilldo.toml)

Key `[generation]` fields:

| Field | Values | Description |
|-------|--------|-------------|
| `language` | `python`, `javascript` (or `typescript`/`ts`/`js`), `rust`, `go`, `java` | Override auto-detection |
| `security_context` | `"api-client"` or omit | Relaxes security scan for API client SDKs that discuss credentials/auth |
| `redact_env_vars` | `["VAR_NAME", ...]` | Env var values masked with `***REDACTED***` in test output/logs |
| `custom_instructions` | `"""..."""` | Repo-specific instructions for the create stage (overrides style/content rules) |
| `enable_test` | `true`/`false` | Toggle test validation (default: true) |
| `enable_review` | `true`/`false` | Toggle review validation (default: true) |
| `test_mode` | `thorough`/`minimal`/`adaptive` | Test 3/1/1+ patterns |
| `max_retries` | integer | Max create→validate retries (default: 10) |

### Model Communication

The model reports uncertainty via HTML comments in the output (stripped before final SKILL.md):
- `<!-- SKILLDO-CONFLICT: description -->` — docs vs code conflicts found
- `<!-- SKILLDO-UNVERIFIED: description -->` — APIs the model couldn't verify from source

View these with `RUST_LOG=info` or `RUST_LOG=debug`. CONFLICT logs at info, UNVERIFIED at warn.

## Common Workflows

### Generate with Anthropic
```bash
source ~/.anthropic
skilldo generate --provider anthropic --model claude-sonnet-4-6
```

### Generate with OpenAI
```bash
source ~/.openai
skilldo generate --provider openai --model gpt-5.2
```

### Generate with local Ollama
```bash
skilldo generate --provider openai-compatible --model qwen3-coder:latest --base-url http://localhost:11434/v1
```

### Batch generation (multiple repos)
```bash
for repo in /path/to/repos/*/; do
  skilldo generate "$repo" --config shared-config.toml --best-effort
done
```

### Generate for an API client SDK
```toml
# skilldo.toml — for libraries that discuss API keys/auth
[generation]
security_context = "api-client"
redact_env_vars = ["MY_API_KEY"]
```
```bash
export MY_API_KEY="..."  # needed for test validation
skilldo generate . --config skilldo.toml
```

### Debug pipeline stages
```bash
skilldo generate . --debug-stage-files ./debug-output/
# Writes: 1-extract.md, 2-map.md, 3-learn.md, 4-create-raw.md, 5-review-*.txt, 6-normalized.md
```

### Review an existing SKILL.md
```bash
skilldo review SKILL.md --config skilldo.toml
```

## Telemetry

When `telemetry = true`, each run appends a row to `~/.skilldo/runs.csv`:
- Timestamp, language, library name, version
- Models and providers used (generate, review, test)
- Pass/fail, retry count, duration, failure details

Data is local only — nothing is sent anywhere.

## Troubleshooting

- **"No test files found"** — a warning, not an error. The pipeline continues without test-derived patterns. Use `--language` to override if detection is wrong.
- **Test failures looping** — try `--test-mode minimal` or `--no-test` for a first pass.
- **Rate limited** — increase `retry_delay` in config, or switch to a local model.
- **OAuth errors** — run `skilldo auth status --config <path>` to check token state.
- **install_source errors on non-Python** — `local-install`/`local-mount` only works for Python. Use default `registry` for other languages.

## Documentation

Full docs — configuration, languages, architecture, authentication, best practices, and telemetry: [docs/](https://github.com/SkillDoAI/skilldo/tree/main/docs)
