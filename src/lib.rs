//! skilldo - Generate SKILL.md files from library source code
//!
//! A 6-stage LLM pipeline (extract → map → learn → create → review → test) that
//! analyzes library repositories and produces structured skill documentation for
//! AI coding assistants. Supports multiple LLM providers (OpenAI, Anthropic, Ollama)
//! and container-based code validation.

pub mod agent5;
pub mod changelog;
pub mod cli;
pub mod config;
pub mod detector;
pub mod ecosystems;
pub mod lint;
pub mod llm;
pub mod pipeline;
pub mod review;
pub mod util;
pub mod validator;
