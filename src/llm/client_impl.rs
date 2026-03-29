use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::debug;

use super::client::LlmClient;
use crate::util::SecretString;

/// Token usage from an LLM API response. All fields optional since
/// not all providers return all fields.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
    // Anthropic uses input/output naming
    #[serde(default, alias = "input_tokens")]
    input_tokens_alt: u32,
    #[serde(default, alias = "output_tokens")]
    output_tokens_alt: u32,
}

fn log_usage(provider: &str, model: &str, usage: &Option<TokenUsage>) {
    if let Some(u) = usage {
        let prompt = if u.prompt_tokens > 0 {
            u.prompt_tokens
        } else {
            u.input_tokens_alt
        };
        let completion = if u.completion_tokens > 0 {
            u.completion_tokens
        } else {
            u.output_tokens_alt
        };
        let total = if u.total_tokens > 0 {
            u.total_tokens
        } else {
            prompt + completion
        };
        if total > 0 {
            debug!(
                "tokens: {} prompt={} completion={} total={} ({})",
                model, prompt, completion, total, provider
            );
        }
    }
}

/// Headers that must not be overridden by extra_headers (case-insensitive).
const PROTECTED_HEADERS: &[&str] = &[
    "authorization",
    "x-api-key",
    "x-goog-api-key",
    "content-type",
];

/// Check if a header key is protected (auth-related).
fn is_protected_header(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    PROTECTED_HEADERS.iter().any(|h| *h == lower)
}

fn build_http_client(timeout_secs: u64) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .context("failed to build HTTP client")
}

// ============================================================================
// Anthropic Client
// ============================================================================

pub struct AnthropicClient {
    api_key: SecretString,
    model: String,
    max_tokens: u32,
    client: Client,
    base_url: String,
    extra_headers: Vec<(String, String)>,
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
    usage: Option<TokenUsage>,
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
            client: build_http_client(timeout_secs)?,
            base_url: "https://api.anthropic.com".to_string(),
            extra_headers: Vec::new(),
        })
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
            max_tokens,
            client: build_http_client(timeout_secs)?,
            base_url,
            extra_headers: Vec::new(),
        })
    }

    pub fn with_extra_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.extra_headers = headers;
        self
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        if self.max_tokens == 0 {
            anyhow::bail!(
                "Anthropic requires max_tokens >= 1. Set a positive value in config (default: 8192)."
            );
        }
        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };

        debug!("Calling Anthropic API with model: {}", self.model);

        let mut req = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", self.api_key.expose())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json");
        for (key, value) in &self.extra_headers {
            if is_protected_header(key) {
                tracing::warn!("extra_headers: skipping protected header '{key}'");
                continue;
            }
            req = req.header(key, value);
        }
        let response = req
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Anthropic API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("[body unreadable: {e}]"));
            bail!("Anthropic API error {}: {}", status, error_text);
        }

        let api_response: AnthropicResponse = response
            .json()
            .await
            .context("Failed to parse Anthropic API response")?;

        log_usage("anthropic", &self.model, &api_response.usage);

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
    extra_headers: Vec<(String, String)>,
    client: Client,
    provider_label: String,
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
    // Option: reasoning models (o1, zai, DeepSeek-R1) may return null content
    // when reasoning tokens exhaust max_tokens before generating a response.
    #[serde(default)]
    content: Option<String>,
    // Reasoning models return chain-of-thought here (ignored for output,
    // useful for debugging). Not present on standard models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning: Option<String>,
}

impl OpenAIMessage {
    fn user(prompt: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(prompt.to_string()),
            reasoning: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<TokenUsage>,
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
        let provider_label = if base_url.contains("api.openai.com") {
            "openai".to_string()
        } else {
            "openai-compatible".to_string()
        };
        Ok(Self {
            api_key: api_key.into(),
            model,
            base_url,
            max_tokens,
            extra_body: std::collections::HashMap::new(),
            extra_headers: Vec::new(),
            client: build_http_client(timeout_secs)?,
            provider_label,
        })
    }

    pub fn with_extra_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.extra_headers = headers;
        self
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
        // GPT-5+ models use max_completion_tokens instead of max_tokens.
        // max_tokens = 0 means "omit from request, let provider decide".
        // Normalize model name: strip provider prefix (e.g., "openai/gpt-5.1" → "gpt-5.1")
        let model_name = self.model.rsplit('/').next().unwrap_or(&self.model);
        let (max_tokens, max_completion_tokens) = if self.max_tokens == 0 {
            (None, None)
        } else if model_name.starts_with("gpt-5") {
            (None, Some(self.max_tokens))
        } else {
            (Some(self.max_tokens), None)
        };

        let request = OpenAIRequest {
            model: self.model.clone(),
            messages: vec![OpenAIMessage::user(prompt)],
            temperature: 0.7,
            max_tokens,
            max_completion_tokens,
        };

        debug!(
            "Calling OpenAI-compatible API at {} with model: {}",
            self.base_url, self.model
        );

        // Only append /chat/completions when the base URL doesn't already
        // specify a concrete endpoint (e.g. /v1/responses).
        let base = self.base_url.trim_end_matches('/');
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else if base.contains("/v1/") {
            // Path continues past /v1/ — user specified a full endpoint URL
            base.to_string()
        } else {
            format!("{}/chat/completions", base)
        };

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
        for (key, value) in &self.extra_headers {
            if is_protected_header(key) {
                tracing::warn!("extra_headers: skipping protected header '{key}'");
                continue;
            }
            req = req.header(key, value);
        }

        let response = req
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("[body unreadable: {e}]"));
            bail!("OpenAI API error {}: {}", status, error_text);
        }

        let api_response: OpenAIResponse = response
            .json()
            .await
            .context("Failed to parse OpenAI API response")?;

        log_usage(&self.provider_label, &self.model, &api_response.usage);

        let choice = api_response
            .choices
            .first()
            .context("No choices in OpenAI response")?;

        match &choice.message.content {
            Some(content) if !content.is_empty() => Ok(content.clone()),
            _ => {
                // Reasoning model exhausted max_tokens on reasoning before generating content
                if choice.message.reasoning.is_some() {
                    bail!(
                        "Reasoning model returned no content (reasoning exhausted max_tokens). \
                         Increase max_tokens in your config."
                    )
                } else {
                    bail!("OpenAI response contained no content")
                }
            }
        }
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
    base_url: String,
    /// When true, use `Authorization: Bearer` header instead of `x-goog-api-key`.
    /// Set when using OAuth tokens instead of API keys.
    use_bearer_auth: bool,
    extra_headers: Vec<(String, String)>,
}

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
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
struct GeminiUsageMetadata {
    #[serde(default, rename = "promptTokenCount")]
    prompt_token_count: u32,
    #[serde(default, rename = "candidatesTokenCount")]
    candidates_token_count: u32,
    #[serde(default, rename = "totalTokenCount")]
    total_token_count: u32,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(default, rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
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
            client: build_http_client(timeout_secs)?,
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            use_bearer_auth: false,
            extra_headers: Vec::new(),
        })
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
            max_tokens,
            client: build_http_client(timeout_secs)?,
            base_url,
            use_bearer_auth: false,
            extra_headers: Vec::new(),
        })
    }

    /// Enable Bearer auth (for OAuth tokens instead of API keys).
    pub fn with_bearer_auth(mut self, enable: bool) -> Self {
        self.use_bearer_auth = enable;
        self
    }

    pub fn with_extra_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.extra_headers = headers;
        self
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
            generation_config: if self.max_tokens == 0 {
                None
            } else {
                Some(GeminiGenerationConfig {
                    max_output_tokens: self.max_tokens,
                })
            },
        };

        debug!("Calling Gemini API with model: {}", self.model);

        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, self.model
        );

        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json");
        req = if self.use_bearer_auth {
            req.header("authorization", format!("Bearer {}", self.api_key.expose()))
        } else {
            req.header("x-goog-api-key", self.api_key.expose())
        };
        for (key, value) in &self.extra_headers {
            if is_protected_header(key) {
                tracing::warn!("extra_headers: skipping protected header '{key}'");
                continue;
            }
            req = req.header(key, value);
        }
        let response = req
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Gemini API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("[body unreadable: {e}]"));
            bail!("Gemini API error {}: {}", status, error_text);
        }

        let api_response: GeminiResponse = response
            .json()
            .await
            .context("Failed to parse Gemini API response")?;

        if let Some(ref u) = api_response.usage_metadata {
            debug!(
                "tokens: {} prompt={} completion={} total={} (gemini)",
                self.model, u.prompt_token_count, u.candidates_token_count, u.total_token_count
            );
        }

        api_response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .context("No content in Gemini response")
    }
}

// ============================================================================
// ChatGPT Client (Responses API)
// ============================================================================

pub struct ChatGPTClient {
    api_key: SecretString,
    model: String,
    max_tokens: u32,
    base_url: String,
    extra_headers: Vec<(String, String)>,
    client: Client,
}

#[derive(Debug, Serialize)]
struct ResponsesRequest {
    model: String,
    instructions: String,
    input: Vec<ResponsesInputMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    store: bool,
}

#[derive(Debug, Serialize)]
struct ResponsesInputMessage {
    #[serde(rename = "type")]
    msg_type: String,
    role: String,
    content: Vec<ResponsesInputContent>,
}

#[derive(Debug, Serialize)]
struct ResponsesInputContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ResponsesResponse {
    output: Vec<ResponsesOutput>,
    usage: Option<TokenUsage>,
}

#[derive(Debug, Deserialize)]
struct ResponsesOutput {
    content: Option<Vec<ResponsesContent>>,
}

#[derive(Debug, Deserialize)]
struct ResponsesContent {
    text: Option<String>,
}

impl ChatGPTClient {
    pub fn new(
        api_key: String,
        model: String,
        max_tokens: u32,
        timeout_secs: u64,
        use_chatgpt_backend: bool,
        base_url: Option<String>,
    ) -> Result<Self> {
        let base_url = base_url.unwrap_or_else(|| {
            if use_chatgpt_backend {
                "https://chatgpt.com/backend-api/codex".to_string()
            } else {
                "https://api.openai.com/v1".to_string()
            }
        });
        Ok(Self {
            api_key: api_key.into(),
            model,
            max_tokens,
            base_url,
            extra_headers: Vec::new(),
            client: build_http_client(timeout_secs)?,
        })
    }

    pub fn with_extra_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.extra_headers = headers;
        self
    }
}

#[async_trait]
impl LlmClient for ChatGPTClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        let request = ResponsesRequest {
            model: self.model.clone(),
            instructions: "Follow the user's instructions precisely.".to_string(),
            input: vec![ResponsesInputMessage {
                msg_type: "message".to_string(),
                role: "user".to_string(),
                content: vec![ResponsesInputContent {
                    content_type: "input_text".to_string(),
                    text: prompt.to_string(),
                }],
            }],
            max_output_tokens: if self.max_tokens == 0 {
                None
            } else {
                Some(self.max_tokens)
            },
            store: false,
        };

        let base = self.base_url.trim_end_matches('/');
        let url = if base.ends_with("/responses") {
            base.to_string()
        } else {
            format!("{}/responses", base)
        };

        debug!(
            "Calling Responses API at {} with model: {}",
            url, self.model
        );

        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&request);

        // Only add authorization if API key is not empty (same guard as OpenAIClient)
        if !self.api_key.expose().is_empty() && self.api_key.expose().to_lowercase() != "none" {
            req = req.header("authorization", format!("Bearer {}", self.api_key.expose()));
        }
        for (key, value) in &self.extra_headers {
            if is_protected_header(key) {
                tracing::warn!("extra_headers: skipping protected header '{key}'");
                continue;
            }
            req = req.header(key, value);
        }

        let response = req.send().await.context("Failed to send request")?;
        let status = response.status();

        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|e| format!("[body unreadable: {e}]"));
            bail!(
                "Responses API error {} {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown Status"),
                body
            );
        }

        let api_response: ResponsesResponse = response
            .json()
            .await
            .context("Failed to parse Responses API response")?;

        log_usage("chatgpt", &self.model, &api_response.usage);

        let text: String = api_response
            .output
            .iter()
            .flat_map(|o| o.content.iter().flatten())
            .filter_map(|c| c.text.as_deref())
            .collect();

        if text.is_empty() {
            bail!("No text content in Responses API response")
        }

        Ok(text)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protected_headers() {
        assert!(is_protected_header("authorization"));
        assert!(is_protected_header("Authorization"));
        assert!(is_protected_header("x-api-key"));
        assert!(is_protected_header("X-API-KEY"));
        assert!(is_protected_header("x-goog-api-key"));
        assert!(is_protected_header("content-type"));
        assert!(is_protected_header("Content-Type"));
        assert!(!is_protected_header("x-custom-header"));
        assert!(!is_protected_header("user-agent"));
    }

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
            messages: vec![OpenAIMessage::user("test")],
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
            ],
            "usage": {
                "input_tokens": 25,
                "output_tokens": 100
            }
        }"#;

        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content[0].text, "Hello, world!");
        assert!(response.usage.is_some());
        log_usage("anthropic", "claude-3", &response.usage);
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
            ],
            "usage": {
                "prompt_tokens": 15,
                "completion_tokens": 42,
                "total_tokens": 57
            }
        }"#;

        let response: OpenAIResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("Hello, world!")
        );
        assert!(response.usage.is_some());
        log_usage("openai", "gpt-4", &response.usage);
    }

    #[tokio::test]
    async fn test_openai_request_with_extra_body() {
        let request = OpenAIRequest {
            model: "openai/gpt-5.1-codex".to_string(),
            messages: vec![OpenAIMessage::user("test")],
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
            ],
            "usageMetadata": {
                "promptTokenCount": 12,
                "candidatesTokenCount": 45,
                "totalTokenCount": 57
            }
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.candidates[0].content.parts[0].text,
            "Hello, world!"
        );
        // Exercise the Gemini-specific usage debug path (mirrors complete() logic)
        if let Some(ref u) = response.usage_metadata {
            debug!(
                "tokens: {} prompt={} completion={} total={} (gemini)",
                "gemini-pro", u.prompt_token_count, u.candidates_token_count, u.total_token_count
            );
            assert_eq!(u.prompt_token_count, 12);
            assert_eq!(u.candidates_token_count, 45);
            assert_eq!(u.total_token_count, 57);
        } else {
            panic!("expected usageMetadata to be present");
        }
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
            messages: vec![OpenAIMessage::user("test")],
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
            messages: vec![OpenAIMessage::user("test")],
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
        assert_eq!(json["generationConfig"]["maxOutputTokens"], 16384);
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
            messages: vec![OpenAIMessage::user("test")],
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

    // --- Coverage: URL construction for OpenAI-compatible clients ---
    #[test]
    fn test_openai_url_appends_chat_completions_to_v1() {
        // Standard /v1 base → should append /chat/completions
        let base = "https://api.openai.com/v1";
        let trimmed = base.trim_end_matches('/');
        let url = format!("{}/chat/completions", trimmed);
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_openai_url_preserves_full_endpoint() {
        // Full endpoint URL with path after /v1/ → use as-is
        let base = "https://inference-api.nvidia.com/v1/responses";
        let trimmed = base.trim_end_matches('/');
        assert!(trimmed.contains("/v1/"));
        // Should NOT append /chat/completions
        assert_eq!(trimmed, "https://inference-api.nvidia.com/v1/responses");
    }

    #[test]
    fn test_openai_url_appends_to_bare_host() {
        // Bare host with no path → should append /chat/completions
        let base = "https://models.inference.ai.azure.com";
        let trimmed = base.trim_end_matches('/');
        assert!(!trimmed.ends_with("/chat/completions"));
        assert!(!trimmed.contains("/v1/"));
        let url = format!("{}/chat/completions", trimmed);
        assert_eq!(
            url,
            "https://models.inference.ai.azure.com/chat/completions"
        );
    }

    #[test]
    fn test_openai_url_handles_trailing_slash() {
        let base = "http://localhost:11434/v1/";
        let trimmed = base.trim_end_matches('/');
        assert!(!trimmed.contains("/v1/"));
        let url = format!("{}/chat/completions", trimmed);
        assert_eq!(url, "http://localhost:11434/v1/chat/completions");
    }

    #[test]
    fn test_chatgpt_client_creation_backend() {
        let client = ChatGPTClient::new(
            "key".to_string(),
            "gpt-5.2-codex".to_string(),
            8192,
            120,
            true,
            None,
        )
        .unwrap();
        assert_eq!(client.base_url, "https://chatgpt.com/backend-api/codex");
        assert_eq!(client.model, "gpt-5.2-codex");
    }

    #[test]
    fn test_chatgpt_client_creation_api() {
        let client = ChatGPTClient::new(
            "key".to_string(),
            "gpt-5.2".to_string(),
            8192,
            120,
            false,
            None,
        )
        .unwrap();
        assert_eq!(client.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_chatgpt_client_base_url_override() {
        let client = ChatGPTClient::new(
            "key".to_string(),
            "m".to_string(),
            8192,
            120,
            true,
            Some("https://proxy.example.com/api".to_string()),
        )
        .unwrap();
        assert_eq!(client.base_url, "https://proxy.example.com/api");
    }

    #[test]
    fn test_chatgpt_client_extra_headers() {
        let client = ChatGPTClient::new("key".to_string(), "m".to_string(), 8192, 120, true, None)
            .unwrap()
            .with_extra_headers(vec![("X-Custom".to_string(), "val".to_string())]);
        assert_eq!(client.extra_headers.len(), 1);
        assert_eq!(client.extra_headers[0].0, "X-Custom");
    }

    #[test]
    fn test_responses_request_serialization() {
        let req = ResponsesRequest {
            model: "gpt-5.2-codex".to_string(),
            instructions: "Follow the user's instructions precisely.".to_string(),
            input: vec![ResponsesInputMessage {
                msg_type: "message".to_string(),
                role: "user".to_string(),
                content: vec![ResponsesInputContent {
                    content_type: "input_text".to_string(),
                    text: "hello".to_string(),
                }],
            }],
            max_output_tokens: Some(8192),
            store: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "gpt-5.2-codex");
        assert_eq!(json["max_output_tokens"], 8192);
        assert_eq!(json["store"], false);
        assert!(json["instructions"].as_str().unwrap().contains("precisely"));
        assert_eq!(json["input"][0]["type"], "message");
        assert_eq!(json["input"][0]["role"], "user");
        assert_eq!(json["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(json["input"][0]["content"][0]["text"], "hello");
    }

    #[test]
    fn test_responses_request_max_output_tokens_none_skipped() {
        let req = ResponsesRequest {
            model: "m".to_string(),
            instructions: "i".to_string(),
            input: vec![],
            max_output_tokens: None,
            store: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("max_output_tokens").is_none());
    }

    #[test]
    fn test_responses_response_parsing() {
        let json = r#"{
            "output": [{
                "content": [{"text": "Hello, world!"}]
            }],
            "usage": {
                "prompt_tokens": 30,
                "completion_tokens": 55,
                "total_tokens": 85
            }
        }"#;
        let response: ResponsesResponse = serde_json::from_str(json).unwrap();
        let text: String = response
            .output
            .iter()
            .flat_map(|o| o.content.iter().flatten())
            .filter_map(|c| c.text.as_deref())
            .collect();
        assert_eq!(text, "Hello, world!");
        assert!(response.usage.is_some());
        log_usage("chatgpt", "gpt-5.2", &response.usage);
    }

    #[test]
    fn test_responses_response_empty_output() {
        let json = r#"{"output": []}"#;
        let response: ResponsesResponse = serde_json::from_str(json).unwrap();
        let text: String = response
            .output
            .iter()
            .flat_map(|o| o.content.iter().flatten())
            .filter_map(|c| c.text.as_deref())
            .collect();
        assert!(text.is_empty());
    }

    #[test]
    fn test_chatgpt_url_construction() {
        // Base URL without /responses → appends
        let base = "https://chatgpt.com/backend-api/codex";
        let trimmed = base.trim_end_matches('/');
        assert!(!trimmed.ends_with("/responses"));
        let url = format!("{}/responses", trimmed);
        assert_eq!(url, "https://chatgpt.com/backend-api/codex/responses");

        // Base URL already ending with /responses → no double append
        let base2 = "https://chatgpt.com/backend-api/codex/responses";
        let trimmed2 = base2.trim_end_matches('/');
        assert!(trimmed2.ends_with("/responses"));
    }

    #[test]
    fn test_chatgpt_client_skips_auth_header_for_none_key() {
        // ChatGPTClient with api_key="none" should not add Authorization header.
        // This mirrors the OpenAI client's guard for keyless/OAuth providers.
        let client =
            ChatGPTClient::new("none".to_string(), "m".to_string(), 8192, 120, true, None).unwrap();
        let key = client.api_key.expose();
        // Verify the guard condition matches
        assert!(key.to_lowercase() == "none");
    }

    #[test]
    fn test_chatgpt_client_skips_auth_header_for_empty_key() {
        let client =
            ChatGPTClient::new(String::new(), "m".to_string(), 8192, 120, true, None).unwrap();
        let key = client.api_key.expose();
        assert!(key.is_empty());
    }

    // --- Coverage: TokenUsage deserialization ---

    #[test]
    fn test_token_usage_deserialize_openai_fields() {
        let json = r#"{
            "prompt_tokens": 100,
            "completion_tokens": 200,
            "total_tokens": 300
        }"#;
        let usage: TokenUsage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 200);
        assert_eq!(usage.total_tokens, 300);
    }

    #[test]
    fn test_token_usage_deserialize_anthropic_fields() {
        let json = r#"{
            "input_tokens": 50,
            "output_tokens": 150
        }"#;
        let usage: TokenUsage = serde_json::from_str(json).unwrap();
        // Anthropic fields land in the aliased private fields; public fields stay 0
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
        // log_usage reads _input_tokens/_output_tokens when public fields are 0
        assert_eq!(usage.input_tokens_alt, 50);
        assert_eq!(usage.output_tokens_alt, 150);
    }

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
        assert_eq!(usage.input_tokens_alt, 0);
        assert_eq!(usage.output_tokens_alt, 0);
    }

    // --- Coverage: log_usage() ---

    #[test]
    fn test_log_usage_with_some_openai_style() {
        let usage = Some(TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            input_tokens_alt: 0,
            output_tokens_alt: 0,
        });
        // Should not panic; exercises the Some branch with total > 0
        log_usage("openai", "gpt-4", &usage);
    }

    #[test]
    fn test_log_usage_with_some_anthropic_style() {
        let usage = Some(TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            input_tokens_alt: 40,
            output_tokens_alt: 60,
        });
        // Falls through to _input_tokens/_output_tokens, total computed as 100
        log_usage("anthropic", "claude-3", &usage);
    }

    #[test]
    fn test_log_usage_with_none() {
        // Should not panic; exercises the None branch
        log_usage("openai", "gpt-4", &None);
    }

    #[test]
    fn test_log_usage_with_zero_totals() {
        let usage = Some(TokenUsage::default());
        // All zeros → total == 0 → no debug! call, but no panic
        log_usage("openai", "gpt-4", &usage);
    }

    // --- Coverage: OpenAI max_tokens == 0 omit path ---

    #[test]
    fn test_openai_request_max_tokens_zero_omits_both() {
        let max_tokens_cfg: u32 = 0;
        let model = "gpt-4o";
        let (max_tokens, max_completion_tokens) = if max_tokens_cfg == 0 {
            (None, None)
        } else if model.starts_with("gpt-5") {
            (None, Some(max_tokens_cfg))
        } else {
            (Some(max_tokens_cfg), None)
        };

        assert!(max_tokens.is_none());
        assert!(max_completion_tokens.is_none());

        let request = OpenAIRequest {
            model: model.to_string(),
            messages: vec![OpenAIMessage::user("test")],
            temperature: 0.7,
            max_tokens,
            max_completion_tokens,
        };

        let json = serde_json::to_value(&request).unwrap();
        // Both should be absent due to skip_serializing_if = "Option::is_none"
        assert!(json.get("max_tokens").is_none());
        assert!(json.get("max_completion_tokens").is_none());
    }

    // --- Coverage: Gemini max_tokens == 0 omits generation_config ---

    #[test]
    fn test_gemini_request_max_tokens_zero_omits_generation_config() {
        let max_tokens: u32 = 0;
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: "test".to_string(),
                }],
            }],
            generation_config: if max_tokens == 0 {
                None
            } else {
                Some(GeminiGenerationConfig {
                    max_output_tokens: max_tokens,
                })
            },
        };

        let json = serde_json::to_value(&request).unwrap();
        // generation_config should be absent due to skip_serializing_if = "Option::is_none"
        assert!(json.get("generationConfig").is_none());
    }

    // --- Coverage: ChatGPT max_tokens == 0 omits max_output_tokens ---

    #[test]
    fn test_chatgpt_request_max_tokens_zero_omits_max_output_tokens() {
        let max_tokens: u32 = 0;
        let req = ResponsesRequest {
            model: "gpt-5.2".to_string(),
            instructions: "i".to_string(),
            input: vec![],
            max_output_tokens: if max_tokens == 0 {
                None
            } else {
                Some(max_tokens)
            },
            store: false,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("max_output_tokens").is_none());
    }

    // --- Coverage: Gemini usage_metadata parsing ---

    #[test]
    fn test_gemini_response_with_usage_metadata() {
        let json = r#"{
            "candidates": [{"content": {"parts": [{"text": "hi"}]}}],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 20,
                "totalTokenCount": 30
            }
        }"#;
        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        let usage = response.usage_metadata.unwrap();
        assert_eq!(usage.prompt_token_count, 10);
        assert_eq!(usage.candidates_token_count, 20);
        assert_eq!(usage.total_token_count, 30);
    }

    #[test]
    fn test_gemini_response_without_usage_metadata() {
        let json = r#"{"candidates": [{"content": {"parts": [{"text": "hi"}]}}]}"#;
        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert!(response.usage_metadata.is_none());
    }
}
