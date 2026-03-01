# Best Practices

Tips for getting the most out of skilldo-generated SKILL.md files.

## How the Pipeline Works

Skilldo runs a 6-stage pipeline that reads your library's source code, tests, and docs, then synthesizes everything into a structured SKILL.md:

```text
Source Code ──→ Extract (API Surface)       ──┐
Test Files  ──→ Map (Pattern Extraction)    ──┤──→ Create ──→ Review ──→ Test ──→ SKILL.md
Docs/README ──→ Learn (Context Extraction)  ──┘      ↑          │         │
                                                     └──────────┴─────────┘
                                                      (retry on failure)
```

1. **Extract**, **Map**, and **Learn** run in parallel to gather API signatures, usage patterns, and conventions
2. **Create** combines everything into a formatted SKILL.md
3. **Review** verifies accuracy (dates, signatures, consistency) and safety (prompt injection, nefarious content)
4. **Test** generates runnable code from the patterns and executes it in a container
5. If Review or Test fails, feedback loops back to Create for regeneration (up to `max_retries`)

## What to Expect

Generation gets you **90-95%** of the way to a production-quality SKILL.md. The pipeline extracts APIs, patterns, pitfalls, and migration notes automatically — but LLMs are imperfect. Common issues in generated output:

- **API signatures**: Parameter defaults, return types, or parameter order may be slightly off
- **Migration sections**: Version direction can be wrong (e.g., "migration from v2.11" when the skill targets v2.10)
- **Code examples**: Occasionally use invalid syntax or hallucinated API calls
- **Security CVEs**: Referenced generically instead of by specific CVE number

The **test stage** catches many of these by actually running the code examples in a container. Enable it when possible.

## Model Selection

Quality varies significantly by model. Choose based on your needs:

| Goal | Recommendation |
|------|---------------|
| Best quality | GPT-5.2 or Claude Sonnet for all stages |
| Best value | Hybrid: local model for extract/map/learn/create, cloud model for review+test |
| Free / offline | Qwen3-Coder 30B via Ollama (expect more retries, slower) |
| Cost-sensitive CI | GitHub Models free tier for extract/map/learn, cloud for create+review+test |

The **create**, **review**, and **test** stages benefit most from strong models. The **extract**, **map**, and **learn** stages handle well with smaller models since they're doing structured extraction, not creative synthesis.

## Prompt Customization

Every stage accepts custom prompt text via config. This is the primary way to improve output quality for specific libraries or domains.

```toml
[prompts]
# "append" adds your text after the built-in prompt (default)
# "overwrite" replaces the built-in prompt entirely
extract_mode = "append"
extract_custom = "Focus on the async API surface. Ignore legacy sync wrappers."

create_mode = "append"
create_custom = "Always include type annotations in code examples. List CVEs by number in migration sections."
```

### What to Customize

- **extract_custom**: Guide what API surface to prioritize (async vs sync, public vs internal)
- **map_custom**: Steer which test patterns to extract (integration tests, unit tests, specific fixtures)
- **learn_custom**: Focus on specific docs sections (migration guides, security advisories, changelogs)
- **create_custom**: Control output formatting, section emphasis, code style
- **review_custom**: Add domain-specific accuracy checks
- **test_custom**: Adjust test generation strategy (mock vs real, import-only vs functional)

### Tips

- Start with `append` mode — the built-in prompts handle the structure, yours handles the specifics
- Use `overwrite` only if you need fundamentally different output structure
- Test prompt changes on a small library first before running on your target

## Reviewing Generated Output

Always review before shipping. Here's a quick checklist:

1. **Frontmatter**: Correct name, version, license?
2. **Imports section**: Only public API imports? No `_internal` or `_compat` modules?
3. **API signatures**: Spot-check 2-3 against official docs
4. **Code examples**: Do they look syntactically valid? Would you copy-paste them?
5. **Pitfalls**: Are the Wrong/Right examples actually wrong/right?
6. **Migration notes**: Correct version direction? Accurate breaking changes?
7. **References**: Do the doc URLs point to real pages?

The built-in **linter** (`skilldo lint`) catches structural issues automatically. The **review stage** checks for accuracy and safety. But human review is still the final gate.

## Iterating on Quality

If generation produces poor output for a specific library:

1. **Check the source directory** — does it have good tests, docs, and a changelog? More source material = better output
2. **Try a stronger model** — switch from local to cloud for the create stage
3. **Add custom prompts** — guide the LLM toward what matters for this library
4. **Increase retries** — `max_retries = 10` gives more chances to self-correct
5. **Use update mode** — `skilldo generate -i SKILL.md -o SKILL.md` refines an existing file

## Community

Found a prompt tweak that dramatically improves output for a class of libraries? We'd love to hear about it:

- **Open an issue** with the prompt text, what library you tested on, and the before/after quality difference
- **Tag it `prompt-improvement`** so we can track patterns across the community
- Over time, we'll incorporate the best generic improvements into the built-in prompts

The goal is a feedback loop: community finds what works → best ideas get built in → everyone benefits.

## Known Limitations

- **API signature accuracy** is the #1 issue. LLMs hallucinate parameter defaults and return types. The review stage helps, but grounded verification (running `inspect.signature()` in the test container) is planned.
- **Large libraries** (pytorch, tensorflow, numpy) push token limits. Use `max_source_tokens` to control how much source code is sent.
- **Local models** produce lower quality output and need more retries. This is inherent to model capability, not a pipeline issue.
- **Single ecosystem** — Python is fully supported today. Go is next (v0.2.0). JS/TS and Rust follow after.
