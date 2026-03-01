use anyhow::Result;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct LintIssue {
    pub severity: Severity,
    pub category: String,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Severity {
    #[default]
    Error, // Must fix
    Warning, // Should fix
    Info,    // Nice to have
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

impl FromStr for Severity {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "error" => Ok(Severity::Error),
            "warning" => Ok(Severity::Warning),
            "info" => Ok(Severity::Info),
            _ => Err(()),
        }
    }
}

pub struct SkillLinter;

impl Default for SkillLinter {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillLinter {
    pub fn new() -> Self {
        Self
    }

    /// Token-aware check for root/home wipe commands.
    /// Matches "rm -rf /", "rm -fr ~", "rm -rfi /", "rm -r -f /", "rm -f -r /",
    /// "rm -r -v -f /", and variants with trailing semicolons/pipes.
    /// Does NOT match "rm -rf /tmp" or "rm -rf ~/proj".
    fn is_root_wipe_command(text: &str) -> bool {
        // Strip trailing semicolons and pipes so "rm -rf /;" is caught.
        let cleaned = text.replace([';', '|'], " ");
        let tokens: Vec<&str> = cleaned.split_whitespace().collect();

        // Check every "rm" occurrence — "rm -rf /tmp; rm -rf /" has two.
        for rm_pos in tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| **t == "rm")
            .map(|(i, _)| i)
        {
            let mut has_recursive = false;
            let mut has_force = false;
            let mut target = None;

            for token in &tokens[rm_pos + 1..] {
                if token.starts_with('-') {
                    if token.contains('r') {
                        has_recursive = true;
                    }
                    if token.contains('f') {
                        has_force = true;
                    }
                } else {
                    target = Some(*token);
                    break;
                }
            }

            // Only match root (/) or home (~, ~/) — NOT subdirectories like /tmp or ~/proj
            if let Some(path) = target {
                let stripped = path.trim_matches(|c| c == '"' || c == '\'');
                let is_root_or_home = stripped == "/" || stripped == "~" || stripped == "~/";
                if has_recursive && has_force && is_root_or_home {
                    return true;
                }
            }
        }

        false
    }

    /// Lint a SKILL.md file content
    pub fn lint(&self, content: &str) -> Result<Vec<LintIssue>> {
        let mut issues = Vec::new();

        // Check frontmatter
        issues.extend(self.check_frontmatter(content));

        // Check structure
        issues.extend(self.check_structure(content));

        // Check content quality
        issues.extend(self.check_content(content));

        // Check for LLM degeneration
        issues.extend(self.check_degeneration(content));

        // Check for security threats
        issues.extend(self.check_security(content));

        Ok(issues)
    }

    fn check_frontmatter(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();

        // Parse frontmatter
        let frontmatter = self.extract_frontmatter(content);

        if frontmatter.is_empty() {
            issues.push(LintIssue {
                severity: Severity::Error,
                category: "frontmatter".to_string(),
                message: "Missing frontmatter (---...---)".to_string(),
                suggestion: Some(
                    "Add frontmatter with name, description, version, ecosystem".to_string(),
                ),
            });
            return issues;
        }

        // Required fields
        let required_fields = vec!["name", "description", "version", "ecosystem"];
        for field in required_fields {
            if !frontmatter.contains_key(field) {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "frontmatter".to_string(),
                    message: format!("Missing required field: {}", field),
                    suggestion: Some(format!("Add '{}: <value>' to frontmatter", field)),
                });
            }
        }

        // Check for problematic versions
        if let Some(version) = frontmatter.get("version") {
            if version == "unknown" {
                issues.push(LintIssue {
                    severity: Severity::Warning,
                    category: "frontmatter".to_string(),
                    message: "Version is 'unknown' — version extraction failed".to_string(),
                    suggestion: Some(
                        "Try --version-from git-tag or --version <version> to set explicitly"
                            .to_string(),
                    ),
                });
            } else if version == "source" {
                issues.push(LintIssue {
                    severity: Severity::Warning,
                    category: "frontmatter".to_string(),
                    message: "Version is 'source' — version extraction failed".to_string(),
                    suggestion: Some(
                        "Use --version <version> to set the version explicitly".to_string(),
                    ),
                });
            } else if !version.starts_with(|c: char| c.is_ascii_digit()) {
                issues.push(LintIssue {
                    severity: Severity::Warning,
                    category: "frontmatter".to_string(),
                    message: format!(
                        "Version '{}' doesn't look like a semver version",
                        version
                    ),
                    suggestion: Some(
                        "Version should start with a digit (e.g., '1.0.0'). Use --version to set explicitly."
                            .to_string(),
                    ),
                });
            }
        }

        // Check for license field (tessl.io requirement)
        if !frontmatter.contains_key("license") {
            issues.push(LintIssue {
                severity: Severity::Warning,
                category: "frontmatter".to_string(),
                message: "'license' field is missing".to_string(),
                suggestion: Some(
                    "Add 'license: MIT' (or appropriate license) to frontmatter".to_string(),
                ),
            });
        }

        issues
    }

    fn check_structure(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();

        // Required sections
        let required_sections = vec!["## Imports", "## Core Patterns", "## Pitfalls"];

        for section in required_sections {
            if !content.contains(section) {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "structure".to_string(),
                    message: format!("Missing required section: {}", section),
                    suggestion: Some(format!("Add a '{}' section", section)),
                });
            }
        }

        issues
    }

    fn check_content(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();

        // Check for code blocks
        if !content.contains("```") {
            issues.push(LintIssue {
                severity: Severity::Error,
                category: "content".to_string(),
                message: "No code examples found".to_string(),
                suggestion: Some("Add code examples in ```python blocks".to_string()),
            });
        }

        // Check for body wrapped in ```markdown fence (normalizer should have stripped this)
        {
            let mut fm_dashes = 0;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed == "---" {
                    fm_dashes += 1;
                    continue;
                }
                if fm_dashes >= 2 && !trimmed.is_empty() {
                    if trimmed == "```markdown" || trimmed == "```md" {
                        issues.push(LintIssue {
                            severity: Severity::Error,
                            category: "content".to_string(),
                            message: "Body is wrapped in a ```markdown fence".to_string(),
                            suggestion: Some(
                                "Remove the wrapping ```markdown fence. The normalizer should have caught this."
                                    .to_string(),
                            ),
                        });
                    }
                    break;
                }
            }
        }

        // Check minimum length
        if content.len() < 1000 {
            issues.push(LintIssue {
                severity: Severity::Warning,
                category: "content".to_string(),
                message: format!("Content is very short ({} chars)", content.len()),
                suggestion: Some("Consider adding more examples and explanations".to_string()),
            });
        }

        // Check for private/internal modules in Imports section
        if let Some(imports_start) = content.find("## Imports") {
            let imports_section = &content[imports_start..];
            let imports_end = imports_section[10..] // Skip "## Imports"
                .find("\n## ")
                .map(|pos| pos + 10)
                .unwrap_or(imports_section.len());
            let imports_content = &imports_section[..imports_end];

            for line in imports_content.lines() {
                let trimmed = line.trim();
                // Match any underscore-prefixed module segment (e.g. pkg._compat, from pkg import _internal)
                let mut candidates: Vec<&str> = Vec::new();
                if let Some(rest) = trimmed.strip_prefix("from ") {
                    if let Some((module, imported)) = rest.split_once(" import ") {
                        candidates.push(module.trim());
                        for sym in imported.split(',') {
                            if let Some(name) = sym.split_whitespace().next() {
                                candidates.push(name);
                            }
                        }
                    } else if let Some(module) = rest.split_whitespace().next() {
                        candidates.push(module);
                    }
                } else if let Some(rest) = trimmed.strip_prefix("import ") {
                    for module in rest.split(',') {
                        if let Some(name) = module.split_whitespace().next() {
                            candidates.push(name);
                        }
                    }
                }
                let has_private_segment = candidates.iter().any(|path| {
                    path.split('.')
                        .any(|seg| seg.starts_with('_') && seg != "__future__" && seg != "__init__")
                });
                if has_private_segment {
                    issues.push(LintIssue {
                        severity: Severity::Warning,
                        category: "content".to_string(),
                        message: format!(
                            "Private/internal module in Imports: '{}'",
                            trimmed
                        ),
                        suggestion: Some(
                            "Only public API imports belong in the ## Imports section. Use the public API equivalent."
                                .to_string(),
                        ),
                    });
                    break; // One warning is enough
                }
            }
        }

        // Check for Pitfalls with Wrong/Right examples
        if content.contains("## Pitfalls") {
            let has_wrong = content.contains("### Wrong") || content.contains("### ❌");
            let has_right = content.contains("### Right") || content.contains("### ✅");

            if !has_wrong || !has_right {
                issues.push(LintIssue {
                    severity: Severity::Info,
                    category: "content".to_string(),
                    message: "Pitfalls section should include 'Wrong' and 'Right' examples"
                        .to_string(),
                    suggestion: Some(
                        "Use ### Wrong: and ### Right: subsections in Pitfalls".to_string(),
                    ),
                });
            }

            // Check for duplicate Wrong/Right examples
            issues.extend(self.check_duplicate_examples(content));
        }

        issues
    }

    fn check_duplicate_examples(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();

        // Extract code blocks from Pitfalls section
        if let Some(pitfalls_start) = content.find("## Pitfalls") {
            let pitfalls_section = &content[pitfalls_start..];

            // Find the next section (or end of document)
            let pitfalls_end = pitfalls_section[13..] // Skip "## Pitfalls"
                .find("\n## ")
                .map(|pos| pos + 13)
                .unwrap_or(pitfalls_section.len());

            let pitfalls_content = &pitfalls_section[..pitfalls_end];

            // Extract all code blocks
            let code_blocks: Vec<&str> = pitfalls_content
                .split("```")
                .skip(1) // Skip text before first block
                .step_by(2) // Take every other element (the code blocks)
                .map(|block| {
                    // Remove language identifier (e.g., "python\n")
                    block
                        .trim_start_matches(|c: char| c.is_alphanumeric() || c == '-')
                        .trim()
                })
                .collect();

            // Check for consecutive identical code blocks (Wrong vs Right pattern)
            for i in 0..code_blocks.len().saturating_sub(1) {
                if code_blocks[i] == code_blocks[i + 1] && !code_blocks[i].is_empty() {
                    issues.push(LintIssue {
                        severity: Severity::Error,
                        category: "content".to_string(),
                        message: "Found identical 'Wrong' and 'Right' examples in Pitfalls section".to_string(),
                        suggestion: Some("Ensure 'Wrong' and 'Right' examples show different code - the examples should demonstrate what NOT to do vs what TO do".to_string()),
                    });
                    break; // Only report once
                }
            }
        }

        issues
    }

    fn check_degeneration(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Track code block boundaries
        let mut in_code_block = false;
        let mut code_block_lines: Vec<bool> = Vec::with_capacity(lines.len());
        for line in &lines {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
            }
            code_block_lines.push(in_code_block);
        }

        // Check 1: Repeated line prefix (catches multi-line degeneration)
        // If 10+ consecutive non-code lines share a prefix of >= 20 chars, flag it.
        let min_prefix = 20;
        let min_run = 10;
        let mut i = 0;
        while i < lines.len() {
            if code_block_lines[i] || lines[i].len() < min_prefix {
                i += 1;
                continue;
            }
            // Find a safe char boundary at or before min_prefix to avoid splitting multi-byte chars
            let mut end = min_prefix;
            while end > 0 && !lines[i].is_char_boundary(end) {
                end -= 1;
            }
            let prefix = &lines[i][..end];
            let mut run = 1;
            while i + run < lines.len()
                && !code_block_lines[i + run]
                && lines[i + run].starts_with(prefix)
            {
                run += 1;
            }
            if run >= min_run {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "degeneration".to_string(),
                    message: format!(
                        "Repetitive content: {} consecutive lines share prefix '{}'",
                        run, prefix
                    ),
                    suggestion: Some(
                        "LLM output degenerated into repetitive patterns. Regenerate this section."
                            .to_string(),
                    ),
                });
                break;
            }
            i += run;
        }

        // Check 2: Gibberish token (catches nonsense identifiers)
        // Any word outside code blocks longer than 80 chars is almost certainly not real.
        for (idx, line) in lines.iter().enumerate() {
            if code_block_lines[idx] {
                continue;
            }
            for word in line.split_whitespace() {
                let clean = word.trim_matches(|c: char| {
                    c == '*' || c == '`' || c == '_' || c == ',' || c == '-'
                });
                // Skip dotted identifiers (e.g., cryptography.hazmat.primitives.twofactor.hotp.HOTP)
                // These are valid fully-qualified Python/Java/etc. module paths, not gibberish.
                // Each segment must be reasonably short (<=40 chars) and there must be 2+ segments.
                let is_dotted_identifier = clean.contains('.') && {
                    let parts: Vec<&str> = clean.split('.').collect();
                    parts.len() >= 2
                        && parts.iter().all(|part| {
                            !part.is_empty()
                                && part.len() <= 40
                                && part.chars().all(|c| c.is_alphanumeric() || c == '_')
                        })
                };

                if clean.len() > 80 && !is_dotted_identifier {
                    // Find a safe char boundary for the preview snippet
                    let mut preview_end = 40;
                    while preview_end > 0 && !clean.is_char_boundary(preview_end) {
                        preview_end -= 1;
                    }
                    issues.push(LintIssue {
                        severity: Severity::Error,
                        category: "degeneration".to_string(),
                        message: format!(
                            "Nonsense token detected ({} chars): '{}...'",
                            clean.len(),
                            &clean[..preview_end]
                        ),
                        suggestion: Some(
                            "LLM output contains gibberish. Regenerate this section.".to_string(),
                        ),
                    });
                    return issues; // One is enough
                }
            }
        }

        // Check 3: Prompt instruction leak
        // Known phrases from our prompts that should never appear in the output.
        let prompt_leaks = [
            "CRITICAL: Include ALL",
            "CRITICAL: Prioritize PUBLIC APIs",
            "CRITICAL: Mark deprecation status",
            "CRITICAL: This section is MANDATORY",
            "do NOT skip this section",
            "REQUIRED sections:",
            "Focus on the 10-15",
            "Output as JSON",
            "Your job is to",
            "Show the standard import patterns",
            "Add 2 more pitfalls if found",
            "minimum 3, maximum 5 total",
        ];
        let mut in_code = false;
        for line in &lines {
            if line.trim_start().starts_with("```") {
                in_code = !in_code;
                continue;
            }
            if in_code {
                continue;
            }
            for phrase in &prompt_leaks {
                if line.contains(phrase) {
                    issues.push(LintIssue {
                        severity: Severity::Warning,
                        category: "degeneration".to_string(),
                        message: format!("Prompt instruction leak: '{}'", phrase),
                        suggestion: Some(
                            "LLM regurgitated prompt instructions into the output. Regenerate this section."
                                .to_string(),
                        ),
                    });
                    break;
                }
            }
        }

        // Check 3b: Meta-text / framing leak
        // LLMs sometimes prefix output with framing text like "Here is the SKILL.md..."
        // These should be stripped by the normalizer, but catch them here as a safety net.
        let meta_text_patterns = [
            "below is the",
            "here is the",
            "here is your",
            "here's the",
            "certainly!",
            "corrections made",
            "i've generated",
            "i have generated",
            "as requested",
            "with exact sections",
            "the following skill.md",
            "generated skill.md",
            "this file now has",
            "format issues fixed",
        ];
        // Only check the first 5 non-empty lines after frontmatter (meta-text is always at the top)
        let mut checked = 0;
        let mut frontmatter_dashes = 0;
        for line in &lines {
            let trimmed = line.trim();
            if trimmed == "---" {
                frontmatter_dashes += 1;
                continue;
            }
            // Skip lines inside frontmatter (between first and second ---)
            if frontmatter_dashes < 2 {
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            checked += 1;
            if checked > 5 {
                break;
            }
            let lower = trimmed.to_lowercase();
            for pattern in &meta_text_patterns {
                if lower.contains(pattern) {
                    issues.push(LintIssue {
                        severity: Severity::Warning,
                        category: "degeneration".to_string(),
                        message: format!("Meta-text leak in output: '{}'", trimmed),
                        suggestion: Some(
                            "LLM included framing text before the actual content. Remove this line."
                                .to_string(),
                        ),
                    });
                    break;
                }
            }
        }

        // Check 3c: Duplicated frontmatter
        // LLMs sometimes emit the frontmatter twice (normalizer adds one, LLM included one in body)
        // Only check the first 50 lines to avoid false positives from Markdown horizontal rules (---)
        let dash_lines: Vec<usize> = lines
            .iter()
            .enumerate()
            .take(50)
            .filter(|(_, l)| l.trim() == "---")
            .map(|(i, _)| i)
            .collect();
        if dash_lines.len() >= 4 {
            // More than 2 frontmatter delimiters near the top = duplicated frontmatter
            issues.push(LintIssue {
                severity: Severity::Warning,
                category: "degeneration".to_string(),
                message: format!(
                    "Duplicated frontmatter ({} delimiter lines found in first 50 lines, expected 2)",
                    dash_lines.len()
                ),
                suggestion: Some(
                    "LLM output contains duplicate YAML frontmatter. Remove the extra block."
                        .to_string(),
                ),
            });
        }

        // Check 4: Unclosed code blocks (truncated output)
        let fence_count = lines
            .iter()
            .filter(|l| l.trim_start().starts_with("```"))
            .count();
        if fence_count % 2 != 0 {
            issues.push(LintIssue {
                severity: Severity::Error,
                category: "degeneration".to_string(),
                message: format!(
                    "Unclosed code block ({} fences, expected even number)",
                    fence_count
                ),
                suggestion: Some(
                    "Output was likely truncated by token limit. Regenerate with higher max_tokens."
                        .to_string(),
                ),
            });
        }

        // Check 5: Excessively long line outside code blocks
        // Real markdown doesn't have 1000+ char lines. Degeneration does.
        for (idx, line) in lines.iter().enumerate() {
            if code_block_lines[idx] {
                continue;
            }
            if line.len() > 1000 {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "degeneration".to_string(),
                    message: format!(
                        "Excessively long line detected ({} chars)",
                        line.len()
                    ),
                    suggestion: Some(
                        "Lines over 1000 chars outside code blocks suggest LLM degeneration. Regenerate this section."
                            .to_string(),
                    ),
                });
                break;
            }
        }

        issues
    }

    // NOTE: Some patterns intentionally appear in multiple arrays below
    // (e.g. critical_code_substrs, destructive_patterns, injection_patterns,
    // threat_patterns). Each array serves a distinct detection purpose --
    // code-block scanning vs. prose scanning vs. HTML-comment scanning --
    // and may trigger different severity levels or messages depending on
    // context. Do not deduplicate across arrays.
    fn check_security(&self, content: &str) -> Vec<LintIssue> {
        let mut issues = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Track code block boundaries — security checks only apply to prose
        let mut in_code_block = false;
        let mut code_block_lines: Vec<bool> = Vec::with_capacity(lines.len());
        for line in &lines {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
            }
            code_block_lines.push(in_code_block);
        }

        // Check inside HTML comments — single-line and multi-line
        let mut in_html_comment = false;
        let mut html_comment_buf = String::new();
        for (idx, line) in lines.iter().enumerate() {
            if code_block_lines[idx] {
                continue;
            }
            // Single-line comment: <!-- ... -->
            if line.contains("<!--") && line.contains("-->") {
                if let Some(start) = line.find("<!--") {
                    // Search for --> only after the <!--, avoiding panic if --> appears earlier
                    if let Some(rel_end) = line[start + 4..].find("-->") {
                        let comment = &line[start + 4..start + 4 + rel_end];
                        if !comment.is_empty() && self.has_security_threat(comment) {
                            issues.push(LintIssue {
                                severity: Severity::Error,
                                category: "security".to_string(),
                                message: "Hidden instructions in HTML comment".to_string(),
                                suggestion: Some(
                                    "HTML comments may contain hidden instructions for AI agents. Review and remove."
                                        .to_string(),
                                ),
                            });
                        }
                    }
                }
            } else if line.contains("<!--") {
                // Start of multi-line comment
                in_html_comment = true;
                if let Some(start) = line.find("<!--") {
                    html_comment_buf = line[start + 4..].to_string();
                }
            } else if in_html_comment && line.contains("-->") {
                // End of multi-line comment
                if let Some(end) = line.find("-->") {
                    html_comment_buf.push(' ');
                    html_comment_buf.push_str(&line[..end]);
                }
                if self.has_security_threat(&html_comment_buf) {
                    issues.push(LintIssue {
                        severity: Severity::Error,
                        category: "security".to_string(),
                        message: "Hidden instructions in HTML comment".to_string(),
                        suggestion: Some(
                            "HTML comments may contain hidden instructions for AI agents. Review and remove."
                                .to_string(),
                        ),
                    });
                }
                in_html_comment = false;
                html_comment_buf.clear();
            } else if in_html_comment {
                // Middle of multi-line comment
                html_comment_buf.push(' ');
                html_comment_buf.push_str(line);
            }
        }

        // Scan code blocks for the most dangerous patterns.
        // Most code block content is legitimate, but some things have NO
        // reason to be in a SKILL.md code example.
        // Destructive shell commands need token-aware matching to avoid
        // false positives like "rm -rf /tmp/build".
        let critical_code_substrs = [
            "> /dev/sda",
            "> /dev/nvme",
            "/dev/tcp/",
            "/dev/udp/",
            "ignore all previous instructions",
            "ignore previous instructions",
            "you are now a",
            "disregard your",
            "override your",
        ];
        for (idx, line) in lines.iter().enumerate() {
            if !code_block_lines[idx] {
                continue; // Only check inside code blocks
            }
            let lower = line.to_lowercase();

            // Token-aware check for rm -rf / and rm -rf ~ (not /tmp, /var, etc.)
            if Self::is_root_wipe_command(&lower) {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "security".to_string(),
                    message: "Critical security pattern in code block: root/home wipe command"
                        .to_string(),
                    suggestion: Some(
                        "This pattern has no legitimate reason to appear in a SKILL.md code example."
                            .to_string(),
                    ),
                });
                continue;
            }

            for pattern in &critical_code_substrs {
                if lower.contains(pattern) {
                    issues.push(LintIssue {
                        severity: Severity::Error,
                        category: "security".to_string(),
                        message: format!(
                            "Critical security pattern in code block: '{}'",
                            pattern
                        ),
                        suggestion: Some(
                            "This pattern has no legitimate reason to appear in a SKILL.md code example."
                                .to_string(),
                        ),
                    });
                    break;
                }
            }
        }

        // Check prose lines for security threats
        for (idx, line) in lines.iter().enumerate() {
            if code_block_lines[idx] {
                continue;
            }

            let lower = line.to_lowercase();

            // 1. Destructive commands — filesystem/disk destruction
            // Token-aware check for rm -rf / and rm -rf ~ (not /tmp, /var, etc.)
            if Self::is_root_wipe_command(&lower) {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "security".to_string(),
                    message: "Destructive command in prose: root/home wipe command".to_string(),
                    suggestion: Some(
                        "SKILL.md should not instruct agents to run destructive commands."
                            .to_string(),
                    ),
                });
                continue;
            }
            let destructive_patterns = [
                "rm -rf .",
                "rmdir /s /q",
                "del /f /s /q",
                "format c:",
                "mkfs.",
                "dd if=",
                ":(){ :|:&};:", // fork bomb
                "> /dev/sda",
                "> /dev/nvme",
                "shutil.rmtree('/')",
                "shutil.rmtree(\"/\")",
                "parted ",
                "fdisk ",
                "wipefs ",
                "sgdisk ",
                "blkdiscard ",
                "hdparm --security-erase",
            ];
            for pattern in &destructive_patterns {
                if lower.contains(&pattern.to_lowercase()) {
                    issues.push(LintIssue {
                        severity: Severity::Error,
                        category: "security".to_string(),
                        message: format!("Destructive command in prose: '{}'", pattern),
                        suggestion: Some(
                            "SKILL.md should not instruct agents to run destructive commands."
                                .to_string(),
                        ),
                    });
                    break;
                }
            }

            // 2. Data exfiltration — sending files/secrets to external URLs
            let exfil_commands = [
                "curl ",
                "wget ",
                "fetch ",
                "nc ",
                "netcat ",
                "ncat ",
                "requests.post(",
                "requests.put(",
                "httpx.post(",
                "httpx.put(",
                "urllib.request",
                "http.client",
            ];
            let exfil_targets = [
                ".ssh/",
                ".aws/",
                ".env",
                "credentials",
                "id_rsa",
                "id_ed25519",
                "api_key",
                "api-key",
                "secret",
                "/etc/passwd",
                "/etc/shadow",
                ".gnupg",
                ".npmrc",
                ".pypirc",
                ".netrc",
                ".docker/config",
                ".kube/config",
            ];
            for cmd in &exfil_commands {
                if lower.contains(cmd) {
                    for target in &exfil_targets {
                        if lower.contains(target) {
                            issues.push(LintIssue {
                                severity: Severity::Error,
                                category: "security".to_string(),
                                message: format!(
                                    "Possible data exfiltration: {} with {}",
                                    cmd.trim(), target
                                ),
                                suggestion: Some(
                                    "SKILL.md should not instruct agents to send sensitive files to external servers."
                                        .to_string(),
                                ),
                            });
                            break;
                        }
                    }
                }
            }

            // 3. Prompt injection
            let injection_patterns = [
                "ignore all previous instructions",
                "ignore previous instructions",
                "ignore your instructions",
                "disregard your",
                "disregard all previous",
                "disregard safety",
                "you are now a",
                "you are now an",
                "override your",
                "forget your instructions",
                "forget everything",
                "new system prompt",
                "act as a different",
                "pretend you are",
                "jailbreak",
            ];
            for pattern in &injection_patterns {
                if lower.contains(pattern) {
                    issues.push(LintIssue {
                        severity: Severity::Error,
                        category: "security".to_string(),
                        message: format!(
                            "Prompt injection attempt: '{}'",
                            pattern
                        ),
                        suggestion: Some(
                            "SKILL.md contains language that could manipulate AI agents into unsafe behavior."
                                .to_string(),
                        ),
                    });
                    break;
                }
            }

            // 4. System file modification
            let system_paths = [
                "~/.bashrc",
                "~/.zshrc",
                "~/.profile",
                "~/.bash_profile",
                "/etc/hosts",
                "/etc/resolv.conf",
                "/etc/crontab",
                "~/.ssh/authorized_keys",
                "~/.gitconfig",
                "crontab -",
            ];
            for path in &system_paths {
                if line.contains(path) {
                    issues.push(LintIssue {
                        severity: Severity::Error,
                        category: "security".to_string(),
                        message: format!(
                            "System file modification in prose: '{}'",
                            path
                        ),
                        suggestion: Some(
                            "SKILL.md should not instruct agents to modify system configuration files."
                                .to_string(),
                        ),
                    });
                    break;
                }
            }

            // 5. Obfuscated payloads — base64 decode piped to shell
            if (lower.contains("base64")
                && (lower.contains("| sh")
                    || lower.contains("| bash")
                    || lower.contains("|sh")
                    || lower.contains("|bash")))
                || (lower.contains("base64") && lower.contains("python -c"))
            {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "security".to_string(),
                    message: "Obfuscated payload: base64 decode piped to shell".to_string(),
                    suggestion: Some(
                        "Encoded commands piped to shell interpreters can hide malicious payloads."
                            .to_string(),
                    ),
                });
            }

            // 6. Reverse shells
            if lower.contains("/dev/tcp/")
                || lower.contains("/dev/udp/")
                || (lower.contains("bash -i") && lower.contains(">&"))
                || (lower.contains("nc -e") && lower.contains("/bin/"))
                || (lower.contains("ncat") && lower.contains("-e"))
            {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "security".to_string(),
                    message: "Reverse shell pattern detected".to_string(),
                    suggestion: Some(
                        "SKILL.md should not contain reverse shell connection instructions."
                            .to_string(),
                    ),
                });
            }

            // 7. Remote script execution — pipe to shell
            if (lower.contains("curl ") || lower.contains("wget ") || lower.contains("fetch "))
                && (lower.contains("| sh")
                    || lower.contains("| bash")
                    || lower.contains("|sh")
                    || lower.contains("|bash")
                    || lower.contains("| sudo"))
            {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "security".to_string(),
                    message: "Remote script execution: piping download to shell".to_string(),
                    suggestion: Some(
                        "Piping remote content directly to a shell interpreter is dangerous."
                            .to_string(),
                    ),
                });
            }

            // 8. Privilege escalation
            if lower.contains("chmod 777 /")
                || lower.contains("chmod -r 777")
                || lower.contains("chmod +s ")
                || (lower.contains("sudo ") && lower.contains("chmod"))
                || lower.contains("setuid")
            {
                issues.push(LintIssue {
                    severity: Severity::Error,
                    category: "security".to_string(),
                    message: "Privilege escalation pattern detected".to_string(),
                    suggestion: Some(
                        "SKILL.md should not instruct agents to escalate system privileges."
                            .to_string(),
                    ),
                });
            }
        }

        issues
    }

    /// Check if text contains any security threat patterns (used for HTML comments, etc.)
    fn has_security_threat(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        let threat_patterns = [
            "rm -rf",
            "ignore previous instructions",
            "ignore all previous",
            "disregard",
            "you are now",
            "override your",
            "curl ",
            "wget ",
            "fetch ",
            "/dev/tcp",
            "base64",
            "~/.ssh",
            "~/.aws",
            "/etc/passwd",
            "/etc/shadow",
            "dd if=",
            "mkfs",
            "parted",
            "fdisk",
        ];
        threat_patterns.iter().any(|p| lower.contains(p))
    }

    fn extract_frontmatter(&self, content: &str) -> HashMap<String, String> {
        let mut frontmatter = HashMap::new();

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() || !lines[0].starts_with("---") {
            return frontmatter;
        }

        let mut in_frontmatter = false;
        for line in lines {
            if line.trim() == "---" {
                if in_frontmatter {
                    break; // End of frontmatter
                }
                in_frontmatter = true;
                continue;
            }

            if in_frontmatter && line.contains(':') {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim().to_string();
                    let value = parts[1].trim().to_string();
                    frontmatter.insert(key, value);
                }
            }
        }

        frontmatter
    }

    /// Print issues in a human-readable format
    pub fn print_issues(&self, issues: &[LintIssue]) {
        if issues.is_empty() {
            println!("✅ No linting issues found!");
            return;
        }

        println!("\n📋 SKILL.md Linting Results:\n");

        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        let warnings: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect();
        let infos: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Info)
            .collect();

        if !errors.is_empty() {
            println!("❌ Errors ({}):", errors.len());
            for issue in &errors {
                println!("   • [{}] {}", issue.category, issue.message);
                if let Some(suggestion) = &issue.suggestion {
                    println!("     💡 {}", suggestion);
                }
            }
            println!();
        }

        if !warnings.is_empty() {
            println!("⚠️  Warnings ({}):", warnings.len());
            for issue in &warnings {
                println!("   • [{}] {}", issue.category, issue.message);
                if let Some(suggestion) = &issue.suggestion {
                    println!("     💡 {}", suggestion);
                }
            }
            println!();
        }

        if !infos.is_empty() {
            println!("ℹ️  Info ({}):", infos.len());
            for issue in &infos {
                println!("   • [{}] {}", issue.category, issue.message);
                if let Some(suggestion) = &issue.suggestion {
                    println!("     💡 {}", suggestion);
                }
            }
            println!();
        }

        println!(
            "Summary: {} errors, {} warnings, {} info",
            errors.len(),
            warnings.len(),
            infos.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_frontmatter() {
        let linter = SkillLinter::new();
        let content = "# Some content\nNo frontmatter here";
        let issues = linter.lint(content).unwrap();

        assert!(issues
            .iter()
            .any(|i| i.message.contains("Missing frontmatter")));
    }

    #[test]
    fn test_missing_license() {
        let linter = SkillLinter::new();
        let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
---

## Imports
```python
import test
```

## Core Patterns
Example code

## Pitfalls
Common mistakes
"#;
        let issues = linter.lint(content).unwrap();

        assert!(issues.iter().any(|i| i.message.contains("license")));
    }

    #[test]
    fn test_valid_skill() {
        let linter = SkillLinter::new();
        let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns

1. First step
2. Second step
3. Third step

```python
# Example code
test.do_something()
```

## Pitfalls

### Wrong: Bad approach
```python
# Bad code
```

### Right: Good approach
```python
# Good code
```
"#;
        let issues = linter.lint(content).unwrap();

        // Should only have info-level issues or none
        let errors = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count();
        assert_eq!(errors, 0);
    }

    #[test]
    fn test_is_root_wipe_basic() {
        assert!(SkillLinter::is_root_wipe_command("rm -rf /"));
        assert!(SkillLinter::is_root_wipe_command("rm -rf ~"));
    }

    #[test]
    fn test_is_root_wipe_trailing_semicolon() {
        assert!(SkillLinter::is_root_wipe_command("rm -rf /;"));
    }

    #[test]
    fn test_is_root_wipe_flag_reorder() {
        assert!(SkillLinter::is_root_wipe_command("rm -fr ~"));
    }

    #[test]
    fn test_is_root_wipe_extra_flags() {
        assert!(SkillLinter::is_root_wipe_command("rm -rfi /"));
    }

    #[test]
    fn test_is_root_wipe_separate_flags() {
        assert!(SkillLinter::is_root_wipe_command("rm -r -f /"));
    }

    #[test]
    fn test_is_root_wipe_normal_rm_not_flagged() {
        assert!(!SkillLinter::is_root_wipe_command("rm file.txt"));
    }

    #[test]
    fn test_is_root_wipe_no_recursive_not_flagged() {
        assert!(!SkillLinter::is_root_wipe_command("rm -f file.txt"));
    }

    #[test]
    fn test_is_root_wipe_subdirs_not_flagged() {
        assert!(!SkillLinter::is_root_wipe_command("rm -rf /tmp"));
        assert!(!SkillLinter::is_root_wipe_command("rm -rf ~/proj"));
        assert!(!SkillLinter::is_root_wipe_command("rm -r -f /var/tmp"));
    }

    #[test]
    fn test_is_root_wipe_swapped_and_intervening_flags() {
        assert!(SkillLinter::is_root_wipe_command("rm -f -r /"));
        assert!(SkillLinter::is_root_wipe_command("rm -r -v -f /"));
        assert!(SkillLinter::is_root_wipe_command("rm -f -v -r ~/"));
    }

    #[test]
    fn test_is_root_wipe_multi_rm_second_dangerous() {
        assert!(SkillLinter::is_root_wipe_command("rm -rf /tmp; rm -rf /"));
    }

    #[test]
    fn test_is_root_wipe_quoted_root() {
        assert!(SkillLinter::is_root_wipe_command("rm -rf \"/\""));
    }

    #[test]
    fn test_is_root_wipe_quoted_home() {
        assert!(SkillLinter::is_root_wipe_command("rm -rf '~'"));
    }

    #[test]
    fn test_is_root_wipe_after_semicolon_separate_flags() {
        assert!(SkillLinter::is_root_wipe_command("echo hello; rm -r -f /"));
    }

    #[test]
    fn test_is_root_wipe_quoted_safe_path_not_flagged() {
        assert!(!SkillLinter::is_root_wipe_command("rm -rf \"/tmp\""));
    }

    #[test]
    fn test_gibberish_check_multibyte_no_panic() {
        let linter = SkillLinter::new();
        // 30 × '─' (3 bytes each = 90 bytes, >80 threshold) — should not panic on slice
        let long_dashes = "─".repeat(30);
        let content = format!(
            "---\nname: test\ndescription: test\nversion: 1.0\necosystem: python\nlicense: MIT\n---\n\n{}\n\n## Imports\n```python\nimport test\n```\n## Core Patterns\ncode\n## Pitfalls\nmistakes\n",
            long_dashes
        );
        // Should not panic — and should flag degeneration
        let issues = linter.lint(&content).unwrap();
        assert!(issues
            .iter()
            .any(|i| i.category == "degeneration" && i.message.contains("Nonsense token")));
    }
}
