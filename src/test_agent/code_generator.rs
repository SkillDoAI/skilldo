//! Shared test prompt builder for the test agent.
//!
//! The test agent simulates a real end-user: given a SKILL.md, can an LLM
//! write working code from it? The prompt is intentionally minimal and
//! language-agnostic — if the SKILL.md needs a clever prompt to produce
//! working code, the SKILL.md is the problem.
//!
//! Language-specific additions are limited to runner environment details
//! (1–2 lines) that the LLM cannot infer from the skill content.

use super::CodePattern;

/// Environment-specific snippet appended to the base prompt.
/// Keep this to genuine environment constraints only — not coding guidance.
pub struct TestEnv {
    /// Language name for code fences (e.g., "go", "python")
    pub lang_tag: &'static str,
    /// How the code will be executed (e.g., "`go run main.go`", "`uv run test.py`")
    pub runner: &'static str,
    /// 1–2 line environment-specific notes the LLM can't infer.
    /// Examples: PEP 723 metadata format, `package main` requirement.
    pub env_notes: &'static str,
}

/// Build the test prompt for any language.
///
/// Layering: base prompt → language env notes → user custom instructions.
pub fn build_test_prompt(
    pattern: &CodePattern,
    env: &TestEnv,
    local_package: Option<&str>,
    custom_instructions: Option<&str>,
) -> String {
    let mut prompt = format!(
        r#"You are an AI developer given a SKILL.md reference file for a library.

Your job: write a short, simple program that proves this pattern actually works.
Don't write anything complex — just enough to confidently show that this skill
reference produces correct, working code on the first try.

Pattern: {name}
Description: {desc}

Code from the SKILL.md:
```{lang}
{code}
```

Environment:
- This runs via {runner} in an isolated container with internet access but no TTY
- Third-party packages are pre-installed
- On success, print: ✓ Test passed: {name}
- On failure, let it crash with a clear error — do not catch or suppress errors
- Follow the SKILL.md exactly for imports and module paths — your training data may be outdated
{env_notes}"#,
        name = pattern.name,
        desc = pattern.description,
        lang = env.lang_tag,
        code = pattern.code,
        runner = env.runner,
        env_notes = env.env_notes,
    );

    if let Some(pkg) = local_package {
        prompt.push_str(&format!(
            "\n\nNote: The library \"{}\" is available locally, not from a package registry.\n",
            pkg
        ));
    }

    if let Some(custom) = custom_instructions {
        prompt.push_str(&format!("\n\n{}\n", custom));
    }

    prompt.push_str(&format!(
        "\n\n```{}\n[your complete, runnable program]\n```",
        env.lang_tag
    ));

    prompt
}

/// Build a retry prompt when the first attempt failed.
/// Shows the LLM its previous code and the error, asks for a fix.
pub fn build_retry_prompt(
    pattern: &CodePattern,
    env: &TestEnv,
    previous_code: &str,
    error_output: &str,
    local_package: Option<&str>,
    custom_instructions: Option<&str>,
) -> String {
    // Truncate error to avoid blowing context (respect UTF-8 boundaries)
    let truncated_error = if error_output.len() > 1500 {
        let mut end = 1500;
        while end > 0 && !error_output.is_char_boundary(end) {
            end -= 1;
        }
        &error_output[..end]
    } else {
        error_output
    };

    let mut prompt = format!(
        r#"Your test code for the pattern "{name}" failed. Fix it.

Your previous code:
```{lang}
{previous_code}
```

Error:
```
{error}
```

Write the complete fixed program. Same rules as before: keep it simple,
let it crash on failure, print "✓ Test passed: {name}" on success."#,
        name = pattern.name,
        lang = env.lang_tag,
        previous_code = previous_code,
        error = truncated_error,
    );

    if let Some(pkg) = local_package {
        prompt.push_str(&format!(
            "\n\nNote: The library \"{}\" is available locally, not from a package registry.\n",
            pkg
        ));
    }

    if let Some(custom) = custom_instructions {
        prompt.push_str(&format!("\n\n{}\n", custom));
    }

    prompt.push_str(&format!(
        "\n\n```{}\n[your complete, fixed program]\n```",
        env.lang_tag
    ));

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_agent::PatternCategory;

    const TEST_ENV: TestEnv = TestEnv {
        lang_tag: "python",
        runner: "`uv run test.py`",
        env_notes: "- Use PEP 723",
    };

    fn sample_pattern() -> CodePattern {
        CodePattern {
            name: "Basic Usage".to_string(),
            description: "A simple example".to_string(),
            code: "import foo\nfoo.bar()".to_string(),
            category: PatternCategory::BasicUsage,
        }
    }

    #[test]
    fn retry_prompt_includes_custom_instructions() {
        let prompt = build_retry_prompt(
            &sample_pattern(),
            &TEST_ENV,
            "old code",
            "ImportError: no module",
            None,
            Some("Always test edge cases"),
        );
        assert!(
            prompt.contains("Always test edge cases"),
            "retry prompt should include custom_instructions"
        );
    }

    #[test]
    fn retry_prompt_includes_local_package() {
        let prompt = build_retry_prompt(
            &sample_pattern(),
            &TEST_ENV,
            "old code",
            "ImportError: no module",
            Some("mylib"),
            None,
        );
        assert!(
            prompt.contains("locally"),
            "retry prompt should mention local availability"
        );
        assert!(
            prompt.contains("mylib"),
            "retry prompt should include the local package name"
        );
    }

    #[test]
    fn retry_prompt_includes_both_options() {
        let prompt = build_retry_prompt(
            &sample_pattern(),
            &TEST_ENV,
            "old code",
            "error output",
            Some("mylib"),
            Some("Use pytest"),
        );
        assert!(prompt.contains("locally"));
        assert!(prompt.contains("mylib"));
        assert!(prompt.contains("Use pytest"));
    }

    #[test]
    fn retry_prompt_without_extras() {
        let prompt = build_retry_prompt(
            &sample_pattern(),
            &TEST_ENV,
            "old code",
            "error output",
            None,
            None,
        );
        assert!(!prompt.contains("locally"));
        assert!(prompt.contains("Basic Usage"));
        assert!(prompt.contains("old code"));
        assert!(prompt.contains("error output"));
    }
}
