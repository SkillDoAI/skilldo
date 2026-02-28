use anyhow::Result;
use std::sync::Mutex;
use tracing::debug;

use super::{CodePattern, LanguageCodeGenerator};
use crate::llm::client::LlmClient;

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
    /// Generate the prompt for creating test code from a pattern
    pub fn create_test_prompt(
        pattern: &CodePattern,
        custom_instructions: Option<&str>,
        local_package: Option<&str>,
    ) -> String {
        let mut prompt = format!(
            r#"You are validating a SKILL.md file by writing test code.

Your task: Write a COMPLETE, RUNNABLE Python script that thoroughly tests this pattern.

Pattern: {}
Description: {}

Example from SKILL.md:
```python
{}
```

ENVIRONMENT: This runs via `uv run test.py` in an isolated container with internet access but NO TTY.

CRITICAL RULES:
1. Start with PEP 723 inline script metadata declaring dependencies with CORRECT PyPI package names:
   # /// script
   # requires-python = ">=3.11"
   # dependencies = ["actual-pypi-package-name"]
   # ///
   (e.g., use "scikit-learn" not "sklearn", "Pillow" not "PIL", "beautifulsoup4" not "bs4")
2. Declare ALL packages your code imports in the dependencies — think through what you need BEFORE writing code. If you use numpy, requests, etc. alongside the main library, include them
3. If the library has a built-in test client (TestClient, test_client(), CliRunner, etc.), USE IT
4. Do NOT assert on ANSI codes, colors, or terminal formatting - no TTY available
5. For output capture, use StringIO and assert on TEXT CONTENT only
6. For HTTP client libraries: use https://httpbin.org for testing (e.g., /get, /post, /status/404, /status/500). For simple GET tests, https://www.google.com is also fine. Do NOT use httpstat.us
7. Keep it under 40 lines, COMPLETE and RUNNABLE with no placeholders
8. Print "✓ Test passed: {}" on success
9. Test real functionality with real assertions - not just "does it import"

ASSERTION RULES (critical for reliability):
- NEVER assert exact floating point values. Use ranges: `assert 0.0 <= score <= 1.0`
- NEVER assert `isinstance(x, int)` for numeric data — libraries return numpy/custom types. Use `hasattr(x, '__len__')` or check `.shape`
- NEVER assert `isinstance(x, list)` — data may be arrays, tuples, or custom sequences. Check `len(x) > 0` instead
- NEVER hardcode expected string content from network responses or datasets that may change
- DO assert on shapes, lengths, types of return values, ranges, and that operations don't raise
- DO use `hasattr`, `len() > 0`, `x.shape == (n, m)`, value ranges, set membership
- Call the API EXACTLY as shown in the example code — do not invent parameters or change signatures
- NEVER assert `__name__` equals a specific value — it varies by execution context (uv run, python, etc.)
- NEVER assert on exact CLI help text formatting or exact exception message strings — check exit codes and broad content instead

Output format:
```python
[your code here - MUST start with # /// script metadata]
```

Write the complete test script now:"#,
            pattern.name, pattern.description, pattern.code, pattern.name
        );

        if let Some(pkg) = local_package {
            prompt.push_str(&format!(
                "\n\nIMPORTANT: The library \"{}\" is installed locally, NOT from PyPI.\nDo NOT include \"{}\" (or its PyPI name) in the PEP 723 dependencies list.\nOnly include OTHER packages your test code needs.\n",
                pkg, pkg
            ));
        }

        if let Some(custom) = custom_instructions {
            prompt.push_str(&format!("\n\n## Additional Instructions\n\n{}\n", custom));
        }

        prompt
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
                let code = trimmed[code_start..code_start + end].trim();
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
        debug!("Generating test code for pattern: {}", pattern.name);

        let local_pkg = self.local_package.lock().unwrap().clone();
        let prompt = Self::create_test_prompt(
            pattern,
            self.custom_instructions.as_deref(),
            local_pkg.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;

        let code = Self::extract_code_from_response(&response)?;

        debug!("Generated {} bytes of test code", code.len());
        debug!("Generated test code:\n{}", code);
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_code_from_response() {
        // Test with python code block
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

    #[test]
    fn test_create_test_prompt() {
        let pattern = CodePattern {
            name: "Basic Click Command".to_string(),
            description: "Create a simple CLI command".to_string(),
            code: "import click\n\n@click.command()\ndef hello():\n    pass".to_string(),
            category: super::super::PatternCategory::BasicUsage,
        };

        let prompt = PythonCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("Basic Click Command"));
        assert!(prompt.contains("Create a simple CLI command"));
        assert!(prompt.contains("import click"));
        assert!(prompt.contains("✓ Test passed: Basic Click Command"));
    }

    #[test]
    fn test_create_test_prompt_local_package() {
        let pattern = CodePattern {
            name: "Basic Click Command".to_string(),
            description: "Create a simple CLI command".to_string(),
            code: "import click\n\n@click.command()\ndef hello():\n    pass".to_string(),
            category: super::super::PatternCategory::BasicUsage,
        };

        let prompt = PythonCodeGenerator::create_test_prompt(&pattern, None, Some("click"));
        assert!(prompt.contains("installed locally"));
        assert!(prompt.contains("click"));
        assert!(prompt.contains("Do NOT include"));
    }

    #[test]
    fn test_create_test_prompt_no_local_package() {
        let pattern = CodePattern {
            name: "Test".to_string(),
            description: "Test".to_string(),
            code: "import requests".to_string(),
            category: super::super::PatternCategory::BasicUsage,
        };

        let prompt = PythonCodeGenerator::create_test_prompt(&pattern, None, None);
        assert!(!prompt.contains("installed locally"));
    }

    // --- Coverage: create_test_prompt with custom_instructions (line 94-95) ---
    #[test]
    fn test_create_test_prompt_with_custom_instructions() {
        let pattern = CodePattern {
            name: "Custom Test".to_string(),
            description: "Tests custom instructions".to_string(),
            code: "import click\nclick.echo('hi')".to_string(),
            category: super::super::PatternCategory::BasicUsage,
        };

        let prompt = PythonCodeGenerator::create_test_prompt(
            &pattern,
            Some("Use pytest-style assertions. Always test edge cases."),
            None,
        );
        assert!(prompt.contains("Additional Instructions"));
        assert!(prompt.contains("Use pytest-style assertions"));
    }

    // --- Coverage: create_test_prompt with both local_package AND custom_instructions ---
    #[test]
    fn test_create_test_prompt_with_both_options() {
        let pattern = CodePattern {
            name: "Full Options".to_string(),
            description: "All options test".to_string(),
            code: "import mylib\nmylib.run()".to_string(),
            category: super::super::PatternCategory::BasicUsage,
        };

        let prompt = PythonCodeGenerator::create_test_prompt(
            &pattern,
            Some("Extra instructions here"),
            Some("mylib"),
        );
        assert!(prompt.contains("installed locally"));
        assert!(prompt.contains("mylib"));
        assert!(prompt.contains("Do NOT include"));
        assert!(prompt.contains("Additional Instructions"));
        assert!(prompt.contains("Extra instructions here"));
    }

    // --- Coverage: generate_test_code path (lines 135-149) via mock LLM ---
    #[tokio::test]
    async fn test_generate_test_code_with_mock() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = PythonCodeGenerator::new(&mock_client);

        let pattern = CodePattern {
            name: "Mock Pattern".to_string(),
            description: "Testing generate_test_code".to_string(),
            code: "import os\nos.getcwd()".to_string(),
            category: super::super::PatternCategory::BasicUsage,
        };

        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
        // Mock returns {"status": "mock"} for unrecognized prompts
        let code = result.unwrap();
        assert!(!code.is_empty());
    }

    // --- Coverage: generate_test_code with local_package set ---
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
            category: super::super::PatternCategory::BasicUsage,
        };

        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    // --- Coverage: generate_test_code with custom_instructions ---
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
            category: super::super::PatternCategory::BasicUsage,
        };

        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }
}
