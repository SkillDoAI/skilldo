//! Lightweight post-processing to ensure critical elements exist.
//! Only fixes what models consistently miss - tries not to rewrite everything.

use tracing::warn;

/// Create proper frontmatter (agentskills.io compliant)
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
        .map(|m| format!("\n  generated-by: skilldo/{}", m))
        .unwrap_or_default();

    format!(
        "---\nname: {}\ndescription: {ecosystem} library\n{}\nmetadata:\n  version: \"{}\"\n  ecosystem: {}{}\n---\n\n",
        package_name, license_field, version, ecosystem, generated_field
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
        // Scope field checks to frontmatter block (between first two --- delimiters)
        if let Some(end_pos) = after_start.find("---") {
            let fm_block = &after_start[..end_pos];
            let has_name = fm_block.contains("name:");
            let has_description = fm_block.contains("description:");
            let has_version = fm_block.contains("version:");
            let has_metadata = fm_block.contains("metadata:");

            // Accept both old format (top-level version/ecosystem) and new (inside metadata:)
            if !has_name || !has_description || (!has_version && !has_metadata) {
                warn!("Frontmatter has wrong format - replacing it");

                let content_after = &after_start[end_pos + 3..];
                return format!(
                    "{}{}",
                    create_frontmatter(package_name, version, ecosystem, license, generated_with),
                    content_after.trim_start()
                );
            }

            // Has correct fields — inject generated-by inside metadata if missing
            if let Some(model) = generated_with {
                if !fm_block.contains("generated-by:") && !fm_block.contains("generated_with:") {
                    let frontmatter = fm_block.trim_end();
                    let content_after = &after_start[end_pos + 3..];

                    if has_metadata {
                        // Append inside metadata block (metadata is last in our format)
                        return format!(
                            "---\n{}\n  generated-by: skilldo/{}\n---{}",
                            frontmatter, model, content_after
                        );
                    } else {
                        // No metadata block (old format) — add one
                        return format!(
                            "---\n{}\nmetadata:\n  generated-by: skilldo/{}\n---{}",
                            frontmatter, model, content_after
                        );
                    }
                }
            }
            return content.to_string();
        }
        // No closing --- found — broken frontmatter, fall through to add new one
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

/// Clean up frontmatter: remove blank lines, trim trailing whitespace on --- delimiters.
fn clean_frontmatter(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();

    // Find opening and closing --- positions
    let mut dash_positions = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "---" {
            dash_positions.push(i);
            if dash_positions.len() == 2 {
                break;
            }
        }
    }

    if dash_positions.len() < 2 {
        return content.to_string();
    }

    let open = dash_positions[0];
    let close = dash_positions[1];

    // Check if any cleaning is needed
    let needs_blank_removal = lines[open + 1..close].iter().any(|l| l.trim().is_empty());
    let needs_trim = lines[open].len() != 3 || lines[close].len() != 3;

    if !needs_blank_removal && !needs_trim {
        return content.to_string();
    }

    // Build result: before frontmatter + clean frontmatter + after frontmatter
    let mut result = Vec::new();

    // Lines before frontmatter (if any)
    for line in &lines[..open] {
        result.push(*line);
    }

    // Opening delimiter (clean)
    result.push("---");

    // Frontmatter body — skip blank lines
    for line in &lines[open + 1..close] {
        if !line.trim().is_empty() {
            result.push(line);
        }
    }

    // Closing delimiter (clean)
    result.push("---");

    // Rest of content
    for line in &lines[close + 1..] {
        result.push(line);
    }

    let mut out = result.join("\n");
    if content.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Strip metadata fields that leaked from frontmatter into the body content.
fn strip_leaked_metadata(content: &str) -> String {
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

    let metadata_patterns = ["generated-by: skilldo"];
    let mut result: Vec<&str> = Vec::with_capacity(lines.len());
    let mut stripped_any = false;

    for (i, line) in lines.iter().enumerate() {
        if i <= fm_end {
            result.push(line);
            continue;
        }
        let lower = line.to_lowercase();
        if metadata_patterns.iter().any(|p| lower.contains(p)) {
            warn!("Stripping leaked metadata from body: '{}'", line.trim());
            stripped_any = true;
            continue;
        }
        result.push(line);
    }

    if !stripped_any {
        return content.to_string();
    }

    let mut out = result.join("\n");
    if content.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Strip meta-text preamble that LLMs sometimes emit before the real content.
/// Removes ALL lines between frontmatter and the first markdown heading (## or #)
/// or code fence (```). This catches conversational preambles like:
/// - "Certainly! Here is your corrected SKILL.md."
/// - "**Corrections made:**" lists
/// - "Below is the generated SKILL.md file..."
fn strip_meta_text(content: &str) -> String {
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

    // Find the first "real content" line after frontmatter:
    // a heading (# or ##) or a code fence (```)
    let mut content_start = None;
    for (i, line) in lines[fm_end + 1..].iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || crate::util::is_fence_line(trimmed) {
            content_start = Some(fm_end + 1 + i);
            break;
        }
    }

    let content_start = match content_start {
        Some(i) => i,
        None => return content.to_string(),
    };

    // Check if there's any non-empty preamble to strip
    let has_preamble = lines[fm_end + 1..content_start]
        .iter()
        .any(|l| !l.trim().is_empty());

    if has_preamble {
        let stripped: Vec<&str> = lines[fm_end + 1..content_start]
            .iter()
            .filter(|l| !l.trim().is_empty())
            .copied()
            .collect();
        for line in &stripped {
            warn!("Stripping meta-text: '{}'", line.trim());
        }

        let mut result = lines[..=fm_end].join("\n");
        result.push('\n');
        result.push('\n');
        result.push_str(&lines[content_start..].join("\n"));
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

    // If 4+ dashes, there may be a duplicate frontmatter block.
    // Only consider duplicates BEFORE the first ## heading — after that, --- is a horizontal rule.
    // Verify the block between positions 2 and 3 actually looks like YAML frontmatter
    // (contains `key:` lines) — otherwise it's a horizontal rule, not a duplicate.
    if dash_positions.len() >= 4 {
        let second_start = dash_positions[2];

        // If a ## heading appears before the candidate duplicate, it's not frontmatter
        let first_heading = lines.iter().position(|l| l.trim_start().starts_with("## "));
        if let Some(h) = first_heading {
            if h < second_start {
                return content.to_string();
            }
        }
        let second_end = dash_positions[3];

        // Check for YAML-like `key: value` lines. Keys must be lowercase identifiers
        // (e.g. `name:`, `version:`) to avoid false positives on prose like "Note: something".
        let yaml_like_count = lines[second_start + 1..second_end]
            .iter()
            .filter(|l| {
                let trimmed = l.trim();
                if let Some(colon_pos) = trimmed.find(':') {
                    let key = trimmed[..colon_pos].trim();
                    !key.is_empty()
                        && key
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c == '_' || c == '-')
                } else {
                    false
                }
            })
            .count();
        let looks_like_frontmatter = yaml_like_count >= 2;

        if looks_like_frontmatter {
            warn!("Stripping duplicate frontmatter block");
            // Keep first frontmatter (positions 0 and 1), skip second (positions 2 and 3)
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
        .filter(|l| crate::util::is_fence_line(l))
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

    // 1.5. Clean frontmatter (remove blank lines, trim --- whitespace)
    normalized = clean_frontmatter(&normalized);

    // 2. Strip meta-text preamble (LLM framing text)
    normalized = strip_meta_text(&normalized);

    // 2.5. Strip leaked metadata from body
    normalized = strip_leaked_metadata(&normalized);

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
        assert!(result.contains("license: BSD"));
        assert!(result.contains("metadata:"));
        assert!(result.contains("  version: \"2.0.0\""));
        assert!(result.contains("  ecosystem: python"));
    }

    #[test]
    fn test_keep_existing_frontmatter_old_format() {
        // Old format (top-level version/ecosystem) should be kept as-is
        let content = "---\nname: torch\ndescription: python library\nversion: 2.0.0\necosystem: python\n---\n\n## Imports";
        let result = ensure_frontmatter(content, "torch", "2.0.0", "python", None, None);
        assert_eq!(result, content);
    }

    #[test]
    fn test_keep_existing_frontmatter_new_format() {
        // New format (metadata block) should be kept as-is
        let content = "---\nname: torch\ndescription: Deep learning framework for Python.\nlicense: BSD-3-Clause\nmetadata:\n  version: \"2.0.0\"\n  ecosystem: python\n---\n\n## Imports";
        let result = ensure_frontmatter(content, "torch", "2.0.0", "python", None, None);
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
    fn test_generated_by_in_new_frontmatter() {
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

        assert!(result.contains("  generated-by: skilldo/gpt-5.2"));
        assert!(result.contains("metadata:"));
    }

    #[test]
    fn test_generated_by_none_omitted() {
        let content = "## Imports\n\nSome content";
        let result = normalize_skill_md(content, "torch", "2.0.0", "python", None, &[], None);

        assert!(!result.contains("generated-by"));
    }

    #[test]
    fn test_strip_meta_text_below_is() {
        let content = "---\nname: click\ndescription: CLI creation toolkit for Python.\nlicense: MIT\nmetadata:\n  version: \"8.0.0\"\n  ecosystem: python\n---\n\nBelow is the generated SKILL.md file with exact sections as requested:\n\n## Imports\n```python\nimport click\n```\n";
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
        let content = "---\nname: test\ndescription: test library.\nlicense: MIT\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n---\n\nHere is the SKILL.md for the requests library:\n\n## Imports\n";
        let result = strip_meta_text(content);
        assert!(!result.contains("Here is the"));
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_strip_meta_text_preserves_clean_content() {
        let content = "---\nname: test\ndescription: test library.\nlicense: MIT\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n---\n\n## Imports\n```python\nimport test\n```\n";
        let result = strip_meta_text(content);
        assert_eq!(result, content, "Clean content should not be modified");
    }

    #[test]
    fn test_strip_duplicate_frontmatter() {
        let content = "---\nname: click\ndescription: CLI creation toolkit.\nlicense: BSD\nmetadata:\n  version: \"8.0.0\"\n  ecosystem: python\n  generated-by: skilldo/phi4\n---\n\nBelow is the generated SKILL.md:\n\n---\nname: click\ndescription: CLI library\nlicense: BSD\nmetadata:\n  version: \"8.0.0\"\n  ecosystem: python\n---\n\n## Imports\n```python\nimport click\n```\n";
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
    fn test_strip_duplicate_frontmatter_preserves_horizontal_rules() {
        // Horizontal rules (---) in body should NOT be treated as duplicate frontmatter
        let content = "---\nname: test\nversion: 1.0\necosystem: python\n---\n\n## Section 1\nSome content.\n\n---\n\n## Section 2\nMore content.\n\n---\n\nFinal section.\n";
        let result = strip_duplicate_frontmatter(content);
        assert_eq!(
            result, content,
            "Horizontal rules should not be stripped as duplicate frontmatter"
        );
    }

    #[test]
    fn test_strip_duplicate_frontmatter_preserves_prose_with_colons() {
        // Prose lines like "Note: something" should NOT trigger frontmatter detection
        let content = "---\nname: test\nversion: 1.0\necosystem: python\n---\n\n## Section\n\n---\n\nNote: this is important.\nWarning: do not skip this step.\n\n---\n\nMore content.\n";
        let result = strip_duplicate_frontmatter(content);
        assert_eq!(
            result, content,
            "Prose with colons should not be mistaken for frontmatter"
        );
    }

    #[test]
    fn test_strip_duplicate_frontmatter_no_duplicate() {
        let content = "---\nname: test\ndescription: test library.\nlicense: MIT\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n---\n\n## Imports\n";
        let result = strip_duplicate_frontmatter(content);
        assert_eq!(result, content, "No duplicate should mean no changes");
    }

    #[test]
    fn test_full_normalization_strips_meta_and_duplicate() {
        // Simulates the phi4 output pattern: preamble before frontmatter
        let raw_llm_output = "Below is the generated SKILL.md file with exact sections as requested:\n\n---\nname: click\ndescription: CLI creation toolkit for Python.\nlicense: BSD-3-Clause\nmetadata:\n  version: \"8.3.dev\"\n  ecosystem: python\n---\n\n## Imports\n```python\nimport click\n```\n";

        let result = normalize_skill_md(
            raw_llm_output,
            "click",
            "8.3.dev",
            "python",
            Some("BSD-3-Clause"),
            &[],
            Some("phi4-reasoning"),
        );

        // Should have generated-by inside metadata
        assert!(result.contains("  generated-by: skilldo/phi4-reasoning"));
        // Should NOT have meta-text
        assert!(!result.contains("Below is the"));
        // Should NOT have duplicate frontmatter
        let dash_count = result.lines().filter(|l| l.trim() == "---").count();
        assert_eq!(dash_count, 2, "Should have exactly one frontmatter block");
        // Should have real content
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_generated_by_hybrid_model() {
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

        assert!(result.contains("  generated-by: skilldo/qwen3-coder + gpt-5.2 (agent5)"));
    }

    #[test]
    fn test_strip_body_markdown_fence() {
        // Simulates the Sonnet 4.6 pattern: frontmatter then ```markdown wrapper
        let content = "---\nname: click\ndescription: CLI toolkit for Python.\nlicense: BSD-3-Clause\nmetadata:\n  version: \"8.3.1\"\n  ecosystem: python\n---\n\n```markdown\n## Imports\n\n```python\nimport click\n```\n\n## Core Patterns\nSome patterns\n```\n";
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
        let content = "---\nname: test\ndescription: test library.\nlicense: MIT\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n---\n\n## Imports\n```python\nimport test\n```\n";
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
            .filter(|l| crate::util::is_fence_line(l))
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
    fn test_strip_meta_text_certainly_preamble() {
        let content = "---\nname: pillow\ndescription: Image processing library for Python.\nlicense: MIT\nmetadata:\n  version: \"12.1.1\"\n  ecosystem: python\n---\n\nCertainly! Here is your corrected SKILL.md.\n\n## Imports\n```python\nfrom PIL import Image\n```\n";
        let result = strip_meta_text(content);
        assert!(
            !result.contains("Certainly!"),
            "Should strip 'Certainly!' preamble. Got: {}",
            result
        );
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_strip_meta_text_corrections_list() {
        let content = "---\nname: typer\ndescription: CLI framework for Python.\nlicense: MIT\nmetadata:\n  version: \"0.24.1\"\n  ecosystem: python\n---\n\nCertainly! Here is your corrected SKILL.md.\n\n**Corrections made:**\n- Fixed import order\n- Added missing examples\n\n## Imports\n```python\nimport typer\n```\n";
        let result = strip_meta_text(content);
        assert!(!result.contains("Certainly!"), "Should strip preamble");
        assert!(
            !result.contains("Corrections made"),
            "Should strip corrections list"
        );
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_strip_meta_text_then_markdown_fence() {
        // The pillow pattern: preamble + ```markdown fence
        let content = "---\nname: pillow\ndescription: Image processing library for Python.\nlicense: MIT\nmetadata:\n  version: \"12.1.1\"\n  ecosystem: python\n---\n\nCertainly! Here is your corrected SKILL.md.\n\n```markdown\n## Imports\n```python\nfrom PIL import Image\n```\n\n## Core Patterns\nPatterns\n```\n";
        // strip_meta_text should remove preamble, leaving fence as first line
        let after_meta = strip_meta_text(content);
        assert!(
            !after_meta.contains("Certainly!"),
            "Preamble should be stripped"
        );
        // Then strip_body_markdown_fence should catch the fence
        let after_fence = strip_body_markdown_fence(&after_meta);
        assert!(
            !after_fence.contains("```markdown"),
            "Fence should be stripped after preamble removal"
        );
        assert!(after_fence.contains("## Imports"));
    }

    #[test]
    fn test_full_normalization_strips_markdown_wrapper() {
        // The exact pattern from CI: frontmatter + ```markdown wrapper
        let raw = "---\nname: click\ndescription: CLI toolkit for Python.\nlicense: BSD-3-Clause\nmetadata:\n  version: \"8.3.1\"\n  ecosystem: python\n  generated-by: skilldo/claude-sonnet-4-6\n---\n\n```markdown\n## Imports\n\n```python\nimport click\n```\n\n## Core Patterns\nPatterns here\n\n## Pitfalls\nPitfalls here\n```\n";

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
            .filter(|l| crate::util::is_fence_line(l))
            .count();
        assert_eq!(
            fence_count % 2,
            0,
            "Should have even fence count after normalization"
        );
    }

    #[test]
    fn test_frontmatter_body_name_does_not_fool_check() {
        // Body contains "name:" in a code example but frontmatter is missing required fields
        let content = "---\ndescription: a library\n---\n\n## Core Patterns\n\n```python\nname: my_config\nvalue: 42\n```";
        let result = ensure_frontmatter(content, "mylib", "1.0.0", "python", None, None);

        // Frontmatter should be replaced (missing name/version/metadata in fm block)
        assert!(result.starts_with("---\n"));
        assert!(result.contains("name: mylib"));
        assert!(result.contains("metadata:"));
        assert!(result.contains("  version: \"1.0.0\""));
        assert!(result.contains("  ecosystem: python"));
    }

    #[test]
    fn test_frontmatter_scoped_to_fm_block() {
        // Frontmatter has all fields — body also has "name:" which should not cause replacement
        let content = "---\nname: mylib\ndescription: test library.\nlicense: MIT\nmetadata:\n  version: \"1.0.0\"\n  ecosystem: python\n---\n\nname: something_else\n";
        let result = ensure_frontmatter(content, "mylib", "1.0.0", "python", None, None);

        // Should keep existing frontmatter unchanged
        assert_eq!(result, content);
    }

    // --- Coverage gap tests below ---

    #[test]
    fn test_inject_generated_by_into_existing_metadata_block() {
        // Line 69: existing frontmatter with metadata block but no generated-by
        let content = "---\nname: click\ndescription: CLI toolkit.\nlicense: BSD\nmetadata:\n  version: \"8.0.0\"\n  ecosystem: python\n---\n\n## Imports\n";
        let result = ensure_frontmatter(
            content,
            "click",
            "8.0.0",
            "python",
            Some("BSD"),
            Some("gpt-5.2"),
        );

        assert!(
            result.contains("  generated-by: skilldo/gpt-5.2"),
            "Should inject generated-by into metadata block. Got: {}",
            result
        );
        // Should still have exactly one frontmatter block
        let dash_count = result.lines().filter(|l| l.trim() == "---").count();
        assert_eq!(dash_count, 2);
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_inject_generated_by_into_old_format_no_metadata() {
        // Line 76: existing frontmatter with old format (no metadata block), inject generated-by
        let content = "---\nname: torch\ndescription: Deep learning framework.\nversion: 2.0.0\necosystem: python\n---\n\n## Imports\n";
        let result = ensure_frontmatter(content, "torch", "2.0.0", "python", None, Some("phi4"));

        assert!(
            result.contains("metadata:\n  generated-by: skilldo/phi4"),
            "Should add metadata block with generated-by. Got: {}",
            result
        );
    }

    #[test]
    fn test_strip_meta_text_no_frontmatter() {
        // Line 148: strip_meta_text with no frontmatter (no --- delimiters)
        let content = "Some random content\n## Imports\nimport foo\n";
        let result = strip_meta_text(content);
        assert_eq!(result, content, "No frontmatter should mean no changes");
    }

    #[test]
    fn test_strip_meta_text_no_heading_after_frontmatter() {
        // Line 164: frontmatter exists but no heading or code fence follows
        let content = "---\nname: test\ndescription: test.\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n---\n\nJust some plain text with no headings.\nMore plain text.\n";
        let result = strip_meta_text(content);
        assert_eq!(
            result, content,
            "No heading after frontmatter should mean no changes"
        );
    }

    #[test]
    fn test_strip_meta_text_warns_for_each_preamble_line() {
        // Line 179: ensure the preamble stripping path runs (multiple non-empty preamble lines)
        let content = "---\nname: test\ndescription: test.\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n---\n\nLine one of preamble.\nLine two of preamble.\n\n## Imports\nimport foo\n";
        let result = strip_meta_text(content);

        assert!(
            !result.contains("Line one of preamble"),
            "Should strip first preamble line"
        );
        assert!(
            !result.contains("Line two of preamble"),
            "Should strip second preamble line"
        );
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_strip_duplicate_frontmatter_lines_without_colon() {
        // Line 234: the `false` branch for lines in the candidate block that have no colon.
        // Build a 4-dash structure where the inner block has lines without colons,
        // but still has >=2 yaml-like lines so it triggers duplicate detection.
        let content = "---\nname: test\ndescription: test.\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n---\n\n---\nname: test\ndescription: test.\nthis line has no colon\n---\n\n## Imports\n";
        let result = strip_duplicate_frontmatter(content);

        // The duplicate block has 2 yaml-like lines (name, description) + 1 without colon
        // so it should still be detected and stripped
        let dash_count = result.lines().filter(|l| l.trim() == "---").count();
        assert_eq!(
            dash_count, 2,
            "Should strip duplicate frontmatter even with non-colon lines. Got: {}",
            result
        );
        assert!(result.contains("## Imports"));
    }

    #[test]
    fn test_strip_body_markdown_fence_no_frontmatter() {
        // Line 281: strip_body_markdown_fence when content has no frontmatter
        let content = "```markdown\n## Imports\nimport foo\n```\n";
        let result = strip_body_markdown_fence(content);
        assert_eq!(
            result, content,
            "No frontmatter should mean no changes for body fence stripping"
        );
    }

    #[test]
    fn test_strip_body_markdown_fence_empty_body_after_frontmatter() {
        // Line 292: body is entirely empty/whitespace after frontmatter
        let content = "---\nname: test\ndescription: test.\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n---\n\n\n";
        let result = strip_body_markdown_fence(content);
        assert_eq!(
            result, content,
            "Empty body after frontmatter should mean no changes"
        );
    }

    #[test]
    fn test_strip_body_markdown_fence_last_line_not_closing_fence() {
        // Line 305: body starts with ```markdown but last non-empty line is NOT ```
        let content = "---\nname: test\ndescription: test.\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n---\n\n```markdown\n## Imports\nimport foo\nNo closing fence here\n";
        let result = strip_body_markdown_fence(content);
        assert_eq!(
            result, content,
            "Missing closing fence should mean no changes"
        );
    }

    #[test]
    fn test_fix_unclosed_code_blocks_no_trailing_newline() {
        // Line 336: unclosed code block where content does NOT end with newline
        let content = "## Imports\n```python\nimport click";
        assert!(
            !content.ends_with('\n'),
            "Precondition: no trailing newline"
        );

        let result = fix_unclosed_code_blocks(content);

        let fence_count = result
            .lines()
            .filter(|l| crate::util::is_fence_line(l))
            .count();
        assert_eq!(fence_count % 2, 0, "Should have even number of fences");
        assert!(
            result.ends_with("```\n"),
            "Should end with closing fence and newline"
        );
    }

    #[test]
    fn test_clean_frontmatter_removes_blank_lines() {
        let input = "---\nname: click\n\ndescription: CLI framework\n\nmetadata:\n  version: \"8.1.7\"\n  ecosystem: python\n---\n\n## Imports";
        let result = normalize_skill_md(input, "click", "8.1.7", "python", None, &[], None);
        // Extract frontmatter block
        let fm_start = result.find("---\n").unwrap();
        let fm_end = result[fm_start + 4..].find("\n---").unwrap();
        let frontmatter = &result[fm_start + 4..fm_start + 4 + fm_end];
        assert!(
            !frontmatter.contains("\n\n"),
            "Frontmatter should not contain blank lines, got:\n{}",
            frontmatter
        );
    }

    #[test]
    fn test_clean_frontmatter_trims_delimiter_whitespace() {
        let input = "---  \nname: click\ndescription: CLI framework\nmetadata:\n  version: \"8.1.7\"\n  ecosystem: python\n---  \n\n## Imports";
        let result = normalize_skill_md(input, "click", "8.1.7", "python", None, &[], None);
        // Check that --- delimiters are clean (no trailing spaces)
        for line in result.lines() {
            if line.starts_with("---") && line.trim() == "---" {
                assert_eq!(
                    line, "---",
                    "Delimiter should not have trailing whitespace: '{}'",
                    line
                );
            }
        }
    }

    #[test]
    fn test_strip_leaked_metadata() {
        let input = "---\nname: testify\ndescription: Go testing\nmetadata:\n  version: \"1.9.0\"\n  ecosystem: go\n  generated-by: skilldo/0.2.4\n---\n\n## Overview\n\nSome content\n\n| Feature | Status |\n| --- | --- |\n| generated-by: skilldo/0.2.4 | stable |\n\nMore content\n";
        let result = normalize_skill_md(input, "testify", "1.9.0", "go", None, &[], Some("0.2.4"));
        // The leaked metadata line in the table should be removed
        let after_fm = result.split("\n---\n").nth(1).unwrap();
        assert!(
            !after_fm.contains("generated-by: skilldo"),
            "Metadata should not leak into body:\n{}",
            after_fm
        );
    }

    #[test]
    fn test_strip_leaked_metadata_preserves_frontmatter() {
        let input = "---\nname: test\ndescription: test lib\nmetadata:\n  version: \"1.0\"\n  ecosystem: python\n  generated-by: skilldo/0.2.4\n---\n\n## Imports\n\nNormal content\n";
        let result = normalize_skill_md(input, "test", "1.0", "python", None, &[], Some("0.2.4"));
        // The generated-by in frontmatter should be preserved
        let parts: Vec<&str> = result.splitn(3, "---").collect();
        assert!(parts.len() >= 3);
        assert!(
            parts[1].contains("generated-by: skilldo/0.2.4"),
            "Frontmatter should keep generated-by"
        );
    }

    #[test]
    fn test_clean_frontmatter_no_change_when_clean() {
        let input = "---\nname: click\ndescription: CLI framework\nmetadata:\n  version: \"8.1.7\"\n  ecosystem: python\n---\n\n## Imports\nContent here\n";
        let result = normalize_skill_md(input, "click", "8.1.7", "python", None, &[], None);
        assert_eq!(result, input, "Clean frontmatter should not be modified");
    }
}
