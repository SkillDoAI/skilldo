---
name: fastapi
description: FastAPI is a modern, high-performance Python web framework for building APIs with automatic validation, documentation, and async support.
version: 0.135.1
ecosystem: python
license: MIT
generated_with: gpt-4.1
---

## Imports

```python
import fastapi
from fastapi import FastAPI, APIRouter, Request, Response, HTTPException, status
from fastapi import Depends, Security, Body, Query, Path, Header, Cookie, Form, File, UploadFile, BackgroundTasks, WebSocketException
from fastapi.responses import JSONResponse
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm, HTTPBearer, HTTPBasic, HTTPDigest
from fastapi.security import APIKeyHeader, APIKeyQuery, APIKeyCookie, OpenIdConnect
```

## Core Patterns

### Creating a FastAPI application ✅ Current
```python
from fastapi import FastAPI

app = FastAPI(title="My API", version="1.0.0", debug=True)
```
* Create your main application instance with `FastAPI`. You can set metadata like `title`, `version`, and `debug`.

### Dependency Injection with Depends ✅ Current
```python
from fastapi import FastAPI, Depends

app = FastAPI()

def common_parameters(q: str | None = None):
    return {"q": q}

@app.get("/users/")
async def read_users(commons: dict = Depends(common_parameters)):
    return commons

# Optional local run:
# uvicorn this_module:app --reload
```
* Use `Depends` to inject dependencies or shared logic into endpoints.
* Dependencies can be functions or classes.

---

### Using Pydantic models for request bodies ✅ Current
```python
from fastapi import FastAPI
from pydantic import BaseModel

class Item(BaseModel):
    name: str
    description: str | None = None

app = FastAPI()

@app.post("/items/")
async def create_item(item: Item):
    return item
```
* Use Pydantic models (subclassing `BaseModel`) to define and validate complex request bodies.

---

### Reading a cookie parameter with Cookie and Annotated ✅ Current
```python
from typing import Annotated
from fastapi import FastAPI, Cookie

app = FastAPI()

@app.get("/get-cookie/")
async def get_cookie(my_cookie: Annotated[str, Cookie()]):
    return {"my_cookie": my_cookie}
```
* Use `Cookie()` in an `Annotated` type hint to declare a cookie parameter.
* Optional cookies can use `str | None` and a default value.

---

### File uploads with File and UploadFile ✅ Current
```python
from fastapi import FastAPI, File, UploadFile

app = FastAPI()

@app.post("/upload/")
async def upload_file(file: UploadFile = File(...)):
    content = await file.read()
    return {"filename": file.filename, "size": len(content)}
```
* Use `File()` and `UploadFile` for handling file uploads in endpoints.

---

### Handling form data with Form ✅ Current
```python
from fastapi import FastAPI, Form

app = FastAPI()

@app.post("/login/")
async def login(username: str = Form(...), password: str = Form(...)):
    return {"username": username}
```
* Use `Form()` to receive form-encoded data in POST requests.

---

### Raising HTTP errors with HTTPException ✅ Current
```python
from fastapi import FastAPI, HTTPException, status

app = FastAPI()

@app.get("/resource/{id}")
async def get_resource(id: int):
    if id != 42:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Resource not found")
    return {"id": id}

# Optional local run:
# uvicorn this_module:app --reload
```
* Use `HTTPException` to return error responses with custom status codes and details.

---

### Securing endpoints with OAuth2PasswordBearer ✅ Current
```python
from fastapi import FastAPI, Depends
from fastapi.security import OAuth2PasswordBearer

app = FastAPI()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

@app.get("/users/me")
async def read_users_me(token: str = Depends(oauth2_scheme)):
    return {"token": token}
```
* Use security utilities like `OAuth2PasswordBearer` for authentication and authorization schemes.

---

## Configuration

- **FastAPI instance options**: Set `title`, `version`, `description`, `debug`, etc. when creating `FastAPI()`.
- **Router configuration**: Use `APIRouter` for grouping endpoints, with `prefix`, `tags`, and `dependencies` options.
- **Default response class**: Set with `default_response_class` in `FastAPI` or `APIRouter`.
- **Environment variables**: Not required for core FastAPI itself, but may be used by dependencies or for configuration management.
- **Startup and shutdown events**: Register with `on_startup` and `on_shutdown` parameters or use the `@app.on_event` decorator.
- **⚠️ Deprecated constructor parameter**: `FastAPI(routes=...)` is a soft-deprecated compatibility parameter inherited from Starlette. Prefer defining routes with FastAPI decorators (`@app.get`, `@app.post`, etc.).

## Pitfalls

### Wrong: Missing async in endpoint using await
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/data")
def get_data():
    data = await some_async_call()
    return data
```
*Fails: `await` can only be used inside async functions.*

### Right: Use async def for endpoints using await
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/data")
async def get_data():
    data = await some_async_call()
    return data
```
*Always declare endpoints as `async def` if you use `await`.*

---

### Wrong: Missing type hints for parameters
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/items/{item_id}")
def read_item(item_id, q=None):
    return {"item_id": item_id, "q": q}
```
*Not recommended: FastAPI can run, but type validation and OpenAPI documentation precision are reduced without explicit type hints.*

### Right: Provide type hints for all parameters
```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/items/{item_id}")
def read_item(item_id: int, q: str | None = None):
    return {"item_id": item_id, "q": q}
```
*Type hints are strongly recommended for better validation and more precise OpenAPI documentation.*

---

### Wrong: Using mutable default in Pydantic model
```python
from pydantic import BaseModel

class Item(BaseModel):
    tags: list = []
```
*Not recommended: Mutable defaults can be error-prone and are better expressed with `default_factory` for clarity and safety.*

### Right: Use default_factory for mutable defaults
```python
from pydantic import BaseModel, Field

class Item(BaseModel):
    tags: list = Field(default_factory=list)
```
*Always use `default_factory` for lists, dicts, or other mutable types.*

---

## References

- [Homepage](https://github.com/fastapi/fastapi)
- [Documentation](https://fastapi.tiangolo.com/)
- [Repository](https://github.com/fastapi/fastapi)
- [Issues](https://github.com/fastapi/fastapi/issues)
- [Changelog](https://fastapi.tiangolo.com/release-notes/)

## Migration from v0.100.0

- **CLI workflow update:** FastAPI CLI is now a recommended workflow for app startup.
- **Migration:**  
  - For development:  
    **Before:**  
    ```shell
    uvicorn main:app --reload
    ```
    **After:**  
    ```shell
    fastapi dev main.py
    ```
  - For production:  
    **Before:**  
    ```shell
    uvicorn main:app
    ```
    **After:**  
    ```shell
    fastapi run main.py
    ```
- **⚠️ Soft deprecation:** `FastAPI(routes=...)` is deprecated as a preferred style.  
  - **Before:** passing pre-built route lists via constructor.
  - **After:** define path operations with `@app.get(...)`, `@app.post(...)`, etc.
- Most endpoint and model code remains compatible. Review [Changelog](https://fastapi.tiangolo.com/release-notes/) for full migration details.

## API Reference

- **fastapi.__version__** – Installed FastAPI version string (for this release: `0.135.1`).
- **FastAPI(\*\*, title, summary, description, version, debug, ...)** – Create the main application instance. Key params: `title`, `summary`, `version`, `description`, `debug`.
- **⚠️ FastAPI(\*\*, routes=...)** – Soft-deprecated compatibility parameter; prefer decorator-based route definitions.
- **APIRouter(\*\*, prefix, tags, dependencies, ...)** – Create groups of routes with shared config.
- **@app.get(path), @app.post(path), ...** – Decorators to define HTTP endpoints.
- **Depends(dependency=None, \*, use_cache=True, scope=None)** – Declare dependencies for injection in endpoints.
- **Security(dependency=None, \*, scopes=None, use_cache=True)** – Declare security dependencies with optional scopes.
- **Query(default, \*\*, ...)** – Declare and validate query parameters.
- **Path(default=..., \*\*, ...)** – Declare and validate path parameters (required by default).
- **Header(default, \*\*, ...)** – Declare and validate HTTP headers.
- **Cookie(default, \*\*, alias, validation_alias, ...)** – Declare, alias, and validate cookie parameters.
- **Body(default, \*\*, ...)** – Declare and validate request bodies.
- **Form(default, \*\*, alias, validation_alias, ...)** – Receive and validate form data.
- **File(default, \*\*, ...)** – Receive file uploads.
- **UploadFile(filename, content_type, ...)** – Handle uploaded files efficiently.
- **HTTPException(status_code, detail, headers=None)** – Raise HTTP error responses.
- **WebSocketException(...)** – Raise structured WebSocket errors.
- **Request(scope, ...)** – Access the incoming HTTP request object.
- **Response(content, status_code=200, ...)** – Custom response handling.
- **status** – HTTP status codes (re-exported from Starlette).
- **BackgroundTasks(...) / add_task(func, \*args, \*\*kwargs)** – Schedule background work after returning a response.
- **OAuth2PasswordBearer(tokenUrl, ...)** – OAuth2 password flow security utility.
- **OAuth2PasswordRequestForm / OAuth2PasswordRequestFormStrict** – Form models for OAuth2 login handling.
- **HTTPBearer, HTTPBasic, HTTPDigest** – HTTP auth security utilities.
- **APIKeyHeader, APIKeyQuery, APIKeyCookie** – API key-based security helpers.
- **OpenIdConnect(...)** – OpenID Connect security scheme helper.