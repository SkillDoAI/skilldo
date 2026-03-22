//! Test agent — meta-validates generated SKILL.md by asking an LLM to write
//! real code from it, then executing that code in a container to verify it works.
//! Language-specific logic lives in parser/codegen submodules (e.g. `python_parser`).

/// Generate three mutex-poison-recovery tests for a code generator type.
/// Covers: set_local_package after poison, generate_test_code after poison,
/// and retry_test_code after poison.
#[allow(unused_macros)]
macro_rules! poison_recovery_tests {
    ($GenType:ident, $sample_pattern:ident) => {
        #[test]
        fn test_mutex_poison_recovery() {
            use crate::llm::client::MockLlmClient;
            let client = MockLlmClient::new();
            let gen = $GenType::new(&client);
            let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = gen.local_package.lock().unwrap();
                panic!("intentional poison");
            }));
            assert!(poisoned.is_err(), "poison setup should panic");
            gen.set_local_package(Some("test-pkg".to_string()));
            let val = gen.local_package.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(val.as_deref(), Some("test-pkg"));
        }

        #[tokio::test]
        async fn test_generate_after_poison_recovers() {
            use crate::llm::client::MockLlmClient;
            let client = MockLlmClient::new();
            let gen = $GenType::new(&client);
            let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = gen.local_package.lock().unwrap();
                panic!("poison");
            }));
            assert!(poisoned.is_err(), "poison setup should panic");
            let pattern = $sample_pattern();
            let result = gen.generate_test_code(&pattern).await;
            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn test_retry_after_poison_recovers() {
            use crate::llm::client::MockLlmClient;
            let client = MockLlmClient::new();
            let gen = $GenType::new(&client);
            let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = gen.local_package.lock().unwrap();
                panic!("poison");
            }));
            assert!(poisoned.is_err(), "poison setup should panic");
            let pattern = $sample_pattern();
            let result = gen.retry_test_code(&pattern, "old code", "error msg").await;
            assert!(result.is_ok());
        }
    };
}

pub mod code_generator;
pub mod container_executor;
pub mod executor;
pub mod go_code_gen;
pub mod go_parser;
pub mod java_code_gen;
pub mod java_parser;
pub mod js_code_gen;
pub mod js_parser;
pub mod parser;
pub mod python_code_gen;
pub mod python_parser;
pub mod rust_code_gen;
pub mod rust_parser;
pub mod validator;

pub use executor::{ExecutionEnv, ExecutionResult};
#[allow(unused_imports)]
pub use parser::{CodePattern, PatternCategory};
pub use validator::{TestCodeValidator, TestResult, ValidationMode};

use anyhow::Result;
use std::sync::{Mutex, MutexGuard};
use tracing::debug;

/// Lock a mutex, recovering from poison instead of panicking.
/// Used by all code generators for their `local_package` field.
pub(crate) fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

use parser::frontmatter_field;

/// Core trait for extracting patterns and dependencies from SKILL.md
/// Language-agnostic interface that each language implements.
pub trait LanguageParser: Send + Sync {
    /// Extract code patterns from the "Core Patterns" section
    fn extract_patterns(&self, skill_md: &str) -> Result<Vec<CodePattern>>;

    /// Extract package dependencies from imports section
    fn extract_dependencies(&self, skill_md: &str) -> Result<Vec<String>>;

    /// Extract version from frontmatter (e.g., "version: 3.0.0")
    /// Returns None if no version found or "unknown"
    fn extract_version(&self, skill_md: &str) -> Result<Option<String>> {
        match frontmatter_field(skill_md, "version") {
            Some(v) if v == "unknown" => {
                debug!("Version field found but set to 'unknown'");
                Ok(None)
            }
            Some(v) => {
                debug!("Extracted version from SKILL.md: {}", v);
                Ok(Some(v))
            }
            None => {
                debug!("No version field found in SKILL.md frontmatter");
                Ok(None)
            }
        }
    }

    /// Extract package name from frontmatter (e.g., "name: scikit-learn")
    /// Used for `pip install <name>` / `npm install <name>` instead of import names
    fn extract_name(&self, skill_md: &str) -> Result<Option<String>> {
        match frontmatter_field(skill_md, "name") {
            Some(name) => {
                debug!("Extracted package name from SKILL.md: {}", name);
                Ok(Some(name))
            }
            None => Ok(None),
        }
    }
}

/// Core trait for generating test code from patterns
/// Uses LLM to create minimal, runnable test scripts.
#[async_trait::async_trait]
pub trait LanguageCodeGenerator: Send + Sync {
    /// Generate a complete, runnable test script for a pattern
    async fn generate_test_code(&self, pattern: &CodePattern) -> Result<String>;

    /// Retry test code generation with the previous code and error output.
    /// The LLM sees what it wrote, what went wrong, and produces a fixed version.
    /// Default: falls back to `generate_test_code` (ignores error context).
    async fn retry_test_code(
        &self,
        pattern: &CodePattern,
        previous_code: &str,
        error_output: &str,
    ) -> Result<String> {
        let _ = (previous_code, error_output);
        self.generate_test_code(pattern).await
    }

    /// Set the local package name to exclude from dependency lists.
    /// Used for local-install/local-mount modes where the package is mounted, not from PyPI.
    /// Default implementation is a no-op for generators that don't support this.
    fn set_local_package(&self, _package: Option<String>) {}
}

/// Core trait for executing code in isolated environments
/// Handles environment setup, code execution, and cleanup.
#[async_trait::async_trait]
pub trait LanguageExecutor: Send + Sync {
    /// Setup isolated environment with dependencies
    async fn setup_environment(&self, deps: &[String]) -> Result<ExecutionEnv>;

    /// Run code in the environment
    async fn run_code(&self, env: &ExecutionEnv, code: &str) -> Result<ExecutionResult>;

    /// Cleanup environment
    async fn cleanup(&self, env: &ExecutionEnv) -> Result<()>;
}
