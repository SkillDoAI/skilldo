use std::fs;
use std::path::Path;

use anyhow::{bail, Result};

use crate::lint::{Severity, SkillLinter};

pub fn run(path: &str) -> Result<()> {
    let file = Path::new(path);
    if !file.exists() {
        bail!("File not found: {}", path);
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
