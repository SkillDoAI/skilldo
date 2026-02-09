---
name: pytest
description: Python testing framework that discovers tests, runs them, and provides fixtures and rich assertion reporting.
version: 3.9.3
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
import pytest

from pytest import fixture, fail
```

## Core Patterns

### Test discovery + plain asserts ✅ Current
```python
# content of: test_math.py
def test_addition() -> None:
    # Use plain assert to get pytest assertion introspection on failure.
    assert 1 + 1 == 2
```
* Put tests in `test_*.py` files and name functions `test_*` so pytest auto-discovers them.
* Prefer plain `assert` to leverage pytest introspection.
* **Status**: Current, stable

### Define and use a fixture via dependency injection ✅ Current
```python
import pytest

@pytest.fixture
def my_fruit() -> str:
    return "apple"

def test_needs_fixture(my_fruit: str) -> None:
    # The fixture value is injected by name via the test function parameter.
    assert my_fruit == "apple"
```
* Use `@pytest.fixture` to create reusable setup.
* Request fixtures by adding parameters to tests (and to other fixtures).
* **Status**: Current, stable

### Compose fixtures (fixture depends on another fixture) ✅ Current
```python
import pytest

@pytest.fixture
def base_url() -> str:
    return "https://example.invalid"

@pytest.fixture
def api_url(base_url: str) -> str:
    # Fixtures can depend on other fixtures by listing them as parameters.
    return base_url + "/api"

def test_api_url(api_url: str) -> None:
    assert api_url.endswith("/api")
```
* Use fixture composition to build layered setup without global state.
* **Status**: Current, stable

### Fail a test with an explicit message ✅ Current
```python
import pytest

def test_not_implemented_yet() -> None:
    # Use pytest.fail() when you want an explicit failure path and message.
    pytest.fail("deliberately failing to highlight missing implementation")
```
* `pytest.fail()` stops the test immediately and records a failure reason.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- **Test discovery conventions**
  - Files: `test_*.py` or `*_test.py` (common convention; `test_*.py` is widely used)
  - Functions: `test_*`
- **Fixtures**
  - Declared with `@pytest.fixture`
  - Used by naming them as parameters in tests/fixtures
- **Environment variables**
  - `CI` or `BUILD_NUMBER`: pytest detects CI and adjusts output behavior (notably: short test summary info is **not truncated** to terminal width in CI).
- **Config file formats**
  - Common pytest config locations include `pytest.ini`, `setup.cfg`, and `tox.ini` (pytest reads configuration from these when present).

## Pitfalls

### Wrong: Using a fixture without requesting it (treating it like a variable)
```python
import pytest

@pytest.fixture
def my_fruit() -> str:
    return "apple"

def test_needs_fixture() -> None:
    # WRONG: this refers to the fixture function object, not the injected value.
    assert my_fruit == "apple"
```

### Right: Request the fixture by name as a test parameter
```python
import pytest

@pytest.fixture
def my_fruit() -> str:
    return "apple"

def test_needs_fixture(my_fruit: str) -> None:
    assert my_fruit == "apple"
```

### Wrong: Calling a fixture function directly (bypasses pytest fixture lifecycle)
```python
import pytest

@pytest.fixture
def token() -> str:
    return "secret-token"

def test_token_value() -> None:
    # WRONG: calling the fixture function directly bypasses pytest's fixture system.
    t = token()  # type: ignore[misc]
    assert t == "secret-token"
```

### Right: Let pytest provide the fixture value
```python
import pytest

@pytest.fixture
def token() -> str:
    return "secret-token"

def test_token_value(token: str) -> None:
    assert token == "secret-token"
```

### Wrong: Brittle assertions about CI-dependent truncated output formatting
```python
# This example is illustrative of a brittle pattern:
# tests that assert exact truncated output may fail on CI.

def test_brittle_output_parsing() -> None:
    # WRONG: relying on '...' truncation is not stable across environments.
    assert "...truncated..." == "...truncated..."
```

### Right: Assert on stable content (don’t depend on truncation behavior)
```python
import pytest

def test_failure_message_is_stable() -> None:
    # Prefer asserting on stable, semantic content in your own code/tests.
    # If you need a controlled failure:
    pytest.fail("deliberately failing")
```

## References

CRITICAL: Include ALL provided URLs below (do NOT skip this section):

- [Official Documentation](https://docs.pytest.org/en/3.9.3/)
- [GitHub Repository](https://github.com/pytest-dev/pytest)

## Migration from v[previous]

What changed in this version (if applicable):
- Breaking changes: Not provided in the input context.
- Deprecated → Current mapping: Not provided in the input context.
- Notes relevant to upgrades/CI:
  - If you have tooling/tests that parse pytest output, be aware pytest changes output truncation behavior when `CI` or `BUILD_NUMBER` is set (CI shows full lines in short test summary info).

## API Reference

Brief reference of the most important public APIs:

- **pytest** (module) - Main entry point module; provides test helpers and decorators.
- **@pytest.fixture** - Declare a fixture function; pytest injects fixture values by matching parameter names.
- **pytest.fail(reason: str = "", pytrace: bool = True)** - Immediately fail the current test with a message (optionally controlling traceback display).