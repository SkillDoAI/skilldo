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

    /// Extract Python code from markdown code blocks (supports both ``` and ~~~ fences).
    /// Two-pass strategy: prefer Python-tagged blocks, fall back to first generic block.
    fn extract_code_from_response(response: &str) -> Result<String> {
        let trimmed = response.trim();

        const PYTHON_TAGS: &[&str] = &["python", "python3", "py"];

        // Collect all fenced blocks with their tags
        let blocks = Self::find_fenced_blocks(trimmed);

        // Pass 1: prefer Python-tagged blocks
        for (tag, body) in &blocks {
            if PYTHON_TAGS.contains(&tag.as_str()) {
                return Ok(body.clone());
            }
        }

        // Pass 2: fall back to first block (any tag)
        if let Some((_tag, body)) = blocks.first() {
            return Ok(body.clone());
        }

        // If no code block found, use the response as-is (may be raw code)
        Ok(trimmed.to_string())
    }

    /// Find all fenced code blocks in text, returning (tag, body) pairs.
    /// The tag is the text on the same line as the opening fence (e.g., "python" in ```python).
    fn find_fenced_blocks(text: &str) -> Vec<(String, String)> {
        let mut blocks = Vec::new();
        let mut pos = 0;
        while pos < text.len() {
            let (fence, start) = if let Some(idx) = text[pos..].find("```") {
                if let Some(tilde_idx) = text[pos..].find("~~~") {
                    if tilde_idx < idx {
                        ("~~~", pos + tilde_idx)
                    } else {
                        ("```", pos + idx)
                    }
                } else {
                    ("```", pos + idx)
                }
            } else if let Some(idx) = text[pos..].find("~~~") {
                ("~~~", pos + idx)
            } else {
                break;
            };

            let after = start + fence.len();
            if let Some(end) = text[after..].find(fence) {
                let raw = &text[after..after + end];
                // Tag is the text on the opener line (before first newline)
                let (tag, body) = if let Some(nl) = raw.find('\n') {
                    let tag_part = raw[..nl].trim().to_ascii_lowercase();
                    let body = raw[nl + 1..].trim().to_string();
                    (tag_part, body)
                } else {
                    (String::new(), raw.trim().to_string())
                };
                blocks.push((tag, body));
                pos = after + end + fence.len();
            } else {
                break;
            }
        }
        blocks
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
    fn test_extract_code_from_tilde_python_block() {
        let response = r#"
Here's the test:

~~~python
import click

@click.command()
def hello():
    print("Hello")
~~~
"#;

        let code = PythonCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("import click"));
        assert!(code.contains("def hello():"));
        assert!(!code.contains("~~~"));
    }

    #[test]
    fn test_extract_code_from_python3_fence() {
        let response = "```python3\nimport json\nprint('ok')\n```";
        let code = PythonCodeGenerator::extract_code_from_response(response).unwrap();
        assert_eq!(code, "import json\nprint('ok')");
        assert!(!code.contains("python3"), "python3 tag should be stripped");
    }

    #[test]
    fn test_extract_code_from_generic_tilde_block() {
        let response = r#"
~~~
import sys
print(sys.version)
~~~
"#;

        let code = PythonCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("import sys"));
        assert!(!code.contains("~~~"));
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

    #[test]
    fn test_extract_code_prefers_python_tagged_over_bash() {
        // LLM response with bash block first, then Python block.
        // Pass 1 should find the Python block; bash should be skipped.
        let response = r#"First install:

```bash
pip install click
```

Then run:

```python
import click

@click.command()
def hello():
    click.echo("Hello")

if __name__ == '__main__':
    hello()
```
"#;
        let code = PythonCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(
            code.contains("import click"),
            "should extract the Python block, got: {code}"
        );
        assert!(
            !code.contains("pip install"),
            "should NOT extract the bash block"
        );
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
