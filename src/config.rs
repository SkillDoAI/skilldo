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
    /// - anthropic: 4096
    /// - openai: 4096
    /// - openai-compatible (ollama): 16384
    /// - gemini: 8192
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

impl LlmConfig {
    /// Get max_tokens value, using provider-specific default if not specified
    pub fn get_max_tokens(&self) -> u32 {
        if let Some(tokens) = self.max_tokens {
            return tokens;
        }

        // Provider-specific defaults
        match self.provider.as_str() {
            "anthropic" => 4096,
            "openai" => 4096,
            "openai-compatible" => 16384, // ollama and similar
            "gemini" => 8192,
            _ => 4096, // Safe default
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub max_retries: usize,
    pub max_source_tokens: usize,

    /// Enable Agent 5 code generation validation (default: true)
    #[serde(default = "default_true")]
    pub enable_agent5: bool,

    /// Agent 5 validation mode: "thorough", "adaptive", or "minimal" (default: "thorough")
    #[serde(default = "default_agent5_mode")]
    pub agent5_mode: String,

    /// Optional: Override LLM model for Agent 5 only (default: use main llm.model)
    #[serde(default)]
    pub agent5_llm: Option<LlmConfig>,

    /// Container configuration for Agent 5 validation
    #[serde(default)]
    pub container: ContainerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Container runtime: "docker", "podman", etc. (default: "docker")
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
        }
    }
}

fn default_runtime() -> String {
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

fn default_true() -> bool {
    true
}

fn default_agent5_mode() -> String {
    "thorough".to_string()
}

impl GenerationConfig {
    /// Parse agent5_mode string into ValidationMode enum
    pub fn get_agent5_mode(&self) -> ValidationMode {
        match self.agent5_mode.to_lowercase().as_str() {
            "minimal" => ValidationMode::Minimal,
            "adaptive" => ValidationMode::Adaptive,
            _ => ValidationMode::Thorough, // Default
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptsConfig {
    /// Global default: if true, custom prompts replace defaults. If false, append.
    /// Per-agent modes (agentN_mode) take precedence over this.
    #[serde(default)]
    pub override_prompts: bool,

    /// Per-agent mode: "append" (default) or "overwrite"
    /// Agent 5 only supports "append" (overwrite is ignored)
    #[serde(default)]
    pub agent1_mode: Option<String>,
    #[serde(default)]
    pub agent2_mode: Option<String>,
    #[serde(default)]
    pub agent3_mode: Option<String>,
    #[serde(default)]
    pub agent4_mode: Option<String>,

    /// Custom prompt additions or replacements
    #[serde(default)]
    pub agent1_custom: Option<String>,
    #[serde(default)]
    pub agent2_custom: Option<String>,
    #[serde(default)]
    pub agent3_custom: Option<String>,
    #[serde(default)]
    pub agent4_custom: Option<String>,
    #[serde(default)]
    pub agent5_custom: Option<String>,
}

impl PromptsConfig {
    /// Check if a given agent should overwrite its default prompt.
    /// Per-agent mode takes precedence, then falls back to global override_prompts.
    /// Agent 5 always returns false (append-only).
    pub fn is_overwrite(&self, agent: u8) -> bool {
        let mode = match agent {
            1 => &self.agent1_mode,
            2 => &self.agent2_mode,
            3 => &self.agent3_mode,
            4 => &self.agent4_mode,
            _ => return false, // Agent 5: always append
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

    /// Get API key from environment variable specified in config
    pub fn get_api_key(&self) -> Result<String> {
        match &self.llm.api_key_env {
            Some(env_var) => {
                // Special case: "none" means no API key needed (e.g., Ollama)
                if env_var.to_lowercase() == "none" {
                    return Ok(String::new());
                }

                // openai-compatible: try env var but don't error if missing
                // (local models like Ollama don't need keys, but gateways like OpenRouter do)
                if self.llm.provider == "openai-compatible" {
                    return Ok(env::var(env_var).unwrap_or_default());
                }

                env::var(env_var).map_err(|_| {
                    anyhow::anyhow!("API key not found in environment variable: {}", env_var)
                })
            }
            None => Ok(String::new()), // No API key needed
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig {
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                api_key_env: Some("AI_API_KEY".to_string()),
                base_url: None,
                max_tokens: None, // Use provider default (4096 for anthropic)
            },
            generation: GenerationConfig {
                max_retries: 5,
                max_source_tokens: 100000,
                enable_agent5: true,
                agent5_mode: "thorough".to_string(),
                agent5_llm: None,
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
        assert_eq!(config.llm.api_key_env, Some("AI_API_KEY".to_string()));
        assert_eq!(config.generation.max_retries, 5);
        assert!(!config.prompts.override_prompts);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("provider = \"anthropic\""));
        assert!(toml_str.contains("AI_API_KEY"));
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
    fn test_is_overwrite_per_agent_mode() {
        let mut prompts = PromptsConfig::default();
        // Default: not overwrite
        assert!(!prompts.is_overwrite(1));
        assert!(!prompts.is_overwrite(4));

        // Agent 5 always returns false
        prompts.override_prompts = true;
        assert!(!prompts.is_overwrite(5));

        // Per-agent mode takes precedence over global
        prompts.agent1_mode = Some("overwrite".to_string());
        assert!(prompts.is_overwrite(1));

        prompts.agent2_mode = Some("append".to_string());
        assert!(!prompts.is_overwrite(2));

        // Global fallback when no per-agent mode
        assert!(prompts.is_overwrite(3)); // no agent3_mode set, falls back to override_prompts=true
    }

    #[test]
    fn test_max_tokens_provider_defaults() {
        let mut llm = LlmConfig {
            provider: "anthropic".to_string(),
            model: "claude-3".to_string(),
            api_key_env: None,
            base_url: None,
            max_tokens: None,
        };
        assert_eq!(llm.get_max_tokens(), 4096);

        llm.provider = "openai".to_string();
        assert_eq!(llm.get_max_tokens(), 4096);

        llm.provider = "openai-compatible".to_string();
        assert_eq!(llm.get_max_tokens(), 16384);

        llm.provider = "gemini".to_string();
        assert_eq!(llm.get_max_tokens(), 8192);

        // Explicit override wins
        llm.max_tokens = Some(2000);
        assert_eq!(llm.get_max_tokens(), 2000);
    }

    #[test]
    fn test_agent5_mode_parsing() {
        let mut gen = GenerationConfig {
            max_retries: 5,
            max_source_tokens: 100000,
            enable_agent5: true,
            agent5_mode: "thorough".to_string(),
            agent5_llm: None,
            container: ContainerConfig::default(),
        };
        assert_eq!(
            gen.get_agent5_mode(),
            crate::agent5::ValidationMode::Thorough
        );

        gen.agent5_mode = "minimal".to_string();
        assert_eq!(
            gen.get_agent5_mode(),
            crate::agent5::ValidationMode::Minimal
        );

        gen.agent5_mode = "adaptive".to_string();
        assert_eq!(
            gen.get_agent5_mode(),
            crate::agent5::ValidationMode::Adaptive
        );

        // Unknown defaults to thorough
        gen.agent5_mode = "unknown".to_string();
        assert_eq!(
            gen.get_agent5_mode(),
            crate::agent5::ValidationMode::Thorough
        );
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
}
