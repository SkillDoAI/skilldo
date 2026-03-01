// Test agent: Code Generation Validator
//
// Meta-validation that tests if the generated SKILL.md is actually useful
// for AI agents writing real code.

pub mod code_generator;
pub mod container_executor;
pub mod executor;
pub mod parser;
pub mod validator;

pub use executor::{ExecutionEnv, ExecutionResult};
#[allow(unused_imports)]
pub use parser::{CodePattern, PatternCategory};
pub use validator::{TestCodeValidator, TestResult, ValidationMode};

use anyhow::Result;

/// Core trait for extracting patterns and dependencies from SKILL.md
/// Language-agnostic interface that each language implements.
pub trait LanguageParser: Send + Sync {
    /// Extract code patterns from the "Core Patterns" section
    fn extract_patterns(&self, skill_md: &str) -> Result<Vec<CodePattern>>;

    /// Extract package dependencies from imports section
    fn extract_dependencies(&self, skill_md: &str) -> Result<Vec<String>>;

    /// Extract version from frontmatter (e.g., "version: 3.0.0")
    /// Returns None if no version found or "unknown"
    fn extract_version(&self, skill_md: &str) -> Result<Option<String>>;

    /// Extract package name from frontmatter (e.g., "name: scikit-learn")
    /// Used for `pip install <name>` / `npm install <name>` instead of import names
    fn extract_name(&self, skill_md: &str) -> Result<Option<String>>;
}

/// Core trait for generating test code from patterns
/// Uses LLM to create minimal, runnable test scripts.
#[async_trait::async_trait]
pub trait LanguageCodeGenerator: Send + Sync {
    /// Generate a complete, runnable test script for a pattern
    async fn generate_test_code(&self, pattern: &CodePattern) -> Result<String>;

    /// Set the local package name to exclude from dependency lists.
    /// Used for local-install/local-mount modes where the package is mounted, not from PyPI.
    /// Default implementation is a no-op for generators that don't support this.
    fn set_local_package(&self, _package: Option<String>) {}
}

/// Core trait for executing code in isolated environments
/// Handles environment setup, code execution, and cleanup.
pub trait LanguageExecutor: Send + Sync {
    /// Setup isolated environment with dependencies
    fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv>;

    /// Run code in the environment
    fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult>;

    /// Cleanup environment
    fn cleanup(&self, env: &ExecutionEnv) -> Result<()>;
}
