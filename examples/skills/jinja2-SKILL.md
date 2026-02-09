---
name: jinja2
description: Template engine for generating text (commonly HTML) from templates and Python data.
version: 3.2.0
ecosystem: python
license: BSD-3-Clause
---

## Imports

```python
from importlib.metadata import version

from jinja2 import (
    Environment,
    FileSystemBytecodeCache,
    FileSystemLoader,
    PackageLoader,
    Undefined,
    pass_context,
    pass_environment,
    pass_eval_context,
)
from jinja2.filters import attr, int as jinja_int, select, unique, urlize, xmlattr
from markupsafe import Markup, escape
```

## Core Patterns

### Create an Environment with a loader ✅ Current
```python
from __future__ import annotations

from pathlib import Path

from jinja2 import Environment, FileSystemLoader, Undefined


def build_env(template_dir: str | Path = "templates") -> Environment:
    # Put templates under ./templates (common convention)
    loader = FileSystemLoader(str(template_dir))

    env = Environment(
        loader=loader,
        autoescape=True,          # enable escaping for HTML-ish output
        undefined=Undefined,      # default: renders as empty string when undefined
        trim_blocks=True,         # optional whitespace control
        lstrip_blocks=True,       # optional whitespace control
    )
    return env


if __name__ == "__main__":
    env = build_env()
    template = env.from_string("Hello {{ user }}!")
    print(template.render(user="Ada"))
```
* Creates a reusable `jinja2.Environment` that loads templates from disk.
* **Status**: Current, stable

### Overlay an Environment for per-request overrides ✅ Current
```python
from __future__ import annotations

from jinja2 import Environment, FileSystemLoader


def make_base_env() -> Environment:
    return Environment(loader=FileSystemLoader("templates"), autoescape=True)


def render_with_request_overrides(env: Environment, *, debug: bool) -> str:
    # overlay() copies the environment with selected overrides
    request_env = env.overlay(
        autoescape=True,
        trim_blocks=not debug,
        lstrip_blocks=not debug,
    )
    template = request_env.from_string("{% if debug %}DEBUG{% else %}OK{% endif %}")
    return template.render(debug=debug)


if __name__ == "__main__":
    base = make_base_env()
    print(render_with_request_overrides(base, debug=True))
    print(render_with_request_overrides(base, debug=False))
```
* Uses `Environment.overlay` to avoid mutating global configuration while customizing behavior.
* **Status**: Current, stable

### Async template rendering (avoid sync render in an active event loop) ✅ Current
```python
from __future__ import annotations

import asyncio
from jinja2 import Environment


async def main() -> None:
    env = Environment(enable_async=True, autoescape=True)

    template = env.from_string("Hello {{ user }}!")
    # In async contexts, use the async API.
    html = await template.render_async(user="Ada")
    print(html)


if __name__ == "__main__":
    asyncio.run(main())
```
* Renders an async-enabled template using `Template.render_async` (preferred in async apps).
* **Status**: Current, stable

### Stream output from an async generator ✅ Current
```python
from __future__ import annotations

import asyncio
from typing import List

from jinja2 import Environment


async def main() -> None:
    env = Environment(enable_async=True, autoescape=True)
    template = env.from_string("{% for x in xs %}{{ x }} {% endfor %}")

    parts: List[str] = []
    async for chunk in template.generate_async(xs=[1, 2, 3]):
        parts.append(chunk)

    print("".join(parts).strip())


if __name__ == "__main__":
    asyncio.run(main())
```
* Uses `Template.generate_async` to stream output (useful for large renders).
* **Status**: Current, stable

### Bytecode cache for faster template compilation ✅ Current
```python
from __future__ import annotations

from jinja2 import Environment, FileSystemBytecodeCache, FileSystemLoader


def build_cached_env() -> Environment:
    cache = FileSystemBytecodeCache(directory=".jinja2_bytecode_cache")
    env = Environment(
        loader=FileSystemLoader("templates"),
        autoescape=True,
        bytecode_cache=cache,
    )
    return env


if __name__ == "__main__":
    env = build_cached_env()
    template = env.from_string("Cached: {{ n }}")
    print(template.render(n=1))
```
* Uses `FileSystemBytecodeCache` to store compiled template bytecode on disk.
* **Status**: Current, stable

### Read installed version (avoid deprecated __version__) ⚠️ Soft Deprecation
```python
from __future__ import annotations

from importlib.metadata import version


def jinja_version() -> str:
    # Prefer feature detection over version checks when possible.
    return version("jinja2")


if __name__ == "__main__":
    print(jinja_version())
```
* Uses `importlib.metadata.version('jinja2')` when you truly need the installed version.
* **Deprecated since**: v3.2.0 (for `jinja2.__version__`, not for `importlib.metadata.version`)
* **Still works**: Yes, safe to use in existing code
* **Guidance**: Still works fine. Don't rewrite existing code - only use current API for new projects.
* **Modern alternative**: Prefer feature detection (e.g., `hasattr(Environment, "overlay")`) instead of version checks.

## Configuration

- **Template loaders**
  - `FileSystemLoader("templates")` for file-based templates.
  - `PackageLoader("your_package", "templates")` for templates shipped inside a Python package.
- **Autoescaping**
  - Set `Environment(autoescape=True)` to escape variables by default (typical for HTML output).
  - If your project uses extension-based autoescape rules, keep consistent naming (e.g. `user.html.jinja`) so tooling and configuration align.
- **Whitespace control**
  - `trim_blocks=True` and `lstrip_blocks=True` reduce extra whitespace.
  - In templates, use `{%- ... -%}` to strip whitespace around a specific tag.
  - Use `+` modifiers per-block to disable trimming behavior when enabled globally.
- **Undefined handling**
  - `undefined=Undefined` is the default behavior (undefined renders as empty string in many contexts).
  - Consider stricter undefined behavior in applications that want failures for missing variables (configure via `Environment(undefined=...)`).
- **Bytecode caching**
  - Use `bytecode_cache=FileSystemBytecodeCache(...)` to speed repeated startup/compilation.
- **Line statements/comments**
  - If enabling, configure `Environment(line_statement_prefix=..., line_comment_prefix=...)` (keep default delimiters unless required).

## Pitfalls

### Wrong: Using `{{ ... }}` inside control tags
```python
from jinja2 import Environment

env = Environment()
template = env.from_string("{% if {{ user }} %}Hello{% endif %}")
print(template.render(user=True))
```

### Right: Use variables directly in `{% ... %}` tags
```python
from jinja2 import Environment

env = Environment()
template = env.from_string("{% if user %}Hello{% endif %}")
print(template.render(user=True))
```

### Wrong: Invalid whitespace control syntax (spaces around `-`)
```python
from jinja2 import Environment

env = Environment()
template = env.from_string("{% - if foo - %}X{% endif %}")
print(template.render(foo=True))
```

### Right: `-` must be adjacent to delimiters
```python
from jinja2 import Environment

env = Environment()
template = env.from_string("{%- if foo -%}X{%- endif -%}")
print(template.render(foo=True))
```

### Wrong: `extends` not first tag (unexpected output before inheritance)
```python
from jinja2 import Environment, DictLoader

env = Environment(loader=DictLoader({
    "base.html": "BASE:{% block content %}{% endblock %}",
    "child.html": "BEFORE{% extends 'base.html' %}{% block content %}C{% endblock %}",
}))
print(env.get_template("child.html").render())
```

### Right: Put `{% extends ... %}` first
```python
from jinja2 import Environment, DictLoader

env = Environment(loader=DictLoader({
    "base.html": "BASE:{% block content %}{% endblock %}",
    "child.html": "{% extends 'base.html' %}{% block content %}C{% endblock %}",
}))
print(env.get_template("child.html").render())
```

### Wrong: Block inside a loop without `scoped` (loop variable not visible in block)
```python
from jinja2 import Environment

env = Environment()
template = env.from_string(
    "{% for item in seq %}"
    "{% block loop_item %}{{ item }}{% endblock %}"
    "{% endfor %}"
)
print(template.render(seq=[1, 2, 3]))
```

### Right: Mark the block `scoped` to access outer variables
```python
from jinja2 import Environment

env = Environment()
template = env.from_string(
    "{% for item in seq %}"
    "{% block loop_item scoped %}{{ item }}{% endblock %}"
    "{% endfor %}"
)
print(template.render(seq=[1, 2, 3]))
```

### Wrong: Calling sync render for async templates from an async context
```python
import asyncio
from jinja2 import Environment

async def handler() -> None:
    env = Environment(enable_async=True)
    template = env.from_string("{{ x }}")
    # In an active event loop this can fail because sync render may call asyncio.run().
    print(template.render(x=1))

asyncio.run(handler())
```

### Right: Use the async rendering API
```python
import asyncio
from jinja2 import Environment

async def handler() -> None:
    env = Environment(enable_async=True)
    template = env.from_string("{{ x }}")
    print(await template.render_async(x=1))

asyncio.run(handler())
```

## References

- [Donate](https://palletsprojects.com/donate)
- [Documentation](https://jinja.palletsprojects.com/)
- [Changes](https://jinja.palletsprojects.com/page/changes/)
- [Source](https://github.com/pallets/jinja/)
- [Chat](https://discord.gg/pallets)

## Migration from v3.1.x

### Breaking changes in 3.2.0
- **Runtime**: Python 3.10+ required (3.7/3.8/3.9 dropped).
- **Dependencies**: MarkupSafe >= 3.0 required.
- **Dependencies (i18n users)**: Babel >= 2.17 required.
- **Build**: Packaging metadata moved to `pyproject.toml` and build backend changed to `flit_core` (mostly affects building from source).
- **Deprecation**: `jinja2.__version__` is deprecated in 3.2.0.

### Deprecated → Current mapping
- `jinja2.__version__` ⚠️ → Prefer feature detection; if you must read a version use:
```python
from importlib.metadata import version

v = version("jinja2")
print(v)
```

### Before/after: avoid `__version__`
```python
# Before (deprecated in 3.2.0)
import jinja2

print(jinja2.__version__)
```

```python
# After
from importlib.metadata import version

print(version("jinja2"))
```

## API Reference

- **Environment(loader=None, autoescape=False, undefined=..., trim_blocks=False, lstrip_blocks=False, enable_async=False, bytecode_cache=None, ...)** - Central configuration object; compiles and loads templates.
- **Environment.overlay(**kwargs)** - Create a derived environment with overridden options (useful per-request/per-tenant).
- **FileSystemLoader(searchpath)** - Load templates from directories on disk.
- **PackageLoader(package_name, package_path="templates")** - Load templates from a Python package.
- **FileSystemBytecodeCache(directory, pattern="__jinja2_%s.cache")** - Persist compiled template bytecode to disk.
- **Undefined** - Default undefined type used by the environment (controls behavior of missing variables).
- **Template.generate_async(\*\*context)** - Async generator yielding rendered chunks.
- **pass_context** - Decorator to pass the active `Context` into a filter/function.
- **pass_eval_context** - Decorator to pass the evaluation context (e.g., autoescape state).
- **pass_environment** - Decorator to pass the active `Environment`.
- **jinja2.filters.attr(value, name)** - Attribute lookup filter (attribute-focused; does not bypass sandbox checks).
- **jinja2.filters.unique(seq, ...)** - Filter unique items from a sequence.
- **jinja2.filters.int(value, default=0, base=10)** - Convert to int with defaults.
- **jinja2.filters.xmlattr(mapping, autospace=True)** - Render a dict as XML/HTML attributes (keys must be validated; never trust user-controlled keys).
- **jinja2.filters.urlize(value, trim_url_limit=None, nofollow=False, target=None, rel=None, extra_schemes=None)** - Convert URLs in text to clickable links.
- **jinja2.filters.select(seq, test=None, ...)** - Filter a sequence by a test.
- **Context.resolve_or_missing(key)** - Context lookup hook for custom `Context` subclasses.
- **Context.resolve(key)** - Resolve a variable name in the context.
- **Markup** (from `markupsafe.Markup`) - Marks a string as already-escaped/safe for HTML.
- **escape** (from `markupsafe.escape`) - Escape text for HTML/XML contexts.