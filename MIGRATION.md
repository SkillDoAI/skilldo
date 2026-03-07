# Migration Guide

If you're one of the three users using this in our early stages — first off, thank you. Seriously. You're either brave, curious, or lost. We appreciate all three.

There are some design changes we made that'll affect your config going forward. Here's what changed, what's deprecated, and what's been removed.

Or, if you're feeling lazy, scroll to the bottom and copy-paste the LLM prompt. Let your AI do it.

---

## v0.2.3 — The Great Rename

We renamed the pipeline stages from numbered agents (`agent1`, `agent2`, etc.) to actual names that tell you what they do. Revolutionary, we know.

> **Note:** The old `agentN` aliases were deprecated in v0.2.3 and **removed in v0.2.4**. If you're upgrading from v0.2.2 or earlier, you must rename these fields.

### Config field renames (`skilldo.toml`)

**`[generation]` section:**

| Was | Is now | What it does |
|-----|--------|-------------|
| `enable_agent5` | `enable_test` | Enable/disable test stage |
| `agent5_mode` | `test_mode` | "thorough", "adaptive", or "minimal" |
| `agent1_llm` | `extract_llm` | Override LLM for API extraction |
| `agent2_llm` | `map_llm` | Override LLM for pattern mapping |
| `agent3_llm` | `learn_llm` | Override LLM for context learning |
| `agent4_llm` | `create_llm` | Override LLM for SKILL.md synthesis |
| `agent5_llm` | `test_llm` | Override LLM for code validation |

**`[prompts]` section:**

| Was | Is now |
|-----|--------|
| `agent1_custom` | `extract_custom` |
| `agent2_custom` | `map_custom` |
| `agent3_custom` | `learn_custom` |
| `agent4_custom` | `create_custom` |
| `agent5_custom` | `test_custom` |

**`[llm]` section:**

| Was | Is now | Why |
|-----|--------|-----|
| `provider` | `provider_type` | Prep for OAuth — `provider` still accepted as alias, removed in 0.5.0 |

### New field: `provider_name`

Optional human label for your provider instance. If you're using OAuth (v0.2.4+), this is the key we use to store your tokens. Defaults to the `provider_type` value, so most people don't need to set it.

```toml
[llm]
provider_type = "openai"
provider_name = "my-openai-sub"  # optional
```

### CLI flag renames

Same idea — numbered flags → named flags. These aliases were **removed in v0.2.4**.

| Was | Is now |
|-----|--------|
| `--agent5-model` | `--test-model` |
| `--agent5-provider` | `--test-provider` |
| `--no-agent5` | `--no-test` |
| `--agent5-mode` | `--test-mode` |

---

## v0.2.4 — OAuth + Breaking Alias Removal

### Breaking: `agentN` aliases removed

The deprecated `agentN` config aliases and CLI flags from v0.2.3 are **removed** in v0.2.4. If you haven't renamed them yet, your config will fail to parse.

**Config fields removed:**
- `agent1_llm`..`agent5_llm` → use `extract_llm`/`map_llm`/`learn_llm`/`create_llm`/`test_llm`
- `enable_agent5` → use `enable_test`
- `agent5_mode` → use `test_mode`
- `agent1_custom`..`agent5_custom` → use `extract_custom`..`test_custom`

**CLI flags removed:**
- `--agent5-model` → use `--test-model`
- `--agent5-provider` → use `--test-provider`
- `--no-agent5` → use `--no-test`
- `--agent5-mode` → use `--test-mode`

### OAuth (new fields, additive)

New optional fields you can add to any `[llm]` or per-stage LLM section to use OAuth instead of API keys:

```toml
oauth_auth_url = "https://accounts.google.com/o/oauth2/v2/auth"
oauth_token_url = "https://oauth2.googleapis.com/token"
oauth_scopes = "https://www.googleapis.com/auth/generative-language openid"
oauth_client_id_env = "GOOGLE_CLIENT_ID"        # env var NAME, not the actual value
oauth_client_secret_env = "GOOGLE_CLIENT_SECRET" # optional, some providers need it
```

Then run `skilldo auth login` and we handle the rest (PKCE, browser flow, token storage, auto-refresh).

---

## The Lazy Way

Copy this, paste it into your favorite LLM, attach your `skilldo.toml`, and let it do the work:

```
Update my skilldo.toml to use the latest field names. Apply these renames:

[generation] section:
- enable_agent5 → enable_test
- agent5_mode → test_mode
- agent1_llm → extract_llm
- agent2_llm → map_llm
- agent3_llm → learn_llm
- agent4_llm → create_llm
- agent5_llm → test_llm

[prompts] section:
- agent1_custom → extract_custom
- agent2_custom → map_custom
- agent3_custom → learn_custom
- agent4_custom → create_custom
- agent5_custom → test_custom

[llm] and any per-stage LLM sections:
- provider → provider_type

Do NOT change values, only field names. Keep all other fields unchanged.

Here is my config:
<paste your skilldo.toml>
```
