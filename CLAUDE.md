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
- **No git worktrees** — worktrees set `bare = true` on the main repo and break pre-commit/pre-push hooks. Just branch from main in the normal working directory.
- **No force pushes** — repo has force push protection. Always use regular `git push`, never `--force` or `--force-with-lease`.

---

## Quality Gates & Reviews

- **Always keep test coverage above 95%**. Never let new code drop coverage below 95%.
- **Greptile confidence score must be 5/5** — aim for maximum Greptile review confidence on every PR.
- **"The 4 Horsemen" / "audit"** — Run ALL four reviewers on the uncommitted diff BEFORE committing: `/simplify` (reuse/quality/efficiency), Codex (architecture/security), Gemini (output quality/prompts), CodeRabbit (nits/style). User may say "roll the 4 horsemen" or "run an audit" — same thing. Fix P1/P2 findings before commit.
- **Reply to review nits inline** — NEVER post standalone PR comments summarizing nit responses. Reply directly in the review thread where the comment lives. For CodeRabbit nits, prefix reply with `@coderabbitai` so the AI picks it up and resolves the thread. For Greptile, reply in the inline thread.
- **Audit scripts** at `dev/scripts/`: `run-gemini-audit.sh`, `run-codex-audit.sh`, `run-coderabbit.sh`. Not committed. Run these directly — they are logged in and don't need user intervention.
- **Update docs on every release** — README.md (ToC, config reference, feature descriptions), CHANGELOG.md (flat bullet list per version, matches GitHub release body). Docs are a first-class deliverable. Check before every commit.
- **Pipeline diagram in two places** — README.md and BEST_PRACTICES.md both contain the ASCII pipeline diagram. Keep them in sync when the pipeline changes.

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

