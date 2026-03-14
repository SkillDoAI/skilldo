# Telemetry

Skilldo can log generation runs to a local CSV file for tracking model performance and pipeline behavior.

## What We Log

Each run appends one row to `~/.skilldo/runs.csv` with these fields:

| Field | Example | Description |
|-------|---------|-------------|
| language | python | Detected or specified language |
| library | click | Library name from package metadata |
| library_version | 8.1.7 | Library version |
| provider | anthropic | Primary LLM provider |
| model | claude-sonnet-4-6 | Primary LLM model used |
| test_provider | openai | Test stage provider (if different) |
| test_model | gpt-5.2 | Test stage model (if different) |
| review_provider | openai | Review stage provider (if different) |
| review_model | gpt-5.2 | Review stage model (if different) |
| max_retries | 5 | Max retry attempts configured |
| retries_used | 2 | Number of test retry attempts used |
| review_retries_used | 1 | Number of review retry attempts used |
| passed | true | Whether generation succeeded |
| failed_stage | test | Which stage failed (if any) |
| failure_reason | test_timeout | Failure detail |
| duration_secs | 180.0 | Total generation time |
| timestamp | 2026-03-13T20:00:00Z | ISO 8601 timestamp |
| skilldo_version | 0.4.2 | Skilldo binary version |

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
awk -F, '{print $5}' ~/.skilldo/runs.csv | sort | uniq -c | sort -rn

# Find failed runs
grep ',false,' ~/.skilldo/runs.csv
```

## Schema Migration

When skilldo adds or removes columns, the CSV header is automatically updated on the next run. Existing data rows are preserved — new columns appear empty for old rows. The migration uses atomic writes to prevent data loss.
