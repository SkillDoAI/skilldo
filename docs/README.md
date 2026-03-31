# Skilldo Documentation

| Guide | Description |
|-------|-------------|
| [Configuration](configuration.md) | All TOML config fields — providers, generation settings, per-stage LLM overrides, security context, redaction |
| [Languages](languages.md) | Supported languages, detection, test validation, install modes |
| [Architecture](architecture.md) | 6-stage pipeline, security scanner, model communication (SKILLDO-* comments) |
| [Authentication](authentication.md) | OAuth flows, API key setup, provider-specific auth |
| [Best Practices](best-practices.md) | Custom instructions, prompt tuning, CI integration |
| [Telemetry](telemetry.md) | Local run logging schema |

## Quick Start

```bash
# Install
brew install skilldoai/tap/skilldo

# Generate a SKILL.md
export ANTHROPIC_API_KEY="..."
skilldo generate /path/to/repo

# Or use Claude CLI (free, no API key)
skilldo generate /path/to/repo --config skilldo.toml
```

See the [main README](../README.md) for full installation and usage instructions.
