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
**Status**: Verified in v0.1.11 — `version_from` exists in both config and CLI (`--version-from`), `--max-retries` exists in both CLI and config (`max_retries`). No gaps remain. Keeping this entry for the principle; revisit if new config fields are added.

**Principle**: Config file is the full spec, CLI args are overrides.

## Changelog as Collection Input

`src/changelog.rs` has a changelog analyzer (currently dead code) that classifies changes as breaking/features/deprecations/behavior/bugfixes. Originally conceived for "should we regen?" decisions. Better use: feed changelog content into stages 1-3 as gathering input.

**Why**: Changelogs are often more honest than stale docs about what actually shipped. They tell the LLM what's new, what's deprecated, what broke — context that improves SKILL.md accuracy.

**Two uses**:
1. **Collection input** — collector already reads changelogs (`changelog_content` in `CollectedData`). The analyzer could pre-classify sections so the LLM prompt highlights what matters most (breaking changes, new APIs) vs noise (patch fixes).
2. **Update mode** — when an existing SKILL.md is present and relevant to the target library, diff the old skill against the changelog to decide what sections need refreshing rather than regenerating from scratch.

**Existing code**: `ChangelogAnalyzer` with keyword-based classification. May want LLM-assisted classification for nuance, but the current structure (significance enum, change categories) is a reasonable scaffold.

**Also**: In monorepos, need to match changelog to the specific library being generated (not assume one changelog = one library).

## ~~Flip Container Default → uv bare metal~~ — Done in v0.3.1

Resolved: `ExecutionMode::BareMetal` is the default for all languages. `PythonUvExecutor`, `GoExecutor`, and `NodeExecutor` run in isolated temp directories with no global state pollution (GOPATH, npm cache, uv venv all kept local). `--container` opts into Docker/Podman isolation. v0.3.1 hardened isolation (GOPATH/GOCACHE/npm cache confined to temp dirs).

## ~~Generation Telemetry~~ — Done in v0.3.1

Resolved: `~/.skilldo/runs.csv` logs generation runs when enabled. Fields: timestamp, language, library, version, models, retries, passed, duration_secs, skilldo_version. Opt in with `--telemetry` or `telemetry = true` in config.

## Multi-Language Test Agent

Extend test agent code generation validation beyond Python:
- ~~JavaScript/TypeScript (node container)~~ — **shipped in v0.3.0**
- ~~Rust (cargo container)~~ — **shipped in v0.4.0**
- ~~Go (go container)~~ — **shipped in v0.2.0**

## Language Ecosystem Support

New ecosystem handlers needed for each language. Each requires: file discovery (source, tests, docs), dependency detection, and test agent container support.

**High priority**:
- ~~JavaScript/TypeScript~~ — **shipped in v0.3.0** (npm, `package.json`, JsHandler, JsParser, JsCodeGenerator, NodeExecutor)
- ~~Go~~ — **shipped in v0.2.0** (go modules, `go.mod`, full pipeline support)
- ~~Rust~~ — **shipped in v0.4.0** (crates.io, `Cargo.toml`, RustHandler, RustParser, RustCodeGenerator, CargoExecutor)

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

### JavaScript/TypeScript ecosystem — shipped in v0.3.0

All JS/TS support landed in v0.3.0:
- `src/ecosystems/javascript.rs` — JsHandler (file discovery, version from package.json, license, deps)
- `src/test_agent/js_parser.rs` — JsParser implementing LanguageParser
- `src/test_agent/js_code_gen.rs` — JsCodeGenerator implementing LanguageCodeGenerator
- `src/test_agent/container_executor.rs` — NodeExecutor with npm install + `--ignore-scripts --no-audit --no-fund`
- Collector `collect_javascript()` method + validator dispatch
- `js_hints()` populated with stage-specific prompt guidance
- `sanitize_dep_name` guard on package names, LANG_TAGS whitelist for code fence stripping
- Full test coverage for JS pipeline paths

### Go ecosystem — shipped in v0.2.0

All Go support landed in v0.2.0:
- `src/ecosystems/go.rs` — GoHandler (file discovery, version from go.mod, license, deps)
- `src/test_agent/go_parser.rs` — GoParser implementing LanguageParser
- `src/test_agent/go_code_gen.rs` — GoCodeGenerator implementing LanguageCodeGenerator
- Collector `collect_go()` method + validator dispatch
- `go_hints()` populated with stage-specific prompt guidance
- Full test coverage for Go pipeline paths

Pre-existing scaffolding (detection, container executor, CLI threading, review agent) carried over from v0.1.x.

### Rust ecosystem — shipped in v0.4.0

All Rust support landed in v0.4.0:
- `src/ecosystems/rust.rs` — RustHandler (file discovery, version from Cargo.toml, license, deps, project URLs)
- `src/test_agent/rust_parser.rs` — RustParser implementing LanguageParser (`use`/`extern crate`/`cargo add`/`[dependencies]` extraction)
- `src/test_agent/rust_code_gen.rs` — RustCodeGenerator implementing LanguageCodeGenerator
- `src/test_agent/executor.rs` — CargoExecutor with isolated CARGO_HOME, `cargo run --quiet`
- Collector `collect_rust()` method + validator dispatch
- `rust_hints()` populated with stage-specific prompt guidance
- Full test coverage for Rust pipeline paths

## GitHub Bot (Phase 2)

Webhook listener for `release.published` events. Auto-generates SKILL.md and submits PR.

## CI/CD GitHub Action (Phase 2)

Publish a GitHub Action so maintainers can add SKILL.md generation to their release workflow without the bot.

## Security: Adversarial Testing + Upstream Alerts

**Priority**: Nice to have. Three layers of security scanning already cover this well: regex lint (`src/lint.rs`) hard-fails on destructive commands/exfiltration/injection, YARA rules (including Cisco AI Defense patterns) catch obfuscated payloads, and the review agent's LLM verdict evaluates safety. `bail_on_security_lint` stops generation cold on security-category findings. `sanitize_dep_name` guards all ingestion paths. Container runs use `--user nobody`. All paths are unit-tested.

**If revisited**: Red-team test SKILL.md files (legit-but-scary, obviously malicious, obfuscated) and a decision on upstream alerting (warn users about sketchy source libraries vs. silent bail).

## ~~Switch to tokio::process~~ — Done in v0.2.2

Resolved: `run_cmd_with_timeout` migrated to `tokio::process::Command` + `tokio::time::timeout` + `kill_on_drop(true)`. `LanguageExecutor` trait made fully async. `libc` dependency removed. CI test skip removed. test-cov moved back to ARM.

## ~~E2E Test Parallelization~~ — Done in v0.3.1

Resolved: Matrix strategy runs Python, Go, and JavaScript e2e tests in parallel. `fail-fast: false` means one language's failure doesn't block others. Adding new languages is one matrix row.

## Mock LLM Services for CI

Deterministic mock server(s) that speak OpenAI/Anthropic/Gemini API formats with canned responses. Not a replacement for real-LLM e2e (which tests model quality), but useful for:

1. **Integration tests** — HTTP client layer, auth flows, retry/rate-limit logic, streaming, error response parsing. Currently these paths are tested via `mockito` inline mocks; a persistent mock service would enable more realistic multi-request sequences.
2. **OAuth flow testing** — mock token endpoints for the full PKCE dance without real provider accounts.
3. **CI cost reduction** — some pipeline tests (normalizer, linter, reviewer logic) don't need real LLM output, just well-formed responses in the right API format.

**Build vs buy**: Existing options include WireMock (generic HTTP mock, mature), LiteLLM proxy (can mock responses), and various OpenAI-specific mock servers. None cover all three providers with canned response scripting out of the box. A lightweight custom service (single binary or Docker image) with provider-format response templates may be the pragmatic path — could live in `SkillDoAI/megamock` or as a `dev/mock-server/` directory.

**Status**: Research phase. Decide scope before building.

## Cache Container Images in CI

Integration tests pull `python:3.11-alpine`, `ghcr.io/astral-sh/uv:python3.11-bookworm-slim`, and `golang:1.25-alpine` on every run. These are ~50-200MB each and rarely change. Options:
- `docker save` / `docker load` with `actions/cache`
- Pre-built composite action that checks cache before pulling
- GitHub Container Registry mirror with pinned digests

**Priority**: Low — integration tests only run on rust-changed PRs. Would save ~30s per run.

## Multi-Skill Test Agent

Libraries like boto3 need mock frameworks (e.g., moto) for test agent validation. Requires teaching the test agent to install test dependencies alongside the target library.

## Linux Dev Setup + Dockerfile

Add `setup-linux` target to Makefile and create a multi-stage `Dockerfile` for building/testing on Linux. The Dockerfile doubles as a Linux compat test.

**Priority**: Medium — CI already tests on Ubuntu, but having a dev container makes it easy to reproduce CI locally.

## Windows Build (Cross-Compile)

Add a Windows target to the release workflow via `cargo build --target x86_64-pc-windows-msvc` on `windows-latest` runner. No platform-specific code exists today, so cross-compile should just work.

**Priority**: Low — no Windows users yet. Revisit when there's demand.

## ~~OAuth Authentication~~ — Done in v0.2.4

Resolved: Generic provider-agnostic OAuth 2.0 + PKCE in `src/auth/`. Per-stage OAuth config in `skilldo.toml`, token storage at `~/.config/skilldo/tokens/`, auto-refresh on expiry, graceful fallback to API key auth. CLI: `skilldo auth login/status/logout`. Works with any provider that speaks OAuth 2.0 + PKCE.

### OAuth follow-ups (v0.3.x)

1. **Scope merging in `group_by_oauth_app`** — when multiple pipeline stages share the same OAuth app (same `token_url` + `client_id`), the login flow currently uses the first-seen endpoint's scopes. Should union scopes from all endpoints sharing the app so a single login covers all stages.

2. **Lazy token refresh in `complete()`** — OAuth tokens are resolved once at client creation (`create_client_from_llm_config`). Long-running pipeline stages (e.g., large test agent loops) can outlive the token's lifetime. Need token refresh inside `complete()` calls — either pass the endpoint through to the client, or wrap the token in an auto-refreshing handle.

---

## Known Issues

Tracked items that auditors have identified but are deferred. Reference this section so automated reviews don't re-report known issues.

### Per-stage retry granularity

Current retry settings: `max_retries` (format lint + test agent loop) and `review_max_retries` (review loop). Both default to 5. Two design questions to resolve:

1. **Stages 1-3 (extract/map/learn)** — gathering stages. Currently no retry setting; they each call the LLM once. Do these need retry knobs? They're less unpredictable than generation, but model quality varies.

2. **Stages 4-5-6 (create/review/test)** — bundled in a loop. If code writer fails, it got bad info from the skill generator and loops back. If validator rejects, it loops back. These are already retried, but the retry is shared across format+test. Should create, review, and test each have independent retry budgets?

**Considerations**: More knobs = more config surface. Stages 4-5-6 are tightly coupled — a test failure may mean create needs to redo, not that test should retry in isolation. May want to think about this holistically when we see how Go behaves (different models per stage may shift where retries matter most). Don't need CLI flags for everything — config-only is fine for niche tuning.

**Files**: `src/pipeline/generator.rs`, `src/config.rs`, `src/cli/generate.rs`

### ~~`config check` runtime health insufficient~~ — Fixed in v0.2.1

Resolved: `check_runtime_daemon()` now runs `runtime info` after `--version` to verify the daemon/socket is responsive. Reports distinct errors for "binary not found" vs "daemon not responding".

**Files**: `src/cli/config_check.rs`

### API key validation allows `"none"` for remote endpoints (by design)

`openai-compatible` endpoints accept missing/`"none"` API keys regardless of base URL. This is intentional — supports Ollama on home networks without auth. Auditors (Codex, Gemini) flag this as a security gap; it's a deliberate design choice for flexibility.

**Files**: `src/config.rs`, `src/llm/client_impl.rs`

### Linter skips code blocks for security patterns (by design)

`src/lint.rs` excludes fenced code blocks from destructive-command scanning. Auditors flag this as a blind spot. It's intentional — SKILL.md code examples legitimately contain `shutil.rmtree()`, `os.remove()`, etc. Scanning code blocks would produce massive false positives. Security checks target prose sections where suspicious content shouldn't appear.

**Files**: `src/lint.rs`

### LocalInstall runs as root in container (by design)

`--user nobody` is applied to all container runs except `LocalInstall`, which needs root for `pip install /src`. Auditors flag root execution as risky. The container is ephemeral and the code being installed is the library under test — the user already trusts it.

**Files**: `src/test_agent/container_executor.rs`

### Review introspection is Python-only

Container introspection (Phase A) only runs for Python. Go, JavaScript, and Rust get LLM-only review (Phase B verdict). Not a bug — non-Python introspection would need language-specific scripts (`go doc`, `node -e`, `cargo doc`). Acceptable trade-off; revisit if review accuracy diverges by language.

**Files**: `src/review/mod.rs`

### OpenAI-compatible endpoint path inconsistency

`openai-compatible` provider appends `/v1/chat/completions` to the base URL, but some providers (e.g., vLLM, LocalAI) already include `/v1` in the base URL. Users must know to strip `/v1` from their base URL. Consider auto-detecting or normalizing trailing path segments.

**Files**: `src/llm/client_impl.rs`

### Review: `{"passed": false, "issues": []}` becomes pass

`parse_review_response` recomputes `passed` from `issues.iter().any(|i| matches!(i.severity, Severity::Error))`. If the LLM returns `{"passed": false, "issues": []}`, the result becomes `passed: true`. This is intentional — we trust the issues list over the LLM's boolean verdict, since an LLM saying "failed" with no actionable issues shouldn't block the pipeline. Conversely, `{"passed": true, "issues": [{"severity": "error", ...}]}` becomes `passed: false`.

**Files**: `src/review/mod.rs`


---

## Coverage Gaps (Defensive/Integration Paths)

**Note**: Coverage improved from 81% to 89% in v0.2.0 (Go ecosystem + test hardening). The remaining gaps below are genuinely hard-to-reach paths (cached singletons, multi-agent mock cycles).

### `mod.rs` 179-189 — YARA fail-closed error path

`OnceLock` cache makes the `Err(e)` branch of `YaraScanner::builtin()` unreachable in unit tests — once embedded rules compile successfully (which they always do), the result is cached. Options:
- Extend `FixtureLlmClient` e2e test with a broken YARA rule file via `with_rules_dir`
- Extract the match arm into a testable helper (but it's a single call site)

**Files**: `src/security/mod.rs`

### `generator.rs` 530-540 — test-fix retry with create-agent rewrite

The path where test failures produce feedback and the create agent rewrites. Needs a `ReviewFixtureClient`-style mock with test failure → feedback → rewrite cycle. Extend `fastapi_session.json` with `test_fail` + `create_fix_test` response pairs.

**Files**: `src/pipeline/generator.rs`, `tests/fixtures/fastapi_session.json`

### `generator.rs` 681-682, 690 — post-review test validation warn paths

The path where a review rewrite breaks a test (warn but keep review-fixed version). Needs a mock where `ReviewFixtureClient` also mocks `TestCodeValidator` returning failures after review rewrite.

**Files**: `src/pipeline/generator.rs`, `tests/test_fixture_pipeline.rs`

### ~~Python parser lacks tilde fence support~~ — Fixed in v0.3.2

Resolved: `python_parser.rs` code_block_re uses `(?:```|~~~)` and `python_code_gen.rs` delegates to `find_fenced_blocks()` which handles both fence styles. Tests cover tilde-fenced blocks.

### ~~Source-budget accounting undercounts file headers~~ — Already fixed

Resolved: Both `read_files()` (line 295) and `read_files_smart()` (lines 438, 445) include header length in `total_chars`. The `remaining` calculation properly accounts for headers. No code change needed.

## Code Nits (v0.1.12)

### ~~PROSE_ONLY_RULES sync risk~~ — Fixed in v0.2.1

Resolved: `prose_only = true` metadata added to YARA rules, read at scan time via `is_prose_only()`. Hardcoded `PROSE_ONLY_RULES` array removed.

**Files**: `src/security/yara.rs`, `rules/skilldo/dangerous_patterns.yara`

---

## Fixed (v0.1.11)

Items resolved in v0.1.11. Kept for audit trail — do not re-report.

- **YARA as primary scanner** — SkillDo YARA rules restored as source of truth for SD-201..SD-211; patterns.rs removed; Rust scanners kept only for SD-001 (homoglyphs), SD-004 (RLO), SD-005 (mixed-script), SD-110 (markdown injection), SD-111 (base64 decode), SD-112 (exfil prose)
- **Code-block filtering for YARA** — prose-only rules (SD-201, SD-202, SD-204, SD-205, SD-209, SD-210) skip matches inside fenced code blocks; checks ALL match offsets, not just first
- **YARA scanner caching** — `OnceLock` caches compiled `boreal::Scanner` process-wide; `builtin()` returns `&'static Self` — zero recompilation after first call
- **Shared security helpers** — `line_number()`, `snippet_at()`, `to_char_boundary()`, `dedup_findings()` extracted to `security/mod.rs`; removed 3 duplicated copies
- **Atomic output write** — SKILL.md written to `.tmp` first, renamed to final path only on success or `--best-effort`; failed runs no longer overwrite known-good output
- **Recursion depth limits** — `collect_py_files` and `collect_test_files` in `src/ecosystems/python.rs` bounded to `MAX_DEPTH = 20`
- **`GenerationConfig::default()` consistency** — uses `default_*()` functions matching serde defaults instead of hardcoded values
- **Short package name heuristic removed** — `is_likely_local_module` no longer drops 2-3 char package names; valid PyPI packages like `ray`, `gym`, `dbt`, `jax`, `six`, `bs4` are now preserved
- **Re-scan after rewrites** — security scan runs after every create-agent rewrite (lint fix, test fix, review fix) and after normalization
- **Re-test after review rewrites** — single test-agent validation pass after review-triggered rewrites; warns but keeps review-fixed version on test failure
- **Config discovery uses target repo** — `skilldo.toml` searched in target repo path between CWD and git-root checks
- **Python src/ layout version detection** — `get_version()` probes `src/<pkg>/__init__.py` in addition to `<pkg>/__init__.py`
- **529/overloaded transient error** — retry client recognizes HTTP 529 and "overloaded" as transient errors
- **Disable-flag log level** — `info!` → `warn!` for CLI overrides disabling test/security/review agents
- **Tighter YARA test assertions** — Cisco tests assert on `Category` enum, not just finding count
- **Stale doc comments** — "four detection layers" → "three detection layers" in 5 files

---

## Fixed (v0.1.9)

Items resolved in v0.1.9. Kept for audit trail — do not re-report.

- **`FunctionalValidator` removed** — deprecated legacy validator deleted (`src/validator.rs` + 3 test files), generator simplified to test-agent-only path
- **Python parser/codegen split** — `PythonParser` → `python_parser.rs`, `PythonCodeGenerator` → `python_code_gen.rs` (Go-ready module structure)
- **Collector budget by actual consumption** — source budget computed from actual bytes consumed by fixed categories, not allocated percentages
- **Module-level `//!` docs on all core files** — 13 files documented so future sessions don't need full code reads
- **`///` docs on key public APIs** — traits, major structs, entry-point functions, enums documented
- **`generate_feedback` None path explicit** — when test feedback is None during retry, logs warning and breaks instead of silently retrying with unchanged SKILL.md
- **`TestCodeValidator` hoisted before retry loop** — avoids repeated construction on each retry attempt
- **`max_retries` semantics inconsistency** — Generator validation loop now uses `0..=max_retries` (same as RetryClient and review loop). `max_retries=3` consistently means 1 initial + 3 retries = 4 total everywhere

---

## Fixed (v0.1.8)

Items resolved in v0.1.8. Kept for audit trail — do not re-report.

- **npm install `--` terminator** — `generate_node_install_script()` includes `--` before dependency names
- **Local-install mode `uv run` fix** — installs with `uv pip install --system /src` before `uv run`
- **Container `--user nobody`** — applied to all container runs except LocalInstall (which needs root for pip)
- **`install_source` typed enum** — `InstallSource` enum with `Registry`, `LocalInstall`, `LocalMount` variants + deser validation
- **Standalone `review` requires ecosystem** — errors with clear message instead of defaulting to Python
- **Config discovery CWD-first** — search order: explicit path → CWD → git root → user config dir → defaults
- **Example files excluded from source collection** — `collect_py_files()` skips `examples/`, `samples/`, `demo/` directories

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
- **Dual sanitization (defense-in-depth)** — ingestion-side `sanitize_dep_name` check in collector, executor check stays as last line of defense
- **Duplicate frontmatter stripping scope** — stops at first `##` heading, validates candidate block has YAML keys before stripping

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
