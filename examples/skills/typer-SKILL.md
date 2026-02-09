---
name: typer
description: Build command-line interfaces from Python functions using type hints.
version: 0.14.14
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
import typer

from typer import Typer, run
```

## Core Patterns

The right way to use the main APIs. Show 3-5 most common patterns.

**CRITICAL: Prioritize PUBLIC APIs over internal/compat modules**
- Use APIs from api_surface with `publicity_score: "high"` first
- Avoid `.compat`, `.internal`, `._private` modules unless they're the only option
- Example: Prefer `library.MainClass` over `library.compat.helper_function`

**CRITICAL: Mark deprecation status with clear indicators**

### Single-command script with `typer.run()` ✅ Current
```python
import typer

def main(
    name: str = typer.Argument(..., help="Name to greet."),
    formal: bool = typer.Option(False, "--formal", help="Use a formal greeting."),
) -> None:
    if formal:
        print(f"Goodbye Ms. {name}. Have a good day.")
    else:
        print(f"Bye {name}!")

if __name__ == "__main__":
    # Creates an app implicitly and runs `main` as the single command.
    # Example:
    #   python main.py Camila
    #   python main.py Camila --formal
    typer.run(main)
```
* Use for one-command CLIs (scripts).
* Typer infers CLI parameter types from type hints.
* **Status**: Current, stable

### Multi-command CLI with `Typer()` + `@app.command()` ✅ Current
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

@app.command()
def goodbye(name: str, formal: bool = False) -> None:
    if formal:
        print(f"Goodbye Ms. {name}. Have a good day.")
    else:
        print(f"Bye {name}!")

if __name__ == "__main__":
    # With multiple commands, users must specify the command name:
    #   python main.py hello Camila
    #   python main.py goodbye Camila --formal
    app()
```
* Use for command groups (subcommands).
* `@app.command()` registers a Python function as a CLI command.
* **Status**: Current, stable

### App instance invocation via `Typer.__call__` (`app()`) ✅ Current
```python
import typer

app = typer.Typer()

@app.command()
def ping() -> None:
    print("pong")

if __name__ == "__main__":
    # Invoke the Typer app to parse argv and dispatch commands.
    # With a multi-command app, you must provide the command name:
    #   python main.py ping
    app(prog_name="main.py")
```
* `Typer.__call__` is what runs the CLI when you do `app()`.
* Keep `app()` in the executable entrypoint (`__main__` guard) for scripts.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Default values
  - Function parameters become CLI parameters.
  - Parameters with defaults become options; parameters without defaults become arguments (Typer/Click convention).
  - `bool` defaults (e.g. `formal: bool = False`) become flags.
- Common customizations
  - Use `@app.command()` to add more commands to a `Typer()` app.
  - Use clear type hints (`str`, `int`, `bool`) so Typer can infer parsing.
- Environment variables
  - Not covered in the provided context. (Typer supports various option sources via Click/Typer features, but only documented APIs provided are listed above.)
- Config file formats
  - Not covered in the provided context.

## Pitfalls

CRITICAL: This section is MANDATORY. Show 3-5 common mistakes with specific Wrong/Right examples.

### Wrong: Creating a `Typer()` app but never calling it
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

if __name__ == "__main__":
    # Script runs but does nothing because the app isn't invoked.
    pass
```

### Right: Call the app (runs `Typer.__call__`)
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

if __name__ == "__main__":
    app()
```

### Wrong: Omitting the command name when the app has multiple commands
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

@app.command()
def goodbye(name: str) -> None:
    print(f"Bye {name}!")

if __name__ == "__main__":
    app()

# User tries:
#   python main.py Camila
# This fails because Typer needs the command name (hello/goodbye).
```

### Right: Include the command name for multi-command apps
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

@app.command()
def goodbye(name: str)) -> None:
    print(f"Bye {name}!")

if __name__ == "__main__":
    app()

# Correct usage:
#   python main.py hello Camila
#   python main.py goodbye Camila
```

### Wrong: Passing an argument with spaces without quoting (shell splits it)
```python
import typer

def main(name: str) -> None:
    print(f"Hello {name}")

if __name__ == "__main__":
    typer.run(main)

# User runs:
#   python main.py Camila Gutiérrez
# The shell passes two tokens; Typer expects one argument.
```

### Right: Quote arguments that contain spaces
```python
import typer

def main(name: str) -> None:
    print(f"Hello {name}")

if __name__ == "__main__":
    typer.run(main)

# Correct usage:
#   python main.py "Camila Gutiérrez"
```

### Wrong: Missing type hints (Typer can’t infer intended parsing reliably)
```python
import typer

def main(total, force=False):
    # Without type hints, Typer has less information to infer types/flags.
    print(total, force)

if __name__ == "__main__":
    typer.run(main)
```

### Right: Add type hints so Typer infers argument/option types
```python
import typer

def main(total: int, force: bool = False) -> None:
    print(total, force)

if __name__ == "__main__":
    typer.run(main)
```

## References

CRITICAL: Include ALL provided URLs below (do NOT skip this section):

- [Homepage](https://github.com/fastapi/typer)
- [Documentation](https://typer.tiangolo.com)
- [Repository](https://github.com/fastapi/typer)
- [Issues](https://github.com/fastapi/typer/issues)
- [Changelog](https://typer.tiangolo.com/release-notes/)

## Migration from v[previous]

What changed in this version (if applicable):
- Breaking changes
  - None provided in the supplied context for v0.14.14.
- Deprecated → Current mapping
  - No deprecations provided in the supplied context.
- Before/after code examples

Conceptual migration when you outgrow a single-command script:

Before (single command via `typer.run`):
```python
import typer

def main(name: str) -> None:
    print(f"Hello {name}")

if __name__ == "__main__":
    typer.run(main)
```

After (multi-command via `Typer()` + `@app.command()` + `app()`):
```python
import typer

app = typer.Typer()

@app.command()
def hello(name: str) -> None:
    print(f"Hello {name}")

@app.command()
def goodbye(name: str, formal: bool = False) -> None:
    if formal:
        print(f"Goodbye Ms. {name}. Have a good day.")
    else:
        print(f"Bye {name}!")

if __name__ == "__main__":
    app()
```

## API Reference

Brief reference of the most important public APIs:

- **typer.run(function)** - Run a single-command CLI from a function (Typer builds an app implicitly).
- **typer.Typer()** - Create an explicit CLI app (command group) for multi-command CLIs.
- **typer.Typer.__call__() / app()** - Invoke the app to parse argv and dispatch to the selected command.
- **typer.Typer.command()** - Decorator to register a function as a command: `@app.command()`.