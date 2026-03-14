---
name: skilldo
description: Generate SKILL.md agent rules files for software libraries. Use when you need to create, review, or lint SKILL.md documentation for Python, Go, JavaScript, or Rust libraries, or when configuring skilldo.toml files.
license: AGPL-3.0
compatibility: Requires an LLM API key (Anthropic, OpenAI, Gemini, or OpenAI-compatible). Optional container runtime (docker/podman) for test validation.
metadata:
  author: SkillDoAI
  version: "0.4.2"
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
max_retries = 5                 # retry on lint/test failures
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

### Container settings (optional)

```toml
[generation.container]
runtime = "podman"              # docker or podman
timeout = 120                   # seconds
cleanup = true
```

## Commands

### `skilldo generate [PATH]`
Generate a SKILL.md for a library repository.

Key flags:
- `--language <LANG>` — force language (python, javascript, rust, go)
- `--config <PATH>` — config file path
- `--model <MODEL>` — override LLM model
- `--no-test` — skip test validation
- `--no-review` — skip review validation
- `--no-security-scan` — skip YARA/unicode/injection scanning
- `--best-effort` — exit 0 even with errors
- `--telemetry` / `--no-telemetry` — toggle run logging
- `--container` — run test agent in container (default: bare-metal)
- `-o <PATH>` — output file (default: SKILL.md)

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

- **"No tests found"** — language detection may have failed. Use `--language` explicitly.
- **Test failures looping** — try `--test-mode minimal` or `--no-test` for a first pass.
- **Rate limited** — increase `retry_delay` in config, or switch to a local model.
- **OAuth errors** — run `skilldo auth status --config <path>` to check token state.
- **install_source errors on non-Python** — `local-install`/`local-mount` only works for Python. Use default `registry` for other languages.
