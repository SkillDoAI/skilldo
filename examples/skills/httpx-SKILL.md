---
name: httpx
description: HTTP client for making synchronous and asynchronous HTTP requests.
version: 0.28.1
ecosystem: python
license: BSD-3-Clause
---

## Imports

```python
import httpx

from httpx import (
    ASGITransport,
    AsyncClient,
    BasicAuth,
    Client,
    DigestAuth,
    FunctionAuth,
    NetRCAuth,
    Request,
    Response,
    WSGITransport,
)
from httpx import InvalidURL
```

## Core Patterns

### One-off request (top-level helper) ✅ Current
```python
import httpx

def fetch_json(url: str) -> dict:
    # One-off request. Does not reuse connections across calls.
    response = httpx.get(url)
    response.raise_for_status()
    return response.json()

if __name__ == "__main__":
    data = fetch_json("https://httpbin.org/json")
    print(data.keys())
```
* Use `httpx.get()` (or `httpx.request()`) for quick scripts.
* **Status**: Current, stable

### Persistent session with connection pooling (sync) ✅ Current
```python
import httpx

def crawl(urls: list[str]) -> list[int]:
    # Prefer a Client for multiple requests (connection pooling, cookies, etc.).
    with httpx.Client() as client:
        statuses: list[int] = []
        for url in urls:
            r = client.get(url)
            statuses.append(r.status_code)
        return statuses

if __name__ == "__main__":
    print(crawl(["https://httpbin.org/status/200", "https://httpbin.org/status/204"]))
```
* Use `httpx.Client()` and `client.get()` / `client.request()` for repeated calls.
* **Status**: Current, stable

### Persistent session with connection pooling (async) ✅ Current
```python
import asyncio
import httpx

async def fetch_all(urls: list[str]) -> list[int]:
    async with httpx.AsyncClient() as client:
        statuses: list[int] = []
        for url in urls:
            r = await client.get(url)
            statuses.append(r.status_code)
        return statuses

if __name__ == "__main__":
    print(asyncio.run(fetch_all(["https://httpbin.org/status/200", "https://httpbin.org/status/204"])))
```
* Use `httpx.AsyncClient()` for async code; always `await` request methods.
* **Status**: Current, stable

### Custom authentication via `httpx.Auth` flows ✅ Current
```python
import httpx

class TokenAuth(httpx.Auth):
    # Keep auth_flow free of non-HTTP I/O so it can work for sync and async clients.
    def __init__(self, token: str) -> None:
        self._token = token

    def auth_flow(self, request: httpx.Request):
        request.headers["Authorization"] = f"Bearer {self._token}"
        yield request

def call_api(url: str, token: str) -> int:
    auth = TokenAuth(token)
    with httpx.Client(auth=auth) as client:
        r = client.get(url)
        r.raise_for_status()
        return r.status_code

if __name__ == "__main__":
    print(call_api("https://httpbin.org/status/200", token="example-token"))
```
* Implement custom auth by subclassing `httpx.Auth` and yielding a `Request` in `auth_flow()`.
* **Status**: Current, stable

### In-process app testing with explicit transports ✅ Current
```python
import httpx

def wsgi_app(environ, start_response):
    start_response("200 OK", [("Content-Type", "text/plain")])
    return [b"ok"]

def test_wsgi_like_call() -> str:
    transport = httpx.WSGITransport(app=wsgi_app)
    with httpx.Client(transport=transport, base_url="http://testserver") as client:
        r = client.get("/")
        r.raise_for_status()
        return r.text

if __name__ == "__main__":
    print(test_wsgi_like_call())
```
* Use `transport=httpx.WSGITransport(...)` or `transport=httpx.ASGITransport(...)` instead of removed `app=...` shortcuts.
* **Status**: Current, stable

## Configuration

- **Clients**
  - Create a client with `httpx.Client(...)` or `httpx.AsyncClient(...)`.
  - Prefer context managers to ensure cleanup: `with Client() as client:` / `async with AsyncClient() as client:`.
- **Authentication**
  - Per-request: `client.get(url, auth=httpx.BasicAuth("user", "pass"))`
  - Client-wide: `httpx.Client(auth=httpx.BasicAuth(...))`
  - Supported auth helpers: `httpx.BasicAuth`, `httpx.DigestAuth`, `httpx.NetRCAuth`, plus custom `httpx.Auth` subclasses.
- **Proxy**
  - Use `proxy=` (note: `proxies=` was removed in 0.28.0).
  - For complex routing, use `mounts=` (referenced as the alternative in changelog notes).
- **Transports**
  - In-process testing: `transport=httpx.WSGITransport(app=...)` or `transport=httpx.ASGITransport(app=...)`.
  - Network transports: `httpx.HTTPTransport`, `httpx.AsyncHTTPTransport` (when you need explicit transport configuration).
- **Error handling**
  - Bad URLs raise `httpx.InvalidURL`.
  - HTTP error statuses: call `Response.raise_for_status()`.

## Pitfalls

### Wrong: Repeated top-level calls (no connection pooling)
```python
import httpx

def many_requests() -> None:
    for _ in range(100):
        httpx.get("https://www.example.org/")  # new connection each time

if __name__ == "__main__":
    many_requests()
```

### Right: Use `httpx.Client` for pooling
```python
import httpx

def many_requests() -> None:
    with httpx.Client() as client:
        for _ in range(100):
            client.get("https://www.example.org/")

if __name__ == "__main__":
    many_requests()
```

### Wrong: Forgetting to close a client
```python
import httpx

def fetch() -> int:
    client = httpx.Client()
    r = client.get("https://www.example.com/")
    # Forgot: client.close()
    return r.status_code

if __name__ == "__main__":
    print(fetch())
```

### Right: Use a context manager (or call `close()`)
```python
import httpx

def fetch() -> int:
    with httpx.Client() as client:
        r = client.get("https://www.example.com/")
        return r.status_code

if __name__ == "__main__":
    print(fetch())
```

### Wrong: Doing I/O/locking inside `Auth.auth_flow()`
```python
import httpx
import threading

_lock = threading.RLock()

def read_token_from_disk() -> str:
    return "token-from-disk"

class MyCustomAuth(httpx.Auth):
    def auth_flow(self, request: httpx.Request):
        # Wrong: locking / external I/O inside auth_flow.
        with _lock:
            token = read_token_from_disk()
        request.headers["Authorization"] = f"Token {token}"
        yield request

if __name__ == "__main__":
    with httpx.Client(auth=MyCustomAuth()) as client:
        r = client.get("https://httpbin.org/status/200")
        print(r.status_code)
```

### Right: Override `sync_auth_flow()` / `async_auth_flow()` for I/O
```python
import asyncio
import threading
import httpx

def read_token_from_disk() -> str:
    return "token-from-disk"

async def read_token_from_disk_async() -> str:
    await asyncio.sleep(0)
    return "token-from-disk"

class MyCustomAuth(httpx.Auth):
    def __init__(self) -> None:
        self._sync_lock = threading.RLock()
        self._async_lock = asyncio.Lock()

    def sync_auth_flow(self, request: httpx.Request):
        with self._sync_lock:
            token = read_token_from_disk()
        request.headers["Authorization"] = f"Token {token}"
        yield request

    async def async_auth_flow(self, request: httpx.Request):
        async with self._async_lock:
            token = await read_token_from_disk_async()
        request.headers["Authorization"] = f"Token {token}"
        yield request

if __name__ == "__main__":
    with httpx.Client(auth=MyCustomAuth()) as client:
        print(client.get("https://httpbin.org/status/200").status_code)
```

### Wrong: Using removed `proxies=` argument 🗑️ Removed
```python
import httpx

def build_client() -> httpx.Client:
    # Removed in 0.28.0 (this will raise TypeError on 0.28+).
    return httpx.Client(proxies={"https://": "http://proxy.local:3128"})

if __name__ == "__main__":
    build_client()
```

### Right: Use `proxy=` (or `mounts=` for complex setups)
```python
import httpx

def build_client() -> httpx.Client:
    return httpx.Client(proxy="http://proxy.local:3128")

if __name__ == "__main__":
    with build_client() as client:
        r = client.get("https://httpbin.org/status/200")
        print(r.status_code)
```

## References

- [Changelog](https://github.com/encode/httpx/blob/master/CHANGELOG.md)
- [Documentation](https://www.python-httpx.org)
- [Homepage](https://github.com/encode/httpx)
- [Source](https://github.com/encode/httpx)

## Migration from v0.27.x

What changed in this version line (0.28.0 → 0.28.1 and upgrade notes around 0.28.0):

- **Breaking change (0.28.0)**: `proxies=` removed.
  - **Before (0.27.x)**:
    ```python
    import httpx
    client = httpx.Client(proxies={"https://": "http://proxy.local:3128"})
    ```
  - **After (0.28.0+)**:
    ```python
    import httpx
    client = httpx.Client(proxy="http://proxy.local:3128")
    ```
- **Breaking change (0.28.0)**: `app=` shortcut removed.
  - **Before (0.27.x)**:
    ```python
    import httpx
    client = httpx.Client(app="...")  # deprecated in 0.27.0, removed in 0.28.0
    ```
  - **After (0.28.0+)**:
    ```python
    import httpx
    client = httpx.Client(transport=httpx.ASGITransport(app="..."))
    ```
- **Behavior change (0.28.0)**: default JSON request bodies are serialized more compactly.
  - Update tests that compare raw JSON bytes/text; prefer comparing `response.json()` results.
- **Deprecations (0.28.0)**: SSL config warnings when passing `verify` as a string path and when using `cert=...`.
  - Migrate to the SSL configuration described in `docs/advanced/ssl.md` (e.g., `verify=True/False` or `verify=<ssl.SSLContext>` remain supported per changelog notes).
- **0.28.1**: bugfix release (SSL edge case when `verify=False` together with client-side certificates).

## API Reference

- **httpx.get(url, \*\*kwargs)** - Top-level GET request. Good for one-offs; does not reuse connections across calls.
- **httpx.request(method, url, \*\*kwargs)** - Generic top-level request entry point.
- **httpx.Client(...)** - Sync client with connection pooling. Key params commonly used by agents: `auth=...`, `proxy=...`, `transport=...`.
- **Client.get(url, \*\*kwargs)** - Send a GET using the client.
- **Client.request(method, url, \*\*kwargs)** - Send an arbitrary request using the client.
- **Client.close()** - Close the client and release resources (prefer `with Client() as client:`).
- **httpx.AsyncClient(...)** - Async client with pooling; use `async with`.
- **httpx.Request(...)** - Request object (used in custom auth flows and advanced cases).
- **httpx.Response(...)** - Response object returned by requests.
- **Response.raise_for_status()** - Raise on 4xx/5xx; returns the response (chainable).
- **Response.iter_text()** - Iterate response body as text (streaming-style consumption).
- **httpx.Auth** - Base class for custom auth.
  - **Auth.auth_flow(request)** - Generator-based flow yielding requests (no non-HTTP I/O).
  - **Auth.sync_auth_flow(request)** / **Auth.async_auth_flow(request)** - Override when you need I/O, locks, or async coordination.
- **httpx.BasicAuth(username, password)** - HTTP Basic authentication.
- **httpx.DigestAuth(username, password)** - HTTP Digest authentication.
- **httpx.NetRCAuth(file=None)** - Load credentials from netrc.
- **httpx.FunctionAuth(callable)** - Public auth helper (function-based auth hook).
- **httpx.WSGITransport(app=...)** - Transport for calling WSGI apps in-process.
- **httpx.ASGITransport(app=...)** - Transport for calling ASGI apps in-process.
- **httpx.HTTPTransport(...)** / **httpx.AsyncHTTPTransport(...)** - Explicit network transports for advanced configuration.
- **httpx.InvalidURL** - Exception raised for invalid URLs.
- **httpx.URLTypes** - URL input type alias shortcut.