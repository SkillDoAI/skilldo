//! skilldo - Generate SKILL.md files from library source code
//!
//! A 5-agent LLM pipeline that analyzes Python library repositories and produces
//! structured skill documentation for AI coding assistants. Supports multiple
//! LLM providers (OpenAI, Anthropic, Ollama) and container-based code validation.

pub mod agent5;
pub mod changelog;
pub mod cli;
pub mod config;
pub mod detector;
pub mod ecosystems;
pub mod lint;
pub mod llm;
pub mod pipeline;
pub mod util;
pub mod validator;
