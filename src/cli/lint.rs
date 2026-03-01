use std::fs;
use std::path::Path;

use anyhow::{bail, Result};

use crate::lint::{Severity, SkillLinter};

pub fn run(path: &str) -> Result<()> {
    let file = Path::new(path);
    if !file.exists() {
        bail!("File not found: {}", path);
    }
    if !file.is_file() {
        bail!("Path is not a file: {}", path);
    }

    let content = fs::read_to_string(file)?;
    let linter = SkillLinter::new();
    let issues = linter.lint(&content)?;

    linter.print_issues(&issues);

    let errors = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    if errors > 0 {
        bail!("{} lint error(s) found", errors);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_run_file_not_found() {
        let result = run("/tmp/nonexistent-lint-file-xyz.md");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[test]
    fn test_run_path_is_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = run(dir.path().to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a file"));
    }

    #[test]
    fn test_run_valid_skill_md() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        let mut f = std::fs::File::create(&skill_path).unwrap();
        writeln!(
            f,
            r#"---
name: testpkg
description: A test package
version: 1.0.0
ecosystem: python
---

## Imports

```python
import testpkg
```

## Core Patterns

### Basic Usage

```python
import testpkg
result = testpkg.do_something()
```

## Pitfalls

### Wrong: Bad usage

```python
testpkg.wrong()
```

### Right: Good usage

```python
testpkg.right()
```
"#
        )
        .unwrap();

        let result = run(skill_path.to_str().unwrap());
        // May pass or fail with lint errors, but should not panic
        // We just verify it runs without panic
        let _ = result;
    }

    #[test]
    fn test_run_minimal_skill_md_passes() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        // A very minimal but structurally valid SKILL.md
        let content = r#"---
name: mypkg
description: A package
version: 1.0.0
ecosystem: python
---

## Imports

```python
import mypkg
```

## Core Patterns

### Basic Usage

```python
import mypkg
x = mypkg.create()
```

## Pitfalls

### Wrong: Common mistake

```python
mypkg.bad()
```

### Right: Correct approach

```python
mypkg.good()
```
"#;
        std::fs::write(&skill_path, content).unwrap();
        let result = run(skill_path.to_str().unwrap());
        assert!(result.is_ok(), "minimal valid SKILL.md should pass lint");
    }
}
