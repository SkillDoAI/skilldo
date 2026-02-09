---
name: celery
description: Distributed task queue for running Python callables asynchronously via message brokers.
version: 5.6.2
ecosystem: python
license: BSD-3-Clause'
---

## Imports

```python
from celery import Celery
from celery import shared_task
from celery.result import AsyncResult
```

## Core Patterns

### Define a Celery app + task with `@app.task` ✅ Current
```python
from __future__ import annotations

from celery import Celery

# Create a single app instance, importable by workers.
app = Celery(
    "hello",
    broker="amqp://guest@localhost//",
)

@app.task
def hello() -> str:
    return "hello world"
```
* Defines a single `celery.Celery` application and registers a task using `Celery.task`.
* **Status**: Current, stable

### Enqueue a task and fetch the result ✅ Current
```python
from __future__ import annotations

from celery import Celery
from celery.result import AsyncResult

# For a self-contained example (no external broker/backend needed),
# run tasks eagerly and store results in-memory so AsyncResult.get() works.
app = Celery("hello")
app.conf.update(
    task_always_eager=True,
    task_store_eager_result=True,
    result_backend="cache+memory://",
)

@app.task
def add(x: int, y: int) -> int:
    return x + y

def main() -> None:
    # Send by calling the task's delay() shortcut.
    async_result = add.delay(2, 3)

    # You can store/transport the id, then re-hydrate later:
    same_result: AsyncResult = AsyncResult(async_result.id, app=app)

    # Block until ready (works here because results are stored eagerly).
    value: int = same_result.get(timeout=10)
    print(value)

if __name__ == "__main__":
    main()
```
* Queues work with `delay()` and retrieves results via `celery.result.AsyncResult`.
* **Status**: Current, stable

### Configure via `app.conf` (module-friendly) ✅ Current
```python
from __future__ import annotations

from celery import Celery

app = Celery("proj", broker="redis://localhost:6379/0")

# Keep configuration close to the app so workers import it consistently.
app.conf.update(
    task_serializer="json",
    accept_content=["json"],
    result_serializer="json",
    timezone="UTC",
    enable_utc=True,
    # result_backend requires installing the correct extra (e.g., celery[redis])
    result_backend="redis://localhost:6379/1",
)

@app.task
def ping() -> str:
    return "pong"
```
* Centralizes Celery configuration in code (common for small projects/services).
* **Status**: Current, stable

### Autodiscover tasks from modules/packages ✅ Current
```python
from __future__ import annotations

from celery import Celery

app = Celery("proj", broker="amqp://guest@localhost//")

# Common layout: proj/celery_app.py defines app, tasks live in proj/tasks.py, etc.
# autodiscover_tasks expects packages where it can import "<pkg>.tasks" by default.
app.autodiscover_tasks(["proj"])

# Example task module would be: proj/tasks.py containing @shared_task or @app.task tasks.
```
* Helps workers find tasks without importing each task module manually.
* **Status**: Current, stable

### Define reusable tasks with `@shared_task` ✅ Current
```python
from __future__ import annotations

from celery import Celery, shared_task

# Library/app code can declare tasks without needing the app instance at import time.
@shared_task
def mul(x: int, y: int) -> int:
    return x * y

# In your main app module:
app = Celery("proj", broker="amqp://guest@localhost//")
app.autodiscover_tasks(["proj"])
```
* Useful when tasks live in reusable Django/apps or libraries and should bind to the “current app”.
* **Status**: Current, stable

## Configuration

- **Create one app instance** per process/package and keep it importable (e.g., `proj/celery_app.py`).
- **Broker URL**: pass explicitly at instantiation time (`Celery(..., broker="amqp://...")` or `broker="redis://..."`).
- **Result backend**: configure when you need `AsyncResult.get()`:
  - `app.conf.result_backend = "redis://..."` (requires installing `celery[redis]`).
- **Common `app.conf` keys**:
  - `timezone`, `enable_utc`
  - `task_serializer`, `result_serializer`, `accept_content`
- **Environment variables** (common in deployments):
  - `CELERY_BROKER_URL` and `CELERY_RESULT_BACKEND` are widely used conventions; you can map them into `Celery(..., broker=...)` / `app.conf.result_backend` in your app code.
- **Dependency extras**: install transport/backend dependencies explicitly, e.g.:
  - `pip install "celery[redis]"` for Redis broker/backend
  - `pip install "celery[gevent]"` for gevent pool support (if you use it)

## Pitfalls

### Wrong: Defining a function without registering it as a task
```python
from celery import Celery

app = Celery("hello", broker="amqp://guest@localhost//")

def hello() -> str:
    return "hello world"
```

### Right: Register the task with `@app.task`
```python
from celery import Celery

app = Celery("hello", broker="amqp://guest@localhost//")

@app.task
def hello() -> str:
    return "hello world"
```

### Wrong: Installing `celery` without extras, then configuring Redis
```python
# requirements.txt
celery

# ...later in code:
# broker="redis://localhost:6379/0"
# result_backend="redis://localhost:6379/1"
```

### Right: Install the correct extra for your broker/backend
```python
# requirements.txt
celery[redis]
```

### Wrong: Calling `.get()` without a result backend configured
```python
from celery import Celery

app = Celery("proj", broker="amqp://guest@localhost//")

@app.task
def add(x: int, y: int) -> int:
    return x + y

result = add.delay(1, 2)
# This will fail/hang without a configured result backend:
value = result.get(timeout=10)
print(value)
```

### Right: Configure a result backend (and install its dependencies)
```python
from celery import Celery

app = Celery("proj", broker="redis://localhost:6379/0")
app.conf.result_backend = "redis://localhost:6379/1"

@app.task
def add(x: int, y: int) -> int:
    return x + y

result = add.delay(1, 2)
value = result.get(timeout=10)
print(value)
```

### Wrong: Expecting official Windows support in production deployments
```python
# Deploying Celery workers on Windows and expecting official support/issue triage.
```

### Right: Run workers on supported Unix-like platforms (best-effort on Windows)
```python
# Prefer Linux/macOS for production workers.
# If you must use Windows, treat it as best-effort and validate thoroughly.
```

### Wrong: Python/Celery version mismatch (Python 3.8 with Celery 5.6.x)
```bash
pip install -U celery==5.6.2
```

### Right: Pin Celery for older Python, or upgrade Python
```bash
# Python 3.8 users should pin Celery 5.5 or earlier:
pip install "celery<5.6"

# Or upgrade Python to >=3.9 to use Celery 5.6.x:
pip install "celery==5.6.2"
```

## References

- [Documentation](https://docs.celeryq.dev/en/stable/")
- [Changelog](https://docs.celeryq.dev/en/stable/changelog.html")
- [Code](https://github.com/celery/celery")
- [Tracker](https://github.com/celery/celery/issues")
- [Funding](https://opencollective.com/celery)

## Migration from v5.5.x

What changed in this version (if applicable):
- **Breaking changes**:
  - Celery **5.6.x requires Python >= 3.9**. Python 3.8 must stay on Celery 5.5 or earlier.
- **Forward-looking compatibility note**:
  - Celery **5.6.x is the last series supporting Python 3.9**. Celery 5.7.x removes Python 3.9 support; upgrade to Python 3.10+ before upgrading Celery.

Before/after (Python version pinning):
```bash
# Before (Python 3.8 runtime)
pip install "celery<5.6"

# After (Python >=3.9 runtime)
pip install "celery==5.6.2"
```

## API Reference

- **Celery(main, broker=..., backend=..., include=...)**
  - Create the application instance used by producers and workers.
- **Celery.task(*dargs, **dkwargs)** (`@app.task`)
  - Decorator to register a Python callable as a task bound to this app.
- **shared_task(*dargs, **dkwargs)** (`@shared_task`)
  - Decorator to define a task that binds to the current Celery app (common for reusable modules).
- **Celery.conf.update(**settings)**
  - Update configuration keys (serializers, timezone, result backend, etc.).
- **Celery.autodiscover_tasks(packages, related_name="tasks")**
  - Import task modules automatically from listed packages.
- **celery.result.AsyncResult(task_id, app=...)**
  - Handle to a task execution; can check state and fetch results.
- **AsyncResult.get(timeout=..., propagate=...)**
  - Wait for and return the task result (requires a result backend).
- **Task.delay(*args, **kwargs)**
  - Shortcut to enqueue a task asynchronously using default options.
- **Task.apply_async(args=None, kwargs=None, countdown=None, eta=None, expires=None, queue=None, retry=None, **options)**
  - Enqueue with scheduling/routing/options (use when you need more control than `delay()`).
- **Task.name**
  - Fully qualified task name used for routing and calling by name.