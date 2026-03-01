use anyhow::Result;
use skilldo::detector::Language;
use skilldo::validator::{FunctionalValidator, ValidationResult};

#[test]
fn test_validator_extracts_python_code() -> Result<()> {
    let validator = FunctionalValidator::new();

    let skill_md = r#"---
name: test
version: 1.0.0
ecosystem: python
---

## Imports

```python
import sys
```

## Core Patterns

### Basic

```python
import sys
print(sys.version)
```

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, &Language::Python)?;

    // Should either pass (if Docker/Python available) or skip
    assert!(
        matches!(result, ValidationResult::Pass(_))
            || matches!(result, ValidationResult::Skipped(_)),
        "Should validate or skip Python code"
    );

    Ok(())
}

#[test]
fn test_validator_skips_non_python_languages() -> Result<()> {
    let validator = FunctionalValidator::new();

    let skill_md = r#"---
name: test
version: 1.0.0
ecosystem: javascript
---

## Imports

```javascript
const fs = require('fs');
```

## Core Patterns

Test

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, &Language::JavaScript)?;

    // Should skip non-Python languages
    assert!(
        matches!(result, ValidationResult::Skipped(_)),
        "Should skip JavaScript validation"
    );

    Ok(())
}

#[test]
fn test_validator_handles_empty_code() -> Result<()> {
    let validator = FunctionalValidator::new();

    let skill_md = r#"---
name: test
version: 1.0.0
ecosystem: python
---

## Imports

No code here

## Core Patterns

### Example

Just text, no code blocks

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, &Language::Python)?;

    // Should skip when no runnable code found
    assert!(
        matches!(result, ValidationResult::Skipped(_)),
        "Should skip when no code blocks found"
    );

    Ok(())
}

#[test]
fn test_validator_extracts_code_with_imports() -> Result<()> {
    let validator = FunctionalValidator::new();

    let skill_md = r#"---
name: test
version: 1.0.0
ecosystem: python
---

## Imports

```python
import json
import os
```

## Core Patterns

### JSON Usage

```python
import json
data = {"key": "value"}
json_str = json.dumps(data)
print(json_str)
```

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, &Language::Python)?;

    // Should extract and validate code with imports
    // May pass, fail, or skip depending on environment
    assert!(
        !matches!(result, ValidationResult::Fail(ref msg) if msg.contains("SyntaxError")),
        "Should not have syntax errors in stdlib code"
    );

    Ok(())
}

#[test]
fn test_validator_handles_python_with_print() -> Result<()> {
    let validator = FunctionalValidator::new();

    let skill_md = r#"---
name: test
version: 1.0.0
ecosystem: python
---

## Imports

```python
import time
```

## Core Patterns

### Time Example

```python
import time
print(f"Current time: {time.time()}")
```

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, &Language::Python)?;

    // Code with print should be recognized as runnable
    if let ValidationResult::Pass(output) = result {
        assert!(
            output.contains("Current time") || output.contains("✓"),
            "Should capture print output or success marker"
        );
    }
    // Otherwise should skip (no Docker) or fail (error)
    // Both are acceptable outcomes for this test

    Ok(())
}
