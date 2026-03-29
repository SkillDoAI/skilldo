```json
{
  "documented_apis": [
    "ServerBuilder",
    "ServerBuilder.with_auth",
    "ServerBuilder.with_bearer_token",
    "ServerBuilder.with_oauth_defaults",
    "ServerBuilder.with_oauth",
    "MockServer",
    "MockServer.check_error",
    "MockServer.oauth_url",
    "MockServer.oauth_client_credentials",
    "MockServer.approve_device_code",
    "OAuthConfig",
    "expires_after_uses",
    "run_with_output",
    "SpecErrorResponse",
    "SpecResponsesUsage",
    "SpecOutputMessage",
    "SpecFunctionCallItem"
  ],
  "conventions": [
    "Use `cargo add llmposter --dev` for in-process test usage; use the binary for standalone/CI use",
    "Fixture files are YAML; first-match-wins ordering — place more specific matchers before catch-all matchers",
    "Provider-agnostic fixtures by default; use `provider` field only when format-specific response fields are required",
    "Clients swap only base URL to `http://127.0.0.1:{port}` — no path changes needed, real API paths are already unique across providers",
    "Use `match.user_message` as a substring match by default; wrap in `{regex: '...'}` for regex matching",
    "Use `error` field for HTTP error code responses; use `failure` field for network/streaming simulations on top of a valid response body — these are distinct concepts",
    "Pin all dependencies to specific known-working versions; never use `@latest`",
    "Test coverage target is ≥ 98%; enforce via CI",
    "TDD: write failing tests before implementation; use AAA pattern",
    "Conventional commits required: feat:, fix:, test:, docs:, refactor:, chore:",
    "Always create PRs as drafts to prevent premature code review bot runs"
  ],
  "behavioral_semantics": [
    {
      "trigger": "Request to any LLM endpoint when `with_auth(true)` is set and no or invalid Bearer token is provided",
      "behavior": "Returns HTTP 401 with a provider-specific error body (OpenAI, Anthropic, and Gemini each have different 401 shapes)",
      "assertion": "response status equals 401; response body matches provider-specific error schema"
    },
    {
      "trigger": "`expires_after_uses(N)` set and N LLM requests have been served",
      "behavior": "Subsequent requests return 401 with provider-specific auth error; token is deterministically expired",
      "assertion": "request N+1 returns status 401"
    },
    {
      "trigger": "OAuth-issued token used on an LLM endpoint",
      "behavior": "Token is automatically accepted as valid on LLM endpoints without additional configuration",
      "assertion": "response status equals 200"
    },
    {
      "trigger": "YAML fixture has a typo in any field name",
      "behavior": "Server fails at fixture load time due to `#[serde(deny_unknown_fields)]` on all fixture YAML structs",
      "assertion": "server startup returns an error before any request is served"
    },
    {
      "trigger": "Fixture `match.user_message` is an empty string",
      "behavior": "Rejected at validation — empty substring patterns act as silent catch-alls and are not allowed",
      "assertion": "fixture load returns a validation error"
    },
    {
      "trigger": "Fixture `match.user_message` is `{regex: ''}` (empty regex)",
      "behavior": "Rejected at fixture validation",
      "assertion": "fixture load returns a validation error"
    },
    {
      "trigger": "Fixture defines a tool call with non-object arguments (e.g., a string or array)",
      "behavior": "Rejected at fixture load time — Anthropic and Gemini require tool-call arguments to be JSON objects",
      "assertion": "fixture load returns a validation error"
    },
    {
      "trigger": "Fixture defines a tool call with a blank tool name",
      "behavior": "Rejected at fixture validation",
      "assertion": "fixture load returns a validation error"
    },
    {
      "trigger": "Fixture `error.status` is outside 400–599",
      "behavior": "Rejected at fixture validation",
      "assertion": "fixture load returns a validation error"
    },
    {
      "trigger": "Fixture defines a regex pattern that would compile to a DFA larger than 1 MB",
      "behavior": "Rejected at fixture load with a size-limit error — prevents OOM from malicious patterns",
      "assertion": "fixture load returns a validation error"
    },
    {
      "trigger": "Any request to any endpoint",
      "behavior": "Response includes `x-request-id` header with deterministic value `req-llmposter-{N}` where N is a monotonically increasing counter",
      "assertion": "response headers contain `x-request-id` matching pattern `req-llmposter-\\d+`"
    },
    {
      "trigger": "Error response with status 429 (rate limit)",
      "behavior": "Provider-specific rate limit headers are included: OpenAI/Responses get `x-ratelimit-{limit,remaining,reset}-requests`; Anthropic gets `anthropic-ratelimit-requests-{limit,remaining,reset}`; Gemini gets `retry-after` only",
      "assertion": "response includes provider-appropriate rate limit headers"
    },
    {
      "trigger": "Multiple tool-call responses in the same server session",
      "behavior": "Tool-call IDs are globally unique across all responses via a server-wide counter — multi-turn scenarios will not produce duplicate IDs",
      "assertion": "all tool-call IDs in a multi-turn session are distinct"
    },
    {
      "trigger": "Gemini response for a safety-blocked message",
      "behavior": "`Content.role` may be absent (None) in the response — do not assume role is always present",
      "assertion": "role field may be null/absent in Gemini candidate content"
    },
    {
      "trigger": "Request body exceeds the default body size limit (413)",
      "behavior": "413 response still includes `x-request-id` header because `DefaultBodyLimit` layer is configured as inner",
      "assertion": "413 response includes `x-request-id` header"
    },
    {
      "trigger": "Streaming fixture with `failure.truncate_after_chunks: N`",
      "behavior": "Stream sends N SSE chunks then closes the connection mid-response",
      "assertion": "received chunk count equals N and connection closes before final chunk"
    }
  ],
  "pitfalls": [
    {
      "category": "Fixture match ordering",
      "wrong": "Placing a broad `user_message: 'stock'` fixture before a specific `user_message: 'stock price of AAPL'` fixture",
      "why": "First-match-wins — the broad pattern intercepts all requests before the specific one can match",
      "right": "Place more specific matchers first; place catch-all or broad matchers last"
    },
    {
      "category": "Error vs failure simulation",
      "wrong": "Using `error: {status: 429}` expecting to test truncated streaming behavior",
      "why": "`error` returns an HTTP error code immediately with no response body stream; `failure` injects problems (truncation, disconnect, latency) into an otherwise valid streaming response",
      "right": "Use `error` for HTTP status error codes; use `failure.truncate_after_chunks` / `failure.disconnect_after_ms` for streaming failure simulation"
    },
    {
      "category": "Responses API streaming event shape (breaking change from 0.3.3 → 0.3.4)",
      "wrong": "Asserting on flat event fields or checking for `response.done` event in Responses API stream",
      "why": "As of 0.3.4 events use nested `response` envelopes with `sequence_number` and correlation fields; `response.done` was removed (non-spec); `response.in_progress` was added",
      "right": "Parse events expecting nested `response` envelope structure; check for `response.in_progress` not `response.done`"
    },
    {
      "category": "Error response field types (breaking change from 0.3.3 → 0.3.4)",
      "wrong": "Asserting `error.code` is an integer or that `error.param` is absent",
      "why": "As of 0.3.4 the error shape matches real OpenAI: `type` maps to error category, `code` is a string, `param` is present as null",
      "right": "Assert `error.code` is a string; assert `error.param` is null (not absent)"
    },
    {
      "category": "Integer time conversion",
      "wrong": "`duration.as_millis() as u64`",
      "why": "`as_millis()` returns u128; casting with `as u64` silently truncates on very large durations",
      "right": "`u64::try_from(duration.as_millis()).expect('duration fits u64')`"
    },
    {
      "category": "Malicious regex patterns",
      "wrong": "Accepting untrusted user input directly as a fixture regex pattern",
      "why": "Complex regex patterns can compile to DFAs exceeding memory limits; llmposter caps DFA at 1 MB but this is a safety net, not a license for arbitrary input",
      "right": "Validate and sanitize regex patterns from untrusted sources before putting them in fixture files"
    },
    {
      "category": "Assuming Gemini role field is always present",
      "wrong": "`let role = candidate.content.role.unwrap()`",
      "why": "`Content.role` is `Option<String>` and may be absent on safety-blocked responses",
      "right": "Handle `Content.role` as `Option<String>`; use `.as_deref().unwrap_or('model')` or match on it"
    },
    {
      "category": "Tool-call ID stability across turns",
      "wrong": "Asserting exact counter-value tool-call IDs like `call_1`, `call_2`",
      "why": "IDs are generated from a server-wide counter shared across all sessions; exact values are not stable across test runs if tests share a server instance",
      "right": "Assert tool-call IDs match a prefix pattern and are unique within the response, not exact counter values"
    },
    {
      "category": "MSRV compatibility (0.3.x → 0.4.x)",
      "wrong": "Building llmposter 0.4.0 with Rust < 1.89",
      "why": "The `oauth-mock` dependency added in 0.4.0 requires MSRV 1.89",
      "right": "Ensure toolchain is Rust 1.89 or later before upgrading to 0.4.0"
    },
    {
      "category": "Optional oauth feature",
      "wrong": "Assuming OAuth mock endpoints are unavailable unless explicitly enabled",
      "why": "The `oauth` feature is on by default in 0.4.0 — OAuth endpoints are active unless the feature is explicitly disabled",
      "right": "To disable OAuth endpoints, add `default-features = false` when depending on llmposter"
    }
  ],
  "breaking_changes": [
    {
      "version_from": "0.3.3",
      "version_to": "0.3.4",
      "change": "Responses API streaming protocol changed: events now use nested `response` envelopes, include `sequence_number` and correlation fields, `response.in_progress` event added, `response.done` removed (was non-spec)",
      "migration": "Update any SSE parsing code for `/v1/responses` streaming to expect nested `response` envelope structure; remove assertions on `response.done`; add handling for `response.in_progress`"
    },
    {
      "version_from": "0.3.3",
      "version_to": "0.3.4",
      "change": "Error response format for `/v1/responses` changed to match real OpenAI shape: `type` maps to error category, `code` is now a string (not integer), `param` field is always present as null",
      "migration": "Update error response assertions: change `code` type expectations from integer to string; add assertion for `param: null`"
    },
    {
      "version_from": "0.3.x",
      "version_to": "0.4.0",
      "change": "MSRV bumped from previous baseline to 1.89 due to oauth-mock dependency",
      "migration": "Upgrade Rust toolchain to 1.89+ before upgrading to 0.4.0; update `rust-toolchain.toml` or CI matrix accordingly"
    }
  ],
  "migration_notes": "0.3.x → 0.4.0: (1) Bump Rust to 1.89+. (2) `oauth` feature is enabled by default — OAuth endpoints at standard OIDC paths are now active; disable with `default-features = false` if not needed. (3) Wire in `with_auth(true)` + `with_bearer_token()` or `with_oauth_defaults()` on ServerBuilder to test auth flows. (4) Use `MockServer.oauth_url()` to get the OAuth base URL for configuring test clients. (5) Use `expires_after_uses(N)` to test token expiry deterministically. 0.3.3 → 0.3.4 (already released): Update Responses API streaming parsers for nested envelope format and removal of `response.done`; update error shape assertions for `code` as string and `param: null`."
}
```
