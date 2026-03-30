```json
[
  {
    "pattern_id": "anthropic_basic_text_response",
    "api_being_tested": "ServerBuilder + Fixture - basic text response via Anthropic /v1/messages",
    "setup_code": {
      "imports": [
        "use llmposter::fixture::{FailureConfig, FixtureResponse, ToolCall};",
        "use llmposter::{Fixture, Provider, ServerBuilder};"
      ],
      "initialization": "ServerBuilder::new().fixture(Fixture::new().match_user_message(\"hello\").respond_with_content(\"Hi from Claude mock!\")).build().await.unwrap()"
    },
    "usage_pattern": {
      "async": true,
      "runtime": "tokio",
      "call": "reqwest::Client::new().post(format!(\"{}/v1/messages\", server.url())).json(&serde_json::json!({\"model\": \"claude-sonnet-4-6\", \"max_tokens\": 1024, \"messages\": [{\"role\": \"user\", \"content\": \"hello world\"}]})).send().await.unwrap()"
    },
    "assertions": [
      "status == 200",
      "body[\"type\"] == \"message\"",
      "body[\"role\"] == \"assistant\"",
      "body[\"content\"][0][\"type\"] == \"text\"",
      "body[\"content\"][0][\"text\"] == \"Hi from Claude mock!\"",
      "body[\"stop_reason\"] == \"end_turn\"",
      "body[\"id\"].starts_with(\"msg-llmposter-\")",
      "body[\"usage\"][\"input_tokens\"] > 0",
      "body[\"usage\"][\"output_tokens\"] > 0"
    ],
    "test_infrastructure": {
      "server": "ServerBuilder builds an HTTP test server on a random port",
      "client": "reqwest::Client (plain HTTP)",
      "fixture_matching": "match_user_message does substring match on last user message content"
    },
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_bad_request_unparseable_body",
    "api_being_tested": "Anthropic endpoint request validation - 400 on unparseable JSON body",
    "setup_code": {
      "imports": ["same as above"],
      "initialization": "ServerBuilder::new().fixture(Fixture::new().respond_with_content(\"x\")).build().await.unwrap()"
    },
    "usage_pattern": {
      "async": true,
      "call": "client.post(format!(\"{}/v1/messages\", server.url())).header(\"content-type\", \"application/json\").body(\"not json\").send().await.unwrap()"
    },
    "assertions": [
      "status == 400"
    ],
    "test_infrastructure": {
      "note": "Raw body sent instead of .json() to trigger parse failure"
    },
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_bad_request_missing_messages",
    "api_being_tested": "Anthropic endpoint request validation - 400 on missing required `messages` field",
    "setup_code": {
      "initialization": "ServerBuilder::new().fixture(Fixture::new().respond_with_content(\"x\")).build().await.unwrap()"
    },
    "usage_pattern": {
      "async": true,
      "call": "client.post(...).json(&serde_json::json!({\"model\": \"claude-sonnet-4-6\", \"max_tokens\": 1024})).send().await.unwrap()"
    },
    "assertions": [
      "status == 400"
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_streaming_text",
    "api_being_tested": "Fixture::with_streaming - SSE text stream via Anthropic endpoint",
    "setup_code": {
      "initialization": "Fixture::new().match_user_message(\"hello\").respond_with_content(\"Hello world\").with_streaming(Some(0), Some(5))"
    },
    "usage_pattern": {
      "async": true,
      "call": "client.post(...).json(&serde_json::json!({..., \"stream\": true})).send().await.unwrap()",
      "with_streaming_args": {
        "latency_ms_per_chunk": "Some(0)",
        "chunk_size_chars": "Some(5)"
      }
    },
    "assertions": [
      "status == 200",
      "content-type header == \"text/event-stream\"",
      "body contains \"event: message_start\"",
      "body contains \"event: content_block_delta\"",
      "body contains \"event: message_stop\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_array_content_format",
    "api_being_tested": "Anthropic endpoint - array content format for user message",
    "setup_code": {
      "initialization": "Fixture::new().match_user_message(\"array content\").respond_with_content(\"got it\")"
    },
    "usage_pattern": {
      "async": true,
      "call": "client.post(...).json(&serde_json::json!({\"messages\": [{\"role\": \"user\", \"content\": [{\"type\": \"text\", \"text\": \"array content\"}]}]})).send().await.unwrap()"
    },
    "assertions": [
      "status == 200"
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_tool_use_response",
    "api_being_tested": "Fixture::respond_with_tool_calls - non-streaming Anthropic tool_use response",
    "setup_code": {
      "initialization": "Fixture::new().match_user_message(\"weather\").respond_with_tool_calls(vec![ToolCall { name: \"get_weather\".to_string(), arguments: serde_json::json!({\"location\": \"London\", \"unit\": \"celsius\"}) }])"
    },
    "usage_pattern": {
      "async": true,
      "call": "client.post(\"{url}/v1/messages\").json(&standard_anthropic_request).send().await.unwrap()"
    },
    "assertions": [
      "status == 200",
      "body[\"type\"] == \"message\"",
      "body[\"role\"] == \"assistant\"",
      "body[\"stop_reason\"] == \"tool_use\"",
      "body[\"content\"][0][\"type\"] == \"tool_use\"",
      "body[\"content\"][0][\"name\"] == \"get_weather\"",
      "body[\"content\"][0][\"id\"] == \"toolu_llmposter_1\"",
      "body[\"content\"][0][\"input\"][\"location\"] == \"London\"",
      "body[\"content\"][0][\"input\"][\"unit\"] == \"celsius\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_streaming_tool_use",
    "api_being_tested": "Fixture - SSE streaming tool_use response via Anthropic endpoint",
    "setup_code": {
      "initialization": "Fixture::new().match_user_message(\"weather\").respond_with_tool_calls(vec![ToolCall { name: \"get_weather\".to_string(), arguments: serde_json::json!({\"location\": \"Paris\"}) }]).with_streaming(Some(0), Some(5))"
    },
    "usage_pattern": {
      "async": true,
      "call": "client.post(...).json(&{..., \"stream\": true}).send().await.unwrap()"
    },
    "assertions": [
      "status == 200",
      "content-type == \"text/event-stream\"",
      "body contains \"event: message_start\"",
      "body contains \"event: message_stop\"",
      "body contains \"tool_use\"",
      "body contains \"get_weather\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_failure_latency",
    "api_being_tested": "FailureConfig::latency_ms - inject artificial latency on non-streaming response",
    "setup_code": {
      "initialization": "Fixture::new().respond_with_content(\"delayed anthropic\").with_failure(FailureConfig { latency_ms: Some(200), corrupt_body: None, truncate_after_frames: None, disconnect_after_ms: None })"
    },
    "usage_pattern": {
      "async": true,
      "call": "standard non-streaming POST, measure elapsed with std::time::Instant::now()"
    },
    "assertions": [
      "status == 200",
      "body[\"content\"][0][\"text\"] == \"delayed anthropic\"",
      "elapsed >= 180ms"
    ],
    "test_infrastructure": {
      "timing": "std::time::Instant for wall-clock verification"
    },
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_streaming_latency_between_chunks",
    "api_being_tested": "Fixture::with_streaming - per-chunk latency in SSE stream",
    "setup_code": {
      "initialization": "Fixture::new().respond_with_content(\"Hello world test\").with_streaming(Some(50), Some(5))"
    },
    "usage_pattern": {
      "async": true,
      "call": "streaming POST, collect full body text, measure elapsed"
    },
    "assertions": [
      "status == 200",
      "body contains \"event: message_stop\"",
      "elapsed >= 150ms (16 chars / chunk_size 5 = 4 deltas * 50ms each = 200ms theoretical)"
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_fixture_first_match_wins",
    "api_being_tested": "ServerBuilder - multiple fixtures, first match wins",
    "setup_code": {
      "initialization": "ServerBuilder::new().fixture(Fixture::new().match_user_message(\"hello\").respond_with_content(\"first match\")).fixture(Fixture::new().match_user_message(\"hello\").respond_with_content(\"second match\")).build().await.unwrap()"
    },
    "usage_pattern": {
      "async": true,
      "call": "standard POST matching both fixtures"
    },
    "assertions": [
      "status == 200",
      "body[\"content\"][0][\"text\"] == \"first match\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_provider_filter_openai_not_matched",
    "api_being_tested": "Fixture::for_provider - OpenAI-scoped fixture does NOT match Anthropic endpoint",
    "setup_code": {
      "initialization": "Fixture::new().respond_with_content(\"openai only\").for_provider(Provider::OpenAI)"
    },
    "usage_pattern": {
      "async": true,
      "call": "POST to /v1/messages (Anthropic endpoint)"
    },
    "assertions": [
      "status == 404"
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_provider_filter_anthropic_matched",
    "api_being_tested": "Fixture::for_provider - Anthropic-scoped fixture matches Anthropic endpoint",
    "setup_code": {
      "initialization": "Fixture::new().respond_with_content(\"anthropic matched\").for_provider(Provider::Anthropic)"
    },
    "usage_pattern": {
      "async": true,
      "call": "POST to /v1/messages"
    },
    "assertions": [
      "status == 200",
      "body[\"content\"][0][\"text\"] == \"anthropic matched\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_model_filter",
    "api_being_tested": "Fixture::match_model - substring model name filter",
    "setup_code": {
      "initialization": "Fixture::new().match_model(\"claude-sonnet\").respond_with_content(\"sonnet response\")"
    },
    "usage_pattern": {
      "async": true,
      "call": "two requests: one with model \"claude-sonnet-4-6\" (matches), one with \"claude-haiku-3\" (does not match)"
    },
    "assertions": [
      "claude-sonnet-4-6: status == 200, text == \"sonnet response\"",
      "claude-haiku-3: status == 404"
    ],
    "test_infrastructure": {
      "note": "match_model does substring matching, not exact equality"
    },
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_custom_stop_reason",
    "api_being_tested": "FixtureResponse::stop_reason - override default stop_reason in response",
    "setup_code": {
      "initialization": "Fixture { response: Some(FixtureResponse { content: Some(\"hit max tokens\".to_string()), tool_calls: None, stop_reason: Some(\"max_tokens\".to_string()), finish_reason: None }), ..Fixture::new() }"
    },
    "usage_pattern": {
      "async": true,
      "note": "Uses struct literal with ..Fixture::new() spread for defaults"
    },
    "assertions": [
      "status == 200",
      "body[\"stop_reason\"] == \"max_tokens\"",
      "body[\"content\"][0][\"text\"] == \"hit max tokens\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_error_fixture_429",
    "api_being_tested": "Fixture::with_error - return HTTP error status with message",
    "setup_code": {
      "initialization": "Fixture::new().match_user_message(\"rate limit\").with_error(429, \"Rate limit exceeded\")"
    },
    "usage_pattern": {
      "async": true,
      "call": "standard POST"
    },
    "assertions": [
      "status == 429",
      "body[\"error\"][\"message\"] == \"Rate limit exceeded\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_error_fixture_500",
    "api_being_tested": "Fixture::with_error - 500 Internal Server Error",
    "setup_code": {
      "initialization": "Fixture::new().with_error(500, \"Internal server error\")"
    },
    "usage_pattern": {
      "async": true
    },
    "assertions": [
      "status == 500",
      "body[\"error\"][\"message\"] == \"Internal server error\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_error_fixture_503",
    "api_being_tested": "Fixture::with_error - 503 Service Unavailable",
    "setup_code": {
      "initialization": "Fixture::new().with_error(503, \"Service overloaded\")"
    },
    "assertions": [
      "status == 503"
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_verbose_mode_no_match",
    "api_being_tested": "ServerBuilder::verbose - verbose logging when no fixture matches",
    "setup_code": {
      "initialization": "ServerBuilder::new().verbose(true).fixture(...).build().await.unwrap()"
    },
    "usage_pattern": {
      "async": true,
      "call": "POST with message that doesn't match any fixture"
    },
    "assertions": [
      "status == 404",
      "body[\"error\"][\"message\"] contains \"No fixture matched\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_verbose_mode_match",
    "api_being_tested": "ServerBuilder::verbose - verbose logging when fixture matches",
    "setup_code": {
      "initialization": "ServerBuilder::new().verbose(true).fixture(Fixture::new().match_user_message(\"hello\").respond_with_content(\"verbose match\")).build().await.unwrap()"
    },
    "assertions": [
      "status == 200",
      "body[\"content\"][0][\"text\"] == \"verbose match\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_failure_corrupt_body",
    "api_being_tested": "FailureConfig::corrupt_body - return plain text 'overloaded' response instead of JSON",
    "setup_code": {
      "initialization": "Fixture::new().respond_with_content(\"should not appear\").with_failure(FailureConfig { latency_ms: None, corrupt_body: Some(true), truncate_after_frames: None, disconnect_after_ms: None })"
    },
    "assertions": [
      "status == 200",
      "content-type contains \"text/plain\"",
      "body == \"overloaded\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_failure_latency_plus_corrupt_body",
    "api_being_tested": "FailureConfig - combine latency_ms and corrupt_body",
    "setup_code": {
      "initialization": "FailureConfig { latency_ms: Some(100), corrupt_body: Some(true), truncate_after_frames: None, disconnect_after_ms: None }"
    },
    "assertions": [
      "status == 200",
      "body == \"overloaded\"",
      "elapsed >= 80ms"
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_streaming_truncate_after_frames",
    "api_being_tested": "FailureConfig::truncate_after_frames - truncate SSE stream after N frames",
    "setup_code": {
      "initialization": "Fixture::new().respond_with_content(\"...\").with_streaming(Some(0), Some(5)).with_failure(FailureConfig { truncate_after_frames: Some(2), ..FailureConfig::default() })"
    },
    "usage_pattern": {
      "async": true,
      "note": "FailureConfig::default() used for spread — implies Default impl exists"
    },
    "assertions": [
      "status == 200",
      "body contains \"event: message_start\"",
      "body does NOT contain \"event: message_stop\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_streaming_truncate_tool_call",
    "api_being_tested": "FailureConfig::truncate_after_frames - truncate tool_use SSE stream after N frames",
    "setup_code": {
      "initialization": "Fixture::new().match_user_message(\"weather\").respond_with_tool_calls([...]).with_streaming(Some(0), Some(5)).with_failure(FailureConfig { truncate_after_frames: Some(2), ..FailureConfig::default() })"
    },
    "assertions": [
      "status == 200",
      "body contains \"event: message_start\"",
      "body does NOT contain \"event: message_stop\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_streaming_custom_stop_reason_tool_call",
    "api_being_tested": "FixtureResponse::stop_reason + streaming tool_use - custom stop_reason propagated into SSE stream",
    "setup_code": {
      "initialization": "Fixture { match_rule: None, provider: None, response: Some(FixtureResponse { content: None, tool_calls: Some(vec![ToolCall { name: \"search\".to_string(), arguments: serde_json::json!({\"query\": \"test\"}) }]), stop_reason: Some(\"custom_stop\".to_string()), finish_reason: None }), error: None, failure: None, streaming: Some(llmposter::fixture::StreamingConfig { latency: Some(0), chunk_size: Some(5) }) }",
      "note": "Full struct literal - exposes all Fixture fields: match_rule, provider, response, error, failure, streaming; and StreamingConfig fields: latency, chunk_size"
    },
    "assertions": [
      "status == 200",
      "body contains \"custom_stop\"",
      "body contains \"event: message_stop\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_streaming_disconnect_after_ms_text",
    "api_being_tested": "FailureConfig::disconnect_after_ms - abort SSE connection after N milliseconds (text response)",
    "setup_code": {
      "initialization": "Fixture::new().respond_with_content(\"Hello world this is a long response\").with_streaming(Some(0), Some(5)).with_failure(FailureConfig { disconnect_after_ms: Some(0), ..FailureConfig::default() })"
    },
    "assertions": [
      "status == 200",
      "body does NOT contain \"event: message_stop\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_streaming_disconnect_after_ms_tool_call",
    "api_being_tested": "FailureConfig::disconnect_after_ms - abort SSE connection after N milliseconds (tool_use response)",
    "setup_code": {
      "initialization": "Fixture::new().respond_with_tool_calls([ToolCall { name: \"get_weather\", arguments: {\"location\": \"London\"} }]).with_streaming(Some(0), Some(5)).with_failure(FailureConfig { disconnect_after_ms: Some(0), ..FailureConfig::default() })"
    },
    "assertions": [
      "status == 200",
      "body does NOT contain \"event: message_stop\""
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "anthropic_streaming_tool_call_latency",
    "api_being_tested": "with_streaming latency on tool_use SSE stream - per-frame delay accumulates",
    "setup_code": {
      "initialization": "Fixture::new().respond_with_tool_calls([ToolCall { name: \"search\", arguments: {\"q\": \"test\"} }]).with_streaming(Some(50), Some(5))"
    },
    "assertions": [
      "status == 200",
      "body contains \"event: message_stop\"",
      "elapsed >= 200ms (tool call stream ~7 frames * 50ms)"
    ],
    "deprecation_status": "current"
  },
  {
    "pattern_id": "fixture_struct_full_literal",
    "api_being_tested": "Fixture - all public fields (struct literal construction)",
    "setup_code": {
      "note": "Full struct literal reveals public API surface of Fixture",
      "fields": {
        "match_rule": "Option<...> — user message / model filter rule",
        "provider": "Option<Provider> — Provider::OpenAI | Provider::Anthropic",
        "response": "Option<FixtureResponse>",
        "error": "Option<...> — for with_error()",
        "failure": "Option<FailureConfig>",
        "streaming": "Option<StreamingConfig>"
      }
    },
    "deprecation_status": "current"
  },
  {
    "pattern_id": "fixture_response_struct",
    "api_being_tested": "FixtureResponse - all fields",
    "fields": {
      "content": "Option<String> — text response body",
      "tool_calls": "Option<Vec<ToolCall>> — tool use responses",
      "stop_reason": "Option<String> — Anthropic stop_reason override",
      "finish_reason": "Option<String> — OpenAI finish_reason override (not tested here)"
    },
    "deprecation_status": "current"
  },
  {
    "pattern_id": "streaming_config_struct",
    "api_being_tested": "StreamingConfig - direct struct construction via llmposter::fixture::StreamingConfig",
    "fields": {
      "latency": "Option<u64> — ms delay between each SSE frame",
      "chunk_size": "Option<usize> — characters per SSE delta frame"
    },
    "deprecation_status": "current"
  },
  {
    "pattern_id": "failure_config_default_spread",
    "api_being_tested": "FailureConfig - Default trait implementation used with struct update syntax",
    "setup_code": {
      "example": "FailureConfig { truncate_after_frames: Some(2), ..FailureConfig::default() }"
    },
    "fields": {
      "latency_ms": "Option<u64>",
      "corrupt_body": "Option<bool>",
      "truncate_after_frames": "Option<u64>",
      "disconnect_after_ms": "Option<u64>"
    },
    "deprecation_status": "current"
  },
  {
    "pattern_id": "server_url",
    "api_being_tested": "Server::url() - returns base URL string for the running test server",
    "usage_pattern": {
      "call": "server.url()",
      "used_as": "format!(\"{}/v1/messages\", server.url())"
    },
    "deprecation_status": "current"
  }
]
```
