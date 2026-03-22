//! Java test code generator — uses the shared prompt builder with minimal
//! Java-specific environment notes. The prompt intentionally avoids coaching
//! the LLM on how to write Java — the SKILL.md should be sufficient.

use anyhow::Result;
use std::sync::Mutex;
use tracing::debug;

use super::code_generator::{build_retry_prompt, build_test_prompt, TestEnv};
use super::{CodePattern, LanguageCodeGenerator};
use crate::llm::client::LlmClient;

/// Java environment: runner command + 1–2 line notes the LLM can't infer.
pub const JAVA_ENV: TestEnv = TestEnv {
    lang_tag: "java",
    runner: "`javac -cp 'deps/*<SEP>.' Main.java && java -cp 'deps/*<SEP>.' Main`  (where `<SEP>` is `:` on Unix/macOS, `;` on Windows)",
    env_notes: "\
- Write a `public class Main` with `public static void main(String[] args)` — no JUnit runner
- Use `System.exit(1)` for assertion failures
- Classpath separator is OS-dependent: `:` on Unix/macOS, `;` on Windows",
};

/// Java code generator using LLM
pub struct JavaCodeGenerator<'a> {
    llm_client: &'a dyn LlmClient,
    custom_instructions: Option<String>,
    /// Package name to exclude from deps (for local-mount modes).
    local_package: Mutex<Option<String>>,
}

impl<'a> JavaCodeGenerator<'a> {
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
        build_test_prompt(pattern, &JAVA_ENV, local_package, custom_instructions)
    }

    /// Extract Java code from markdown code blocks using the shared fence parser.
    fn extract_code_from_response(response: &str) -> Result<String> {
        let blocks = crate::util::find_fenced_blocks(response);

        // Prefer java-tagged blocks (case-insensitive)
        for (tag, body) in &blocks {
            if tag.eq_ignore_ascii_case("java") {
                return Ok(body.clone());
            }
        }

        // Fall back to first untagged block
        for (tag, body) in &blocks {
            if tag.is_empty() {
                return Ok(body.clone());
            }
        }

        // Last resort: extract body from any tagged block (strips fences)
        if let Some((_, body)) = blocks.first() {
            return Ok(body.clone());
        }

        // No code block at all — use the response as-is
        Ok(response.trim().to_string())
    }
}

#[async_trait::async_trait]
impl<'a> LanguageCodeGenerator for JavaCodeGenerator<'a> {
    fn set_local_package(&self, package: Option<String>) {
        *super::lock_or_recover(&self.local_package) = package;
    }

    async fn generate_test_code(&self, pattern: &CodePattern) -> Result<String> {
        debug!("Generating Java test code for pattern: {}", pattern.name);

        let local_pkg = super::lock_or_recover(&self.local_package).clone();
        let prompt = Self::create_test_prompt(
            pattern,
            self.custom_instructions.as_deref(),
            local_pkg.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;

        let code = Self::extract_code_from_response(&response)?;

        debug!("Generated {} bytes of Java test code", code.len());
        debug!("Generated Java test code:\n{}", code);
        Ok(code)
    }

    async fn retry_test_code(
        &self,
        pattern: &CodePattern,
        previous_code: &str,
        error_output: &str,
    ) -> Result<String> {
        debug!(
            "Retrying Java test code for pattern: {} (fixing error)",
            pattern.name
        );

        let local_pkg = super::lock_or_recover(&self.local_package).clone();
        let prompt = build_retry_prompt(
            pattern,
            &JAVA_ENV,
            previous_code,
            error_output,
            local_pkg.as_deref(),
            self.custom_instructions.as_deref(),
        );
        let response = self.llm_client.complete(&prompt).await?;
        let code = Self::extract_code_from_response(&response)?;

        debug!("Retry generated {} bytes of Java test code", code.len());
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_agent::PatternCategory;

    fn sample_pattern() -> CodePattern {
        CodePattern {
            name: "Basic Serialization".to_string(),
            description: "Convert Java objects to JSON".to_string(),
            code: r#"Gson gson = new Gson();
String json = gson.toJson(new int[]{1, 2, 3});
System.out.println(json);"#
                .to_string(),
            category: PatternCategory::BasicUsage,
        }
    }

    #[test]
    fn test_create_test_prompt_contains_pattern_info() {
        let pattern = sample_pattern();
        let prompt = JavaCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("Basic Serialization"));
        assert!(prompt.contains("Convert Java objects"));
        assert!(prompt.contains("Gson"));
        assert!(prompt.contains("Test passed: Basic Serialization"));
    }

    #[test]
    fn test_create_test_prompt_java_env_notes() {
        let pattern = sample_pattern();
        let prompt = JavaCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(prompt.contains("public class Main"));
        assert!(prompt.contains("System.exit(1)"));
        assert!(prompt.contains("javac -cp 'deps/*<SEP>.' Main.java"));
    }

    #[test]
    fn test_create_test_prompt_local_package() {
        let pattern = sample_pattern();
        let prompt = JavaCodeGenerator::create_test_prompt(
            &pattern,
            None,
            Some("com.google.code.gson:gson"),
        );

        assert!(prompt.contains("locally"));
        assert!(prompt.contains("com.google.code.gson:gson"));
    }

    #[test]
    fn test_create_test_prompt_no_local_package() {
        let pattern = sample_pattern();
        let prompt = JavaCodeGenerator::create_test_prompt(&pattern, None, None);

        assert!(!prompt.contains("locally"));
    }

    #[test]
    fn test_create_test_prompt_with_custom_instructions() {
        let pattern = sample_pattern();
        let prompt =
            JavaCodeGenerator::create_test_prompt(&pattern, Some("Always test null inputs."), None);

        assert!(prompt.contains("Always test null inputs"));
    }

    #[test]
    fn test_extract_code_from_java_block() {
        let response = r#"
Here's the test:

```java
public class Main {
    public static void main(String[] args) {
        System.out.println("Test passed");
    }
}
```
"#;
        let code = JavaCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("public class Main"));
        assert!(code.contains("println"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_from_generic_block() {
        let response = r#"
```
public class Main {
    public static void main(String[] args) {}
}
```
"#;
        let code = JavaCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("public class Main"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_raw_code() {
        let response = r#"
public class Main {
    public static void main(String[] args) {}
}
"#;
        let code = JavaCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("public class Main"));
    }

    #[tokio::test]
    async fn test_generate_test_code_with_mock() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = JavaCodeGenerator::new(&mock_client);

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
        let generator = JavaCodeGenerator::new(&mock_client);
        generator.set_local_package(Some("com.google.code.gson:gson".to_string()));

        let pattern = sample_pattern();
        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_test_code_with_custom_instructions() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = JavaCodeGenerator::new(&mock_client)
            .with_custom_instructions(Some("Test edge cases".to_string()));

        let pattern = sample_pattern();
        let result = generator.generate_test_code(&pattern).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_code_from_tilde_java_block() {
        let response = r#"
~~~java
public class Main {
    public static void main(String[] args) {
        System.out.println("Test passed");
    }
}
~~~
"#;
        let code = JavaCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("public class Main"));
        assert!(!code.contains("~~~"));
    }

    #[test]
    fn test_extract_code_handles_bash_tagged_block() {
        // find_fenced_blocks parses the tag — bash tag on same line as fence
        let response = "```bash\necho hello\n```";
        let code = JavaCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(
            code.contains("echo hello"),
            "should extract code from bash-tagged block"
        );
    }

    #[test]
    fn test_extract_code_handles_shell_tagged_block() {
        let response = "```shell\nls -la\n```";
        let code = JavaCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("ls -la"));
    }

    #[test]
    fn test_extract_code_handles_xml_tagged_block() {
        let response = "```xml\n<root>data</root>\n```";
        let code = JavaCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("<root>data</root>"));
    }

    #[test]
    fn test_extract_code_preserves_unknown_first_line_in_generic_block() {
        // If the first line is not a known tag, it should be preserved
        let response = "```\nSomeClass obj = new SomeClass();\nobj.run();\n```";
        let code = JavaCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(
            code.contains("SomeClass"),
            "non-tag first line should be preserved"
        );
    }

    #[tokio::test]
    async fn test_retry_test_code_with_mock() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = JavaCodeGenerator::new(&mock_client);

        let pattern = sample_pattern();
        let result = generator
            .retry_test_code(&pattern, "old code", "compile error: missing semicolon")
            .await;
        assert!(result.is_ok());
        let code = result.unwrap();
        assert!(!code.is_empty());
    }

    #[tokio::test]
    async fn test_retry_test_code_with_local_package() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = JavaCodeGenerator::new(&mock_client);
        generator.set_local_package(Some("com.google.code.gson:gson".to_string()));

        let pattern = sample_pattern();
        let result = generator
            .retry_test_code(&pattern, "previous attempt", "NullPointerException")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_retry_test_code_with_custom_instructions() {
        use crate::llm::client::MockLlmClient;

        let mock_client = MockLlmClient::new();
        let generator = JavaCodeGenerator::new(&mock_client)
            .with_custom_instructions(Some("Always test null inputs.".to_string()));

        let pattern = sample_pattern();
        let result = generator
            .retry_test_code(&pattern, "prev code", "AssertionError")
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_code_from_tilde_generic_block() {
        let response =
            "~~~\npublic class Main {\n    public static void main(String[] args) {}\n}\n~~~";
        let code = JavaCodeGenerator::extract_code_from_response(response).unwrap();
        assert!(code.contains("public class Main"));
        assert!(!code.contains("~~~"));
    }

    poison_recovery_tests!(JavaCodeGenerator, sample_pattern);
}
