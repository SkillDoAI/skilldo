// Improved prompts based on analysis of FastAPI, Django, and Click

pub fn agent1_api_extractor_v2(
    package_name: &str,
    version: &str,
    source_code: &str,
    source_file_count: usize,
    custom_instructions: Option<&str>,
    overwrite: bool,
) -> String {
    // If overwrite mode and custom provided, use it directly
    if overwrite {
        if let Some(custom) = custom_instructions {
            return custom.to_string();
        }
    }

    // Add scale-aware hints for large libraries
    let scale_hint = if source_file_count > 2000 {
        "\n\n⚠️  **LARGE LIBRARY ALERT** (2000+ files)\n\
         This is a massive codebase. Focus on:\n\
         1. **Main entry points** - Look for top-level `__init__.py` files\n\
         2. **Most commonly used APIs** - Core functions/classes used in examples\n\
         3. **Skip implementation details** - Only extract public interfaces\n\
         4. **Prioritize __all__ exports** - These explicitly mark the public API\n"
    } else if source_file_count > 1000 {
        "\n\n📦 **LARGE LIBRARY** (1000+ files)\n\
         Focus on top-level public APIs and main entry points. Skip internal modules.\n"
    } else {
        ""
    };

    let mut prompt = format!(
        r#"You are analyzing the Python package "{}" v{} ({} source files).

Your job: Extract the complete public API surface.{}

## FIRST: Identify Library Category

Analyze the codebase and classify it:
- **web_framework** - Has routing decorators (@app.get, @route), request/response handling
- **orm** - Has Model base classes, query builders, database operations
- **cli** - Has command/argument decorators (@command, @option, argparse)
- **http_client** - Has HTTP method functions (get, post, put, delete)
- **async_framework** - Heavy use of async/await patterns
- **testing** - Assert helpers, fixtures, test runners
- **general** - None of the above

Include this in output: `"library_category": "web_framework"`

## What to Extract

For each public function/method/class:
1. **Name** - Full qualified name (module.Class.method)
2. **Type** - One of: function, method, classmethod, staticmethod, property, class, descriptor
3. **Signature** - See signature handling below
4. **Return Type** - From type hints or docstring
5. **Module/File** - Where it's defined
6. **Decorator Information** - Full stack with order and parameters
7. **Deprecation Status** - Detailed deprecation info

## Signature Handling

- If signature exceeds 120 characters, format on multiple lines
- For very complex signatures (>200 chars), extract key parameters only
- Mark truncated signatures with `"signature_truncated": true`
- Always include required parameters, mark optional ones clearly

Example:
```
"signature": "function(required_param: str, optional: int = 0, **kwargs)",
"signature_truncated": false
```

## Method Type Classification

Clearly distinguish:
- **function** - Module-level function
- **method** - Instance method (regular def in class)
- **classmethod** - Has @classmethod decorator
- **staticmethod** - Has @staticmethod decorator
- **property** - Has @property decorator
  - For properties, also include: `"has_setter": bool, "has_deleter": bool`
- **descriptor** - Has __get__, __set__, or __delete__ methods

## Type Hint Handling

### Complex Type Extraction
Handle these patterns:
- `Annotated[T, metadata]` → Extract both T and metadata separately
- `Union[A, B]` or `A | B` → List all variants
- `Optional[T]` → Mark as `"optional": true`
- `Generic[T]` → Extract type parameters
- `Callable[[Args], Return]` → Extract signature structure

For each parameter with type hints, include:
```json
"type_hints": {{
  "param_name": {{
    "base_type": "str",
    "is_optional": false,
    "default_value": null,
    "metadata": ["Query()"] // For Annotated types
  }}
}}
```

### Special Cases
- FastAPI: Extract Query(), Path(), Body(), Depends() from Annotated
- Pydantic: Extract Field() metadata
- Click: Extract Option(), Argument() metadata

## Public API Detection (CRITICAL - Prioritize This)

PRIORITY: Focus on extracting PUBLIC user-facing APIs, NOT internal utilities.

**How to identify PUBLIC APIs:**
- Check `__all__` exports in `__init__.py` → These are the official public API
- Top-level imports (e.g., `from library import MainClass`) → More public than submodules
- Documented in user-facing docs → Public
- Used in example code → Public
- Module paths with `.compat`, `.internal`, `._private`, `._impl` → INTERNAL, deprioritize

**Scoring system:**
For each API, assign a "publicity_score":
- `"high"` - In `__all__`, top-level import, documented (PREFER THESE)
- `"medium"` - In public module, documented but not in `__all__`
- `"low"` - In `.compat`, `.internal`, or underscore modules (DEPRIORITIZE)

Include in output:
```json
"publicity_score": "high",
"module_type": "public" // or "internal" or "compatibility"
```

**Example:**
- `library.MainClass` → publicity_score: "high" (top-level, in __all__)
- `library.compat.helper_function()` → publicity_score: "low" (internal compat layer)

**Extract both, but MARK internal APIs clearly** so downstream agents can prioritize correctly.

## Deprecation Tracking and Categorization

Look for deprecation signals:
- `@deprecated` decorator (hard deprecation if removal_version set)
- `warnings.warn()` calls with DeprecationWarning or FutureWarning
- Docstrings containing "deprecated", ".. deprecated::", "removal in"
- CHANGELOG mentions of "Breaking Changes" or "Removed"
- `raise` statements for removed APIs (fully removed)

**Categorize deprecation severity:**

1. **Soft Deprecation** - "Still okay to use for now"
   - Signals: DeprecationWarning without removal version, "discouraged", "prefer"
   - Removal timeline: >2 versions away or unspecified
   - Replacement may not be fully stable yet
   - Mark as: `"deprecation_severity": "soft"`

2. **Hard Deprecation** - "Move off of these"
   - Signals: FutureWarning, specific removal version, "will be removed in"
   - Removal timeline: 1-2 versions away
   - Replacement is stable and ready
   - Mark as: `"deprecation_severity": "hard"`

3. **Removed** - Already gone
   - Raises error when called
   - Mark as: `"deprecation_severity": "removed"`

Extract:
```json
"deprecation": {{
  "is_deprecated": true,
  "severity": "soft", // or "hard" or "removed"
  "since_version": "1.5.0",
  "removal_version": "3.0.0", // null if unspecified
  "replacement_api": "new_function_name", // exact API name
  "replacement_example": "new_function(param)", // how to use it
  "reason": "Brief explanation",
  "migration_note": "Still works fine, but prefer new API for new code"
}}
```

**Key differences:**
- Soft: `"migration_note": "Still safe to use, consider migrating when convenient"`
- Hard: `"migration_note": "Action required: Migrate before v3.0.0"`
- Removed: `"migration_note": "No longer available, must use replacement"`

## Decorator Stacks

Record ALL decorators in order (top to bottom):
```json
"decorators": [
  {{"name": "app.get", "params": {{"/items/{{{{item_id}}}}"}}}},
  {{"name": "requires_auth", "params": {{}}}}
]
```

## Class Hierarchies

For classes, include:
- Base classes (direct parents)
- Whether it's abstract (has ABCMeta or abstractmethod)
- Key metaclass info if relevant (e.g., Django models)

## Library-Specific Patterns

### Web Frameworks (FastAPI, Flask, Django)
Extract:
- Route decorators and paths
- HTTP methods
- Request/response type signatures
- Dependency injection patterns

### CLI Tools (Click, argparse)
Extract:
- Command decorators
- Argument/option decorators with all parameters
- Context parameter patterns
- Command groups and nesting

### ORMs (Django ORM, SQLAlchemy)
Extract:
- Model field definitions
- Query method signatures
- Relationship fields
- Manager methods

### HTTP Clients (requests, httpx)
Extract:
- HTTP method signatures
- Session methods
- Auth patterns
- Streaming methods

## Extraction Priorities (NOT Exclusions)

**HIGH PRIORITY - Extract these first:**
- APIs in `__all__` exports
- Top-level public APIs (e.g., `library.MainClass`)
- Well-documented user-facing classes/functions
- APIs used in official examples

**MEDIUM PRIORITY - Extract but mark as internal:**
- Compatibility layers (`.compat` modules)
- Internal utilities (`.internal`, `._utils` modules)
- Undocumented but potentially useful APIs

**LOW PRIORITY - Skip these:**
- Functions/classes starting with `_` (unless in `__all__`)
- Test utilities and fixtures
- Vendored third-party code
- Build/packaging code

**CRITICAL: For internal/compat APIs, mark them clearly:**
```json
{{
  "name": "library.compat.helper_function",
  "publicity_score": "low",
  "module_type": "compatibility",
  "note": "Internal compatibility utility, not primary public API"
}}
```

This allows downstream agents to **prioritize public APIs** in pattern selection.

## Output Format

Return JSON with this structure:
```json
{{
  "library_category": "general",
  "apis": [
    {{
      "name": "library.MainClass",
      "type": "class",
      "signature": "MainClass(debug: bool = False, config: Optional[Config] = None)",
      "signature_truncated": false,
      "return_type": "MainClass",
      "module": "library.core",
      "publicity_score": "high",
      "module_type": "public",
      "decorators": [],
      "deprecation": {{
        "is_deprecated": false
      }},
      "type_hints": {{
        "debug": {{
          "base_type": "bool",
          "is_optional": true,
          "default_value": "False"
        }},
        "config": {{
          "base_type": "Optional[Config]",
          "is_optional": true,
          "default_value": "None"
        }}
      }}
    }},
    {{
      "name": "library.compat.helper_function",
      "type": "function",
      "signature": "helper_function() -> bool",
      "signature_truncated": false,
      "return_type": "bool",
      "module": "library.compat",
      "publicity_score": "low",
      "module_type": "compatibility",
      "decorators": [],
      "deprecation": {{
        "is_deprecated": false
      }},
      "note": "Internal compatibility utility, not primary public API"
    }},
    {{
      "name": "library.deprecated.old_function",
      "type": "function",
      "signature": "old_function(data, options)",
      "signature_truncated": false,
      "return_type": "Result",
      "module": "library.deprecated",
      "publicity_score": "medium",
      "module_type": "public",
      "decorators": [],
      "deprecation": {{
        "is_deprecated": true,
        "severity": "removed",
        "since_version": "1.5.0",
        "removal_version": "2.0.0",
        "replacement_api": "library.new_function",
        "replacement_example": "library.new_function(data, options)",
        "reason": "Removed in v2.0, use new_function instead",
        "migration_note": "No longer available, must use replacement"
      }}
    }}
  ]
}}
```

Source code:
{}
"#,
        package_name, version, source_file_count, scale_hint, source_code
    );

    if let Some(custom) = custom_instructions {
        prompt.push_str(&format!("\n\n## Additional Instructions\n\n{}\n", custom));
    }

    prompt
}

pub fn agent2_pattern_extractor_v2(
    package_name: &str,
    version: &str,
    test_code: &str,
    custom_instructions: Option<&str>,
    overwrite: bool,
) -> String {
    if overwrite {
        if let Some(custom) = custom_instructions {
            return custom.to_string();
        }
    }

    let mut prompt = format!(
        r#"You are analyzing the test suite for Python package "{}" v{}.

Your job: Extract correct usage patterns from the tests.

## What to Extract

For each distinct usage pattern:
1. **API Being Tested** - Which function/class/method
2. **Setup Code** - Imports, initialization, configuration
3. **Usage Pattern** - The actual API call with parameters
4. **Assertions** - What's being verified (shows expected behavior)
5. **Test Infrastructure** - TestClient, fixtures, mocks used

## Key Testing Patterns to Recognize

### Test Clients & Runners
- `TestClient(app)` - FastAPI/Starlette
- `CliRunner().invoke()` - Click
- `self.client.get/post()` - Django
- Pytest fixtures

### Setup Methods
- `setUpTestData(cls)` - Django class-level setup
- `@pytest.fixture` - Pytest fixtures
- Context managers for resource cleanup

### Parametrized Tests
```python
@pytest.mark.parametrize("input,expected", [...])
def test_something(input, expected):
    ...
```
- Extract all parameter combinations
- Each is a distinct usage pattern

### Decorator Testing
- For decorator-heavy libraries (Click, FastAPI)
- Show decorator order and stacking
- Document context/object passing patterns

## Special Cases

### Async Patterns
```python
async def test_async():
    result = await async_function()
```
- Mark patterns as async
- Show proper await usage

### Dependency Injection
```python
@app.get("/items")
async def get_items(db = Depends(get_db)):
    ...
```
- Extract dependency patterns
- Show how dependencies are created/injected

### Error Handling
```python
def test_validation_error():
    response = client.post("/items", json={{"invalid": "data"}})
    assert response.status_code == 422
```
- Document expected error responses
- Show validation patterns

## Deprecation Awareness

If test code uses deprecated APIs, note it in the pattern:

**Markers of deprecated usage:**
- `warnings.simplefilter('ignore', DeprecationWarning)` in test setup
- Comments indicating deprecation or migration needed
- Test isolation of old vs new API versions
- Version-specific test skipping decorators

**Tag patterns using deprecated APIs:**
Add these fields to each pattern:
- `deprecation_status`: "current" | "soft" | "hard"
- `deprecation_note`: Brief migration guidance (if deprecated)

This allows downstream synthesis to properly label deprecated patterns with guidance.

## Output Format

Return JSON with pattern objects containing standard fields plus deprecation info when applicable.

Test code:
{}
"#,
        package_name, version, test_code
    );

    if let Some(custom) = custom_instructions {
        prompt.push_str(&format!("\n\n## Additional Instructions\n\n{}\n", custom));
    }

    prompt
}

pub fn agent3_context_extractor_v2(
    package_name: &str,
    version: &str,
    docs_and_changelog: &str,
    custom_instructions: Option<&str>,
    overwrite: bool,
) -> String {
    if overwrite {
        if let Some(custom) = custom_instructions {
            return custom.to_string();
        }
    }

    let mut prompt = format!(
        r#"You are analyzing documentation and changelog for Python package "{}" v{}.

Your job: Extract conventions, best practices, pitfalls, and migration notes.

## What to Extract

### 1. CONVENTIONS - Best Practices
- Recommended usage patterns
- Naming conventions
- Code organization guidelines
- Type hint requirements
- Async vs sync guidelines

### 2. PITFALLS - Common Mistakes

Structure each as:
```
Wrong: [bad pattern with code example]
Why it fails: [explanation]
Right: [correct pattern with code example]
```

Look for:
- Mutable default arguments
- Missing await on async functions
- Decorator order issues
- Context/scope problems
- Type validation gotchas

### 3. BREAKING CHANGES

For each breaking change:
- What changed (API signature, behavior, etc.)
- Affected versions (from X to Y)
- Migration path (how to update code)
- Deprecation warnings if any

### 4. MIGRATION NOTES

- Version upgrade guides
- Deprecated → Current API mapping
- Code examples showing before/after
- Database migrations (if applicable)

## Documentation Patterns to Recognize

### Docstring Styles
- ReStructuredText (`:param:`, `:returns:`)
- Google style
- NumPy style
- Plain markdown

### Code Examples
- Extract working examples
- Note which ones show pitfalls vs best practices
- Preserve exact syntax (indentation matters!)

### Warning Boxes
```
.. warning::
   Don't do X because Y
```
- These are high-value pitfalls!

### Changelog Entries
```
## 1.0.0 (2024-01-01)
### Breaking Changes
- Removed deprecated X, use Y instead
### Fixed
- Bug in Z that caused A
```

## Special Considerations

### Large Frameworks (Django-style)
- Settings configuration patterns
- Database backend differences
- Feature gates (what requires what)

### CLI Tools (Click-style)
- Command-line argument patterns
- Environment variable usage
- Configuration file formats

### Async Frameworks
- Async/await requirements
- Synchronous vs asynchronous endpoints
- Background task patterns

## Documented API Extraction (CRITICAL)

**Purpose**: Identify which APIs are officially documented (public) vs undocumented (internal).

**Where to look for documented APIs:**

1. **API Reference Sections**
   - Look for "API Reference", "API Documentation", "Reference Guide" headings
   - Function/class definitions with full signatures
   - Method listings under class documentation

2. **Sphinx/Autodoc Patterns** (Python docs)
   - `.. autofunction:: module.function_name`
   - `.. autoclass:: module.ClassName`
   - `.. automethod:: ClassName.method_name`
   - Module tables listing functions/classes

3. **Markdown Patterns**
   - Function headings: `### function_name(params)` or `## ClassName`
   - Code blocks showing imports: `from module import ClassName`
   - Documented examples in README

4. **Docstring References**
   - Any function/class/method shown in rendered documentation
   - Parameter descriptions indicate it's documented
   - Return type documentation

5. **Import Examples**
   - `from package import Class, function` → Extract "Class" and "function"
   - `import package.module` → Extract what's used from that module in examples

**What to extract:**
- Fully qualified names when possible: `module.ClassName`, `module.function_name`
- For imports like `from module import X`, extract just `X`
- Include both classes AND their documented methods: `ClassName.method_name`
- Top-level functions: `function_name`

**What NOT to extract:**
- Functions/classes only mentioned in passing
- Internal implementation details not shown in API reference
- Private methods (usually starting with `_`)

**Critical**: Be thorough - extract ALL documented APIs. Missing documented APIs means we'll incorrectly filter them out.

## Output Format

Return JSON:
```json
{{
  "documented_apis": [
    "ClassName",
    "function_name",
    "module.ClassName",
    "ClassName.method_name"
  ],
  "conventions": [
    "Use async def for I/O operations",
    "Type hints required for validation"
  ],
  "pitfalls": [
    {{
      "category": "Async handling",
      "wrong": "Missing await on async call",
      "why": "Async functions need await",
      "right": "Use await with async functions"
    }}
  ],
  "breaking_changes": [
    {{
      "version_from": "0.95.0",
      "version_to": "1.0.0",
      "change": "API signature changed",
      "migration": "Update code to new signature"
    }}
  ],
  "migration_notes": "See CHANGELOG.md for migration guide"
}}
```

Documentation and changelog:
{}
"#,
        package_name, version, docs_and_changelog
    );

    if let Some(custom) = custom_instructions {
        prompt.push_str(&format!("\n\n## Additional Instructions\n\n{}\n", custom));
    }

    prompt
}

#[allow(clippy::too_many_arguments)]
pub fn agent4_synthesizer_v2(
    package_name: &str,
    version: &str,
    license: Option<&str>,
    project_urls: &[(String, String)],
    ecosystem: &str,
    api_surface: &str,
    patterns: &str,
    context: &str,
    custom_instructions: Option<&str>,
    overwrite: bool,
) -> String {
    // If overwrite mode and custom provided, use it directly
    if overwrite {
        if let Some(custom) = custom_instructions {
            return custom.to_string();
        }
    }

    // Format references section
    let references = if project_urls.is_empty() {
        "- [Official Documentation](search for official docs)\n- [GitHub Repository](search for GitHub repo)".to_string()
    } else {
        project_urls
            .iter()
            .map(|(name, url)| format!("- [{}]({})", name, url))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let mut prompt = format!(
        r#"You are creating an agent rules file for Python package "{}" v{}.

This file helps AI coding agents write correct code using this library.

## Inputs Provided

1. **PUBLIC API SURFACE**: {}
2. **USAGE PATTERNS FROM TESTS**: {}
3. **CONVENTIONS & PITFALLS**: {}

## Output Structure

Generate a SKILL.md file with EXACTLY these sections:

```markdown
---
name: {}
description: [one clear sentence describing the library]
version: {}
ecosystem: {}
license: {}
---

## Imports

Show the standard import patterns. Most common first:
```python
from {} import MainClass, helper_function
from {}.submodule import SpecialCase
```

## Core Patterns

The right way to use the main APIs. Show 3-5 most common patterns.

**CRITICAL: Prioritize PUBLIC APIs over internal/compat modules**
- Use APIs from api_surface with `publicity_score: "high"` first
- Avoid `.compat`, `.internal`, `._private` modules unless they're the only option
- Example: Prefer `library.MainClass` over `library.compat.helper_function`

**CRITICAL: Mark deprecation status with clear indicators**

Each pattern must include:
1. **Status indicator** in the heading:
   - `✅ Current` - Modern, stable API (use this for new code)
   - `⚠️ Soft Deprecation` - Still safe to use, but newer option available
   - `❌ Hard Deprecation` - Will be removed soon, migrate required
   - `🗑️ Removed` - No longer available, use replacement

2. **Deprecation guidance** (if deprecated):
   - For soft: "Still works fine. Don't rewrite existing code - only use current API for new projects."
   - For hard: "Action required: Migrate before vX.X. Include replacement API name."
   - For removed: "No longer available since vX.X. Include replacement API name."

**Pattern Format:**

### Pattern Name ✅ Current
```python
# Modern, recommended code
```
* Description of what it does
* **Status**: Current, stable

### Legacy Pattern ⚠️ Soft Deprecation
```python
# Older code that still works
```
* Description of what it does
* **Deprecated since**: vX.X
* **Still works**: Yes, safe to use in existing code
* **Guidance**: Don't rewrite working code - only use current API for new code
* **Modern alternative**: Use current API name

### Old Pattern ❌ Hard Deprecation
```python
# Code that will stop working soon
```
* Description of what it does
* **Deprecated since**: vX.X
* **Removal planned**: vX.X
* **Replacement**: Use replacement API name with example
* **Action required**: Migrate existing code before removal

### Removed API 🗑️ Removed
```python
# This no longer works - shown for reference only
```
* **Removed in**: vX.X
* **Replacement**: Use replacement API name with example
* **Why**: Brief reason for removal

Each pattern:
- Clear heading with status indicator
- Complete, runnable code example
- Comments explaining key points
- Type hints if the library uses them
- Deprecation guidance if applicable

## Configuration

Standard configuration and setup:
- Default values
- Common customizations
- Environment variables
- Config file formats

## Pitfalls

CRITICAL: This section is MANDATORY. Show 3-5 common mistakes with specific Wrong/Right examples.

### Wrong: [First specific mistake - use actual API names]
```python
# Code that looks right but fails
```

### Right: [Correct approach for first mistake]
```python
# The fix with explanation
```

### Wrong: [Second specific mistake]
```python
# Another common mistake
```

### Right: [Correct approach for second mistake]
```python
# The fix
```

### Wrong: [Third specific mistake]
```python
# Third common mistake
```

### Right: [Correct approach for third mistake]
```python
# The fix
```

Add 2 more pitfalls if found in the context (minimum 3, maximum 5 total).

## References

CRITICAL: Include ALL provided URLs below (do NOT skip this section):

{}

## Migration from v[previous]

What changed in this version (if applicable):
- Breaking changes
- Deprecated → Current mapping
- Before/after code examples

## API Reference

Brief reference of the most important public APIs:

- **ClassName()** - Constructor with key parameters
- **method_name()** - What it does, key parameters
- **@decorator_name** - When to use it

Focus on the 10-15 most used APIs.
```

## LIBRARY-SPECIFIC SECTIONS

Based on the library category from api_surface, include appropriate sections:

### For Web Frameworks (FastAPI, Flask, Django)
**REQUIRED sections:**
- Routing Patterns - Show route decorators with different HTTP methods
- Request Handling - Query params, path params, body parsing
- Response Handling - Status codes, headers, JSON/HTML responses
- Middleware/Dependencies - Dependency injection patterns
- Error Handling - Exception handlers, HTTP exceptions
- Background Tasks - If supported
- WebSocket Patterns - If supported

**Core Patterns must show:**
```python
# Route with path parameter
@app.get("/items/{{item_id}}")
def read_item(item_id: int):
    return {{"item_id": item_id}}

# POST with body
@app.post("/items/")
def create_item(item: ItemModel):
    return item

# Dependency injection
@app.get("/protected")
def protected(user = Depends(get_current_user)):
    return {{"user": user}}
```

### For CLI Tools (Click, argparse)
**REQUIRED sections:**
- Command Definition - Command decorators and functions
- Arguments vs Options - When to use each
- Context Passing - Click context, argparse Namespace
- Command Groups - Nesting and organization
- Error Handling - Exit codes and error messages
- Configuration - Config file support if present

**Core Patterns must show:**
```python
# Basic command
@click.command()
@click.option('--count', default=1)
@click.argument('name')
def greet(count, name):
    for _ in range(count):
        click.echo(f'Hello {{name}}!')

# Command group
@click.group()
def cli():
    pass

@cli.command()
def command1():
    pass
```

### For ORMs (Django ORM, SQLAlchemy)
**REQUIRED sections:**
- Model Definition - Field types and constraints
- Query Patterns - filter, get, create, update, delete
- Relationships - ForeignKey, ManyToMany patterns
- Transaction Management - Atomic operations
- Query Optimization - select_related, prefetch_related
- Migration Patterns - If applicable

**Core Patterns must show:**
```python
# Model definition
class User(Model):
    name = CharField(max_length=100)
    email = EmailField(unique=True)

# Query patterns
users = User.objects.filter(name__startswith='A')
user = User.objects.get(id=1)
User.objects.create(name='Alice', email='a@example.com')

# Relationships
class Post(Model):
    author = ForeignKey(User, on_delete=CASCADE)
```

### For HTTP Clients (requests, httpx)
**REQUIRED sections:**
- HTTP Methods - GET, POST, PUT, DELETE with examples
- Request Parameters - Query params, headers, body
- Response Handling - Status codes, JSON, content
- Session Management - Persistent sessions
- Authentication - Auth patterns supported
- Timeout and Retry - Error handling
- Streaming - If supported

**Core Patterns must show:**
```python
# GET request
response = requests.get('https://api.example.com/users')
data = response.json()

# POST with JSON
response = requests.post('https://api.example.com/users',
                        json={{"name": "Alice"}})

# Session with auth
session = requests.Session()
session.auth = ('user', 'pass')
session.get('https://api.example.com/protected')
```

### For Async Frameworks
**REQUIRED sections:**
- Async/Await Basics - When to use async def
- Concurrency Patterns - gather, create_task
- Synchronous Wrappers - run_in_executor for blocking code
- Event Loop Management - get_event_loop patterns
- Background Tasks - Fire and forget patterns

**Core Patterns must show:**
```python
# Async function
async def fetch_data():
    async with httpx.AsyncClient() as client:
        response = await client.get('https://api.example.com')
        return response.json()

# Concurrent operations
results = await asyncio.gather(
    fetch_data(),
    fetch_data(),
)

# Running blocking code
result = await loop.run_in_executor(None, blocking_function)
```

---
END OF SKILL.MD TEMPLATE ABOVE
---

NOW FOLLOW THESE RULES (do NOT include this section in your output):

1. **Prefer APIs that are in context.documented_apis list**
   - Cross-reference every API you use against the documented_apis from context
   - If an API is in api_surface but NOT in documented_apis, skip it (it's undocumented/internal)
   - If documented_apis is empty or very small, use the api_surface and patterns to identify the most important public APIs. For well-known libraries, you may use your knowledge of their public API to supplement
   - This ensures SKILL.md documents officially public APIs

2. **Log skipped undocumented APIs internally** (don't include in output)
   - When you skip an API because it's not documented, note it mentally
   - Example: "Skipping library.compat.helper - not in documented_apis"
   - Continue with documented APIs only

3. **Every code example MUST use real APIs**
   - If api_surface is empty or doesn't contain valid APIs, use patterns and your knowledge of the library to identify real public APIs
   - NEVER use placeholder names like "MyClass", "my_function"
   - ALWAYS use actual API names from documented_apis, api_surface, or well-known public APIs of the library

3. **Every code example MUST be complete and runnable** (valid Python syntax)
   - Include all necessary imports
   - Show required parameters
   - Use correct indentation

4. **Mark deprecation status correctly with indicators**
   - Check api_surface and patterns for deprecation info
   - Use ✅ for current APIs, ⚠️ for soft deprecation, ❌ for hard deprecation, 🗑️ for removed
   - Include migration guidance for deprecated APIs
   - Never say "don't use" for soft deprecations - say "still okay to use, prefer new API for new code"

5. **Do NOT invent APIs that don't exist**
   - Cross-reference every API used against api_surface
   - If unsure, omit the example rather than guess

6. **Prefer patterns from the actual test suite**
   - Use patterns input as primary source of truth
   - Adapt test patterns into user-facing examples

5. **Keep it concise** - agents have limited context windows
   - Focus on top 10-15 most used APIs
   - Omit rarely-used features

6. **No marketing language** - just facts and patterns
   - No "powerful", "easy", "simple" adjectives
   - State what it does, not how good it is

7. **Type hints required** if the library uses them
   - Match type hints from api_surface exactly
   - Show complex types (Annotated, Union, etc.) correctly

8. **Show async/await properly** for async libraries
   - NEVER forget await on async function calls
   - Mark async contexts clearly

9. **Document decorator order** for decorator-heavy libraries
   - Order matters! Show from top to bottom
   - Include decorator parameters

10. **Include error handling** if it's a common pattern
    - Show try/except for expected errors
    - Document exception types

VERIFY INTERNALLY BEFORE OUTPUT (do NOT include this checklist in output):
- [ ] Library category identified from api_surface
- [ ] documented_apis or api_surface provides enough APIs to generate useful content
- [ ] Every API used is real and public (from documented_apis, api_surface, or well-known public API)
- [ ] At least 5 public APIs documented (from any reliable source)
- [ ] Core patterns use actual API names (not placeholders)
- [ ] All imports match documented_apis entries
- [ ] Deprecation status marked with correct indicators (✅ ⚠️ ❌ 🗑️)
- [ ] Deprecated patterns include migration guidance
- [ ] Soft deprecations say "still okay to use" not "don't use"
- [ ] Pitfalls section has 3-5 specific examples
- [ ] No generic framework names used

If validation issues exist, do your BEST to generate useful content anyway. Only output an error if you truly have ZERO information about the library's API (no api_surface, no patterns, no documented_apis, and you don't recognize the library).

Now generate the SKILL.md content:
"#,
        package_name,
        version,
        api_surface,
        patterns,
        context,
        package_name,
        version,
        ecosystem,
        license.unwrap_or("MIT"),
        package_name,
        package_name,
        references
    );

    if let Some(custom) = custom_instructions {
        prompt.push_str(&format!(
            "\n## CUSTOM INSTRUCTIONS FOR THIS REPO\n\n{}\n",
            custom
        ));
    }

    prompt
}

/// Update prompt for Agent 4: patches an existing SKILL.md with new data
pub fn agent4_update_v2(
    package_name: &str,
    version: &str,
    existing_skill: &str,
    api_surface: &str,
    patterns: &str,
    context: &str,
) -> String {
    format!(
        r#"You are updating an existing SKILL.md for "{}" to version {}.

## Existing SKILL.md (preserve everything that's still correct)

{}

## Current Library State (from source analysis)

### API Surface
{}

### Usage Patterns
{}

### Documentation & Changelog
{}

## Instructions

1. Keep all code patterns that are still valid — do NOT rewrite working examples
2. Update version in frontmatter to {}
3. If APIs changed signatures, update the code examples to match the current API
4. Add deprecation markers (⚠️) where the changelog indicates deprecations
5. Add a Migration section if there are breaking changes from the previous version
6. Add new patterns ONLY if significant new APIs were added
7. Remove patterns for APIs that were completely removed
8. Update the API Reference section if signatures changed
9. Keep the same structure, formatting, and style as the existing file
10. Do NOT invent APIs — only use what appears in the API surface above

Output the complete updated SKILL.md:
"#,
        package_name, version, existing_skill, api_surface, patterns, context, version
    )
}

#[allow(dead_code)]
pub fn agent5_reviewer_v2(
    package_name: &str,
    version: &str,
    api_surface: &str,
    rules: &str,
) -> String {
    format!(
        r#"You are reviewing a generated SKILL.md for Python package "{}" v{}.

## Known Public API Surface
{}

## Generated Rules File
{}

## Review Checklist

### 1. API Accuracy (CRITICAL)
- [ ] Does any code example reference an API NOT in the public API surface?
  - List EVERY hallucinated API with line number
  - Check imports, class names, function names, decorators
- [ ] Are all API signatures correct?
  - Parameter names must match exactly
  - Type hints must match exactly
  - Default values must match exactly
- [ ] Are type hints accurate?
  - Complex types (Annotated, Union, etc.) handled correctly?

### 2. Code Completeness (CRITICAL)
- [ ] Can each code example run standalone without modification?
  - All imports present?
  - All required parameters included?
  - Valid Python syntax?
- [ ] No placeholder names used?
  - NO "MyClass", "my_function", "example_app"
  - Only actual API names from api_surface

### 3. Library-Specific Validation
- [ ] Does the SKILL.md match the library category?
  - Web framework MUST show routing examples
  - CLI tool MUST show command/argument decorators
  - ORM MUST show model and query examples
  - HTTP client MUST show request methods
- [ ] Are library-specific patterns shown?
  - Not generic code that could be any library

### 4. Pattern Correctness
- [ ] Do async functions use await properly?
  - EVERY async call must have await
- [ ] Are decorators in the right order?
  - Top to bottom order matters
- [ ] Is error handling shown correctly?
  - Exception types match library
- [ ] Are type hints used correctly?
  - Match the library's type hint style

### 5. Pitfalls Section (CRITICAL)
- [ ] Do "Wrong" examples actually demonstrate the problem?
  - Show real mistakes developers make
- [ ] Do "Right" examples actually solve it?
  - Show working corrected code
- [ ] Are explanations clear?
  - Explain WHY it's wrong and WHY the fix works
- [ ] At least 3 pitfalls present?
  - Fewer than 3 = FAIL

### 6. Factual Accuracy
- [ ] Is anything contradicted by the API surface?
  - Cross-check every statement
- [ ] Are import paths correct?
  - Must match module names from api_surface
- [ ] Are default values accurate?
  - Must match api_surface signatures
- [ ] Are version-specific features noted?
  - Deprecation warnings if present

### 7. Completeness
- [ ] Are the top 10 most-used APIs covered?
  - Not obscure edge cases
- [ ] Are async patterns shown if library is async?
- [ ] Are error handling patterns shown?
- [ ] Is configuration shown if needed?

## STRICT FAILURE CRITERIA

MUST FAIL the review if ANY of these are true:
- ANY API used that is NOT in api_surface
- ANY import path that doesn't match api_surface modules
- ANY code example with syntax errors
- Generic placeholder names used (MyClass, example_app, etc.)
- Pitfalls section has fewer than 3 examples
- Wrong decorator order for the library's decorators
- Async function missing await
- Web framework without routing examples
- CLI tool without command decorators
- ORM without model/query examples

## Output Format

If ALL checks pass:
```json
{{"status": "pass"}}
```

If ANY fail:
```json
{{
  "status": "fail",
  "issues": [
    {{
      "type": "hallucinated_api",
      "location": "Core Patterns section, example 2",
      "problem": "Uses `FastAPI.create()` which doesn't exist",
      "fix": "Use `FastAPI()` constructor instead"
    }},
    {{
      "type": "incomplete_code",
      "location": "Dependency Injection example",
      "problem": "Missing import for Depends",
      "fix": "Add: from fastapi import Depends"
    }},
    {{
      "type": "incorrect_syntax",
      "location": "Async example",
      "problem": "Missing await before async database call",
      "fix": "Change `db.query()` to `await db.query()`"
    }}
  ]
}}
```

Review the SKILL.md now:
"#,
        package_name, version, api_surface, rules
    )
}
