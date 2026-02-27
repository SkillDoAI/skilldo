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

/// Strip meta-text preamble that LLMs sometimes emit before the real content.
/// e.g., "Below is the generated SKILL.md file with exact sections as requested:"
fn strip_meta_text(content: &str) -> String {
    let meta_patterns = [
        "below is the",
        "here is the",
        "here's the",
        "i've generated",
        "i have generated",
        "as requested",
        "with exact sections",
        "the following skill.md",
        "generated skill.md",
    ];

    let lines: Vec<&str> = content.lines().collect();
    let mut start_idx = 0;
    let mut frontmatter_dashes = 0;
    let mut found_meta = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed == "---" {
            frontmatter_dashes += 1;
            if frontmatter_dashes == 2 {
                // Just passed the frontmatter — check lines after it
                start_idx = i + 1;
                continue;
            }
            continue;
        }

        // Only check lines immediately after frontmatter (skip empties)
        if frontmatter_dashes >= 2 {
            if trimmed.is_empty() {
                start_idx = i + 1;
                continue;
            }

            let lower = trimmed.to_lowercase();
            if meta_patterns.iter().any(|p| lower.contains(p)) {
                warn!("Stripping meta-text: '{}'", trimmed);
                start_idx = i + 1;
                found_meta = true;
                continue;
            }

            // First real content line after frontmatter — stop checking
            break;
        }
    }

    // Only rebuild if we actually found meta-text to strip
    if found_meta {
        // Find where frontmatter ends (second ---)
        let mut fm_end = 0;
        let mut dashes = 0;
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == "---" {
                dashes += 1;
                if dashes == 2 {
                    fm_end = i;
                    break;
                }
            }
        }

        let mut result = lines[..=fm_end].join("\n");
        result.push('\n');
        // Skip empty lines right after meta-text
        let remaining = &lines[start_idx..];
        let content_start = remaining
            .iter()
            .position(|l| !l.trim().is_empty())
            .unwrap_or(0);
        result.push('\n');
        result.push_str(&remaining[content_start..].join("\n"));
        result.push('\n');
        return result;
    }

    content.to_string()
}

/// Strip duplicated frontmatter blocks that LLMs sometimes emit in the body
fn strip_duplicate_frontmatter(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();

    // Count --- lines
    let dash_positions: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim() == "---")
        .map(|(i, _)| i)
        .collect();

    // If 4+ dashes, there's a duplicate frontmatter block
    if dash_positions.len() >= 4 {
        warn!("Stripping duplicate frontmatter block");
        // Keep first frontmatter (positions 0 and 1), skip second (positions 2 and 3)
        let second_start = dash_positions[2];
        let second_end = dash_positions[3];

        let mut result: Vec<&str> = Vec::new();
        result.extend_from_slice(&lines[..second_start]);
        // Skip blank lines between meta-text and duplicate frontmatter
        let after = &lines[second_end + 1..];
        let content_start = after.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
        result.extend_from_slice(&after[content_start..]);

        let mut out = result.join("\n");
        out.push('\n');
        return out;
    }

    content.to_string()
}

/// Strip a wrapping ```markdown fence from the body (after frontmatter).
/// LLMs sometimes emit: ---\nfrontmatter\n---\n\n```markdown\n...content...\n```
/// The strip_markdown_fences in generator.rs only catches fences at the very start,
/// not after frontmatter.
fn strip_body_markdown_fence(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();

    // Find end of frontmatter
    let mut fm_end = None;
    let mut dashes = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "---" {
            dashes += 1;
            if dashes == 2 {
                fm_end = Some(i);
                break;
            }
        }
    }

    let fm_end = match fm_end {
        Some(i) => i,
        None => return content.to_string(),
    };

    // Find first non-empty line after frontmatter
    let body_start = lines[fm_end + 1..]
        .iter()
        .position(|l| !l.trim().is_empty())
        .map(|p| p + fm_end + 1);

    let body_start = match body_start {
        Some(i) => i,
        None => return content.to_string(),
    };

    // Check if body starts with ```markdown (or ```md)
    let first_body = lines[body_start].trim();
    if first_body != "```markdown" && first_body != "```md" {
        return content.to_string();
    }

    // Check if last non-empty line is a closing ```
    let last_nonempty = lines.iter().rposition(|l| !l.trim().is_empty());
    let last_nonempty = match last_nonempty {
        Some(i) if lines[i].trim() == "```" => i,
        _ => return content.to_string(),
    };

    warn!("Stripping wrapping ```markdown fence from body");

    // Rebuild: frontmatter + body without the wrapping fences
    let mut result: Vec<&str> = Vec::new();
    result.extend_from_slice(&lines[..=fm_end]);
    result.push(""); // blank line after frontmatter
    result.extend_from_slice(&lines[body_start + 1..last_nonempty]);

    let mut out = result.join("\n");
    out.push('\n');
    out
}

/// Fix unclosed code blocks by appending a closing fence.
/// LLMs sometimes emit an odd number of ``` fences due to truncation or nesting errors.
fn fix_unclosed_code_blocks(content: &str) -> String {
    let fence_count = content
        .lines()
        .filter(|l| l.trim_start().starts_with("```"))
        .count();

    if fence_count % 2 != 0 {
        warn!(
            "Fixing unclosed code block ({} fences, appending closing fence)",
            fence_count
        );
        let mut fixed = content.to_string();
        if !fixed.ends_with('\n') {
            fixed.push('\n');
        }
        fixed.push_str("```\n");
        return fixed;
    }

    content.to_string()
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

    // 2. Strip meta-text preamble (LLM framing text)
    normalized = strip_meta_text(&normalized);

    // 3. Strip duplicate frontmatter blocks
    normalized = strip_duplicate_frontmatter(&normalized);

    // 4. Strip wrapping ```markdown fence from body
    normalized = strip_body_markdown_fence(&normalized);

    // 5. Fix unclosed code blocks (safety net)
    normalized = fix_unclosed_code_blocks(&normalized);

    // 6. Ensure References (if URLs exist)
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
    fn test_strip_meta_text_below_is() {
        let content = "---\nname: click\ndescription: python library\nversion: 8.0.0\necosystem: python\nlicense: MIT\n---\n\nBelow is the generated SKILL.md file with exact sections as requested:\n\n## Imports\n```python\nimport click\n```\n";
        let result = strip_meta_text(content);
        assert!(
            !result.contains("Below is the"),
            "Should strip 'Below is the' meta-text. Got: {}",
            result
        );
        assert!(
            result.contains("## Imports"),
            "Should preserve real content"
        );
    }

    #[test]
    fn test_strip_meta_text_here_is() {
        let content = "---\nname: test\ndescription: test\nversion: 1.0\necosystem: python\n---\n\nHere is the SKILL.md for the requests library:\n\n## Imports\n";
        let result = strip_meta_text(content);
        assert!(!result.contains("Here is the"));
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_strip_meta_text_preserves_clean_content() {
        let content = "---\nname: test\ndescription: test\nversion: 1.0\necosystem: python\n---\n\n## Imports\n```python\nimport test\n```\n";
        let result = strip_meta_text(content);
        assert_eq!(result, content, "Clean content should not be modified");
    }

    #[test]
    fn test_strip_duplicate_frontmatter() {
        let content = "---\nname: click\ndescription: python library\nversion: 8.0.0\necosystem: python\nlicense: BSD\ngenerated_with: phi4\n---\n\nBelow is the generated SKILL.md:\n\n---\nname: click\ndescription: CLI library\nversion: 8.0.0\necosystem: python\nlicense: BSD\n---\n\n## Imports\n```python\nimport click\n```\n";
        let result = strip_duplicate_frontmatter(content);

        // Count --- lines — should be exactly 2 (one frontmatter block)
        let dash_count = result.lines().filter(|l| l.trim() == "---").count();
        assert_eq!(
            dash_count, 2,
            "Should have exactly 2 --- lines after stripping. Got: {}",
            result
        );
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_strip_duplicate_frontmatter_no_duplicate() {
        let content = "---\nname: test\ndescription: test\nversion: 1.0\necosystem: python\n---\n\n## Imports\n";
        let result = strip_duplicate_frontmatter(content);
        assert_eq!(result, content, "No duplicate should mean no changes");
    }

    #[test]
    fn test_full_normalization_strips_meta_and_duplicate() {
        // Simulates the phi4 output pattern exactly
        let raw_llm_output = "Below is the generated SKILL.md file with exact sections as requested:\n\n---\nname: click\ndescription: A Python package for CLI.\nversion: 8.3.dev\necosystem: python\nlicense: BSD-3-Clause\n---\n\n## Imports\n```python\nimport click\n```\n";

        let result = normalize_skill_md(
            raw_llm_output,
            "click",
            "8.3.dev",
            "python",
            Some("BSD-3-Clause"),
            &[],
            Some("phi4-reasoning"),
        );

        // Should have clean frontmatter with generated_with
        assert!(result.contains("generated_with: phi4-reasoning"));
        // Should NOT have meta-text
        assert!(!result.contains("Below is the"));
        // Should NOT have duplicate frontmatter
        let dash_count = result.lines().filter(|l| l.trim() == "---").count();
        assert_eq!(dash_count, 2, "Should have exactly one frontmatter block");
        // Should have real content
        assert!(result.contains("## Imports"));
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

    #[test]
    fn test_strip_body_markdown_fence() {
        // Simulates the Sonnet 4.6 pattern: frontmatter then ```markdown wrapper
        let content = "---\nname: click\ndescription: python library\nversion: 8.3.1\necosystem: python\nlicense: BSD-3-Clause\n---\n\n```markdown\n## Imports\n\n```python\nimport click\n```\n\n## Core Patterns\nSome patterns\n```\n";
        let result = strip_body_markdown_fence(content);

        assert!(
            !result.contains("```markdown"),
            "Should strip ```markdown wrapper"
        );
        assert!(
            result.contains("## Imports"),
            "Should preserve body content"
        );
        assert!(
            result.contains("name: click"),
            "Should preserve frontmatter"
        );
    }

    #[test]
    fn test_strip_body_markdown_fence_no_wrapper() {
        let content = "---\nname: test\ndescription: test\nversion: 1.0\necosystem: python\n---\n\n## Imports\n```python\nimport test\n```\n";
        let result = strip_body_markdown_fence(content);
        assert_eq!(result, content, "No wrapper should mean no changes");
    }

    #[test]
    fn test_fix_unclosed_code_blocks() {
        // Odd number of fences
        let content =
            "## Imports\n```python\nimport click\n```\n\n## Patterns\n```python\nclick.command()\n";
        let result = fix_unclosed_code_blocks(content);

        let fence_count = result
            .lines()
            .filter(|l| l.trim_start().starts_with("```"))
            .count();
        assert_eq!(fence_count % 2, 0, "Should have even number of fences");
    }

    #[test]
    fn test_fix_unclosed_code_blocks_already_closed() {
        let content = "```python\nimport click\n```\n\n```python\nclick.command()\n```\n";
        let result = fix_unclosed_code_blocks(content);
        assert_eq!(result, content, "Already closed should mean no changes");
    }

    #[test]
    fn test_full_normalization_strips_markdown_wrapper() {
        // The exact pattern from CI: frontmatter + ```markdown wrapper
        let raw = "---\nname: click\ndescription: python library\nversion: 8.3.1\necosystem: python\nlicense: BSD-3-Clause\ngenerated_with: claude-sonnet-4-6\n---\n\n```markdown\n## Imports\n\n```python\nimport click\n```\n\n## Core Patterns\nPatterns here\n\n## Pitfalls\nPitfalls here\n```\n";

        let result = normalize_skill_md(
            raw,
            "click",
            "8.3.1",
            "python",
            Some("BSD-3-Clause"),
            &[],
            Some("claude-sonnet-4-6"),
        );

        assert!(
            !result.contains("```markdown"),
            "Should strip ```markdown wrapper"
        );
        assert!(result.contains("## Imports"), "Should preserve content");

        // Fence count should be even
        let fence_count = result
            .lines()
            .filter(|l| l.trim_start().starts_with("```"))
            .count();
        assert_eq!(
            fence_count % 2,
            0,
            "Should have even fence count after normalization"
        );
    }
}
