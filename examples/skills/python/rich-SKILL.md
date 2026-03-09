---
name: rich
description: Rich is a Python library for richly formatted terminal output, including styled text, tables, progress bars, and more.
license: MIT
metadata:
  version: "14.3.3"
  ecosystem: python
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```python
import rich
from rich import print, print_json, filesize
from rich.console import Console, Group
from rich.prompt import Prompt, IntPrompt, Confirm
from rich.table import Table
from rich.progress import track, Progress
from rich.tree import Tree
from rich.columns import Columns
from rich.panel import Panel
from rich.pretty import pprint, pretty_repr
import rich.traceback
import rich.pretty
```

## Core Patterns

### Styled printing with `rich.print` ✅ Current
```python
from rich import print

def main() -> None:
    print("Hello, [bold magenta]World[/]!")
    print("[green]OK[/] [dim](dim text)[/]")
    print("A whole line styled via markup, plus an emoji: [bold]Done[/] ✅")

if __name__ == "__main__":
    main()
```
* Use `from rich import print` as a drop‑in replacement for built‑in `print`.
* Inline styling uses Rich markup tags (BBCode‑like), e.g. `[bold magenta]...[/]`.

### Use a shared `Console` for app‑wide output ✅ Current
```python
from __future__ import annotations

from rich.console import Console
from rich.table import Table

def build_table() -> Table:
    table = Table(title="Build Summary")
    table.add_column("Step", style="bold")
    table.add_column("Status", justify="right")
    table.add_row("Lint", "[green]pass[/]")
    table.add_row("Tests", "[green]pass[/]")
    table.add_row("Package", "[yellow]skipped[/]")
    return table

def main() -> None:
    console = Console()
    console.print("Starting build...", style="bold cyan")
    console.print(build_table())
    console.log("Build finished", log_locals=False)

if __name__ == "__main__":
    main()
```
* Prefer a single `rich.console.Console` instance for consistent width/color/logging configuration.
* Use `console.print(..., style="...")` to style an entire renderable/line (and markup for parts).

### JSON pretty printing with `print_json` ✅ Current
```python
from __future__ import annotations

from rich import print_json

def main() -> None:
    payload: dict[str, object] = {
        "name": "example",
        "ok": True,
        "count": 3,
        "items": ["a", "b", "c"],
        "meta": {"source": "unit-test"},
    }
    print_json(data=payload, indent=2, highlight=True, sort_keys=True)

if __name__ == "__main__":
    main()
```
* `print_json(json=...)` prints a JSON string; `print_json(data=...)` encodes Python data then prints.
* Useful for debugging structured output with syntax highlighting.

### Progress over an iterable with `track` ✅ Current
```python
from __future__ import annotations

import time
from rich.progress import track

def main() -> None:
    for _ in track(range(50), description="Working..."):
        time.sleep(0.01)

if __name__ == "__main__":
    main()
```
* `rich.progress.track(sequence, description=..., total=...)` is the quick pattern for a single progress bar.

### Prompts with validation (`Prompt.ask`, `Confirm.ask`) ✅ Current
```python
from __future__ import annotations

from rich.prompt import Prompt, IntPrompt, Confirm

def main() -> None:
    name: str = Prompt.ask("Name", default="Ada")
    color: str = Prompt.ask(
        "Favorite color",
        choices=["red", "green", "blue"],
        default="green",
        case_sensitive=False,
    )
    age: int = IntPrompt.ask("Age", default=30)
    proceed: bool = Confirm.ask("Proceed?", default=True)

    from rich import print
    print(f"Hello [bold]{name}[/], age={age}, color={color}, proceed={proceed}")

if __name__ == "__main__":
    main()
```
* `Prompt.ask(..., choices=[...])` loops until valid input; set `case_sensitive=False` if desired.
* `Confirm.ask(...)` is for yes/no prompts; `IntPrompt` / `FloatPrompt` parse numeric input.
* The prompt string supports Rich markup; escape literal brackets with `\[` if needed.

### Pretty printing with `pretty.pprint` ✅ Current
```python
from __future__ import annotations

from rich.pretty import pprint

def main() -> None:
    data = {
        "name": "example",
        "items": list(range(1, 11)),
        "nested": {"foo": "bar", "baz": [True, False]},
    }

    # Basic pretty print
    pprint(data)

    # Truncate long sequences
    pprint(data, max_length=3)

    # Truncate long strings
    pprint({"long": "Hello" * 50}, max_string=20)

if __name__ == "__main__":
    main()
```
* `pprint(obj, ...)` pretty‑prints objects with automatic layout and syntax highlighting.
* `max_length` limits items shown in sequences/dicts; `max_string` truncates long strings.
* `expand_all=True` forces a multi‑line layout even for small objects.
* `max_depth=N` truncates nested structures beyond a given depth.

### Trees for hierarchical data ✅ Current
```python
from __future__ import annotations

from rich.tree import Tree
from rich.console import Console

def main() -> None:
    console = Console()

    tree = Tree("Project Root")
    tree.add("README.md")
    src = tree.add("src/", style="bold blue")
    src.add("main.py")
    src.add("utils.py")
    tree.add("tests/", style="bold green")

    console.print(tree)

if __name__ == "__main__":
    main()
```
* `Tree(label)` creates a tree structure for hierarchical data visualization.
* `.add(item, style=..., guide_style=...)` adds branches; returns a `Tree` for nesting.

### Columns for multi‑column layout ✅ Current
```python
from __future__ import annotations

from rich.columns import Columns
from rich.console import Console
from rich.panel import Panel

def main() -> None:
    console = Console()

    panels = [Panel(f"Item {i}", expand=True) for i in range(6)]
    columns = Columns(panels, equal=True, expand=True)

    console.print(columns)

if __name__ == "__main__":
    main()
```
* `Columns(renderables, ...)` arranges items in columns.
* `equal=True` gives equal‑width columns; `expand=True` fills available width.
* `align="left"`, `"center"`, or `"right"` controls alignment.
* **Note (14.3.0+):** Extra padding on the first cell was fixed; upgrade to 14.3.0+ for correct alignment.

### Soft‑wrap and trailing whitespace ✅ Current (14.3.0+)
```python
from __future__ import annotations

from rich.console import Console

def main() -> None:
    console = Console()

    # In 14.3.0+, trailing whitespace is REMOVED with soft_wrap=True
    console.print("hello   ", soft_wrap=True)
    # Output: "hello" (trailing spaces stripped)

    # Preserve whitespace by disabling soft_wrap
    console.print("hello   ", soft_wrap=False)

if __name__ == "__main__":
    main()
```
* **Breaking change in 14.3.0:** `console.print(..., soft_wrap=True)` now **removes** trailing whitespace.
* If your code relied on trailing whitespace being kept, remove `soft_wrap=True` or handle whitespace before printing.

### Nesting Progress / Live contexts ✅ Current (14.1.0+)
```python
from __future__ import annotations

import time
from rich.progress import Progress

def main() -> None:
    # Nesting Live/Progress is supported from 14.1.0 onward
    with Progress() as outer:
        task1 = outer.add_task("Outer", total=3)
        for _ in range(3):
            with Progress() as inner:
                task2 = inner.add_task("Inner", total=5)
                for _ in range(5):
                    time.sleep(0.01)
                    inner.advance(task2)
            outer.advance(task1)

if __name__ == "__main__":
    main()
```
* Nesting `Live` objects (including `Progress`) is supported from 14.1.0 onward.

### Pretty repr of complex objects ✅ Current
```python
from __future__ import annotations

from dataclasses import dataclass, field
from typing import List
from rich.pretty import pretty_repr

@dataclass
class BuildResult:
    name: str
    steps: List[str] = field(default_factory=list)
    success: bool = True
    _internal: int = field(default=0, repr=False)

def main() -> None:
    result = BuildResult("my-build", steps=["lint", "test", "package"])

    # Wide layout
    print(pretty_repr(result, max_width=80))

    # Narrow layout forces multi‑line
    print(pretty_repr(result, max_width=20))

    # Depth‑limited
    nested = {"build": result}
    print(pretty_repr(nested, max_depth=1))

if __name__ == "__main__":
    main()
```
* `pretty_repr(obj, max_width=..., max_depth=..., max_length=..., max_string=...)` returns a styled string.
* Fields with `repr=False` are excluded; circular references render as `...`.
* Works with dataclasses, `namedtuple`, attrs classes, and objects with `__rich_repr__`.

### File‑size formatting with `rich.filesize` ✅ New
```python
from __future__ import annotations

from rich import filesize

def main() -> None:
    sizes = [
        0,
        1,
        2,
        1_000,
        1_500_000,
        1_234_567_890,
    ]
    for s in sizes:
        print(f"{s} → {filesize.decimal(s)}")

if __name__ == "__main__":
    main()
```
* `filesize.decimal(size, precision=1, separator=" ")` formats a byte count into a human‑readable string (e.g. `1.5 MB`).
* The function respects the `precision` and `separator` arguments for fine‑grained control.

## Configuration

- **Console configuration**
  - Prefer constructing a `Console()` and passing it through your app.
  - If you rely on Rich's global console, you can access it via:
    - `rich.get_console() -> Console`
    - `rich.reconfigure(**kwargs) -> None` (reconfigures the global console)
- **Environment variables (behavior change in 14.0.0)**
  - `NO_COLOR`: if set to a **non‑empty** value, disables color output; **empty** is treated as disabled (i.e. does **not** disable colors).
  - `FORCE_COLOR`: if set to a **non‑empty** value, forces color output; **empty** is treated as disabled.
  - `UNICODE_VERSION`: control Unicode version used for cell width calculations (added in 14.3.0).
  - `TTY_COMPATIBLE`: override auto‑detection of TTY support (added in 14.0.0).
  - `TTY_INTERACTIVE`: force interactive mode on or off (added in 14.1.0).
- **Unicode width handling**
  - Rich has internal support for Unicode cell width tables; avoid relying on internal loaders.
  - If using `rich.cells.cell_len`, prefer keyword args (not positional), especially after signature changes in 14.3.0.
- **Pretty printing in REPL / IPython**
  - `rich.pretty.install()` enables pretty‑printing in the Python REPL.
  - On 14.3.0+, IPython respects a passed `Console` in `pretty.install(console=...)`.
- **typing_extensions (14.1.0+)**
  - `typing_extensions` is no longer a runtime dependency of Rich. Add it explicitly to your own project if you use it.

## Pitfalls

### Wrong: Using built‑in `print` and expecting Rich markup to render
```python
def main() -> None:
    # Built‑in print will output markup tags literally.
    print("Hello, [bold magenta]World[/]!")

if __name__ == "__main__":
    main()
```

### Right: Import `rich.print` (or use `Console.print`)
```python
from rich import print

def main() -> None:
    print("Hello, [bold magenta]World[/]!")

if __name__ == "__main__":
    main()
```

### Wrong: `Prompt.ask` choices are case‑sensitive by default
```python
from rich.prompt import Prompt

def main() -> None:
    # User typing "paul" will be rejected.
    name = Prompt.ask(
        "Enter your name",
        choices=["Paul", "Jessica", "Duncan"],
        default="Paul",
    )
    from rich import print
    print(name)

if __name__ == "__main__":
    main()
```

### Right: Set `case_sensitive=False` when appropriate
```python
from rich.prompt import Prompt

def main() -> None:
    name = Prompt.ask(
        "Enter your name",
        choices=["Paul", "Jessica", "Duncan"],
        default="Paul",
        case_sensitive=False,
    )
    from rich import print
    print(name)

if __name__ == "__main__":
    main()
```

### Wrong: Passing multiple renderables where a single renderable is expected (e.g., `Panel`)
```python
from rich import print
from rich.panel import Panel

def main() -> None:
    # Panel expects a single renderable as its content.
    print(Panel("Hello", "World"))

if __name__ == "__main__":
    main()
```

### Right: Combine multiple renderables with `Group`
```python
from rich import print
from rich.console import Group
from rich.panel import Panel

def main() -> None:
    content = Group(
        "Hello",
        "World",
    )
    print(Panel(content, title="Greeting"))

if __name__ == "__main__":
    main()
```

### Wrong: Relying on exact traceback formatting in snapshot tests across versions
```python
import rich.traceback

def main() -> None:
    rich.traceback.install()
    raise ValueError("boom")

if __name__ == "__main__":
    main()
```

### Right: Assert on stable substrings / exception types, not exact rendered frames
```python
from __future__ import annotations

import rich.traceback

def main() -> None:
    rich.traceback.install()
    try:
        raise ValueError("boom")
    except ValueError as exc:
        # In tests, assert on message/type rather than exact terminal rendering.
        assert "boom" in str(exc)

if __name__ == "__main__":
    main()
```

### Wrong: Expecting empty environment variables to enable features
```python
from __future__ import annotations

import os

def main() -> None:
    # Empty NO_COLOR will NOT disable colors in Rich 14.0.0+
    os.environ["NO_COLOR"] = ""
    from rich import print
    print("[red]This will still be colored[/]")

if __name__ == "__main__":
    main()
```

### Right: Set environment variables to non‑empty values
```python
from __future__ import annotations

import os

def main() -> None:
    # Set to non‑empty value to disable colors
    os.environ["NO_COLOR"] = "1"
    from rich import print
    print("[red]This will not be colored[/]")

if __name__ == "__main__":
    main()
```

### Wrong: Assuming `soft_wrap=True` preserves trailing whitespace (pre‑14.3.0 behavior)
```python
from rich.console import Console

def main() -> None:
    console = Console()
    # In 14.3.0+, trailing spaces are REMOVED — do not rely on them being kept
    value = "hello   "
    console.print(value, soft_wrap=True)
    # Output: "hello" (trailing spaces stripped in 14.3.0+)

if __name__ == "__main__":
    main()
```

### Right: Don't use soft_wrap if you need to preserve trailing whitespace
```python
from rich.console import Console

def main() -> None:
    console = Console()
    # Omit soft_wrap (default False) if trailing whitespace must be preserved
    value = "hello   "
    console.print(value)

if __name__ == "__main__":
    main()
```

### Wrong: Relying on `typing_extensions` being available via Rich (14.1.0+)
```python
# Wrong: assuming rich pulls in typing_extensions transitively
import typing_extensions  # may fail if not explicitly installed

if __name__ == "__main__":
    pass
```

### Right: Declare `typing_extensions` as an explicit dependency
```python
# In pyproject.toml or requirements.txt, add:
#   typing_extensions>=4.0
# Then import directly:
import typing_extensions  # now guaranteed to be present

if __name__ == "__main__":
    pass
```

### Wrong: Using `rich.cells.cell_len` with positional arguments after 14.3.0
```python
from rich.cells import cell_len

def main() -> None:
    # Positional argument order may have changed in 14.3.0
    length = cell_len("hello", some_callable)  # fragile

if __name__ == "__main__":
    main()
```

### Right: Use keyword arguments with `cell_len`
```python
from rich.cells import cell_len

def main() -> None:
    # Prefer keyword arguments for stability across versions
    length = cell_len(text="hello")

if __name__ == "__main__":
    main()
```

## Migration

### Breaking changes from previous versions

| From → To | Change | Migration |
|-----------|--------|-----------|
| **14.3.0** | `soft_wrap=True` now **removes** trailing whitespace. | Disable `soft_wrap` or use explicit padding/non‑breaking spaces if you need trailing spaces. |
| **14.3.0** | `rich.cells.cell_len` signature changed. | Call `cell_len` with keyword arguments (`text=...`). |
| **14.3.0** | `pretty.install` now respects a passed `Console` in IPython. | Pass your console: `pretty.install(console=Console())`. |
| **14.3.0** | Markdown styling updates (`markdown.table.header`, `markdown.table.border`). | Review custom Markdown rendering; adjust style overrides if needed. |
| **14.1.0** | `typing_extensions` removed from runtime dependencies. | Add `typing_extensions` to your project if you use it, or rely on std‑lib equivalents. |
| **14.1.0** | Live/Progress nesting now supported. | No action needed; nesting works out‑of‑the‑box. |
| **14.1.0** | New `TTY_INTERACTIVE` env var. | Use it to force interactive mode when required. |
| **14.0.0** | `NO_COLOR` / `FORCE_COLOR` empty‑value semantics changed. | Ensure scripts set these variables to **non‑empty** values to enable/disable colors. |
| **13.9.0** | Python 3.7 dropped. | Run on Python ≥ 3.8 or pin Rich < 13.9.0. |

### General migration guide (14.x → latest)

1. **Python version** – Ensure you are on Python ≥ 3.8.  
2. **`typing_extensions`** – Add it explicitly if your code uses it.  
3. **Soft‑wrap** – Review any `Console(soft_wrap=True)` usage; trailing spaces are now stripped.  
4. **Environment variables** – Update scripts that set `NO_COLOR` or `FORCE_COLOR` to use non‑empty values for the intended effect.  
5. **Live rendering** – Nested `Live`/`Progress` now works; remove any work‑arounds.  
6. **REPL pretty printer** – Pass a `Console` to `pretty.install` to get Rich formatting in IPython.  
7. **Unicode handling** – The new `UNICODE_VERSION` env var gives finer control; most users can ignore it.  

For the full list of changes, see the project's `CHANGELOG.md`.

## API Reference

- **rich.print(*objects, sep=" ", end="\\n", file=None, flush=False)** – Rich‑enhanced `print` with markup rendering.  
- **rich.print_json(json=None, *, data=None, indent=2, highlight=True, skip_keys=False, ensure_ascii=False, check_circular=True, allow_nan=True, default=None, sort_keys=False)** – Pretty‑print JSON (string or data) with optional highlighting.  
- **rich.inspect(obj, *, console=None, title=None, help=False, methods=False, docs=True, private=False, dunder=False, sort=True, all=False, value=True)** – Introspect and render object details to the console.  
- **rich.get_console() → Console** – Retrieve the global `Console` instance.  
- **rich.reconfigure(**kwargs) → None** – Reconfigure the global `Console`.  
- **rich.console.Console(...)** – Primary output object; controls width, color system, recording, etc.  
- **rich.console.Console.print(*objects, sep=" ", end="\\n", style=None, justify=None, overflow=None, no_wrap=False, emoji=None, markup=True, highlight=False, width=None, height=None, crop=True, soft_wrap=False, new_line_start=False)** – Print objects (strings, Tables, Markdown, Syntax, Panels, etc.) with styling.  
  - **Note (14.3.0+):** `soft_wrap=True` now **removes** trailing whitespace.  
- **rich.console.Console.log(*objects, **kwargs)** – Log with timestamps and optional locals capture for debugging.  
- **rich.console.Console.rule(title=None, characters="─", style=None)** – Print a horizontal rule with optional centered title.  
- **rich.console.Console.status(status, spinner="dots", spinner_style="status.spinner", speed=1.0, refresh_per_second=12.5)** – Context manager for a live status spinner.  
- **rich.console.Console.input(prompt="", *, markup=True, emoji=True, password=False, stream=None)** – Read input with an optional Rich‑styled prompt.  
- **rich.console.Console.pager(pager=None, styles=False, links=False)** – Context manager to display output in a pager.  
- **rich.console.Console.screen(hide_cursor=True, style=None)** – Context manager for alternate screen mode.  
- **rich.console.Group(*renderables)** – Combine multiple renderables into one for containers expecting a single renderable.  
- **rich.prompt.Prompt.ask(prompt, *, choices=None, default=None, case_sensitive=True, ...)** – Prompt for text input with optional validation.  
- **rich.prompt.IntPrompt.ask(...) / rich.prompt.FloatPrompt.ask(...)** – Prompt for numeric input with parsing and validation.  
- **rich.prompt.Confirm.ask(prompt, *, default=..., …)** – Prompt for yes/no input.  
- **rich.table.Table(title=None, …)** – Build tables for console rendering.  
- **rich.table.Table.add_column(header, *, style=None, justify=None, …)** – Define columns.  
- **rich.table.Table.add_row(*cells, …)** – Add rows.  
- **rich.progress.track(sequence, description='Working...', total=None, …)** – Iterate with a progress bar.  
- **rich.progress.Progress(...)** – Full progress bar manager; supports multiple tasks and nesting (14.1.0+).  
- **rich.markdown.Markdown(markdown_text)** – Render Markdown; styles updated in 14.3.0 (`markdown.table.header`, `markdown.table.border`).  
- **rich.syntax.Syntax(code, lexer, theme="monokai", line_numbers=False, …)** – Render syntax‑highlighted code.  
- **rich.pretty.install(console=None, **options)** – Enable Rich pretty‑printing in the REPL/IPython. In 14.3.0+, the `console` argument is respected in IPython.  
- **rich.pretty.pprint(obj, *, console=None, indent_guides=True, max_length=None, max_string=None, max_depth=None, expand_all=False, …)** – Pretty‑print an object to the console with Rich formatting.  
- **rich.pretty.pretty_repr(obj, *, max_width=80, indent_size=4, max_length=None, max_string=None, max_depth=None, expand_all=False, …)** – Generate a pretty string representation of an object. Handles dataclasses, NamedTuples, attrs classes, and objects with `__rich_repr__`.  
- **rich.traceback.install(*, locals_max_depth=None, locals_overflow=None, …)** – Install Rich traceback handler. New parameters (`locals_max_depth`, `locals_overflow`) added in 14.3.0.  
- **rich.tree.Tree(label, *, guide_style="tree.line", …)** – Create a tree structure for hierarchical data.  
- **rich.tree.Tree.add(label, *, style=None, guide_style=None, …)** – Add a branch; returns a `Tree` for nesting.  
- **rich.columns.Columns(renderables, padding=(0, 1), *, width=None, equal=False, expand=False, title=None, column_first=False, right_to_left=False, align="left")** – Arrange renderables in columns. First‑cell padding bug fixed in 14.3.0.  
- **rich.align.Align(renderable, align="left", style=None, *, vertical=None, pad=True, width=None, height=None)** – Wrap a renderable with alignment. Class methods: `Align.left(...)`, `Align.center(...)`, `Align.right(...)`.  
- **rich.filesize.decimal(size, *, precision=1, separator=" ")** – Format a file size in decimal units (bytes, kB, MB, …).  
- **rich.filesize.pick_unit_and_suffix(value, units, divisor)** – Helper for size formatting.  
- **rich.cells.cell_len(text, …)** – Return the cell width of a string; use keyword arguments (signature changed in 14.3.0).  
- **rich.cells.get_character_cell_size(character)** – Return the cell width of a single character.  
- **rich.color.Color.parse(color)** – Parse a color string (name, hex, or ANSI number) into a `Color` object.  
- **rich.color.Color.from_rgb(red, green, blue)** – Construct a `Color` from RGB float components.  
- **rich.color.Color.from_ansi(number)** – Construct a `Color` from an ANSI color number.  
- **rich.ansi.AnsiDecoder** – Decode ANSI‑escaped terminal text into Rich `Text` objects (`decode(text)`, `decode_line(line)`).  

## References

- [Official Documentation](https://rich.readthedocs.io/)
- [GitHub Repository](https://github.com/Textualize/rich)