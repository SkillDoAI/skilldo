# Changelog

All notable changes to Skilldo are documented here. This changelog is also
published verbatim in [GitHub Releases](https://github.com/SkillDoAI/skilldo/releases).

## 0.4.1

- Added hard-error guard for `install_source = "local-install"` / `"local-mount"` on non-Python languages — previously silently did nothing useful, now fails early with a clear error message
- Added `review_degraded` field to `ReviewResult`, `GenerateOutput`, and `RunRecord` — propagates degraded introspection status through to telemetry CSV and structured log output
- Structured log status now three-valued: "errors" / "degraded" / "ok" (was binary "errors" / "ok") — CI consumers can distinguish grounded vs advisory reviews
- Added tests for review degraded propagation, telemetry CSV formatting, auth token error handling, factory API key edge cases, security boundary helpers
- Added `migrate_header_if_stale` for CSV telemetry — transparently upgrades old `runs.csv` headers when new columns are added (e.g., `review_degraded`)
- Fixed install-source guard to skip when test agent is disabled (`--no-test` / `enable_test = false`)
- Coverage: 1894 tests, 97.9%+ line coverage

## 0.4.0

- Added full Rust/Cargo ecosystem support across all 6 pipeline stages (extract → map → learn → create → review → test)
- Added `RustHandler`: source file discovery with `lib.rs` > `mod.rs` > `main.rs` priority, `Cargo.toml` metadata extraction, workspace-inherited field rejection
- Added `RustParser`: dependency extraction from `use crate::`, `extern crate`, `cargo add`, and `[dependencies]` TOML sections with stdlib filtering
- Added `RustCodeGenerator`: standalone `fn main()` programs using shared `find_fenced_blocks()` utility
- Added `CargoExecutor`: bare-metal executor with isolated `CARGO_HOME` in temp dir
- Added `rust_hints()`: stage-specific prompt guidance for all 6 pipeline stages
- Added Rust e2e matrix entry in CI (matklad/once_cell v1.21.3)
- Added path traversal guard on `license-file` in Rust Cargo.toml parsing
- Fixed Rust parser: CRLF-safe code block regex, aliased import extraction, inline TOML comment handling
- Fixed Go e2e: switched from `go-chi/chi` to `gorilla/mux` for stable CI
- 117 new tests, 1825 lib tests passing, 97%+ coverage

## 0.3.2

- Fixed degraded review introspection: now surfaces as `unresolved_warnings` even when review passes — no longer silently swallowed as a clean pass
- Fixed post-review test failure: marks run as unresolved instead of silently accepting broken rewrites
- Fixed collector budget accounting: uses worst-case header length to prevent overflow
- Added tilde fence (`~~~`) support in `strip_markdown_fences` and `extract_python_script`
- Added pip install extras/version spec preservation (`requests[socks]>=2.32`, `"sqlalchemy[asyncio]"`)
- Coverage: 1594 tests, 97.03% line coverage

## 0.3.1

- Added `--telemetry` / `--no-telemetry` CLI flags — telemetry is now opt-in (disabled by default), `--no-telemetry` overrides `telemetry = true` in config
- Added executor isolation: GoExecutor sets `GOPATH`/`GOCACHE`/`GOMODCACHE` to temp dir subdirs; NodeExecutor sets `npm_config_cache` to temp dir; PythonUvExecutor sets `UV_CACHE_DIR` to temp dir — prevents global state pollution during bare-metal test runs
- Added E2E matrix strategy in CI: Python, Go, and JavaScript e2e tests run in parallel (`fail-fast: false`), split into build + test jobs
- Added `is_tool_available()` shared helper — replaced 5 duplicated tool-check implementations across executors
- Added `classify_result()` shared helper — replaced 3 duplicated pass/fail match blocks across executors
- Added tests for `classify_result`, `calculate_file_priority`, `is_tool_available`, `stdout_and_stderr` combiner
- Fixed `calculate_file_priority` bug: `__init__.py` inside internal/test directories (e.g., `tests/__init__.py`) now correctly gets priority 100, not priority 0
- Updated README: bare-metal default, prerequisites table (uv, go, node/npm)
- DRY executor refactor: consolidated 17 duplicate tests, net −155 lines

## 0.3.0

- Added full JavaScript/TypeScript ecosystem support — package.json metadata, npm dependency management, `node:24-alpine` container image, bare-metal (`node` + `npm`) validation
- Added `JsHandler` ecosystem handler with file discovery, priority scoring, license detection, and project URL extraction
- Added `JsParser` for extracting imports (CommonJS `require()` and ES Module `import`), detecting 42 Node.js built-in modules, normalizing scoped/subpath packages
- Added `JsCodeGenerator` for extracting code from js/javascript/ts/typescript/jsx/tsx fenced blocks
- Added `NodeExecutor` for bare-metal JavaScript test execution
- Added npm subpath import normalization — collapses `lodash/chunk` → `lodash` and `@scope/pkg/utils` → `@scope/pkg` for correct `npm install`
- Added JavaScript e2e smoke test in CI (lodash 4.17.21 via Cerebras)
- Fixed npm install command construction — `Command::args()` doesn't use a shell, so quotes passed as literal characters

## 0.2.5

- Added `provider_type = "cli"` — shell out to vendor CLIs (claude, codex, gemini) instead of HTTP API calls for subscription-based model access
- Added `cli_command`, `cli_args`, `cli_json_path` config fields for CLI provider configuration (json_path supports dot-notation for nested fields like `data.response`)
- Added auto-disable of parallel extraction when any stage uses a CLI provider
- Added `Severity::deduction()` method to replace 2 duplicated match blocks in security module
- Added shared ecosystem utilities: `classify_license()`, `LICENSE_FILENAMES` in `ecosystems/mod.rs`
- Added normalizer: strips blank lines inside YAML frontmatter, trims trailing whitespace on `---` delimiters
- Added normalizer: detects and strips metadata fields (e.g., `generated-by`) leaking from frontmatter into body content
- Added dual-licensing SPDX expression guidance (`MIT OR Apache-2.0`) to create prompt
- Bumped `review_max_retries` default from 5 to 10
- Improved CLI provider error messages: shows up to 5 lines of stderr (was 1) for better debugging
- Improved BSD license classification: uses non-endorsement clause to distinguish BSD-3-Clause from BSD-2-Clause (was relying on header ordering only)
- Improved normalizer: `strip_leaked_metadata` now skips lines inside fenced code blocks to prevent content corruption
- Fixed auth CLI tests failing when local `skilldo.toml` has OAuth config
- CI: switched e2e tests from Anthropic to Cerebras (`gpt-oss-120b` via `openai-compatible`)

## 0.2.4

- Added generic OAuth 2.0 + PKCE authentication for any provider (Google, OpenAI, or any OAuth 2.0-compatible endpoint)
- Added `skilldo auth login` / `status` / `logout` CLI commands for managing OAuth sessions
- Added `oauth_credentials_env` shortcut for base64-encoded OAuth credentials JSON (uses Google's `client_secret_*.json` format — any provider can use it, not Google-specific)
- Added per-stage OAuth — each pipeline stage can authenticate with a different provider/subscription
- Added token storage at `~/.config/skilldo/tokens/{provider_name}.json` with secure permissions (0600 file, 0700 dir)
- Added automatic token refresh when access tokens expire (60s safety buffer)
- Added `extra_headers` config field for injecting custom HTTP headers into LLM API requests (e.g., `ChatGPT-Account-ID`, `OpenAI-Beta`)
- Added `GoExecutor` for bare-metal Go test execution (no longer requires container runtime for Go)
- Renamed CI e2e steps to include language prefix (e.g., "Generate Python SKILL.md", "Validate Go output")
- Renamed test references from `agent5` to `test_agent` naming convention
- Updated README with Authentication section (OAuth setup for OpenAI + Google, `auth` commands)
- Removed `dev/scripts/migrate-config.sh` (replaced by MIGRATION.md)
- Async migration: `create_client` and `create_client_from_llm_config` are now `async fn`
- Added `chatgpt` provider type with Responses API client — supports OAuth-based ChatGPT subscription use via the Codex backend
- ChatGPT client uses non-streaming Responses API (simpler, no SSE parsing)
- ChatGPT provider warns when `extra_body` is configured (Responses API does not support it)
- Added README Table of Contents, config file vs CLI callout, and expanded Full Documented Config reference
- GeminiClient conditionally uses `Authorization: Bearer` header when authenticated via OAuth
- **BREAKING:** Removed deprecated `agentN` config aliases (`agent1_llm`..`agent5_llm`, `enable_agent5`, `agent5_mode`, `agent1_mode`..`agent4_mode`, `agent1_custom`..`agent5_custom`). Use `extract_llm`/`map_llm`/`learn_llm`/`create_llm`/`test_llm`, `enable_test`, `test_mode`, `extract_mode`..`create_mode`, `extract_custom`..`test_custom` instead.
- **BREAKING:** Removed `--agent5-model`, `--agent5-provider`, `--no-agent5`, `--agent5-mode` CLI aliases. Use `--test-model`, `--test-provider`, `--no-test`, `--test-mode` instead.

## 0.2.3

- Added typed timeout errors with `thiserror` crate — `SkillDoError::Timeout` replaces string matching
- Added `provider_type` as preferred config field name with `provider` as legacy alias
- Added `provider_name` field for human-readable provider instance labels
- Added Go e2e smoke test in CI alongside existing Python (click) test
- Updated README with badges (CI, Codecov), refreshed pitch deck, fixed Go language support table
- Updated model references to current names (claude-sonnet-4-6, gpt-5.2)

## 0.2.2

- Migrated all subprocess execution from `std::process::Command` to `tokio::process::Command`
- Made `LanguageExecutor` trait async with `#[async_trait]`
- Added `kill_on_drop(true)` on all child processes to prevent zombie leaks
- Eliminated `libc` dependency (was used for process group kills, now handled by tokio)
- Fixed profdata deadlock in CI coverage runs caused by blocking process waits

## 0.2.1

- Migrated all 7 `Command::new("git")` calls to `git2` native library
- Added `Git2Repo` struct in `src/git.rs` with SSH/HTTPS credential callbacks
- Added YARA `prose_only = true` metadata replacing hardcoded `PROSE_ONLY_RULES` array
- Added `check_runtime_daemon()` config health check with 10s spawn+try_wait timeout
- Fixed `describe_tags()` stripping `-N-gHASH` suffix from git describe output
- Fixed `branch_name()` returning "HEAD" in detached state (matches git CLI behavior)
- Fixed `fetch_tags()` force refspec, distinguishes timeout vs thread panic errors

## 0.2.0

- Added full Go ecosystem support — `go.mod` parsing, Go file collection, Go containers
- Added Go version detection (git tags, `const Version`, VERSION files, Major/Minor/Patch ints)
- Added Go example/test/doc file categorization
- Added version extraction improvements across all languages
- Added tilde fence fix in normalizer
- Hardened prompt injection defenses
- 1182 tests

## 0.1.11

- Migrated to YARA-primary scanning (pattern + unicode + injection scanners now secondary)
- Added fail-closed YARA gate — pipeline aborts if YARA engine fails to initialize
- Added config file discovery (`skilldo.toml`, `.skilldo.toml`, `.config/skilldo.toml`)
- Added Python `src/` layout detection for package discovery
- Added 529 retry handling for overloaded API responses
- Hardened pipeline safety with additional input validation

## 0.1.10

- Added `--no-security-scan` flag to skip security scanning during generation
- Consolidated CI workflows
- Deduplicated SkillDo YARA rules with Rust-native scanners

## 0.1.9

- Added 3-layer security scanner: regex patterns + prompt injection detection + YARA rules
- Added agentskills.io frontmatter compatibility
- Added typed enums for Language, Provider, and pipeline stages
- Added CI ARM runner support
- Added UTF-8 safety hardening across all file operations
- Added telemetry CSV output for pipeline metrics
- Added changelog annotation in generated skills
- Added `--review-only` and `--no-review` CLI flags
- Added adversarial prompt injection test suite
- Fixed SD-106 false positives on documentation
- Fixed pattern scanner to skip code blocks for library API patterns
- Fixed SD-201 false positives on prose mentions of subprocess APIs
- Fixed YARA scanner code-block awareness

## 0.1.8

- Refactored to language-generic architecture — type-safe `Language` enum replaces string matching
- Added container hardening with read-only filesystem and resource limits
- Optimized CI with shared Rust cache and parallel test execution
- Fixed integration test flake from non-deterministic temp directory cleanup

## 0.1.7

- Added pipeline trust framework — output validation between agents
- Added data quality checks for extracted API surfaces
- Added Go language detection (prep for v0.2.0 full support)

## 0.1.6

- Hardened review agent with structured validation
- Improved linter accuracy and coverage
- Refactored agent naming (Agent 1-5 to extract/map/learn/create/test)
- Added type safety improvements across pipeline

## 0.1.5

- Renamed agents to semantic names (extract, map, learn, create, test)
- Added review agent (accuracy + safety validation)
- Hardened test coverage to 96%

## 0.1.4

- Added linter hardening with additional rule checks
- Added `skilldo config check` command for configuration validation
- Added version detection from git tags and source constants
- Fixed normalizer handling of LLM body-wrapping markdown fences
- Enabled Agent 5 (test agent) in CI e2e tests

## 0.1.3

- Added linter for generated SKILL.md validation
- Added config check command
- Improved version detection across ecosystems

## 0.1.2

- Initial public release improvements
- Pipeline stabilization

## 0.1.1

- Added CI release builds, auto-tagging, and Homebrew tap
- Added Dependabot for automated dependency updates
- Fixed auto-tag to wait for CI, grouped weekly dependency updates

## 0.1.0

- Initial release
- 6-agent pipeline: extract, map, learn, create, review, test
- Multi-provider LLM support (Anthropic, OpenAI, Gemini, OpenAI-compatible)
- Python ecosystem support
- Container-based code validation
- YAML frontmatter + Markdown output format
- Regex-based security scanning
