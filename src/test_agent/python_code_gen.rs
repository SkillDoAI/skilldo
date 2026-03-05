//! Python test code generator — uses the shared prompt builder with minimal
//! Python-specific environment notes. The prompt intentionally avoids coaching
//! the LLM on how to write Python — the SKILL.md should be sufficient.

use anyhow::Result;
use std::sync::Mutex;
use tracing::debug;

use super::code_generator::{build_retry_prompt, build_test_prompt, TestEnv};
use super::{CodePattern, LanguageCodeGenerator};
use crate::llm::client::LlmClient;

/// Python environment: runner command + PEP 723 note the LLM can't infer.
pub const PYTHON_ENV: TestEnv = TestEnv {
    lang_tag: "python",
    runner: "`uv run test.py`",
    env_notes: "\
- Start with PEP 723 inline script metadata: `# /// script` / `# dependencies = [...]` / `# ///`
- Use correct PyPI names in dependencies (e.g., \"scikit-learn\" not \"sklearn\", \"Pillow\" not \"PIL\")",
};

/// Python code generator using LLM
pub struct PythonCodeGenerator<'a> {
    llm_client: &'a dyn LlmClient,
    custom_instructions: Option<String>,
    /// Package name to exclude from PEP 723 deps (for local-install/local-mount modes).
    /// Uses Mutex so it can be set after construction (e.g., after parsing SKILL.md).
    local_package: Mutex<Option<String>>,
}

impl<'a> PythonCodeGenerator<'a> {
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
        build_test_prompt(pattern, &PYTHON_ENV, local_package, custom_instructions)
    }

    /// Extract Python code from markdown code blocks
    fn extract_code_from_response(response: &str) -> Result<String> {
        let trimmed = response.trim();

        // Try to extract from ```python code block
        if let Some(start) = trimmed.find("```python") {
            let code_start = start + "```python".len();
            if let Some(end) = trimmed[code_start..].find("```") {
                let code = trimmed[code_start..code_start + end].trim();
                return Ok(code.to_string());
            }
        }

        // Try generic ``` code block
        if let Some(start) = trimmed.find("```") {
            let code_start = start + "```".len();
            if let Some(end) = trimmed[code_start..].find("```") {
                let mut code = trimmed[code_start..code_start + end].trim();
                // Strip known language tags (e.g., "python", "py", "sh")
                if let Some((first_line, rest)) = code.split_once('\n') {
                    let tag = first_line.trim().to_ascii_lowercase();
                    const KNOWN_TAGS: &[&str] = &[
                        "py", "python", "python3", "bash", "sh", "shell", "zsh", "fish", "text",
                        "txt", "json", "yaml", "yml", "toml",
                    ];
                    if KNOWN_TAGS.contains(&tag.as_str()) {
                        code = rest.trim();
                    }
                }
                return Ok(code.to_string());
            }
        }

        // If no code block found, use the response as-is (may be raw code)
        Ok(trimmed.to_string())
    }
}

#[async_trait::async_trait]
impl<'a> LanguageCodeGenerator for PythonCodeGenerator<'a> {
    fn set_local_package(&self, package: Option<String>) {
        *self.local_package.lock().unwrap() = package;
    }

    async fn generate_test_code(&self, pattern: &CodePattern) -> Result<String> {
        debug!("Generating Python test code for pattern: {}", pattern.name);

        let local_pkg = self.local_package.lock().unwrap().clone();
        let prompt = Self::create_test_prompt(
            pattern,
            self.custom_instructions.as_deref(),
            local_pkg.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;

        let code = Self::extract_code_from_response(&response)?;

        debug!("Generated {} bytes of Python test code", code.len());
        debug!("Generated Python test code:\n{}", code);
        Ok(code)
    }

    async fn retry_test_code(
        &self,
        pattern: &CodePattern,
        previous_code: &str,
        error_output: &str,
    ) -> Result<String> {
        debug!(
            "Retrying Python test code for pattern: {} (fixing error)",
            pattern.name
        );

        let local_pkg = self.local_package.lock().unwrap().clone();
        let prompt = build_retry_prompt(
            pattern,
            &PYTHON_ENV,
            previous_code,
            error_output,
            local_pkg.as_deref(),
            self.custom_instructions.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;
        let code = Self::extract_code_from_response(&response)?;

        debug!("Retry generated {} bytes of Python test code", code.len());
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_agent::PatternCategory;

    #[test]
    fn test_extract_code_from_response() {
        let response = r#"
Here's the test:

```python
import click

@click.command()
def hello():
    print("Hello")

if __name__ == '__main__':
    hello()
```
"#;

        let code = PythonCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("import click"));
        assert!(code.contains("def hello():"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_generic_block() {
        let response = r#"
```
import sys
print(sys.version)
```
"#;

        let code = PythonCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("import sys"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_raw_code() {
        let response = r#"
import os
print(os.getcwd())
"#;

        let code = PythonCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("import os"));
    }

    fn sample_pattern() -> CodePattern {
        CodePattern {
            name: "Basic Click Command".to_string(),
            description: "Create a simple CLI command".to_string(),
            code: "import click\n\n@click.command()\ndef hello():\n    pass".to_string(),
            category: PatternCategory::BasicUsage,
        }
    }

    #[test]
    fn test_create_test_prompt_contains_pattern_info() {
        let pattern = sample_pattern();
        let prompt = PythonCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("Basic Click Command"));
        assert!(prompt.contains("Create a simple CLI command"));
        assert!(prompt.contains("import click"));
        assert!(prompt.contains("Test passed: Basic Click Command"));
    }

    #[test]
    fn test_create_test_prompt_python_env_notes() {
        let pattern = sample_pattern();
        let prompt = PythonCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("PEP 723"));
        assert!(prompt.contains("uv run test.py"));
    }

    #[test]
    fn test_create_test_prompt_is_minimal() {
        let pattern = sample_pattern();
        let prompt = PythonCodeGenerator::create_test_prompt(&pattern, None, None);

        // Should NOT contain verbose coaching — the SKILL.md is the test
        assert!(
            !prompt.contains("ASSERTION RULES"),
            "prompt should not dictate assertion style"
        );
        assert!(
            !prompt.contains("CRITICAL RULES"),
            "prompt should not have numbered rules list"
        );
    }

    #[test]
    fn test_create_test_prompt_local_package() {
        let pattern = sample_pattern();
        let prompt = PythonCodeGenerator::create_test_prompt(&pattern, None, Some("click"));

        assert!(prompt.contains("locally"));
        assert!(prompt.contains("click"));
    }

    #[test]
    fn test_create_test_prompt_no_local_package() {
        let pattern = sample_pattern();
        let prompt = PythonCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(!prompt.contains("locally"));
    }

    #[test]
    fn test_create_test_prompt_with_custom_instructions() {
        let pattern = sample_pattern();
        let prompt = PythonCodeGenerator::create_test_prompt(
            &pattern,
            Some("Use pytest-style assertions. Always test edge cases."),
            None,
        );

        assert!(prompt.contains("Use pytest-style assertions"));
    }

    #[test]
    fn test_create_test_prompt_with_both_options() {
        let pattern = sample_pattern();
        let prompt = PythonCodeGenerator::create_test_prompt(
            &pattern,
            Some("Extra instructions here"),
            Some("click"),
        );

        assert!(prompt.contains("locally"));
        assert!(prompt.contains("Extra instructions here"));
    }

    #[tokio::test]
    async fn test_generate_test_code_with_mock() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = PythonCodeGenerator::new(&mock_client);

        let pattern = CodePattern {
            name: "Mock Pattern".to_string(),
            description: "Testing generate_test_code".to_string(),
            code: "import os\nos.getcwd()".to_string(),
            category: PatternCategory::BasicUsage,
        };

        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
        let code = result.unwrap();
        assert!(!code.is_empty());
    }

    #[tokio::test]
    async fn test_generate_test_code_with_local_package() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = PythonCodeGenerator::new(&mock_client);
        generator.set_local_package(Some("mylib".to_string()));

        let pattern = CodePattern {
            name: "Local Package".to_string(),
            description: "Testing with local package".to_string(),
            code: "import mylib\nmylib.run()".to_string(),
            category: PatternCategory::BasicUsage,
        };

        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_test_code_with_custom_instructions() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = PythonCodeGenerator::new(&mock_client)
            .with_custom_instructions(Some("Always test edge cases".to_string()));

        let pattern = CodePattern {
            name: "Custom Instructions".to_string(),
            description: "Test with custom instructions".to_string(),
            code: "import os".to_string(),
            category: PatternCategory::BasicUsage,
        };

        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }
}
