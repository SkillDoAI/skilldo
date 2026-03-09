---
name: typer
description: A library for building CLI applications in Python, based on Click
version: 0.24.1
ecosystem: python
license: MIT
generated_with: gpt-4.1
---

## Imports

<!--
List the main imports needed for using Typer’s public API.
(Stub: expand as needed.)
-->
```python
import typer
from typer.testing import CliRunner
```

## Core Patterns

<!--
Describe the core usage patterns for Typer.
(Stub: expand as needed.)
-->
- Define a CLI app with `typer.Typer()`.
- Decorate functions with `@app.command()`.
- Use `typer.Option`, `typer.Argument`, etc. for argument parsing.

## Pitfalls

<!--
Document any common pitfalls or gotchas.
(Stub: expand as needed.)
-->
- Missing or misused decorators may prevent commands from registering.
- Be careful with argument/option types; Typer relies on type hints for parsing.
- Remember to call `app()` or `app(prog_name=...)` in test scenarios.

---

## Usage Patterns

```json
[
  {
    "api": "typer.Typer.command (decorator)",
    "setup_code": [
      "import typer",
      "from typing import Annotated",
      "from typer.testing import CliRunner",
      "runner = CliRunner()",
      "app = typer.Typer()"
    ],
    "usage_pattern": [
      "@app.command()",
      "def cmd(force: Annotated[bool, typer.Option('--force')] = False):",
      "    if force:",
      "        print('Forced!')",
      "    else:",
      "        print('Not forcing')",
      "",
      "result = runner.invoke(app, ['cmd'])",
      "assert 'Not forcing' in result.output",
      "",
      "result = runner.invoke(app, ['cmd', '--force'])",
      "assert 'Forced!' in result.output"
    ]
  },
  {
    "api": "typer.Option (unsupported params) via @app.command()",
    "setup_code": [
      "import pytest",
      "import typer",
      "app = typer.Typer()"
    ],
    "usage_pattern": {
      "decorator_and_signature": "@app.command()",
      "call": "def cmd(opt: float | None = typer.Option(3.14, is_flag=True, flag_value='42', help='Some wonderful number')):",
      "body": "    pass",
      "context": "Option used as default value for a command parameter annotated as `float | None`"
    },
    "assertions": [
      "Wrap command registration in `pytest.warns(...)` and assert the warning message matches: \"The 'is_flag' and 'flag_value' parameters are not supported by Typer\""
    ],
    "deprecation_marker": "⚠️"
  },
  {
    "api": "typer.launch (Unix – open / xdg-open)",
    "setup_code": [
      "import subprocess",
      "from unittest.mock import patch",
      "import pytest",
      "import typer",
      "url = 'http://example.com'"
    ],
    "usage_pattern": {
      "parametrized_cases": [
        { "system": "Darwin", "expected_command": "open" },
        { "system": "Linux", "expected_command": "xdg-open" },
        { "system": "FreeBSD", "expected_command": "xdg-open" }
      ],
      "mocks": [
        "patch('platform.system', return_value=system)",
        "patch('shutil.which', return_value=True)",
        "patch('subprocess.Popen') as mock_popen"
      ],
      "call": "typer.launch(url)"
    },
    "assertions": [
      "subprocess.Popen is called once with [[expected_command, url], stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT]"
    ]
  },
  {
    "api": "typer.launch (Windows – webbrowser.open)",
    "setup_code": [
      "from unittest.mock import patch",
      "import typer",
      "url = 'http://example.com'"
    ],
    "usage_pattern": {
      "mocks": [
        "patch('platform.system', return_value='Windows')",
        "patch('webbrowser.open') as mock_webbrowser_open"
      ],
      "call": "typer.launch(url)"
    },
    "assertions": [
      "webbrowser.open is called once with the URL"
    ]
  },
  {
    "api": "typer.launch (fallback when xdg-open missing)",
    "setup_code": [
      "from unittest.mock import patch",
      "import typer",
      "url = 'http://example.com'"
    ],
    "usage_pattern": {
      "mocks": [
        "patch('platform.system', return_value='Linux')",
        "patch('shutil.which', return_value=None)",
        "patch('webbrowser.open') as mock_webbrowser_open"
      ],
      "call": "typer.launch(url)"
    },
    "assertions": [
      "webbrowser.open is called once with the URL"
    ]
  },
  {
    "api": "typer.launch (non‑URL – delegated to click.launch)",
    "setup_code": [
      "from unittest.mock import patch",
      "import typer"
    ],
    "usage_pattern": {
      "mocks": [
        "patch('typer.main.click.launch', return_value=0) as launch_mock"
      ],
      "call": "typer.launch('not a url')"
    },
    "assertions": [
      "click.launch is called once with the original argument"
    ]
  },
  {
    "api": "Optional string argument (user: str | None = None)",
    "setup_code": [
      "import typer",
      "from typer.testing import CliRunner",
      "runner = CliRunner()",
      "app = typer.Typer()"
    ],
    "usage_pattern": [
      "@app.command()",
      "def opt(user: str | None = None):",
      "    if user:",
      "        print(f'User: {user}')",
      "    else:",
      "        print('No user')",
      "",
      "result = runner.invoke(app, ['opt'])",
      "assert 'No user' in result.output",
      "",
      "result = runner.invoke(app, ['opt', '--user', 'Camila'])",
      "assert 'User: Camila' in result.output"
    ]
  },
  {
    "api": "Optional tuple argument (tuple[int, int] | None)",
    "setup_code": [
      "import typer",
      "from typer.testing import CliRunner",
      "runner = CliRunner()",
      "app = typer.Typer()"
    ],
    "usage_pattern": [
      "@app.command()",
      "def opt(number: tuple[int, int] | None = None):",
      "    if number:",
      "        print(f'Number: {number}')",
      "    else:",
      "        print('No number')",
      "",
      "result = runner.invoke(app, ['opt'])",
      "assert 'No number' in result.output",
      "",
      "result = runner.invoke(app, ['opt', '--number', '4', '2'])",
      "assert 'Number: (4, 2)' in result.output"
    ]
  },
  {
    "api": "Positional argument without type annotation",
    "setup_code": [
      "import typer",
      "from typer.testing import CliRunner",
      "runner = CliRunner()",
      "app = typer.Typer()"
    ],
    "usage_pattern": [
      "@app.command()",
      "def no_type(user):",
      "    print(f'User: {user}')",
      "",
      "result = runner.invoke(app, ['no_type', 'Camila'])",
      "assert 'User: Camila' in result.output"
    ]
  },
  {
    "api": "List parameter conversion (list[Path], list[Enum], list[str])",
    "setup_code": [
      "import typer",
      "from typer.testing import CliRunner",
      "from pathlib import Path",
      "from enum import Enum",
      "runner = CliRunner()",
      "app = typer.Typer()",
      "class SomeEnum(Enum):",
      "    ONE = 'one'",
      "    TWO = 'two'",
      "    THREE = 'three'"
    ],
    "usage_pattern": [
      "@app.command()",
      "def list_conversion(container: list[Path] | list[SomeEnum] | list[str]):",
      "    print(container)",
      "",
      "result = runner.invoke(app, ['list_conversion', 'one', 'two', 'three'])",
      "assert result.exit_code == 0"
    ]
  },
  {
    "api": "Tuple parameter recursive conversion",
    "setup_code": [
      "import typer",
      "from typer.testing import CliRunner",
      "from pathlib import Path",
      "from enum import Enum",
      "runner = CliRunner()",
      "app = typer.Typer()",
      "class SomeEnum(Enum):",
      "    ONE = 'one'",
      "    TWO = 'two'",
      "    THREE = 'three'"
    ],
    "usage_pattern": [
      "@app.command()",
      "def tuple_recursive_conversion(container: tuple[str, Path] | tuple[SomeEnum, SomeEnum]):",
      "    print(container)",
      "",
      "result = runner.invoke(app, ['tuple_recursive_conversion', 'one', 'two'])",
      "assert result.exit_code == 0"
    ]
  },
  {
    "api": "Custom parser via typer.Argument",
    "setup_code": [
      "import typer",
      "from typer.testing import CliRunner",
      "runner = CliRunner()",
      "app = typer.Typer()"
    ],
    "usage_pattern": [
      "@app.command()",
      "def custom_parser(hex_value: int = typer.Argument(None, parser=lambda x: int(x, 0))):",
      "    assert hex_value == 0x56",
      "",
      "result = runner.invoke(app, ['custom_parser', '0x56'])",
      "assert result.exit_code == 0"
    ]
  },
  {
    "api": "Custom Click type via typer.Argument",
    "setup_code": [
      "import typer",
      "import click",
      "from typer.testing import CliRunner",
      "runner = CliRunner()",
      "app = typer.Typer()",
      "class BaseNumberParamType(click.ParamType):",
      "    name = 'base_integer'",
      "    def convert(self, value, param, ctx):",
      "        return int(value, 0)"
    ],
    "usage_pattern": [
      "@app.command()",
      "def custom_click_type(hex_value: int = typer.Argument(None, click_type=BaseNumberParamType())):",
      "    assert hex_value == 0x56",
      "",
      "result = runner.invoke(app, ['custom_click_type', '0x56'])",
      "assert result.exit_code == 0"
    ]
  },
  {
    "api": "_sanitize_help_text (completion helper)",
    "setup_code": [
      "import pytest",
      "from typer._completion_classes import _sanitize_help_text",
      "from unittest.mock import patch"
    ],
    "usage_pattern": [
      "# Example help text containing Rich markup",
      "help_text = \"[bold]Important[/bold] message\"",
      "",
      "# Case 1: Rich is NOT installed – the text is returned unchanged",
      "with patch('importlib.util.find_spec', return_value=None):",
      "    cleaned = _sanitize_help_text(help_text)",
      "    assert cleaned == help_text",
      "",
      "# Case 2: Rich IS installed – markup is stripped",
      "mock_spec = object()  # any truthy value signals Rich is present",
      "with patch('importlib.util.find_spec', return_value=mock_spec):",
      "    cleaned = _sanitize_help_text(help_text)",
      "    assert cleaned == \"Important message\""
    ],
    "assertions": [
      "When Rich is not installed (find_spec returns None), the original help text is returned unchanged.",
      "When Rich is installed, any Rich markup (e.g., [bold]…[/]) is stripped from the help text."
    ]
  }
]
```

**Notes:**
- All APIs shown are exported via `typer/__init__.py` and thus have `"publicity_score": "high"`.
- Many are re‑exports of Click APIs (documented as part of Typer's public API surface).
- Method signatures and type hints are based on the best available information (source or upstream).
- No deprecation found for any public API in this surface, except the soft warning for unsupported `is_flag`/`flag_value` parameters on `typer.Option` (marked with ⚠️).

---

## Migration

- **Breaking change (0.22.0+ including 0.24.1):** `typer-slim` is no longer a slim distribution; it installs full Typer.
  - **Migration:** Update installation instructions to `pip install typer`. If you want to disable Rich formatting globally, set `TYPER_USE_RICH=0` (or `False`) when running your CLI.

- **Breaking change (0.24.0 → 0.24.1):** Strict validation for `bool` options.
  - **Migration:** Ensure boolean options have an explicit default (`True` or `False`). If you need the classic flag‑only behaviour, use `typer.Option(..., is_flag=True)` explicitly.

## References

- [Homepage](https://github.com/fastapi/typer)
- [Documentation](https://typer.tiangolo.com)
- [Repository](https://github.com/fastapi/typer)
- [Issues](https://github.com/fastapi/typer/issues)
- [Changelog](https://typer.tiangolo.com/release-notes/)


## Current Library State (from source analysis)

### API Surface
```json
{
  "library_category": "cli",
  "apis": [
    {
      "name": "typer.__version__",
      "type": "variable",
      "signature": "\"0.24.1\"",
      "return_type": "str",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.get_terminal_size",
      "type": "function",
      "signature": "get_terminal_size(fallback=(80, 24))",
      "return_type": "os.terminal_size",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of shutil.get_terminal_size"
    },

    {
      "name": "typer.Abort",
      "type": "class",
      "signature": "Abort(message: str | None = None)",
      "return_type": "Abort",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.exceptions.Abort"
    },

    {
      "name": "typer.BadParameter",
      "type": "class",
      "signature": "BadParameter(message: str, ctx: click.Context | None = None, param: click.Parameter | None = None, param_hint: str | None = None)",
      "return_type": "BadParameter",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.exceptions.BadParameter"
    },

    {
      "name": "typer.Exit",
      "type": "class",
      "signature": "Exit(code: int = 0)",
      "return_type": "Exit",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.exceptions.Exit"
    },

    {
      "name": "typer.clear",
      "type": "function",
      "signature": "clear() -> None",
      "return_type": "None",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.clear"
    },

    {
      "name": "typer.confirm",
      "type": "function",
      "signature": "confirm(text: str, default: bool = False, abort: bool = False, prompt_suffix: str = ': ', show_default: bool = True, err: bool = False) -> bool",
      "return_type": "bool",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.confirm"
    },

    {
      "name": "typer.echo_via_pager",
      "type": "function",
      "signature": "echo_via_pager(generator: collections.abc.Iterable[str] | str, color: bool | None = None) -> None",
      "return_type": "None",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.echo_via_pager"
    },

    {
      "name": "typer.edit",
      "type": "function",
      "signature": "edit(text: str | None = None, editor: str | None = None, env: dict[str, str] | None = None, require_save: bool = True, extension: str = '.txt', filename: str | None = None) -> str | None",
      "return_type": "str | None",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.edit"
    },

    {
      "name": "typer.getchar",
      "type": "function",
      "signature": "getchar(echo: bool = False) -> str",
      "return_type": "str",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.getchar"
    },

    {
      "name": "typer.pause",
      "type": "function",
      "signature": "pause(info: str | None = None, err: bool = False) -> None",
      "return_type": "None",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.pause"
    },

    {
      "name": "typer.progressbar",
      "type": "function",
      "signature": "progressbar(iterable=None, length: int | None = None, label: str | None = None, show_eta: bool = True, show_percent: bool | None = None, show_pos: bool = False, item_show_func=None, fill_char: str = '#', empty_char: str = '-', bar_template: str = '%(label)s  [%(bar)s]  %(info)s', info_sep: str = '  ', width: int = 36, file=None, color: bool | None = None, update_min_steps: int = 1) -> click.termui.ProgressBar",
      "return_type": "click.termui.ProgressBar",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.progressbar"
    },

    {
      "name": "typer.prompt",
      "type": "function",
      "signature": "prompt(text: str, default=None, hide_input: bool = False, confirmation_prompt: bool | str = False, type=None, value_proc=None, prompt_suffix: str = ': ', show_default: bool = True, err: bool = False, show_choices: bool = True) -> Any",
      "return_type": "Any",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.prompt"
    },

    {
      "name": "typer.secho",
      "type": "function",
      "signature": "secho(message: Any | None = None, file=None, nl: bool = True, err: bool = False, color: bool | None = None, **styles) -> None",
      "return_type": "None",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.secho"
    },

    {
      "name": "typer.style",
      "type": "function",
      "signature": "style(text: Any, fg: str | int | tuple[int, int, int] | None = None, bg: str | int | tuple[int, int, int] | None = None, bold: bool | None = None, dim: bool | None = None, underline: bool | None = None, overline: bool | None = None, italic: bool | None = None, blink: bool | None = None, reverse: bool | None = None, strikethrough: bool | None = None, reset: bool = True) -> str",
      "return_type": "str",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.style"
    },

    {
      "name": "typer.unstyle",
      "type": "function",
      "signature": "unstyle(text: str) -> str",
      "return_type": "str",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.termui.unstyle"
    },

    {
      "name": "typer.echo",
      "type": "function",
      "signature": "echo(message: Any = None, file: Any = None, nl: bool = True, err: bool = False, color: bool | None = None) -> None",
      "return_type": "None",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.utils.echo"
    },

    {
      "name": "typer.format_filename",
      "type": "function",
      "signature": "format_filename(filename: str, shorten: bool = False) -> str",
      "return_type": "str",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.utils.format_filename"
    },

    {
      "name": "typer.get_app_dir",
      "type": "function",
      "signature": "get_app_dir(app_name: str, roaming: bool = True, force_posix: bool = False) -> str",
      "return_type": "str",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.utils.get_app_dir"
    },

    {
      "name": "typer.get_binary_stream",
      "type": "function",
      "signature": "get_binary_stream(name: str) -> typing.BinaryIO",
      "return_type": "BinaryIO",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.utils.get_binary_stream"
    },

    {
      "name": "typer.get_text_stream",
      "type": "function",
      "signature": "get_text_stream(name: str, encoding: str | None = None, errors: str | None = 'strict') -> typing.TextIO",
      "return_type": "TextIO",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.utils.get_text_stream"
    },

    {
      "name": "typer.open_file",
      "type": "function",
      "signature": "open_file(filename: str, mode: str = 'r', encoding: str | None = None, errors: str | None = 'strict', lazy: bool = False, atomic: bool = False) -> typing.IO[Any]",
      "return_type": "IO[Any]",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Re-export of click.utils.open_file"
    },

    {
      "name": "typer.colors",
      "type": "descriptor",
      "signature": "colors: module",
      "return_type": "module",
      "module": "typer.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.Typer",
      "type": "class",
      "signature": "Typer(*, add_completion: bool = True, no_args_is_help: bool = False, invoke_without_command: bool = False, **kwargs)",
      "return_type": "Typer",
      "module": "typer.main",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Full constructor signature defined in typer/main.py."
    },

    {
      "name": "typer.run",
      "type": "function",
      "signature": "run(app: Typer, *, prog_name: str | None = None, args: list[str] | None = None, **kwargs) -> int",
      "return_type": "int",
      "module": "typer.main",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Runs a Typer app; imported from typer.main."
    },

    {
      "name": "typer.launch",
      "type": "function",
      "signature": "launch(url: str, wait: bool = False, locate: bool = False) -> int",
      "return_type": "int",
      "module": "typer.main",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false },
      "note": "Opens URLs or delegates to click.launch for non‑URLs."
    },

    {
      "name": "typer.CallbackParam",
      "type": "class",
      "signature": "CallbackParam(name: str, type: type | None = None, default: Any = None, ...)",
      "return_type": "CallbackParam",
      "module": "typer.models",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.Context",
      "type": "class",
      "signature": "Context(...)",
      "return_type": "Context",
      "module": "typer.models",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.FileBinaryRead",
      "type": "descriptor",
      "signature": "FileBinaryRead: typing.TypeAlias",
      "return_type": "TypeAlias",
      "module": "typer.models",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.FileBinaryWrite",
      "type": "descriptor",
      "signature": "FileBinaryWrite: typing.TypeAlias",
      "return_type": "TypeAlias",
      "module": "typer.models",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.FileText",
      "type": "descriptor",
      "signature": "FileText: typing.TypeAlias",
      "return_type": "TypeAlias",
      "module": "typer.models",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.FileTextWrite",
      "type": "descriptor",
      "signature": "FileTextWrite: typing.TypeAlias",
      "return_type": "TypeAlias",
      "module": "typer.models",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.Argument",
      "type": "function",
      "signature": "Argument(..., default: Any = ..., help: str | None = None, ...)",
      "return_type": "Any",
      "module": "typer.params",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.Option",
      "type": "function",
      "signature": "Option(..., default: Any = ..., help: str | None = None, ...)",
      "return_type": "Any",
      "module": "typer.params",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.colors.BLACK",
      "type": "variable",
      "signature": "\"black\"",
      "return_type": "str",
      "module": "typer.colors",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.colors.RED",
      "type": "variable",
      "signature": "\"red\"",
      "return_type": "str",
      "module": "typer.colors",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.colors.GREEN",
      "type": "variable",
      "signature": "\"green\"",
      "return_type": "str",
      "module": "typer.colors",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.colors.YELLOW",
      "type": "variable",
      "signature": "\"yellow\"",
      "return_type": "str",
      "module": "typer.colors",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.colors.BLUE",
      "type": "variable",
      "signature": "\"blue\"",
      "return_type": "str",
      "module": "typer.colors",
      "publicity_score": "high",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.completion.get_completion_inspect_parameters",
      "type": "function",
      "signature": "get_completion_inspect_parameters() -> tuple[ParamMeta, ParamMeta]",
      "return_type": "tuple[ParamMeta, ParamMeta]",
      "module": "typer.completion",
      "publicity_score": "medium",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.completion.install_callback",
      "type": "function",
      "signature": "install_callback(ctx: click.Context, param: click.Parameter, value: Any) -> Any",
      "return_type": "Any",
      "module": "typer.completion",
      "publicity_score": "medium",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.completion.show_callback",
      "type": "function",
      "signature": "show_callback(ctx: click.Context, param: click.Parameter, value: Any) -> Any",
      "return_type": "Any",
      "module": "typer.completion",
      "publicity_score": "medium",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.completion.shell_complete",
      "type": "function",
      "signature": "shell_complete(cli: click.Command, ctx_args: collections.abc.MutableMapping[str, Any], prog_name: str, complete_var: str, instruction: str) -> int",
      "return_type": "int",
      "module": "typer.completion",
      "publicity_score": "medium",
      "module_type": "public",
      "deprecation": {
        "is_deprecated": true,
        "severity": "soft",
        "reason": "Compatibility shim for Click 8.x shell completion."
      }
    },

    {
      "name": "typer.core.MARKUP_MODE_KEY",
      "type": "descriptor",
      "signature": "\"TYPER_RICH_MARKUP_MODE\"",
      "return_type": "str",
      "module": "typer.core",
      "publicity_score": "medium",
      "module_type": "internal",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.core.HAS_RICH",
      "type": "descriptor",
      "signature": "bool",
      "return_type": "bool",
      "module": "typer.core",
      "publicity_score": "medium",
      "module_type": "internal",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.core.DEFAULT_MARKUP_MODE",
      "type": "descriptor",
      "signature": "MarkupMode",
      "return_type": "MarkupMode",
      "module": "typer.core",
      "publicity_score": "medium",
      "module_type": "internal",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.cli.TyperCLIGroup",
      "type": "class",
      "signature": "TyperCLIGroup(*args: Any, **kwargs: Any)",
      "return_type": "TyperCLIGroup",
      "module": "typer.cli",
      "publicity_score": "medium",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.cli.print_version",
      "type": "function",
      "signature": "print_version(ctx: click.Context, param: Option, value: bool) -> None",
      "return_type": "None",
      "module": "typer.cli",
      "publicity_score": "medium",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    },

    {
      "name": "typer.cli.docs",
      "type": "function",
      "signature": "docs(ctx: typer.Context, name: str = \"\", output: Path | None = None, title: str | None = None) -> None",
      "return_type": "None",
      "module": "typer.cli",
      "publicity_score": "medium",
      "module_type": "public",
      "deprecation": { "is_deprecated": false }
    }
  ]
}
```