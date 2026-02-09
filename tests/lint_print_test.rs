//! Tests for linter output formatting
//! Tests the print_issues method for human-readable output

use skilldo::lint::{LintIssue, Severity, SkillLinter};

#[test]
fn test_print_issues_with_no_issues() {
    let linter = SkillLinter::new();
    let issues = vec![];

    // Should print success message without panicking
    linter.print_issues(&issues);
    // Test passes if no panic
}

#[test]
fn test_print_issues_with_errors() {
    let linter = SkillLinter::new();
    let issues = vec![
        LintIssue {
            severity: Severity::Error,
            category: "frontmatter".to_string(),
            message: "Missing required field: name".to_string(),
            suggestion: Some("Add 'name: <value>' to frontmatter".to_string()),
        },
        LintIssue {
            severity: Severity::Error,
            category: "structure".to_string(),
            message: "Missing required section: ## Core Patterns".to_string(),
            suggestion: Some("Add a '## Core Patterns' section".to_string()),
        },
    ];

    // Should print error messages without panicking
    linter.print_issues(&issues);
    // Test passes if no panic
}

#[test]
fn test_print_issues_with_warnings() {
    let linter = SkillLinter::new();
    let issues = vec![LintIssue {
        severity: Severity::Warning,
        category: "frontmatter".to_string(),
        message: "Missing recommended field: license".to_string(),
        suggestion: Some("Add 'license: <value>' to frontmatter".to_string()),
    }];

    // Should print warning messages without panicking
    linter.print_issues(&issues);
    // Test passes if no panic
}

#[test]
fn test_print_issues_with_info() {
    let linter = SkillLinter::new();
    let issues = vec![LintIssue {
        severity: Severity::Info,
        category: "optimization".to_string(),
        message: "Consider adding more code examples".to_string(),
        suggestion: None,
    }];

    // Should print info messages without panicking
    linter.print_issues(&issues);
    // Test passes if no panic
}

#[test]
fn test_print_issues_with_mixed_severity() {
    let linter = SkillLinter::new();
    let issues = vec![
        LintIssue {
            severity: Severity::Error,
            category: "frontmatter".to_string(),
            message: "Missing name".to_string(),
            suggestion: None,
        },
        LintIssue {
            severity: Severity::Warning,
            category: "content".to_string(),
            message: "Empty section".to_string(),
            suggestion: Some("Add content".to_string()),
        },
        LintIssue {
            severity: Severity::Info,
            category: "style".to_string(),
            message: "Use consistent formatting".to_string(),
            suggestion: None,
        },
    ];

    // Should print all severity levels
    linter.print_issues(&issues);
    // Test passes if no panic
}

#[test]
fn test_print_issues_with_suggestions() {
    let linter = SkillLinter::new();
    let issues = vec![LintIssue {
        severity: Severity::Error,
        category: "test".to_string(),
        message: "Test message".to_string(),
        suggestion: Some("Test suggestion".to_string()),
    }];

    // Should print suggestions
    linter.print_issues(&issues);
    // Test passes if no panic
}

#[test]
fn test_print_issues_without_suggestions() {
    let linter = SkillLinter::new();
    let issues = vec![LintIssue {
        severity: Severity::Error,
        category: "test".to_string(),
        message: "Test message".to_string(),
        suggestion: None,
    }];

    // Should handle missing suggestions
    linter.print_issues(&issues);
    // Test passes if no panic
}
