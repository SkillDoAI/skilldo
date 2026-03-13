# Telemetry

Skilldo can log generation runs to a local CSV file for tracking model performance and pipeline behavior.

## What We Log

Each run appends one row to `~/.skilldo/runs.csv` with these fields:

| Field | Example | Description |
|-------|---------|-------------|
| language | python | Detected or specified language |
| library | click | Library name from package metadata |
| version | 8.1.7 | Library version |
| model | claude-sonnet-4-6 | Primary LLM model used |
| test_model | gpt-5.2 | Test stage model (if different) |
| review_model | gpt-5.2 | Review stage model (if different) |
| passed | true | Whether generation succeeded |
| retries | 2 | Number of retry attempts |
| duration_secs | 180 | Total generation time |
| skilldo_version | 0.4.1 | Skilldo binary version |
| failure_stage | test | Which stage failed (if any) |
| failure_reason | test_timeout | Failure detail |
| review_degraded | false | Whether review ran in degraded mode |
| timestamp | 2026-03-13T20:00:00Z | ISO 8601 timestamp |

## What We Don't Log

- No source code or file contents
- No API keys or credentials
- No generated SKILL.md content
- No file paths from your system
- No network requests or telemetry sent externally

**All data stays local on your machine.** The CSV file is append-only and never transmitted anywhere.

## Enabling Telemetry

### Via config file

```toml
[generation]
telemetry = true
```

### Via CLI flag

```bash
skilldo generate --telemetry /path/to/repo
```

### Disabling (overrides config)

```bash
skilldo generate --no-telemetry /path/to/repo
```

## Viewing Your Data

The CSV file is plain text — open it in any spreadsheet or query it:

```bash
# View recent runs
cat ~/.skilldo/runs.csv

# Count runs by model
awk -F, '{print $4}' ~/.skilldo/runs.csv | sort | uniq -c | sort -rn

# Find failed runs
grep ',false,' ~/.skilldo/runs.csv
```

## Schema Migration

When skilldo adds new columns (e.g., `review_degraded` in v0.4.1), the CSV header is automatically updated on the next run. Existing data rows are preserved — new columns appear empty for old rows. The migration uses atomic writes to prevent data loss.

## Review Degraded

The `review_degraded` field indicates whether the review agent ran with full container introspection (grounded) or LLM-only (advisory):

- `false` — Review was fully grounded (Python with successful introspection) or introspection was not applicable (non-Python)
- `true` — Python introspection was expected but failed (container/runtime/script error). The review verdict is advisory, not verified against the actual package.

CI consumers can use this field to distinguish high-confidence reviews from advisory ones.
