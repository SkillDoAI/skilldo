# Backlog

Items identified during development. Not prioritized yet.

Pipeline: **extract** → **map** → **learn** → **create** → **review** → **test**

## Review Agent: Grounded Verification

Review agent (`src/review/mod.rs`) runs a two-phase pipeline: LLM-generated introspection script in a container, then LLM verdict. Introspection already runs `pip show` and `inspect.signature()` to verify claims. Remaining gaps:

- PyPI / crates.io API — verify version numbers exist
- Date/calendar validation — compute day-of-week for dates in examples
- Docs URL fetching — verify ## References links are live

**Status**: Phase A (container introspection) done. Phase B gaps above are best handled by teaching the introspection prompt to include these checks, not by adding Rust code.

## SKILL.md Merging

Merge multiple SKILL.md files for related libraries (e.g., `requests` + `urllib3`, or `pandas` + `numpy`).

**Why**: Some libraries are commonly used together. A merged skill would help AI agents understand the interaction patterns.

**Status**: Not started. Would need a merge strategy (combine sections, deduplicate patterns, resolve conflicts).

## LLM Client Abstraction (genai)

Evaluated `genai` crate (v0.5.3) — blocked by issue #139 (GPT-5.2 `max_completion_tokens` not supported). Our hand-rolled clients handle this correctly. Revisit when genai 0.6.0 stabilizes.

**Status**: Researched, plan written, deferred. Last checked March 2026 — issue #139 still open ("In-discussion", "In-queue"). No fix released. See [genai #139](https://github.com/jeremychone/rust-genai/issues/139).

## CLI/Config Symmetry

CLI args and config file options should fully overlap. For CI/CD and bot usage, everything configurable from either place with CLI overriding config.

**Added in v0.1.7**: output, input, language (config fields + CLI override chain)
**Remaining**: version_from (config), --max-retries (CLI)

**Principle**: Config file is the full spec, CLI args are overrides.

## Documentation: Model Quality Expectations

Add user-facing docs that set expectations around SKILL.md quality:

- Generation gets you 90-95% of the way — a validated, well-structured starting point
- Quality varies by model (frontier models nail it first try; local models may need retries)
- The `test_llm` split lets you use a cheap local model for extract/map/learn/create and a stronger cloud model for test validation
- Users should review generated SKILL.md files and tweak patterns for their specific needs

## Multi-Language Test Agent

Extend test agent code generation validation beyond Python:
- JavaScript/TypeScript (node container)
- Rust (cargo container)
- Go (go container)

Container executor already has stubs for these languages.

## Language Ecosystem Support

New ecosystem handlers needed for each language. Each requires: file discovery (source, tests, docs), dependency detection, and test agent container support.

**High priority**:
- JavaScript/TypeScript — npm/yarn, `package.json`, huge ecosystem
- Go — go modules, `go.mod`, strong library culture
- Rust — crates.io, `Cargo.toml`, our own language

**Medium priority**:
- Java — Maven/Gradle, `pom.xml`/`build.gradle`
- C# — NuGet, `*.csproj`

**Lower priority**:
- C/C++ — conan, vcpkg, CMake
- Ruby — RubyGems, `Gemfile`
- PHP — Composer, `composer.json`
- Swift — SwiftPM, `Package.swift`
- Kotlin — Maven/Gradle (shares with Java)

**Pattern**: Detector already has enum entries for Python, JavaScript, Rust, Go. Each new language needs: `ecosystems/{lang}.rs` handler + `detector.rs` detection entry + test agent container support.

## GitHub Bot (Phase 2)

Webhook listener for `release.published` events. Auto-generates SKILL.md and submits PR.

## CI/CD GitHub Action (Phase 2)

Publish a GitHub Action so maintainers can add SKILL.md generation to their release workflow without the bot.

## Security Linter: LLM Adversarial Review (Layer 2)

Layer 1 (regex scan) is done — `src/lint.rs` checks for destructive commands, exfiltration, credential access, prompt injection, reverse shells, obfuscated payloads, etc. Post-normalization security hard-fail also implemented in generator.

Remaining: Layer 2 — LLM adversarial review before code execution ("is this code safe to run?"). Could be a dedicated security check within the review agent or a separate pipeline step.

## Switch to tokio::process

Replace `std::thread` + `mpsc::channel` timeout pattern with `tokio::process::Command` + `tokio::time::timeout`. Benefits:
- `kill_on_drop(true)` eliminates manual PID cleanup
- Non-blocking I/O scales better for parallel agent runs
- Simpler code (no channel boilerplate)
- Already have `#[tokio::main]`, so the runtime is there

Currently using a shared `run_cmd_with_timeout` in `src/util.rs` that uses the thread pattern.

**Priority**: Low — current pattern works and is DRY. `setsid` + process group kill already handles orphans.

## Multi-Skill Test Agent

Libraries like boto3 need mock frameworks (e.g., moto) for test agent validation. Requires teaching the test agent to install test dependencies alongside the target library.

## Defense-in-Depth: Dual Sanitization

Executor-level `sanitize_dep_name` is done (`src/test_agent/container_executor.rs`). Remaining: add validation at ingestion (parser.rs) so `CollectedData` carries clean strings from the start. Executor check stays as last line of defense.

## Tighten Duplicate Frontmatter Stripping Scope

`strip_duplicate_frontmatter()` in `normalizer.rs` counts all `---` lines globally. A SKILL.md using `---` as a horizontal rule could theoretically trigger false removal. Fix: only scan before the first `##` heading, and validate the candidate block contains frontmatter keys before stripping.

**Risk**: Low — only runs on LLM output, and LLMs don't typically emit horizontal rules.

## Linux Dev Setup + Dockerfile

Add `setup-linux` target to Makefile and create a multi-stage `Dockerfile` for building/testing on Linux. The Dockerfile doubles as a Linux compat test.

**Priority**: Medium — CI already tests on Ubuntu, but having a dev container makes it easy to reproduce CI locally.

## Windows Build (Cross-Compile)

Add a Windows target to the release workflow via `cargo build --target x86_64-pc-windows-msvc` on `windows-latest` runner. No platform-specific code exists today, so cross-compile should just work.

**Priority**: Low — no Windows users yet. Revisit when there's demand.

## OAuth Authentication for LLM Providers

Allow users to authenticate via OAuth instead of raw API keys. Would open a browser for the OAuth flow, store refresh tokens locally, and handle token refresh automatically.

**Priority**: Low — API keys work fine for v0.1. Nice-to-have for accessibility.

---

## Known Issues

Tracked items that auditors have identified but are deferred. Reference this section so automated reviews don't re-report known issues.

### `max_retries` naming inconsistency

`Generator` uses `0..max_retries` (treats value as attempt count), while `RetryClient` uses `0..=max_retries` (treats value as retry count, giving `max_retries + 1` total attempts). The two components interpret the same semantic name differently. Renaming to `max_attempts` is a breaking config change — defer to v0.2.0.

**Files**: `src/pipeline/generator.rs`, `src/llm/client.rs`

### `config check` runtime health insufficient

`config check` verifies the container runtime with `--version`, which proves the binary exists but not that the daemon/socket is available. A more robust check would attempt `podman/docker info` or a minimal container run.

**Files**: `src/cli/config_check.rs`

### Recursion depth limits in Python file discovery

`collect_py_files` and `collect_test_files` in `src/ecosystems/python.rs` recurse without depth limits. Pathological directory structures could cause stack overflow. Low risk for Python projects but may become relevant for Go (deeply nested module trees).

**Files**: `src/ecosystems/python.rs`

### Review: `{"passed": false, "issues": []}` becomes pass

`parse_review_response` recomputes `passed` from `issues.iter().any(|i| matches!(i.severity, Severity::Error))`. If the LLM returns `{"passed": false, "issues": []}`, the result becomes `passed: true`. This is intentional — we trust the issues list over the LLM's boolean verdict, since an LLM saying "failed" with no actionable issues shouldn't block the pipeline. Conversely, `{"passed": true, "issues": [{"severity": "error", ...}]}` becomes `passed: false`.

**Files**: `src/review/mod.rs`

### npm install missing `--` terminator (JS path)

`container_executor.rs` splices dependencies directly into `npm install --no-save ...` without a `--` separator. A dependency name starting with `-` could be interpreted as a flag. Not exercised today (Python-only), but becomes a footgun when JS validation lands.

**Files**: `src/test_agent/container_executor.rs`

### Local-install mode broken with `uv run`

When `container_runtime` is unset and the executor falls back to `uv run`, it invokes `uv run --with {dep} python script.py`. If the dep isn't already installed, `uv run` creates an ephemeral virtualenv per invocation — losing any `pip install` side effects from the introspection script. This means review introspection doesn't work outside containers.

**Files**: `src/test_agent/container_executor.rs`

### Container `--user nobody` not present in code

v0.1.6 backlog says "Non-Root Container Execution" was fixed, but `--user nobody` is not present in `container_executor.rs` or `validator.rs`. Generated code runs as root inside containers. Needs investigation — may have been lost or never landed.

**Files**: `src/test_agent/container_executor.rs`, `src/validator.rs`

### `install_source` config field is stringly-typed

Config/docs say valid values are `registry`, `local-install`, and `local-mount`, but the code stores a raw `String`. Typos silently change execution semantics (any non-`registry` value mounts `/src`). Should be a typed enum with deserialization validation.

**Files**: `src/config.rs`, `src/test_agent/container_executor.rs`

### Standalone `review` silently defaults to Python for unknown ecosystems

When frontmatter `ecosystem:` is missing or contains an unsupported value, `cli/review.rs` defaults to Python instead of failing. This can produce misleading review results for non-Python skills.

**Files**: `src/cli/review.rs`

### Config discovery anchored to CWD, not target repo

`generate` and `review` both load config via `Config::load_with_path()` before anchoring to the supplied repo path. Default `skilldo.toml` lookup probes CWD, not the target repo. Running from outside the target repo silently uses the wrong config.

**Files**: `src/cli/generate.rs`, `src/cli/review.rs`, `src/config.rs`

### Example files collected as source files

Source discovery (`collect_py_files`) does not exclude `examples/`, `samples/`, or `demo*/` directories. This burns source budget on duplicate content when the collector also reads examples separately via `find_examples()`.

**Files**: `src/ecosystems/python.rs`, `src/pipeline/collector.rs`

---

## Fixed (v0.1.7)

Items resolved in v0.1.7. Kept for audit trail — do not re-report.

- **`pyproject.toml` name parsing not scoped to `[project]`** — shared `pyproject_project_field()` helper with exact key matching
- **Python version detection prefers docs over package metadata** — reordered: pyproject.toml first, docs as fallback
- **Frontmatter normalization checks whole document body** — scoped field checks to frontmatter block only
- **Collector silently drops unreadable files** — added `warn!` on read failures
- **Source file discovery includes test files** — excluded `tests/`, `test/`, `testing/` dirs and `test_*.py` filenames
- **Test agent failures swallowed by generator** — `has_unresolved_errors` flag + `--best-effort` CLI option
- **Review malformed verdicts silently pass** — `malformed` flag on `ReviewResult` with retry logic
- **Test agent parser case-sensitive headings** — case-insensitive regex for section headings and code fences

---

## Fixed (v0.1.6)

Items resolved in v0.1.6. Kept for audit trail — do not re-report.

- **Gemini API key in query string** — moved to `x-goog-api-key` header
- **Non-Root Container Execution** — `--user nobody` added to container runs
- **Type-Safe Config Enums** — `Provider` enum replaces stringly-typed provider matching
- **Process Tree Cleanup on Timeout** — `setsid` + process group kill for orphan prevention
- **Data-Driven Lint Rules** — `src/lint.rs` refactored to data-driven rule definitions
- **Ollama concurrent request hang** — requests serialized when provider is Ollama
- **Malformed config files silently ignored** — parse errors now surface immediately
- **`extra_env` allows environment exfiltration** — warning emitted when `extra_env` is non-empty
- **CLI advertises unsupported languages** — validates language is fully supported, errors early
- **`FunctionalValidator` language coupling** — accepts `Language` enum, dispatches generically
- **Smart URL construction** — OpenAI-compatible clients respect full endpoint paths
- **Locality-aware API key validation** — only downgrades missing keys for truly local endpoints
- **`config check` non-gating by default** — prints diagnostics and exits 0; `--strict` for CI gating
- **Case-insensitive safety/security matching** — review bail uses `eq_ignore_ascii_case`
- **Introspection JSON validation** — uses `serde_json::from_str` instead of brace-checking
- **YAML frontmatter disambiguation** — tightened heuristic (lowercase-only keys, >= 2 matches)
