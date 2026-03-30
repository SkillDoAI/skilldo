# Changelog

All notable changes to Skilldo are documented here. This changelog is also
published verbatim in [GitHub Releases](https://github.com/SkillDoAI/skilldo/releases).

## 0.5.10

### Added
- **`with_base_url` for Anthropic and Gemini clients** — custom base URLs for Bedrock (Anthropic on AWS), Vertex AI (Gemini on GCP), and other private instances. Factory wires `base_url` from config when present
- **16 llmposter integration tests** — 3 tiers: client coverage (OpenAI, Anthropic, Gemini, 429, auth), failure handling (latency, corrupt body, provider routing), deterministic pipeline (full extract→map→learn→create→review against fixtures)
- **Deterministic pipeline test** — full pipeline runs against llmposter with canned responses. Zero LLM calls, fully reproducible. Validates plumbing end-to-end
- **Coverage sweep** — config loading fallbacks, Go ecosystem branches, rust parser dep sanitization, telemetry HOME dir path, factory base_url wiring. 2702 tests total

### Changed
- CI coverage now includes llmposter integration tests (`--test test_llmposter`) for client method coverage
- llmposter bumped to 0.4.1 in dev-dependencies
- `AnthropicClient::new()` and `GeminiClient::new()` delegate to `with_base_url()` (DRY, matches OpenAI pattern)
- Factory uses `match` for base_url routing instead of `if let` (cleaner)
- Documentation: `base_url` support noted for Anthropic and Gemini in provider table

## 0.5.9

### Fixed
- **Normalizer recursion depth limit** — `ensure_frontmatter` capped at 3 recursions, preventing stack overflow on pathological input
- **Malformed review → failed** — unparseable review verdicts now default to `passed: false` instead of silently bypassing the review gate
- **Collector warns on empty source** — logs warning when 0 bytes read from source files with available budget
- **Auth body errors logged** — `response.text().await.unwrap_or_default()` now logs the read error before defaulting (4 sites in oauth.rs/device_code.rs)
- **`failed_stage` preserves root cause** — review failures no longer overwrite an earlier test failure stage
- **`&PathBuf` → `&Path`** — `ensure_secure_dir` and `write_secure_file` accept `&Path` (API improvement)
- **Homebrew tap clone security** — PAT no longer embedded in git clone URL (same fix as llmposter)

### Changed
- Removed `claude-code-review.yml` and `claude.yml` CI workflows (noisy, not useful)
- SKILL.md updated to v0.5.9: added `--input`, `--debug-stage-files`, `--no-parallel` flags, CLI provider config, Java language support

## 0.5.8

### Added
- **Proactive conflict detection (RULE 13)** — create prompt actively scans for contradictions between custom_instructions, source comments, behavioral semantics, and actual code behavior before writing
- **Invisible Unicode sanitizer** — shared `strip_invisible_unicode()` in `security::unicode` strips model tokenizer artifacts (SD-002) at all sanitization sites
- **Extract prompt: method-level API surface** — models now list `TypeName::method_name` for public methods inside impl blocks, not just type definitions. Fixes false hallucination flags on legitimate methods
- **7 llmposter integration tests** — sequential consistency, no-match 404, concurrent requests, plus original 3
- **Windows `cargo test --lib` in CI** — informational (continue-on-error), surfaces platform-specific failures without blocking

### Fixed
- Normalizer: `ensure_frontmatter` scopes `name:`/`description:` check to candidate frontmatter block, not whole tail
- Normalizer: `strip_trailing_meta_text` tracks fence state in `all_trailing` check — prevents false positives on code inside fenced blocks
- Normalizer: `strip_body_markdown_fence` handles ````text` openers alongside ````markdown` and ````md`
- Normalizer: `strip_body_markdown_fence` finds closing fence by backward scan at depth 0, not just last non-empty line — fixes misclassifying paired wrappers with trailing content
- Generator: `with_debug_stage_dir(None)` now clears existing dir
- Generator: `/dev/null` test fixture replaced with portable temp-file approach (Windows-compatible)
- Generator: strip ordering — conflict notes before fence unwrapping at all sites
- Generator: sanitize conflict notes and invisible Unicode at all 4 rewrite paths (lint fix, test fix, review fix), not just initial create
- Review: malformed verdict now preserves raw response for debugging
- Review: reference data sections explicitly marked as user-controlled (security hardening)
- Review: hallucination check extended to Core Patterns code blocks, not just API Reference
- Prompts: custom_instructions override scoped to style/content rules only — RULE 8 (Security) is explicitly non-overridable
- Prompts: API Reference limited to library-owned methods (excludes stdlib)
- Security: RUSTSEC-2023-0071 exception for rsa timing sidechannel (dev-dep only via llmposter → oauth-mock)

## 0.5.7

### Added
- **Dep enrichment from source manifest** — test validator merges deps from source Cargo.toml when model omits them from `## Imports`. Prevents compile failures (e.g., `serde_json` missing). Also upgrades name-only deps (`tokio = "*"`) with manifest specs (`tokio = { version = "1", features = ["full"] }`)
- **RULE 13: custom_instructions override source** — when custom_instructions contradict source code comments, the model follows custom_instructions. Section headers explicitly signal override priority (security rules excluded)
- **Conflict notes diagnostic channel** — model can append `<!-- SKILLDO-CONFLICT: description -->` when it detects contradictions. Logged at INFO level, stripped before security scan and normalizer. Zero-risk diagnostic for pipeline debugging
- **API Reference completeness check** — VERIFY checklist requires scanning code examples and ensuring each library-owned method has an API Reference entry
- **llmposter integration tests** — 3 tests using llmposter v0.4 (crates.io) as mock LLM backend: basic completion, fixture matching (simulates pipeline stages), 429 error handling

### Changed
- API Reference cardinality: removed 10-15 item cap; now covers all library-owned methods used in examples plus up to 5 additional high-value APIs
- Custom instructions override style/content rules only; RULE 8 (Security) is explicitly non-overridable
- Conflict notes stripped before fence unwrapping at all 4 sanitization sites (initial create + 3 rewrite paths)

### Fixed
- Extract prompt softens test-only usage signal for public API identification (CodeRabbit)
- Conflict marker renamed from `<!-- CONFLICT: -->` to `<!-- SKILLDO-CONFLICT: -->` to avoid collisions with legitimate HTML comments
- Normalizer test used wrong prefix, making assertion trivially true (Greptile P1)
- Stale doc comment referenced old `<!-- CONFLICT: -->` prefix
- Removed redundant `extract_conflict_notes()` from normalizer (generator already handles it)
- Security audit exception for RUSTSEC-2023-0071 (rsa timing sidechannel, dev-dep only via llmposter → oauth-mock → rsa)

### Findings (A/B testing: 12 sonnet runs + 8 gpt-oss runs + 2 opus runs)
- **Sonnet 4.6**: 5 consecutive Greptile 5/5. 100% test pass rate (12/12 runs). Reliable for production
- **Opus 4.6**: Unreliable — lint loops, crashes, prompt injection content. Dep enrichment fixed test compilation but instability persisted. Stopped testing after sonnet hit 5/5
- **gpt-oss-120b (Cerebras)**: Greptile 3-4/5. 12.5% test pass rate (1/8 runs). 6x faster but inconsistent code quality
- **GLM 4.7 (Cerebras)**: Dead — lint loops + LLM call failures. Can't sustain multi-stage pipeline
- **Key insight**: models consistently put only 2/10 deps in `## Imports` — dep enrichment is critical. Source code comments override custom_instructions — RULE 13 + conflict notes address this

## 0.5.6

### Added
- `--debug-stage-files DIR` flag dumps each pipeline stage's raw output for diagnosis
- Behavioral semantics extraction in learn stage — discovers observable behaviors (error codes, side effects, edge cases)
- Review stage receives behavioral_semantics for completeness verification — flags missing behavioral coverage
- Token usage logging at debug level for all 4 providers (Anthropic, OpenAI, Gemini, ChatGPT)
- Extract prompt warns against inferring methods from doc comments
- Review checks API Reference descriptions against custom_instructions for consistency
- "Darryl" review persona for more thorough defect detection

### Fixed
- Normalizer strips unclosed markdown fence wraps (Sonnet CLI pattern)
- Normalizer strips duplicate frontmatter when LLM prepends preamble text
- Normalizer strips trailing AI review notes with fence-aware scanning
- Normalizer ordering: duplicate frontmatter stripped before meta-text
- Go code extractor refactored to use `find_fenced_blocks()` with tag priority
- Test agent strips `optional = true` from structured deps in temp Cargo.toml
- Rust create hint no longer contradicts custom_instructions on import paths
- `max_tokens = 0` in config omits the field from API requests; Anthropic fast-fails with clear error
- Gemini `generationConfig` serialized as camelCase per API spec (was snake_case)
- GPT-5 model detection normalizes provider-prefixed names (e.g., `openai/gpt-5.1`)
- Behavioral semantics extractor handles double-backslash escapes and prose mentions of key
- OpenAI-compatible usage logs correctly attributed (not hardcoded "openai")
- Hallucination cross-reference rule conditional on API surface presence
- `completeness` added to verdict schema categories
- Review fix prompt includes API surface for accuracy and bans AI commentary
- Stale mock review trigger updated for Darryl prompt

### Changed
- Extract/map/learn prompts language-gated: Python-specific patterns moved to python_hints()
- Rust-specific public API detection, deprecation signals, and async hints added
- Go, Java public API detection and deprecation hints added
- Create prompt explicitly bans AI self-commentary and process notes
- Review prompt checks for leaked AI commentary as consistency error
- Unused imports rule: import statements flagged, dependency declarations exempt
- Test failure summary names which patterns failed
- Async runtime hint is runtime-agnostic (not tokio-specific)
- Extract prompt references only source-visible signals for publicity scoring

## 0.5.5

### Changed
- Review prompt: unused imports and speculative future versions no longer trigger false error-severity retries. Reduces review loop from ~4 retries to ~1 for typical libraries.
- Mutex locks in all 5 code generators use `lock_or_recover()` shared helper instead of `.unwrap()` — recovers from poisoned mutex instead of crashing.
- Poison recovery tests deduplicated into `poison_recovery_tests!` macro — shared across all 5 language code generators (was copy-pasted in 4, missing in Java).

## 0.5.4

### Added
- Java e2e test in CI matrix using google/gson v2.12.1

### Changed
- `skilldo auth logout` now prints confirmation messages to stdout (was only visible with `RUST_LOG=info`)
- CLI output functions refactored for testability (`status_to`, `logout_to`, `write_results`, `write_security_scan`, `write_review_output`)

### Fixed
- Rust container validate() now uses the container executor instead of bare-metal CargoExecutor — fixes setup/run/cleanup mismatch where container_name was None
- Output file write uses `NamedTempFile::persist()` for cross-platform safety (Windows `fs::rename` fails if dest exists)
- Go module name sanitized before shell interpolation in container executor

### Notes
- Java e2e verified with both Cerebras `gpt-oss-120b` (20s, paid key) and `qwen-3-235b-a22b-instruct-2507` (259s, free key). Both produce clean SKILL.md output.

## 0.5.3

### Added
- Container local-mount support for all 5 languages — Go, JavaScript, Java, and Rust now wire `/src` into container import resolution (previously Python-only)
- Rust container mode enabled end-to-end — validator no longer falls back to bare-metal. Container generates Cargo.toml with deps and uses `cargo run`
- Rust container registry mode — generates Cargo.toml with deps even without local source (was silently using rustc which can't resolve deps)
- Tilde fence (`~~~`) support in linter — code block detection, duplicate example check, and markdown-wrap check all handle both fence types
- CommonMark-compliant fence length matching — closing fences must match or exceed opener length, trailing info text rejected
- `CARGO_HOME` isolation in Rust container scripts

### Changed
- Pre-push hook uses `--quiet` flag to prevent pipe overflow with 2500+ tests
- Java container jar copy checks both `target/` and `build/libs/` independently (was `elif`)
- Container Java local-mount filters local artifact from Maven POM deps (prevents classpath collisions)
- npm dep filtering strips `@version` specifiers and handles scoped packages (`@scope/pkg@^1.0`)
- Go module name extraction strips trailing `//` comments
- Rust container dep names strip version constraints and brackets for valid TOML keys

### Fixed
- Process group kill on timeout (Unix) — commands spawn in their own process group, SIGKILL sent to entire group on timeout
- Security: `aws-lc-sys` 0.37.1 → 0.39.0 (RUSTSEC-2026-0048), `rustls-webpki` 0.103.9 → 0.103.10 (RUSTSEC-2026-0049)

## 0.5.2

### Added
- `JavaHandler::get_artifact_id()` — returns actual artifact name instead of Gradle group namespace for accurate local-install dep filtering
- `parse_gradle_archives_base_name()` — parses `archivesBaseName`, `archivesName`, and `base.archivesName.set()` from Gradle build files
- Symlink traversal guard — `filter_within_boundary()` canonicalizes collected file paths and rejects any escaping the repo root. Applied to all 5 ecosystem handlers.
- Windows-safe atomic write — `write_atomic()` uses `tempfile::NamedTempFile::persist()` instead of `fs::rename`

### Changed
- Default model updated from `claude-sonnet-4-20250514` (Sonnet 4) to `claude-sonnet-4-6` (Sonnet 4.6)

### Fixed
- POM license parser off-by-one — section slice now includes the closing `</licenses>` tag
- Device code polling interval floor at 1 second to prevent busy-spin
- Workspace-inherited Rust features now union correctly (was replacing)

## 0.5.1

### Added
- Local-install support for all 5 languages (Python, Go, JavaScript, Java, Rust) — bare-metal executors exclude local package from registry deps
- Structured Rust dependency pipeline — `StructuredDep` preserves raw TOML specs (versions, features, git refs) end-to-end through collection → prompts → parsing → execution
- Rust update mode now receives structured deps for the `[dependencies]` block
- 150+ new coverage tests across executor, validator, collector, rust_parser, yara

### Changed
- Executor helper functions extracted from async methods into standalone testable functions
- `append_rust_deps_section()` shared between create and update prompts (DRY)

### Fixed
- Python local-install excludes local package from `pyproject.toml` deps (PEP 503 normalization)
- Cargo path dep preserves features/default-features from raw_spec
- Go module matching excludes major version paths (`/v2`, `/v3`)
- Workspace dep resolution unions child features instead of replacing
- `LocalMount` keeps container mode (only `LocalInstall` triggers bare-metal fallback)
- Review `malformed` flag no longer discards valid issues when `passed` field has wrong type
- Dash/underscore normalization in Rust parser structured dep dedup
- Multiple TOML fences in `## Imports` now all contribute specs

## 0.5.0

- Added Java language ecosystem support — full pipeline with Maven and Gradle projects
- `JavaHandler`: file discovery, metadata extraction from pom.xml and build.gradle/build.gradle.kts
- `JavaParser`: import and Maven coordinate extraction from SKILL.md
- `JavaCodeGenerator`: Main.java generation with pattern wrapping
- `JavaExecutor`: bare-metal (javac+java) and container (maven:3-eclipse-temurin-21-alpine)
- Package name detection: handles parent POMs (-parent suffix), Gradle constants (falls back to settings.gradle), -root suffix stripping
- Java prompt hints for all pipeline stages
- 150+ new tests for Java ecosystem
- Fixed re-test logic when review passes after test-breaking rewrite
- Re-run tests when review passes but `last_review_tests_passed` is false

## 0.4.2

- Removed review introspection (Phase A) — the test agent's feedback loop already validates correctness, making container introspection redundant. Simplifies the review agent to LLM-verdict-only for all languages. (-1042 lines)
- Removed `--no-container` flag from `skilldo review` (no longer needed)
- Removed `review_degraded` from telemetry CSV schema (no introspection = no degraded state)
- Removed `--runtime` and `--timeout` flags from `skilldo review` (were only used by introspection)
- Added `skilldo skill` command — prints the embedded SKILL.md for AI assistants
- Added `skilldo completion <shell>` — generates shell completions for bash, zsh, fish, elvish, powershell
- Added `docs/` directory — architecture, configuration, authentication, languages, telemetry, best-practices
- Slimmed README to sales pitch + quick start + links to docs
- Implemented custom Debug for TokenSet and OAuthEndpoint — redacts secrets in log output
- Hardened error handling: descriptive retry fallback, client_id validation, readable HTTP error bodies
- Set 0o700 permissions on `~/.skilldo/` telemetry directory
- Fixed sticky `has_unresolved_errors` — review loop now clears error state when final review+test cycle succeeds after earlier failures. Fixes false exit-1 on successful generation runs.
- Fixed CSV header migration to respect RFC 4180 quoting and per-row column counting
- OAuth scope merging — `group_by_oauth_app` now unions scopes across all endpoints sharing the same app
- Coverage: 1913 tests, ~97.5% line coverage

## 0.4.1

- Added hard-error guard for `install_source = "local-install"` / `"local-mount"` on non-Python languages — previously silently did nothing useful, now fails early with a clear error message
- Added `review_degraded` field to `ReviewResult`, `GenerateOutput`, and `RunRecord` — propagates degraded introspection status through to telemetry CSV and structured log output
- Structured log status now three-valued: "errors" / "degraded" / "ok" (was binary "errors" / "ok") — CI consumers can distinguish grounded vs advisory reviews
- Added tests for review degraded propagation, telemetry CSV formatting, auth token error handling, factory API key edge cases, security boundary helpers
- Added `migrate_header_if_stale` for CSV telemetry — transparently upgrades old `runs.csv` headers when new columns are added (e.g., `review_degraded`)
- Added atomic write for CSV header migration — prevents data loss if process killed mid-write
- Fixed install-source guard to skip when test agent is disabled (`--no-test` / `enable_test = false`)
- Fixed `review_degraded` accumulation order in review loop — degraded state preserved on malformed verdict retries
- Coverage: 1895 tests, 97.9%+ line coverage

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


