---

name: jinja2
description: A Python templating engine for rendering text (often HTML) from templates and context data.
version: 3.2.0
ecosystem: python
license: BSD-3-Clause
generated_with: gpt-5.2
---

## Imports

```python
import jinja2
from jinja2 import (
    Environment,
    FileSystemLoader,
    PackageLoader,
    Template,
    Undefined,
    pass_context,
    pass_environment,
    pass_eval_context,
)
from jinja2.ext import AutoEscapeExtension, WithExtension
from jinja2.filters import attr, int, select, unique, urlize, xmlattr
from importlib.metadata import version
```

## Core Patterns

### Create an Environment with a loader and render a template ✅ Current
```python
from __future__ import annotations

from pathlib import Path

from jinja2 import Environment, FileSystemLoader


def main() -> None:
    templates_dir = Path("templates")
    templates_dir.mkdir(parents=True, exist_ok=True)

    # Ensure the file ends with exactly one trailing newline.
    (templates_dir / "hello.txt.jinja").write_text(
        "Hello {{ name }}!\nItems: {{ items|join(', ') }}\n",
        encoding="utf-8",
        newline="\n",
    )

    env = Environment(
        loader=FileSystemLoader(str(templates_dir)),
        autoescape=False,
        trim_blocks=True,
        lstrip_blocks=True,
        keep_trailing_newline=True,  # preserve the final "\n" from the template source
    )

    template = env.get_template("hello.txt.jinja")
    out = template.render(name="Ada", items=["a", "b", "c"])

    # Print without adding an extra newline (the template already ends with one).
    print(out, end="")


if __name__ == "__main__":
    main()
```
* Use `Environment(loader=...)` + `env.get_template(name)` for file-based templates.
* `trim_blocks` and `lstrip_blocks` are common whitespace controls for text/HTML output.

### Render from an in-memory Template ✅ Current
```python
from __future__ import annotations

from jinja2 import Environment


def main() -> None:
    # Use an Environment so rendering behavior (like trailing newlines) is explicit.
    env = Environment(keep_trailing_newline=True)
    template = env.from_string("2 + 2 = {{ 2 + 2 }}\n")

    out = template.render()
    print(out, end="")


if __name__ == "__main__":
    main()
```
* Use `Template(source)` for short, in-memory templates (no loader required).
* Prefer `Environment.from_string(...)` when you need consistent environment configuration.

### Add custom filters using pass_* decorators ✅ Current
```python
from __future__ import annotations

from typing import Any

from jinja2 import Environment, pass_context


@pass_context
def is_defined(ctx: Any, name: str) -> bool:
    # ctx is a jinja2.runtime.Context at runtime; treat as Any for stable typing here.
    return ctx.resolve_or_missing(name) is not ctx.environment.undefined


def main() -> None:
    env = Environment()
    env.filters["is_defined"] = is_defined

    template = env.from_string(
        "{% if 'x'|is_defined %}x is defined{% else %}x is missing{% endif %}\n"
    )
    print(template.render(x=123))
    print(template.render())


if __name__ == "__main__":
    main()
```
* Use `@pass_context`, `@pass_environment`, `@pass_eval_context` instead of removed legacy decorators.
* `Context.resolve_or_missing(name)` is the supported way to check for missing variables.

### Overlay an Environment for per-request configuration ✅ Current
```python
from __future__ import annotations

from jinja2 import Environment


def main() -> None:
    base_env = Environment(autoescape=False, keep_trailing_newline=True)
    html_env = base_env.overlay(autoescape=True)

    # Use a variable so the template behavior is explicit and easy to test.
    t = "{{ value }}\n"
    out_base = base_env.from_string(t).render(value="<b>x</b>")
    out_html = html_env.from_string(t).render(value="<b>x</b>")

    # base_env does not autoescape; html_env does.
    assert out_base == "<b>x</b>\n"
    assert out_html == "&lt;b&gt;x&lt;/b&gt;\n"

    # Print without adding extra newlines; each render already ends with "\n".
    print(out_base, end="")
    print(out_html, end="")


if __name__ == "__main__":
    main()
```
* `Environment.overlay(...)` creates a derived environment that shares caches but can override options.
* Useful for toggling `autoescape`, adding request-specific globals/filters, etc.

### Async template generation with generate_async ✅ Current
```python
from __future__ import annotations

import asyncio
from typing import List

from jinja2 import Environment


async def main() -> None:
    env = Environment(enable_async=True)
    template = env.from_string("Hello {{ name }}!\n")

    chunks: List[str] = []
    async for piece in template.generate_async(name="Async"):
        chunks.append(piece)

    print("".join(chunks), end="")


if __name__ == "__main__":
    asyncio.run(main())
```
* When `enable_async=True`, use async-aware APIs like `Template.generate_async(...)`.
* Consume the async generator fully (or ensure it’s properly closed) to avoid resource leaks.

## Configuration

- **Template loading**
  - `FileSystemLoader("templates")` for filesystem templates (common project convention: `templates/` directory).
  - `PackageLoader("your_pkg", "templates")` for templates shipped inside a Python package.
- **Whitespace control**
  - `Environment(trim_blocks=True, lstrip_blocks=True)` for cleaner output.
  - In templates, use `{%- ... -%}` to strip adjacent whitespace; `+` can disable configured stripping for a specific tag.
- **Autoescaping**
  - Configure via `Environment(autoescape=...)` or use `jinja2.ext.AutoEscapeExtension` where appropriate.
- **Bytecode caching**
  - `FileSystemBytecodeCache(directory="...")` can speed up template compilation in some deployments.
- **Undefined handling**
  - `Environment(undefined=Undefined)` (default) produces `Undefined` objects; configure a different undefined type if you need stricter behavior.
- **Line statements/comments**
  - If using line statements/comments, set `line_statement_prefix` / `line_comment_prefix` on `Environment`.

## Pitfalls

### Wrong: Content before `{% extends %}` renders unexpectedly
```python
from __future__ import annotations

from jinja2 import Environment, DictLoader


def main() -> None:
    env = Environment(loader=DictLoader({
        "base.html": "<body>{% block content %}BASE{% endblock %}</body>",
        "child.html": "Hello!\n{% extends 'base.html' %}{% block content %}CHILD{% endblock %}",
    }))
    print(env.get_template("child.html").render())


if __name__ == "__main__":
    main()
```

### Right: Keep `{% extends %}` as the first tag
```python
from __future__ import annotations

from jinja2 import Environment, DictLoader


def main() -> None:
    env = Environment(loader=DictLoader({
        "base.html": "<body>{% block content %}BASE{% endblock %}</body>",
        "child.html": "{% extends 'base.html' %}{% block content %}CHILD{% endblock %}",
    }))
    print(env.get_template("child.html").render())


if __name__ == "__main__":
    main()
```

### Wrong: Block inside a loop can’t see loop variables (missing `scoped`)
```python
from __future__ import annotations

from jinja2 import Environment


def main() -> None:
    env = Environment()
    template = env.from_string(
        "{% for item in seq %}"
        "<li>{% block loop_item %}{{ item }}{% endblock %}</li>"
        "{% endfor %}\n"
    )
    print(template.render(seq=[1, 2]))


if __name__ == "__main__":
    main()
```

### Right: Mark the block `scoped` to access loop variables
```python
from __future__ import annotations

from jinja2 import Environment


def main() -> None:
    env = Environment()
    template = env.from_string(
        "{% for item in seq %}"
        "<li>{% block loop_item scoped %}{{ item }}{% endblock %}</li>"
        "{% endfor %}\n"
    )
    print(template.render(seq=[1, 2]))


if __name__ == "__main__":
    main()
```

### Wrong: Invalid whitespace control syntax (`-` must touch the delimiters)
```python
from __future__ import annotations

from jinja2 import Environment


def main() -> None:
    env = Environment()
    # This template will raise a TemplateSyntaxError due to "{% - if foo - %}".
    template = env.from_string("{% - if foo - %}X{% endif %}\n")
    print(template.render(foo=True))


if __name__ == "__main__":
    main()
```

### Right: Use `{%- ... -%}` (minus adjacent to delimiters)
```python
from __future__ import annotations

from jinja2 import Environment


def main() -> None:
    env = Environment()
    template = env.from_string("{%- if foo -%}X{%- endif -%}\n")
    print(template.render(foo=True))


if __name__ == "__main__":
    main()
```

### Wrong: Untrusted keys passed to `xmlattr` can cause injection/malformed output
```python
from __future__ import annotations

from jinja2 import Environment


def main() -> None:
    env = Environment(autoescape=True)
    template = env.from_string("<a{{ attrs|xmlattr }}>link</a>\n")

    # Simulating untrusted input: keys should be validated/whitelisted in Python.
    user_attrs = {"onmouseover": "alert(1)", "href": "https://example.com"}
    print(template.render(attrs=user_attrs))


if __name__ == "__main__":
    main()
```

### Right: Whitelist keys in Python before rendering `xmlattr`
```python
from __future__ import annotations

from jinja2 import Environment


def main() -> None:
    env = Environment(autoescape=True)
    template = env.from_string("<a{{ attrs|xmlattr }}>link</a>\n")

    user_attrs = {"onmouseover": "alert(1)", "href": "https://example.com"}
    allowed = {"href", "title", "rel", "class", "id"}
    safe_attrs = {k: v for k, v in user_attrs.items() if k in allowed}

    print(template.render(attrs=safe_attrs))


if __name__ == "__main__":
    main()
```

## References

- [Donate](https://palletsprojects.com/donate)
- [Documentation](https://jinja.palletsprojects.com/)
- [Changes](https://jinja.palletsprojects.com/page/changes/)
- [Source](https://github.com/pallets/jinja/)
- [Chat](https://discord.gg/pallets)

## Migration from v3.1.x

- **Python support (breaking)**: 3.2.0 requires **Python 3.10+** (drops 3.7–3.9).
- **Dependency minimums (breaking)**: MarkupSafe **>= 3.0**, Babel **>= 2.17**.
- **Version checks (soft deprecation)**: `jinja2.__version__` is deprecated.

### `jinja2.__version__` ⚠️ Soft Deprecation
- Deprecated since: 3.2.0
- Still works: true (but emits deprecation warnings)
- Modern alternative: `importlib.metadata.version("jinja2")`
- Migration guidance:
```python
from __future__ import annotations

from importlib.metadata import version


def main() -> None:
    print(version("jinja2"))


if __name__ == "__main__":
    main()
```

### Legacy decorator aliases (`contextfilter`, etc.) 🗑️ Removed
- Removed since: 3.1.0
- Modern alternatives: `pass_context`, `pass_eval_context`, `pass_environment`
- Migration guidance (example):
```python
from __future__ import annotations

from jinja2 import Environment, pass_environment


@pass_environment
def env_name(env: Environment, value: str) -> str:
    return f"{value} (autoescape={env.autoescape})"


def main() -> None:
    env = Environment()
    env.filters["env_name"] = env_name
    print(env.from_string("{{ 'x'|env_name }}\n").render())


if __name__ == "__main__":
    main()
```

## API Reference

- **Environment(...)** - Central configuration object; key params include `loader`, `autoescape`, `trim_blocks`, `lstrip_blocks`, `undefined`, `enable_async`, `line_statement_prefix`, `line_comment_prefix`.
- **Environment.__init__(...)** - Constructs an environment; use to configure loaders, escaping, async, whitespace.
- **Environment.overlay(...)** - Create a derived environment that shares internal state/caches but overrides selected options.
- **Template(...)** - In-memory compiled template from source text.
- **Template.render(...)** - Render synchronously with context variables (`template.render(**vars)`).
- **Template.generate_async(...)** - Async generator yielding rendered chunks; requires `Environment(enable_async=True)`.
- **FileSystemLoader(...)** - Load templates from directories on disk.
- **PackageLoader(...)** - Load templates from a Python package’s resources.
- **FileSystemBytecodeCache(...)** - Persist compiled template bytecode to the filesystem.
- **Undefined** - Default undefined value type used for missing variables.
- **pass_context** - Decorator to pass the active `Context` as first argument to a filter/test/global.
- **pass_eval_context** - Decorator to pass the evaluation context (e.g., autoescape state) to a callable.
- **pass_environment** - Decorator to pass the active `Environment` to a callable.
- **Context.resolve_or_missing(name)** - Resolve a variable name or return a sentinel indicating it is missing (preferred override point for custom Context behavior).
- **jinja2.filters.attr(value, name)** - Filter to fetch an attribute; respects environment attribute lookup/sandbox rules.
- **jinja2.filters.xmlattr(mapping)** - Convert a dict of attributes to XML/HTML attribute syntax; validate/whitelist keys before use.
- **jinja2.filters.unique(seq)** - Filter to yield unique items from a sequence.
- **jinja2.filters.int(value, default=0, base=10)** - Convert to int with defaults.
- **jinja2.filters.urlize(value)** - Convert URLs in text into clickable links (HTML output).
- **jinja2.filters.select(seq, test_name=None, **kwargs)** - Select items from a sequence based on a test.
- **jinja2.ext.WithExtension** - Built-in extension for the `{% with %}` statement.
- **jinja2.ext.AutoEscapeExtension** - Built-in extension related to autoescaping behavior.
- **importlib.metadata.version("jinja2")** - Recommended way to query installed Jinja version.