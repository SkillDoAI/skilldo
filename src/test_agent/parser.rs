//! Shared types for SKILL.md parsing — used by all language-specific parsers.

/// Represents a code pattern extracted from SKILL.md
#[derive(Debug, Clone, PartialEq)]
pub struct CodePattern {
    pub name: String,
    pub description: String,
    pub code: String,
    pub category: PatternCategory,
}

/// Categories for prioritizing pattern testing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternCategory {
    BasicUsage,
    Configuration,
    ErrorHandling,
    AsyncPattern,
    Other,
}
