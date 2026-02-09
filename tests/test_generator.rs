#![allow(clippy::field_reassign_with_default)]
// Generator and integration tests
use skilldo::config::Config;
use skilldo::detector::Language;
use skilldo::llm::client::MockLlmClient;
use skilldo::pipeline::collector::CollectedData;
use skilldo::pipeline::generator::Generator;
use std::str::FromStr;

#[tokio::test]
async fn test_generator_with_mock_client() {
    let client = Box::new(MockLlmClient::new());
    let generator = Generator::new(client, 3);

    let data = CollectedData {
        package_name: "test_package".to_string(),
        version: "1.0.0".to_string(),
        license: None,
        project_urls: vec![],
        language: Language::Python,
        examples_content: String::new(),
        source_content: "def hello(): pass".to_string(),
        test_content: "def test_hello(): pass".to_string(),
        docs_content: "# Documentation".to_string(),
        changelog_content: "# Changelog".to_string(),
        source_file_count: 1,
    };

    let result = generator.generate(&data).await;
    assert!(result.is_ok());
    let skill_md = result.unwrap();
    assert!(skill_md.contains("---"));
    assert!(skill_md.contains("name:"));
}

#[tokio::test]
async fn test_generator_with_custom_instructions() {
    let client = Box::new(MockLlmClient::new());
    let mut prompts_config = skilldo::config::PromptsConfig::default();
    prompts_config.agent4_custom = Some("Test custom instructions".to_string());
    let generator = Generator::new(client, 3).with_prompts_config(prompts_config);

    let data = CollectedData {
        package_name: "test_package".to_string(),
        version: "1.0.0".to_string(),
        license: None,
        project_urls: vec![],
        language: Language::Python,
        examples_content: String::new(),
        source_content: "def hello(): pass".to_string(),
        test_content: "def test_hello(): pass".to_string(),
        docs_content: "# Documentation".to_string(),
        changelog_content: "# Changelog".to_string(),
        source_file_count: 1,
    };

    let result: Result<String, anyhow::Error> = generator.generate(&data).await;
    assert!(result.is_ok());
}

#[test]
fn test_generator_creation() {
    let client = Box::new(MockLlmClient::new());
    // If it compiles and doesn't crash, the test passes
    let _generator = Generator::new(client, 5);
}

#[test]
fn test_config_serialization() {
    let config = Config::default();
    let serialized = toml::to_string(&config);
    assert!(serialized.is_ok());
}

#[test]
fn test_language_clone() {
    let lang1 = Language::Python;
    let lang2 = lang1.clone();
    assert_eq!(lang1.as_str(), lang2.as_str());
}

#[test]
fn test_language_all_variants() {
    let languages = vec![
        Language::Python,
        Language::JavaScript,
        Language::Rust,
        Language::Go,
    ];

    for lang in languages {
        assert!(!lang.as_str().is_empty());
        assert!(Language::from_str(lang.as_str()).is_ok());
    }
}
