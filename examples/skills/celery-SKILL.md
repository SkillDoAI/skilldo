---

name: celery
description: Distributed task queue for running Python callables asynchronously via a message broker.
version: 5.6.2
ecosystem: python
license: BSD-3-Clause'
generated_with: gpt-5.2
---

## Imports

```python
import celery
from celery import Celery
```

## Core Patterns

### Create an app and register tasks with `@app.task` ✅ Current
```python
from __future__ import annotations

from celery import Celery

app = Celery("hello", broker="amqp://guest@localhost//")


@app.task(name="hello")
def hello() -> str:
    return "hello world"


def main() -> None:
    # Celery registers a Task object in app.tasks, not the raw function.
    task = app.tasks["hello"]
    assert task.name == "hello"
    assert task() == "hello world"


if __name__ == "__main__":
    main()
```
* Create exactly one `Celery(...)` application instance per logical app and register tasks on that instance.
* The `Celery('name', ...)` identifier should be stable for monitoring/logging and multi-app setups.

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
* Celery tasks are still regular callables; invoking them directly runs in-process and bypasses the broker/worker.

### Use extras to match broker/backend dependencies ✅ Current
```python
from __future__ import annotations

# This file is runnable Python, but the key is the dependency pin:
# requirements.txt:
#   celery[redis]

from celery import Celery
from kombu import Connection

app = Celery("redis_app", broker="redis://localhost:6379/0")


def main() -> None:
    # Prove the redis transport is importable/resolvable (i.e., the extra is installed).
    conn = Connection(app.conf.broker_url)

    # kombu resolves the transport implementation on-demand; just accessing it
    # is enough to verify the optional dependency is present.
    transport = conn.transport
    module_path = transport.__class__.__module__

    assert module_path.startswith("kombu.transport.redis")


if __name__ == "__main__":
    main()
```
* Install Celery with the correct extras for your chosen broker/backend (e.g., `celery[redis]`) to avoid missing optional dependencies at runtime.

## Configuration

- Create the app with explicit name and broker:
  - `app = Celery("my_app", broker="amqp://guest@localhost//")`
- Prefer real brokers (RabbitMQ/Redis) for production; treat local-only or experimental transports as development-only.
- Dependency management:
  - Use extras such as `celery[redis]`, `celery[gevent]`, `celery[eventlet]`, `celery[sqlalchemy]` depending on transport/backend/pool needs.
- Runtime compatibility planning:
  - Celery 5.6.x supports Python 3.9–3.13 and PyPy 3.9+.
  - Celery 5.7.x will require Python 3.10+ (Python 3.9 removed).
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

# This code may run, but operational support is best-effort on Windows.
app = Celery("win_app", broker="amqp://guest@localhost//")
```

### Right: Deploy workers on supported Unix-like environments (validate Windows yourself if required)
```python
from __future__ import annotations

from celery import Celery

# Prefer deploying Celery workers on Linux/macOS in production.
# If you must use Windows, validate thoroughly in your environment.
app = Celery("prod_app", broker="amqp://guest@localhost//")
```

## References

- [Documentation](https://docs.celeryq.dev/en/stable/")
- [Changelog](https://docs.celeryq.dev/en/stable/changelog.html")
- [Code](https://github.com/celery/celery")
- [Tracker](https://github.com/celery/celery/issues")
- [Funding](https://opencollective.com/celery)

## Migration from v5.6.x

- Breaking change when upgrading to Celery 5.7.x:
  - **Change:** Python 3.9 support removed; Celery 5.7.x requires Python 3.10+.
  - **Migration guidance:** Upgrade runtime/CI to Python 3.10+ before upgrading Celery. If you must stay on Python 3.9, pin Celery to `~=5.6`.

## API Reference

- **celery.Celery(main, broker=...)** - Create a Celery application instance; key params: `main` (app name), `broker` (broker URL).
- **Celery.task(*dargs, **dkwargs)** - Decorator to register a function as a task on that app instance; used as `@app.task`.