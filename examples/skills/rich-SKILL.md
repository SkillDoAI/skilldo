---
name: rich
description: A library for rich text and beautiful formatting in the terminal.
version: 14.3.2
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
from rich.console import Console
from rich.panel import Panel
from rich.text import Text
from rich.progress import Progress
from rich import print
```

## Core Patterns

### Console.print ✅ Current
```python
from rich.console import Console

console = Console()
console.print("Hello, World!")
```
* Prints the specified text to the console in a rich format.
* **Status**: Current, stable

### Panel.fit ✅ Current
```python
from rich.console import Console
from rich.panel import Panel

console = Console()
panel = Panel.fit("Hello, World!", padding=1)
console.print(panel)
```
* Creates a panel around the text with specified padding.
* **Status**: Current, stable

### print_json ✅ Current
```python
from rich import print_json

data = {"name": "Alice", "age": 30}
print_json(data=data)
```
* Prints JSON data in a pretty format.
* **Status**: Current, stable

### Progress ✅ Current
```python
from rich.progress import Progress

with Progress() as progress:
    task = progress.add_task("[cyan]Loading...", total=100)
    while not progress.finished:
        progress.update(task, advance=1)
```
* Displays a progress bar in the terminal.
* **Status**: Current, stable

### inspect ✅ Current
```python
from rich.inspect import inspect

class MyClass:
    def method(self):
        pass

inspect(MyClass)
```
* Inspects and displays information about the specified object.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- **Console**: Use `Console()` to instantiate a console object for printing.
- **Panel**: Create panels with the desired text and styling options.
- **Progress**: Utilize `with Progress()` for managing progress displays.

## Pitfalls

### Wrong: Not using a context manager for Console
```python
# This is incorrect and may cause resource leaks
console = Console()
console.print("Hello, World!")
```

### Right: Using a context manager for Console
```python
from rich.console import Console

with Console() as console:
    console.print("Hello, World!")
```
* Ensures proper resource management.

### Wrong: Missing await on async call
```python
# This will not work correctly in an async context
async def main():
    print("Hello, World!")
```

### Right: Using await with async functions
```python
async def main():
    await print("Hello, World!")
```
* Ensures async functions execute properly.

### Wrong: Using mutable defaults in functions
```python
def my_function(arg=[]):
    arg.append(1)
    return arg
```

### Right: Using immutables as defaults
```python
def my_function(arg=None):
    if arg is None:
        arg = []
    arg.append(1)
    return arg
```
* Prevents unexpected behavior due to mutable default arguments.

## References

- [Official Documentation](https://rich.readthedocs.io/)
- [GitHub Repository](https://github.com/Textualize/rich)

## Migration from v14.2

What changed in this version:
- **Breaking changes**: Review the changes to `cells.cell_len` that now includes a `unicode_version` parameter.
- **Deprecated → Current mapping**: Ensure to migrate any deprecated usages as noted in the changelog.

## API Reference

- **Console()** - Constructor for creating a console object for output.
- **print()** - Prints formatted text to the console.
- **Panel()** - Creates a panel with specified content and styles.
- **Progress()** - Manages progress displays in the terminal.
- **inspect()** - Inspects objects and displays their attributes and methods.