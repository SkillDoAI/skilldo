# Backlog

Items identified during development. Not prioritized yet.

## Per-Agent Model Config

Allow each agent (1-4) to use a different LLM model. Already plumbed for Agent 5 via `agent5_llm` in config. Extend to `agent1_llm`, `agent2_llm`, etc.

**Why**: Different agents have different needs. Agent 1 (API extraction) benefits from large context; Agent 4 (synthesis) benefits from strong instruction following. Agent 5 already uses a separate model for code generation.

**Status**: Config structure exists for agent5_llm. Would need to add agent1-4_llm fields and pass separate clients through the generator pipeline.

## Agent Naming

Rename agents from numbers to descriptive names reflecting their role:
- Agent 1 -> Extractor (API surface extraction)
- Agent 2 -> Patterns (usage pattern extraction)
- Agent 3 -> Context (conventions/pitfalls extraction)
- Agent 4 -> Synthesizer (SKILL.md composition)
- Agent 5 -> Validator (code generation + execution testing)

**Why**: Better log readability, clearer documentation, easier onboarding.

**Note**: User is partial to keeping "Agent 5" as-is since it's special.

## SKILL.md Merging

Merge multiple SKILL.md files for related libraries (e.g., `requests` + `urllib3`, or `pandas` + `numpy`).

**Why**: Some libraries are commonly used together. A merged skill would help AI agents understand the interaction patterns.

**Status**: Not started. Would need a merge strategy (combine sections, deduplicate patterns, resolve conflicts).

## LLM Client Abstraction (genai)

Evaluated `genai` crate (v0.5.3) — blocked by issue #139 (GPT-5.2 `max_completion_tokens` not supported). Our hand-rolled clients handle this correctly. Revisit when genai 0.6.0 stabilizes.

**Status**: Researched, plan written, deferred. See [genai #139](https://github.com/jeremychone/rust-genai/issues/139).

## CLI/Config Symmetry

Currently CLI args and config file options don't overlap. For CI/CD and bot usage, everything should be configurable from either place with CLI overriding config.

**Add to config**: output, input, language, version_from
**Add to CLI**: --max-retries, --model, --no-agent5, --agent5-mode

**Principle**: Config file is the full spec, CLI args are overrides.

**Status**: Not started. Get user feedback on v0.1 first.

## Documentation: Model Quality Expectations (YMMV)

Add user-facing docs that set expectations around SKILL.md quality:

- Generation gets you 90-95% of the way — a validated, well-structured starting point
- Quality varies by model: gpt-5.2 nailed click first try (3 min); qwen3-coder couldn't fix one pattern in 10 retries (49 min)
- The `agent5_llm` split lets you use a cheap local model for extraction (agents 1-4) and a stronger cloud model for validation (Agent 5)
- Users should review generated SKILL.md files and tweak patterns for their specific needs
- Just because it generates doesn't mean it's 100% right — but it's a lot closer than starting from scratch

**E2E benchmark data (Feb 2026)**:

| Library | Model | Time | Agent 5 | Retries |
|---------|-------|------|---------|---------|
| requests | qwen3-coder (30B, local) | ~17 min | 3/3 | 3 |
| requests | gpt-5.2 (cloud) | ~4.5 min | 3/3 | 3 |
| click | qwen3-coder (30B, local) | ~49 min | 2/3 (gave up) | 10 |
| click | gpt-5.2 (cloud) | ~3 min | 3/3 | 1 |

## Multi-Language Agent 5

Extend Agent 5 code generation validation beyond Python:
- JavaScript/TypeScript (node container)
- Rust (cargo container)
- Go (go container)

Container executor already has stubs for these languages.

## Language Ecosystem Support

New ecosystem handlers needed for each language. Each requires: file discovery (source, tests, docs), dependency detection, and Agent 5 container support.

**High priority** (have library package ecosystems):
- JavaScript/TypeScript — npm/yarn, `package.json`, huge ecosystem
- Go — go modules, `go.mod`, strong library culture
- Rust — crates.io, `Cargo.toml`, our own language

**Medium priority**:
- Java — Maven/Gradle, `pom.xml`/`build.gradle`, massive enterprise ecosystem
- C# — NuGet, `*.csproj`, large enterprise user base

**Lower priority** (libraries exist but packaging is less standardized):
- C/C++ — conan, vcpkg, CMake. Detection harder (CMakeLists.txt, Makefile, configure). Lots of header-only libraries.
- Ruby — RubyGems, `Gemfile`
- PHP — Composer, `composer.json`
- Swift — SwiftPM, `Package.swift`
- Kotlin — Maven/Gradle (shares with Java)

**Unlikely for v1**:
- Lisp/Clojure — small ecosystems, niche
- Elixir/Erlang — Hex packages, small but dedicated community
- Haskell — Hackage/Stack, niche

**Pattern**: Detector already has enum entries for Python, JavaScript, Rust, Go. Each new language needs: `ecosystems/{lang}.rs` handler + `detector.rs` detection entry + Agent 5 container support.

## GitHub Bot (Phase 2)

Webhook listener for `release.published` events. Auto-generates SKILL.md and submits PR. Stubs exist in `src/bot/` and `src/git/`.

## CI/CD GitHub Action (Phase 2)

Publish a GitHub Action so maintainers can add SKILL.md generation to their release workflow without the bot.

## Security Linter for Generated SKILL.md Content

Two-layer approach:
1. **Lightweight regex scan** between every agent step — catches obvious injection attempts (shell commands, URLs, encoded payloads)
2. **LLM adversarial review** once before code execution — "is this code safe to run?"

Possible pipeline restructuring: current Agent 5 becomes Agent 6, new Agent 5 is the security agent.

**Source**: Discussed Feb 2026. User wants to ensure nobody can inject malicious code into the pipeline.

## Switch to tokio::process

Replace `std::thread` + `mpsc::channel` timeout pattern with `tokio::process::Command` + `tokio::time::timeout`. Benefits:
- `kill_on_drop(true)` eliminates manual PID cleanup
- Non-blocking I/O scales better for parallel agent runs
- Simpler code (no channel boilerplate)
- Already have `#[tokio::main]`, so the runtime is there

Currently using a shared `run_cmd_with_timeout` in `src/util.rs` that uses the thread pattern. Would replace with an async version.

**Source**: Gemini audit Feb 2026. Low priority — current pattern works and is DRY.

## Non-Root Container Execution

Run containers with `--user nobody` or similar to minimize risk from container escape. Currently runs as root inside containers.

**Source**: Gemini audit Feb 2026.

## Multi-Skill Agent 5 Testing

Libraries like boto3 need mock frameworks (e.g., moto) for Agent 5 validation. Requires teaching Agent 5 to install test dependencies alongside the target library.

**Source**: Identified during batch generation Feb 2026.

## Type-Safe Config Enums

Replace string-based provider matching (`"openai"`, `"anthropic"`) with Rust enums. Eliminates typo-based bugs and enables exhaustive match checking.

**Source**: Gemini audit Feb 2026.

## ~~DRY: File Priority Logic~~ (DONE)

Extracted to `util::calculate_file_priority()`. Both `python.rs` and `collector.rs` now delegate to it. Uses `Path::components()` for separator-agnostic matching.

## Process Tree Cleanup on Timeout

`util::kill_process()` kills only the direct child PID, not its descendant process tree. In the non-container path (`uv` executor), if a child process spawns grandchildren, they can outlive the timeout. The container path is unaffected — killing the container kills everything inside.

**Fix**: Use `setsid` to create a process group, then `kill -<pgid>` to kill the whole tree. Alternatively, switch to `tokio::process::Command` with `kill_on_drop(true)` (see "Switch to tokio::process" above).

**Impact**: Low — the uv executor runs short-lived Python scripts that rarely spawn subprocesses.

**Source**: GPT audit Feb 2026.

## OAuth Authentication for LLM Providers

Allow users to authenticate with their existing Google/OpenAI/Anthropic accounts via OAuth instead of raw API keys. Would open a browser for the OAuth flow, store refresh tokens locally, and handle token refresh automatically.

**Potential providers**:
- Google (Gemini) — OAuth2 with Google Cloud credentials
- OpenAI — OAuth via ChatGPT account
- Anthropic — OAuth via Console account

**Caveats**: Using consumer accounts for programmatic API access may violate provider ToS. Users assume that risk. Document clearly.

**Implementation**: Add an `auth_method` config field (`api_key` | `oauth`). OAuth flow would use a local redirect server for the callback. Store tokens in `~/.config/skilldo/tokens.json` (encrypted or OS keychain).

**Priority**: Low — API keys work fine for v0.1. Nice-to-have for accessibility.

## Defense-in-Depth: Dual Sanitization

Gemini audit finding #5 suggested moving `sanitize_dep_name` from the executor to the parser (earlier in the pipeline). The recommendation is to do **both**: validate at ingestion (parser.rs) so `CollectedData` carries clean strings, AND keep the executor-level sanitization as a last line of defense before shell execution. Moving it exclusively to the parser removes the safety net at the point of highest risk.

**Implementation**: Add validation in `parser.rs` during extraction. Keep existing `sanitize_dep_name` in executor unchanged.

**Source**: Gemini audit Feb 2026. Disagreement on approach — defense-in-depth preferred over single-point sanitization.

## git2 Unsoundness Advisory (RUSTSEC-2026-0008)

`git2` v0.19.0 has an unsoundness warning for `Buf` struct dereferencing. No fix available yet. Monitor for a patched release.

**Status**: cargo audit warning, no fix available as of Feb 2026.
