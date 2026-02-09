---
name: fastapi
description: A Python web framework for building ASGI APIs with type hints and automatic request validation.
version: 0.128.4
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
from fastapi import FastAPI, Depends, HTTPException, status
from pydantic import BaseModel
```

## Core Patterns

The right way to use the main APIs. Show 3-5 most common patterns.

**CRITICAL: Prioritize PUBLIC APIs over internal/compat modules**
- Use APIs from api_surface with `publicity_score: "high"` first
- Avoid `.compat`, `.internal`, `._private` modules unless they're the only option
- Example: Prefer `library.MainClass` over `library.compat.helper_function`

**CRITICAL: Mark deprecation status with clear indicators**

### Create an app + basic routes ✅ Current
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
def read_root() -> dict[str, str]:
    return {"Hello": "World"}

@app.get("/items/{item_id}")
def read_item(item_id: int) -> dict[str, int]:
    # Path parameters are parsed/validated from the URL
    return {"item_id": item_id}
```
* Create a single `FastAPI()` application instance and register path operations with decorators.
* **Status**: Current, stable

### Query parameters: required vs optional ✅ Current
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/items/{item_id}")
def read_item(item_id: int, q: str | None = None) -> dict[str, object]:
    # q is optional because it defaults to None
    return {"item_id": item_id, "q": q}

@app.get("/search/")
def search(q: str) -> dict[str, str]:
    # q is required because it has no default
    return {"q": q}
```
* Use type hints and defaults to control validation and required/optional behavior.
* **Status**: Current, stable

### Request body with Pydantic models ✅ Current
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
    # FastAPI parses JSON into the Pydantic model and validates types
    return {"item_id": item_id, "item": item.model_dump()}
```
* Model JSON request bodies with `pydantic.BaseModel` subclasses.
* **Status**: Current, stable

### Async endpoints when using async/await ✅ Current
```python
from fastapi import FastAPI

app = FastAPI()

async def some_async_call() -> str:
    return "ok"

@app.get("/async")
async def read_async() -> dict[str, str]:
    # If you call async code, your endpoint should be async and must await it
    data = await some_async_call()
    return {"data": data}
```
* Use `async def` for I/O-bound endpoints that await coroutines.
* **Status**: Current, stable

### Dependency injection with Depends ✅ Current
```python
from fastapi import Depends, FastAPI, HTTPException, status

app = FastAPI()

def get_current_user(token: str | None = None) -> dict[str, str]:
    # In real apps, parse/verify token (e.g., from Authorization header)
    if token != "secret":
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Unauthorized")
    return {"username": "alice"}

@app.get("/protected")
def protected(user: dict[str, str] = Depends(get_current_user)) -> dict[str, object]:
    return {"user": user}
```
* Use `Depends(...)` to inject shared logic (auth, DB sessions, etc.) into endpoints.
* **Status**: Current, stable

## Routing Patterns

```python
from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()

class Item(BaseModel):
    name: str
    price: float

@app.get("/items/{item_id}")
def read_item(item_id: int) -> dict[str, int]:
    return {"item_id": item_id}

@app.post("/items/")
def create_item(item: Item) -> dict[str, object]:
    return {"created": item.model_dump()}

@app.put("/items/{item_id}")
def replace_item(item_id: int, item: Item) -> dict[str, object]:
    return {"item_id": item_id, "item": item.model_dump()}
```

## Request Handling

```python
from fastapi import FastAPI
from pydantic import BaseModel

app = FastAPI()

class Item(BaseModel):
    name: str
    price: float

@app.get("/items/{item_id}")
def read_item(item_id: int, q: str | None = None) -> dict[str, object]:
    # item_id: path param, q: query param
    return {"item_id": item_id, "q": q}

@app.post("/items/")
def create_item(item: Item) -> dict[str, object]:
    # item: request body parsed as JSON and validated
    return {"item": item.model_dump()}
```

## Response Handling

```python
from fastapi import FastAPI, HTTPException, status

app = FastAPI()

@app.get("/ok")
def ok() -> dict[str, str]:
    return {"status": "ok"}

@app.get("/not-found")
def not_found() -> None:
    # Raise HTTP errors with status codes
    raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Not found")
```

## Middleware/Dependencies

```python
from fastapi import Depends, FastAPI

app = FastAPI()

def common_params(q: str | None = None) -> dict[str, str | None]:
    return {"q": q}

@app.get("/items/")
def list_items(params: dict[str, str | None] = Depends(common_params)) -> dict[str, object]:
    return {"params": params}
```

## Error Handling

```python
from fastapi import FastAPI, HTTPException, status

app = FastAPI()

@app.get("/divide")
def divide(a: int, b: int) -> dict[str, float]:
    if b == 0:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail="b must not be zero",
        )
    return {"result": a / b}
```

## Background Tasks

FastAPI supports background tasks via `BackgroundTasks`, but it is not included in the provided documented API list. Prefer consulting the official docs before using it in agent-generated code for this ruleset.

## WebSocket Patterns

FastAPI supports WebSockets, but WebSocket APIs are not included in the provided documented API list. Prefer consulting the official docs before using it in agent-generated code for this ruleset.

## Configuration

- **App creation**: create one instance, typically:
  - `app = FastAPI()`
- **Run commands (CLI)**:
  - Development (auto-reload by default): `fastapi dev main.py`
  - Production: `fastapi run main.py`
- **Install extras (portable quoting)**:
  - `pip install "fastapi[standard]"`
- **Type-hint driven validation**:
  - Path/query/body validation is derived from function signatures and Pydantic models.

## Pitfalls

### Wrong: Calling async code from a sync endpoint
```python
from fastapi import FastAPI

app = FastAPI()

async def some_async_call() -> str:
    return "ok"

@app.get("/")
def read_root() -> dict[str, object]:
    data = some_async_call()  # forgot await; also endpoint isn't async
    return {"data": data}
```

### Right: Use `async def` and `await`
```python
from fastapi import FastAPI

app = FastAPI()

async def some_async_call() -> str:
    return "ok"

@app.get("/")
async def read_root() -> dict[str, str]:
    data = await some_async_call()
    return {"data": data}
```

### Wrong: Making a query parameter required by accident
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/items/{item_id}")
def read_item(item_id: int, q: str) -> dict[str, object]:
    # q is required because it has no default
    return {"item_id": item_id, "q": q}
```

### Right: Mark it optional with `| None` and a default `None`
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/items/{item_id}")
def read_item(item_id: int, q: str | None = None) -> dict[str, object]:
    return {"item_id": item_id, "q": q}
```

### Wrong: Using `dict` for request bodies (loses validation/schema)
```python
from fastapi import FastAPI

app = FastAPI()

@app.put("/items/{item_id}")
def update_item(item_id: int, item: dict) -> dict[str, object]:
    return {"item_name": item["name"], "item_id": item_id}
```

### Right: Use a Pydantic `BaseModel`
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
    return {"item_name": item.name, "item_id": item_id}
```

## References

- [Homepage](https://github.com/fastapi/fastapi)
- [Documentation](https://fastapi.tiangolo.com/)
- [Repository](https://github.com/fastapi/fastapi)
- [Issues](https://github.com/fastapi/fastapi/issues)
- [Changelog](https://fastapi.tiangolo.com/release-notes/)

## Migration from v[previous]

What changed in this version (if applicable):
- Breaking changes: Not provided in the supplied context.
- Deprecated → Current mapping: Not provided in the supplied context.
- Notes from provided context:
  - Prefer `fastapi dev main.py` for development (auto-reload).
  - Prefer `fastapi run main.py` for production.
  - If upgrading from invoking Uvicorn directly, consider migrating to these CLI commands.

## API Reference

Brief reference of the most important public APIs:

- **fastapi.FastAPI() / FastAPI()** — Create the ASGI application.
  - Common use: `app = FastAPI()`
- **FastAPI.get(path, ...)** — Decorator to register a GET path operation.
  - Use: `@app.get("/items/{item_id}")`
- **FastAPI.put(path, ...)** — Decorator to register a PUT path operation.
  - Use: `@app.put("/items/{item_id}")`
- **pydantic.BaseModel / BaseModel** — Define request/response body schemas with validation.
  - Use: `class Item(BaseModel): ...`
- **fastapi dev** — CLI command for local development (auto-reload by default).
  - Use: `fastapi dev main.py`
- **fastapi run** — CLI command for production runs.
  - Use: `fastapi run main.py`