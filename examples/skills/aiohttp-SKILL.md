---

name: aiohttp
description: Async HTTP client/server framework for asyncio, providing ClientSession for outbound HTTP and aiohttp.web for building web servers.
version: 3.13.3
ecosystem: python
license: MIT
generated_with: gpt-5.2
---

## Imports

```python
import asyncio
import aiohttp
from aiohttp import TraceConfig, web
from aiohttp.test_utils import (
    AioHTTPTestCase,
    TestClient,
    TestServer,
    make_mocked_request,
    unused_port,
)
```

## Core Patterns

### HTTP client request with `ClientSession` ✅ Current
```python
import asyncio
import aiohttp


async def main() -> None:
    url = "https://example.com/"
    timeout = aiohttp.ClientTimeout(total=10)

    async with aiohttp.ClientSession(timeout=timeout) as session:
        async with session.get(url, headers={"Accept": "text/html"}) as resp:
            resp.raise_for_status()
            body = await resp.text()
            print(resp.status, len(body))


if __name__ == "__main__":
    asyncio.run(main())
```
* Use `async with aiohttp.ClientSession()` to ensure the connector and sockets are closed.
* Use `async with session.get(...) as resp` to ensure the response is released back to the connection pool.

### Client tracing with `TraceConfig` ✅ Current
```python
import asyncio
import time
from types import SimpleNamespace

import aiohttp
from aiohttp import TraceConfig


async def on_request_start(
    session: aiohttp.ClientSession,
    context: SimpleNamespace,
    params: aiohttp.TraceRequestStartParams,
) -> None:
    context.t0 = time.perf_counter()
    print("->", params.method, params.url)


async def on_request_end(
    session: aiohttp.ClientSession,
    context: SimpleNamespace,
    params: aiohttp.TraceRequestEndParams,
) -> None:
    dt = time.perf_counter() - context.t0
    print("<-", params.response.status, f"{dt:.3f}s")


async def main() -> None:
    trace_config = TraceConfig(trace_config_ctx_factory=SimpleNamespace)
    trace_config.on_request_start.append(on_request_start)
    trace_config.on_request_end.append(on_request_end)

    async with aiohttp.ClientSession(trace_configs=[trace_config]) as session:
        # Per-request context can be passed via trace_request_ctx
        async with session.get(
            "https://example.com/",
            trace_request_ctx={"request_id": "req-1"},
        ) as resp:
            await resp.read()


if __name__ == "__main__":
    asyncio.run(main())
```
* Trace callbacks are `async def on_signal(session, context, params)`; `context` comes from `TraceConfig.trace_config_ctx_factory`.
* Use `trace_request_ctx=...` to pass per-request metadata into `context` (available via `TraceConfig.trace_config_ctx(...)` internally).

### Web server routes + WebSocket handler (`aiohttp.web`) ✅ Current
```python
import asyncio
from aiohttp import web


async def index(request: web.Request) -> web.Response:
    return web.Response(text="ok")


async def websocket_echo(request: web.Request) -> web.WebSocketResponse:
    ws = web.WebSocketResponse()
    await ws.prepare(request)

    async for msg in ws:
        if msg.type == web.WSMsgType.TEXT:
            await ws.send_str(msg.data)
        elif msg.type == web.WSMsgType.BINARY:
            await ws.send_bytes(msg.data)
        else:
            break

    return ws


def create_app() -> web.Application:
    app = web.Application()
    app.router.add_get("/", index)
    app.router.add_get("/ws", websocket_echo)
    return app


async def main() -> None:
    app = create_app()
    web.run_app(app, host="127.0.0.1", port=8080)


if __name__ == "__main__":
    asyncio.run(main())
```
* Handlers are `async def handler(request: web.Request) -> web.StreamResponse`.
* WebSocket pattern: `WebSocketResponse()` → `await ws.prepare(request)` → `async for msg in ws` → `await ws.send_*`.

### Async integration testing with `TestServer` + `TestClient` ✅ Current
```python
import asyncio
from aiohttp import web
from aiohttp.test_utils import TestClient, TestServer


async def hello(request: web.Request) -> web.Response:
    return web.Response(text="hello")


async def main() -> None:
    app = web.Application()
    app.router.add_get("/", hello)

    async with TestServer(app) as server:
        async with TestClient(server) as client:
            async with client.get("/") as resp:
                text = await resp.text()
                print(resp.status, text)


if __name__ == "__main__":
    asyncio.run(main())
```
* `async with TestServer(app)` calls `start_server()` and `close()` automatically.
* `TestClient` tracks created responses and websockets and closes them on `client.close()`.

### Unit-test request object creation with `make_mocked_request` ✅ Current
```python
from aiohttp import hdrs, web
from aiohttp.test_utils import make_mocked_request


def build_request() -> web.Request:
    app = web.Application()
    req = make_mocked_request(
        "GET",
        "/",
        headers={hdrs.HOST: "example.com"},
        match_info={"id": "1"},
        app=app,
    )
    return req


if __name__ == "__main__":
    r = build_request()
    print(r.method, r.path, r.headers.get(hdrs.HOST), r.match_info["id"])
```
* Produces an `aiohttp.web.Request` suitable for unit-testing handlers/middleware without a real server.

## Configuration

* **Client timeouts**: configure via `aiohttp.ClientTimeout(total=..., connect=..., sock_read=...)` and pass to `ClientSession(timeout=...)`.
* **Tracing**: `TraceConfig(trace_config_ctx_factory=...)`, attach callbacks to signals like `on_request_start`, `on_request_end`, `on_request_exception`, DNS/connection events, etc.
* **Web app**: create `web.Application()` and register routes via `app.router.add_get(...)` (and other HTTP methods).
* **Testing ports**: `aiohttp.test_utils.unused_port()` returns an available port for tests (best-effort; still subject to race conditions).
* **Test client cookie jar**: `TestClient` uses an internal `ClientSession` with `CookieJar(unsafe=True)` by default (test-friendly behavior).

## Pitfalls

### Wrong: leaking connections by not closing `ClientSession` / response
```python
import asyncio
import aiohttp


async def main() -> None:
    session = aiohttp.ClientSession()
    resp = await session.get("https://example.com/")
    _ = await resp.text()
    # forgot: await resp.release() / resp.close()
    # forgot: await session.close()


if __name__ == "__main__":
    asyncio.run(main())
```

### Right: use async context managers for both session and response
```python
import asyncio
import aiohttp


async def main() -> None:
    async with aiohttp.ClientSession() as session:
        async with session.get("https://example.com/") as resp:
            _ = await resp.text()


if __name__ == "__main__":
    asyncio.run(main())
```

### Wrong: forgetting `await` in WebSocket handshake and sends
```python
from aiohttp import web


async def websocket_handler(request: web.Request) -> web.WebSocketResponse:
    ws = web.WebSocketResponse()
    ws.prepare(request)  # missing await

    async for msg in ws:
        if msg.type == web.WSMsgType.TEXT:
            ws.send_str(msg.data)  # missing await
    return ws
```

### Right: await `prepare()` and `send_*()` calls
```python
from aiohttp import web


async def websocket_handler(request: web.Request) -> web.WebSocketResponse:
    ws = web.WebSocketResponse()
    await ws.prepare(request)

    async for msg in ws:
        if msg.type == web.WSMsgType.TEXT:
            await ws.send_str(msg.data)
    return ws
```

### Wrong: `TestServer.make_url()` with an absolute URL/path
```python
import asyncio
from aiohttp import web
from aiohttp.test_utils import TestServer


async def handler(request: web.Request) -> web.Response:
    return web.Response(text="ok")


async def main() -> None:
    app = web.Application()
    app.router.add_get("/", handler)

    async with TestServer(app) as server:
        # make_url expects a relative path (e.g., "/"), not a full URL
        server.make_url("http://example.com/")  # may assert/fail


if __name__ == "__main__":
    asyncio.run(main())
```

### Right: pass a relative path to `make_url()`
```python
import asyncio
from aiohttp import web
from aiohttp.test_utils import TestServer


async def handler(request: web.Request) -> web.Response:
    return web.Response(text="ok")


async def main() -> None:
    app = web.Application()
    app.router.add_get("/", handler)

    async with TestServer(app) as server:
        url = server.make_url("/")
        print(url)


if __name__ == "__main__":
    asyncio.run(main())
```

### Wrong: assuming Unicode regex routes match raw Unicode (requoting mismatch)
```python
# This is illustrative: aiohttp requotes paths internally.
# If you rely on custom regex patterns for Unicode segments, they may not match as expected.
from aiohttp import web


async def handler(request: web.Request) -> web.Response:
    return web.Response(text="matched")


def create_app() -> web.Application:
    app = web.Application()
    # Risky if you expect raw Unicode matching; aiohttp matches against requoted/percent-encoded paths.
    app.router.add_get(r"/{name:[А-Яа-я]+}", handler)
    return app
```

### Right: prefer non-regex variable routes or match encoded form
```python
from aiohttp import web


async def handler(request: web.Request) -> web.Response:
    # Treat as text after aiohttp routing; avoid relying on raw-Unicode regex matching.
    name = request.match_info["name"]
    return web.Response(text=f"hello {name}")


def create_app() -> web.Application:
    app = web.Application()
    # Prefer a simple variable route unless you must enforce a regex.
    app.router.add_get(r"/{name}", handler)
    return app
```

## References

- [Homepage](https://github.com/aio-libs/aiohttp)
- [Chat: Matrix](https://matrix.to/#/#aio-libs:matrix.org)
- [Chat: Matrix Space](https://matrix.to/#/#aio-libs-space:matrix.org)
- [CI: GitHub Actions](https://github.com/aio-libs/aiohttp/actions?query=workflow%3ACI)
- [Coverage: codecov](https://codecov.io/github/aio-libs/aiohttp)
- [Docs: Changelog](https://docs.aiohttp.org/en/stable/changes.html)
- [Docs: RTD](https://docs.aiohttp.org)
- [GitHub: issues](https://github.com/aio-libs/aiohttp/issues)
- [GitHub: repo](https://github.com/aio-libs/aiohttp)

## Migration from v3.13.2

* ✅ Current: upgrade recommended (3.13.3 includes security/vulnerability fixes).
* **Brotli/brotlicffi minimum version**: now requires `Brotli>=1.2` or `brotlicffi>=1.2`.
* **Brotli decompression limit**: default maximum output size is **32 MiB per decompress call**.
  * Migration guidance: if you process very large Brotli payloads, prefer streaming reads (`async for chunk in resp.content.iter_chunked(...)`) and incremental processing rather than expecting a single huge in-memory decompression result.
* Packaging note: project metadata moved from `setup.cfg` to `pyproject.toml` (update any tooling that parses `setup.cfg`).
* Proxy auth behavior fix: proxy authorization headers are now correctly passed on connection reuse; re-test and remove workarounds for 407s if present.

## API Reference

- **aiohttp.ClientSession(**\*, timeout=None, trace_configs=None, headers=None, raise_for_status=False, connector=None, cookie_jar=None**)** - HTTP client session managing connection pooling and cookies.
- **aiohttp.ClientSession.get(url, \*\*kwargs)** / **post(...)** / **request(method, url, \*\*kwargs)** - Perform HTTP requests; use as `async with session.get(...) as resp:`.
- **aiohttp.ClientResponse.text(encoding=None, errors="strict")** - Read response body decoded as text.
- **aiohttp.ClientResponse.read()** - Read response body as bytes.
- **aiohttp.ClientResponse.raise_for_status()** - Raise an exception for 4xx/5xx responses.
- **aiohttp.TraceConfig(trace_config_ctx_factory=types.SimpleNamespace)** - Configure client tracing hooks.
- **aiohttp.TraceConfig.on_request_start / on_request_end / on_request_exception / on_dns_resolvehost_start / on_connection_create_start ...** - Signals (lists) to append async callbacks to.
- **aiohttp.web.Application()** - Web application container; holds router and app state.
- **aiohttp.web.Response(text=None, body=None, status=200, headers=None, content_type=None)** - Basic HTTP response.
- **aiohttp.web.WebSocketResponse()** - WebSocket server response; call `await prepare(request)` then send/receive messages.
- **aiohttp.web.WSMsgType** - Enum of websocket message types (e.g., `TEXT`, `BINARY`, `CLOSE`).
- **aiohttp.web.run_app(app, host=None, port=None, \*\*kwargs)** - Run an `Application` (blocking call; typically top-level).
- **aiohttp.web.get(path, handler, \*\*kwargs)** - Route helper/decorator for GET (also available via `app.router.add_get`).
- **aiohttp.test_utils.TestServer(app, host="127.0.0.1", port=0, scheme="")** - Test server wrapper; `await start_server()`, `make_url(path)`, `await close()`, supports `async with`.
- **aiohttp.test_utils.TestClient(server)** - Test client bound to a `TestServer`; `.get()/.post()/.request()` and `.ws_connect()`; supports `async with`.