---

name: fastapi
description: ASGI web framework for building HTTP APIs with Python type hints and automatic OpenAPI generation.
version: 0.128.4
ecosystem: python
license: MIT
generated_with: gpt-5.2
---

## Imports

```python
import fastapi
from fastapi import FastAPI
from pydantic import BaseModel
```

## Core Patterns

### Create an app instance and define routes ✅ Current
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
def read_root() -> dict[str, str]:
    return {"status": "ok"}

@app.get("/items/{item_id}")
def read_item(item_id: int, q: str | None = None) -> dict[str, object]:
    return {"item_id": item_id, "q": q}
```
* Create a single `app = FastAPI()` at module scope, then register routes with `@app.get(...)`, etc.
* Use type hints for path/query parsing and OpenAPI schema generation.

### Request bodies with Pydantic models ✅ Current
```python
from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()

class Item(BaseModel):
    name: str
    price: float
    is_offer: bool | None = None

@app.put("/items/{item_id}")
def update_item(item_id: int, item: Item) -> dict[str, object]:
    return {"item_id": item_id, "item": item.model_dump()}
```
* Model JSON request bodies with `pydantic.BaseModel` to get validation and schema.
* Pass the model as a typed endpoint parameter to make it a body parameter.

### Async endpoints for awaited I/O ✅ Current
```python
from fastapi import FastAPI
import asyncio

app = FastAPI()

async def fetch_value() -> str:
    await asyncio.sleep(0.01)
    return "value"

@app.get("/async")
async def read_async() -> dict[str, str]:
    value = await fetch_value()
    return {"value": value}
```
* Use `async def` when you need `await` inside the endpoint.
* Keep endpoints as `def` when you don’t need async I/O.

### Development and production CLI ✅ Current
```python
"""
Run locally (auto-reload):
    fastapi dev main.py

Run for production:
    fastapi run main.py

This file should be named main.py and expose `app`.
"""
from fastapi import FastAPI

app = FastAPI()

@app.get("/health")
def health() -> dict[str, str]:
    return {"status": "healthy"}
```
* `fastapi dev main.py` is the recommended development workflow (auto-reload by default).
* `fastapi run main.py` is the recommended production entrypoint.

## Configuration

- **Application instance**: create exactly one `FastAPI()` instance at module scope (e.g., `main.py`) so CLI runners can import it.
- **Parameter required vs optional**:
  - Required query/path parameters: omit a default (e.g., `q: str`)
  - Optional query parameters: use `| None` and a default (e.g., `q: str | None = None`)
- **Sync vs async**:
  - Use `def` for purely synchronous work.
  - Use `async def` if you need to `await` (e.g., async DB/HTTP clients).
- **CLI**:
  - Development: `fastapi dev main.py`
  - Production: `fastapi run main.py`

## Pitfalls

### Wrong: using `await` inside a sync endpoint
```python
from fastapi import FastAPI
import asyncio

app = FastAPI()

async def some_async_call() -> str:
    await asyncio.sleep(0.01)
    return "done"

@app.get("/")
def read_root():
    data = await some_async_call()  # SyntaxError / invalid usage
    return {"data": data}
```

### Right: make the endpoint `async def` when awaiting
```python
from fastapi import FastAPI
import asyncio

app = FastAPI()

async def some_async_call() -> str:
    await asyncio.sleep(0.01)
    return "done"

@app.get("/")
async def read_root() -> dict[str, str]:
    data = await some_async_call()
    return {"data": data}
```

### Wrong: missing type hints prevents intended parsing/validation
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/items/{item_id}")
def read_item(item_id, q=None):
    return {"item_id": item_id, "q": q}
```

### Right: add type hints for path/query parameters
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/items/{item_id}")
def read_item(item_id: int, q: str | None = None) -> dict[str, object]:
    return {"item_id": item_id, "q": q}
```

### Wrong: using an untyped `dict` for request bodies
```python
from fastapi import FastAPI

app = FastAPI()

@app.put("/items/{item_id}")
def update_item(item_id: int, item: dict):
    return {"item_id": item_id, "item_name": item.get("name")}
```

### Right: use `pydantic.BaseModel` for request bodies
```python
from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()

class Item(BaseModel):
    name: str
    price: float
    is_offer: bool | None = None

@app.put("/items/{item_id}")
def update_item(item_id: int, item: Item) -> dict[str, object]:
    return {"item_id": item_id, "item_name": item.name, "price": item.price}
```

### Wrong: marking optional query params as required by omitting defaults
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/search")
def search(q: str) -> dict[str, str]:
    # q is required here; requests without ?q=... will fail validation
    return {"q": q}
```

### Right: make query params optional with `| None` and a default
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/search")
def search(q: str | None = None) -> dict[str, str | None]:
    return {"q": q}
```

## References

- [Homepage](https://github.com/fastapi/fastapi)
- [Documentation](https://fastapi.tiangolo.com/)
- [Repository](https://github.com/fastapi/fastapi)
- [Issues](https://github.com/fastapi/fastapi/issues)
- [Changelog](https://fastapi.tiangolo.com/release-notes/)

## Migration from v[previous]

No breaking changes or deprecations were provided in the supplied context for FastAPI v0.128.4. If migrating between versions, validate:
- CLI usage expectations (`fastapi dev` for development, `fastapi run` for production)
- Endpoint signatures keep correct type hints and optional defaults
- Async endpoints are declared with `async def` when using `await`

## API Reference

- **fastapi.FastAPI()** - Create an application instance; used to register routes and serve ASGI.
- **FastAPI.get(path)** - Decorator to register an HTTP GET endpoint for `path`.
- **FastAPI.put(path)** - Decorator to register an HTTP PUT endpoint for `path`.
- **pydantic.BaseModel** - Base class for request/response models; provides validation and schema.
- **fastapi (CLI command)** - Command-line entrypoint for running FastAPI apps.
- **fastapi dev** - CLI subcommand for local development (auto-reload by default).
- **fastapi run** - CLI subcommand for production execution.