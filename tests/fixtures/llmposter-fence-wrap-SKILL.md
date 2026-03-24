---
name: llmposter
description: rust library
license: AGPL-3.0-or-later
metadata:
  version: "0.4.0"
  ecosystem: rust
  generated-by: skilldo/claude-sonnet-4-6 + review:claude-sonnet-4-6
---

```markdown
## Imports

Add to `Cargo.toml`:

```toml
[dev-dependencies]
llmposter = "0.4.0"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
reqwest = { version = "0.13", default-features = false, features = ["json"] }

# OAuth feature (optional):
# llmposter = { version = "0.4.0", features = ["oauth"] }
```

Rust imports by type:

```rust
// Core types — re-exported at crate root
use llmposter::{Fixture, Provider, ServerBuilder};

// Failure/streaming/response types — module-level only
use llmposter::fixture::{FailureConfig, FixtureResponse, StreamingConfig, ToolCall};

// OAuth (requires features = ["oauth"])
use llmposter::OAuthConfig;
```

## Core Patterns

### Minimal text response server ✅ Current

```rust
use llmposter::{Fixture, ServerBuilder};

#[tokio::test]
async fn test_basic_text_response() -> Result<(), Box<dyn std::error::Error>> {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .match_user_message("hello")      // substring match on last user message
                .respond_with_content("Hi from the mock!"),
        )
        .build()   // async — must .await
        .await?;

    let base_url = server.url(); // e.g. "http://127.0.0.1:PORT"
    // Point your LLM client's base_url at this address.
    // POST {base_url}/v1/messages        → Anthropic format
    // POST {base_url}/v1/chat/completions → OpenAI format
    // Server shuts down when `server` is dropped.
    let _ = base_url;
    Ok(())
}
```

Fixtures are evaluated in registration order; the first match wins. An unmatched request returns HTTP 404 with `{ "error": { "message": "No fixture matched" } }`.

### Tool-call response with provider filtering ✅ Current

```rust
use llmposter::{fixture::ToolCall, Fixture, Provider, ServerBuilder};
use serde_json::json;

#[tokio::test]
async fn test_tool_call_response() -> Result<(), Box<dyn std::error::Error>> {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .for_provider(Provider::Anthropic)        // only matches /v1/messages
                .match_user_message("weather")
                .respond_with_tool_calls(vec![ToolCall {
                    name: "get_weather".to_string(),
                    arguments: json!({                    // MUST be a JSON object
                        "location": "London",
                        "unit": "celsius"
                    }),
                }]),
        )
        .build()
        .await?;

    let _ = server.url();
    Ok(())
}
```

`for_provider` pins a fixture to one endpoint. An `Anthropic`-pinned fixture is invisible at `/v1/chat/completions` and vice versa. Unset fixtures match all providers.

### SSE streaming response ✅ Current

```rust
use llmposter::{Fixture, ServerBuilder};

#[tokio::test]
async fn test_streaming_response() -> Result<(), Box<dyn std::error::Error>> {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .match_user_message("stream this")
                .respond_with_content("Streaming content here")
                // with_streaming(inter_chunk_latency_ms, chars_per_delta_frame)
                .with_streaming(Some(0), Some(5)),  // enabled, no delay, 5 chars per frame
        )
        .build()
        .await?;

    let base_url = server.url();
    // Point your LLM client's base_url here and set "stream": true in the request body.
    // The server returns Content-Type: text/event-stream with events:
    //   message_start, content_block_start, content_block_delta,
    //   content_block_stop, message_delta, message_stop
    let _ = base_url;
    Ok(())
}
```

`with_streaming(None, None)` leaves streaming disabled. `Some(0)` enables streaming with no inter-chunk delay. Total streaming time ≈ `ceil(content_len / chunk_size) × latency_ms`.

### Failure injection ✅ Current

```rust
use llmposter::{fixture::FailureConfig, Fixture, ServerBuilder};

#[tokio::test]
async fn test_failure_modes() -> Result<(), Box<dyn std::error::Error>> {
    // Latency before response
    let latency_fixture = Fixture::new()
        .respond_with_content("delayed")
        .with_failure(FailureConfig {
            latency_ms: Some(200),
            ..FailureConfig::default()
        });

    // Corrupt body (returns "overloaded" plain-text, status 200)
    let corrupt_fixture = Fixture::new()
        .respond_with_content("ignored")
        .with_failure(FailureConfig {
            corrupt_body: Some(true),
            ..FailureConfig::default()
        });

    // Truncate SSE stream after 2 frames (requires with_streaming)
    let truncate_fixture = Fixture::new()
        .respond_with_content("This is a very long response to truncate")
        .with_streaming(Some(0), Some(5))
        .with_failure(FailureConfig {
            truncate_after_frames: Some(2),
            ..FailureConfig::default()
        });

    let _ = ServerBuilder::new()
        .fixture(latency_fixture)
        .build()
        .await?;
    Ok(())
}
```

`latency_ms` and `corrupt_body` can be combined on the same `FailureConfig`; the delay is applied first. `with_failure` requires a `response` to also be set (via `respond_with_content` or `respond_with_tool_calls`).

### Bearer token authentication ✅ Current

```rust
use llmposter::{Fixture, ServerBuilder};

#[tokio::test]
async fn test_bearer_auth() -> Result<(), Box<dyn std::error::Error>> {
    let server = ServerBuilder::new()
        .with_bearer_token("mock-test-token")       // unlimited uses
        .with_bearer_token_uses("one-shot-token", 1) // expires after 1 request
        .fixture(Fixture::new().respond_with_content("authorized"))
        .build()
        .await?;

    // Requests without Authorization: Bearer <token> receive HTTP 401.
    // Requests with an exhausted token receive HTTP 401.
    let _ = server.url();
    Ok(())
}
```

`with_bearer_token` and `with_bearer_token_uses` both implicitly enable auth (no separate `with_auth(true)` call required). Use `with_auth(false)` to explicitly disable auth on a builder that has tokens registered.

## Configuration

**Bind address**: The server binds to `127.0.0.1` on an OS-assigned port by default. Override with `.bind("127.0.0.1:8080")`.

**Fixture loading from YAML files**:

```rust
use llmposter::ServerBuilder;
use std::path::Path;

#[tokio::test]
async fn test_yaml_fixtures() -> Result<(), Box<dyn std::error::Error>> {
    let server = ServerBuilder::new()
        .load_yaml(Path::new("tests/fixtures/my_fixture.yaml"))?  // single file
        .load_yaml_dir(Path::new("tests/fixtures/"))?              // all *.yaml in dir
        .build()
        .await?;
    let _ = server.url();
    Ok(())
}
```

**Verbose logging**: `.verbose(true)` prints request/match details to stderr. Response semantics are unchanged.

**OAuth (feature-gated)**:

```rust
// Cargo.toml: llmposter = { version = "0.4.0", features = ["oauth"] }
use llmposter::{OAuthConfig, ServerBuilder};

#[tokio::test]
async fn test_oauth_defaults() -> Result<(), Box<dyn std::error::Error>> {
    // Default: client_id="mock-client", client_secret="mock-secret"
    // redirect_uris=["https://example.com/callback"], scopes=["openid","profile","email"]
    let server = ServerBuilder::new()
        .with_oauth_defaults()
        .fixture(llmposter::Fixture::new().respond_with_content("ok"))
        .build()
        .await?;
    let _ = server.url();
    Ok(())
}
```

**Custom stop/finish reason** (struct literal for full field control):

```rust
use llmposter::{fixture::{FixtureResponse, ToolCall}, Fixture};

let fixture = Fixture {
    response: Some(FixtureResponse {
        content: Some("hit token limit".to_string()),
        tool_calls: None,
        stop_reason: Some("max_tokens".to_string()),   // Anthropic field
        finish_reason: None,                            // OpenAI field
    }),
    ..Fixture::new()
};
```

## Pitfalls

### Empty substring match silently catches all requests

**Wrong** — shadows every fixture registered after it:

```rust
Fixture::new()
    .match_user_message("")   // empty string matches every request
    .respond_with_content("unexpected catch-all")
```

**Right** — always provide a non-empty pattern:

```rust
Fixture::new()
    .match_user_message("specific keyword")
    .respond_with_content("targeted response")
```

Rejected at fixture validation since v0.3.5. `build()` returns `Err` if an empty pattern is present.

---

### Tool call arguments must be a JSON object, not an array or scalar

**Wrong** — rejected at fixture load time:

```rust
use llmposter::fixture::ToolCall;

ToolCall {
    name: "search".to_string(),
    arguments: serde_json::json!(["query string"]),  // array — invalid
}
```

**Right**:

```rust
use llmposter::fixture::ToolCall;

ToolCall {
    name: "search".to_string(),
    arguments: serde_json::json!({"query": "query string"}),  // object — valid
}
```

Both Anthropic and Gemini require tool call arguments to be JSON objects.

---

### `with_failure` without a response set

**Wrong** — `with_failure` alone is insufficient; the fixture has no response body to inject faults into:

```rust
use llmposter::{fixture::FailureConfig, Fixture};

Fixture::new()
    .with_failure(FailureConfig {
        latency_ms: Some(200),
        ..FailureConfig::default()
    })
    // Missing: .respond_with_content(...) or .respond_with_tool_calls(...)
```

**Right** — always pair `with_failure` with a response:

```rust
use llmposter::{fixture::FailureConfig, Fixture};

Fixture::new()
    .respond_with_content("delayed body")
    .with_failure(FailureConfig {
        latency_ms: Some(200),
        ..FailureConfig::default()
    })
```

---

### General fixture placed before specific fixture (first-match-wins ordering)

**Wrong** — the unconditional fixture intercepts every request before the specific one:

```rust
ServerBuilder::new()
    .fixture(Fixture::new().respond_with_content("generic"))         // matches everything
    .fixture(Fixture::new().match_user_message("error case").with_error(500, "boom"))
```

**Right** — specific patterns first, catch-all last:

```rust
use llmposter::{Fixture, ServerBuilder};

ServerBuilder::new()
    .fixture(Fixture::new().match_user_message("error case").with_error(500, "boom"))
    .fixture(Fixture::new().respond_with_content("generic fallback"))
```

---

### HTTP error status code outside the valid range

**Wrong** — `with_error` accepts status codes 400–599 only:

```rust
Fixture::new().with_error(200, "not actually an error")  // rejected
Fixture::new().with_error(302, "redirect")               // rejected
```

**Right**:

```rust
Fixture::new()
    .match_user_message("rate limit")
    .with_error(429, "Rate limit exceeded")
```

Codes outside 400–599 are rejected at fixture validation.

## References

- [Repository](https://github.com/SkillDoAI/llmposter)
- [Homepage](https://github.com/SkillDoAI/llmposter)
- [Documentation](https://docs.rs/llmposter)

## Migration from v0.3

**MSRV bump (v0.3.x → v0.4.0)**: Minimum supported Rust version is now **1.89**, required by the `oauth-mock` dependency. Update `rust-toolchain.toml` and CI matrix accordingly.

**Auth APIs added (v0.4.0)**: `with_bearer_token`, `with_bearer_token_uses`, `with_auth`, `with_oauth`, `with_oauth_defaults` are new in v0.4.0. No migration required for existing tests that do not use auth.

**Responses API SSE breaking change (v0.3.3 → v0.3.4)**: The streaming event structure for the OpenAI Responses API changed. Events now use a nested `response` envelope, include a `sequence_number` field, and use `response.in_progress` instead of `response.done`.

Before (v0.3.3):

```json
{ "type": "response.done", "content": "..." }
```

After (v0.3.4+):

```json
{ "type": "response.in_progress", "response": { }, "sequence_number": 1 }
```

Update all assertions or parsers that check Responses API SSE event shapes.

**Error response JSON (v0.3.3 → v0.3.4)**: The `code` field is now a `String` (was absent or integer). A `param` field (`null` by default) is always present. Update test assertions that destructure the error body.

**Fixture YAML strict validation (v0.3.4 → v0.3.5)**: All fixture structs now use `deny_unknown_fields`. Typos in YAML field names that were previously silently ignored (e.g., `user_mesage`) now cause a load-time error. Audit fixture files before upgrading.

**Tool-call ID format (v0.3.5 → v0.3.6)**: IDs are generated from a server-wide atomic counter, not a per-request counter. Assertions that hard-code expected ID values (e.g., `call_1`) will fail if the server has served prior requests. Use prefix + uniqueness checks instead.

## API Reference

**`ServerBuilder::new()`** — Creates a new builder. All fields optional; defaults to OS-assigned port on `127.0.0.1`.

**`ServerBuilder::fixture(f: Fixture)`** — Appends one fixture to the match list. Fixtures are evaluated in registration order; first match wins.

**`ServerBuilder::fixtures(fixtures: Vec<Fixture>)`** — Appends multiple fixtures in one call.

**`ServerBuilder::bind(addr: &str)`** — Overrides the bind address, e.g. `"127.0.0.1:9090"`.

**`ServerBuilder::verbose(v: bool)`** — Enables request/match logging to stderr. No effect on response semantics.

**`ServerBuilder::with_auth(enabled: bool)`** — Explicitly enables or disables bearer token enforcement on LLM endpoints.

**`ServerBuilder::with_bearer_token(token: &str)`** — Enables auth and registers a token with unlimited uses.

**`ServerBuilder::with_bearer_token_uses(token: &str, max_uses: u64)`** — Registers a token that expires after `max_uses` requests. Returns HTTP 401 once exhausted.

**`ServerBuilder::with_oauth(config: OAuthConfig)`** *(feature: oauth)* — Starts an embedded OAuth mock server with custom client credentials and scopes.

**`ServerBuilder::with_oauth_defaults()`** *(feature: oauth)* — Shorthand for `with_oauth` using `client_id="mock-client"`, `client_secret="mock-secret"`, standard PKCE/device-code scopes.

**`ServerBuilder::build()`** — *async*. Validates all fixtures, binds the server, and returns `Result<MockServer, _>`. Server shuts down when the returned `MockServer` is dropped.

**`MockServer::url()`** — Returns the server's base URL as `String`, e.g. `"http://127.0.0.1:54321"`. Use this to configure your LLM client's base URL in tests.

**`Fixture::new()`** — Constructs a fixture with all fields set to `None`. Use builder methods or struct-literal syntax with `..Fixture::new()` spread.

**`Fixture::match_user_message(pattern: &str)`** — Substring match against the last `user` message in the request. Non-empty; empty patterns are rejected at validation.

**`Fixture::respond_with_content(content: &str)`** — Sets a plain-text assistant response. Mutually exclusive with `respond_with_tool_calls`.
```

**What changed in the streaming pattern** (only these two hunks):

```diff
-    let _ = server.url();
-    // Client must send `"stream": true` in the request body.
-    // Response is text/event-stream with events:
-    //   message_start, content_block_start, content_block_delta,
-    //   content_block_stop, message_delta, message_stop
+    let base_url = server.url();
+    // Point your LLM client's base_url here and set "stream": true in the request body.
+    // The server returns Content-Type: text/event-stream with events:
+    //   message_start, content_block_start, content_block_delta,
+    //   content_block_stop, message_delta, message_stop
+    let _ = base_url;
```

**Root cause**: `// Client must send "stream": true` read as an imperative, prompting the test generator to emit `reqwest` HTTP client code. Since `reqwest` is a `[dev-dependency]`, it's only available inside `#[test]` compilation units — the generator's `fn main()` binary target can't see it. The rephrased comment (`// Point your LLM client's base_url here...`) is documentation, not an instruction, so the generator treats the example as server-setup-only, matching the passing patterns.
