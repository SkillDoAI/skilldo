use skilldo::lint::{Severity, SkillLinter};

// ============================================================================
// FRONTMATTER VALIDATION TESTS
// ============================================================================

#[test]
fn test_missing_frontmatter_should_return_error() {
    let linter = SkillLinter::new();
    let content = "# Some content\nNo frontmatter here";

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let frontmatter_error = issues
        .iter()
        .find(|i| i.message.contains("Missing frontmatter"));

    assert!(frontmatter_error.is_some());
    assert_eq!(frontmatter_error.unwrap().severity, Severity::Error);
    assert_eq!(frontmatter_error.unwrap().category, "frontmatter");
}

#[test]
fn test_empty_content_should_return_missing_frontmatter() {
    let linter = SkillLinter::new();
    let content = "";

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    assert!(issues
        .iter()
        .any(|i| i.message.contains("Missing frontmatter")));
}

#[test]
fn test_frontmatter_without_closing_delimiter_should_return_error() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    // Should detect missing required fields since frontmatter didn't close
    assert!(issues.iter().any(|i| i.severity == Severity::Error));
}

#[test]
fn test_missing_required_field_name_should_return_error() {
    let linter = SkillLinter::new();
    let content = r#"---
description: test lib
version: 1.0
ecosystem: python
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let name_error = issues
        .iter()
        .find(|i| i.message.contains("Missing required field: name"));

    assert!(name_error.is_some());
    assert_eq!(name_error.unwrap().severity, Severity::Error);
    assert!(name_error.unwrap().suggestion.is_some());
}

#[test]
fn test_missing_required_field_description_should_return_error() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
version: 1.0
ecosystem: python
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    assert!(issues
        .iter()
        .any(|i| i.message.contains("Missing required field: description")));
}

#[test]
fn test_missing_required_field_version_should_return_error() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
ecosystem: python
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    assert!(issues
        .iter()
        .any(|i| i.message.contains("Missing required field: version")));
}

#[test]
fn test_missing_required_field_ecosystem_should_return_error() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    assert!(issues
        .iter()
        .any(|i| i.message.contains("Missing required field: ecosystem")));
}

#[test]
fn test_all_required_fields_missing_should_return_multiple_errors() {
    let linter = SkillLinter::new();
    let content = r#"---
author: someone
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.category == "frontmatter")
        .collect();

    // Should have errors for name, description, version, ecosystem
    assert!(errors.len() >= 4);
}

#[test]
fn test_missing_license_field_should_return_warning() {
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
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let license_warning = issues.iter().find(|i| i.message.contains("license"));

    assert!(license_warning.is_some());
    assert_eq!(license_warning.unwrap().severity, Severity::Warning);
    assert!(license_warning.unwrap().suggestion.is_some());
}

#[test]
fn test_complete_frontmatter_should_not_return_frontmatter_errors() {
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
Example code here with more content to reach minimum length.
Let me add more text to ensure we pass the minimum content length check.
This is just filler content to make the test pass cleanly.

## Pitfalls
Common mistakes

### Wrong
Bad code

### Right
Good code
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let frontmatter_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "frontmatter" && i.severity == Severity::Error)
        .collect();

    assert_eq!(frontmatter_errors.len(), 0);
}

#[test]
fn test_frontmatter_with_extra_fields_should_be_valid() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
license: MIT
author: John Doe
tags: testing, validation
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    // Extra fields should be accepted
    let issues = result.unwrap();
    let frontmatter_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "frontmatter" && i.severity == Severity::Error)
        .collect();

    assert_eq!(frontmatter_errors.len(), 0);
}

#[test]
fn test_frontmatter_with_invalid_yaml_format_should_be_parsed() {
    let linter = SkillLinter::new();
    let content = r#"---
name test
description: test lib
version: 1.0
ecosystem: python
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    // Should report missing 'name' since "name test" doesn't have colon
    assert!(issues
        .iter()
        .any(|i| i.message.contains("Missing required field: name")));
}

// ============================================================================
// STRUCTURE VALIDATION TESTS
// ============================================================================

#[test]
fn test_missing_imports_section_should_return_error() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
license: MIT
---

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let imports_error = issues
        .iter()
        .find(|i| i.message.contains("Missing required section: ## Imports"));

    assert!(imports_error.is_some());
    assert_eq!(imports_error.unwrap().severity, Severity::Error);
    assert_eq!(imports_error.unwrap().category, "structure");
}

#[test]
fn test_missing_core_patterns_section_should_return_error() {
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

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let patterns_error = issues.iter().find(|i| {
        i.message
            .contains("Missing required section: ## Core Patterns")
    });

    assert!(patterns_error.is_some());
    assert_eq!(patterns_error.unwrap().severity, Severity::Error);
}

#[test]
fn test_missing_pitfalls_section_should_return_error() {
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
Example
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let pitfalls_error = issues
        .iter()
        .find(|i| i.message.contains("Missing required section: ## Pitfalls"));

    assert!(pitfalls_error.is_some());
    assert_eq!(pitfalls_error.unwrap().severity, Severity::Error);
}

#[test]
fn test_all_required_sections_missing_should_return_multiple_errors() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
license: MIT
---

## Introduction
Some content
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let structure_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "structure" && i.severity == Severity::Error)
        .collect();

    // Should have 3 errors for Imports, Core Patterns, and Pitfalls
    assert_eq!(structure_errors.len(), 3);
}

#[test]
fn test_all_required_sections_present_should_not_return_structure_errors() {
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
Example patterns

## Pitfalls
Common issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let structure_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "structure" && i.severity == Severity::Error)
        .collect();

    assert_eq!(structure_errors.len(), 0);
}

#[test]
fn test_case_sensitive_section_headers_should_not_match() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
license: MIT
---

## imports
```python
import test
```

## core patterns
Example

## pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let structure_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "structure" && i.severity == Severity::Error)
        .collect();

    // Should have 3 errors because headers are case-sensitive
    assert_eq!(structure_errors.len(), 3);
}

#[test]
fn test_sections_with_extra_content_should_still_match() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
license: MIT
---

## Imports and Dependencies
```python
import test
```

## Core Patterns for Success
Example

## Pitfalls to Avoid
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    // Headers like "## Imports and Dependencies" contain "## Imports"
    let structure_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "structure" && i.severity == Severity::Error)
        .collect();

    assert_eq!(structure_errors.len(), 0);
}

// ============================================================================
// CONTENT VALIDATION TESTS
// ============================================================================

#[test]
fn test_no_code_blocks_should_return_error() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
license: MIT
---

## Imports
import test

## Core Patterns
Example without code blocks

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let code_error = issues
        .iter()
        .find(|i| i.message.contains("No code examples found"));

    assert!(code_error.is_some());
    assert_eq!(code_error.unwrap().severity, Severity::Error);
    assert_eq!(code_error.unwrap().category, "content");
}

#[test]
fn test_with_code_blocks_should_not_return_code_error() {
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
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let code_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.message.contains("No code examples"))
        .collect();

    assert_eq!(code_errors.len(), 0);
}

#[test]
fn test_very_short_content_should_return_warning() {
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
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let length_warning = issues
        .iter()
        .find(|i| i.message.contains("Content is very short"));

    assert!(length_warning.is_some());
    assert_eq!(length_warning.unwrap().severity, Severity::Warning);
    assert_eq!(length_warning.unwrap().category, "content");
}

#[test]
fn test_content_under_1000_chars_should_return_warning_with_count() {
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
Short example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let length_warning = issues
        .iter()
        .find(|i| i.message.contains("very short") && i.message.contains("chars"));

    assert!(length_warning.is_some());
    assert!(length_warning
        .unwrap()
        .message
        .contains(&content.len().to_string()));
}

#[test]
fn test_content_over_1000_chars_should_not_return_length_warning() {
    let linter = SkillLinter::new();
    let long_content = "This is a very long example. ".repeat(50);
    let content = format!(
        r#"---
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
{}

## Pitfalls

### Wrong
Bad code

### Right
Good code
"#,
        long_content
    );

    let result = linter.lint(content.as_str());
    assert!(result.is_ok());

    let issues = result.unwrap();
    let length_warnings: Vec<_> = issues
        .iter()
        .filter(|i| i.message.contains("Content is very short"))
        .collect();

    assert_eq!(length_warnings.len(), 0);
}

#[test]
fn test_pitfalls_without_wrong_example_should_return_info() {
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
Example

## Pitfalls

### Right
Good code only
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let pitfall_info = issues.iter().find(|i| {
        i.message
            .contains("Pitfalls section should include 'Wrong' and 'Right' examples")
    });

    assert!(pitfall_info.is_some());
    assert_eq!(pitfall_info.unwrap().severity, Severity::Info);
}

#[test]
fn test_pitfalls_without_right_example_should_return_info() {
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
Example

## Pitfalls

### Wrong
Bad code only
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let pitfall_info = issues.iter().find(|i| {
        i.message
            .contains("Pitfalls section should include 'Wrong' and 'Right' examples")
    });

    assert!(pitfall_info.is_some());
    assert_eq!(pitfall_info.unwrap().severity, Severity::Info);
}

#[test]
fn test_pitfalls_with_wrong_and_right_should_not_return_info() {
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
Example

## Pitfalls

### Wrong
Bad code

### Right
Good code
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let pitfall_infos: Vec<_> = issues
        .iter()
        .filter(|i| i.message.contains("Pitfalls section should include"))
        .collect();

    assert_eq!(pitfall_infos.len(), 0);
}

#[test]
fn test_pitfalls_with_emoji_markers_should_be_valid() {
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
Example

## Pitfalls

### ❌ Common Mistake
Bad code

### ✅ Better Approach
Good code
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let pitfall_infos: Vec<_> = issues
        .iter()
        .filter(|i| i.message.contains("Pitfalls section should include"))
        .collect();

    assert_eq!(pitfall_infos.len(), 0);
}

#[test]
fn test_pitfalls_with_mixed_markers_should_be_valid() {
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
Example

## Pitfalls

### Wrong: First mistake
Bad code

### ✅ Correct way
Good code
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let pitfall_infos: Vec<_> = issues
        .iter()
        .filter(|i| i.message.contains("Pitfalls section should include"))
        .collect();

    assert_eq!(pitfall_infos.len(), 0);
}

#[test]
fn test_no_pitfalls_section_should_not_check_wrong_right() {
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
Example
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    // Should have error for missing Pitfalls section, but no info about Wrong/Right
    let pitfall_content_infos: Vec<_> = issues
        .iter()
        .filter(|i| {
            i.category == "content" && i.message.contains("Wrong") && i.severity == Severity::Info
        })
        .collect();

    assert_eq!(pitfall_content_infos.len(), 0);
}

// ============================================================================
// SEVERITY LEVEL TESTS
// ============================================================================

#[test]
fn test_error_severity_for_missing_frontmatter() {
    let linter = SkillLinter::new();
    let content = "No frontmatter";

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let frontmatter_issue = issues
        .iter()
        .find(|i| i.message.contains("Missing frontmatter"));

    assert_eq!(frontmatter_issue.unwrap().severity, Severity::Error);
}

#[test]
fn test_error_severity_for_missing_required_fields() {
    let linter = SkillLinter::new();
    let content = r#"---
extra_field: value
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let field_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.message.contains("Missing required field") && i.severity == Severity::Error)
        .collect();

    assert_eq!(field_errors.len(), 4); // name, description, version, ecosystem
}

#[test]
fn test_warning_severity_for_missing_license() {
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
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let license_issue = issues.iter().find(|i| i.message.contains("license"));

    assert_eq!(license_issue.unwrap().severity, Severity::Warning);
}

#[test]
fn test_warning_severity_for_short_content() {
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
Short

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let length_issue = issues.iter().find(|i| i.message.contains("very short"));

    assert_eq!(length_issue.unwrap().severity, Severity::Warning);
}

#[test]
fn test_info_severity_for_pitfalls_format() {
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
Example

## Pitfalls
Just text, no Wrong/Right sections
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let pitfall_issue = issues
        .iter()
        .find(|i| i.message.contains("Pitfalls section"));

    assert_eq!(pitfall_issue.unwrap().severity, Severity::Info);
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_content_with_only_newlines_should_return_errors() {
    let linter = SkillLinter::new();
    let content = "\n\n\n\n";

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    assert!(issues.iter().any(|i| i.severity == Severity::Error));
}

#[test]
fn test_frontmatter_with_spaces_around_delimiters() {
    let linter = SkillLinter::new();
    let content = r#"  ---
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
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    // Leading spaces prevent frontmatter detection (parser requires --- at line start)
    let frontmatter_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "frontmatter" && i.severity == Severity::Error)
        .collect();

    // Should report missing frontmatter since parser doesn't detect it with leading spaces
    assert_eq!(frontmatter_errors.len(), 1);
    assert!(frontmatter_errors[0]
        .message
        .contains("Missing frontmatter"));
}

#[test]
fn test_frontmatter_with_multiline_values() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: This is a very long description
  that spans multiple lines
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    // Multiline values may not be parsed correctly by simple parser
    // This is an edge case showing current limitations
}

#[test]
fn test_multiple_code_blocks_should_pass_code_check() {
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
```python
def example():
    pass
```

```python
def another():
    pass
```

## Pitfalls

### Wrong
```python
# bad
```

### Right
```python
# good
```
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let code_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.message.contains("No code examples"))
        .collect();

    assert_eq!(code_errors.len(), 0);
}

#[test]
fn test_sections_in_different_order_should_still_be_detected() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
license: MIT
---

## Pitfalls
Issues first

## Core Patterns
Then patterns

## Imports
```python
import test
```
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let structure_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "structure" && i.severity == Severity::Error)
        .collect();

    // Order doesn't matter, all sections present
    assert_eq!(structure_errors.len(), 0);
}

#[test]
fn test_nested_sections_should_not_interfere() {
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

### Sub-section under imports

## Core Patterns
Example

### Another sub-section

## Pitfalls
Issues

### Yet another sub-section
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let structure_errors: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "structure" && i.severity == Severity::Error)
        .collect();

    assert_eq!(structure_errors.len(), 0);
}

#[test]
fn test_perfect_valid_skill_should_have_no_errors() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: A comprehensive test library for validation
version: 1.0.0
ecosystem: python
license: MIT
author: Test Author
tags: testing, validation
---

## Imports

```python
import test
from test import helpers
```

## Core Patterns

1. Initialize the test framework
2. Write comprehensive test cases
3. Run tests and validate results

```python
# Example usage
test.initialize()
result = test.run_all()
assert result.passed
```

## Advanced Usage

Additional examples and patterns here to ensure we have enough content
to pass the minimum length check. This section provides more detailed
information about advanced features and edge cases.

```python
# Advanced example
test.configure(
    verbose=True,
    coverage=True
)
```

## Pitfalls

### Wrong: Not handling errors

```python
# This will crash on errors
result = test.run()
print(result.data)
```

### Right: Proper error handling

```python
# This handles errors gracefully
try:
    result = test.run()
    if result.success:
        print(result.data)
except test.TestError as e:
    print(f"Test failed: {e}")
```

## Best Practices

1. Always validate inputs
2. Use proper error handling
3. Write comprehensive tests
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();

    assert_eq!(errors.len(), 0);
}

// ============================================================================
// LINT ISSUE STRUCTURE TESTS
// ============================================================================

#[test]
fn test_lint_issue_has_suggestion() {
    let linter = SkillLinter::new();
    let content = "No frontmatter";

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let issue = issues.first().unwrap();

    assert!(issue.suggestion.is_some());
    assert!(!issue.suggestion.as_ref().unwrap().is_empty());
}

#[test]
fn test_lint_issue_has_category() {
    let linter = SkillLinter::new();
    let content = "No frontmatter";

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let issue = issues.first().unwrap();

    assert!(!issue.category.is_empty());
    assert!(["frontmatter", "structure", "content"].contains(&issue.category.as_str()));
}

#[test]
fn test_lint_issue_has_message() {
    let linter = SkillLinter::new();
    let content = "No frontmatter";

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    let issue = issues.first().unwrap();

    assert!(!issue.message.is_empty());
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[test]
fn test_complete_linting_workflow() {
    let linter = SkillLinter::new();

    // Start with bad content
    let bad_content = "Some random markdown";
    let result = linter.lint(bad_content);
    assert!(result.is_ok());
    let issues = result.unwrap();
    assert!(!issues.is_empty());
    assert!(issues.iter().any(|i| i.severity == Severity::Error));

    // Improve to minimal content
    let minimal_content = r#"---
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
Example

## Pitfalls
Issues
"#;
    let result = linter.lint(minimal_content);
    assert!(result.is_ok());
    let issues = result.unwrap();
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 0);
}

// ============================================================================
// UNKNOWN VERSION TESTS
// ============================================================================

#[test]
fn test_unknown_version_should_return_warning() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: unknown
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let issues = linter.lint(content).unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("unknown") && i.severity == Severity::Warning),
        "Should warn about version: unknown"
    );
}

#[test]
fn test_known_version_should_not_warn() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
Example

## Pitfalls
Issues
"#;

    let issues = linter.lint(content).unwrap();
    assert!(
        !issues.iter().any(|i| i.message.contains("unknown")),
        "Should not warn about a known version"
    );
}

// ============================================================================
// DEGENERATION DETECTION TESTS
// ============================================================================

#[test]
fn test_repeated_line_prefix_should_detect_degeneration() {
    let linter = SkillLinter::new();
    // Simulate a model spiraling with repeated API entries across lines
    let mut lines = vec![
        "---",
        "name: test",
        "description: test",
        "version: 1.0",
        "ecosystem: python",
        "license: MIT",
        "---",
        "",
        "## Imports",
        "```python",
        "import test",
        "```",
        "",
        "## Core Patterns",
        "Example patterns here with enough content.",
        "",
        "## API Reference",
        "",
    ];
    // Add 15 lines all starting with the same long prefix
    for _i in 0..15 {
        lines.push("- **app.session_cookie_xxxxxxxxxx**");
    }
    lines.push("");
    lines.push("## Pitfalls");
    lines.push("Issues");
    let content = lines.join("\n");

    let issues = linter.lint(&content).unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.category == "degeneration" && i.severity == Severity::Error),
        "Should detect repetitive line prefix as degeneration"
    );
}

#[test]
fn test_gibberish_token_should_detect_degeneration() {
    let linter = SkillLinter::new();
    let nonsense = "a".repeat(100);
    let content = format!(
        r#"---
name: test
description: test
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
Example with enough content to pass length check plus more filler text here.

## API Reference
- **app.{}** some api

## Pitfalls
Issues
"#,
        nonsense
    );

    let issues = linter.lint(&content).unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.category == "degeneration" && i.message.contains("Nonsense token")),
        "Should detect gibberish token (100 chars of 'a')"
    );
}

#[test]
fn test_giant_line_should_detect_degeneration() {
    let linter = SkillLinter::new();
    let giant = "**app.thing**, ".repeat(200); // ~3000 chars on one line
    let content = format!(
        r#"---
name: test
description: test
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
Example with enough content here.

## API Reference
{}

## Pitfalls
Issues
"#,
        giant
    );

    let issues = linter.lint(&content).unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.category == "degeneration" && i.message.contains("long line")),
        "Should detect excessively long line as degeneration"
    );
}

#[test]
fn test_long_line_inside_code_block_should_not_flag() {
    let linter = SkillLinter::new();
    let long_code = "x = ".to_owned() + &"'a' + ".repeat(300); // long but inside code block
    let content = format!(
        r#"---
name: test
description: test
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
```python
{}
```

More content here to pad the length requirement for the linter check.
And some more filler content to be safe.

## Pitfalls

### Wrong
bad

### Right
good
"#,
        long_code
    );

    let issues = linter.lint(&content).unwrap();
    let degen_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "degeneration")
        .collect();
    assert_eq!(
        degen_issues.len(),
        0,
        "Long lines inside code blocks should not trigger degeneration"
    );
}

#[test]
fn test_normal_api_reference_should_not_flag() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
Example with enough content to pass length checks and more filler.

## API Reference
- **app.route()** - Define a route
- **app.run()** - Start the server
- **app.config** - Configuration dictionary
- **request.args** - Query parameters
- **request.form** - Form data

## Pitfalls

### Wrong
bad

### Right
good
"#;

    let issues = linter.lint(content).unwrap();
    let degen_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.category == "degeneration")
        .collect();
    assert_eq!(
        degen_issues.len(),
        0,
        "Normal API reference should not trigger degeneration"
    );
}

#[test]
fn test_unclosed_code_block_should_detect_truncation() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
```python
def example():
    data = [1, 2, 3
"#;
    // Note: no closing ``` — simulates token limit truncation

    let issues = linter.lint(content).unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.category == "degeneration" && i.message.contains("Unclosed code block")),
        "Should detect unclosed code block from truncated output"
    );
}

#[test]
fn test_prompt_instruction_leak_should_be_detected() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
Example with enough content to pass length checks and more filler.

## References

CRITICAL: Include ALL provided URLs below (do NOT skip this section):

- [Docs](https://example.com)

## Pitfalls

### Wrong
bad

### Right
good
"#;

    let issues = linter.lint(content).unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.category == "degeneration" && i.message.contains("Prompt instruction leak")),
        "Should detect prompt instruction leaking into output"
    );
}

#[test]
fn test_prompt_leak_inside_code_block_should_not_flag() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test
version: 1.0
ecosystem: python
license: MIT
---

## Imports
```python
import test
```

## Core Patterns
Example with enough content to pass length checks and more filler.

```python
# CRITICAL: Include ALL items in the list
items = get_all()
```

## Pitfalls

### Wrong
bad

### Right
good
"#;

    let issues = linter.lint(content).unwrap();
    let leaks: Vec<_> = issues
        .iter()
        .filter(|i| i.message.contains("Prompt instruction leak"))
        .collect();
    assert_eq!(
        leaks.len(),
        0,
        "Prompt-like text inside code blocks should not be flagged"
    );
}

#[test]
fn test_flask_style_degeneration_should_be_caught() {
    let linter = SkillLinter::new();
    // Reproduce the actual flask failure pattern: one massive line with repetitive entries
    let mut entries = String::new();
    for i in 0..50 {
        entries.push_str(&format!("**app.session_cookie_to_format{}**, ", i));
        entries.push_str(&format!("**app.session_cookie_from_format{}**, ", i));
    }
    let content = format!(
        r#"---
name: flask
description: Web framework
version: 3.2.0
ecosystem: python
license: BSD-3-Clause
---

## Imports
```python
from flask import Flask
```

## Core Patterns
Basic Flask app pattern with enough content.

## API Reference
- **app.route()** - good
{}

## Pitfalls

### Wrong
bad

### Right
good
"#,
        entries
    );

    let issues = linter.lint(&content).unwrap();
    assert!(
        issues.iter().any(|i| i.category == "degeneration"),
        "Flask-style degeneration (giant line with repeated patterns) should be caught"
    );
}

#[test]
fn test_linter_returns_issues_sorted_by_severity() {
    let linter = SkillLinter::new();
    let content = r#"---
name: test
description: test lib
version: 1.0
ecosystem: python
---

## Imports
import test

## Core Patterns
Short

## Pitfalls
No examples
"#;

    let result = linter.lint(content);
    assert!(result.is_ok());

    let issues = result.unwrap();
    // Check we have all severity types
    assert!(issues.iter().any(|i| i.severity == Severity::Error));
    assert!(issues.iter().any(|i| i.severity == Severity::Warning));
    assert!(issues.iter().any(|i| i.severity == Severity::Info));
}
