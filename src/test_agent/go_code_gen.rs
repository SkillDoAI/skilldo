//! Go test code generator — uses the shared prompt builder with minimal
//! Go-specific environment notes. The prompt intentionally avoids coaching
//! the LLM on how to write Go — the SKILL.md should be sufficient.

use anyhow::Result;
use std::sync::Mutex;
use tracing::debug;

use super::code_generator::{build_retry_prompt, build_test_prompt, TestEnv};
use super::{CodePattern, LanguageCodeGenerator};
use crate::llm::client::LlmClient;

/// Go environment: runner command + 1–2 line notes the LLM can't infer.
pub const GO_ENV: TestEnv = TestEnv {
    lang_tag: "go",
    runner: "`go run main.go`",
    env_notes: "\
- Write a `package main` program with `func main()` — no testing.T is available
- Use `log.Fatal()` or `log.Fatalf()` for assertion failures",
};

/// Go code generator using LLM
pub struct GoCodeGenerator<'a> {
    llm_client: &'a dyn LlmClient,
    custom_instructions: Option<String>,
    /// Package name to exclude from `go get` deps (for local-mount modes).
    local_package: Mutex<Option<String>>,
}

impl<'a> GoCodeGenerator<'a> {
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
        build_test_prompt(pattern, &GO_ENV, local_package, custom_instructions)
    }

    /// Extract Go code from markdown code blocks (supports both ``` and ~~~ fences).
    fn extract_code_from_response(response: &str) -> Result<String> {
        let trimmed = response.trim();

        // Try language-tagged fence first (```go or ~~~go), then generic fence
        for fence in &["```", "~~~"] {
            let go_fence = format!("{fence}go");
            if let Some(start) = trimmed.find(&go_fence) {
                let code_start = start + go_fence.len();
                // Skip optional "lang" suffix (e.g., ```golang)
                let after = &trimmed[code_start..];
                let newline_pos = after.find('\n').unwrap_or(0);
                let actual_start = code_start + newline_pos;
                if let Some(end) = trimmed[actual_start..].find(*fence) {
                    let code = trimmed[actual_start..actual_start + end].trim();
                    return Ok(code.to_string());
                }
            }
        }

        // Try generic ``` or ~~~ code block
        for fence in &["```", "~~~"] {
            if let Some(start) = trimmed.find(*fence) {
                let code_start = start + fence.len();
                if let Some(end) = trimmed[code_start..].find(*fence) {
                    let mut code = trimmed[code_start..code_start + end].trim();
                    // Strip known language tags
                    if let Some((first_line, rest)) = code.split_once('\n') {
                        let tag = first_line.trim().to_ascii_lowercase();
                        const KNOWN_TAGS: &[&str] = &[
                            "go", "golang", "bash", "sh", "shell", "text", "txt", "json", "yaml",
                            "yml", "toml",
                        ];
                        if KNOWN_TAGS.contains(&tag.as_str()) {
                            code = rest.trim();
                        }
                    }
                    return Ok(code.to_string());
                }
            }
        }

        // If no code block found, use the response as-is
        Ok(trimmed.to_string())
    }
}

#[async_trait::async_trait]
impl<'a> LanguageCodeGenerator for GoCodeGenerator<'a> {
    fn set_local_package(&self, package: Option<String>) {
        *self.local_package.lock().unwrap() = package;
    }

    async fn generate_test_code(&self, pattern: &CodePattern) -> Result<String> {
        debug!("Generating Go test code for pattern: {}", pattern.name);

        let local_pkg = self.local_package.lock().unwrap().clone();
        let prompt = Self::create_test_prompt(
            pattern,
            self.custom_instructions.as_deref(),
            local_pkg.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;

        let code = Self::extract_code_from_response(&response)?;

        debug!("Generated {} bytes of Go test code", code.len());
        debug!("Generated Go test code:\n{}", code);
        Ok(code)
    }

    async fn retry_test_code(
        &self,
        pattern: &CodePattern,
        previous_code: &str,
        error_output: &str,
    ) -> Result<String> {
        debug!(
            "Retrying Go test code for pattern: {} (fixing error)",
            pattern.name
        );

        let local_pkg = self.local_package.lock().unwrap().clone();
        let prompt = build_retry_prompt(
            pattern,
            &GO_ENV,
            previous_code,
            error_output,
            local_pkg.as_deref(),
            self.custom_instructions.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;
        let code = Self::extract_code_from_response(&response)?;

        debug!("Retry generated {} bytes of Go test code", code.len());
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_agent::PatternCategory;

    fn sample_pattern() -> CodePattern {
        CodePattern {
            name: "Basic Router Setup".to_string(),
            description: "Create a chi router and add routes".to_string(),
            code: r#"r := chi.NewRouter()
r.Use(middleware.Logger)
r.Get("/", func(w http.ResponseWriter, r *http.Request) {
    w.Write([]byte("hello"))
})"#
            .to_string(),
            category: PatternCategory::BasicUsage,
        }
    }

    #[test]
    fn test_create_test_prompt_contains_pattern_info() {
        let pattern = sample_pattern();
        let prompt = GoCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("Basic Router Setup"));
        assert!(prompt.contains("Create a chi router"));
        assert!(prompt.contains("chi.NewRouter()"));
        assert!(prompt.contains("Test passed: Basic Router Setup"));
    }

    #[test]
    fn test_create_test_prompt_go_env_notes() {
        let pattern = sample_pattern();
        let prompt = GoCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("package main"));
        assert!(prompt.contains("log.Fatal"));
        assert!(prompt.contains("go run main.go"));
    }

    #[test]
    fn test_create_test_prompt_is_minimal() {
        let pattern = sample_pattern();
        let prompt = GoCodeGenerator::create_test_prompt(&pattern, None, None);

        // Should NOT contain coaching — the SKILL.md is the test
        assert!(
            !prompt.contains("httptest"),
            "prompt should not coach on httptest"
        );
        assert!(
            !prompt.contains("ASSERTION RULES"),
            "prompt should not dictate assertion style"
        );
        assert!(
            !prompt.contains("COMPILATION RULES"),
            "prompt should not teach Go compilation"
        );
    }

    #[test]
    fn test_create_test_prompt_local_package() {
        let pattern = sample_pattern();
        let prompt =
            GoCodeGenerator::create_test_prompt(&pattern, None, Some("github.com/go-chi/chi/v5"));

        assert!(prompt.contains("locally"));
        assert!(prompt.contains("github.com/go-chi/chi/v5"));
    }

    #[test]
    fn test_create_test_prompt_no_local_package() {
        let pattern = sample_pattern();
        let prompt = GoCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(!prompt.contains("locally"));
    }

    #[test]
    fn test_create_test_prompt_with_custom_instructions() {
        let pattern = sample_pattern();
        let prompt = GoCodeGenerator::create_test_prompt(
            &pattern,
            Some("Always test error paths. Use table-driven tests."),
            None,
        );

        assert!(prompt.contains("Always test error paths"));
    }

    #[test]
    fn test_create_test_prompt_with_both_options() {
        let pattern = sample_pattern();
        let prompt = GoCodeGenerator::create_test_prompt(
            &pattern,
            Some("Use subtests"),
            Some("github.com/go-chi/chi/v5"),
        );

        assert!(prompt.contains("locally"));
        assert!(prompt.contains("Use subtests"));
    }

    #[test]
    fn test_extract_code_from_go_block() {
        let response = r#"
Here's the test:

```go
package main

import "fmt"

func main() {
    fmt.Println("✓ Test passed: Basic")
}
```
"#;
        let code = GoCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("package main"));
        assert!(code.contains("fmt.Println"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_golang_block() {
        let response = r#"
```golang
package main

func main() {}
```
"#;
        let code = GoCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("package main"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_generic_block() {
        let response = r#"
```
package main

import "os"

func main() {
    os.Exit(0)
}
```
"#;
        let code = GoCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("package main"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_raw_code() {
        let response = r#"
package main

func main() {}
"#;
        let code = GoCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("package main"));
    }

    #[tokio::test]
    async fn test_generate_test_code_with_mock() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = GoCodeGenerator::new(&mock_client);

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
        let generator = GoCodeGenerator::new(&mock_client);
        generator.set_local_package(Some("github.com/go-chi/chi/v5".to_string()));

        let pattern = sample_pattern();
        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_test_code_with_custom_instructions() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = GoCodeGenerator::new(&mock_client)
            .with_custom_instructions(Some("Use table-driven tests".to_string()));

        let pattern = sample_pattern();
        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_code_strips_language_tag_in_generic_block() {
        let response = "```go\npackage main\n\nfunc main() {}\n```";
        let code = GoCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("package main"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_tilde_go_block() {
        let response = r#"
Here's the test:

~~~go
package main

import "fmt"

func main() {
    fmt.Println("✓ Test passed: Tilde")
}
~~~
"#;
        let code = GoCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("package main"));
        assert!(code.contains("fmt.Println"));
        assert!(!code.contains("~~~"));
    }

    #[test]
    fn test_extract_code_from_tilde_golang_block() {
        let response = "~~~golang\npackage main\n\nfunc main() {}\n~~~\n";
        let code = GoCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("package main"));
        assert!(!code.contains("~~~"));
    }

    #[test]
    fn test_extract_code_from_generic_tilde_block() {
        let response =
            "~~~\npackage main\n\nimport \"os\"\n\nfunc main() {\n    os.Exit(0)\n}\n~~~\n";
        let code = GoCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("package main"));
        assert!(!code.contains("~~~"));
    }
}
