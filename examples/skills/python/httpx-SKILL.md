---
name: httpx
description: A modern, fast, fully‑featured HTTP client for Python that supports both synchronous and asynchronous requests.
license: BSD-3-Clause
metadata:
  version: "0.28.1"
  ecosystem: python
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```python
import httpx
from httpx import AsyncClient, BasicAuth, Client, DigestAuth, NetRCAuth, Response
from httpx import ASGITransport, WSGITransport
```

## Core Patterns

### One-off request with top-level API ✅ Current
```python
import httpx

def fetch_json(url: str) -> dict:
    response = httpx.get(url)
    response.raise_for_status()
    return response.json()

if __name__ == "__main__":
    data = fetch_json("https://httpbin.org/json")
    print(sorted(data.keys()))
```
* Use `httpx.get()`/`httpx.request()` for quick scripts or single calls.
* Prefer a persistent `httpx.Client` once you have multiple requests.

### Persistent client with connection pooling (recommended) ✅ Current
```python
import httpx

def fetch_many(urls: list[str]) -> list[int]:
    status_codes: list[int] = []
    with httpx.Client() as client:
        for url in urls:
            r = client.get(url)
            status_codes.append(r.status_code)
    return status_codes

if __name__ == "__main__":
    codes = fetch_many(["https://httpbin.org/status/200", "https://httpbin.org/status/204"])
    print(codes)
```
* Use `httpx.Client()` for connection pooling, cookie persistence, shared headers, proxy support, and HTTP/2 support (when configured).
* Use `with httpx.Client() as client:` (or call `client.close()`) to release resources.

### Async requests with AsyncClient ✅ Current
```python
import asyncio
import httpx

async def fetch_text(url: str) -> str:
    async with httpx.AsyncClient() as client:
        r = await client.get(url)
        r.raise_for_status()
        return r.text

async def main() -> None:
    text = await fetch_text("https://httpbin.org/uuid")
    print(text.strip())

if __name__ == "__main__":
    asyncio.run(main())
```
* Use `httpx.AsyncClient()` for async I/O; always `await` request calls.
* Use `async with` to ensure the client is closed.

### Authentication (built-in) ✅ Current
```python
import httpx

def fetch_with_basic_auth(url: str, username: str, password: str) -> int:
    auth = httpx.BasicAuth(username, password)
    with httpx.Client(auth=auth) as client:
        r = client.get(url)
        return r.status_code

if __name__ == "__main__":
    code = fetch_with_basic_auth("https://httpbin.org/basic-auth/user/pass", "user", "pass")
    print(code)
```
* Configure auth per request (`client.get(..., auth=…)`) or on the client (`Client(auth=…)`) depending on scope.
* Other built‑ins include `DigestAuth` and `NetRCAuth`.

### Mounting an ASGI/WSGI app via explicit transports ✅ Current
```python
import httpx

async def asgi_app(scope, receive, send) -> None:
    assert scope["type"] == "http"
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [(b"content-type", b"text/plain")],
        }
    )
    await send({"type": "http.response.body", "body": b"ok"})

def wsgi_app(environ, start_response):
    start_response("200 OK", [("Content-Type", "text/plain")])
    return [b"ok"]

def call_wsgi() -> str:
    transport = httpx.WSGITransport(app=wsgi_app)
    with httpx.Client(transport=transport, base_url="http://testserver") as client:
        return client.get("/").text

async def call_asgi() -> str:
    transport = httpx.ASGITransport(app=asgi_app)
    async with httpx.AsyncClient(transport=transport, base_url="http://testserver") as client:
        return (await client.get("/")).text

if __name__ == "__main__":
    print(call_wsgi())
    import asyncio
    print(asyncio.run(call_asgi()))
```
* Use `transport=httpx.WSGITransport(app=…)` or `transport=httpx.ASGITransport(app=…)`.
* The older `app=` shortcut was removed in 0.28.0 (see Migration).

### Streaming responses ✅ Current
```python
import httpx

def stream_text(url: str) -> None:
    with httpx.Client() as client:
        with client.stream("GET", url) as response:
            response.raise_for_status()
            for chunk in response.iter_text():
                print(chunk, end="", flush=True)

if __name__ == "__main__":
    stream_text("https://httpbin.org/stream/3")
```
* Use `client.stream()` as a context manager to keep the connection open while iterating.
* Call `response.iter_text()`, `response.iter_bytes()`, or `response.iter_lines()` inside the block.

### POST with JSON body ✅ Current
```python
import httpx

def create_resource(url: str, payload: dict) -> dict:
    with httpx.Client() as client:
        r = client.post(url, json=payload)
        r.raise_for_status()
        return r.json()

if __name__ == "__main__":
    result = create_resource("https://httpbin.org/post", {"key": "value"})
    print(result.get("json"))
```
* Pass `json=` for automatic serialization and `Content-Type: application/json`.
* JSON bodies use a compact representation (no extra whitespace) as of 0.28.0.

### Timeout configuration ✅ Updated
```python
import httpx

def fetch_with_timeout(url: str) -> int:
    # `connect` timeout of 5 s, overall read timeout of 10 s
    timeout = httpx.Timeout(connect=5.0, read=10.0)
    with httpx.Client(timeout=timeout) as client:
        r = client.get(url)
        return r.status_code

if __name__ == "__main__":
    print(fetch_with_timeout("https://httpbin.org/delay/1"))
```
* `httpx.Timeout(connect=…, read=…, write=…, pool=…)` — set per‑phase timeouts.
* Pass `timeout=None` to disable timeouts entirely.

### SSL configuration ✅ Current
```python
import ssl
import httpx

def fetch_with_custom_ca(url: str, cafile: str) -> int:
    ctx = ssl.create_default_context(cafile=cafile)
    with httpx.Client(verify=ctx) as client:
        r = client.get(url)
        return r.status_code
```
* Pass `verify=<ssl.SSLContext>` for custom CA bundles or client certificates.
* `verify=True` (default) uses the system CA store; `verify=False` disables verification (insecure).
* Passing a string path to `verify` is deprecated as of 0.28.0 — use an `ssl.SSLContext` instead. ⚠️

### Custom transport with HTTPTransport (advanced) ✅ New
```python
import httpx

def custom_http_transport() -> httpx.Response:
    transport = httpx.HTTPTransport(http2=True)  # enable HTTP/2
    with httpx.Client(transport=transport) as client:
        return client.get("https://http2.pro/api/v1")

if __name__ == "__main__":
    resp = custom_http_transport()
    print(resp.http_version, resp.status_code)
```
* `httpx.HTTPTransport` (or `httpx.AsyncHTTPTransport`) gives low‑level control over TLS verification, proxy handling, socket options, and HTTP/2.
* Useful when you need to tweak `limits`, `socket_options`, or enable HTTP/2 without the extra `httpx[http2]` extra.

## Configuration

- **Client lifecycle**
  - Recommended: `with httpx.Client(...) as client:` / `async with httpx.AsyncClient(...) as client:`
  - Manual cleanup: `client.close()` / `await client.aclose()` when not using a context manager.
- **Auth**
  - Pass `auth=` per request or set `Client(auth=…)`.
  - Built‑ins: `httpx.BasicAuth`, `httpx.DigestAuth`, `httpx.NetRCAuth`.
  - Custom: implement `httpx.Auth` and its `auth_flow()`/`sync_auth_flow()`/`async_auth_flow()`.
- **Proxy configuration**
  - Use `proxy=` for simple cases.
  - For complex routing, use `mounts=` (preferred over the removed `proxies=` argument). ⚠️
- **Transports**
  - In‑process apps: `WSGITransport`, `ASGITransport`.
  - Network transports: `HTTPTransport`, `AsyncHTTPTransport` (advanced usage).
- **SSL notes (0.28.0 deprecations) ⚠️**
  - Passing `verify` as a string path and using `cert=` are deprecated and will issue warnings.
  - `verify=True`, `verify=False`, or `verify=<ssl.SSLContext>` remain valid.
- **JSON request formatting changed (0.28.0)**
  - Default JSON encoding is now more compact; tests asserting exact JSON bytes may need updates.
  - If you need stable formatting, pre‑serialize and send via `content=` with an explicit `content-type`.
- **HTTP/2**
  - Enable with `Client(http2=True)` after installing `httpx[http2]`.
- **Connection pool limits**
  - Configure with `httpx.Limits(max_connections=…, max_keepalive_connections=…, keepalive_expiry=…)`.

## Pitfalls

### Wrong: Using top‑level `httpx.get()` in a loop (no pooling)
```python
import httpx

def download_many() -> None:
    for _ in range(10):
        httpx.get("https://httpbin.org/get").raise_for_status()

if __name__ == "__main__":
    download_many()
```

### Right: Reuse a `httpx.Client()` for pooling
```python
import httpx

def download_many() -> None:
    with httpx.Client() as client:
        for _ in range(10):
            client.get("https://httpbin.org/get").raise_for_status()

if __name__ == "__main__":
    download_many()
```

### Wrong: Forgetting to close a `httpx.Client()`
```python
import httpx

def fetch_once() -> int:
    client = httpx.Client()
    r = client.get("https://httpbin.org/status/200")
    r.raise_for_status()
    return r.status_code  # client.close() never called

if __name__ == "__main__":
    print(fetch_once())
```

### Right: Use a context manager (or call `client.close()`)
```python
import httpx

def fetch_once() -> int:
    with httpx.Client() as client:
        r = client.get("https://httpbin.org/status/200")
        r.raise_for_status()
        return r.status_code

if __name__ == "__main__":
    print(fetch_once())
```

### Wrong: Using removed `proxies=` argument (0.28+) ⚠️
```python
import httpx

def build_client() -> httpx.Client:
    return httpx.Client(proxies={"https": "http://proxy.local:8080"})  # removed in 0.28.0

if __name__ == "__main__":
    build_client()
```

### Right: Use `proxy=` (or `mounts=` for complex routing)
```python
import httpx

def build_client() -> httpx.Client:
    return httpx.Client(proxy="http://proxy.local:8080")

if __name__ == "__main__":
    with build_client() as client:
        print(client.get("https://httpbin.org/get").status_code)
```

### Wrong: Passing a string path to `verify=` (deprecated in 0.28.0) ⚠️
```python
import httpx

def build_client() -> httpx.Client:
    return httpx.Client(verify="/path/to/ca-bundle.pem")  # deprecated in 0.28.0
```

### Right: Use an `ssl.SSLContext`
```python
import ssl
import httpx

def build_client(cafile: str) -> httpx.Client:
    ctx = ssl.create_default_context(cafile=cafile)
    return httpx.Client(verify=ctx)

if __name__ == "__main__":
    with build_client("/path/to/ca-bundle.pem") as client:
        print(client.get("https://httpbin.org/get").status_code)
```

### Wrong: Custom `httpx.Auth.auth_flow()` doing async/non‑HTTP I/O
```python
import asyncio
import httpx

async def get_token() -> str:
    await asyncio.sleep(0)
    return "token"

class TokenAuth(httpx.Auth):
    def auth_flow(self, request: httpx.Request):
        # BAD: running async I/O inside a sync generator.
        token = asyncio.get_event_loop().run_until_complete(get_token())
        request.headers["Authorization"] = f"Bearer {token}"
        yield request

def main() -> None:
    with httpx.Client(auth=TokenAuth()) as client:
        client.get("https://httpbin.org/get")

if __name__ == "__main__":
    main()
```

### Right: Provide `sync_auth_flow()` and `async_auth_flow()`
```python
import asyncio
import threading
import httpx

class TokenAuth(httpx.Auth):
    def __init__(self) -> None:
        self._sync_lock = threading.RLock()
        self._async_lock = asyncio.Lock()

    def _sync_get_token(self) -> str:
        with self._sync_lock:
            return "token"

    def sync_auth_flow(self, request: httpx.Request):
        token = self._sync_get_token()
        request.headers["Authorization"] = f"Bearer {token}"
        yield request

    async def _async_get_token(self) -> str:
        async with self._async_lock:
            await asyncio.sleep(0)
            return "token"

    async def async_auth_flow(self, request: httpx.Request):
        token = await self._async_get_token()
        request.headers["Authorization"] = f"Bearer {token}"
        yield request

def main() -> None:
    with httpx.Client(auth=TokenAuth()) as client:
        client.get("https://httpbin.org/get").raise_for_status()

if __name__ == "__main__":
    main()
```

### Wrong: Accessing `response.encoding` after `.text` is cached
```python
import httpx

def fetch_latin(url: str) -> str:
    with httpx.Client() as client:
        response = client.get(url)
        _ = response.text          # caches decoded text
        response.encoding = "latin-1"  # raises ValueError
        return response.text
```

### Right: Set encoding before accessing `.text`
```python
import httpx

def fetch_latin(url: str) -> str:
    with httpx.Client() as client:
        response = client.get(url)
        response.encoding = "latin-1"  # set before first .text access
        return response.text

if __name__ == "__main__":
    print(fetch_latin("https://httpbin.org/get"))
```

## References

- [Changelog](https://github.com/encode/httpx/blob/master/CHANGELOG.md)
- [Documentation](https://www.python-httpx.org)
- [Homepage](https://github.com/encode/httpx)
- [Source](https://github.com/encode/httpx)

## Migration from v0.27.x → v0.28.0

### `proxies=` argument removed 🗑️
- Deprecated since: 0.26.0, removed in 0.28.0
- Modern alternative: `proxy=` for simple cases, or `mounts=` for complex routing
- Migration guidance:
```python
import httpx

# Before (0.27.x and earlier; removed in 0.28.0)
# client = httpx.Client(proxies={"https": "http://proxy.local:8080"})

# After (0.28.0+)
with httpx.Client(proxy="http://proxy.local:8080") as client:
    r = client.get("https://httpbin.org/get")
    print(r.status_code)
```

### `app=` shortcut removed 🗑️
- Deprecated since: 0.27.0, removed in 0.28.0
- Modern alternative: `transport=httpx.ASGITransport(app=…)` or `transport=httpx.WSGITransport(app=…)`
- Migration guidance:
```python
import httpx

async def asgi_app(scope, receive, send) -> None:
    await send({"type": "http.response.start", "status": 204, "headers": []})
    await send({"type": "http.response.body", "body": b""})

# Before (removed in 0.28.0)
# client = httpx.AsyncClient(app=asgi_app)

# After
transport = httpx.ASGITransport(app=asgi_app)
async with httpx.AsyncClient(transport=transport, base_url="http://testserver") as client:
    r = await client.get("/")
    print(r.status_code)
```

### SSL arguments deprecated ⚠️
- Deprecated since: 0.28.0
- Passing a string path to `verify` or using the `cert` argument issues deprecation warnings.
- Modern alternative: Use `verify=True`, `verify=False`, or `verify=<ssl.SSLContext>`. Load client certificates via `ssl.SSLContext.load_cert_chain()`.
```python
import ssl
import httpx

# Before (deprecated in 0.28.0)
# client = httpx.Client(verify="/path/to/ca.pem", cert=("client.crt", "client.key"))

# After
ctx = ssl.create_default_context(cafile="/path/to/ca.pem")
ctx.load_cert_chain("client.crt", "client.key")
with httpx.Client(verify=ctx) as client:
    r = client.get("https://example.com")
    print(r.status_code)
```

### JSON request formatting changed ✅ (behavioral change in 0.28.0)
- Still works: true (but output bytes may differ)
- Modern alternative: if you require stable JSON bytes, pre‑serialize and send via `content=` with an explicit `content-type`.
```python
import json
import httpx

def post_stable_json(url: str, payload: dict) -> int:
    body = json.dumps(payload, separators=(",", ":"), sort_keys=True).encode("utf-8")
    headers = {"content-type": "application/json"}
    r = httpx.request("POST", url, content=body, headers=headers)
    return r.status_code

if __name__ == "__main__":
    print(post_stable_json("https://httpbin.org/post", {"b": 1, "a": 2}))
```

### `params={}` now strictly replaces query string (0.28.0)
- Behavioral change: passing `params={}` now **replaces** (not merges) the existing query string.
- Review any code that passes `params={}` expecting the original query string to be preserved.

## API Reference

- **httpx.get**
  ```python
  httpx.get(
      url: URL | str,
      *,
      params: QueryParamTypes | None = None,
      headers: HeaderTypes | None = None,
      cookies: CookieTypes | None = None,
      timeout: TimeoutTypes | None = None,
      follow_redirects: bool = False,
      auth: AuthTypes | None = None,
      proxy: ProxyTypes | None = None,
      extensions: dict[str, Any] | None = None,
      **kwargs,
  ) -> Response
  ```

- **httpx.post**
  ```python
  httpx.post(
      url: URL | str,
      *,
      content: RequestContent | None = None,
      data: RequestData | None = None,
      json: Any | None = None,
      files: RequestFiles | None = None,
      params: QueryParamTypes | None = None,
      headers: HeaderTypes | None = None,
      cookies: CookieTypes | None = None,
      timeout: TimeoutTypes | None = None,
      follow_redirects: bool = False,
      auth: AuthTypes | None = None,
      proxy: ProxyTypes | None = None,
      extensions: dict[str, Any] | None = None,
      **kwargs,
  ) -> Response
  ```

- **httpx.put**, **httpx.delete**, **httpx.head**, **httpx.options**, **httpx.request**, **httpx.stream**
  Signatures analogous to `get`/`post`, supporting `proxy`, `timeout`, `auth`, `extensions`, etc.

- **httpx.Client**
  ```python
  httpx.Client(
      base_url: URL | str | None = None,
      *,
      timeout: Timeout | None = None,
      transport: BaseTransport | None = None,
      verify: ssl.SSLContext | bool = True,
      cert: CertTypes | None = None,
      trust_env: bool = True,
      limits: Limits = DEFAULT_LIMITS,
      http2: bool = False,
      follow_redirects: bool = False,
      auth: AuthTypes | None = None,
      headers: HeaderTypes | None = None,
      cookies: CookieTypes | None = None,
      event_hooks: Mapping[str, list[Callable[..., Any]]] | None = None,
      proxy: ProxyTypes | None = None,
      mounts: dict[ProxyTypes, BaseTransport] | None = None,
      max_redirects: int = 20,
      default_encoding: str = "utf-8",
  ) -> Client
  ```

- **httpx.AsyncClient**
  ```python
  httpx.AsyncClient(
      base_url: URL | str | None = None,
      *,
      timeout: Timeout | None = None,
      transport: AsyncBaseTransport | None = None,
      verify: ssl.SSLContext | bool = True,
      cert: CertTypes | None = None,
      trust_env: bool = True,
      limits: Limits = DEFAULT_LIMITS,
      http2: bool = False,
      follow_redirects: bool = False,
      auth: AuthTypes | None = None,
      headers: HeaderTypes | None = None,
      cookies: CookieTypes | None = None,
      event_hooks: Mapping[str, list[Callable[..., Any]]] | None = None,
      proxy: ProxyTypes | None = None,
      mounts: dict[ProxyTypes, AsyncBaseTransport] | None = None,
      max_redirects: int = 20,
      default_encoding: str = "utf-8",
  ) -> AsyncClient
  ```

- **httpx.Request**
  ```python
  httpx.Request(
      method: bytes | str,
      url: URL | str,
      headers: HeaderTypes | None = None,
      content: RequestContent | None = None,
      data: RequestData | None = None,
      files: RequestFiles | None = None,
      json: Any | None = None,
      extensions: dict[str, Any] | None = None,
  ) -> Request
  ```

- **httpx.Response**
  ```python
  httpx.Response(
      status_code: int,
      headers: HeaderTypes | None = None,
      content: bytes | None = None,
      stream: SyncByteStream | AsyncByteStream | None = None,
      request: Request | None = None,
      extensions: dict[str, Any] | None = None,
  ) -> Response
  ```

- **httpx.HTTPTransport**
  ```python
  httpx.HTTPTransport(
      verify: ssl.SSLContext | bool = True,
      cert: CertTypes | None = None,
      trust_env: bool = True,
      http1: bool = True,
      http2: bool = False,
      limits: Limits = DEFAULT_LIMITS,
      proxy: ProxyTypes | None = None,
      uds: str | None = None,
      local_address: str | None = None,
      retries: int = 0,
      socket_options: Iterable[SOCKET_OPTION] | None = None,
  ) -> HTTPTransport
  ```

- **httpx.AsyncHTTPTransport**
  ```python
  httpx.AsyncHTTPTransport(
      verify: ssl.SSLContext | bool = True,
      cert: CertTypes | None = None,
      trust_env: bool = True,
      http1: bool = True,
      http2: bool = False,
      limits: Limits = DEFAULT_LIMITS,
      proxy: ProxyTypes | None = None,
      uds: str | None = None,
      local_address: str | None = None,
      retries: int = 0,
      socket_options: Iterable[SOCKET_OPTION] | None = None,
  ) -> AsyncHTTPTransport
  ```

- **httpx.WSGITransport**
  ```python
  httpx.WSGITransport(
      app: WSGIApplication,
      raise_app_exceptions: bool = True,
      script_name: str = "",
      remote_addr: str = "127.0.0.1",
      wsgi_errors: typing.TextIO | None = None,
  ) -> WSGITransport
  ```

- **httpx.ASGITransport**
  ```python
  httpx.ASGITransport(
      app: _ASGIApp,
      raise_app_exceptions: bool = True,
      root_path: str = "",
      client: tuple[str, int] = ("127.0.0.1", 123),
  ) -> ASGITransport
  ```

- **httpx.Limits**
  ```python
  httpx.Limits(
      max_connections: int = 100,
      max_keepalive_connections: int = 20,
      keepalive_expiry: float = 5.0,
  ) -> Limits
  ```

- **httpx.Proxy**
  ```python
  httpx.Proxy(
      url: str | URL,
      *,
      ssl_context: ssl.SSLContext | None = None,
      auth: tuple[str, str] | None = None,
      headers: HeaderTypes | None = None,
  ) -> Proxy
  ```

- **httpx.Timeout**
  ```python
  httpx.Timeout(
      connect: float | None = None,
      read: float | None = None,
      write: float | None = None,
      pool: float | None = None,
  ) -> Timeout
  ```

- **httpx.codes**
  ```python
  class httpx.codes(IntEnum):
      ...
  ```

--- 

**Security Note:**  
All examples are designed for use within your own project directory and for safe, local development or controlled network requests. Never use these patterns to transmit, modify, or access data outside your intended project or environment. Do not copy/paste code into environments where you lack permission or understanding of the security implications.