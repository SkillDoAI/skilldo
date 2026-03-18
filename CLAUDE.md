# CLAUDE.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### Enumerate Input Variants

When writing match, filter, or comparison logic, **enumerate every input variant and trace each through the code path before committing**. This is non-negotiable — "works for the common case" is not enough.

Concretely:
- **String matching**: Ask "what other strings could appear here?" A `starts_with` check must consider partial prefix collisions (`foo` matching `foobar`). An `==` check must consider versioned variants (`foo/v2`).
- **Cross-ecosystem logic**: If code handles multiple project layouts (Maven vs Gradle, pom.xml vs settings.gradle), trace each layout through the function and verify the output is correct for all of them — not just the one you tested.
- **Platform paths**: If a path is interpolated into a config format (TOML, JSON, YAML), verify it works on Windows (`\` vs `/`), with spaces, and with special characters.
- **Identity checks**: If you're comparing a "package name" against structured data (Maven coordinates, Go module paths), verify what the name function actually returns for each project type and whether the comparison still holds.

The pattern to avoid: implementing the happy path, getting green tests, and moving on without asking "what inputs would make this wrong?"

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.

---

## Project-Specific Rules

### No Hardcoding in Rust
- **No language-specific logic** outside of parser modules. Parsers (`src/test_agent/`, `src/ecosystems/`) are the ONLY place for language-specific handling.
- **No hardcoded package names** in Rust code. Package names come from SKILL.md frontmatter (`name:` field) or config. If guidance is needed, put it in LLM prompts, not Rust.
- Keep Rust code generic and data-driven. Language/package knowledge belongs in prompts and config.

### Environment
- API keys: `source ~/.openai` and `source ~/.anthropic` (both are `export VAR=value` format)
- Container runtime: podman

### Project Structure
- 6-agent pipeline: **extract** → **map** → **learn** → **create** → **review** → **test**
  - review loops back to create on failures (same pattern as test → create)
  - review = accuracy (verify dates, signatures, consistency) + safety (prompt injection, nefarious code)
- Key dirs: `src/pipeline/`, `src/test_agent/`, `src/llm/`, `src/cli/`, `src/auth/`
- Config: TOML files, reference at `dev/configs/`
- CI: `.github/workflows/ci.yml` (tests + e2e), `release.yml` (auto-tag + homebrew)

---

## Git & PR Workflow

- **Squash merges**: User always squash-merges PRs. Rebase onto `origin/main` before pushing new PR branches to avoid conflicts.
- **Conventional commits**: Required. Pre-commit hook enforces format.
- **NEVER `git add -A` or `git add .`** — always stage files by name. Review untracked files before staging. Local docs (BACKLOG.md, BUILD.md, CLAUDE.md, AUDIT-PROMPT*.md) and dev artifacts must not be committed without explicit user approval.
- **Always create PRs as drafts** (`gh pr create --draft`). This prevents CodeRabbit from reviewing before the PR is clean. Mark ready for review only when the diff is final.
- **Commit/push timing** — weekdays during working hours (~9a-5p): ask before committing and pushing. Weekends and evenings: can commit and push freely, especially if user has given permission for the session. User may also grant blanket permission to open PRs or even merge if there are no greptile/coderabbit/coverage/audit issues.
- **Bedtime mode** — When the user says something like "do you remember how we worked last night?" or explicitly grants autonomous mode: push, open draft PR, run all 5 audit scripts, fix findings, iterate with reviewers, mark ready for review — all without asking. Only stop to ask if something is genuinely blocked. This is the default for evening/weekend sessions once granted. Also hunt for 10-20 lines of extra test coverage across the codebase to push unit test coverage as high as possible within reason.
  - **Monitor reviewers in a loop** — don't wait to be prompted. Run a background loop checking Greptile score, CodeRabbit status, and new inline comments every 5 minutes. Fix findings as they come in. Break when Greptile hits 5/5, CodeRabbit passes, and no new unaddressed comments exist.
  - **Push coverage proactively** — after any significant code addition, run `cargo llvm-cov` and launch coverage agents for files below 97%.
- **No git worktrees** — worktrees set `bare = true` on the main repo and break pre-commit/pre-push hooks. Just branch from main in the normal working directory.
- **No force pushes** — repo has force push protection. Always use regular `git push`, never `--force` or `--force-with-lease`.

---

## Quality Gates & Reviews

- **Always keep test coverage above 95%**. Never let new code drop coverage below 95%. Target 98% as the running average — push coverage proactively after adding new code.
- **Greptile confidence score must be 5/5** — aim for maximum Greptile review confidence on every PR.
- **"The 4 Horsemen" / "audit"** — Run ALL five reviewers on the uncommitted diff BEFORE committing: `/simplify` (reuse/quality/efficiency), Codex (architecture/security), Gemini (output quality/prompts), CodeRabbit (nits/style), Claude (`dev/scripts/run-claude-audit.sh` — Rust/security deep dive). User may say "roll the 4 horsemen" or "run an audit" — same thing, all 5 run. Fix P1/P2 findings before commit.
- **Reply to review nits inline** — NEVER post standalone PR comments summarizing nit responses. Reply directly in the review thread where the comment lives. Always tag the bot by handle so it sees the reply and can resolve the thread: `@coderabbitai` for CodeRabbit, `@greptile-apps` for Greptile. Do this for ALL review bots, every time, even when the bot might pick it up without the tag.
- **Check for follow-up comments** — bots reply to your fixes with follow-ups, new concerns, or verification results. After replying to a thread, monitor it for bot responses. Greptile updates its summary comment in-place (check "Comments Outside Diff" section). CodeRabbit posts analysis chains that may reveal new issues. Use GraphQL `reviewThreads` query to find ALL unresolved threads — REST API paginates at 30 and misses outdated-diff threads.
- **Audit scripts** at `dev/scripts/`: `run-gemini-audit.sh`, `run-codex-audit.sh`, `run-coderabbit.sh`, `run-claude-audit.sh`. Not committed. Run these directly — they are logged in and don't need user intervention.
- **Update docs on every release** — README.md, CHANGELOG.md, SKILL.md, and all `docs/*.md` files. Docs are a first-class deliverable. When behavior changes (new flags, new stages, changed defaults), update the corresponding doc file. Check before every commit.
- **CHANGELOG.md is mandatory** — every PR gets a changelog entry, even QoL branches that don't bump the version. Use "## Unreleased" or the target version heading. Include what was added, changed, removed, and fixed. This is the user-facing record of what shipped.
- **Doc accuracy sweep** — before every release, verify `docs/` content matches actual code behavior. Pay special attention to: architecture.md (pipeline stages, review phases), configuration.md (TOML fields, defaults), languages.md (supported ecosystems, validation details), SKILL.md (embedded skill — version, commands, flags, license, telemetry schema). Don't document aspirational behavior as current — if it's not implemented, say "planned" or don't mention it.
- **SKILL.md is compiled into the binary** — via `include_str!`. If it's wrong, every AI agent using `skilldo skill` gets bad instructions. Treat it like code, not like a README. Verify it on every release.
- **Pipeline diagram in two places** — README.md and docs/architecture.md both contain the ASCII pipeline diagram. Keep them in sync when the pipeline changes.

---

## Model Preferences

User is strict about this — burned money on wrong model choices.

- "sonnet" = `claude-sonnet-4-6` (Sonnet 4.6) until further notice
- "openai" = ChatGPT 5.2, or 5.3 if available
- "frontier model" = latest available model for that provider
- NEVER default to older models like `claude-sonnet-4-20250514` (Sonnet 4)

---

## SKILL.md Rules

- **Never fix SKILL.md files manually** — quality issues in generated skills should be fixed via code (normalizer, linter) or prompts, not by hand-editing. The goal is pipeline improvements that prevent issues on future generations.
- `--version` is the skilldo binary version, NOT a library version override. Custom instructions is the escape hatch for unusual versioning.

### Known LLM Output Patterns
- Sonnet 4.6 wraps SKILL.md body in ` ```markdown ` fences after frontmatter — handled by `strip_body_markdown_fence()` in normalizer
- Truncated outputs leave unclosed code blocks — handled by `fix_unclosed_code_blocks()` in normalizer

---

## Testing Conventions

### TDD Workflow
- Always write failing tests BEFORE implementation
- Use AAA pattern: Arrange-Act-Assert
- One assertion per test when possible
- Test names describe behavior: "should_return_empty_when_no_items"

### Test-First Rules
- When I ask for a feature, write tests first
- Tests should FAIL initially (no implementation exists)
- Only after tests are written, implement minimal code to pass

### Running Tests
- This is a Rust CLI project. Always use `cargo test`, `cargo clippy`, and `cargo build` for validation.
- When fixing tests, run the full test suite afterward to check for test pollution or regressions.
- Always run tests after writing them — including integration tests (`cargo test -- --ignored`). Never assume tests pass without verifying.

### Smoke-Testing LLM Providers
- Use `skilldo hello-world --config <path>` to test any provider config (hidden command, not in `--help`).
- It loads the config, creates a client via the factory, sends one prompt, and prints the response — fastest way to verify a provider works end-to-end.
- Works with all provider types including `model_type = "cli"`.
- Preferred model: `gemini-3-pro-preview` for Gemini CLI tests (use `gemini-3-flash-preview` if pro is rate-limited).

## Debugging

When debugging, always check debug logs and error output the user points to FIRST before doing broad exploration. Do not ask clarifying questions when the user has already provided specific files or logs to examine.

## Working Style

When the user asks you to test or validate something, start executing immediately. Do not spend time exploring the codebase or asking clarifying questions unless truly blocked.

- **MR = PR** — user may say "MR" (merge request) when they mean "PR" (pull request), unless clearly used in another context.

## Dependencies

When installing or depend on external packages (npm, cargo, etc.), always pin to specific known-working versions. Never use `@latest` for dependencies in hooks or MCP servers without verifying compatibility first.

---

## Housekeeping

- **Check disk space periodically** — if running low, clean up /tmp files, stale podman/docker containers, and unneeded Ollama models.
- **skilldo.toml is reusable** — one config can be used across repos. For batch runs, point `--config` at a shared config file rather than copying per repo.

## Known Gotchas

- **CI temp scripts**: Shell scripts in /tmp fail under LLVM coverage on Linux (noexec). Don't use temp shell scripts in tests that run under cargo-llvm-cov.

## Reference: ChatGPT OAuth (v0.3.x scope)

- Codex CLI uses `chatgpt.com/backend-api/codex/responses` (NOT `api.openai.com/v1/chat/completions`)
- Requires `ChatGPT-Account-ID` header extracted from JWT claims
- Uses OpenAI Responses API format (not Chat Completions) — different request/response shape
- Token scopes `openid profile email offline_access` work for backend API, NOT for public `/v1/` API
- Future `chatgpt` provider_type would need a Responses API client

