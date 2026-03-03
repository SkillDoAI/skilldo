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

## Changelog as Collection Input

`src/changelog.rs` has a changelog analyzer (currently dead code) that classifies changes as breaking/features/deprecations/behavior/bugfixes. Originally conceived for "should we regen?" decisions. Better use: feed changelog content into stages 1-3 as gathering input.

**Why**: Changelogs are often more honest than stale docs about what actually shipped. They tell the LLM what's new, what's deprecated, what broke — context that improves SKILL.md accuracy.

**Two uses**:
1. **Collection input** — collector already reads changelogs (`changelog_content` in `CollectedData`). The analyzer could pre-classify sections so the LLM prompt highlights what matters most (breaking changes, new APIs) vs noise (patch fixes).
2. **Update mode** — when an existing SKILL.md is present and relevant to the target library, diff the old skill against the changelog to decide what sections need refreshing rather than regenerating from scratch.

**Existing code**: `ChangelogAnalyzer` with keyword-based classification. May want LLM-assisted classification for nuance, but the current structure (significance enum, change categories) is a reasonable scaffold.

**Also**: In monorepos, need to match changelog to the specific library being generated (not assume one changelog = one library).

## Flip Container Default → uv bare metal (v0.1.9)

**Decision**: `uv` (bare metal) is the default execution mode. `--container` opts into podman/docker isolation. Opposite of current behavior.

`PythonUvExecutor` stub in `src/test_agent/executor.rs` — needs fleshing out as the new default path.

**Why uv default**:
- **CI environments** — already in a clean container, podman-in-container is wasted overhead
- **Local dev** — `uv` venvs are isolated enough to not trash a user's system
- **Simpler onboarding** — no podman/docker dependency for first-time users

**Container mode stays** — `--container` flag for users who want full isolation (untrusted code, paranoia, matching prod environments). Already written, no reason to remove.

**Status**: Stub only. Needs: actual uv execution logic, venv creation, dep install, script running, cleanup. Then flip the default in config/CLI.

## Generation Telemetry (v0.1.9)

Structured run log at `~/.skilldo/runs.csv` — tracks every generation for data-driven quality analysis.

**Fields**: language, library, library_version, models (per stage), retries, pass/fail, timestamp, prompt_version (or skilldo version as proxy)

**Purpose**: Collect real data on what model combos work for which libraries before making formal quality docs. Optional for users (opt-in via config), always-on for us during development.

**Later**: Once we have enough data, build something more formal — success rate by model, retry patterns, prompt version regression tracking.

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

### Go readiness (v0.2.0 prep, audited March 2026)

What's already done for Go:
- `Language::Go` enum variant, detection from `go.mod`, `Language::from_str("go"/"golang")`
- Container executor: image selection, `go run main.go`, script generation, install scripts
- Prompt layering: `go_hints()` placeholder in `prompts_v2.rs` (returns `""` for all stages)
- Review agent: works without introspection (Phase B LLM verdict is language-agnostic)
- CLI: `--language go` threaded through entire pipeline

What needs building:
- `src/ecosystems/go.rs` — GoHandler (file discovery, version from go.mod, license, deps)
- `src/test_agent/go_parser.rs` — GoParser implementing LanguageParser
- `src/test_agent/go_code_gen.rs` — GoCodeGenerator implementing LanguageCodeGenerator
- Collector `collect_go()` method + validator dispatch
- Populate `go_hints()` with stage-specific prompt guidance

**Approach**: Experiment branch off v0.1.9 to see what overlaps with Python parser helpers vs what's genuinely different. Don't abstract prematurely — extract shared code when we see real duplication across two implementations.

## GitHub Bot (Phase 2)

Webhook listener for `release.published` events. Auto-generates SKILL.md and submits PR.

## CI/CD GitHub Action (Phase 2)

Publish a GitHub Action so maintainers can add SKILL.md generation to their release workflow without the bot.

## Security: Adversarial Testing + Upstream Alerts

Two layers already exist: regex lint (`src/lint.rs`) hard-fails on destructive commands/exfiltration/injection, and the review agent's LLM verdict evaluates safety. `bail_on_security_lint` stops generation cold on security-category findings. No new infrastructure needed.

**What's missing**:
1. **Red-team test suite** — three tiers of test SKILL.md files:
   - **Legit-but-scary**: `os.remove(tempfile)`, `subprocess.run(["rm", "-rf", build_dir])`, `shutil.rmtree(cache_path)` — reviewer should PASS these. Context matters: cleanup of temp/build/cache dirs is normal library behavior.
   - **Obviously malicious**: reverse shells, credential exfiltration, download-and-execute — reviewer should hard-FAIL.
   - **Subtle/obfuscated**: dynamic construction of dangerous calls, encoded payloads, variable indirection that chains into something bad — the real test. This is the cat-and-mouse layer; we won't catch everything but should raise the bar.
   - **Real-world supply chain examples**: npm/PyPI have had published packages with obfuscated exfiltration (post-mortems are public). Those make great test cases grounded in actual attacks.
2. **Prompt tuning** — based on red-team results, teach the reviewer to evaluate *intent* (what is the target of the destructive call?) not just *presence* (does this code contain rm?). The regex linter catches presence; the LLM layer should judge intent.
3. **Upstream alerting** — open question: if the *source library itself* has sketchy patterns that surface in the SKILL.md, should we just bail silently, or actively alert the user that something looks wrong in the library's own code? Options:
   - Hard stop + report (current behavior for generated content)
   - Warning report file alongside SKILL.md (lets user decide)
   - Separate `skilldo audit` command that checks a library for suspicious patterns without generating

**Status**: Infrastructure done. Needs adversarial test cases and a decision on upstream alerting behavior.

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

### Per-stage retry granularity

Current retry settings: `max_retries` (format lint + test agent loop) and `review_max_retries` (review loop). Both default to 5. Two design questions to resolve:

1. **Stages 1-3 (extract/map/learn)** — gathering stages. Currently no retry setting; they each call the LLM once. Do these need retry knobs? They're less unpredictable than generation, but model quality varies.

2. **Stages 4-5-6 (create/review/test)** — bundled in a loop. If code writer fails, it got bad info from the skill generator and loops back. If validator rejects, it loops back. These are already retried, but the retry is shared across format+test. Should create, review, and test each have independent retry budgets?

**Considerations**: More knobs = more config surface. Stages 4-5-6 are tightly coupled — a test failure may mean create needs to redo, not that test should retry in isolation. May want to think about this holistically when we see how Go behaves (different models per stage may shift where retries matter most). Don't need CLI flags for everything — config-only is fine for niche tuning.

**Files**: `src/pipeline/generator.rs`, `src/config.rs`, `src/cli/generate.rs`

### `config check` runtime health insufficient

`config check` verifies the container runtime with `--version`, which proves the binary exists but not that the daemon/socket is available. A more robust check would attempt `podman/docker info` or a minimal container run.

**Files**: `src/cli/config_check.rs`

### Recursion depth limits in Python file discovery

`collect_py_files` and `collect_test_files` in `src/ecosystems/python.rs` recurse without depth limits. Pathological directory structures could cause stack overflow. Low risk for Python projects but may become relevant for Go (deeply nested module trees).

**Files**: `src/ecosystems/python.rs`

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

Container introspection (Phase A) only runs for Python. Non-Python languages get LLM-only review (Phase B verdict). Not a bug — Go/JS/Rust introspection would need language-specific scripts. Acceptable for v0.2.0; revisit when Go is the primary ecosystem.

**Files**: `src/review/mod.rs`

### Review: `{"passed": false, "issues": []}` becomes pass

`parse_review_response` recomputes `passed` from `issues.iter().any(|i| matches!(i.severity, Severity::Error))`. If the LLM returns `{"passed": false, "issues": []}`, the result becomes `passed: true`. This is intentional — we trust the issues list over the LLM's boolean verdict, since an LLM saying "failed" with no actionable issues shouldn't block the pipeline. Conversely, `{"passed": true, "issues": [{"severity": "error", ...}]}` becomes `passed: false`.

**Files**: `src/review/mod.rs`


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
