# Skilldo

[![CI](https://github.com/SkillDoAI/skilldo/actions/workflows/ci.yml/badge.svg)](https://github.com/SkillDoAI/skilldo/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/SkillDoAI/skilldo/graph/badge.svg)](https://codecov.io/gh/SkillDoAI/skilldo)

*Pronounced "skill-do"* — The artificial skill generator.

Skilldo automatically generates `SKILL.md` agent rules files for open-source libraries. Point it at any repo and get structured rules that help AI coding assistants (Claude, Cursor, Copilot, Codex, etc.) use the library correctly — with validated, tested code examples.

The goal: make agent rules a standard part of every open-source package — like README.md or .gitignore.

## Why Skilldo?

- **Validated code examples** — Generated patterns are executed locally to catch hallucinated APIs before they ship
- **3-layer security scanning** — Regex patterns + prompt injection detection + 41 YARA rules catch malicious content
- **Multi-provider** — Anthropic, OpenAI, Google Gemini, ChatGPT (OAuth), any OpenAI-compatible endpoint (Ollama, DeepSeek, Groq, etc.), or CLI provider mode (Claude CLI, Codex CLI, Gemini CLI)
- **Per-stage model mixing** — Cheap local model for extraction, frontier cloud model for review. One config, multiple providers.
- **Python, Go, JavaScript/TypeScript, Rust, and Java** — Full pipeline support with language-specific parsers and test validation
- **Free with local models** — Run the entire pipeline on Ollama with zero API cost
- **Zero global pollution** — Test validation runs in isolated temp directories; nothing installed to your system

## Install

```bash
# macOS / Linux via Homebrew
brew install skilldoai/tap/skilldo

# Or build from source
cargo build --release
```

Binaries for macOS (ARM) and Linux (x86_64, ARM) are also available on the [Releases](https://github.com/SkillDoAI/skilldo/releases) page.

## Quick Start

```bash
# Set your API key
export OPENAI_API_KEY="sk-your-key-here"

# Generate a SKILL.md for any library
git clone --depth 1 https://github.com/pallets/click.git /tmp/click
skilldo generate /tmp/click --provider openai --model gpt-5.2

# Or use a config file (recommended)
skilldo generate /tmp/click --config my-config.toml -o click-SKILL.md

# With local Ollama (free, no API key)
skilldo generate /tmp/click --provider openai-compatible --model qwen3-coder:latest \
  --base-url http://localhost:11434/v1 --no-parallel
```

That's it. Skilldo reads the source, runs a 6-agent pipeline, validates the output, and writes a `SKILL.md`.

```text
Source Code ──→ Extract (API Surface)       ──┐
Test Files  ──→ Map (Pattern Extraction)    ──┤──→ Create ──→ Review ──→ Test ──→ SKILL.md
Docs/README ──→ Learn (Context Extraction)  ──┘      ↑          ↓         ↓
                                                     │       failed?   failed?
                                                     │          ↓         ↓
                                                     ←── feedback ←───────┘
```

Three agents gather information from the source code in parallel, then Create combines their output into a SKILL.md. Review and Test validate the result — if either fails, error feedback loops back to Create for regeneration.

## More Commands

```bash
# Lint a SKILL.md for structural/security issues
skilldo lint SKILL.md

# Review an existing SKILL.md with LLM-powered accuracy checking
skilldo review SKILL.md --config my-config.toml

# Validate your config before burning API credits
skilldo config check --config my-config.toml

# Smoke-test your LLM provider
skilldo hello-world --config my-config.toml

# Print the embedded skilldo SKILL.md (for AI assistants)
skilldo skill
```

## Documentation

Skilldo is more powerful than the quick start above suggests. Dive deeper:

- **[Architecture](docs/architecture.md)** — How the 6-stage pipeline works, what each agent does, security scanning details
- **[Configuration](docs/configuration.md)** — Full config reference, all TOML fields, per-stage model overrides, CLI provider mode, example configs
- **[Authentication](docs/authentication.md)** — OAuth 2.0 + PKCE setup, API keys, Google credentials, per-stage auth
- **[Languages](docs/languages.md)** — Supported ecosystems, test validation details, how to add a new language
- **[Telemetry](docs/telemetry.md)** — What we log, what we don't, schema details, how to query your data
- **[Best Practices](docs/best-practices.md)** — SKILL.md authoring guidelines and quality tips
- **[SKILL.md](SKILL.md)** — Agent skill for the skilldo CLI itself (use with Claude, Copilot, Codex)

## Example Output

We've generated SKILL.md files for **28+ libraries** and counting. Browse them in [`examples/skills/`](examples/skills/):

aiohttp, arrow, beautifulsoup4, boto3, celery, click, cryptography, django, fastapi, flask, httpx, jinja2, keras, matplotlib, numpy, pandas, pillow, pydantic, pytest, pytorch, requests, rich, scikit-learn, scipy, sqlalchemy, transformers, typer, unstructured

## Tips

- **Iterate fast with `--replay-from`** — after a `--debug-stage-files` run, use `--replay-from <DIR>` to skip extract/map/learn and re-run only create/review/test (~5 min vs ~15 min). Great for prompt tuning.
- **Start with `--dry-run`** to verify file collection and language detection before burning API credits
- **Use a config file** for repeated runs — it supports per-stage model overrides, OAuth, custom headers, and prompt customization that CLI flags can't express
- **Best overall model**: GPT-5.2 produces the cleanest output with fewest retries
- **Best value**: Hybrid setup — local model for extract/map/learn, cloud model for review+test
- **Local-only**: Qwen3-Coder (30B) via Ollama works well for small-to-medium libraries. Increase `max_retries` to 10+.
- **Test modes**: `test_mode = "thorough"` (default) tests ALL patterns for maximum accuracy. Use `test_mode = "quick"` (2-3 patterns) for faster iteration. See [configuration docs](docs/configuration.md).
- **CLI providers**: Set `cli_system_args = ["--system-prompt"]` in your config to pass system prompts correctly for CLI-mode providers (Claude CLI, Codex CLI, etc.). See [configuration docs](docs/configuration.md).
- **Review the output.** Generation is a starting point, not a finished product.

## Building from Source

```bash
cargo build --release
make test          # Run tests (requires uv)
make coverage      # HTML coverage report
make lint          # Clippy + formatter check
```

**Runtime requirements**: An LLM API key (or Ollama). For test validation: `uv`+`python3` (Python), `go` 1.21+ (Go), `node` 18+ (JS/TS), `cargo` (Rust), or `javac`+`java` (Java). Pass `--container` to use Docker/Podman instead.

## License

[AGPL-3.0](LICENSE) — free to use, modify, and distribute. If you modify the source and distribute it or run it as a service, you must share your changes under the same license.
