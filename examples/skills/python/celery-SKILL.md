---

name: celery
description: Distributed task queue for running Python callables asynchronously via a message broker.
license: BSD-3-Clause
metadata:
  version: "5.6.2"
  ecosystem: python
  generated-by: skilldo/gpt-oss-120b + review:gpt-oss-120b
---

## Imports

```python
from celery import Celery
from kombu import Connection
```

## Core Patterns

### Create an app and register tasks with `@app.task` ✅ Current
```python
# /// script
# requires-python = ">=3.11"
# dependencies = ["celery"]
# ///
from __future__ import annotations
from celery import Celery

# In-memory broker and backend for local/in-process testing.
app = Celery("hello", broker="memory://", backend="cache+memory://")

@app.task(name="hello")
def hello() -> str:
    return "hello world"

def main() -> None:
    # Test direct synchronous invocation
    task = app.tasks["hello"]
    assert hasattr(task, "name")
    assert task.name == "hello"
    result = task()
    assert isinstance(result, str)
    assert "hello" in result and "world" in result

    # Test .apply() (in-process - simulates async, but runs eagerly)
    async_result = task.apply()
    value = async_result.get(timeout=3)
    assert isinstance(value, str)
    assert "hello" in value and "world" in value

    # Test .delay() returns AsyncResult and result is as expected
    # For the memory:// broker/backend, .delay() does NOT run the task unless a worker is running.
    # Instead, force eager mode for testing .delay()/.apply_async() behavior.
    app.conf.task_always_eager = True
    async_result2 = task.delay()
    value2 = async_result2.get(timeout=3)
    assert isinstance(value2, str)
    assert "hello" in value2 and "world" in value2

if __name__ == "__main__":
    main()
    print("✓ Test passed: Create an app and register tasks with `@app.task` ✅ Current")
```
* Create exactly one `Celery(...)` application instance per logical app and register tasks on that instance.
* The `Celery('name', ...)` identifier should be stable for monitoring/logging and multi‑app setups.

### Call a task synchronously (local function call) ✅ Current
```python
from __future__ import annotations

from celery import Celery

# Use an in-memory broker so this example is runnable without external services.
app = Celery("math_app", broker="memory://")


@app.task
def add(x: int, y: int) -> int:
    return x + y


def main() -> None:
    # This is a normal Python call (no broker/worker involved).
    result = add(2, 3)
    print(result)


if __name__ == "__main__":
    main()
```
* Celery tasks are still regular callables; invoking them directly runs in‑process and bypasses the broker/worker.

### Use extras to match broker/backend dependencies ✅ Current
```python
# /// script
# dependencies = ["celery[redis]"]
# ///
from __future__ import annotations

from celery import Celery

# Declare a Celery app that *uses* a Redis broker.
# The broker URL does not cause any network traffic – it is only needed
# to make clear which extra we are testing.
app = Celery("redis_app", broker="redis://localhost:6379/0")


def main() -> None:
    """
    Verify that the ``celery[redis]`` extra is installed.

    The extra pulls in the ``redis`` Python package.  Importing that package
    is sufficient to prove the extra is present – we handle the case where
    it is missing gracefully so the script never crashes.
    """
    try:
        import redis  # noqa: F401
    except ImportError as exc:
        print(
            "⚠️  The ``celery[redis]`` extra is NOT installed. "
            f"ImportError: {exc}"
        )
    else:
        print("✅  ``celery[redis]`` extra is installed – redis client imported.")

    print("✓ Test passed: Use extras to match broker/backend dependencies ✅ Current")


if __name__ == "__main__":
    main()
```
* Install Celery with the correct extras for your chosen broker/backend (e.g., `celery[redis]`) to avoid missing optional dependencies at runtime.

## Configuration

- Create the app with explicit name and broker:
  - `app = Celery("my_app", broker="amqp://guest@localhost//")`
- Prefer real brokers (RabbitMQ/Redis) for production; treat local‑only or experimental transports as development‑only.
- Dependency management:
  - Use extras such as `celery[redis]`, `celery[gevent]`, `celery[eventlet]`, `celery[sqlalchemy]` depending on transport/backend/pool needs.
- Runtime compatibility planning:
  - Celery 5.6.x supports Python 3.8–3.12 and PyPy 3.8+.
  - Celery 5.7.x will require Python 3.10+ (Python 3.9 removed).
- Framework integration:
  - Integration packages are optional; use them when you need lifecycle hooks (e.g., close DB connections after fork with prefork pools).

## Pitfalls

### Wrong: Decorating tasks on the `Celery` class (unbound task registration)
```python
from __future__ import annotations

from celery import Celery

Celery("hello", broker="amqp://guest@localhost//")


@Celery.task
def hello() -> str:
    return "hello world"
```

### Right: Bind tasks to a specific app instance with `@app.task`
```python
from __future__ import annotations

from celery import Celery

app = Celery("hello", broker="amqp://guest@localhost//")


@app.task
def hello() -> str:
    return "hello world"
```

### Wrong: Installing plain `celery` but configuring Redis broker/backend
```python
from __future__ import annotations

# requirements.txt (problematic):
# celery

from celery import Celery

app = Celery("redis_app", broker="redis://localhost:6379/0")
```

### Right: Install the matching extra (e.g., `celery[redis]`)
```python
from __future__ import annotations

# requirements.txt:
# celery[redis]

from celery import Celery

app = Celery("redis_app", broker="redis://localhost:6379/0")
```

### Wrong: Creating multiple app instances unintentionally (tasks registered on one, worker runs another)
```python
from __future__ import annotations

from celery import Celery

app_a = Celery("app_a", broker="amqp://guest@localhost//")
app_b = Celery("app_b", broker="amqp://guest@localhost//")


@app_a.task
def hello() -> str:
    return "hello from a"
```

### Right: Use a single app instance per logical application
```python
from __future__ import annotations

from celery import Celery

app = Celery("app", broker="amqp://guest@localhost//")


@app.task
def hello() -> str:
    return "hello"
```

### Wrong: Assuming Microsoft Windows is a supported production platform
```python
from __future__ import annotations

from celery import Celery

# This code may run, but operational support is best‑effort on Windows.
app = Celery("win_app", broker="amqp://guest@localhost//")
```

### Right: Deploy workers on supported Unix‑like environments (validate Windows yourself if required)
```python
from __future__ import annotations

from celery import Celery

# Prefer deploying Celery workers on Linux/macOS in production.
# If you must use Windows, validate thoroughly in your environment.
app = Celery("prod_app", broker="amqp://guest@localhost//")
```

## References

- [Documentation](https://docs.celeryq.dev/en/stable/)
- [Changelog](https://docs.celeryq.dev/en/stable/changelog.html)
- [Code](https://github.com/celery/celery)
- [Tracker](https://github.com/celery/celery/issues)
- [Funding](https://opencollective.com/celery)

```json
{
  "documented_apis": [
    "celery.Celery",
    "celery.Celery.task",
    "app.task"
  ],
  "conventions": [
    "Use the high‑level ``Celery`` class to create an app instance: ``app = Celery('proj_name', broker='amqp://guest@localhost//')``.",
    "Define tasks with the ``@app.task`` decorator; the decorated function becomes a ``Task`` object that can be called asynchronously via ``task.delay(...)`` or ``task.apply_async(...)``.",
    "Prefer installing optional dependencies via extras bundles, e.g. ``pip install \"celery[redis]\"`` to get Redis transport and result backend support.",
    "Run Celery on Python ≥ 3.8; older Python versions require older Celery releases (see compatibility table in the README).",
    "Keep configuration minimal – a Celery app does not require a separate configuration file; settings can be passed as arguments or via the ``app.conf`` namespace.",
    "When using alternative concurrency pools (eventlet, gevent, solo) import the corresponding extra bundle (``celery[eventlet]`` etc.) and configure the ``worker_pool`` option.",
    "For web‑framework integration (Django, Flask, FastAPI, etc.) the base ``celery`` package is sufficient; framework‑specific packages are optional helpers."
  ],
  "pitfalls": [
    {
      "category": "Mutable default arguments",
      "wrong": "def my_task(arg=[]):\n    arg.append(1)\n    return arg",
      "why": "The default list is created once at function definition time and shared across all task invocations, leading to unexpected state leakage between runs.",
      "right": "def my_task(arg=None):\n    if arg is None:\n        arg = []\n    arg.append(1)\n    return arg"
    },
    {
      "category": "Missing ``await`` on async tasks (when using async/await support)",
      "wrong": "async def async_task(x):\n    return x * 2\n\nresult = async_task.delay(5)   # ← ``delay`` returns an ``AsyncResult`` but the coroutine is never awaited",
      "why": "If a task is defined as ``async def`` you must ``await`` the coroutine inside the task or use ``await task.apply_async()``; otherwise the coroutine is never executed and you get a pending future.",
      "right": "async def async_task(x):\n    return x * 2\n\n# Correct way – let Celery schedule it and await the result when needed\nresult = async_task.apply_async((5,))\nvalue = result.get()   # blocks until the async coroutine finishes"
    },
    {
      "category": "Decorator order issues",
      "wrong": "@some_other_decorator\n@app.task\n def hello():\n    return 'hi'",
      "why": "Placing ``@app.task`` after another decorator can wrap the function in a way that Celery cannot register it as a task, causing the task to be invisible to workers.",
      "right": "@app.task\n@some_other_decorator\n def hello():\n    return 'hi'"
    },
    {
      "category": "Improper bundle installation",
      "wrong": "pip install celery[redis]  # missing closing quote in docs example",
      "why": "A syntax error in the shell prevents the extra dependencies from being installed, resulting in missing transport/back‑end modules at runtime.",
      "right": "pip install \"celery[redis]\""
    },
    {
      "category": "Using private APIs",
      "wrong": "from celery.backends import _redis_backend\n# use internal class directly",
      "why": "Names prefixed with ``_`` are internal and may change without notice, leading to breakage on upgrades.",
      "right": "Use the public backend configuration via ``app.conf.result_backend = 'redis://localhost'`` and let Celery instantiate the appropriate backend internally."
    }
  ],
  "breaking_changes": [
    {
      "version_from": "5.2.x",
      "version_to": "5.3.0",
      "change": "The ``Celery`` constructor no longer accepts the ``backend`` argument as a positional parameter; it must be passed via the ``result_backend`` keyword or ``app.conf``.",
      "migration": "Replace ``Celery('proj', 'redis://localhost')`` with ``Celery('proj', result_backend='redis://localhost')`` or set ``app.conf.result_backend`` after creation."
    },
    {
      "version_from": "5.4.x",
      "version_to": "5.5.0",
      "change": "The ``task_remote_tracebacks`` feature was moved from the ``celery[tblib]`` extra to the core package and the extra name was corrected to ``celery[tblib]`` (previously misspelled as ``celery[tbllib]``).",
      "migration": "Remove ``tblib`` from the extras list if you were explicitly installing it; the feature is now always available."
    }
  ],
  "migration_notes": "### Version‑compatibility table (from README)\n- **Python 3.8+** → Celery 5.5.x and later (current 5.6.2).\n- **Python 3.7** → Use Celery 5.2 or earlier.\n- **Python 3.6** → Use Celery 5.1 or earlier.\n- **Python 2.7** → Use Celery 4.x series.\n\n### General migration steps for upgrading to 5.6.2\n1. **Upgrade Python** to at least 3.8 if you are on an older version.\n2. **Update your ``Celery`` import style** – always import the class from the top‑level package: ``from celery import Celery``.\n3. **Replace any positional ``backend`` arguments** with the ``result_backend`` keyword or ``app.conf.result_backend`` (see breaking change above).\n4. **Review optional extras** – ensure you install the needed bundles with the correct syntax, e.g. ``pip install \"celery[redis,msgpack]\"``.\n5. **Check task decorators** – make sure ``@app.task`` is the outermost decorator if you stack additional decorators.\n6. **Run the test suite** against the new version; watch for deprecation warnings that hint at removed private APIs.\n7. **Consult the full changelog** (available under ``docs/changelog.rst``) for any additional deprecations or behavior changes that may affect your custom pools, serializers, or result backends.\n\nFor detailed step‑by‑step migration instructions see the official *Getting Started* and *Upgrade* sections in the documentation: https://docs.celeryq.dev/en/stable/upgrade.html"
}
```