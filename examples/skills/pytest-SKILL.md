---

name: pytest
description: Python testing framework that discovers tests, provides fixtures, and offers rich assertion introspection.
version: 3.9.3
ecosystem: python
license: MIT
generated_with: gpt-5.2
---

## Imports

```python
import pytest
from pytest import fixture, fail, raises
```

## Core Patterns

### Define and use fixtures (dependency injection) ✅ Current
```python
import pytest

@pytest.fixture()
def fruit() -> str:
    return "apple"

def test_fruit_value(fruit: str) -> None:
    assert fruit == "apple"
```
* Request fixtures by naming them as test function parameters.
* Prefer fixtures over manual setup/teardown in tests.

### Assert exceptions with pytest.raises ✅ Current
```python
import pytest

def parse_port(value: str) -> int:
    port = int(value)
    if not (0 <= port <= 65535):
        raise ValueError("port out of range")
    return port

def test_parse_port_rejects_out_of_range() -> None:
    with pytest.raises(ValueError, match="out of range"):
        parse_port("70000")
```
* Use `match=` to assert on stable parts of the error message.

### Fail fast with pytest.fail ✅ Current
```python
import pytest

def test_requires_feature_flag() -> None:
    feature_enabled = False
    if not feature_enabled:
        pytest.fail("feature flag must be enabled for this test")
```
* Use `pytest.fail(...)` for explicit, readable failure paths (e.g., unreachable branches).

### Parametrize tests with pytest.mark.parametrize ✅ Current
```python
import pytest

def add(a: int, b: int) -> int:
    return a + b

@pytest.mark.parametrize(
    ("a", "b", "expected"),
    [(1, 2, 3), (0, 0, 0), (-1, 1, 0)],
)
def test_add(a: int, b: int, expected: int) -> None:
    assert add(a, b) == expected
```
* Prefer parametrization over loops inside a single test for clearer reporting.

### Use tmp_path for filesystem isolation ✅ Current
```python
from __future__ import annotations

from pathlib import Path

def test_writes_file(tmp_path: Path) -> None:
    out = tmp_path / "out.txt"
    out.write_text("hello", encoding="utf-8")
    assert out.read_text(encoding="utf-8") == "hello"
```
* `tmp_path` provides a unique `pathlib.Path` per test for safe filesystem operations.

## Configuration

* **Config formats**: `pytest.ini`, `pyproject.toml` (under `[tool.pytest.ini_options]`), `tox.ini`, `setup.cfg`.
* **Common settings**:
  * `testpaths`: directories to search for tests.
  * `python_files`, `python_classes`, `python_functions`: discovery patterns.
  * `addopts`: default CLI args (e.g., `-q`, `-ra`).
  * `markers`: register custom markers to avoid warnings.
* **INI “pathlist” values**: options declared by plugins/conftest via `parser.addini(..., type="pathlist")` are returned as paths relative to the config file directory.
* **Environment variables affecting output**:
  * `CI` or `BUILD_NUMBER`: pytest adapts output for CI logs (notably: short test summary is not truncated to terminal width). Avoid tests that assert on truncation/terminal-width-dependent formatting.

## Pitfalls

### Wrong: Referencing a fixture function instead of requesting it
```python
import pytest

@pytest.fixture
def fruit() -> str:
    return "apple"

def test_fruit_value() -> None:
    # This refers to the function object, not the fixture value.
    assert fruit == "apple"
```

### Right: Request the fixture by name as a parameter
```python
import pytest

@pytest.fixture
def fruit() -> str:
    return "apple"

def test_fruit_value(fruit: str) -> None:
    assert fruit == "apple"
```

### Wrong: Asserting on terminal-width-dependent output (brittle in CI)
```python
def test_output_truncation_assumption(capsys) -> None:
    print("short test summary info " + ("x" * 200))
    out = capsys.readouterr().out
    # Brittle: assumes truncation that may not happen in CI.
    assert out.endswith("...\n")
```

### Right: Assert on stable substrings instead
```python
def test_output_contains_stable_substring(capsys) -> None:
    print("short test summary info " + ("x" * 200))
    out = capsys.readouterr().out
    assert "short test summary info" in out
```

### Wrong: Overly strict exception message checks
```python
import pytest

def f() -> None:
    raise ValueError("bad value: 42")

def test_exception_message_too_strict() -> None:
    # Too strict: minor message changes can break the test.
    with pytest.raises(ValueError, match=r"^bad value: 42$"):
        f()
```

### Right: Match only the stable part of the message
```python
import pytest

def f() -> None:
    raise ValueError("bad value: 42")

def test_exception_message_stable_match() -> None:
    with pytest.raises(ValueError, match=r"bad value"):
        f()
```

### Wrong: Writing incorrect expectations and ignoring assertion introspection
```python
def inc(x: int) -> int:
    return x + 1

def test_inc_wrong_expected_value() -> None:
    assert inc(3) == 5
```

### Right: Fix the expected value; rely on plain assert
```python
def inc(x: int) -> int:
    return x + 1

def test_inc_correct_expected_value() -> None:
    assert inc(3) == 4
```

## References

- [Official Documentation](https://docs.pytest.org/en/stable/)
- [GitHub Repository](https://github.com/pytest-dev/pytest)

## Migration from v[previous]

No specific breaking-change notes were provided for v3.9.3 in the given context.

Deprecated-to-current guidance reflected in the test patterns:
* Prefer `tmp_path`/`tmp_path_factory` (`pathlib.Path`) over legacy `tmpdir`/`tmpdir_factory` (py.path-style).
* Prefer public fixtures like `request` instead of constructing internal request objects directly.

## API Reference

- **pytest.fixture** - Define a fixture; key params: `scope`, `params`, `autouse`, `name`.
- **pytest.fail** - Immediately fail a test with a message; key params: `reason`, `pytrace`.
- **pytest.raises** - Assert an exception is raised; key params: expected exception type(s), `match`.
- **pytest.mark.parametrize** - Parametrize a test function over input cases; key params: argnames, argvalues, `ids`.
- **tmp_path (fixture)** - Per-test temporary directory as `pathlib.Path`.
- **tmp_path_factory (fixture)** - Session-scoped factory for temporary paths; method: `getbasetemp()`.
- **capsys (fixture)** - Capture `stdout`/`stderr` from Python-level writes; method: `readouterr()`.
- **capfd (fixture)** - Capture `stdout`/`stderr` at the file-descriptor level; method: `readouterr()`.
- **monkeypatch (fixture)** - Temporarily modify environment/attributes; methods: `setattr`, `setenv`, `chdir`.
- **pytestconfig (fixture)** - Access configuration and CLI options; common method: `getoption(...)`.
- **request (fixture)** - Introspect the requesting test context; common method: `getfixturevalue(name)`.
- **pytest.skip** - Skip at runtime; key params: `reason`, `allow_module_level`.
- **pytest.importorskip** - Skip if an import fails; key params: module name, `minversion`, `reason`.
- **pytest.xfail** - Mark expected failure at runtime; key params: `reason`, `strict`.
- **pytest.mark** - Namespace for markers (e.g., `skip`, `xfail`, custom markers).