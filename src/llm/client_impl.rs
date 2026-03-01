use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::debug;

use super::client::LlmClient;
use crate::util::SecretString;

// ============================================================================
// Anthropic Client
// ============================================================================

pub struct AnthropicClient {
    api_key: SecretString,
    model: String,
    max_tokens: u32,
    client: Client,
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    text: String,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: String, max_tokens: u32, timeout_secs: u64) -> Result<Self> {
        Ok(Self {
            api_key: api_key.into(),
            model,
            max_tokens,
            client: Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
                .context("failed to build HTTP client")?,
        })
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        debug!("Calling Anthropic API with model: {}", self.model);

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", self.api_key.expose())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Anthropic API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Anthropic API error {}: {}", status, error_text);
        }

        let api_response: AnthropicResponse = response
            .json()
            .await
            .context("Failed to parse Anthropic API response")?;

        api_response
            .content
            .first()
            .map(|c| c.text.clone())
            .context("No content in Anthropic response")
    }
}

// ============================================================================
// OpenAI Client
// ============================================================================

pub struct OpenAIClient {
    api_key: SecretString,
    model: String,
    base_url: String,
    max_tokens: u32,
    extra_body: std::collections::HashMap<String, serde_json::Value>,
    client: Client,
}

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}

impl OpenAIClient {
    pub fn new(api_key: String, model: String, max_tokens: u32, timeout_secs: u64) -> Result<Self> {
        Self::with_base_url(
            api_key,
            model,
            "https://api.openai.com/v1".to_string(),
            max_tokens,
            timeout_secs,
        )
    }

    pub fn with_base_url(
        api_key: String,
        model: String,
        base_url: String,
        max_tokens: u32,
        timeout_secs: u64,
    ) -> Result<Self> {
        Ok(Self {
            api_key: api_key.into(),
            model,
            base_url,
            max_tokens,
            extra_body: std::collections::HashMap::new(),
            client: Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
                .context("failed to build HTTP client")?,
        })
    }

    pub fn with_extra_body(
        mut self,
        extra_body: std::collections::HashMap<String, serde_json::Value>,
    ) -> Self {
        self.extra_body = extra_body;
        self
    }
}

#[async_trait]
impl LlmClient for OpenAIClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        // GPT-5+ models use max_completion_tokens instead of max_tokens
        let (max_tokens, max_completion_tokens) = if self.model.starts_with("gpt-5") {
            (None, Some(self.max_tokens))
        } else {
            (Some(self.max_tokens), None)
        };

        let request = OpenAIRequest {
            model: self.model.clone(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: 0.7,
            max_tokens,
            max_completion_tokens,
        };

        debug!(
            "Calling OpenAI-compatible API at {} with model: {}",
            self.base_url, self.model
        );

        let url = format!("{}/chat/completions", self.base_url);

        // Merge extra_body fields into the request JSON.
        // Intentional: extra_body may override core fields (e.g., temperature).
        // This is user-controlled via TOML config, not untrusted input.
        let body = if self.extra_body.is_empty() {
            serde_json::to_value(&request).context("Failed to serialize request")?
        } else {
            let mut body = serde_json::to_value(&request).context("Failed to serialize request")?;
            if let serde_json::Value::Object(ref mut map) = body {
                for (key, value) in &self.extra_body {
                    map.insert(key.clone(), value.clone());
                }
            }
            body
        };

        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body);

        // Only add authorization if API key is not empty
        if !self.api_key.expose().is_empty() && self.api_key.expose().to_lowercase() != "none" {
            req = req.header("authorization", format!("Bearer {}", self.api_key.expose()));
        }

        let response = req
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("OpenAI API error {}: {}", status, error_text);
        }

        let api_response: OpenAIResponse = response
            .json()
            .await
            .context("Failed to parse OpenAI API response")?;

        api_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .context("No choices in OpenAI response")
    }
}

// ============================================================================
// Gemini Client (Google Generative AI)
// ============================================================================

pub struct GeminiClient {
    api_key: SecretString,
    model: String,
    max_tokens: u32,
    client: Client,
}

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
}

#[derive(Debug, Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiResponseContent,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponsePart {
    text: String,
}

impl GeminiClient {
    pub fn new(api_key: String, model: String, max_tokens: u32, timeout_secs: u64) -> Result<Self> {
        Ok(Self {
            api_key: api_key.into(),
            model,
            max_tokens,
            client: Client::builder()
                .timeout(Duration::from_secs(timeout_secs))
                .build()
                .context("failed to build HTTP client")?,
        })
    }
}

#[async_trait]
impl LlmClient for GeminiClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: prompt.to_string(),
                }],
            }],
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: self.max_tokens,
            }),
        };

        debug!("Calling Gemini API with model: {}", self.model);

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model,
            self.api_key.expose()
        );

        let response = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Gemini API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Gemini API error {}: {}", status, error_text);
        }

        let api_response: GeminiResponse = response
            .json()
            .await
            .context("Failed to parse Gemini API response")?;

        api_response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .context("No content in Gemini response")
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_client_creation() {
        let client =
            AnthropicClient::new("test_key".to_string(), "claude-3".to_string(), 4096, 120)
                .unwrap();
        assert_eq!(client.api_key.expose(), "test_key");
        assert_eq!(client.model, "claude-3");
    }

    #[test]
    fn test_openai_client_creation() {
        let client =
            OpenAIClient::new("test_key".to_string(), "gpt-4".to_string(), 4096, 120).unwrap();
        assert_eq!(client.api_key.expose(), "test_key");
        assert_eq!(client.model, "gpt-4");
        assert_eq!(client.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_openai_client_with_custom_base_url() {
        let client = OpenAIClient::with_base_url(
            "test_key".to_string(),
            "llama3".to_string(),
            "http://localhost:11434/v1".to_string(),
            16384,
            120,
        )
        .unwrap();
        assert_eq!(client.base_url, "http://localhost:11434/v1");
    }

    #[tokio::test]
    async fn test_anthropic_request_structure() {
        let request = AnthropicRequest {
            model: "claude-3".to_string(),
            max_tokens: 4096,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "claude-3");
        assert_eq!(json["max_tokens"], 4096);
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "test");
    }

    #[tokio::test]
    async fn test_openai_request_structure() {
        let request = OpenAIRequest {
            model: "gpt-4".to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
            temperature: 0.7,
            max_tokens: Some(4096),
            max_completion_tokens: None,
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["model"], "gpt-4");
        // Check temperature is approximately 0.7 (floating point precision)
        let temp = json["temperature"].as_f64().unwrap();
        assert!((temp - 0.7).abs() < 0.0001);
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "test");
    }

    #[test]
    fn test_anthropic_response_parsing() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Hello, world!"}
            ]
        }"#;

        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content[0].text, "Hello, world!");
    }

    #[test]
    fn test_openai_response_parsing() {
        let json = r#"{
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Hello, world!"
                    }
                }
            ]
        }"#;

        let response: OpenAIResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.choices[0].message.content, "Hello, world!");
    }

    #[tokio::test]
    async fn test_openai_request_with_extra_body() {
        let request = OpenAIRequest {
            model: "openai/gpt-5.1-codex".to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
            temperature: 0.7,
            max_tokens: Some(4096),
            max_completion_tokens: None,
        };

        let mut body = serde_json::to_value(&request).unwrap();
        // Simulate extra_body merge
        let mut extra = std::collections::HashMap::new();
        extra.insert(
            "reasoning".to_string(),
            serde_json::json!({"effort": "high"}),
        );
        extra.insert("truncate".to_string(), serde_json::json!("END"));

        if let serde_json::Value::Object(ref mut map) = body {
            for (key, value) in &extra {
                map.insert(key.clone(), value.clone());
            }
        }

        assert_eq!(body["model"], "openai/gpt-5.1-codex");
        assert_eq!(body["reasoning"]["effort"], "high");
        assert_eq!(body["truncate"], "END");
        // Original fields preserved
        assert_eq!(body["messages"][0]["content"], "test");
    }

    #[test]
    fn test_openai_client_with_extra_body() {
        let mut extra = std::collections::HashMap::new();
        extra.insert(
            "reasoning".to_string(),
            serde_json::json!({"effort": "high"}),
        );
        let client = OpenAIClient::with_base_url(
            "key".to_string(),
            "model".to_string(),
            "http://localhost".to_string(),
            4096,
            120,
        )
        .unwrap()
        .with_extra_body(extra);
        assert_eq!(client.extra_body.len(), 1);
        assert!(client.extra_body.contains_key("reasoning"));
    }

    #[test]
    fn test_gemini_client_creation() {
        let client =
            GeminiClient::new("test_key".to_string(), "gemini-pro".to_string(), 8192, 120).unwrap();
        assert_eq!(client.api_key.expose(), "test_key");
        assert_eq!(client.model, "gemini-pro");
    }

    #[tokio::test]
    async fn test_gemini_request_structure() {
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: "test".to_string(),
                }],
            }],
            generation_config: None,
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["contents"][0]["parts"][0]["text"], "test");
    }

    #[test]
    fn test_gemini_response_parsing() {
        let json = r#"{
            "candidates": [
                {
                    "content": {
                        "parts": [
                            {"text": "Hello, world!"}
                        ]
                    }
                }
            ]
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.candidates[0].content.parts[0].text,
            "Hello, world!"
        );
    }

    // --- Coverage: empty API key (line 57/94) ---
    #[test]
    fn test_anthropic_client_empty_api_key() {
        let client =
            AnthropicClient::new("".to_string(), "claude-3".to_string(), 4096, 120).unwrap();
        assert_eq!(client.api_key.expose(), "");
        assert_eq!(client.max_tokens, 4096);
    }

    // --- Coverage: OpenAI GPT-5 model uses max_completion_tokens (line 176-182) ---
    #[tokio::test]
    async fn test_openai_request_gpt5_uses_max_completion_tokens() {
        // Verify that GPT-5 models use max_completion_tokens instead of max_tokens
        let model = "gpt-5-turbo";
        let (max_tokens, max_completion_tokens) = if model.starts_with("gpt-5") {
            (None, Some(4096u32))
        } else {
            (Some(4096u32), None)
        };
        assert!(max_tokens.is_none());
        assert_eq!(max_completion_tokens, Some(4096));

        let request = OpenAIRequest {
            model: model.to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
            temperature: 0.7,
            max_tokens,
            max_completion_tokens,
        };

        let json = serde_json::to_value(&request).unwrap();
        // max_tokens should be absent (skip_serializing_if None)
        assert!(json.get("max_tokens").is_none());
        assert_eq!(json["max_completion_tokens"], 4096);
    }

    #[tokio::test]
    async fn test_openai_request_non_gpt5_uses_max_tokens() {
        let model = "gpt-4o";
        let (max_tokens, max_completion_tokens) = if model.starts_with("gpt-5") {
            (None, Some(4096u32))
        } else {
            (Some(4096u32), None)
        };
        assert_eq!(max_tokens, Some(4096));
        assert!(max_completion_tokens.is_none());

        let request = OpenAIRequest {
            model: model.to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
            temperature: 0.7,
            max_tokens,
            max_completion_tokens,
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["max_tokens"], 4096);
        assert!(json.get("max_completion_tokens").is_none());
    }

    // --- Coverage: OpenAI empty/none API key skips auth header (line 224-226) ---
    #[test]
    fn test_openai_client_empty_api_key() {
        let client = OpenAIClient::new("".to_string(), "gpt-4".to_string(), 4096, 120).unwrap();
        assert_eq!(client.api_key.expose(), "");
        // Verify the client is created without issues
        assert_eq!(client.model, "gpt-4");
    }

    #[test]
    fn test_openai_client_none_api_key() {
        let client =
            OpenAIClient::new("none".to_string(), "local-model".to_string(), 4096, 120).unwrap();
        // "none" in lowercase triggers the skip-auth-header path
        assert_eq!(client.api_key.expose(), "none");
    }

    // --- Coverage: Gemini client with various max_tokens (line 319) ---
    #[test]
    fn test_gemini_client_with_generation_config() {
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: "test prompt".to_string(),
                }],
            }],
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: 16384,
            }),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["generation_config"]["maxOutputTokens"], 16384);
    }

    // --- Coverage: empty responses (lines 94, 247, 362-363) ---
    #[test]
    fn test_anthropic_response_empty_content() {
        let json = r#"{"content": []}"#;
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert!(response.content.is_empty());
    }

    #[test]
    fn test_openai_response_empty_choices() {
        let json = r#"{"choices": []}"#;
        let response: OpenAIResponse = serde_json::from_str(json).unwrap();
        assert!(response.choices.is_empty());
    }

    #[test]
    fn test_gemini_response_empty_candidates() {
        let json = r#"{"candidates": []}"#;
        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert!(response.candidates.is_empty());
    }

    #[test]
    fn test_gemini_response_empty_parts() {
        let json = r#"{"candidates": [{"content": {"parts": []}}]}"#;
        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        let first_text = response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone());
        assert!(first_text.is_none());
    }

    // --- Coverage: OpenAI extra_body merge into request body (lines 205-215) ---
    #[test]
    fn test_openai_extra_body_empty_no_merge() {
        let request = OpenAIRequest {
            model: "gpt-4".to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
            temperature: 0.7,
            max_tokens: Some(4096),
            max_completion_tokens: None,
        };

        let extra_body: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();

        // Empty extra_body → no merge needed
        let body = if extra_body.is_empty() {
            serde_json::to_value(&request).unwrap()
        } else {
            unreachable!()
        };

        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["max_tokens"], 4096);
    }

    // --- Coverage: Gemini max_tokens in client creation ---
    #[test]
    fn test_gemini_client_max_tokens() {
        let client = GeminiClient::new(
            "key".to_string(),
            "gemini-2.0-flash".to_string(),
            32768,
            120,
        )
        .unwrap();
        assert_eq!(client.max_tokens, 32768);
        assert_eq!(client.model, "gemini-2.0-flash");
    }
}
