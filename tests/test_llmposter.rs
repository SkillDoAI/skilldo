//! Integration tests using llmposter as a mock LLM backend.
//!
//! Tier 1: Client coverage (Anthropic, OpenAI, streaming, retry, auth)
//! Tier 2: Failure handling (latency, disconnect, provider routing)
//! Tier 3: Deterministic pipeline (full extract→map→learn→create→review)

use llmposter::fixture::FailureConfig;
use llmposter::{Fixture, Provider, ServerBuilder};
use skilldo::llm::client::LlmClient;
use skilldo::llm::client_impl::OpenAIClient;

fn make_openai_client(base_url: &str) -> OpenAIClient {
    OpenAIClient::with_base_url(
        "fake-api-key".to_string(),
        "mock-model".to_string(),
        format!("{base_url}/v1"),
        8192,
        30,
    )
    .unwrap()
}

// ============================================================================
// Tier 1 — Client coverage
// ============================================================================

/// Basic OpenAI-compatible completion against llmposter.
#[tokio::test]
async fn test_openai_client_basic() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new().respond_with_content("Mock response from llmposter"))
        .build()
        .await
        .unwrap();

    let client = make_openai_client(&server.url());
    let text = LlmClient::complete(&client, "Say hello").await.unwrap();
    assert_eq!(text, "Mock response from llmposter");
}

/// Anthropic client against llmposter /v1/messages endpoint.
#[tokio::test]
async fn test_anthropic_client_basic() {
    use skilldo::llm::client_impl::AnthropicClient;

    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .for_provider(Provider::Anthropic)
                .respond_with_content("Anthropic mock response"),
        )
        .build()
        .await
        .unwrap();

    let client = AnthropicClient::with_base_url(
        "fake-api-key".to_string(),
        "mock-model".to_string(),
        server.url(),
        8192,
        30,
    )
    .unwrap();

    let text = LlmClient::complete(&client, "Hello from Anthropic")
        .await
        .unwrap();
    assert_eq!(text, "Anthropic mock response");
}

/// Gemini client against llmposter /v1beta/models endpoint.
#[tokio::test]
async fn test_gemini_client_basic() {
    use skilldo::llm::client_impl::GeminiClient;

    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .for_provider(Provider::Gemini)
                .respond_with_content("Gemini mock response"),
        )
        .build()
        .await
        .unwrap();

    let client = GeminiClient::with_base_url(
        "fake-api-key".to_string(),
        "mock-model".to_string(),
        server.url(),
        8192,
        30,
    )
    .unwrap();

    let text = LlmClient::complete(&client, "Hello from Gemini")
        .await
        .unwrap();
    assert_eq!(text, "Gemini mock response");
}

/// Fixture matching simulates pipeline stages (extract/map/learn).
#[tokio::test]
async fn test_fixture_matching_pipeline_stages() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .match_user_message("extract")
                .respond_with_content("## API Surface\nfn hello() -> String"),
        )
        .fixture(
            Fixture::new()
                .match_user_message("map")
                .respond_with_content("## Patterns\n- Basic usage"),
        )
        .fixture(Fixture::new().respond_with_content("Default fallback"))
        .build()
        .await
        .unwrap();

    let client = make_openai_client(&server.url());

    let r1 = LlmClient::complete(&client, "Please extract the API surface")
        .await
        .unwrap();
    assert!(r1.contains("API Surface"), "extract: {r1}");

    let r2 = LlmClient::complete(&client, "Please map the patterns")
        .await
        .unwrap();
    assert!(r2.contains("Patterns"), "map: {r2}");

    let r3 = LlmClient::complete(&client, "Something else entirely")
        .await
        .unwrap();
    assert!(r3.contains("Default"), "fallback: {r3}");
}

/// Raw client 429 error propagation (no RetryClient wrapping).
#[tokio::test]
async fn test_error_429_raw_client() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new().with_error(429, "Rate limited"))
        .build()
        .await
        .unwrap();

    let client = make_openai_client(&server.url());
    let result = LlmClient::complete(&client, "test").await;
    assert!(result.is_err(), "429 should produce an error");
}

/// Sequential calls reuse the same server.
#[tokio::test]
async fn test_sequential_calls_consistency() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new().respond_with_content("consistent"))
        .build()
        .await
        .unwrap();

    let client = make_openai_client(&server.url());
    for i in 0..5 {
        let text = LlmClient::complete(&client, &format!("call {i}"))
            .await
            .unwrap();
        assert_eq!(text, "consistent", "call {i}");
    }
}

/// Server returns 404 when no fixture matches.
#[tokio::test]
async fn test_no_fixture_match_returns_error() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .match_user_message("specific-keyword")
                .respond_with_content("matched"),
        )
        .build()
        .await
        .unwrap();

    let c = make_openai_client(&server.url());
    let result = LlmClient::complete(&c, "unrelated prompt").await;
    assert!(result.is_err(), "unmatched request should error");
}

/// Concurrent requests to the same server.
#[tokio::test]
async fn test_concurrent_requests() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new().respond_with_content("concurrent ok"))
        .build()
        .await
        .unwrap();

    let url = server.url();
    let mut handles = Vec::new();
    for _ in 0..5 {
        let c = make_openai_client(&url);
        handles.push(tokio::spawn(async move {
            LlmClient::complete(&c, "parallel").await.unwrap()
        }));
    }
    for h in handles {
        assert_eq!(h.await.unwrap(), "concurrent ok");
    }
}

/// /code/{N} utility endpoint for HTTP status testing.
#[tokio::test]
async fn test_code_endpoint_status_codes() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new().respond_with_content("fallback"))
        .build()
        .await
        .unwrap();

    let http = reqwest::Client::new();

    let resp = http
        .get(format!("{}/code/200", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = http
        .get(format!("{}/code/429", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 429);

    let resp = http
        .get(format!("{}/code/500", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);

    let resp = http
        .get(format!("{}/code/999", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

/// Bearer auth — requests without token get 401.
#[tokio::test]
async fn test_bearer_auth_rejection() {
    let server = ServerBuilder::new()
        .with_bearer_token("secret-token")
        .fixture(Fixture::new().respond_with_content("authorized"))
        .build()
        .await
        .unwrap();

    // Client doesn't send auth header — should get 401
    let client = make_openai_client(&server.url());
    let result = LlmClient::complete(&client, "test").await;
    assert!(result.is_err(), "missing auth should fail");
}

// ============================================================================
// Tier 2 — Failure handling
// ============================================================================

/// Provider-specific fixture routing (OpenAI vs Anthropic).
#[tokio::test]
async fn test_provider_specific_routing() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .for_provider(Provider::Anthropic)
                .respond_with_content("anthropic only"),
        )
        .fixture(
            Fixture::new()
                .for_provider(Provider::OpenAI)
                .respond_with_content("openai only"),
        )
        .build()
        .await
        .unwrap();

    // OpenAI client hits /v1/chat/completions → gets "openai only"
    let openai = make_openai_client(&server.url());
    let text = LlmClient::complete(&openai, "test").await.unwrap();
    assert_eq!(text, "openai only");
}

/// Latency injection — client should still succeed (within timeout).
#[tokio::test]
async fn test_latency_injection_within_timeout() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .respond_with_content("delayed response")
                .with_failure(FailureConfig {
                    latency_ms: Some(100),
                    ..FailureConfig::default()
                }),
        )
        .build()
        .await
        .unwrap();

    // Client timeout is 30s, latency is 100ms — should succeed
    let client = make_openai_client(&server.url());
    let start = std::time::Instant::now();
    let text = LlmClient::complete(&client, "test").await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(text, "delayed response");
    assert!(
        elapsed.as_millis() >= 90,
        "Should have waited ~100ms, got {}ms",
        elapsed.as_millis()
    );
}

/// Latency injection exceeding client timeout.
#[tokio::test]
async fn test_latency_exceeds_timeout() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .respond_with_content("too slow")
                .with_failure(FailureConfig {
                    latency_ms: Some(5000),
                    ..FailureConfig::default()
                }),
        )
        .build()
        .await
        .unwrap();

    // Client with 1-second timeout against 5-second latency
    let client = OpenAIClient::with_base_url(
        "fake-key".to_string(),
        "mock".to_string(),
        format!("{}/v1", server.url()),
        8192,
        1, // 1 second timeout
    )
    .unwrap();

    let result = LlmClient::complete(&client, "test").await;
    assert!(result.is_err(), "should timeout");
}

/// Corrupt body returns unparseable response.
#[tokio::test]
async fn test_corrupt_body_injection() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .respond_with_content("this will be corrupted")
                .with_failure(FailureConfig {
                    corrupt_body: Some(true),
                    ..FailureConfig::default()
                }),
        )
        .build()
        .await
        .unwrap();

    let client = make_openai_client(&server.url());
    let result = LlmClient::complete(&client, "test").await;
    // Corrupt body returns text/plain "overloaded" — client can't parse as JSON
    assert!(result.is_err(), "corrupt body should produce parse error");
}

/// SSE streaming via OpenAI client — verify we get the full content back.
#[tokio::test]
async fn test_openai_streaming_response() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .respond_with_content("Streamed content from mock")
                .with_streaming(Some(0), Some(5)),
        )
        .build()
        .await
        .unwrap();

    // Make a raw streaming request to verify SSE works
    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{}/v1/chat/completions", server.url()))
        .json(&serde_json::json!({
            "model": "mock",
            "messages": [{"role": "user", "content": "test"}],
            "stream": true
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("text/event-stream"), "Expected SSE, got: {ct}");

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("data:"),
        "SSE body should contain data: lines"
    );
}

/// SSE streaming via Anthropic endpoint.
#[tokio::test]
async fn test_anthropic_streaming_response() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .for_provider(Provider::Anthropic)
                .respond_with_content("Anthropic streamed content")
                .with_streaming(Some(0), Some(5)),
        )
        .build()
        .await
        .unwrap();

    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{}/v1/messages", server.url()))
        .header("x-api-key", "fake")
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": "mock",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": "test"}],
            "stream": true
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("content_block_delta") || body.contains("data:"),
        "Anthropic SSE should contain delta events"
    );
}

/// OpenAI-compatible client with /v1/responses URL — documents the body format mismatch.
/// The client sends Chat Completions JSON but the Responses endpoint expects a different schema.
#[tokio::test]
async fn test_openai_compatible_responses_endpoint() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .for_provider(Provider::Responses)
                .respond_with_content("responses api content"),
        )
        .build()
        .await
        .unwrap();

    // Client pointed at /v1/responses — sends Chat Completions body
    let client = OpenAIClient::with_base_url(
        "fake-key".to_string(),
        "mock-model".to_string(),
        format!("{}/v1/responses", server.url()),
        8192,
        30,
    )
    .unwrap();

    // The URL is preserved (no /chat/completions appended) but the body format
    // is Chat Completions, not Responses API. llmposter routes by URL path,
    // so this exercises the path-preservation logic.
    let result = LlmClient::complete(&client, "test").await;
    // May succeed or fail depending on how llmposter handles the schema mismatch
    match result {
        Ok(text) => assert!(!text.is_empty(), "Got response from responses endpoint"),
        Err(e) => {
            // Expected: body format doesn't match what responses endpoint expects
            let err = e.to_string();
            assert!(
                err.contains("404")
                    || err.contains("error")
                    || err.contains("parse")
                    || err.contains("status"),
                "Expected format/routing error, got: {err}"
            );
        }
    }
}

/// Stream truncation — server cuts SSE after N frames.
#[tokio::test]
async fn test_stream_truncation() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .respond_with_content("This is a long response that will be truncated")
                .with_streaming(Some(0), Some(5))
                .with_failure(FailureConfig {
                    truncate_after_frames: Some(2),
                    ..FailureConfig::default()
                }),
        )
        .build()
        .await
        .unwrap();

    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{}/v1/chat/completions", server.url()))
        .json(&serde_json::json!({
            "model": "mock",
            "messages": [{"role": "user", "content": "test"}],
            "stream": true
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    // Stream should be shorter than full content — truncated after 2 frames
    let data_lines: Vec<&str> = body.lines().filter(|l| l.starts_with("data:")).collect();
    // With 2 frames truncation, we should get fewer data events than the full content would produce
    assert!(
        data_lines.len() <= 5,
        "Truncated stream should have few data events, got {}",
        data_lines.len()
    );
}

/// Disconnect mid-stream — server drops TCP connection.
#[tokio::test]
async fn test_disconnect_mid_stream() {
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .respond_with_content("This will be cut short by disconnect")
                .with_streaming(Some(10), Some(5))
                .with_failure(FailureConfig {
                    disconnect_after_ms: Some(20),
                    ..FailureConfig::default()
                }),
        )
        .build()
        .await
        .unwrap();

    let http = reqwest::Client::new();
    let result = http
        .post(format!("{}/v1/chat/completions", server.url()))
        .json(&serde_json::json!({
            "model": "mock",
            "messages": [{"role": "user", "content": "test"}],
            "stream": true
        }))
        .send()
        .await;

    // Disconnect should cause either a connection error or a body-read error
    let disconnect_observed = match result {
        Err(_) => true, // Connection failed entirely
        Ok(resp) => {
            let body_result = resp.text().await;
            match body_result {
                Err(_) => true,                       // Body read failed mid-stream
                Ok(body) => !body.contains("[DONE]"), // Partial body, no completion marker
            }
        }
    };
    assert!(
        disconnect_observed,
        "Disconnect should produce an error or incomplete response"
    );
}

// ============================================================================
// Tier 3 — Deterministic pipeline
// ============================================================================

/// Full pipeline test: extract → map → learn → create → review
/// using real stage outputs captured from sonnet-run5 as fixtures.
#[tokio::test]
async fn test_deterministic_pipeline() {
    use skilldo::detector::Language;
    use skilldo::pipeline::collector::CollectedData;
    use skilldo::pipeline::generator::Generator;

    // Clean, minimal responses for each pipeline stage
    let extract_response = r#"{"api_surface": [{"name": "ServerBuilder", "type": "struct", "publicity_score": "high"}], "documented_apis": []}"#;
    let create_output = "---\nname: llmposter\ndescription: Mock LLM server\nmetadata:\n  version: \"0.4.1\"\n  ecosystem: rust\n---\n\n## Imports\n\n```rust\nuse llmposter::{Fixture, ServerBuilder};\n```\n\n```toml\n[dependencies]\nllmposter = \"0.4.1\"\n```\n\n## Core Patterns\n\n### Basic Mock Server\n\n```rust\nuse llmposter::{Fixture, ServerBuilder};\n\n#[tokio::test]\nasync fn test_basic() {\n    let server = ServerBuilder::new()\n        .fixture(Fixture::new().respond_with_content(\"hello\"))\n        .build().await.unwrap();\n    let _ = server.url();\n}\n```\n\n## Pitfalls\n\n### Wrong\n\n```rust\nFixture::new().match_user_message(\"\")\n```\n\n### Right\n\n```rust\nFixture::new().match_user_message(\"specific\")\n```\n\n## References\n\n- [Repository](https://github.com/SkillDoAI/llmposter)\n\n## API Reference\n\n**ServerBuilder::new()** — Creates a new mock server builder.\n";

    // Build llmposter server — fixture order matters (first match wins)
    // Create prompt contains "agent rules file" — match it specifically
    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .match_user_message("agent rules file")
                .respond_with_content(create_output),
        )
        // Extract/map/learn catch-all — returns simple JSON for all three
        .fixture(Fixture::new().respond_with_content(extract_response))
        .build()
        .await
        .unwrap();

    // Create client pointing at llmposter
    let client = OpenAIClient::with_base_url(
        "fake-key".to_string(),
        "mock-model".to_string(),
        format!("{}/v1", server.url()),
        16384,
        60,
    )
    .unwrap();

    // Build generator with no test/review to test just the core pipeline
    let generator = Generator::new(Box::new(client), 3)
        .with_test(false) // Skip test validation (no container/executor)
        .with_review(false) // Skip review for this basic test
        .with_security_scan(false);

    // Minimal collected data
    let data = CollectedData {
        package_name: "llmposter".to_string(),
        version: "0.4.1".to_string(),
        license: Some("AGPL-3.0-or-later".to_string()),
        project_urls: vec![],
        language: Language::Rust,
        source_file_count: 10,
        examples_content: String::new(),
        test_content: String::new(),
        docs_content: String::new(),
        source_content: "pub struct ServerBuilder;\npub struct Fixture;\n".to_string(),
        changelog_content: String::new(),
        dependencies: vec![],
    };

    // Run the pipeline
    let result = generator.generate(&data).await;
    assert!(
        result.is_ok(),
        "Pipeline should succeed: {:?}",
        result.err()
    );

    let output = result.unwrap();
    let skill_md = output.skill_md;

    // Verify the output is a valid SKILL.md
    assert!(skill_md.contains("---"), "Should have frontmatter");
    assert!(skill_md.contains("llmposter"), "Should mention the package");
    assert!(
        skill_md.contains("## Imports") || skill_md.contains("## Core Patterns"),
        "Should have standard sections"
    );
}

/// Pipeline with review enabled — review passes on first attempt.
#[tokio::test]
async fn test_deterministic_pipeline_with_review() {
    use skilldo::detector::Language;
    use skilldo::pipeline::collector::CollectedData;
    use skilldo::pipeline::generator::Generator;

    let extract_response = r#"{"api_surface": [{"name": "ServerBuilder", "type": "struct", "publicity_score": "high"}], "documented_apis": []}"#;
    let review_pass = r#"{"passed": true, "issues": []}"#;
    let create_output = "---\nname: llmposter\ndescription: Mock LLM server\nmetadata:\n  version: \"0.4.1\"\n  ecosystem: rust\n---\n\n## Imports\n\n```rust\nuse llmposter::{Fixture, ServerBuilder};\n```\n\n```toml\n[dependencies]\nllmposter = \"0.4.1\"\n```\n\n## Core Patterns\n\n### Basic Mock Server\n\n```rust\nuse llmposter::{Fixture, ServerBuilder};\n\n#[tokio::test]\nasync fn test_basic() {\n    let server = ServerBuilder::new()\n        .fixture(Fixture::new().respond_with_content(\"hello\"))\n        .build().await.unwrap();\n    let _ = server.url();\n}\n```\n\n## Pitfalls\n\n### Wrong\n\n```rust\nFixture::new().match_user_message(\"\")\n```\n\n### Right\n\n```rust\nFixture::new().match_user_message(\"specific\")\n```\n\n## References\n\n- [Repository](https://github.com/SkillDoAI/llmposter)\n\n## API Reference\n\n**ServerBuilder::new()** — Creates a new mock server builder.\n";

    let server = ServerBuilder::new()
        .fixture(
            Fixture::new()
                .match_user_message("SKILL.MD UNDER REVIEW")
                .respond_with_content(review_pass),
        )
        .fixture(
            Fixture::new()
                .match_user_message("agent rules file")
                .respond_with_content(create_output),
        )
        .fixture(Fixture::new().respond_with_content(extract_response))
        .build()
        .await
        .unwrap();

    let client = OpenAIClient::with_base_url(
        "fake-key".to_string(),
        "mock-model".to_string(),
        format!("{}/v1", server.url()),
        16384,
        60,
    )
    .unwrap();

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(true)
        .with_security_scan(false);

    let data = CollectedData {
        package_name: "llmposter".to_string(),
        version: "0.4.1".to_string(),
        license: Some("AGPL-3.0-or-later".to_string()),
        project_urls: vec![],
        language: Language::Rust,
        source_file_count: 10,
        examples_content: String::new(),
        test_content: String::new(),
        docs_content: String::new(),
        source_content: "pub struct ServerBuilder;\npub struct Fixture;\n".to_string(),
        changelog_content: String::new(),
        dependencies: vec![],
    };

    let result = generator.generate(&data).await;
    assert!(
        result.is_ok(),
        "Pipeline with review should succeed: {:?}",
        result.err()
    );
}
