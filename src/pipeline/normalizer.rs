//! Lightweight post-processing to ensure critical elements exist.
//! Only fixes what models consistently miss - tries not to rewrite everything.

use tracing::warn;

/// Create proper frontmatter
fn create_frontmatter(
    package_name: &str,
    version: &str,
    ecosystem: &str,
    license: Option<&str>,
    generated_with: Option<&str>,
) -> String {
    let license_field = license
        .map(|l| format!("license: {}", l))
        .unwrap_or_else(|| "# license: Unknown".to_string());

    let generated_field = generated_with
        .map(|m| format!("\ngenerated_with: {}", m))
        .unwrap_or_default();

    format!(
        "---\nname: {}\ndescription: {ecosystem} library\nversion: {}\necosystem: {}\n{}{}\n---\n\n",
        package_name, version, ecosystem, license_field, generated_field
    )
}

/// Ensure frontmatter exists (critical for version tracking)
pub fn ensure_frontmatter(
    content: &str,
    package_name: &str,
    version: &str,
    ecosystem: &str,
    license: Option<&str>,
    generated_with: Option<&str>,
) -> String {
    let trimmed = content.trim_start();

    // If frontmatter exists but has wrong format, replace it
    if let Some(after_start) = trimmed.strip_prefix("---") {
        // Check if it has all required fields (must match linter's required list)
        let has_name = trimmed.contains("name:");
        let has_description = trimmed.contains("description:");
        let has_version = trimmed.contains("version:");
        let has_ecosystem = trimmed.contains("ecosystem:");

        // If missing any required field, replace the frontmatter
        if !has_name || !has_description || !has_version || !has_ecosystem {
            warn!("Frontmatter has wrong format - replacing it");

            // Find end of existing frontmatter
            if let Some(end_pos) = after_start.find("---") {
                let content_after = &after_start[end_pos + 3..];
                return format!(
                    "{}{}",
                    create_frontmatter(package_name, version, ecosystem, license, generated_with),
                    content_after.trim_start()
                );
            }
        } else {
            // Has correct fields — inject generated_with if missing
            if let Some(model) = generated_with {
                if !trimmed.contains("generated_with:") {
                    if let Some(end_pos) = after_start.find("---") {
                        let frontmatter = &after_start[..end_pos].trim_end();
                        let content_after = &after_start[end_pos + 3..];
                        return format!(
                            "---\n{}\ngenerated_with: {}\n---{}",
                            frontmatter, model, content_after
                        );
                    }
                }
            }
            return content.to_string();
        }
    }

    warn!("Frontmatter missing - adding it");

    // Strip "# SKILL.md" header if present
    let content_clean = trimmed
        .strip_prefix("# SKILL.md")
        .unwrap_or(trimmed)
        .trim_start();

    format!(
        "{}{}",
        create_frontmatter(package_name, version, ecosystem, license, generated_with),
        content_clean
    )
}

/// Ensure References section exists if URLs provided
pub fn ensure_references(content: &str, project_urls: &[(String, String)]) -> String {
    if project_urls.is_empty() {
        return content.to_string();
    }

    // Check if References section already exists
    if content.contains("## References") {
        return content.to_string();
    }

    warn!("References section missing - adding it");

    // Build References section
    let mut refs = String::from("\n## References\n\n");
    for (name, url) in project_urls {
        refs.push_str(&format!("- [{}]({})\n", name, url));
    }

    format!("{}{}", content, refs)
}

/// Apply all normalizations (lightweight - only critical fixes)
pub fn normalize_skill_md(
    content: &str,
    package_name: &str,
    version: &str,
    ecosystem: &str,
    license: Option<&str>,
    project_urls: &[(String, String)],
    generated_with: Option<&str>,
) -> String {
    let mut normalized = content.to_string();

    // 1. Ensure frontmatter (critical)
    normalized = ensure_frontmatter(
        &normalized,
        package_name,
        version,
        ecosystem,
        license,
        generated_with,
    );

    // 2. Ensure References (if URLs exist)
    normalized = ensure_references(&normalized, project_urls);

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_missing_frontmatter() {
        let content = "## Imports\n\nSome content";
        let result = ensure_frontmatter(content, "torch", "2.0.0", "python", Some("BSD"), None);

        assert!(result.starts_with("---\n"));
        assert!(result.contains("name: torch"));
        assert!(result.contains("version: 2.0.0"));
        assert!(result.contains("license: BSD"));
    }

    #[test]
    fn test_keep_existing_frontmatter() {
        let content = "---\nname: torch\ndescription: python library\nversion: 2.0.0\necosystem: python\n---\n\n## Imports";
        let result = ensure_frontmatter(content, "torch", "2.0.0", "python", None, None);

        // Should not duplicate frontmatter (has all required fields)
        assert_eq!(result, content);
    }

    #[test]
    fn test_strip_skill_md_header() {
        let content = "# SKILL.md\n\n## Imports\n\nSome content";
        let result = ensure_frontmatter(content, "torch", "2.0.0", "python", None, None);

        // Should strip "# SKILL.md" header
        assert!(!result.contains("# SKILL.md"));
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_add_missing_references() {
        let content = "## Imports\n\nSome content";
        let urls = vec![
            ("Homepage".to_string(), "https://example.com".to_string()),
            ("Docs".to_string(), "https://docs.example.com".to_string()),
        ];

        let result = ensure_references(content, &urls);

        assert!(result.contains("## References"));
        assert!(result.contains("[Homepage](https://example.com)"));
        assert!(result.contains("[Docs](https://docs.example.com)"));
    }

    #[test]
    fn test_keep_existing_references() {
        let content = "## Imports\n\n## References\n\n- [Link](url)";
        let urls = vec![("Homepage".to_string(), "https://example.com".to_string())];

        let result = ensure_references(content, &urls);

        // Should not duplicate References section
        assert_eq!(result, content);
    }

    #[test]
    fn test_skip_references_if_no_urls() {
        let content = "## Imports\n\nSome content";
        let result = ensure_references(content, &[]);

        // Should not add empty References section
        assert!(!result.contains("## References"));
    }

    #[test]
    fn test_full_normalization() {
        let content = "# SKILL.md\n\n## Imports\n\nSome content";
        let urls = vec![("Homepage".to_string(), "https://pytorch.org".to_string())];

        let result = normalize_skill_md(
            content,
            "torch",
            "2.0.0",
            "python",
            Some("BSD-3-Clause"),
            &urls,
            None,
        );

        // Should have frontmatter
        assert!(result.starts_with("---\n"));
        assert!(result.contains("name: torch"));

        // Should NOT have extra header
        assert!(!result.contains("# SKILL.md"));

        // Should have References
        assert!(result.contains("## References"));
        assert!(result.contains("[Homepage](https://pytorch.org)"));
    }

    #[test]
    fn test_generated_with_in_new_frontmatter() {
        let content = "## Imports\n\nSome content";
        let result = normalize_skill_md(
            content,
            "torch",
            "2.0.0",
            "python",
            Some("BSD"),
            &[],
            Some("gpt-5.2"),
        );

        assert!(result.contains("generated_with: gpt-5.2"));
    }

    #[test]
    fn test_generated_with_none_omitted() {
        let content = "## Imports\n\nSome content";
        let result = normalize_skill_md(content, "torch", "2.0.0", "python", None, &[], None);

        assert!(!result.contains("generated_with"));
    }

    #[test]
    fn test_generated_with_hybrid_model() {
        let content = "## Imports\n\nSome content";
        let result = normalize_skill_md(
            content,
            "torch",
            "2.0.0",
            "python",
            None,
            &[],
            Some("qwen3-coder + gpt-5.2 (agent5)"),
        );

        assert!(result.contains("generated_with: qwen3-coder + gpt-5.2 (agent5)"));
    }
}
