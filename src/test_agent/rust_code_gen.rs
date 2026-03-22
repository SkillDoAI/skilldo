//! Rust test code generator — uses the shared prompt builder with minimal
//! Rust-specific environment notes. The prompt intentionally avoids coaching
//! the LLM on how to write Rust — the SKILL.md should be sufficient.

use anyhow::Result;
use std::sync::Mutex;
use tracing::debug;

use super::code_generator::{build_retry_prompt, build_test_prompt, TestEnv};
use super::{CodePattern, LanguageCodeGenerator};
use crate::llm::client::LlmClient;

/// Rust environment: runner command + 1–2 line notes the LLM can't infer.
pub const RUST_ENV: TestEnv = TestEnv {
    lang_tag: "rust",
    runner: "`cargo run`",
    env_notes: "\
- Write a standalone `fn main()` program — no test harness available
- External crates from the Imports section are pre-installed; just `use` them directly
- Use `eprintln!` and `std::process::exit(1)` for assertion failures",
};

/// Rust code generator using LLM
pub struct RustCodeGenerator<'a> {
    llm_client: &'a dyn LlmClient,
    custom_instructions: Option<String>,
    /// Package name to exclude from deps (for local-mount modes).
    local_package: Mutex<Option<String>>,
}

impl<'a> RustCodeGenerator<'a> {
    pub fn new(llm_client: &'a dyn LlmClient) -> Self {
        Self {
            llm_client,
            custom_instructions: None,
            local_package: Mutex::new(None),
        }
    }

    pub fn with_custom_instructions(mut self, instructions: Option<String>) -> Self {
        self.custom_instructions = instructions;
        self
    }

    /// Generate the prompt for creating test code from a pattern.
    /// Public for tests — delegates to the shared builder.
    pub fn create_test_prompt(
        pattern: &CodePattern,
        custom_instructions: Option<&str>,
        local_package: Option<&str>,
    ) -> String {
        build_test_prompt(pattern, &RUST_ENV, local_package, custom_instructions)
    }

    /// Extract Rust code from markdown code blocks (supports both ``` and ~~~ fences).
    fn extract_code_from_response(response: &str) -> Result<String> {
        let trimmed = response.trim();
        let blocks = crate::util::find_fenced_blocks(trimmed);

        const RUST_TAGS: &[&str] = &["rust", "rs"];

        // Pass 1: prefer Rust-tagged blocks
        for (tag, body) in &blocks {
            if RUST_TAGS.contains(&tag.as_str()) {
                return Ok(body.clone());
            }
        }

        // Pass 2: fall back to first block that isn't a known non-Rust language
        const NON_RUST_TAGS: &[&str] = &[
            "json",
            "bash",
            "sh",
            "shell",
            "text",
            "txt",
            "yaml",
            "yml",
            "toml",
            "sql",
            "python",
            "py",
            "javascript",
            "js",
            "typescript",
            "ts",
            "go",
            "html",
            "css",
            "xml",
        ];
        for (tag, body) in &blocks {
            if NON_RUST_TAGS.contains(&tag.as_str()) {
                continue;
            }
            if body.starts_with('{') {
                continue;
            }
            return Ok(body.clone());
        }

        // If no code block found, use the response as-is
        Ok(trimmed.to_string())
    }
}

#[async_trait::async_trait]
impl<'a> LanguageCodeGenerator for RustCodeGenerator<'a> {
    fn set_local_package(&self, package: Option<String>) {
        *super::lock_or_recover(&self.local_package) = package;
    }

    async fn generate_test_code(&self, pattern: &CodePattern) -> Result<String> {
        debug!("Generating Rust test code for pattern: {}", pattern.name);

        let local_pkg = super::lock_or_recover(&self.local_package).clone();
        let prompt = Self::create_test_prompt(
            pattern,
            self.custom_instructions.as_deref(),
            local_pkg.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;

        let code = Self::extract_code_from_response(&response)?;

        debug!("Generated {} bytes of Rust test code", code.len());
        debug!("Generated Rust test code:\n{}", code);
        Ok(code)
    }

    async fn retry_test_code(
        &self,
        pattern: &CodePattern,
        previous_code: &str,
        error_output: &str,
    ) -> Result<String> {
        debug!(
            "Retrying Rust test code for pattern: {} (fixing error)",
            pattern.name
        );

        let local_pkg = super::lock_or_recover(&self.local_package).clone();
        let prompt = build_retry_prompt(
            pattern,
            &RUST_ENV,
            previous_code,
            error_output,
            local_pkg.as_deref(),
            self.custom_instructions.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;
        let code = Self::extract_code_from_response(&response)?;

        debug!("Retry generated {} bytes of Rust test code", code.len());
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_agent::PatternCategory;

    fn sample_pattern() -> CodePattern {
        CodePattern {
            name: "Basic HTTP Client".to_string(),
            description: "Create a reqwest client and make a GET request".to_string(),
            code: r#"let client = reqwest::Client::new();
let resp = client.get("https://httpbin.org/get")
    .send()
    .await?;
println!("{}", resp.status());"#
                .to_string(),
            category: PatternCategory::BasicUsage,
        }
    }

    #[test]
    fn test_create_test_prompt_contains_pattern_info() {
        let pattern = sample_pattern();
        let prompt = RustCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("Basic HTTP Client"));
        assert!(prompt.contains("Create a reqwest client"));
        assert!(prompt.contains("reqwest::Client::new()"));
        assert!(prompt.contains("Test passed: Basic HTTP Client"));
    }

    #[test]
    fn test_create_test_prompt_rust_env_notes() {
        let pattern = sample_pattern();
        let prompt = RustCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("fn main()"));
        assert!(prompt.contains("eprintln!"));
        assert!(prompt.contains("cargo run"));
    }

    #[test]
    fn test_create_test_prompt_is_minimal() {
        let pattern = sample_pattern();
        let prompt = RustCodeGenerator::create_test_prompt(&pattern, None, None);

        // Should NOT contain coaching — the SKILL.md is the test
        assert!(
            !prompt.contains("#[tokio::main]"),
            "prompt should not coach on tokio"
        );
        assert!(
            !prompt.contains("ASSERTION RULES"),
            "prompt should not dictate assertion style"
        );
        assert!(
            !prompt.contains("COMPILATION RULES"),
            "prompt should not teach Rust compilation"
        );
    }

    #[test]
    fn test_create_test_prompt_local_package() {
        let pattern = sample_pattern();
        let prompt = RustCodeGenerator::create_test_prompt(&pattern, None, Some("reqwest"));

        assert!(prompt.contains("locally"));
        assert!(prompt.contains("reqwest"));
    }

    #[test]
    fn test_create_test_prompt_no_local_package() {
        let pattern = sample_pattern();
        let prompt = RustCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(!prompt.contains("locally"));
    }

    #[test]
    fn test_create_test_prompt_with_custom_instructions() {
        let pattern = sample_pattern();
        let prompt = RustCodeGenerator::create_test_prompt(
            &pattern,
            Some("Always test error paths. Use unwrap_or_else."),
            None,
        );

        assert!(prompt.contains("Always test error paths"));
    }

    #[test]
    fn test_create_test_prompt_with_both_options() {
        let pattern = sample_pattern();
        let prompt = RustCodeGenerator::create_test_prompt(
            &pattern,
            Some("Use async runtime"),
            Some("reqwest"),
        );

        assert!(prompt.contains("locally"));
        assert!(prompt.contains("Use async runtime"));
    }

    #[test]
    fn test_extract_code_from_rust_block() {
        let response = r#"
Here's the test:

```rust
fn main() {
    println!("✓ Test passed: Basic HTTP Client");
}
```
"#;
        let code = RustCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("fn main()"));
        assert!(code.contains("println!"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_rs_block() {
        let response = r#"
```rs
fn main() {
    let x = 42;
    println!("{x}");
}
```
"#;
        let code = RustCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("fn main()"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_generic_block() {
        let response = r#"
```
fn main() {
    std::process::exit(0);
}
```
"#;
        let code = RustCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("fn main()"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_raw_code() {
        let response = r#"
fn main() {
    println!("hello");
}
"#;
        let code = RustCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("fn main()"));
    }

    #[tokio::test]
    async fn test_generate_test_code_with_mock() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = RustCodeGenerator::new(&mock_client);

        let pattern = sample_pattern();
        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
        let code = result.unwrap();
        assert!(!code.is_empty());
    }

    #[tokio::test]
    async fn test_generate_test_code_with_local_package() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = RustCodeGenerator::new(&mock_client);
        generator.set_local_package(Some("reqwest".to_string()));

        let pattern = sample_pattern();
        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_test_code_with_custom_instructions() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = RustCodeGenerator::new(&mock_client)
            .with_custom_instructions(Some("Use tokio runtime".to_string()));

        let pattern = sample_pattern();
        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_code_from_tilde_rust_block() {
        let response = r#"
Here's the test:

~~~rust
fn main() {
    println!("✓ Test passed: Tilde");
}
~~~
"#;
        let code = RustCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("fn main()"));
        assert!(code.contains("println!"));
        assert!(!code.contains("~~~"));
    }

    #[test]
    fn test_extract_code_from_generic_tilde_block() {
        let response = "~~~\nfn main() {\n    std::process::exit(0);\n}\n~~~\n";
        let code = RustCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("fn main()"));
        assert!(!code.contains("~~~"));
    }

    #[test]
    fn test_extract_code_skips_non_rust_blocks() {
        // Python block followed by a generic block with Rust code
        let response =
            "```python\nprint('hello')\n```\n\n```\nfn main() { println!(\"hi\"); }\n```\n";
        let code = RustCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(
            code.contains("fn main()"),
            "should skip python block and use generic: {}",
            code
        );
        assert!(!code.contains("print('hello')"));
    }

    #[test]
    fn test_extract_code_skips_json_blocks() {
        // JSON-looking block (starts with {) followed by Rust code
        let response =
            "```\n{\"key\": \"value\"}\n```\n\n```\nfn main() { println!(\"ok\"); }\n```\n";
        let code = RustCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(
            code.contains("fn main()"),
            "should skip JSON block: {}",
            code
        );
    }

    #[tokio::test]
    async fn test_retry_test_code_with_mock() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = RustCodeGenerator::new(&mock_client);

        let pattern = sample_pattern();
        let result = generator
            .retry_test_code(
                &pattern,
                "fn main() { println!(\"old\"); }",
                "error[E0425]: cannot find value `x`",
            )
            .await;
        assert!(result.is_ok());
        let code = result.unwrap();
        assert!(!code.is_empty());
    }

    #[tokio::test]
    async fn test_retry_test_code_with_local_package() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = RustCodeGenerator::new(&mock_client)
            .with_custom_instructions(Some("Use tokio runtime".to_string()));
        generator.set_local_package(Some("reqwest".to_string()));

        let pattern = sample_pattern();
        let result = generator
            .retry_test_code(
                &pattern,
                "fn main() { reqwest::get(\"url\"); }",
                "error: unresolved import",
            )
            .await;
        assert!(result.is_ok());
    }

    poison_recovery_tests!(RustCodeGenerator, sample_pattern);
}
