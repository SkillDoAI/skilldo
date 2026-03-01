use skilldo::detector::Language;
use skilldo::lint::{Severity, SkillLinter};
use skilldo::validator::{FunctionalValidator, ValidationResult};

/// Test that format validation catches missing frontmatter
#[test]
fn test_format_validation_catches_missing_frontmatter() {
    let invalid_skill = r#"
## Core Patterns

Some content without frontmatter.
"#;

    let linter = SkillLinter::new();
    let issues = linter.lint(invalid_skill).unwrap();

    let has_errors = issues.iter().any(|i| matches!(i.severity, Severity::Error));
    assert!(has_errors, "Should detect missing frontmatter");
}

/// Test that format validation catches invalid frontmatter format
#[test]
fn test_format_validation_catches_invalid_frontmatter() {
    let invalid_skill = r#"---
# This is wrong format
name: torch
---

Content here.
"#;

    let linter = SkillLinter::new();
    let issues = linter.lint(invalid_skill).unwrap();

    // Should catch malformed frontmatter
    assert!(!issues.is_empty(), "Should detect invalid frontmatter");
}

/// Test that format validation passes with valid frontmatter
#[test]
fn test_format_validation_passes_valid_frontmatter() {
    let valid_skill = r#"---
name: torch
version: 2.0.0
ecosystem: python
description: PyTorch deep learning framework
license: BSD-3-Clause
---

## Imports

```python
import torch
```

## Core Patterns

```python
import torch
x = torch.tensor([1, 2, 3])
print(x)
```

## Pitfalls

- CUDA tensors must be moved to CPU before converting to numpy
- Autograd requires tensors with requires_grad=True

## References

- [Homepage](https://pytorch.org)
"#;

    let linter = SkillLinter::new();
    let issues = linter.lint(valid_skill).unwrap();

    let has_errors = issues.iter().any(|i| matches!(i.severity, Severity::Error));
    assert!(!has_errors, "Valid skill should pass format validation");
}

/// Test that functional validator extracts code from SKILL.md
#[test]
fn test_functional_validator_extracts_code() {
    let skill_md = r#"---
name: requests
version: 2.31.0
ecosystem: python
---

## Core Patterns

```python
import math
result = math.sqrt(16)
assert result == 4.0
print("✓ Test passed")
```
"#;

    let validator = FunctionalValidator::new();
    let result = validator.validate(skill_md, &Language::Python).unwrap();

    match result {
        ValidationResult::Pass(output) => {
            assert!(output.contains("Test passed") || output.contains("successfully"));
        }
        ValidationResult::Skipped(msg) => {
            println!("Skipped (expected in CI): {}", msg);
        }
        ValidationResult::Fail(err) => {
            panic!("Should not fail with valid code: {}", err);
        }
    }
}

/// Test that functional validator catches invalid Python code
#[test]
fn test_functional_validator_catches_invalid_code() {
    let skill_md = r#"---
name: broken
version: 1.0.0
ecosystem: python
---

## Core Patterns

```python
import nonexistent_module_that_does_not_exist
x = undefined_function()
```
"#;

    let validator = FunctionalValidator::new();
    let result = validator.validate(skill_md, &Language::Python).unwrap();

    match result {
        ValidationResult::Fail(_) => {
            // Expected - invalid code should fail
        }
        ValidationResult::Skipped(msg) => {
            println!("Skipped (expected in CI): {}", msg);
        }
        ValidationResult::Pass(_) => {
            panic!("Should not pass with invalid code");
        }
    }
}

/// Test that functional validator skips when no code blocks present
#[test]
fn test_functional_validator_skips_no_code() {
    let skill_md = r#"---
name: empty
version: 1.0.0
ecosystem: python
---

## Core Patterns

This has no code blocks.
"#;

    let validator = FunctionalValidator::new();
    let result = validator.validate(skill_md, &Language::Python).unwrap();

    match result {
        ValidationResult::Skipped(reason) => {
            assert!(reason.contains("No runnable code") || reason.contains("No code blocks"));
        }
        _ => panic!("Should skip when no code blocks present"),
    }
}

/// Test that functional validator handles JavaScript ecosystem
#[test]
fn test_functional_validator_javascript_not_implemented() {
    let skill_md = r#"---
name: express
version: 4.18.0
ecosystem: javascript
---

```javascript
const express = require('express');
```
"#;

    let validator = FunctionalValidator::new();
    let result = validator.validate(skill_md, &Language::JavaScript).unwrap();

    match result {
        ValidationResult::Skipped(reason) => {
            assert!(reason.contains("JavaScript") || reason.contains("not yet implemented"));
        }
        _ => panic!("JavaScript validation should be skipped"),
    }
}

/// Test that format validation detects structural issues
#[test]
fn test_format_validation_missing_references() {
    let skill_md = r#"---
name: torch
version: 2.0.0
ecosystem: python
---

## Core Patterns

Content without proper structure.
"#;

    let linter = SkillLinter::new();
    let issues = linter.lint(skill_md).unwrap();

    // Should detect missing required fields or sections
    assert!(!issues.is_empty(), "Should detect structural issues");
}

/// Test dual validation sequence: format then functional
#[test]
fn test_dual_validation_sequence() {
    let valid_skill = r#"---
name: math
version: 1.0.0
ecosystem: python
description: Python math module for mathematical functions
license: PSF-2.0
---

## Imports

```python
import math
```

## Core Patterns

```python
import math
x = math.pi
assert x > 3.14 and x < 3.15
print("✓ Valid")
```

## Pitfalls

- Be careful with floating point precision
- Some functions may raise ValueError for invalid inputs

## References

- [Docs](https://docs.python.org)
"#;

    // 1. Format validation first
    let linter = SkillLinter::new();
    let lint_issues = linter.lint(valid_skill).unwrap();
    let has_format_errors = lint_issues
        .iter()
        .any(|i| matches!(i.severity, Severity::Error));

    assert!(!has_format_errors, "Format validation should pass");

    // 2. Functional validation second
    let validator = FunctionalValidator::new();
    let result = validator.validate(valid_skill, &Language::Python).unwrap();

    match result {
        ValidationResult::Pass(_) => {
            // Both validations passed!
        }
        ValidationResult::Skipped(msg) => {
            println!("Functional validation skipped: {}", msg);
            // Format passed, functional skipped - acceptable
        }
        ValidationResult::Fail(err) => {
            panic!("Functional validation should pass: {}", err);
        }
    }
}

/// Test that code extraction handles comments and empty lines
#[test]
fn test_code_extraction_handles_comments() {
    let skill_md = r#"
```python
# This is a comment
import math

# Another comment
x = math.sqrt(25)
# More comments
assert x == 5.0
```
"#;

    let validator = FunctionalValidator::new();

    // This should work - comments should be skipped during extraction
    let result = validator.validate(skill_md, &Language::Python).unwrap();

    match result {
        ValidationResult::Pass(_) | ValidationResult::Skipped(_) => {
            // Expected - either runs successfully or skips
        }
        ValidationResult::Fail(err) => {
            panic!("Should handle comments properly: {}", err);
        }
    }
}

/// Test that validator adds print statement when no assertions
#[test]
fn test_validator_adds_print_when_no_assertions() {
    let skill_md = r#"
```python
import math
x = math.sqrt(16)
# No assertions or prints
```
"#;

    let validator = FunctionalValidator::new();
    let result = validator.validate(skill_md, &Language::Python).unwrap();

    // Should either pass (added print) or skip (no docker/python)
    match result {
        ValidationResult::Pass(output) => {
            assert!(output.contains("successfully") || output.contains("executed"));
        }
        ValidationResult::Skipped(_) => {
            // Expected in CI environments
        }
        ValidationResult::Fail(err) => {
            panic!("Should add print and succeed: {}", err);
        }
    }
}

/// Test functional validator with multiple code blocks (uses first)
#[test]
fn test_functional_validator_uses_first_code_block() {
    let skill_md = r#"
## First Example

```python
import math
x = math.sqrt(9)
assert x == 3.0
print("✓ First block")
```

## Second Example

```python
# This should not be used
raise Exception("Second block should not run")
```
"#;

    let validator = FunctionalValidator::new();
    let result = validator.validate(skill_md, &Language::Python).unwrap();

    match result {
        ValidationResult::Pass(output) => {
            assert!(output.contains("First block") || output.contains("successfully"));
            assert!(!output.contains("Second block"));
        }
        ValidationResult::Skipped(_) => {
            // Expected in CI
        }
        ValidationResult::Fail(err) => {
            assert!(
                !err.contains("Second block"),
                "Should use first block, not second: {}",
                err
            );
        }
    }
}

/// Test format validation with multiple errors
#[test]
fn test_format_validation_multiple_errors() {
    let invalid_skill = r#"
# Missing frontmatter completely

## Core Patterns

No References section either.
"#;

    let linter = SkillLinter::new();
    let issues = linter.lint(invalid_skill).unwrap();

    let error_count = issues
        .iter()
        .filter(|i| matches!(i.severity, Severity::Error))
        .count();

    assert!(error_count > 0, "Should detect multiple format errors");
}

/// Test functional validation with syntax errors
#[test]
fn test_functional_validation_syntax_error() {
    let skill_md = r#"
```python
import math
x = math.sqrt(  # Missing closing parenthesis
print(x)
```
"#;

    let validator = FunctionalValidator::new();
    let result = validator.validate(skill_md, &Language::Python).unwrap();

    match result {
        ValidationResult::Fail(err) => {
            assert!(err.contains("SyntaxError") || err.contains("invalid syntax"));
        }
        ValidationResult::Skipped(_) => {
            // Expected in CI
        }
        ValidationResult::Pass(_) => {
            panic!("Should fail with syntax error");
        }
    }
}

/// Test functional validation with runtime errors
#[test]
fn test_functional_validation_runtime_error() {
    let skill_md = r#"
```python
x = 1 / 0  # Division by zero
```
"#;

    let validator = FunctionalValidator::new();
    let result = validator.validate(skill_md, &Language::Python).unwrap();

    match result {
        ValidationResult::Fail(err) => {
            assert!(err.contains("ZeroDivisionError") || err.contains("division"));
        }
        ValidationResult::Skipped(_) => {
            // Expected in CI
        }
        ValidationResult::Pass(_) => {
            panic!("Should fail with runtime error");
        }
    }
}

/// Test that validator initializes and handles simple code
#[test]
fn test_docker_detection() {
    let validator = FunctionalValidator::new();

    // Verify the validator initializes and can process simple code
    let skill_md = r#"
```python
x = 1 + 1
assert x == 2
print("test")
```
"#;

    let result = validator.validate(skill_md, &Language::Python);

    // Should return Ok with some ValidationResult (Pass, Fail, or Skipped)
    match result {
        Ok(_) => { /* Success - validator works */ }
        Err(e) => panic!("Validator should not error on valid input: {}", e),
    }
}

/// Test ecosystem validation for unsupported languages
#[test]
fn test_unsupported_ecosystem() {
    let skill_md = r#"
```rust
fn main() {
    println!("Hello");
}
```
"#;

    let validator = FunctionalValidator::new();
    let result = validator.validate(skill_md, &Language::Rust).unwrap();

    match result {
        ValidationResult::Skipped(reason) => {
            assert!(reason.contains("not supported") || reason.contains("rust"));
        }
        _ => panic!("Should skip unsupported ecosystems"),
    }
}

/// Test that validator handles empty skill.md
#[test]
fn test_validator_handles_empty_skill() {
    let validator = FunctionalValidator::new();
    let result = validator.validate("", &Language::Python).unwrap();

    match result {
        ValidationResult::Skipped(reason) => {
            assert!(reason.contains("No") || reason.contains("code"));
        }
        _ => panic!("Should skip empty skill.md"),
    }
}

/// Test code block with language specifier variations
#[test]
fn test_code_block_language_variations() {
    // Test with "py" instead of "python"
    let skill_md = r#"
```py
import math
x = math.sqrt(4)
assert x == 2
```
"#;

    let validator = FunctionalValidator::new();

    // Should still extract code (implementation might need to handle "py" as well)
    let result = validator.validate(skill_md, &Language::Python);
    assert!(result.is_ok());
}
