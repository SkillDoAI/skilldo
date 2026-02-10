# BUILD.md — Skilldo

**Last Updated**: 2026-02-06
**Current Status**: v0.1 release candidate
**Tests**: 783 passing, 0 failing (1 ignored — requires network)
**Branch**: `initial` (dev branch)
**Binary**: 7.1MB release

## What This Is

A Rust CLI that automatically generates `SKILL.md` files for open source libraries. It reads source code, tests, docs, and changelogs, then uses a 5-agent LLM pipeline to produce structured agent rules that help AI coding assistants (Claude, Cursor, Copilot, etc.) use libraries correctly.

The goal: make agent rules a standard part of every open source package — like README.md or .gitignore.

## Why Rust

- Single binary, zero runtime dependencies
- Fast enough for CI on every release
- Cross-compiles for Linux/macOS/Windows

## Architecture

### Source Layout

```
skilldo/
├── src/
│   ├── main.rs                # CLI entry point
│   ├── lib.rs                 # Library exports
│   ├── config.rs              # TOML config parsing
│   ├── detector.rs            # Language detection
│   ├── changelog.rs           # Version/changelog analysis
│   ├── lint.rs                # SKILL.md format linter
│   ├── validator.rs           # Functional code validator
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── generate.rs        # `skilldo generate` command
│   │   └── version.rs         # Version extraction strategies
│   ├── llm/
│   │   ├── mod.rs
│   │   ├── client.rs          # LlmClient trait + MockLlmClient
│   │   ├── client_impl.rs     # Anthropic, OpenAI, Gemini clients
│   │   ├── factory.rs         # Client creation factory
│   │   ├── agents.rs          # Agent definitions and orchestration
│   │   └── prompts_v2.rs      # Prompt templates (v2, current)
│   ├── pipeline/
│   │   ├── mod.rs
│   │   ├── collector.rs       # Source/test/doc file gathering
│   │   ├── generator.rs       # Multi-agent generation + retry loop
│   │   └── normalizer.rs      # Post-processing (frontmatter, refs)
│   ├── ecosystems/
│   │   ├── mod.rs
│   │   └── python.rs          # Python ecosystem handler
│   ├── agent5/
│   │   ├── mod.rs
│   │   ├── parser.rs          # SKILL.md pattern extraction
│   │   ├── code_generator.rs  # LLM-powered test code generation
│   │   ├── executor.rs        # Test execution orchestration
│   │   ├── container_executor.rs  # Docker/Podman container runner
│   │   └── validator.rs       # Result validation
│   ├── bot/                   # GitHub bot (stub, Phase 2)
│   └── git/                   # Git operations (stub, Phase 2)
├── tests/                     # 26 test files, 783+ tests
├── examples/
│   ├── configs/               # 5 example configs (anthropic, openai, ollama, hybrid, deepseek)
│   └── skills/                # 13 generated SKILL.md examples
├── dev/
│   ├── configs/               # Working test configs (gitignored)
│   └── scripts/               # Dev/test scripts
├── Cargo.toml
├── Makefile
├── BUILD.md                   # This file
├── BACKLOG.md                 # Deferred work items
├── CLAUDE.md                  # AI assistant guidelines
├── README.md                  # User-facing docs
└── LICENSE                    # MIT
```

### 5-Agent Pipeline

```
Source Code ──→ Agent 1 (API Extraction)     ──┐
Test Files  ──→ Agent 2 (Pattern Extraction) ──┤──→ Agent 4 (Synthesis) ──→ SKILL.md
Docs/README ──→ Agent 3 (Context Extraction) ──┘        ↑                      │
                                                        └── Agent 5 ←──────────┘
                                                        (Code Validation)
```

1. **Agent 1**: Extracts public API surface from source code
2. **Agent 2**: Extracts usage patterns from test files
3. **Agent 3**: Extracts conventions, pitfalls, breaking changes from docs
4. **Agent 4**: Synthesizes everything into a SKILL.md
5. **Agent 5**: Generates and runs test code in containers to validate patterns

If Agent 5 fails, feedback loops back to Agent 4 for regeneration (max retries configurable).

### Validation Stack

Three layers of validation before output:
1. **Format validation** (SkillLinter): Structure, frontmatter, sections
2. **Functional validation**: Code blocks are syntactically valid
3. **Agent 5** (meta-validation): Generated test code actually runs in Docker/Podman

## LLM Providers

Hand-rolled clients covering all major providers:

| Provider | Client | Notes |
|----------|--------|-------|
| Anthropic | Native | Claude models, `max_tokens` |
| OpenAI | Native | GPT models, handles `max_completion_tokens` for GPT-5+ |
| Gemini | Native | Google models |
| OpenAI-compatible | Generic | Ollama, DeepSeek, Groq, Together, Fireworks, xAI, Mistral, vLLM |

Hybrid configs supported — use different models for different agents (e.g., cheap local for 1-4, strong cloud for Agent 5).

### E2E Verified Providers

| Provider | Model | Library | Agent 5 | Time |
|----------|-------|---------|---------|------|
| Ollama | qwen3-coder (30B) | click | 2/3 | ~49 min |
| OpenAI | gpt-5.2 | click | 3/3 | ~3 min |
| OpenAI | gpt-5.2 | requests | 3/3 | ~4.5 min |
| Anthropic | claude-sonnet-4-5 + gpt-5.2 | scipy | 3/3 | ~3.5 min |
| Anthropic | claude-haiku-4-5 | arrow | format fail | ~2 min |

## Language Support

### Implemented
- **Python**: Full support (PyPI, setup.py, pyproject.toml, uv environments)

### Detector Ready (enum + detection, no ecosystem handler yet)
- JavaScript/TypeScript (package.json)
- Rust (Cargo.toml)
- Go (go.mod)

### Planned
- C/C++ (CMakeLists.txt, Makefile, conan, vcpkg)
- Java (pom.xml, build.gradle)
- Other ecosystems with published library packages

## CLI Usage

```bash
# Generate SKILL.md from a local repo
skilldo generate /path/to/repo --language python --output SKILL.md

# Use a config file
skilldo generate /path/to/repo --config my-config.toml

# Update existing SKILL.md (reads current, regenerates)
skilldo generate /path/to/repo -i SKILL.md -o SKILL.md

# Specify version extraction strategy
skilldo generate /path/to/repo --version-from pyproject
```

## Configuration

TOML config file (see `examples/configs/` for full examples):

```toml
[llm]
provider = "anthropic"          # anthropic | openai | gemini | openai-compatible
model = "claude-sonnet-4-5-20250929"
api_key_env = "ANTHROPIC_API_KEY"

[generation]
max_retries = 5
max_source_tokens = 100000
enable_agent5 = true
agent5_mode = "thorough"        # thorough | minimal | adaptive

# Optional: different model for Agent 5
[generation.agent5_llm]
provider = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"

[generation.container]
runtime = "docker"              # docker | podman
timeout = 1800
```

## Building

```bash
cargo build --release           # 7.1MB binary
cargo test                      # 783+ tests (requires uv)
```

## Dependencies

```toml
clap = "4"          # CLI
reqwest = "0.12"    # HTTP (LLM APIs)
serde = "1"         # Serialization
tokio = "1"         # Async runtime
octocrab = "0.41"   # GitHub API (Phase 2)
git2 = "0.19"       # Git operations
handlebars = "6"    # Templates
toml = "0.8"        # Config
regex = "1"         # Pattern matching
anyhow = "1"        # Error handling
tracing = "0.1"     # Logging
```

## License

MIT
