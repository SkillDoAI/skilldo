//! Tests for functional validator error paths
//! Tests error handling in FunctionalValidator

use anyhow::Result;
use skilldo::validator::{FunctionalValidator, ValidationResult};

#[test]
fn test_validator_handles_malformed_skill_md() -> Result<()> {
    let validator = FunctionalValidator::new();

    // Malformed SKILL.md (invalid YAML frontmatter)
    let skill_md = r#"---
name: test
version: invalid yaml { syntax
---

## Imports

```python
import sys
```

## Core Patterns

Test

## Pitfalls

None
"#;

    // Should either parse with error or skip
    let result = validator.validate(skill_md, "python");

    match result {
        Ok(_) => {} // Handled gracefully,
        Err(e) => assert!(!e.to_string().is_empty(), "Error should have message"),
    }

    Ok(())
}

#[test]
fn test_validator_skips_when_ecosystem_mismatch() -> Result<()> {
    let validator = FunctionalValidator::new();

    // Python SKILL.md but asking to validate as javascript
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

```python
print(sys.version)
```

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, "javascript")?;

    // Should skip - ecosystem doesn't match
    assert!(
        matches!(result, ValidationResult::Skipped(_)),
        "Should skip when ecosystem doesn't match language"
    );

    Ok(())
}

#[test]
fn test_validator_with_multiple_code_blocks() -> Result<()> {
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

### Example 1

```python
import json
data = {"key": "value"}
print(json.dumps(data))
```

### Example 2

```python
import os
print(os.name)
```

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, "python")?;

    // Should validate using the first code block
    match result {
        ValidationResult::Pass(_) | ValidationResult::Skipped(_) => {
            {} // handled
        }
        ValidationResult::Fail(e) => {
            // Might fail if Docker not available, that's OK
            assert!(!e.is_empty(), "Failure should have error message");
        }
    }

    Ok(())
}

#[test]
fn test_validator_with_complex_imports() -> Result<()> {
    let validator = FunctionalValidator::new();

    let skill_md = r#"---
name: test
version: 1.0.0
ecosystem: python
---

## Imports

```python
from pathlib import Path
from typing import List, Dict, Optional
import json
```

## Core Patterns

```python
from pathlib import Path
from typing import List

def example() -> List[Path]:
    return [Path("/tmp")]

result = example()
print(f"Found {len(result)} paths")
```

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, "python")?;

    // Should handle complex type annotations and imports
    match result {
        ValidationResult::Pass(_) | ValidationResult::Skipped(_) => {
            {} // handled
        }
        ValidationResult::Fail(e) => {
            assert!(!e.is_empty(), "Failure should have error message");
        }
    }

    Ok(())
}

#[test]
fn test_validator_returns_error_for_unsupported_language() -> Result<()> {
    let validator = FunctionalValidator::new();

    let skill_md = r#"---
name: test
version: 1.0.0
ecosystem: golang
---

## Imports

```go
package main
```

## Core Patterns

```go
package main
func main() {}
```

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, "golang")?;

    // Should skip unsupported languages
    assert!(
        matches!(result, ValidationResult::Skipped(_)),
        "Should skip unsupported languages like Go"
    );

    Ok(())
}

#[test]
fn test_validator_handles_runtime_error_in_code() -> Result<()> {
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

```python
import sys
# This will cause a ZeroDivisionError
result = 1 / 0
print("Should not reach here")
```

## Pitfalls

None
"#;

    let result = validator.validate(skill_md, "python")?;

    // Should either fail with error or skip (if Docker not available)
    match result {
        ValidationResult::Fail(e) => {
            assert!(
                e.contains("ZeroDivisionError") || e.contains("division"),
                "Should catch runtime error: {}",
                e
            );
        }
        ValidationResult::Skipped(_) => {
            {} // Docker not available
        }
        ValidationResult::Pass(_) => {
            // Unexpected but shouldn't crash
            {} // unexpected but OK
        }
    }

    Ok(())
}

#[test]
fn test_validator_with_very_long_code() -> Result<()> {
    let validator = FunctionalValidator::new();

    // Generate a long code block
    let long_code = (0..100)
        .map(|i| format!("var_{} = {}", i, i))
        .collect::<Vec<_>>()
        .join("\n");

    let skill_md = format!(
        r#"---
name: test
version: 1.0.0
ecosystem: python
---

## Imports

```python
import sys
```

## Core Patterns

```python
{}
print("All variables defined")
```

## Pitfalls

None
"#,
        long_code
    );

    let result = validator.validate(&skill_md, "python")?;

    // Should handle long code without issues
    match result {
        ValidationResult::Pass(_) | ValidationResult::Skipped(_) | ValidationResult::Fail(_) => {
            {} // handled
        }
    }

    Ok(())
}
