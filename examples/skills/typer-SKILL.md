---

name: typer
description: Build command-line interfaces (CLIs) from Python functions using type hints.
version: 0.14.14
ecosystem: python
license: MIT
generated_with: gpt-5.2
---

## Imports

```python
import typer
from typer import Typer, run
```

## Core Patterns

### Single-command script with `typer.run()` ✅ Current
```python
import typer

def main(name: str, formal: bool = False) -> None:
    if formal:
        print(f"Good day, {name}.")
    else:
        print(f"Hi {name}!")

if __name__ == "__main__":
    typer.run(main)
```
* Use for one-command CLIs; Typer derives argument/option parsing from type hints and defaults.
* `name: str` becomes a required positional argument; `formal: bool = False` becomes a flag (`--formal/--no-formal`).
* Testing tip (Click): `typer.run()` is a convenience wrapper; for tests, build a Click command and invoke it:
  * `from click.testing import CliRunner`
  * `import typer`
  * `runner = CliRunner()`
  * `command = typer.main.get_command(main)`
  * `result = runner.invoke(command, ["Alice"])`
  * Note: invoking with no args (missing required `name`) prints usage/help and exits with a non-zero exit code:
    * `result = runner.invoke(command, [])`
    * `assert result.exit_code != 0`
    * `assert "Usage:" in result.output`

### Multi-command app with `typer.Typer()` and `@app.command()` ✅ Current
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str, formal: bool = False) -> None:
    if formal:
        print(f"Good day, {name}.")
    else:
        print(f"Hello {name}")

@app.command()
def goodbye(name: str) -> None:
    print(f"Goodbye {name}")

if __name__ == "__main__":
    app()
```
* Use when you need subcommands; each decorated function becomes a command (default name is the function name).
* Run as `python main.py hello Camila` or `python main.py goodbye Camila`.
* Note: Running `python main.py` (with no subcommand) shows help and exits successfully; it is not an error.
* Testing tip (Click): invoke the underlying Click command created from the Typer app:
  * `from click.testing import CliRunner`
  * `import typer`
  * `runner = CliRunner()`
  * `result = runner.invoke(typer.main.get_command(app), ["hello", "Camila"])`
  * Missing required args is an error; Click/Typer may not include the exact text `"Missing argument"` across versions, so assert on exit code and usage:
    * `result = runner.invoke(typer.main.get_command(app), ["hello"])`
    * `assert result.exit_code != 0`
    * `assert "Usage:" in result.output`
    * `assert "Error" in result.output`

### Executing a `Typer` app via `app()` (`Typer.__call__`) ✅ Current
```python
import typer

app = typer.Typer()

@app.command()
def greet(name: str) -> None:
    print(f"Hello {name}")

if __name__ == "__main__":
    app()
```
* `Typer.__call__` is the standard way to run a multi-command app from `__main__`.
* For testing, invoke the underlying Click command (not the `Typer` instance):
  * `from click.testing import CliRunner`
  * `import typer`
  * `runner = CliRunner()`
  * `result = runner.invoke(typer.main.get_command(app), ["greet", "Alice"])`
  * `assert result.exit_code == 0`
  * `assert "Hello Alice" in result.output`

### Running a typed script via the `typer` CLI program ✅ Current
```python
# main.py
def main(name: str) -> None:
    print(f"Hello {name}")
```
* Run without adding `import typer` to the script:
  * `typer main.py run --help`
  * `typer main.py run Camila`
* Requires installing the `typer` package (the `typer` CLI program is not included in `typer-slim`).

### Shell completion flags (`--install-completion`, `--show-completion`) ✅ Current
```python
import typer

def main() -> None:
    print("Try: python main.py --install-completion")

if __name__ == "__main__":
    typer.run(main)
```
* `--install-completion` installs shell completion (auto-detects shell when `shellingham` is installed).
* `--show-completion` prints the completion script for the current shell.

## Configuration

- **Types and parsing**
  - Prefer standard Python type hints on parameters (`str`, `int`, `bool`); Typer derives CLI parameter types from annotations.
- **Arguments vs options**
  - Parameters **without defaults** are typically **required positional arguments**.
  - Parameters **with defaults** are typically **options** (named flags like `--formal`), especially for `bool` which becomes `--formal/--no-formal`.
- **Completion**
  - `--install-completion` may require an explicit shell name if `shellingham` is not installed (e.g. `bash`).
- **Packaging**
  - `typer` includes the `typer` CLI tool and standard extras.
  - `typer-slim` omits extras; use `typer-slim[standard]` if you want extras like completion auto-detection.

## Pitfalls

### Wrong: Defining commands but never invoking `app()`
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

# Missing:
# if __name__ == "__main__":
#     app()
```

### Right: Invoke `app()` under `__main__`
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

if __name__ == "__main__":
    app()
```

### Wrong: Omitting the command name in a multi-command app
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

@app.command()
def goodbye(name: str) -> None:
    print(f"Goodbye {name}")

if __name__ == "__main__":
    app()

# Running like:
#   python main.py Camila
# fails because Typer expects a command name first.
```

### Right: Provide the subcommand explicitly
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

@app.command()
def goodbye(name: str) -> None:
    print(f"Goodbye {name}")

if __name__ == "__main__":
    app()

# Run:
#   python main.py hello Camila
# or:
#   python main.py goodbye Camila
```

### Wrong: Passing an argument containing spaces without quoting (shell splits it)
```python
import typer

def main(name: str) -> None:
    print(f"Hello {name}")

if __name__ == "__main__":
    typer.run(main)

# Running like:
#   python main.py Camila Gutiérrez
# passes two arguments, not one.
```

### Right: Quote values that contain spaces
```python
import typer

def main(name: str) -> None:
    print(f"Hello {name}")

if __name__ == "__main__":
    typer.run(main)

# Run:
#   python main.py "Camila Gutiérrez"
```

### Wrong: Expecting the `typer` CLI program after installing `typer-slim`
```python
# This is a packaging pitfall demonstrated as comments:
#   pip install typer-slim
# Then expecting:
#   typer main.py run
# but the `typer` command isn't installed by typer-slim.
```

### Right: Install `typer` (or `typer-slim[standard]`) when you need the CLI tool/extras
```python
# Use one of:
#   pip install typer
# or:
#   pip install "typer-slim[standard]"
#
# Then:
#   typer main.py run --help
```

### Wrong: Assuming `--install-completion` always auto-detects the shell
```python
import typer

def main() -> None:
    print("Install completion with: --install-completion")

if __name__ == "__main__":
    typer.run(main)

# Running:
#   python main.py --install-completion
# may fail to auto-detect the shell if `shellingham` isn't installed.
```

### Right: Specify the shell explicitly (or install standard extras)
```python
import typer

def main() -> None:
    print("Completion can require specifying the shell.")

if __name__ == "__main__":
    typer.run(main)

# If shell auto-detection isn't available:
#   python main.py --install-completion bash
#
# Or install extras that enable detection:
#   pip install "typer-slim[standard]"
```

## References

- [Homepage](https://github.com/fastapi/typer)
- [Documentation](https://typer.tiangolo.com)
- [Repository](https://github.com/fastapi/typer)
- [Issues](https://github.com/fastapi/typer/issues)
- [Changelog](https://typer.tiangolo.com/release-notes/)

## Migration from v[previous]

No breaking changes were provided in the input materials for v0.14.14.

Common migration path (conceptual, not version-specific):
- **Single command**: `typer.run(main)`
- **Multiple commands**: move to `app = typer.Typer()`, decorate with `@app.command()`, and execute with `app()`.

Packaging migration note:
- If you previously relied on the `typer` CLI tool, ensure you install `typer` (not plain `typer-slim`).

## API Reference

- **typer.run(main)** - Run a single-command CLI from a typed callable (argument parsing/help generated from signature).
- **typer.Typer(...)** - Create a multi-command application object (command group).
- **Typer.command(...)** - Decorator to register a function as a subcommand on the app.
- **Typer.__call__(...)** - Execute the app (parses CLI args and dispatches to the chosen command).
- **`typer` (CLI command/program)** - Run typed scripts/modules without modifying them; e.g. `typer main.py run`.
- **`typer [PATH_OR_MODULE] run`** - Subcommand to execute a typed entry function from a file/module.
- **`--install-completion`** - CLI flag to install shell completion for a Typer app.
- **`--show-completion`** - CLI flag to print the completion script for the current shell.