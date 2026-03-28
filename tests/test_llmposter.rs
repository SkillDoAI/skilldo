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

/// 429 error responses are handled gracefully.
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
