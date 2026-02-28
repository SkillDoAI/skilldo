use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;
use tracing::debug;

use crate::agent5::ValidationMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub generation: GenerationConfig,
    #[serde(default)]
    pub prompts: PromptsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>, // For OpenAI-compatible APIs

    /// Optional: Override max_tokens for LLM requests
    /// If not specified, uses provider-specific defaults:
    /// - anthropic: 8192
    /// - openai: 8192
    /// - openai-compatible (ollama): 16384
    /// - gemini: 8192
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Max retries for transient network errors (connection drops, 429s, 5xx).
    /// Default: 10. Set to 0 to disable.
    #[serde(default = "default_network_retries")]
    pub network_retries: usize,

    /// Delay between retries in seconds (constant interval).
    /// Default: 120 (retries every 2 minutes, 10 attempts = 20 min max).
    #[serde(default = "default_retry_delay")]
    pub retry_delay: u64,

    /// Extra fields merged into the LLM request body (TOML table style).
    /// Use for provider-specific parameters not covered by standard fields.
    /// Example: extra_body = { reasoning = { effort = "high" }, truncate = "END" }
    #[serde(default)]
    pub extra_body: std::collections::HashMap<String, serde_json::Value>,

    /// Extra fields as a raw JSON string — alternative to extra_body for complex payloads.
    /// Easier to copy/paste from provider docs. Validated at config load time.
    /// Example: extra_body_json = '{"reasoning": {"effort": "high"}, "truncate": "END"}'
    ///
    /// If both extra_body and extra_body_json are set, they are merged (JSON wins on conflict).
    #[serde(default)]
    pub extra_body_json: Option<String>,
}

fn default_network_retries() -> usize {
    10
}

fn default_retry_delay() -> u64 {
    120
}

impl LlmConfig {
    /// Get max_tokens value, using provider-specific default if not specified
    pub fn get_max_tokens(&self) -> u32 {
        if let Some(tokens) = self.max_tokens {
            return tokens;
        }

        // Provider-specific defaults
        match self.provider.as_str() {
            "anthropic" => 8192,
            "openai" => 8192,
            "openai-compatible" => 16384, // ollama and similar
            "gemini" => 8192,
            _ => 8192, // Safe default
        }
    }

    /// Resolve extra_body by merging TOML table and JSON string sources.
    /// Returns the merged map, or an error if extra_body_json is invalid JSON.
    /// JSON keys win on conflict with TOML keys.
    pub fn resolve_extra_body(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>> {
        let mut merged = self.extra_body.clone();

        if let Some(ref json_str) = self.extra_body_json {
            let parsed: serde_json::Value = serde_json::from_str(json_str)
                .map_err(|e| anyhow::anyhow!("Invalid JSON in extra_body_json: {}", e))?;

            match parsed {
                serde_json::Value::Object(map) => {
                    for (key, value) in map {
                        merged.insert(key, value);
                    }
                }
                _ => {
                    anyhow::bail!(
                        "extra_body_json must be a JSON object ({{}}), got: {}",
                        json_str.chars().take(50).collect::<String>()
                    );
                }
            }
        }

        Ok(merged)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,
    #[serde(default = "default_max_source_tokens")]
    pub max_source_tokens: usize,

    /// Run extract/map/learn agents in parallel (default: true).
    /// Disable for local models (Ollama) to avoid overloading the machine.
    #[serde(default = "default_true")]
    pub parallel_extraction: bool,

    /// Enable test agent code generation validation (default: true)
    /// Alias: enable_agent5 (deprecated, will be removed in v0.2.0)
    #[serde(default = "default_true", alias = "enable_agent5")]
    pub enable_test: bool,

    /// Test agent validation mode: "thorough", "adaptive", or "minimal" (default: "thorough")
    /// Alias: agent5_mode (deprecated, will be removed in v0.2.0)
    #[serde(default = "default_test_mode", alias = "agent5_mode")]
    pub test_mode: String,

    /// Enable review agent accuracy/safety validation (default: true)
    #[serde(default = "default_true")]
    pub enable_review: bool,

    /// Max retries for review → create feedback loop (default: 5)
    #[serde(default = "default_review_max_retries")]
    pub review_max_retries: usize,

    /// Optional: Override LLM for extract agent (API extraction)
    /// Alias: agent1_llm (deprecated, will be removed in v0.2.0)
    #[serde(default, alias = "agent1_llm")]
    pub extract_llm: Option<LlmConfig>,

    /// Optional: Override LLM for map agent (pattern extraction)
    /// Alias: agent2_llm (deprecated, will be removed in v0.2.0)
    #[serde(default, alias = "agent2_llm")]
    pub map_llm: Option<LlmConfig>,

    /// Optional: Override LLM for learn agent (context extraction)
    /// Alias: agent3_llm (deprecated, will be removed in v0.2.0)
    #[serde(default, alias = "agent3_llm")]
    pub learn_llm: Option<LlmConfig>,

    /// Optional: Override LLM for create agent (synthesis)
    /// Alias: agent4_llm (deprecated, will be removed in v0.2.0)
    #[serde(default, alias = "agent4_llm")]
    pub create_llm: Option<LlmConfig>,

    /// Optional: Override LLM for review agent (accuracy/safety validation)
    #[serde(default)]
    pub review_llm: Option<LlmConfig>,

    /// Optional: Override LLM for test agent (code execution validation)
    /// Alias: agent5_llm (deprecated, will be removed in v0.2.0)
    #[serde(default, alias = "agent5_llm")]
    pub test_llm: Option<LlmConfig>,

    /// Container configuration for test agent validation
    #[serde(default)]
    pub container: ContainerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Container runtime: "podman", "docker", etc. (default: auto-detected)
    #[serde(default = "default_runtime")]
    pub runtime: String,

    /// Default container image for Python (default: "python:3.11-slim")
    #[serde(default = "default_python_image")]
    pub python_image: String,

    /// Container image for JavaScript/TypeScript (default: "node:20-slim")
    #[serde(default = "default_node_image")]
    pub javascript_image: String,

    /// Container image for Rust (default: "rust:1.75-slim")
    #[serde(default = "default_rust_image")]
    pub rust_image: String,

    /// Container image for Go (default: "golang:1.21-alpine")
    #[serde(default = "default_go_image")]
    pub go_image: String,

    /// Cleanup containers after execution (default: true)
    #[serde(default = "default_true")]
    pub cleanup: bool,

    /// Timeout for code execution in seconds (default: 60)
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Library install source for Agent 5 validation:
    ///   "registry"       — install from PyPI (default)
    ///   "local-install"  — mount local repo, pip install /src
    ///   "local-mount"    — mount local repo, PYTHONPATH=/src
    #[serde(default = "default_install_source")]
    pub install_source: String,

    /// Path to local source repo for local-install or local-mount modes.
    /// Only used when install_source is not "registry".
    /// If not set, defaults to the repo path passed to `skilldo generate`.
    #[serde(default)]
    pub source_path: Option<String>,

    /// Extra environment variables to pass into the container.
    /// Use for private registries, proxies, or any ecosystem-specific config.
    /// Example: { UV_EXTRA_INDEX_URL = "https://pypi.corp.com/simple/", HTTP_PROXY = "http://proxy:8080" }
    #[serde(default)]
    pub extra_env: std::collections::HashMap<String, String>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            runtime: default_runtime(),
            python_image: default_python_image(),
            javascript_image: default_node_image(),
            rust_image: default_rust_image(),
            go_image: default_go_image(),
            cleanup: true,
            timeout: 60,
            install_source: default_install_source(),
            source_path: None,
            extra_env: std::collections::HashMap::new(),
        }
    }
}

fn default_runtime() -> String {
    detect_container_runtime()
}

/// Detect available container runtime (podman preferred, then docker)
pub fn detect_container_runtime() -> String {
    for runtime in &["podman", "docker"] {
        if std::process::Command::new(runtime)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return runtime.to_string();
        }
    }
    // Fallback — will fail at execution time with a clear error
    "docker".to_string()
}

fn default_python_image() -> String {
    "ghcr.io/astral-sh/uv:python3.11-bookworm-slim".to_string()
}

fn default_node_image() -> String {
    "node:20-slim".to_string()
}

fn default_rust_image() -> String {
    "rust:1.75-slim".to_string()
}

fn default_go_image() -> String {
    "golang:1.21-alpine".to_string()
}

fn default_timeout() -> u64 {
    60
}

fn default_install_source() -> String {
    "registry".to_string()
}

fn default_true() -> bool {
    true
}

fn default_test_mode() -> String {
    "thorough".to_string()
}

fn default_max_retries() -> usize {
    5
}

fn default_max_source_tokens() -> usize {
    100000
}

fn default_review_max_retries() -> usize {
    5
}

impl GenerationConfig {
    /// Parse test_mode string into ValidationMode enum
    pub fn get_test_mode(&self) -> ValidationMode {
        match self.test_mode.to_lowercase().as_str() {
            "minimal" => ValidationMode::Minimal,
            "adaptive" => ValidationMode::Adaptive,
            _ => ValidationMode::Thorough, // Default
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptsConfig {
    /// Global default: if true, custom prompts replace defaults. If false, append.
    /// Per-stage modes (extract_mode, etc.) take precedence over this.
    #[serde(default)]
    pub override_prompts: bool,

    /// Per-stage mode: "append" (default) or "overwrite"
    /// Test stage only supports "append" (overwrite is ignored)
    /// Aliases: agent1_mode..agent4_mode (deprecated, will be removed in v0.2.0)
    #[serde(default, alias = "agent1_mode")]
    pub extract_mode: Option<String>,
    #[serde(default, alias = "agent2_mode")]
    pub map_mode: Option<String>,
    #[serde(default, alias = "agent3_mode")]
    pub learn_mode: Option<String>,
    #[serde(default, alias = "agent4_mode")]
    pub create_mode: Option<String>,

    /// Custom prompt additions or replacements
    /// Aliases: agent1_custom..agent5_custom (deprecated, will be removed in v0.2.0)
    #[serde(default, alias = "agent1_custom")]
    pub extract_custom: Option<String>,
    #[serde(default, alias = "agent2_custom")]
    pub map_custom: Option<String>,
    #[serde(default, alias = "agent3_custom")]
    pub learn_custom: Option<String>,
    #[serde(default, alias = "agent4_custom")]
    pub create_custom: Option<String>,
    #[serde(default)]
    pub review_custom: Option<String>,
    #[serde(default, alias = "agent5_custom")]
    pub test_custom: Option<String>,
}

impl PromptsConfig {
    /// Check if a given stage should overwrite its default prompt.
    /// Per-stage mode takes precedence, then falls back to global override_prompts.
    /// Test and review stages always return false (append-only).
    pub fn is_overwrite(&self, stage: &str) -> bool {
        let mode = match stage {
            "extract" => &self.extract_mode,
            "map" => &self.map_mode,
            "learn" => &self.learn_mode,
            "create" => &self.create_mode,
            _ => return false, // test, review: always append
        };
        match mode.as_deref() {
            Some("overwrite") => true,
            Some(_) => false,              // explicit "append" or anything else
            None => self.override_prompts, // fall back to global
        }
    }
}

impl Config {
    /// Load config from repo root or user config directory
    #[allow(dead_code)]
    pub fn load() -> Result<Self> {
        Self::load_with_path(None)
    }

    /// Load configuration from a specific path, or use default search paths
    pub fn load_with_path(path: Option<String>) -> Result<Self> {
        // If explicit path provided, use it
        if let Some(config_path) = path {
            debug!("Loading config from explicit path: {}", config_path);
            return Self::load_from_path(&config_path);
        }

        // Try repo root first (per-repo config)
        if let Ok(config) = Self::load_from_path("skilldo.toml") {
            debug!("Loaded config from ./skilldo.toml");
            return Ok(config);
        }

        // Try user config directory
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("skilldo").join("config.toml");
            if let Ok(config) = Self::load_from_path(&config_path) {
                debug!("Loaded config from {:?}", config_path);
                return Ok(config);
            }
        }

        // Return defaults
        debug!("Using default config");
        Ok(Self::default())
    }

    fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Get API key from environment variable specified in config.
    /// If `api_key_env` is not set, infers the canonical env var from provider:
    ///   openai → OPENAI_API_KEY, anthropic → ANTHROPIC_API_KEY,
    ///   gemini → GEMINI_API_KEY, openai-compatible → optional (no error if missing).
    pub fn get_api_key(&self) -> Result<String> {
        let env_var = match &self.llm.api_key_env {
            Some(v) => v.clone(),
            None => self.default_api_key_env().to_string(),
        };

        // Special case: "none" means no API key needed (e.g., Ollama)
        if env_var.to_lowercase() == "none" || env_var.is_empty() {
            return Ok(String::new());
        }

        // openai-compatible: try env var but don't error if missing
        // (local models like Ollama don't need keys, but gateways like OpenRouter do)
        if self.llm.provider == "openai-compatible" {
            return Ok(env::var(&env_var).unwrap_or_default());
        }

        env::var(&env_var)
            .map_err(|_| anyhow::anyhow!("API key not found in environment variable: {}", env_var))
    }

    /// Canonical env var name for each provider.
    /// Unknown providers return "none" — this is intentional because custom
    /// openai-compatible setups may use arbitrary provider names and often
    /// don't need API keys (e.g., Ollama). The caller treats "none" as "no key required".
    fn default_api_key_env(&self) -> &str {
        match self.llm.provider.as_str() {
            "openai" => "OPENAI_API_KEY",
            "anthropic" => "ANTHROPIC_API_KEY",
            "gemini" => "GEMINI_API_KEY",
            "openai-compatible" => "OPENAI_API_KEY", // Best guess; won't error if missing
            _ => "none",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig {
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                api_key_env: None, // Inferred from provider in get_api_key()
                base_url: None,
                max_tokens: None, // Use provider default (8192 for anthropic)
                network_retries: default_network_retries(),
                retry_delay: default_retry_delay(),
                extra_body: std::collections::HashMap::new(),
                extra_body_json: None,
            },
            generation: GenerationConfig {
                max_retries: 5,
                max_source_tokens: 100000,
                parallel_extraction: true,
                enable_test: true,
                test_mode: "thorough".to_string(),
                enable_review: true,
                review_max_retries: default_review_max_retries(),
                extract_llm: None,
                map_llm: None,
                learn_llm: None,
                create_llm: None,
                review_llm: None,
                test_llm: None,
                container: ContainerConfig::default(),
            },
            prompts: PromptsConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.llm.provider, "anthropic");
        assert_eq!(config.llm.api_key_env, None); // Inferred from provider
        assert_eq!(config.generation.max_retries, 5);
        assert!(!config.prompts.override_prompts);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("provider = \"anthropic\""));
        // api_key_env is None by default, so it won't appear in serialized TOML
        assert!(!toml_str.contains("AI_API_KEY"));
    }

    #[test]
    fn test_api_key_from_env() {
        env::set_var("TEST_API_KEY", "test_key_123");
        let mut config = Config::default();
        config.llm.api_key_env = Some("TEST_API_KEY".to_string());

        let api_key = config.get_api_key().unwrap();
        assert_eq!(api_key, "test_key_123");

        env::remove_var("TEST_API_KEY");
    }

    #[test]
    fn test_api_key_missing_fails() {
        let mut config = Config::default();
        config.llm.api_key_env = Some("NONEXISTENT_KEY_XYZ".to_string());

        let result = config.get_api_key();
        assert!(result.is_err());
    }

    #[test]
    fn test_is_overwrite_per_stage_mode() {
        let mut prompts = PromptsConfig::default();
        // Default: not overwrite
        assert!(!prompts.is_overwrite("extract"));
        assert!(!prompts.is_overwrite("create"));

        // Test and review always return false
        prompts.override_prompts = true;
        assert!(!prompts.is_overwrite("test"));
        assert!(!prompts.is_overwrite("review"));

        // Per-stage mode takes precedence over global
        prompts.extract_mode = Some("overwrite".to_string());
        assert!(prompts.is_overwrite("extract"));

        prompts.map_mode = Some("append".to_string());
        assert!(!prompts.is_overwrite("map"));

        // Global fallback when no per-stage mode
        assert!(prompts.is_overwrite("learn")); // no learn_mode set, falls back to override_prompts=true
    }

    #[test]
    fn test_max_tokens_provider_defaults() {
        let mut llm = LlmConfig {
            provider: "anthropic".to_string(),
            model: "claude-3".to_string(),
            api_key_env: None,
            base_url: None,
            max_tokens: None,
            network_retries: 10,
            retry_delay: 120,
            extra_body: std::collections::HashMap::new(),
            extra_body_json: None,
        };
        assert_eq!(llm.get_max_tokens(), 8192);

        llm.provider = "openai".to_string();
        assert_eq!(llm.get_max_tokens(), 8192);

        llm.provider = "openai-compatible".to_string();
        assert_eq!(llm.get_max_tokens(), 16384);

        llm.provider = "gemini".to_string();
        assert_eq!(llm.get_max_tokens(), 8192);

        // Explicit override wins
        llm.max_tokens = Some(2000);
        assert_eq!(llm.get_max_tokens(), 2000);
    }

    #[test]
    fn test_test_mode_parsing() {
        let mut gen = GenerationConfig {
            max_retries: 5,
            max_source_tokens: 100000,
            parallel_extraction: true,
            enable_test: true,
            test_mode: "thorough".to_string(),
            enable_review: true,
            review_max_retries: 5,
            extract_llm: None,
            map_llm: None,
            learn_llm: None,
            create_llm: None,
            review_llm: None,
            test_llm: None,
            container: ContainerConfig::default(),
        };
        assert_eq!(gen.get_test_mode(), crate::agent5::ValidationMode::Thorough);

        gen.test_mode = "minimal".to_string();
        assert_eq!(gen.get_test_mode(), crate::agent5::ValidationMode::Minimal);

        gen.test_mode = "adaptive".to_string();
        assert_eq!(gen.get_test_mode(), crate::agent5::ValidationMode::Adaptive);

        // Unknown defaults to thorough
        gen.test_mode = "unknown".to_string();
        assert_eq!(gen.get_test_mode(), crate::agent5::ValidationMode::Thorough);
    }

    #[test]
    fn test_api_key_none_for_ollama() {
        let mut config = Config::default();
        config.llm.api_key_env = Some("none".to_string());
        let key = config.get_api_key().unwrap();
        assert_eq!(key, "");
    }

    #[test]
    fn test_api_key_openai_compatible_missing_ok() {
        let mut config = Config::default();
        config.llm.provider = "openai-compatible".to_string();
        config.llm.api_key_env = Some("SKILLDO_NONEXISTENT_KEY_OAI_999".to_string());
        let key = config.get_api_key().unwrap();
        assert_eq!(key, "");
    }

    #[test]
    fn test_api_key_openai_compatible_uses_key_when_set() {
        env::set_var("SKILLDO_TEST_OAI_COMPAT_KEY", "test_gateway_key");
        let mut config = Config::default();
        config.llm.provider = "openai-compatible".to_string();
        config.llm.api_key_env = Some("SKILLDO_TEST_OAI_COMPAT_KEY".to_string());
        let key = config.get_api_key().unwrap();
        assert_eq!(key, "test_gateway_key");
        env::remove_var("SKILLDO_TEST_OAI_COMPAT_KEY");
    }

    #[test]
    fn test_install_source_defaults() {
        let config = ContainerConfig::default();
        assert_eq!(config.install_source, "registry");
        assert!(config.source_path.is_none());
    }

    #[test]
    fn test_install_source_from_toml() {
        let toml = r#"
[llm]
provider = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"

[generation]
max_retries = 5
max_source_tokens = 100000

[generation.container]
runtime = "podman"
install_source = "local-install"
source_path = "/tmp/my-lib"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.generation.container.install_source, "local-install");
        assert_eq!(
            config.generation.container.source_path,
            Some("/tmp/my-lib".to_string())
        );
    }

    #[test]
    fn test_install_source_local_mount() {
        let toml = r#"
[llm]
provider = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"

[generation]
max_retries = 5
max_source_tokens = 100000

[generation.container]
install_source = "local-mount"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.generation.container.install_source, "local-mount");
        assert!(config.generation.container.source_path.is_none());
    }

    #[test]
    fn test_per_agent_llm_defaults_to_none() {
        let config = Config::default();
        assert!(config.generation.extract_llm.is_none());
        assert!(config.generation.map_llm.is_none());
        assert!(config.generation.learn_llm.is_none());
        assert!(config.generation.create_llm.is_none());
        assert!(config.generation.review_llm.is_none());
        assert!(config.generation.test_llm.is_none());
    }

    #[test]
    fn test_per_agent_llm_from_toml() {
        let toml = r#"
[llm]
provider = "anthropic"
model = "claude-sonnet-4-5"
api_key_env = "ANTHROPIC_API_KEY"

[generation]
max_retries = 5
max_source_tokens = 100000

[generation.agent1_llm]
provider = "openai-compatible"
model = "qwen3-coder:latest"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.agent5_llm]
provider = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.generation.extract_llm.is_some());
        assert!(config.generation.map_llm.is_none());
        assert!(config.generation.learn_llm.is_none());
        assert!(config.generation.create_llm.is_none());
        assert!(config.generation.test_llm.is_some());

        let extract = config.generation.extract_llm.unwrap();
        assert_eq!(extract.provider, "openai-compatible");
        assert_eq!(extract.model, "qwen3-coder:latest");
        assert_eq!(extract.base_url.unwrap(), "http://localhost:11434/v1");
    }

    #[test]
    fn test_all_agents_different_providers() {
        let toml = r#"
[llm]
provider = "anthropic"
model = "claude-sonnet"
api_key_env = "ANTHROPIC_API_KEY"

[generation]
max_retries = 5
max_source_tokens = 100000

[generation.agent1_llm]
provider = "openai-compatible"
model = "qwen3-coder"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.agent2_llm]
provider = "openai-compatible"
model = "qwen3-coder"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.agent3_llm]
provider = "openai-compatible"
model = "qwen3-coder"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[generation.agent4_llm]
provider = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"

[generation.agent5_llm]
provider = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.generation.extract_llm.is_some());
        assert!(config.generation.map_llm.is_some());
        assert!(config.generation.learn_llm.is_some());
        assert!(config.generation.create_llm.is_some());
        assert!(config.generation.test_llm.is_some());
    }

    #[test]
    fn test_extra_body_defaults_to_empty() {
        let config = Config::default();
        assert!(config.llm.extra_body.is_empty());
    }

    #[test]
    fn test_extra_body_from_toml() {
        let toml = r#"
[llm]
provider = "openai-compatible"
model = "openai/gpt-5.1-codex"
api_key_env = "NVIDIA_API_KEY"
base_url = "https://inference-api.nvidia.com/v1/responses"

[llm.extra_body]
truncate = "\"END\""

[llm.extra_body.reasoning]
effort = "high"

[generation]
max_retries = 5
max_source_tokens = 100000
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.llm.extra_body.len(), 2);
        assert!(config.llm.extra_body.contains_key("reasoning"));
        assert!(config.llm.extra_body.contains_key("truncate"));
    }

    #[test]
    fn test_per_agent_extra_body_from_toml() {
        let toml = r#"
[llm]
provider = "anthropic"
model = "claude-sonnet"
api_key_env = "ANTHROPIC_API_KEY"

[generation]
max_retries = 5
max_source_tokens = 100000

[generation.agent4_llm]
provider = "openai-compatible"
model = "openai/gpt-5.1-codex"
api_key_env = "NVIDIA_API_KEY"
base_url = "https://inference-api.nvidia.com/v1/responses"

[generation.agent4_llm.extra_body.reasoning]
effort = "high"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let create = config.generation.create_llm.unwrap();
        assert!(!create.extra_body.is_empty());
        assert!(create.extra_body.contains_key("reasoning"));
    }

    #[test]
    fn test_extra_body_json_parsed() {
        let toml = r#"
[llm]
provider = "openai-compatible"
model = "test"
api_key_env = "none"
extra_body_json = '{"reasoning": {"effort": "high"}, "truncate": "END", "top_p": 0.9}'

[generation]
max_retries = 5
max_source_tokens = 100000
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let resolved = config.llm.resolve_extra_body().unwrap();
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved["reasoning"]["effort"], "high");
        assert_eq!(resolved["truncate"], "END");
        assert_eq!(resolved["top_p"], 0.9);
    }

    #[test]
    fn test_extra_body_json_invalid_errors() {
        let mut config = Config::default();
        config.llm.extra_body_json = Some("not valid json!!!".to_string());
        let result = config.llm.resolve_extra_body();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_extra_body_json_not_object_errors() {
        let mut config = Config::default();
        config.llm.extra_body_json = Some("[1, 2, 3]".to_string());
        let result = config.llm.resolve_extra_body();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be a JSON object"));
    }

    #[test]
    fn test_extra_body_json_wins_on_conflict() {
        let mut config = Config::default();
        // TOML sets truncate = "START"
        config
            .llm
            .extra_body
            .insert("truncate".to_string(), serde_json::json!("START"));
        // JSON sets truncate = "END"
        config.llm.extra_body_json = Some(r#"{"truncate": "END"}"#.to_string());
        let resolved = config.llm.resolve_extra_body().unwrap();
        assert_eq!(resolved["truncate"], "END");
    }

    #[test]
    fn test_extra_body_json_merges_with_toml() {
        let mut config = Config::default();
        config
            .llm
            .extra_body
            .insert("top_p".to_string(), serde_json::json!(0.9));
        config.llm.extra_body_json = Some(r#"{"truncate": "END"}"#.to_string());
        let resolved = config.llm.resolve_extra_body().unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved["top_p"], 0.9);
        assert_eq!(resolved["truncate"], "END");
    }

    #[test]
    fn test_resolve_extra_body_empty() {
        let config = Config::default();
        let resolved = config.llm.resolve_extra_body().unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn test_old_config_names_still_work_via_aliases() {
        // Old agent1_llm..agent5_llm names should deserialize into new field names
        let toml = r#"
[llm]
provider = "openai"
model = "gpt-5.2"
api_key_env = "OPENAI_API_KEY"

[generation]
max_retries = 5
max_source_tokens = 100000
enable_agent5 = false
agent5_mode = "minimal"

[generation.agent1_llm]
provider = "openai-compatible"
model = "qwen3-coder:latest"
api_key_env = "none"
base_url = "http://localhost:11434/v1"

[prompts]
agent1_mode = "overwrite"
agent4_custom = "extra instructions"
agent5_custom = "test instructions"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        // Old enable_agent5 → new enable_test
        assert!(!config.generation.enable_test);
        // Old agent5_mode → new test_mode
        assert_eq!(config.generation.test_mode, "minimal");
        // Old agent1_llm → new extract_llm
        assert!(config.generation.extract_llm.is_some());
        // Old agent1_mode → new extract_mode
        assert_eq!(config.prompts.extract_mode.as_deref(), Some("overwrite"));
        // Old agent4_custom → new create_custom
        assert_eq!(
            config.prompts.create_custom.as_deref(),
            Some("extra instructions")
        );
        // Old agent5_custom → new test_custom
        assert_eq!(
            config.prompts.test_custom.as_deref(),
            Some("test instructions")
        );
    }

    #[test]
    fn test_new_review_config_defaults() {
        let config = Config::default();
        assert!(config.generation.enable_review);
        assert_eq!(config.generation.review_max_retries, 5);
        assert!(config.generation.review_llm.is_none());
        assert!(config.prompts.review_custom.is_none());
    }

    #[test]
    fn test_detect_container_runtime_returns_non_empty() {
        let runtime = detect_container_runtime();
        assert!(!runtime.is_empty(), "runtime should be a non-empty string");
    }

    #[test]
    fn test_detect_container_runtime_returns_known_value() {
        let runtime = detect_container_runtime();
        assert!(
            runtime == "podman" || runtime == "docker",
            "runtime should be 'podman' or 'docker', got '{}'",
            runtime
        );
    }
}
