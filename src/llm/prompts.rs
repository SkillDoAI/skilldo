// Prompt templates for the 5 agents (v1 - kept for backwards compatibility)
#![allow(dead_code)]

pub fn agent1_api_extractor(package_name: &str, version: &str, source_code: &str) -> String {
    format!(
        r#"You are analyzing the source code of {} v{}.

Extract COMPLETE API surface with maximum detail for AI code generation:

For each public function/method/class:
- Full signature with parameter names, types, and defaults
- Return type with type hints
- Decorators applied (e.g., @hookimpl, @property, @staticmethod)
- Docstring (if present)
- Whether async/sync
- Module/file path
- Whether deprecated

For each public class:
- Class name and base classes
- __init__ signature with all parameters
- All public methods with full signatures
- Class attributes and their types
- Decorators on the class

For decorator/hook systems:
- Decorator names and signatures
- Hook/plugin registration methods
- Extension points

For configuration:
- Config class structures
- Environment variable names
- Configuration file formats
- Setup/initialization methods

CRITICAL: Include exact parameter names and types - AI agents need these to generate correct code.

Only include genuinely public APIs. Skip private helpers.

Output as JSON with this structure:
{{
  "functions": [{{"name": "...", "signature": "...", "module": "...", "decorators": [...], "async": bool}}],
  "classes": [{{"name": "...", "init_signature": "...", "methods": [...], "attributes": [...]}}],
  "decorators": [{{"name": "...", "usage": "..."}}],
  "config": [{{"method": "...", "signature": "..."}}]
}}

Source code:
{}
"#,
        package_name, version, source_code
    )
}

pub fn agent2_pattern_extractor(package_name: &str, version: &str, test_code: &str) -> String {
    format!(
        r#"You are reading the test suite for {} v{}.

These tests show CORRECT usage patterns written by maintainers.

Extract detailed patterns for AI code generation:

For each test pattern:
- API being tested (full function/class name)
- Complete usage pattern (setup → call → assertion)
- Parameter values used (actual names and types)
- Configuration required (env vars, files, initialization)
- Decorator patterns (@decorator usage)
- Hook implementations (if plugin system)
- Context managers (async with, with statements)
- Error handling patterns (try/except for expected errors)

Special focus areas:
- Decorator application patterns
- Plugin registration/hooks
- Configuration methods (CLI args, config files, env vars)
- Async/await usage
- Common parameter combinations
- Integration points (how to extend/customize)

Group by use case. Prioritize:
1. Minimal working example (simplest possible)
2. Common real-world patterns
3. Configuration/customization patterns
4. Extension/plugin patterns

Output as JSON:
{{
  "minimal_example": {{"code": "...", "apis_used": [...]}},
  "common_patterns": [{{"name": "...", "code": "...", "parameters": {{...}}}}],
  "decorator_patterns": [{{"decorator": "...", "usage": "..."}}],
  "config_patterns": [{{"method": "...", "example": "..."}}],
  "integration_patterns": [{{"extension_point": "...", "how_to": "..."}}]
}}

Test code:
{}
"#,
        package_name, version, test_code
    )
}

pub fn agent3_context_extractor(package_name: &str, version: &str, docs: &str) -> String {
    format!(
        r#"You are reading the documentation and changelog for {} v{}.

Extract detailed context for AI code generation:

1. INSTALLATION:
   - Package name for pip/npm/cargo
   - Any optional dependencies or extras
   - System requirements

2. ARCHITECTURE:
   - High-level design patterns (e.g., "async-first", "plugin-based")
   - Core abstractions (e.g., "uses context managers", "decorator-driven")
   - Threading/async model
   - State management approach

3. CONFIGURATION:
   - Environment variables used
   - Config file formats and locations
   - CLI argument patterns
   - Initialization/setup methods
   - What overrides what (precedence rules)

4. CONVENTIONS:
   - Best practices stated in docs
   - Recommended patterns
   - "The right way" to do things
   - Style guidelines

5. PITFALLS:
   - Common mistakes documented
   - Gotchas and surprises
   - Things that don't work as expected
   - Performance traps
   - Security issues
   - Look in: FAQ, troubleshooting, GitHub issues

6. BREAKING CHANGES:
   - What changed between recent versions
   - Deprecated APIs and replacements
   - Migration paths

7. TYPE INFORMATION:
   - Common data structures/types
   - Return types for key APIs
   - Parameter types

Be specific. Include actual API names, parameter names, and examples.

Output as JSON:
{{
  "installation": {{"command": "...", "extras": [...]}},
  "architecture": {{"design": "...", "patterns": [...]}},
  "configuration": [{{"method": "...", "format": "...", "precedence": "..."}}],
  "conventions": [{{"practice": "...", "reason": "..."}}],
  "pitfalls": [{{"mistake": "...", "correct": "...", "why": "..."}}],
  "breaking_changes": [{{"from_version": "...", "change": "...", "migration": "..."}}],
  "types": [{{"name": "...", "structure": "..."}}]
}}

Documentation:
{}
"#,
        package_name, version, docs
    )
}

pub fn agent4_synthesizer(
    package_name: &str,
    version: &str,
    ecosystem: &str,
    api_surface: &str,
    patterns: &str,
    context: &str,
) -> String {
    format!(
        r#"You are creating a SKILL.md file for {} v{}.

This file will be read by AI coding agents (Claude, Cursor, etc.) to generate correct code.

INPUTS:
1. PUBLIC API SURFACE: {}
2. USAGE PATTERNS: {}
3. CONTEXT: {}

Generate a SKILL.md with this EXACT structure:

---
name: {}
description: [one sentence - what it does and key capabilities]
version: {}
ecosystem: {}
license: MIT
---

IMPORTANT: If you can extract the actual license from context/docs, use that. Otherwise use MIT as default.

# {}

[2-3 sentence overview of what this library does and its design approach]

## Installation

```bash
pip install {}
# or with extras:
pip install {}[extra1,extra2]
```

## Core Imports

Show the most common import patterns. Group by use case if needed.

```python
# Most common (90% of users)
from {} import MainClass, main_function

# For advanced usage
from {}.submodule import AdvancedFeature

# Type hints (if library uses typing)
from typing import Dict, List
from {} import CustomType
```

## Quick Start

A minimal working example (5-10 lines) showing the simplest possible usage.

```python
# Example that works immediately
```

## API Signatures

Show the most important functions/classes with FULL signatures including parameter names, types, and defaults.

```python
class MainClass:
    """What it does."""

    def __init__(
        self,
        param1: str,
        param2: int = 10,
        optional: Optional[Config] = None
    ):
        """Initialize with required and optional parameters."""

@decorator
def main_function(
    arg: str,
    *,
    kwonly: bool = False,
    timeout: float = 30.0
) -> Result:
    """
    What this function does.

    Args:
        arg: Description
        kwonly: Description
        timeout: Description in seconds

    Returns:
        Result object with data
    """
```

Include decorators, async markers, type hints as they appear in the actual API.

## Core Patterns

Show 3-5 most common real-world usage patterns with complete working code.

### Pattern 1: [Most Common Use Case]

```python
# Complete example from minimal_example or common_patterns
# Must use ONLY APIs from api_surface
# Include setup, usage, cleanup if needed
```

### Pattern 2: [Configuration/Customization]

```python
# Show how to configure via CLI, file, env vars
# Include precedence rules if relevant
```

### Pattern 3: [Extension/Integration]

```python
# Show decorator usage, hooks, plugins if library supports it
# Include @decorator patterns
```

## Configuration

How to configure the library. Cover all methods:

**Environment Variables:**
- `VARIABLE_NAME`: What it controls (default: value)

**Config Files:**
```toml
[section]
key = "value"
```

**Programmatic:**
```python
config = Config(
    setting1="value",
    setting2=123
)
```

**Precedence:** CLI args > environment > config file > defaults

## Common Types

Key data structures and return types:

```python
class Result:
    """Returned by main_function()."""
    field1: str
    field2: int
    data: Dict[str, Any]

class Config:
    """Configuration object."""
    setting1: str
    setting2: int = 10
```

## Pitfalls

CRITICAL: This section is MANDATORY. Show 3-5 common mistakes with specific Wrong/Right examples.

### Wrong: [First specific mistake - use actual API names]

```python
# Code that looks right but fails/breaks
# Use actual code from pitfalls input
```

### Right: [Correct approach for first mistake]

```python
# Correct code showing the fix
# Explain why this works
```

### Wrong: [Second specific mistake - use actual API names]

```python
# Another common mistake
```

### Right: [Correct approach for second mistake]

```python
# The fix
```

### Wrong: [Third specific mistake - use actual API names]

```python
# Third common mistake
```

### Right: [Correct approach for third mistake]

```python
# The fix
```

Add more pitfalls if found in the pitfalls input (up to 5 total).

## CRITICAL RULES

1. **Every code example MUST use only APIs from the public API surface**
   - All imports match actual module structure
   - All function signatures match api_surface exactly
   - No placeholder names like "MyClass", "my_function"

2. **Every code example MUST be complete and runnable**
   - Includes all necessary imports
   - Shows required parameters with actual values
   - Uses correct syntax for the ecosystem

3. **Do NOT invent APIs that don't exist**
   - Cross-reference every API with api_surface
   - Use actual parameter names from signatures
   - Include actual decorators used

4. **Prefer patterns from actual test suite**
   - Adapt from patterns input
   - Show real-world usage, not toy examples

5. **Be specific about types and signatures**
   - Include parameter types from api_surface
   - Show return types
   - Note async/sync correctly

6. **Keep it concise** - top 10-15 most used APIs
   - Omit rarely-used features
   - Focus on 80% use cases

7. **No marketing language** - just facts and patterns
   - "Enables X" not "Powerful X"
   - "Returns Y" not "Beautifully returns Y"

BEFORE YOU OUTPUT, VERIFY INTERNALLY (do NOT include this checklist in output):
✓ License field present in frontmatter (MIT if unknown)
✓ At least 5 real APIs extracted from api_surface
✓ Core patterns use actual API names (not placeholders)
✓ All imports are real (from api_surface module names)
✓ API Signatures section has full function signatures with parameters
✓ Pitfalls section has 3-5 specific Wrong/Right examples (MANDATORY)
✓ No generic placeholder names used
✓ At least one complete working example in Quick Start
✓ Common Types section if library has custom data structures

NOW OUTPUT THE SKILL.MD (not the checklist above):
- Output the SKILL.md content directly
- DO NOT wrap in markdown code fences (```markdown)
- DO NOT wrap in any code blocks
- Start directly with the frontmatter (---)
"#,
        package_name,
        version,
        api_surface,
        patterns,
        context,
        package_name,
        version,
        ecosystem,
        package_name,
        package_name,
        package_name,
        package_name,
        package_name,
        package_name
    )
}

pub fn agent5_reviewer(
    package_name: &str,
    version: &str,
    api_surface: &str,
    rules: &str,
) -> String {
    format!(
        r#"You are reviewing a generated SKILL.md for {} v{}.

PUBLIC API SURFACE:
{}

GENERATED SKILL.md:
{}

COMPREHENSIVE QUALITY CHECKS:

1. **API Accuracy:**
   - Does any code example use an API NOT in the public API surface?
   - Are all function signatures correct (parameters, types, defaults)?
   - Are decorators shown correctly (@decorator from api_surface)?
   - Are async/sync patterns correct?
   - List any invented/hallucinated APIs

2. **Code Completeness:**
   - Are all code examples complete and runnable?
   - Do examples include necessary imports?
   - Are parameter values realistic (not just "value" or "string")?
   - Would these examples work if copy-pasted?

3. **Structure Compliance:**
   - Is frontmatter present with name, description, version, ecosystem, license?
   - Does it have Installation section?
   - Does it have Core Imports section?
   - Does it have Quick Start section?
   - Does it have API Signatures section with full function signatures?
   - Does it have Core Patterns section with examples?
   - Does it have Common Types section (if applicable)?
   - Does it have Pitfalls section with Wrong/Right examples?
   - Are there at least 3 pitfalls shown?

4. **Specificity:**
   - Are there placeholder names like "MyClass", "my_function", "value"?
   - Are parameter names specific (not generic)?
   - Are examples realistic (not toy examples)?

5. **Factual Correctness:**
   - Is anything wrong based on the API surface?
   - Are import paths correct for the ecosystem?
   - Are type hints correct?

6. **Tessl.io Quality Standards:**
   - Would this SKILL enable an LLM to generate correct code first try?
   - Are API signatures detailed enough (parameter names, types, defaults)?
   - Are decorator/hook patterns shown if library uses them?
   - Are configuration methods documented?
   - Is it specific enough to pass a rubric evaluation?

7. **Linter Compliance:**
   - License field present?
   - Required sections present (Imports, Core Patterns, Pitfalls)?
   - Code examples present (at least 3)?
   - Step-by-step structure with numbered list or clear workflow?
   - Minimum content length (1000+ chars)?

CRITICAL: If ANY code example uses an API that doesn't exist in api_surface, this is FAIL.

If ALL checks pass, output ONLY:
{{"status": "pass"}}

If ANY check fails, output:
{{
  "status": "fail",
  "issues": [
    {{"category": "api_accuracy|completeness|structure|specificity|factual|quality|linter", "description": "specific issue", "severity": "error|warning"}},
    ...
  ]
}}
"#,
        package_name, version, api_surface, rules
    )
}
