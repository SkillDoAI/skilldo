# Adding a New Language to skilldo

Checklist for adding full ecosystem support for a new language. Each item must be completed and tested before the language is considered "shipped."

## 1. Ecosystem Handler (`src/ecosystems/<lang>.rs`)

- [ ] `<Lang>Handler` struct with `new(repo_path)` constructor
- [ ] `find_source_files()` — discover source files, exclude tests/build output
- [ ] `find_test_files()` — discover test files
- [ ] `find_doc_files()` — discover documentation (README, docs/, etc.)
- [ ] `find_example_files()` — discover examples
- [ ] `get_package_name()` — extract package/crate/module name from manifest
- [ ] `get_version()` — extract version from manifest
- [ ] `get_license()` — detect license (SPDX from manifest or file detection)
- [ ] `get_project_urls()` — extract repository/homepage URLs
- [ ] `get_dependencies()` — extract dependency list from manifest
- [ ] All `find_*` methods call `filter_within_boundary()` for symlink safety
- [ ] Registered in `src/detector.rs` for auto-detection

## 2. Collector (`src/pipeline/collector.rs`)

- [ ] `collect_<lang>()` method reading source, tests, docs, examples within budget
- [ ] Package name detection via `detect_package_name()` for the new language
- [ ] Changelog detection if the ecosystem has a convention

## 3. Test Agent — Parser (`src/test_agent/<lang>_parser.rs`)

- [ ] Implements `LanguageParser` trait
- [ ] `extract_patterns()` — extract code patterns from SKILL.md
- [ ] `extract_dependencies()` — extract deps from SKILL.md imports section
- [ ] `extract_name()` / `extract_version()` — extract metadata from frontmatter

## 4. Test Agent — Code Generator (`src/test_agent/<lang>_code_gen.rs`)

- [ ] Implements `LanguageCodeGenerator` trait
- [ ] LLM prompt generates runnable test code for the language
- [ ] Code fence stripping handles language-specific tags

## 5. Test Agent — Bare-Metal Executor (`src/test_agent/executor.rs`)

- [ ] `<Lang>Executor` struct with `new()`, `with_timeout()`, `with_local_source()`
- [ ] `setup_environment()` — create isolated temp dir, install deps
- [ ] `run_code()` — compile/run test code, classify result
- [ ] `cleanup()` — clean up temp resources
- [ ] Local-install support: exclude local package from registry deps
- [ ] Environment isolation (cache dirs confined to temp)

## 6. Test Agent — Container Executor (`src/test_agent/container_executor.rs`)

- [ ] Container image configured in config defaults
- [ ] `generate_<lang>_install_script()` — install deps inside container
- [ ] Local-mount support: wire `/src` into import/build resolution
- [ ] Local artifact filtering from registry deps (avoid classpath collisions)
- [ ] Run line in `generate_container_script()` for the language

## 7. Validator (`src/test_agent/validator.rs`)

- [ ] Language dispatched in `new()` for both BareMetal and Container modes
- [ ] Local-source wiring for both modes
- [ ] LocalInstall bare-metal fallback for non-Python container

## 8. LLM Prompts (`src/llm/prompts_v2.rs`)

- [ ] `<lang>_hints()` function with stage-specific guidance (extract, map, learn, create)
- [ ] Language dispatched in `language_hints()` for all stages
- [ ] Structured deps injection if applicable (like Rust's `[dependencies]` block)

## 9. CLI Integration

- [ ] Language added to `detector.rs` enum
- [ ] E2E test in `.github/workflows/ci.yml` matrix with a real library
- [ ] `--language <lang>` CLI flag works

## 10. Documentation

- [ ] `docs/languages.md` — language table + subsection with details
- [ ] `docs/configuration.md` — container image config
- [ ] `CHANGELOG.md` entry
- [ ] `SKILL.md` updated if embedded skill references supported languages
- [ ] `BACKLOG.md` ecosystem entry marked as shipped

## 11. Quality

- [ ] Unit tests for all new modules (target 98%+ coverage)
- [ ] Integration tests via `cargo test -- --ignored`
- [ ] `sanitize_dep_name()` handles language-specific dep name patterns
- [ ] Security: YARA rules cover language-specific attack patterns if needed
