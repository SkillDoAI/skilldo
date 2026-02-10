---
name: click
description: A package for creating command-line interfaces (CLI) in Python.
version: 8.3.1
ecosystem: python
license: BSD-3-Clause
---

## Imports

Show the standard import patterns. Most common first:
```python
import click
from click.testing import CliRunner
```

## Core Patterns

### Basic Command ✅ Current
```python
import click

@click.command()
@click.argument('name')
@click.option('--count', default=1, help='Number of greetings.')
def greet(count: int, name: str) -> None:
    """Greets NAME for COUNT times."""
    for _ in range(count):
        click.echo(f'Hello {name}!')

if __name__ == '__main__':
    greet()
```
* This command takes a name and an optional count, greeting the name the specified number of times.
* **Status**: Current, stable

### Command Group ✅ Current
```python
import click

@click.group()
def cli() -> None:
    """A group of commands."""
    pass

@cli.command()
def command1() -> None:
    """A simple command in the group."""
    click.echo("Command 1 executed.")

@cli.command()
def command2() -> None:
    """Another command in the group."""
    click.echo("Command 2 executed.")

if __name__ == '__main__':
    cli()
```
* This pattern demonstrates how to organize commands into groups.
* **Status**: Current, stable

### Handling Bad Parameters ✅ Current
```python
import click

@click.command()
@click.argument("number", type=int)
def process_number(number: int) -> None:
    """Processes a number and checks for validity."""
    if number < 0:
        raise click.BadParameter("Number must be non-negative.")
    click.echo(f"OK: {number}")

if __name__ == "__main__":
    process_number()
```
* This command raises a `BadParameter` exception for invalid input.
* **Status**: Current, stable

### Using Click Echo ✅ Current
```python
import click

@click.command()
def say_hello() -> None:
    """Outputs a greeting."""
    click.echo('Hello World!')

if __name__ == '__main__':
    say_hello()
```
* This command simply prints "Hello World!" to the console.
* **Status**: Current, stable

### File Option ✅ Current
```python
import click

@click.command()
@click.option('--file', type=click.File('w'), help='File to write to.')
def write_file(file) -> None:
    """Writes to a file."""
    file.write("Hello, file!")
    click.echo("Written to file.")

if __name__ == '__main__':
    write_file()
```
* This command demonstrates how to use a file option to write data to a file.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Use `@click.option` for optional parameters and `@click.argument` for required parameters.
- Default values can be specified directly in the decorators.

## Pitfalls

### Wrong: Mutable Default Arguments
```python
@click.command()
@click.option('--list', default=[], help='A list of items.')
def process_items(list):
    """Processes a list of items."""
    click.echo(list)
```

### Right: Using None for Default Arguments
```python
@click.command()
@click.option('--list', default=None, help='A list of items.')
def process_items(list):
    """Processes a list of items."""
    if list is None:
        list = []
    click.echo(list)
```
* Mutable default arguments can lead to unexpected behavior across function calls.

### Wrong: Missing Await on Async Call
```python
async def fetch_data():
    response = await click.get('https://api.example.com')
```

### Right: Properly Using Await
```python
import httpx

async def fetch_data():
    async with httpx.AsyncClient() as client:
        response = await client.get('https://api.example.com')
        return response.json()
```
* Async functions need await to function properly.

### Wrong: Ignoring Eager Parameters
```python
@click.option('--foo', is_eager=False)
@click.option('--bar')
def cli(foo, bar):
    pass
```

### Right: Proper Order for Eager Parameters
```python
@click.option('--foo', is_eager=True)
@click.option('--bar')
def cli(foo, bar):
    pass
```
* Eager parameters are evaluated before non-eager parameters.

## References

- [Donate](https://palletsprojects.com/donate)
- [Documentation](https://click.palletsprojects.com/)
- [Changes](https://click.palletsprojects.com/page/changes/)
- [Source](https://github.com/pallets/click/)
- [Chat](https://discord.gg/pallets)

## Migration from v8.2.0

What changed in this version:
- **Breaking changes**: Deprecation of `BaseCommand` and `MultiCommand`. Use `Command` and `Group` instead.
- **Removal**: The `__version__` attribute has been removed. Use `importlib.metadata.version('click')` to get the version.
- **Deprecated → Current mapping**: Replace `BaseCommand` with `Command`, and `MultiCommand` with `Group`.

## API Reference

- **Command()** - Main command decorator for defining a command.
- **Option()** - Used to define options for commands.
- **Argument()** - Used to define required positional arguments.
- **echo()** - Outputs text to the console.
- **BadParameter()** - Exception raised for invalid parameters.