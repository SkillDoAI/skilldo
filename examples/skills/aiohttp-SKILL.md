---
name: aiohttp
description: Async HTTP client/server framework for asyncio, including a web server and client session APIs.
version: 3.13.3
ecosystem: python
license: MIT
---

## Imports

```python
import asyncio

import aiohttp
from aiohttp import web
from aiohttp import hdrs

from aiohttp.test_utils import AioHTTPTestCase, TestClient, TestServer
```

## Core Patterns

### ClientSession: GET/POST with proper cleanup ✅ Current
```python
import asyncio
import aiohttp


async def main() -> None:
    url = "https://httpbin.org/get"
    async with aiohttp.ClientSession() as session:
        async with session.get(url, params={"q": "test"}) as resp:
            resp.raise_for_status()
            data = await resp.json()
            print(data["url"])


if __name__ == "__main__":
    asyncio.run(main())
```
* Uses `aiohttp.ClientSession` with an `async with` block to ensure the connector and sockets are closed.
* Uses `async with session.get(...) as resp` to release the connection back to the pool promptly.
* **Status**: Current, stable

### Web server routing + JSON response ✅ Current
```python
from aiohttp import web


async def health(request: web.Request) -> web.Response:
    return web.json_response({"status": "ok"})


def create_app() -> web.Application:
    app = web.Application()
    app.add_routes([web.get("/health", health)])
    return app


if __name__ == "__main__":
    web.run_app(create_app(), host="127.0.0.1", port=8080)
```
* Creates a `web.Application`, registers routes via `Application.add_routes`, runs with `web.run_app`.
* **Status**: Current, stable

### WebSocket handler (prepare + async for messages) ✅ Current
```python
from aiohttp import web


async def ws_handler(request: web.Request) -> web.WebSocketResponse:
    ws = web.WebSocketResponse()
    await ws.prepare(request)

    async for msg in ws:
        if msg.type == web.WSMsgType.TEXT:
            await ws.send_str(f"echo: {msg.data}")
        elif msg.type == web.WSMsgType.BINARY:
            await ws.send_bytes(msg.data)
        elif msg.type == web.WSMsgType.ERROR:
            break

    return ws


def create_app() -> web.Application:
    app = web.Application()
    app.add_routes([web.get("/ws", ws_handler)])
    return app


if __name__ == "__main__":
    web.run_app(create_app(), host="127.0.0.1", port=8080)
```
* `web.WebSocketResponse.prepare()` is required before receiving/sending.
* Uses `async for msg in ws` and `web.WSMsgType` to branch by message type.
* **Status**: Current, stable

### Client tracing with TraceConfig (correct handler signature) ✅ Current
```python
import asyncio
import aiohttp


async def on_request_start(
    session: aiohttp.ClientSession,
    context: object,
    params: aiohttp.TraceRequestStartParams,
) -> None:
    # params.headers is mutable
    print("starting", params.method, params.url)


async def main() -> None:
    trace = aiohttp.TraceConfig()
    trace.on_request_start.append(on_request_start)

    async with aiohttp.ClientSession(trace_configs=[trace]) as session:
        async with session.get("https://httpbin.org/get") as resp:
            await resp.text()


if __name__ == "__main__":
    asyncio.run(main())
```
* Tracing callbacks must be `async def on_signal(session, context, params)`.
* **Status**: Current, stable

### Testing a web app using TestServer + TestClient ✅ Current
```python
import asyncio
from aiohttp import web
from aiohttp.test_utils import TestClient, TestServer


async def index(request: web.Request) -> web.Response:
    return web.Response(text="ok")


async def main() -> None:
    app = web.Application()
    app.add_routes([web.get("/", index)])

    server = TestServer(app)
    client = TestClient(server)

    await client.start_server()
    try:
        # Prefer TestClient.get()/request() to auto-prefix the server URL
        async with client.get("/") as resp:
            assert resp.status == 200
            body = await resp.text()
            assert body == "ok"
    finally:
        await client.close()


if __name__ == "__main__":
    asyncio.run(main())
```
* `TestClient` requires a `BaseTestServer` (e.g., `TestServer(app)`); otherwise it raises `TypeError`.
* `TestClient.get()/request()` joins relative paths against the server root URL.
* **Status**: Current, stable

## Configuration

- **Client defaults**
  - `aiohttp.ClientSession()` manages a connection pool via an internal connector; reuse the same session for multiple requests.
  - Use `timeout=` per request or session-wide (via `aiohttp.ClientTimeout`, public API).
- **Tracing**
  - Configure via `aiohttp.TraceConfig()` and pass `trace_configs=[trace]` into `ClientSession(...)`.
  - Per-request trace context: use `TraceConfig.trace_config_ctx(trace_request_ctx=...)` when you need request-specific metadata.
- **Web server**
  - `web.run_app(app, host=..., port=...)` is the common entrypoint.
  - For advanced lifecycle control (e.g., embedding), use `web.AppRunner` (public API) with a `TCPSite` (public API, not listed above but part of aiohttp web runner system).
- **Testing**
  - `TestClient(server, cookie_jar=None, **kwargs)`: when `cookie_jar` is `None`, `TestClient` creates `aiohttp.CookieJar(unsafe=True)` internally (test behavior).

## Pitfalls

### Wrong: Not closing ClientSession / ClientResponse
```python
import asyncio
import aiohttp


async def main() -> None:
    session = aiohttp.ClientSession()
    resp = await session.get("https://httpbin.org/get")
    print(await resp.text())
    # missing: await resp.release()/resp.close() and await session.close()


asyncio.run(main())
```

### Right: Use async context managers for session + response
```python
import asyncio
import aiohttp


async def main() -> None:
    async with aiohttp.ClientSession() as session:
        async with session.get("https://httpbin.org/get") as resp:
            print(await resp.text())


if __name__ == "__main__":
    asyncio.run(main())
```

### Wrong: WebSocketResponse without prepare()
```python
from aiohttp import web


async def ws_handler(request: web.Request) -> web.WebSocketResponse:
    ws = web.WebSocketResponse()
    # missing: await ws.prepare(request)
    async for msg in ws:
        await ws.send_str(msg.data)
    return ws
```

### Right: Prepare before iterating/sending
```python
from aiohttp import web


async def ws_handler(request: web.Request) -> web.WebSocketResponse:
    ws = web.WebSocketResponse()
    await ws.prepare(request)

    async for msg in ws:
        if msg.type == web.WSMsgType.TEXT:
            await ws.send_str(msg.data)
    return ws
```

### Wrong: Using TestClient.session.request with a relative URL
```python
import asyncio
from aiohttp import web
from aiohttp.test_utils import TestClient, TestServer


async def main() -> None:
    app = web.Application()
    server = TestServer(app)
    client = TestClient(server)
    await client.start_server()
    try:
        # This does NOT auto-prefix the host/root URL:
        await client.session.request("GET", "/")  # likely invalid URL
    finally:
        await client.close()


asyncio.run(main())
```

### Right: Use TestClient.get()/request() or make_url()
```python
import asyncio
from aiohttp import web, hdrs
from aiohttp.test_utils import TestClient, TestServer


async def main() -> None:
    app = web.Application()
    server = TestServer(app)
    client = TestClient(server)
    await client.start_server()
    try:
        async with client.get("/") as resp:
            await resp.text()

        # Or: build an absolute URL explicitly
        url = client.make_url("/")
        async with client.session.request(hdrs.METH_GET, url) as resp:
            await resp.text()
    finally:
        await client.close()


asyncio.run(main())
```

### Wrong: Tracing callback has the wrong signature
```python
import aiohttp


async def on_request_start(params):  # wrong: aiohttp passes (session, context, params)
    print(params.url)


trace = aiohttp.TraceConfig()
trace.on_request_start.append(on_request_start)
```

### Right: Use (session, context, params)
```python
import aiohttp


async def on_request_start(
    session: aiohttp.ClientSession,
    context: object,
    params: aiohttp.TraceRequestStartParams,
) -> None:
    print(params.url)


trace = aiohttp.TraceConfig()
trace.on_request_start.append(on_request_start)
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

What changed in this version (if applicable):
- **Breaking changes**
  - Brotli/brotlicffi minimum supported version is now **1.2**.
  - Brotli decompression enforces a default maximum output size of **32MiB per decompress call**.
- **Deprecated → Current mapping**
  - No specific deprecation migrations provided in the inputs for 3.13.3.
- **Before/after notes**
  - If your environment pins Brotli/brotlicffi < 1.2, update pins to `Brotli>=1.2` and/or `brotlicffi>=1.2`.
  - If you handle Brotli-compressed responses that can expand beyond 32MiB in a single step, prefer processing response data incrementally (streaming) rather than relying on a single large decompress call.

## API Reference

- **aiohttp.ClientSession(**`**kwargs`**)** - HTTP client session; reuse for multiple requests; supports `trace_configs=[TraceConfig(...)]`.
- **ClientSession.request(method, url, \*\*kwargs)** - Perform a request; common kwargs: `params=`, `headers=`, `json=`, `data=`, `timeout=`, `ssl=`.
- **ClientSession.get/post/put/delete(...)** - Convenience wrappers around `request`.
- **aiohttp.ClientResponse** - Response object; use `await resp.text()`, `await resp.json()`, `await resp.read()`, and `resp.raise_for_status()`.
- **aiohttp.TraceConfig(...)** - Configure tracing signals (e.g., `on_request_start`).
- **TraceConfig.trace_config_ctx(trace_request_ctx=...)** - Create per-request trace context.
- **aiohttp.web.Application()** - Web app container; register routes and middleware.
- **aiohttp.web.Application.add_routes(routes)** - Add routes (e.g., `[web.get("/path", handler)]`).
- **aiohttp.web.get(path, handler)** - Route factory for HTTP GET.
- **aiohttp.web.Response(...)** - Basic HTTP response (text/body/headers/status).
- **aiohttp.web.WebSocketResponse()** - Server-side WebSocket; must `await prepare(request)` then send/receive.
- **aiohttp.web.WSMsgType** - Enum of websocket message types (`TEXT`, `BINARY`, `ERROR`, etc.).
- **aiohttp.web.run_app(app, host=..., port=...)** - Run an app with an event loop.
- **aiohttp.web.AppRunner(app)** - Lower-level server runner for embedding aiohttp.
- **aiohttp.test_utils.TestServer(app)** - Wrap a `web.Application` for tests.
- **aiohttp.test_utils.TestClient(server, cookie_jar=None, \*\*session_kwargs)** - Test client; use `await start_server()`, then `client.get()/request()/ws_connect()`.
- **aiohttp.test_utils.AioHTTPTestCase** - `unittest` base class; override `get_application()` and use `self.client` in async tests.