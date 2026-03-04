//! Go test code generator — takes parsed code patterns from SKILL.md and
//! asks an LLM to produce runnable Go test scripts that verify each pattern.

use anyhow::Result;
use std::sync::Mutex;
use tracing::debug;

use super::{CodePattern, LanguageCodeGenerator};
use crate::llm::client::LlmClient;

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

    /// Generate the prompt for creating test code from a pattern
    pub fn create_test_prompt(
        pattern: &CodePattern,
        custom_instructions: Option<&str>,
        local_package: Option<&str>,
    ) -> String {
        let mut prompt = format!(
            r#"You are validating a SKILL.md file by writing test code.

Your task: Write a COMPLETE, RUNNABLE Go program that thoroughly tests this pattern.

Pattern: {}
Description: {}

Example from SKILL.md:
```go
{}
```

ENVIRONMENT: This runs via `go run main.go` in an isolated container with internet access but NO TTY.
The container has `go mod init test` already done.

CRITICAL RULES:
1. Write a complete `package main` program with a `func main()` entry point
2. Include ALL imports your code needs (both stdlib and third-party)
3. Use `log.Fatal()` or `log.Fatalf()` for assertion failures — no testing.T available
4. For comparisons: `if got != want {{ log.Fatalf("got %v, want %v", got, want) }}`
5. Always handle errors: `if err != nil {{ log.Fatal(err) }}`
6. Print "✓ Test passed: {}" on success (at the end of main)
7. Keep it under 50 lines, COMPLETE and RUNNABLE with no placeholders
8. Test real functionality with real assertions — not just "does it import"
9. For HTTP libraries: create a test server with `httptest.NewServer()` instead of making external requests
10. Third-party packages will be installed via `go get` before your code runs

ASSERTION RULES (critical for reliability):
- NEVER assert exact floating point values. Use ranges or epsilon comparisons
- NEVER hardcode expected string content from network responses that may change
- DO assert on types, lengths, presence of keys, value ranges, and that operations don't error
- Call the API EXACTLY as shown in the example code — do not invent parameters or change signatures
- For HTTP handlers: use httptest.NewServer and make real requests to verify behavior

Output format:
```go
[your code here - MUST be a complete package main program]
```

Write the complete test program now:"#,
            pattern.name, pattern.description, pattern.code, pattern.name
        );

        if let Some(pkg) = local_package {
            prompt.push_str(&format!(
                "\n\nIMPORTANT: The library \"{}\" is mounted locally at /src.\n\
                 It will be available via a `replace` directive in go.mod.\n\
                 Do NOT expect it to be fetched from the internet.\n",
                pkg
            ));
        }

        if let Some(custom) = custom_instructions {
            prompt.push_str(&format!("\n\n## Additional Instructions\n\n{}\n", custom));
        }

        prompt
    }

    /// Extract Go code from markdown code blocks
    fn extract_code_from_response(response: &str) -> Result<String> {
        let trimmed = response.trim();

        // Try to extract from ```go code block
        if let Some(start) = trimmed.find("```go") {
            let code_start = start + "```go".len();
            // Skip optional "lang" suffix (e.g., ```golang)
            let after = &trimmed[code_start..];
            let newline_pos = after.find('\n').unwrap_or(0);
            let actual_start = code_start + newline_pos;
            if let Some(end) = trimmed[actual_start..].find("```") {
                let code = trimmed[actual_start..actual_start + end].trim();
                return Ok(code.to_string());
            }
        }

        // Try generic ``` code block
        if let Some(start) = trimmed.find("```") {
            let code_start = start + "```".len();
            if let Some(end) = trimmed[code_start..].find("```") {
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
        assert!(prompt.contains("✓ Test passed: Basic Router Setup"));
    }

    #[test]
    fn test_create_test_prompt_go_specific_rules() {
        let pattern = sample_pattern();
        let prompt = GoCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("package main"));
        assert!(prompt.contains("func main()"));
        assert!(prompt.contains("log.Fatal"));
        assert!(prompt.contains("go run main.go"));
    }

    #[test]
    fn test_create_test_prompt_local_package() {
        let pattern = sample_pattern();
        let prompt =
            GoCodeGenerator::create_test_prompt(&pattern, None, Some("github.com/go-chi/chi/v5"));

        assert!(prompt.contains("mounted locally"));
        assert!(prompt.contains("github.com/go-chi/chi/v5"));
        assert!(prompt.contains("replace"));
    }

    #[test]
    fn test_create_test_prompt_no_local_package() {
        let pattern = sample_pattern();
        let prompt = GoCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(!prompt.contains("mounted locally"));
    }

    #[test]
    fn test_create_test_prompt_with_custom_instructions() {
        let pattern = sample_pattern();
        let prompt = GoCodeGenerator::create_test_prompt(
            &pattern,
            Some("Always test error paths. Use table-driven tests."),
            None,
        );

        assert!(prompt.contains("Additional Instructions"));
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

        assert!(prompt.contains("mounted locally"));
        assert!(prompt.contains("Additional Instructions"));
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
}
