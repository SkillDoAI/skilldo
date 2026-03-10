//! JavaScript/TypeScript test code generator — uses the shared prompt builder
//! with minimal JS-specific environment notes. The prompt intentionally avoids
//! coaching the LLM on how to write JS — the SKILL.md should be sufficient.

use anyhow::Result;
use std::sync::Mutex;
use tracing::debug;

use super::code_generator::{build_retry_prompt, build_test_prompt, TestEnv};
use super::{CodePattern, LanguageCodeGenerator};
use crate::llm::client::LlmClient;

/// JS environment: runner command + 1–2 line notes the LLM can't infer.
pub const JS_ENV: TestEnv = TestEnv {
    lang_tag: "javascript",
    runner: "`node test.js`",
    env_notes: "\
- Write a single .js file that can be run with `node test.js`
- Use `process.exit(1)` for assertion failures, `console.log()` for success messages",
};

/// JavaScript/TypeScript code generator using LLM
pub struct JsCodeGenerator<'a> {
    llm_client: &'a dyn LlmClient,
    custom_instructions: Option<String>,
    /// Package name to exclude from `npm install` deps (for local-mount modes).
    local_package: Mutex<Option<String>>,
}

impl<'a> JsCodeGenerator<'a> {
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
        build_test_prompt(pattern, &JS_ENV, local_package, custom_instructions)
    }

    /// Extract JS/TS code from markdown code blocks (supports both ``` and ~~~ fences).
    fn extract_code_from_response(response: &str) -> Result<String> {
        let trimmed = response.trim();

        // Try language-tagged fence first, then generic fence
        for fence in &["```", "~~~"] {
            for tag in &["javascript", "js", "typescript", "ts", "jsx", "tsx"] {
                let tagged_fence = format!("{fence}{tag}");
                if let Some(start) = trimmed.find(&tagged_fence) {
                    let code_start = start + tagged_fence.len();
                    let after = &trimmed[code_start..];
                    let newline_pos = after.find('\n').unwrap_or(0);
                    let actual_start = code_start + newline_pos;
                    if let Some(end) = trimmed[actual_start..].find(*fence) {
                        let code = trimmed[actual_start..actual_start + end].trim();
                        return Ok(code.to_string());
                    }
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
                            "javascript",
                            "js",
                            "typescript",
                            "ts",
                            "jsx",
                            "tsx",
                            "bash",
                            "sh",
                            "shell",
                            "text",
                            "txt",
                            "json",
                            "yaml",
                            "yml",
                            "toml",
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
impl<'a> LanguageCodeGenerator for JsCodeGenerator<'a> {
    fn set_local_package(&self, package: Option<String>) {
        *self.local_package.lock().unwrap() = package;
    }

    async fn generate_test_code(&self, pattern: &CodePattern) -> Result<String> {
        debug!(
            "Generating JavaScript test code for pattern: {}",
            pattern.name
        );

        let local_pkg = self.local_package.lock().unwrap().clone();
        let prompt = Self::create_test_prompt(
            pattern,
            self.custom_instructions.as_deref(),
            local_pkg.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;

        let code = Self::extract_code_from_response(&response)?;

        debug!("Generated {} bytes of JavaScript test code", code.len());
        debug!("Generated JavaScript test code:\n{}", code);
        Ok(code)
    }

    async fn retry_test_code(
        &self,
        pattern: &CodePattern,
        previous_code: &str,
        error_output: &str,
    ) -> Result<String> {
        debug!(
            "Retrying JavaScript test code for pattern: {} (fixing error)",
            pattern.name
        );

        let local_pkg = self.local_package.lock().unwrap().clone();
        let prompt = build_retry_prompt(
            pattern,
            &JS_ENV,
            previous_code,
            error_output,
            local_pkg.as_deref(),
            self.custom_instructions.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;
        let code = Self::extract_code_from_response(&response)?;

        debug!(
            "Retry generated {} bytes of JavaScript test code",
            code.len()
        );
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_agent::PatternCategory;

    fn sample_pattern() -> CodePattern {
        CodePattern {
            name: "Basic Server Setup".to_string(),
            description: "Create an Express server and listen on a port".to_string(),
            code: r#"const app = express();
app.get('/', (req, res) => {
    res.send('Hello World');
});
app.listen(3000);"#
                .to_string(),
            category: PatternCategory::BasicUsage,
        }
    }

    #[test]
    fn test_create_test_prompt_contains_pattern_info() {
        let pattern = sample_pattern();
        let prompt = JsCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("Basic Server Setup"));
        assert!(prompt.contains("Create an Express server"));
        assert!(prompt.contains("express()"));
        assert!(prompt.contains("Test passed: Basic Server Setup"));
    }

    #[test]
    fn test_create_test_prompt_js_env_notes() {
        let pattern = sample_pattern();
        let prompt = JsCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("node test.js"));
        assert!(prompt.contains("process.exit"));
    }

    #[test]
    fn test_create_test_prompt_is_minimal() {
        let pattern = sample_pattern();
        let prompt = JsCodeGenerator::create_test_prompt(&pattern, None, None);

        // Should NOT contain coaching — the SKILL.md is the test
        assert!(
            !prompt.contains("require("),
            "prompt should not coach on require"
        );
        assert!(
            !prompt.contains("ASSERTION RULES"),
            "prompt should not dictate assertion style"
        );
        assert!(
            !prompt.contains("COMPILATION RULES"),
            "prompt should not teach JS compilation"
        );
    }

    #[test]
    fn test_create_test_prompt_local_package() {
        let pattern = sample_pattern();
        let prompt = JsCodeGenerator::create_test_prompt(&pattern, None, Some("express"));

        assert!(prompt.contains("locally"));
        assert!(prompt.contains("express"));
    }

    #[test]
    fn test_create_test_prompt_no_local_package() {
        let pattern = sample_pattern();
        let prompt = JsCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(!prompt.contains("locally"));
    }

    #[test]
    fn test_create_test_prompt_with_custom_instructions() {
        let pattern = sample_pattern();
        let prompt = JsCodeGenerator::create_test_prompt(
            &pattern,
            Some("Always test error paths. Use async/await."),
            None,
        );

        assert!(prompt.contains("Always test error paths"));
    }

    #[test]
    fn test_create_test_prompt_with_both_options() {
        let pattern = sample_pattern();
        let prompt =
            JsCodeGenerator::create_test_prompt(&pattern, Some("Use ES modules"), Some("express"));

        assert!(prompt.contains("locally"));
        assert!(prompt.contains("Use ES modules"));
    }

    #[test]
    fn test_extract_code_from_javascript_block() {
        let response = r#"
Here's the test:

```javascript
const express = require('express');
const app = express();
console.log("✓ Test passed: Basic Server Setup");
```
"#;
        let code = JsCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("const express"));
        assert!(code.contains("console.log"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_js_block() {
        let response = r#"
```js
const app = require('express')();
app.listen(3000);
```
"#;
        let code = JsCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("const app"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_typescript_block() {
        let response = r#"
```typescript
import express from 'express';
const app: express.Application = express();
console.log("✓ Test passed: Basic");
```
"#;
        let code = JsCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("import express"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_ts_block() {
        let response = r#"
```ts
const x: number = 42;
console.log(x);
```
"#;
        let code = JsCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("const x: number"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_generic_block() {
        let response = r#"
```
const http = require('http');
http.createServer().listen(3000);
```
"#;
        let code = JsCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("const http"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_raw_code() {
        let response = r#"
const fs = require('fs');
console.log(fs.existsSync('.'));
"#;
        let code = JsCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("const fs"));
    }

    #[tokio::test]
    async fn test_generate_test_code_with_mock() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = JsCodeGenerator::new(&mock_client);

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
        let generator = JsCodeGenerator::new(&mock_client);
        generator.set_local_package(Some("express".to_string()));

        let pattern = sample_pattern();
        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_test_code_with_custom_instructions() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = JsCodeGenerator::new(&mock_client)
            .with_custom_instructions(Some("Use ES modules".to_string()));

        let pattern = sample_pattern();
        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_code_from_tilde_js_block() {
        let response = r#"
Here's the test:

~~~javascript
const express = require('express');
const app = express();
console.log("✓ Test passed: Tilde");
~~~
"#;
        let code = JsCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("const express"));
        assert!(code.contains("console.log"));
        assert!(!code.contains("~~~"));
    }

    #[test]
    fn test_extract_code_from_generic_tilde_block() {
        let response =
            "~~~\nconst http = require('http');\nhttp.createServer().listen(3000);\n~~~\n";
        let code = JsCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("const http"));
        assert!(!code.contains("~~~"));
    }
}
