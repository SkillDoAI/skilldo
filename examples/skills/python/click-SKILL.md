---

name: click
description: Click is a Python library for building command-line interfaces with commands, options, arguments, prompts, styled output, and command groups.
license: BSD-3-Clause
metadata:
  version: "8.3.1"
  ecosystem: python
  generated-by: skilldo/gpt-5.3-codex
---

## Imports
```python
from importlib.metadata import version

import click
from click import Abort, BadParameter, Choice, ClickException, Context, File, Path, UsageError
```

## Core Patterns

### Define a command with options and arguments ✅ Current
```python
import click


@click.command()
@click.argument("filename", type=click.Path(exists=True, dir_okay=False))
@click.option("--count", type=int, default=1, show_default=True)
@click.option("--mode", type=click.Choice(["summary", "full"]), default="summary")
def cli(filename, count, mode):
    for _ in range(count):
        click.echo(f"{mode}: {filename}")


if __name__ == "__main__":
    cli()
```

Use `@click.command`, `@click.argument`, and `@click.option` for the standard CLI structure. Keep decorator parameter names aligned with function argument names.

### Group subcommands for multi-command CLIs ✅ Current
```python
import click


@click.group()
def cli():
    """Top-level CLI."""


@cli.command()
@click.option("--name", default="world", show_default=True)
def greet(name):
    click.echo(f"Hello, {name}!")


@cli.command()
def version_cmd():
    click.echo("App version endpoint")


if __name__ == "__main__":
    cli()
```

Use `@click.group` for a root command and `@cli.command()` for subcommands. This is the preferred replacement for old multi-command aliases.

### Use context and shared state across commands ✅ Current
```python
import click


@click.group()
@click.option("--verbose", is_flag=True)
@click.pass_context
def cli(ctx, verbose):
    ctx.ensure_object(dict)
    ctx.obj["verbose"] = verbose


@cli.command()
@click.pass_obj
def run(obj):
    if obj["verbose"]:
        click.echo("Running in verbose mode")
    else:
        click.echo("Running")


if __name__ == "__main__":
    cli(obj={})
```

Use `click.pass_context`, `click.pass_obj`, or `click.make_pass_decorator` to share app state safely between group and subcommands.

### Interactive terminal UX (prompt, confirm, style, progress) ✅ Current
```python
import time
import click


@click.command()
def cli():
    username = click.prompt("Username", type=str)
    if not click.confirm("Continue?", default=True):
        raise click.Abort()

    click.secho(f"Preparing task for {username}", fg="green", bold=True)

    with click.progressbar(range(5), label="Working") as items:
        for _ in items:
            time.sleep(0.05)

    click.echo(click.style("Done", fg="cyan"))


if __name__ == "__main__":
    cli()
```

Use `prompt` / `confirm` for interactive input and `echo` / `secho` / `style` for terminal-safe output.

### Deprecated aliases: BaseCommand/MultiCommand/OptionParser/__version__ ❌ Hard Deprecation
Deprecated since: available as deprecated compatibility aliases in 8.x  
Still works: yes (temporarily)  
Modern alternative: `click.Command`, `click.Group`, `importlib.metadata.version("click")`, and avoid Click parser internals  
Migration guidance: action required before Click 9.x removals (`__version__` by 9.1; aliases by 9.0).

```python
import click
from importlib.metadata import version


@click.command()
def cli():
    click.echo(f"Click version: {version('click')}")


if __name__ == "__main__":
    cli()
```

## Configuration
- `@click.command(context_settings={...})` and `@click.group(context_settings={...})` customize parser/context behavior.
- Common option settings:
  - `show_default=True` to display defaults in help.
  - `required=True` to force values.
  - `multiple=True` for repeatable options.
  - `is_flag=True`, or dual form `--on/--off` for booleans.
- Type system:
  - `click.Choice([...], case_sensitive=True|False)`
  - `click.Path(exists=..., file_okay=..., dir_okay=..., writable=..., readable=...)`
  - `click.File("r"|"w", lazy=...)`
- Help behavior:
  - `@click.help_option(...)`, `@click.version_option(...)`, `no_args_is_help=True`.
- Environment-driven completion protocol uses variables like `_<PROG_NAME>_COMPLETE` (for shell completion source/complete modes).
- For package version display, use `importlib.metadata.version("click")` (not `click.__version__`).

## Pitfalls

### Wrong: Argument name mismatch with function parameter
```python
import click


@click.command()
@click.argument("filename")
def cli(file_name):
    click.echo(file_name)


if __name__ == "__main__":
    cli()
```

### Right: Argument name matches function parameter
```python
import click


@click.command()
@click.argument("filename")
def cli(filename):
    click.echo(filename)


if __name__ == "__main__":
    cli()
```

### Wrong: Using print instead of click.echo
```python
import click


@click.command()
def cli():
    print("Hello")


if __name__ == "__main__":
    cli()
```

### Right: Use click.echo for stdout/stderr-safe output
```python
import click


@click.command()
def cli():
    click.echo("Hello")
    click.echo("Error line", err=True)


if __name__ == "__main__":
    cli()
```

### Wrong: Using deprecated click.__version__
```python
import click


@click.command()
def cli():
    click.echo(click.__version__)


if __name__ == "__main__":
    cli()
```

### Right: Use importlib.metadata.version("click")
```python
from importlib.metadata import version
import click


@click.command()
def cli():
    click.echo(version("click"))


if __name__ == "__main__":
    cli()
```

### Wrong: Option name mismatch with function parameter
```python
import click


@click.command()
@click.option("--mode", type=click.Choice(["fast", "safe"]))
def cli(style):
    click.echo(style)


if __name__ == "__main__":
    cli()
```

### Right: Option name matches function parameter
```python
import click


@click.command()
@click.option("--mode", type=click.Choice(["fast", "safe"]))
def cli(mode):
    click.echo(mode)


if __name__ == "__main__":
    cli()
```

## References
- [Donate](https://palletsprojects.com/donate)
- [Documentation](https://click.palletsprojects.com/)
- [Changes](https://click.palletsprojects.com/page/changes/)
- [Source](https://github.com/pallets/click/)
- [Chat](https://discord.gg/pallets)

## Migration from v8.1
- Python support change in 8.2.0: Python 3.7/3.8/3.9 dropped; require Python >= 3.10.
- Command name inference changed in 8.2.0: suffixes `_command`, `_cmd`, `_group`, `_grp` are stripped automatically.
- Deprecated APIs to migrate now:
  - `click.BaseCommand` → `click.Command` (removal in 9.0)
  - `click.MultiCommand` → `click.Group` (removal in 9.0)
  - `click.__version__` → `importlib.metadata.version("click")` (removal in 9.1)
- 8.3.0 adjusted `flag_value`/default behavior consistency; retest flag options relying on older implicit behavior.
- 8.3.1 includes UNSET/internal parsing fixes; retest callbacks/default handling if code relied on previous sentinel visibility.

Before (deprecated):
```python
import click


@click.command()
def cli():
    click.echo(click.__version__)


if __name__ == "__main__":
    cli()
```

After (current):
```python
from importlib.metadata import version
import click


@click.command()
def cli():
    click.echo(version("click"))


if __name__ == "__main__":
    cli()
```

## API Reference
- **click.command()** - Decorator that converts a function into a `Command`; key params: `name`, `cls`, command attrs.
- **click.group()** - Decorator for multi-command `Group`; key params: `name`, `cls`, group attrs.
- **click.argument()** - Adds positional arguments; key params: declaration names and `type`, `nargs`, `required`.
- **click.option()** - Adds optional parameters; key params: option flags, `type`, `default`, `is_flag`, `multiple`.
- **click.Command()** - Command object class; key params include `name`, `callback`, `params`, `help`.
- **click.Group()** - Command collection class for subcommands; key params: `name`, `commands`.
- **click.Context()** - Invocation context; carries `obj`, settings, and command execution state.
- **click.echo()** - Terminal-safe text output; key params: `message`, `err`, `nl`, `color`.
- **click.prompt()** - Interactive input prompt with conversion; key params: `text`, `default`, `type`, `hide_input`.
- **click.confirm()** - Yes/no confirmation prompt; key params: `text`, `default`, `abort`.
- **click.secho()** - Styled echo output; combines `echo` and style keyword args.
- **click.style()** - Returns ANSI-styled text; key params: `fg`, `bg`, `bold`, `underline`, etc.
- **click.progressbar()** - Progress UI for iterables; key params: `iterable`, `length`, `label`.
- **click.Choice()** - Parameter type for fixed allowed values; key params: `choices`, `case_sensitive`.
- **click.Path()** - Parameter type for filesystem paths; key params: `exists`, `file_okay`, `dir_okay`, `writable`, `readable`.