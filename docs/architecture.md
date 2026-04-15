# Architecture

Skilldo reads a library's source directory and runs a 7-stage pipeline to extract knowledge and synthesize it into a single `SKILL.md` file:

```text
Source Code ──→ Extract (API Surface)       ──┐
Test Files  ──→ Map (Pattern Extraction)    ──┤──→ Fact Ledger ──→ Create ──→ Review ──→ Test ──→ SKILL.md
Docs/README ──→ Learn (Context Extraction)  ──┘                     ↑          ↓         ↓
                                                                    │       failed?   failed?
                                                                    │          ↓         ↓
                                                                    ←── feedback ←───────┘
```

Three agents (Extract, Map, Learn) gather information from the source code in parallel, then the Fact Ledger distills their output into a compact truth table before Create combines everything into a SKILL.md. Review and Test validate the result — if either fails, the error feedback loops back to Create for regeneration, retrying up to a configurable limit.

1. **Collect** — Discovers source files, tests, documentation, and changelogs from the local directory
2. **Extract** — Three stages work in parallel to pull out the API surface, usage patterns, and conventions/pitfalls
3. **Fact Ledger** — Extracts a compact truth table with negative assertions from stages 1-3, fed into Create as the highest-salience constraint
4. **Create** — Combines everything into a formatted SKILL.md
5. **Review** — Verifies accuracy (dates, signatures, consistency) and safety (prompt injection, nefarious content)
6. **Test** — Generates test code from the patterns and runs it to verify correctness
7. **Iterate** — If review or test fails, feedback loops back for regeneration (configurable retries)

The output is a structured Markdown file with YAML frontmatter, ready to drop into any repository.

## The Agents

| Stage | Role | What It Does |
|-------|------|-------------|
| **Extract** | API Surface | Reads source code to identify public functions, classes, methods, parameters, return types, and deprecations |
| **Map** | Usage Patterns | Reads test files to extract real-world usage examples showing how the library is actually used |
| **Learn** | Conventions & Pitfalls | Reads docs, README, and changelogs to find common mistakes, migration notes, and best practices |
| **Fact Ledger** | Truth Extraction | Distills extract/map/learn outputs into a compact truth table of positive and negative assertions — the highest-salience constraint for Create |
| **Create** | SKILL.md Composition | Combines output from extract/map/learn plus the fact ledger into a structured SKILL.md with sections for imports, patterns, pitfalls, and more |
| **Review** | Accuracy & Safety | Verifies dates, API signatures, and content consistency; checks for prompt injection, destructive commands, and credential leaks |
| **Test** | Code Validation | Generates runnable test code from the SKILL.md patterns and executes it to verify they actually work |

Test is optional but recommended. When enabled, it catches hallucinated APIs, wrong parameter names, and broken code examples before they ship. If validation fails, Create regenerates with the error feedback.

## Pipeline Flow

Stages 1-3 (Extract, Map, Learn) run in parallel by default. Use `--no-parallel` for local models that can't handle concurrent requests. The Fact Ledger runs after stages 1-3 complete and before Create.

The Create → Review → Test loop is sequential:
- **Create** generates the SKILL.md
- **Review** checks accuracy and safety. If it finds issues, Create regenerates.
- **Test** generates and runs code. If tests fail, Create regenerates with error feedback.

Both Review and Test can be disabled (`--no-review`, `--no-test`) for faster iterations.

### System Prompt Split

The Create and Review stages use a split prompt architecture: instructions (formatting rules, structural requirements, persona) are sent through the native system prompt channel, while data (extracted API surface, patterns, learn context, fact ledger) is sent in the user message. This separation leverages the model's built-in distinction between system-level directives and user-level content, improving instruction adherence and reducing prompt anchoring effects.

For CLI providers, the `cli_system_args` config field controls how the system prompt is passed to the CLI (e.g., `--system-prompt-file` for Claude CLI). The system prompt is written to a temp file and the file path is passed as the argument, avoiding exposure of prompt content via `ps aux`. See [Configuration](configuration.md#cli-provider-mode).

### No-Test Fallback

When a library has no test files or examples, the Map stage falls back to extracting usage patterns from documentation (README, guides) and source code doc comments instead of sending an empty prompt. This allows SKILL.md generation for libraries without test suites.

### Native Dependency Detection

After file collection, the pipeline checks for native/C dependency indicators:
- **Rust**: `-sys` crates (including renamed via `package =`), `build.rs`, `links` field. For workspace roots, scans all member crates.
- **Go**: `import "C"`, `#cgo LDFLAGS/CFLAGS` (recursive scan, line-based to avoid false positives)
- **Python**: `ext_modules`, `cffi_modules`, maturin, pyo3
- **JavaScript**: `binding.gyp`, `node-gyp`, `@napi-rs` (in all dep sections including optional/peer)

When detected and `--container` is not set, a warning suggests using container mode for reliable test execution.

### Secret Redaction

The test agent redacts configured environment variable values from all output (stdout, stderr, logs) to prevent secret leakage. Both bare-metal and container executors apply redaction. Container execution also redacts `extra_env` values passed via config, which may not be in the host process environment.

## Review Agent

The review agent evaluates the SKILL.md for accuracy and safety using an LLM verdict. It checks for incorrect API signatures, wrong version numbers, hallucinated features, and security issues (prompt injection, destructive commands, credential leaks).

Since v0.5.6, the review receives the extract stage's API surface as ground truth context. Methods documented in the SKILL.md that do not appear in the extracted API surface are flagged as hallucinations. This cross-reference catches invented methods that LLMs sometimes generate based on plausible API patterns.

The review also receives behavioral semantics from the learn stage — observable behaviors such as error codes, side effects, and edge cases discovered from documentation and changelogs. The review cross-references these against the SKILL.md to flag missing behavioral coverage (e.g., documented error codes that the skill never mentions).

The review uses a "Darryl" persona — a meticulous, slightly adversarial reviewer designed to catch defects that a more agreeable persona would wave through. This produces more thorough defect detection, particularly for subtle consistency issues like leaked AI commentary and descriptions that contradict custom_instructions.

If the review fails, error feedback is sent back to the Create stage for regeneration.

## Test Execution

The test agent runs generated code via bare-metal executors or containers. On Unix, each command spawns in its own process group. If execution exceeds the configured timeout, SIGKILL is sent to the entire process group, ensuring that child processes (compilers, package managers, language runtimes) are cleaned up and don't leak.

## Security Scanner

Three-layer scanning runs during the review stage:

1. **Regex patterns** — credential access, destructive commands, exfiltration URLs, reverse shells, obfuscated payloads
2. **Prompt injection detection** — instruction overrides, identity reassignment, secrecy demands, indirect injection
3. **YARA rules** — 24 SkillDo rules (SD-001 to SD-211) + 17 vendored [Cisco AI Defense](https://github.com/cisco-ai-defense/skill-scanner) rules (Apache 2.0)

YARA rules are evaluated at runtime via [boreal](https://github.com/vthib/boreal), a pure Rust YARA engine. All SkillDo and Cisco rules ship in `rules/`.

Security scanning is code-block-aware: prose-only rules skip matches inside fenced code blocks, since code examples legitimately contain patterns like `os.remove()` or `shutil.rmtree()`.

Each finding carries a `FindingRouting` — either `Definitive` (pattern match is conclusive) or `NeedsReview` (high false-positive rate, should be confirmed by LLM review). Prose-only YARA rules (SD-201 dynamic code execution, SD-202 credential access, SD-204 persistence) are tagged `NeedsReview`.

For API client SDKs that inherently discuss API keys and credentials, set `security_context = "api-client"` in the config to suppress SD-202 (credential store access) false positives.

## Model Communication

The model can communicate uncertainty and conflicts back to the pipeline via HTML comments:

- `<!-- SKILLDO-CONFLICT: description -->` — conflicts between custom_instructions and source data
- `<!-- SKILLDO-UNVERIFIED: description -->` — APIs or behaviors the model discovered but couldn't fully verify from the provided source code

All `<!-- SKILLDO-* -->` comments are stripped from the final output and logged for debugging. CONFLICT notes log at `info` level, UNVERIFIED notes at `warn`. Use `RUST_LOG=info` or `RUST_LOG=debug` to see them.

This mechanism encourages accuracy over completeness — the model is instructed that a hallucinated API detail is 3x worse than a missing one, and to flag uncertainty rather than guess.
