//! Integration tests using llmposter as a mock LLM backend.
//!
//! These tests verify that skilldo's LLM clients can communicate with
//! a real HTTP server without hitting actual provider APIs.

use llmposter::{Fixture, ServerBuilder};
use skilldo::llm::client::LlmClient;
use skilldo::llm::client_impl::OpenAIClient;

fn make_client(base_url: &str) -> OpenAIClient {
    OpenAIClient::with_base_url(
        "fake-api-key".to_string(),
        "mock-model".to_string(),
        format!("{base_url}/v1"),
        8192,
        30,
    )
    .unwrap()
}

/// Basic completion against llmposter mock server.
#[tokio::test]
async fn test_openai_client_with_llmposter() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new().respond_with_content("Mock response from llmposter"))
        .build()
        .await
        .unwrap();

    let client = make_client(&server.url());
    let text = LlmClient::complete(&client, "Say hello").await.unwrap();
    assert_eq!(text, "Mock response from llmposter");
}

/// Fixture matching simulates pipeline stages (extract/map/learn).
#[tokio::test]
async fn test_fixture_matching_simulates_pipeline_stages() {
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

    let client = make_client(&server.url());

    let r1 = LlmClient::complete(&client, "Please extract the API surface")
        .await
        .unwrap();
    assert!(r1.contains("API Surface"), "extract response: {r1}");

    let r2 = LlmClient::complete(&client, "Please map the patterns")
        .await
        .unwrap();
    assert!(r2.contains("Patterns"), "map response: {r2}");

    let r3 = LlmClient::complete(&client, "Something else entirely")
        .await
        .unwrap();
    assert!(r3.contains("Default"), "default response: {r3}");
}

/// Raw client 429 error propagation (no RetryClient wrapping).
#[tokio::test]
async fn test_error_response_handled() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new().with_error(429, "Rate limited"))
        .build()
        .await
        .unwrap();

    let client = make_client(&server.url());
    let response = LlmClient::complete(&client, "test").await;
    assert!(response.is_err(), "429 should produce an error");
}

/// Sequential calls reuse the same server (connection persistence).
#[tokio::test]
async fn test_sequential_calls_same_server() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new().respond_with_content("consistent"))
        .build()
        .await
        .unwrap();

    let client = make_client(&server.url());
    for i in 0..5 {
        let text = LlmClient::complete(&client, &format!("call {i}"))
            .await
            .unwrap();
        assert_eq!(text, "consistent", "call {i} should return same fixture");
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

    let c = make_client(&server.url());
    // "unrelated" doesn't contain "specific-keyword" — no match → 404
    let response = LlmClient::complete(&c, "unrelated prompt").await;
    assert!(response.is_err(), "unmatched request should error");
}

/// Server handles concurrent requests.
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
        let c = make_client(&url);
        handles.push(tokio::spawn(async move {
            LlmClient::complete(&c, "parallel").await.unwrap()
        }));
    }
    for h in handles {
        let text = h.await.unwrap();
        assert_eq!(text, "concurrent ok");
    }
}

/// Test /code/{N} utility endpoint for HTTP status code testing.
#[tokio::test]
async fn test_code_endpoint_status_codes() {
    let server = ServerBuilder::new()
        .fixture(Fixture::new().respond_with_content("fallback"))
        .build()
        .await
        .unwrap();

    let http = reqwest::Client::new();

    // 200 OK
    let resp = http
        .get(format!("{}/code/200", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // 429 Too Many Requests
    let resp = http
        .get(format!("{}/code/429", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 429);

    // 500 Internal Server Error
    let resp = http
        .get(format!("{}/code/500", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);

    // Invalid code returns 400
    let resp = http
        .get(format!("{}/code/999", server.url()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}
