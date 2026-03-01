//! Pipeline integration tests using recorded LLM fixtures.
//!
//! These tests replay captured LLM responses instead of calling a live API,
//! making them deterministic and CI-friendly. To upgrade fixtures with real
//! captured data, replace the JSON files in tests/fixtures/.
//!
//! Each fixture file contains responses for all pipeline stages:
//! extract, map, learn, create, review_introspect, review_verdict.

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use skilldo::detector::Language;
use skilldo::llm::client::LlmClient;
use skilldo::pipeline::collector::CollectedData;
use skilldo::pipeline::generator::Generator;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ============================================================================
// Fixture Loading
// ============================================================================

#[derive(Debug, Deserialize)]
struct FixtureFile {
    metadata: FixtureMetadata,
    responses: HashMap<String, FixtureResponse>,
}

#[derive(Debug, Deserialize)]
struct FixtureMetadata {
    package: String,
    #[allow(dead_code)]
    version: String,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    captured: String,
}

#[derive(Debug, Deserialize)]
struct FixtureResponse {
    prompt_contains: String,
    response: String,
}

fn load_fixture(name: &str) -> FixtureFile {
    let path = format!(
        "{}/tests/fixtures/{}.json",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to load fixture {}: {}", path, e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse fixture {}: {}", path, e))
}

// ============================================================================
// FixtureLlmClient — replays recorded responses
// ============================================================================

/// LLM client that replays recorded fixture responses.
/// Matches incoming prompts against fixture `prompt_contains` patterns.
struct FixtureLlmClient {
    responses: Vec<(String, String)>, // (prompt_contains, response)
    call_log: Arc<std::sync::Mutex<Vec<String>>>,
}

impl FixtureLlmClient {
    fn from_fixture(fixture: &FixtureFile) -> Self {
        let responses: Vec<(String, String)> = fixture
            .responses
            .values()
            .map(|r| (r.prompt_contains.clone(), r.response.clone()))
            .collect();

        Self {
            responses,
            call_log: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl LlmClient for FixtureLlmClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        for (pattern, response) in &self.responses {
            if prompt.contains(pattern.as_str()) {
                self.call_log.lock().unwrap().push(pattern.clone());
                return Ok(response.clone());
            }
        }
        anyhow::bail!(
            "FixtureLlmClient: no fixture matched prompt (first 100 chars): {}",
            prompt.chars().take(100).collect::<String>()
        );
    }
}

/// Variant that controls review pass/fail across attempts.
struct ReviewFixtureClient {
    fixture: FixtureFile,
    review_call_count: Arc<AtomicUsize>,
    review_pass_on: usize, // Pass on this review attempt (0-indexed)
}

impl ReviewFixtureClient {
    fn new(fixture: FixtureFile, pass_on: usize) -> Self {
        Self {
            fixture,
            review_call_count: Arc::new(AtomicUsize::new(0)),
            review_pass_on: pass_on,
        }
    }
}

#[async_trait]
impl LlmClient for ReviewFixtureClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        // Review verdict — pick pass or fail response based on attempt count
        if prompt.contains("quality gate for a generated SKILL.md") {
            let attempt = self.review_call_count.fetch_add(1, Ordering::SeqCst);
            let key = if attempt >= self.review_pass_on {
                "review_verdict_pass"
            } else {
                "review_verdict_fail"
            };
            if let Some(r) = self.fixture.responses.get(key) {
                return Ok(r.response.clone());
            }
            // Fallback: pass
            return Ok(r#"{"passed": true, "issues": []}"#.to_string());
        }

        // Create fix (review sends SKILL.md back for patching)
        if prompt.contains("Here is the current SKILL.md") {
            if let Some(r) = self.fixture.responses.get("create_fix") {
                return Ok(r.response.clone());
            }
        }

        // All other stages — match by prompt_contains
        for r in self.fixture.responses.values() {
            if prompt.contains(r.prompt_contains.as_str()) {
                return Ok(r.response.clone());
            }
        }

        anyhow::bail!(
            "ReviewFixtureClient: no fixture matched prompt (first 100 chars): {}",
            prompt.chars().take(100).collect::<String>()
        );
    }
}

// ============================================================================
// Test Data Helpers
// ============================================================================

fn fastapi_collected_data() -> CollectedData {
    CollectedData {
        package_name: "fastapi".to_string(),
        version: "0.115.0".to_string(),
        license: Some("MIT".to_string()),
        project_urls: vec![
            (
                "homepage".to_string(),
                "https://fastapi.tiangolo.com".to_string(),
            ),
            (
                "repository".to_string(),
                "https://github.com/tiangolo/fastapi".to_string(),
            ),
        ],
        language: Language::Python,
        examples_content: "# Example\nfrom fastapi import FastAPI\napp = FastAPI()\n\n@app.get('/')\ndef root():\n    return {'hello': 'world'}\n".to_string(),
        test_content: "from fastapi.testclient import TestClient\ndef test_root():\n    client = TestClient(app)\n    response = client.get('/')\n    assert response.status_code == 200\n".to_string(),
        docs_content: "# FastAPI\n\nFastAPI is a modern, fast (high-performance), web framework for building APIs with Python 3.7+ based on standard Python type hints.\n".to_string(),
        source_content: "class FastAPI(Starlette):\n    def __init__(self, *, debug=False, routes=None, title='FastAPI', description='', version='0.1.0'):\n        ...\n".to_string(),
        changelog_content: "## 0.115.0\n- Add support for Pydantic v2\n- Performance improvements\n".to_string(),
        source_file_count: 42,
    }
}

// ============================================================================
// Tests: Full Pipeline with Fixtures
// ============================================================================

#[tokio::test]
async fn test_fixture_full_pipeline_no_review() {
    let fixture = load_fixture("fastapi_session");
    let client = FixtureLlmClient::from_fixture(&fixture);

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(false);

    let data = fastapi_collected_data();
    let output = generator.generate(&data).await.unwrap();

    // Verify output has expected structure
    assert!(output.skill_md.contains("---"), "should have frontmatter");
    assert!(
        output.skill_md.contains("name: fastapi"),
        "should have correct package name"
    );
    assert!(
        output.skill_md.contains("ecosystem: python"),
        "should have ecosystem"
    );
    assert!(
        output.skill_md.contains("## Imports"),
        "should have Imports section"
    );
    assert!(
        output.skill_md.contains("## Core Patterns"),
        "should have Core Patterns section"
    );
    assert!(
        output.skill_md.contains("## Pitfalls"),
        "should have Pitfalls section"
    );
    assert!(
        output.skill_md.contains("FastAPI"),
        "should mention FastAPI"
    );
    assert!(output.unresolved_warnings.is_empty());
}

#[tokio::test]
async fn test_fixture_pipeline_with_review_pass() {
    let fixture = load_fixture("fastapi_session");
    // Review passes on first attempt
    let client = ReviewFixtureClient::new(fixture, 0);

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(true)
        .with_review_max_retries(2);

    let data = fastapi_collected_data();
    let output = generator.generate(&data).await.unwrap();

    assert!(output.skill_md.contains("---"));
    assert!(output.skill_md.contains("fastapi"));
    assert!(output.unresolved_warnings.is_empty());
}

#[tokio::test]
async fn test_fixture_pipeline_review_fail_then_pass() {
    let fixture = load_fixture("fastapi_session");
    // Review fails first, passes on second attempt
    let client = ReviewFixtureClient::new(fixture, 1);

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(true)
        .with_review_max_retries(3);

    let data = fastapi_collected_data();
    let output = generator.generate(&data).await.unwrap();

    assert!(output.skill_md.contains("---"));
    assert!(
        output.unresolved_warnings.is_empty(),
        "review should have passed on retry"
    );
}

#[tokio::test]
async fn test_fixture_pipeline_review_exhausted() {
    let fixture = load_fixture("fastapi_session");
    // Review never passes (pass_on > max_retries)
    let client = ReviewFixtureClient::new(fixture, usize::MAX);

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(true)
        .with_review_max_retries(1);

    let data = fastapi_collected_data();
    let output = generator.generate(&data).await.unwrap();

    assert!(output.skill_md.contains("---"));
    assert!(
        !output.unresolved_warnings.is_empty(),
        "should have unresolved warnings when review exhausted"
    );
    assert_eq!(output.unresolved_warnings[0].category, "accuracy");
    assert!(output.unresolved_warnings[0]
        .complaint
        .contains("signature"));
}

#[tokio::test]
async fn test_fixture_all_stages_called() {
    let fixture = load_fixture("fastapi_session");
    let client = FixtureLlmClient::from_fixture(&fixture);
    let call_log = client.call_log.clone();

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(false);

    let data = fastapi_collected_data();
    generator.generate(&data).await.unwrap();

    let log = call_log.lock().unwrap().clone();
    // All 4 extraction stages should be called
    assert!(
        log.iter()
            .any(|s| s.contains("Extract the complete public API")),
        "extract stage should be called"
    );
    assert!(
        log.iter()
            .any(|s| s.contains("Extract correct usage patterns")),
        "map stage should be called"
    );
    assert!(
        log.iter()
            .any(|s| s.contains("Extract conventions, best practices")),
        "learn stage should be called"
    );
    assert!(
        log.iter()
            .any(|s| s.contains("creating an agent rules file")),
        "create stage should be called"
    );
}

#[tokio::test]
async fn test_fixture_output_passes_linter() {
    let fixture = load_fixture("fastapi_session");
    let client = FixtureLlmClient::from_fixture(&fixture);

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(false);

    let data = fastapi_collected_data();
    let output = generator.generate(&data).await.unwrap();

    // Run the linter on the output — it should pass (no errors)
    let linter = skilldo::lint::SkillLinter::new();
    let issues = linter.lint(&output.skill_md).unwrap();
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| matches!(i.severity, skilldo::lint::Severity::Error))
        .collect();
    assert!(
        errors.is_empty(),
        "Fixture pipeline output should pass linter, got {} errors: {:?}",
        errors.len(),
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_fixture_normalizer_injects_metadata() {
    let fixture = load_fixture("fastapi_session");
    let client = FixtureLlmClient::from_fixture(&fixture);

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(false)
        .with_model_name("gpt-4.1".to_string());

    let data = fastapi_collected_data();
    let output = generator.generate(&data).await.unwrap();

    // Normalizer should inject generated_with into frontmatter
    assert!(
        output.skill_md.contains("generated_with:"),
        "should have generated_with field"
    );
    // Normalizer should preserve/add homepage from project_urls
    assert!(
        output.skill_md.contains("fastapi.tiangolo.com") || output.skill_md.contains("homepage"),
        "should reference homepage URL"
    );
}

#[tokio::test]
async fn test_fixture_non_python_language() {
    let fixture = load_fixture("fastapi_session");
    let client = FixtureLlmClient::from_fixture(&fixture);

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(false);

    let mut data = fastapi_collected_data();
    data.language = Language::Rust;
    data.package_name = "fastapi".to_string(); // Keep same to match fixtures

    let output = generator.generate(&data).await.unwrap();

    // Should still produce output — non-Python skips functional validation
    assert!(output.skill_md.contains("---"));
    // Normalizer uses the language param to set ecosystem when frontmatter
    // already has ecosystem: python from the LLM — normalizer preserves existing
    // frontmatter, so it stays python. The key check is that the pipeline
    // completes without error for non-Python languages.
    assert!(!output.skill_md.is_empty());
}

#[tokio::test]
async fn test_fixture_sequential_extraction() {
    let fixture = load_fixture("fastapi_session");
    let client = FixtureLlmClient::from_fixture(&fixture);

    let generator = Generator::new(Box::new(client), 3)
        .with_test(false)
        .with_review(false)
        .with_parallel_extraction(false); // Force sequential

    let data = fastapi_collected_data();
    let output = generator.generate(&data).await.unwrap();

    assert!(output.skill_md.contains("---"));
    assert!(output.skill_md.contains("fastapi"));
}

#[tokio::test]
async fn test_fixture_per_stage_clients() {
    let fixture = load_fixture("fastapi_session");

    // Use separate fixture clients for each stage
    let main_client = FixtureLlmClient::from_fixture(&fixture);
    let extract_client = FixtureLlmClient::from_fixture(&fixture);
    let map_client = FixtureLlmClient::from_fixture(&fixture);
    let learn_client = FixtureLlmClient::from_fixture(&fixture);
    let create_client = FixtureLlmClient::from_fixture(&fixture);
    let review_client = FixtureLlmClient::from_fixture(&fixture);

    let generator = Generator::new(Box::new(main_client), 3)
        .with_extract_client(Box::new(extract_client))
        .with_map_client(Box::new(map_client))
        .with_learn_client(Box::new(learn_client))
        .with_create_client(Box::new(create_client))
        .with_review_client(Box::new(review_client))
        .with_test(false)
        .with_review(false);

    let data = fastapi_collected_data();
    let output = generator.generate(&data).await.unwrap();

    assert!(output.skill_md.contains("---"));
    assert!(output.skill_md.contains("fastapi"));
}

// ============================================================================
// Tests: Fixture Data Integrity
// ============================================================================

#[test]
fn test_fixture_loads_and_has_required_stages() {
    let fixture = load_fixture("fastapi_session");

    assert_eq!(fixture.metadata.package, "fastapi");
    assert!(
        fixture.responses.contains_key("extract"),
        "should have extract stage"
    );
    assert!(
        fixture.responses.contains_key("map"),
        "should have map stage"
    );
    assert!(
        fixture.responses.contains_key("learn"),
        "should have learn stage"
    );
    assert!(
        fixture.responses.contains_key("create"),
        "should have create stage"
    );
    assert!(
        fixture.responses.contains_key("review_introspect"),
        "should have review_introspect stage"
    );
    assert!(
        fixture.responses.contains_key("review_verdict_pass"),
        "should have review_verdict_pass"
    );
    assert!(
        fixture.responses.contains_key("review_verdict_fail"),
        "should have review_verdict_fail"
    );
}

#[test]
fn test_fixture_responses_are_valid_json_where_expected() {
    let fixture = load_fixture("fastapi_session");

    // Extract, map, learn responses should be valid JSON
    for stage in ["extract", "map", "learn"] {
        let response = &fixture.responses[stage].response;
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(response);
        assert!(
            parsed.is_ok(),
            "{} response should be valid JSON: {:?}",
            stage,
            parsed.err()
        );
    }

    // Review verdict responses should be valid JSON
    for stage in ["review_verdict_pass", "review_verdict_fail"] {
        let response = &fixture.responses[stage].response;
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(response);
        assert!(
            parsed.is_ok(),
            "{} response should be valid JSON: {:?}",
            stage,
            parsed.err()
        );
    }

    // Create response should have frontmatter
    let create_response = &fixture.responses["create"].response;
    assert!(
        create_response.starts_with("---"),
        "create response should start with frontmatter"
    );
}

#[test]
fn test_fixture_create_response_passes_linter() {
    let fixture = load_fixture("fastapi_session");
    let create_response = &fixture.responses["create"].response;

    let linter = skilldo::lint::SkillLinter::new();
    let issues = linter.lint(create_response).unwrap();
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| matches!(i.severity, skilldo::lint::Severity::Error))
        .collect();
    assert!(
        errors.is_empty(),
        "Fixture create response should pass linter, got {} errors: {:?}",
        errors.len(),
        errors
            .iter()
            .map(|e| format!("[{}] {}", e.category, e.message))
            .collect::<Vec<_>>()
    );
}
