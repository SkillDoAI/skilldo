# Skilldo

*Pronounced "skill-do"* — The artificial skill generator.

Skilldo is a Rust CLI that automatically generates `SKILL.md` files for open source libraries. It reads your source code, tests, docs, and changelogs, then uses a multi-agent LLM pipeline to produce structured agent rules that help AI coding assistants (Claude, Cursor, Copilot, etc.) use your library correctly.

The goal: make agent rules a standard part of every open source package — like README.md or .gitignore.

## How It Works

Skilldo clones (or reads) a library's repository and runs a 5-agent pipeline to extract knowledge and synthesize it into a single `SKILL.md` file:

```
Source Code ──→ Agent 1 (API Extraction)     ──┐
Test Files  ──→ Agent 2 (Pattern Extraction) ──┤──→ Agent 4 (Synthesis) ──→ SKILL.md
Docs/README ──→ Agent 3 (Context Extraction) ──┘        ↑                      │
                                                        └── Agent 5 ←──────────┘
                                                        (Code Validation)
```

1. **Collect** — Discovers source files, tests, documentation, and changelogs from the repository
2. **Extract** — Three agents work in parallel to pull out the API surface, usage patterns, and conventions/pitfalls
3. **Synthesize** — A fourth agent combines everything into a formatted SKILL.md
4. **Validate** — A fifth agent generates test code from the patterns and runs it in a container to verify correctness
5. **Iterate** — If validation fails, feedback loops back for regeneration (configurable retries)

The output is a structured Markdown file with YAML frontmatter, ready to drop into any repository.

## The Agents

| Agent | Role | What It Does |
|-------|------|-------------|
| **Agent 1** — Extractor | API Surface | Reads source code to identify public functions, classes, methods, parameters, return types, and deprecations |
| **Agent 2** — Patterns | Usage Patterns | Reads test files to extract real-world usage examples showing how the library is actually used |
| **Agent 3** — Context | Conventions & Pitfalls | Reads docs, README, and changelogs to find common mistakes, migration notes, and best practices |
| **Agent 4** — Synthesizer | SKILL.md Composition | Combines output from Agents 1-3 into a structured SKILL.md with sections for imports, patterns, pitfalls, and more |
| **Agent 5** — Validator | Code Testing | Generates runnable test code from the SKILL.md patterns and executes it in a Docker/Podman container to verify they actually work |

Agent 5 is optional but recommended. When enabled, it catches hallucinated APIs, wrong parameter names, and broken code examples before they ship. If validation fails, Agent 4 regenerates with the error feedback.

> **Note**: A security review agent may be added before Agent 5 in a future release. Agent numbers are subject to change — the roles are what matter.

## Quick Start

```bash
# Build from source
cargo build --release

# Generate a SKILL.md for a Python library
git clone --depth 1 https://github.com/pallets/click.git /tmp/click
skilldo generate /tmp/click --config my-config.toml --output click-SKILL.md
```

You'll need a config file with your LLM provider credentials. See [Configuration](#configuration) below.

## Usage

```
skilldo generate [PATH] [OPTIONS]
```

**Arguments:**

| Argument | Default | Description |
|----------|---------|-------------|
| `PATH` | `.` (current dir) | Path to the library's repository |

**Options:**

| Option | Description |
|--------|-------------|
| `--language <LANG>` | Force language detection (`python`, `javascript`, `rust`, `go`). Auto-detected if omitted. |
| `-o, --output <PATH>` | Output file path. Default: `SKILL.md` |
| `-i, --input <PATH>` | Existing SKILL.md to update (enables update mode) |
| `--version <VER>` | Explicit version override, e.g. `"2.1.0"` |
| `--version-from <STRATEGY>` | Version extraction strategy: `git-tag`, `package`, `branch`, `commit` |
| `--config <PATH>` | Path to config file |
| `--dry-run` | Use a mock LLM client (no API key needed, for testing) |

### Examples

```bash
# Basic generation with auto-detection
skilldo generate /path/to/repo

# Force Python, custom output path
skilldo generate /path/to/repo --language python -o rules/SKILL.md

# Update an existing SKILL.md (reads it, regenerates, writes back)
skilldo generate /path/to/repo -i SKILL.md -o SKILL.md

# Extract version from git tags instead of package files
skilldo generate /path/to/repo --version-from git-tag

# Dry run to test without an LLM
skilldo generate /path/to/repo --dry-run
```

## Configuration

Skilldo uses a TOML config file. It searches for config in this order:
1. `--config <path>` (explicit CLI argument)
2. `./skilldo.toml` (repository root)
3. `~/.config/skilldo/config.toml` (user config directory)

### Minimal Config (Get Started Fast)

If you have an OpenAI API key, this is all you need:

```toml
[llm]
provider = "openai"
model = "gpt-5.2"
# Name of the environment variable holding your API key.
# Skilldo reads the key from this env var at runtime — it never stores keys in config.
api_key_env = "OPENAI_API_KEY"

[generation]
max_retries = 5
max_source_tokens = 100000
```

Set your API key before running: `export OPENAI_API_KEY="sk-your-key-here"`

This uses GPT-5.2 for all agents, with Agent 5 validation enabled by default using Docker.

### Full Documented Config

```toml
# ── LLM Provider ──────────────────────────────────────────────
# Configures the model used for Agents 1-4 (and Agent 5 if no
# separate agent5_llm is specified).
[llm]
# Provider: "anthropic", "openai", "gemini", or "openai-compatible"
provider = "anthropic"

# Model name (provider-specific)
model = "claude-sonnet-4-5-20250929"

# Environment variable containing the API key.
# Set to "none" for local models (Ollama) that don't need a key.
api_key_env = "ANTHROPIC_API_KEY"

# Base URL — only needed for openai-compatible providers
# base_url = "http://localhost:11434/v1"

# Override max output tokens per LLM request.
# Defaults: anthropic=4096, openai=4096, openai-compatible=16384, gemini=8192
# max_tokens = 8192

# ── Generation Settings ───────────────────────────────────────
[generation]
# Max retry attempts for the generate→validate loop (default: 5)
max_retries = 5

# Approximate token budget for source code sent to agents (default: 100000)
max_source_tokens = 100000

# Enable Agent 5 code validation in containers (default: true)
enable_agent5 = true

# Agent 5 validation mode (default: "thorough")
#   "thorough"  — test every extracted pattern
#   "adaptive"  — test patterns, reduce scope on repeated failures
#   "minimal"   — test only core import + one pattern
agent5_mode = "thorough"

# ── Agent 5 LLM Override (Optional) ──────────────────────────
# Use a different (typically stronger) model for Agent 5 code generation.
# This is the "hybrid" setup — cheap local model for extraction, strong
# cloud model for validation.
#
# [generation.agent5_llm]
# provider = "openai"
# model = "gpt-5.2"
# api_key_env = "OPENAI_API_KEY"

# ── Container Settings ────────────────────────────────────────
# Agent 5 runs generated test code inside containers for safety.
[generation.container]
# Container runtime: "docker" or "podman" (default: "docker")
runtime = "docker"

# Timeout for container execution in seconds (default: 60)
# Increase for libraries with heavy dependencies (numpy, pytorch, etc.)
timeout = 60

# Auto-remove containers after execution (default: true)
cleanup = true

# Container images (override if you need specific versions)
# python_image = "ghcr.io/astral-sh/uv:python3.11-bookworm-slim"
# javascript_image = "node:20-slim"
# rust_image = "rust:1.75-slim"
# go_image = "golang:1.21-alpine"

# ── Custom Prompts (Advanced) ────────────────────────────────
# Override or extend the built-in agent prompts.
# Mode per agent: "append" (default) adds your text after the built-in
# prompt, "overwrite" replaces it entirely. Agent 5 is always append-only.
#
# [prompts]
# override_prompts = false        # Global default: false = append
# agent1_mode = "append"          # Per-agent override
# agent1_custom = "Also extract all class methods that start with 'get_'"
# agent4_mode = "overwrite"
# agent4_custom = "Your entirely custom Agent 4 prompt here..."
```

### Supported Providers

| Provider | Config `provider` | Needs API Key | Notes |
|----------|-------------------|---------------|-------|
| **Anthropic** | `"anthropic"` | Yes (`ANTHROPIC_API_KEY`) | Claude models |
| **OpenAI** | `"openai"` | Yes (`OPENAI_API_KEY`) | GPT models. Handles `max_completion_tokens` for GPT-5+. |
| **Google Gemini** | `"gemini"` | Yes (`GEMINI_API_KEY`) | Gemini models |
| **OpenAI-compatible** | `"openai-compatible"` | Varies | Ollama, DeepSeek, Groq, Together, Fireworks, xAI, Mistral, vLLM, etc. Set `base_url`. |

### Example Config Files

Ready-to-use configs for common setups:

- [`examples/configs/anthropic.toml`](examples/configs/anthropic.toml) — Claude Sonnet (cloud)
- [`examples/configs/openai.toml`](examples/configs/openai.toml) — GPT-5.2 (cloud)
- [`examples/configs/ollama.toml`](examples/configs/ollama.toml) — Qwen3-Coder via Ollama (local, no API key)
- [`examples/configs/hybrid.toml`](examples/configs/hybrid.toml) — Local extraction + cloud validation (best of both worlds)
- [`examples/configs/deepseek.toml`](examples/configs/deepseek.toml) — DeepSeek (OpenAI-compatible cloud)

## Example Output

We've generated SKILL.md files for the **top 25 Python libraries** and counting. Browse them all in [`examples/skills/`](examples/skills/):

aiohttp, arrow, boto3, celery, click, django, fastapi, flask, httpx, jinja2, keras, matplotlib, numpy, pandas, pillow, pydantic, pytest, pytorch, requests, rich, scikit-learn, scipy, sqlalchemy, transformers, typer

## Tips and Model Experience

Generation gets you 90-95% of the way to a good SKILL.md — a validated, well-structured starting point. Quality varies by model. Here's what we've found:

### Model Recommendations

**Best overall**: GPT-5.2 produces the cleanest output with fewest retries. It nailed the `click` library on the first try in under 3 minutes.

**Best value**: The **hybrid setup** (local model for Agents 1-4, cloud model for Agent 5) gives you the best of both worlds. Extraction doesn't need a frontier model — a 14B-30B local model handles it fine. Code validation is where model quality matters most.

**Local-only**: Qwen3-Coder (30B) via Ollama works well for small-to-medium libraries but may struggle with complex ones. Expect more retries and occasional truncation from token limits. Totally usable for free, just slower.

**Claude Sonnet**: Strong extraction and synthesis. Works great across the board.

### Benchmarks (Feb 2026)

| Library | Model | Time | Agent 5 Result | Retries |
|---------|-------|------|-----------------|---------|
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
- **The linter runs automatically** after generation and will flag issues like unclosed code blocks, missing sections, or prompt instruction leaks. These are hints, not hard failures.
- **Update mode** (`-i SKILL.md -o SKILL.md`) preserves your manual edits where possible while regenerating from the latest source.

## Language Support

| Language | Status | Notes |
|----------|--------|-------|
| Python | Full support | PyPI, setup.py, pyproject.toml, uv environments |
| JavaScript/TypeScript | Detected, no ecosystem handler | package.json detection works, generation pipeline not yet specialized |
| Rust | Detected, no ecosystem handler | Cargo.toml detection works |
| Go | Detected, no ecosystem handler | go.mod detection works |

Full ecosystem handlers for JS/TS, Rust, and Go are planned.

## Building from Source

```bash
# Build release binary
cargo build --release

# Run tests (~755 passing, requires uv)
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
- Docker or Podman (for Agent 5 validation)
- An LLM API key (or Ollama for local models)

**Development / Testing:**
- [uv](https://docs.astral.sh/uv/) — required to run the full test suite (Agent 5 executor tests use `uv` to create isolated Python environments)
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
