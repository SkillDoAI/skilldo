/// Returns the full sample config text for `skilldo config sample`.
pub fn sample_config_text() -> &'static str {
    "\
# Full documented sample config:
# https://github.com/SkillDoAI/skilldo/blob/main/docs/configuration.md
#
# Quick start — copy and customize:

[llm]
provider_type = \"anthropic\"
model = \"claude-sonnet-4-6\"
api_key_env = \"ANTHROPIC_API_KEY\"

[generation]
max_retries = 10
enable_test = true
enable_review = true
# security_context = \"api-client\"  # for API client SDKs
# redact_env_vars = [\"MY_API_KEY\"]  # mask secrets in CI logs
# custom_instructions = \"\"\"
# Repo-specific instructions here.
# \"\"\""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_config_text_contains_required_sections() {
        let text = sample_config_text();
        assert!(text.contains("[llm]"), "missing [llm] section");
        assert!(
            text.contains("[generation]"),
            "missing [generation] section"
        );
    }

    #[test]
    fn sample_config_text_contains_provider_defaults() {
        let text = sample_config_text();
        assert!(text.contains("provider_type = \"anthropic\""));
        assert!(text.contains("model = \"claude-sonnet-4-6\""));
        assert!(text.contains("api_key_env = \"ANTHROPIC_API_KEY\""));
    }

    #[test]
    fn sample_config_text_contains_generation_defaults() {
        let text = sample_config_text();
        assert!(text.contains("max_retries = 10"));
        assert!(text.contains("enable_test = true"));
        assert!(text.contains("enable_review = true"));
    }

    #[test]
    fn sample_config_text_contains_commented_options() {
        let text = sample_config_text();
        assert!(
            text.contains("# security_context"),
            "missing security_context comment"
        );
        assert!(
            text.contains("# redact_env_vars"),
            "missing redact_env_vars comment"
        );
        assert!(
            text.contains("# custom_instructions"),
            "missing custom_instructions comment"
        );
    }

    #[test]
    fn sample_config_text_contains_docs_link() {
        let text = sample_config_text();
        assert!(
            text.contains("https://github.com/SkillDoAI/skilldo/blob/main/docs/configuration.md")
        );
    }

    #[test]
    fn sample_config_text_is_not_empty() {
        assert!(!sample_config_text().is_empty());
    }
}
