---
name: requests
description: Synchronous HTTP client for sending HTTP requests and handling responses.
license: MIT
metadata:
  version: "2.32.5"
  ecosystem: python
  generated-by: skilldo/gpt-oss-120b + review:gpt-oss-120b
---

## Imports

```python
import requests
from requests import Session
from requests.auth import HTTPBasicAuth, HTTPDigestAuth, AuthBase
from requests.exceptions import (
    RequestException,
    Timeout,
    ConnectTimeout,
    ReadTimeout,
    ConnectionError,
    HTTPError,
    TooManyRedirects,
    JSONDecodeError,
)
```

## Core Patterns
### GET with query params + status checking ✅ Current
```python
from __future__ import annotations

import requests


def fetch_repo(owner: str, repo: str) -> dict:
    url = "https://api.github.com/repos/{owner}/{repo}".format(owner=owner, repo=repo)
    r = requests.get(url, params={"per_page": 1}, timeout=10)
    r.raise_for_status()
    return r.json()


if __name__ == "__main__":
    data = fetch_repo("psf", "requests")
    print(data["full_name"])
```
* Use `params=` for query strings; call `Response.raise_for_status()` before trusting the body.

### POST JSON body ✅ Current
```python
from __future__ import annotations

import os
import requests


def create_widget(api_base: str, token: str, name: str) -> dict:
    url = f"{api_base}/post"
    headers = {"Authorization": f"Bearer {token}"}

    r = requests.post(
        url,
        json={"name": name},
        headers=headers,
        timeout=10,
    )
    r.raise_for_status()
    return r.json()


if __name__ == "__main__":
    # Example: using httpbin.org/post to echo back JSON
    api_base = "https://httpbin.org"
    # In real code, fetch the token from a secure source (e.g., env var, secret manager)
    token = os.getenv("API_TOKEN", "<YOUR_API_TOKEN>")
    name = "mywidget"
    result = create_widget(api_base, token, name)
    # httpbin.org returns JSON with keys: json, headers, url, etc.
    assert isinstance(result, dict)
    # The JSON body should be echoed back under "json"
    assert result.get("json") == {"name": name}
    # The Authorization header should be present (case‑insensitive)
    headers = result.get("headers", {})
    authval = headers.get("Authorization") or headers.get("authorization")
    assert authval and token in authval
    # The URL should end with /post
    assert result.get("url", "").endswith("/post")
    print("✓ POST JSON body pattern ok")
```
* Prefer `json=` for JSON request bodies (sets JSON encoding; also sets an appropriate `Content-Type`).

### Reuse connections with Session ✅ Current
```python
from __future__ import annotations

import requests


def fetch_many(urls: list[str]) -> list[tuple[str, int]]:
    results: list[tuple[str, int]] = []
    with requests.Session() as s:
        s.headers.update({"User-Agent": "example-client/1.0"})
        for url in urls:
            r = s.get(url, timeout=10)
            results.append((r.url, r.status_code))
    return results


if __name__ == "__main__":
    out = fetch_many(["https://httpbin.org/get", "https://example.com/"])
    print(out)
```
* Use `requests.Session()` (or `requests.session()`) for connection pooling and shared headers/cookies.

### Streaming download to disk ✅ Current
```python
from __future__ import annotations

import requests


def download_file(url: str, path: str) -> None:
    with requests.get(url, stream=True, timeout=30) as r:
        r.raise_for_status()
        with open(path, "wb") as f:
            for chunk in r.iter_content(chunk_size=1024 * 128):
                if chunk:  # filter out keep-alive chunks
                    f.write(chunk)


if __name__ == "__main__":
    download_file("https://httpbin.org/bytes/1024", "out.bin")
    print("Wrote out.bin")
```
* For large responses, use `stream=True` and `Response.iter_content()`; write to a binary file.

### Auth: Basic/Digest/custom AuthBase ✅ Current
```python
from __future__ import annotations

import os
import requests
from requests.auth import AuthBase, HTTPBasicAuth, HTTPDigestAuth


class BearerAuth(AuthBase):
    def __init__(self, token: str) -> None:
        self._token = token

    def __call__(self, r: requests.PreparedRequest) -> requests.PreparedRequest:
        r.headers["Authorization"] = f"Bearer {self._token}"
        return r


def demo_auth() -> None:
    # Basic auth (tuple shorthand also works: auth=("user", "pass"))
    r1 = requests.get(
        "https://httpbin.org/basic-auth/user/pass",
        auth=HTTPBasicAuth("user", "pass"),
        timeout=10,
    )
    print(r1.status_code)

    # Digest auth
    r2 = requests.get(
        "https://httpbin.org/digest-auth/auth/user/pass",
        auth=HTTPDigestAuth("user", "pass"),
        timeout=10,
    )
    print(r2.status_code)

    # Custom auth – token should be obtained from a secure source
    token = os.getenv("BEARER_TOKEN", "<YOUR_BEARER_TOKEN>")
    r3 = requests.get(
        "https://httpbin.org/bearer", auth=BearerAuth(token), timeout=10
    )
    print(r3.status_code)


if __name__ == "__main__":
    demo_auth()
```
* Use `auth=("user","pass")` or `HTTPBasicAuth` for Basic; `HTTPDigestAuth` for Digest; subclass `AuthBase` for custom schemes.

### Custom HTTPAdapter with TLS context ⚠️ Deprecated `get_connection`
```python
from __future__ import annotations

import ssl
import requests
from requests.adapters import HTTPAdapter


class TLSAdapter(HTTPAdapter):
    """Adapter that supplies a custom SSLContext for HTTPS connections."""

    def __init__(self, ssl_context: ssl.SSLContext, **kwargs):
        self._ssl_context = ssl_context
        super().__init__(**kwargs)

    # New public API (Requests >= 2.32.0)
    def get_connection_with_tls_context(
        self,
        request,
        verify,
        proxies=None,
        cert=None,
    ):
        """
        Return a urllib3 connection pool that uses the supplied SSLContext.

        Parameters match the signature of ``HTTPAdapter.get_connection``:
        - ``request`` – the prepared ``requests.Request`` object.
        - ``verify`` – the ``verify`` flag or path passed to ``Session.request``.
        - ``proxies`` – optional proxy mapping.
        - ``cert`` – optional client certificate.
        """
        # ``HTTPAdapter.get_connection`` expects a URL string, not a PreparedRequest.
        # Use ``request.url`` to obtain the target URL.
        conn = super().get_connection(
            request.url,
            proxies=proxies,
        )
        # The underlying pool manager will pick up the SSLContext we stored.
        conn.poolmanager.connection_kw["ssl_context"] = self._ssl_context
        return conn


def fetch_with_custom_tls(url: str) -> str:
    # Example: enforce TLS 1.3 only
    ctx = ssl.create_default_context()
    ctx.minimum_version = ssl.TLSVersion.TLSv1_3

    session = requests.Session()
    session.mount("https://", TLSAdapter(ctx))
    resp = session.get(url, timeout=10)
    resp.raise_for_status()
    return resp.text


if __name__ == "__main__":
    print(fetch_with_custom_tls("https://www.example.com"))
```
* When subclassing `HTTPAdapter`, use `get_connection_with_tls_context`. The older `get_connection` method is now deprecated ⚠️ and will be removed in a future major release.

## Migration
### Breaking changes & migration steps
- **2.32.1 → 2.32.2** – `HTTPAdapter.get_connection` and the private `_get_connection` were deprecated in favor of the public `get_connection_with_tls_context`. Update any custom adapters to call the new method.
- **2.32.2 → 2.32.3** – Support for passing a custom `ssl_context` via `init_poolmanager` was removed. Use the `verify` argument or the new `get_connection_with_tls_context` API to control TLS settings.
- **2.30.0 → 2.31.0** – Proxy‑Authorization header leakage was fixed. No code changes required unless you embed credentials in proxy URLs.
- **2.33.0 (packaging change)** – Source layout moved to `src/requests` and the build system now uses PEP 517 with *hatchling*. Re‑install with a recent pip (≥23) to avoid legacy build‑system issues.

### Upgrade guide
1. **Upgrade to ≥ 2.31.0** to get the proxy‑authorization fix.
2. **Replace deprecated adapter calls**:
   ```python
   # Old (pre‑2.32.2)
   conn = adapter.get_connection(request, proxies)
   # New (2.32.2+)
   conn = adapter.get_connection_with_tls_context(
       request,
       verify=True,          # or a path to a CA bundle / False to disable verification
       proxies=proxies,
   )
   ```
3. **Remove custom `ssl_context` arguments** from `HTTPAdapter` subclasses if you were using them. Use `verify=` or the new TLS‑context method instead.
4. **Re‑install the package** with a recent pip version to handle the new build system.

## Configuration

- **Timeouts**: No default timeout; always pass `timeout=` (float seconds or `(connect, read)` tuple) to avoid hanging indefinitely.
- **Redirects**: Followed by default for GET/OPTIONS; can control with `allow_redirects=` (e.g., `requests.get(..., allow_redirects=False)`).
- **TLS verification**: Verified by default (`verify=True`). Override per request with `verify=False` (discouraged) or `verify="/path/to/ca-bundle.pem"`.
- **Proxies**: Configure via `proxies=` dict or environment variables (`HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY`) when `Session.trust_env` is `True`.
- **Environment / netrc**: By default, Requests may consult environment and `.netrc` for credentials when `auth=` is not provided. Disable by setting `Session.trust_env = False`.
- **Response encoding**: Requests infers encoding from headers; set `Response.encoding` manually only when you know the correct encoding before accessing `Response.text`.
- **Python version**: Requests 2.33.0 requires **Python ≥ 3.9**.

## Pitfalls
### Wrong: assuming JSON decoding implies HTTP success
```python
from __future__ import annotations

import requests

r = requests.get("https://httpbin.org/status/500", timeout=10)
data = r.json()  # may succeed/fail independently of HTTP status
print(data)
```

### Right: check HTTP status first
```python
from __future__ import annotations

import requests

r = requests.get("https://httpbin.org/status/500", timeout=10)
r.raise_for_status()
print(r.json())
```

### Wrong: calling `Response.raw` without `stream=True`
```python
from __future__ import annotations

import requests

r = requests.get("https://httpbin.org/bytes/10", timeout=10)
raw_bytes = r.raw.read()  # not intended without stream=True
print(raw_bytes)
```

### Right: `stream=True` and `iter_content()`
```python
from __future__ import annotations

import requests

with requests.get("https://httpbin.org/bytes/10", stream=True, timeout=10) as r:
    r.raise_for_status()
    data = b"".join(r.iter_content(chunk_size=4))
    print(data)
```

### Wrong: unexpected credential sending via netrc/env when `auth=` is not set
```python
from __future__ import annotations

import requests

s = requests.Session()
# May consult environment (.netrc, proxy env vars, etc.) by default:
r = s.get("https://example.com/private", timeout=10)
print(r.status_code)
```

### Right: disable environment-based behavior when you must control auth explicitly
```python
from __future__ import annotations

import requests

s = requests.Session()
s.trust_env = False
r = s.get("https://example.com/private", timeout=10)
print(r.status_code)
```

## References
- [Documentation](https://requests.readthedocs.io)
- [Source](https://github.com/psf/requests)

### Migration from v2.31.x
- **Python support**: Requests 2.33.0 requires **Python ≥ 3.9**. If you must run on older versions, pin the library to `<2.33.0` temporarily and upgrade Python as soon as possible.
- **Custom HTTPAdapter changes (2.32.2–2.32.3)**: Use `get_connection_with_tls_context`. Example:
```python
class MyAdapter(requests.adapters.HTTPAdapter):
    def get_connection_with_tls_context(self, request, verify, proxies=None, cert=None):
        conn = super().get_connection_with_tls_context(request, verify, proxies=proxies, cert=cert)
        # apply custom SSLContext here if needed
        return conn
```
- **verify handling**: Do not set `Session.verify = False`; pass `verify=False` per request or use a dedicated insecure `Session`.

### API Reference
- **requests.get(url, params=None, **kwargs)** – Send a GET request; common kwargs: `headers`, `timeout`, `auth`, `cookies`, `allow_redirects`, `proxies`, `verify`, `cert`, `stream`.
- **requests.post(url, data=None, json=None, **kwargs)** – Send a POST request; prefer `json=` for JSON bodies.
- **requests.put(url, data=None, **kwargs)** – Send a PUT request.
- **requests.patch(url, data=None, **kwargs)** – Send a PATCH request.
- **requests.delete(url, **kwargs)** – Send a DELETE request.
- **requests.head(url, **kwargs)** – Send a HEAD request (often with `allow_redirects=`).
- **requests.options(url, **kwargs)** – Send an OPTIONS request.
- **requests.request(method, url, **kwargs)** – Generic request entry point for custom/variable methods.
- **requests.Session()** – Persistent session with connection pooling; configure `headers`, `cookies`, `proxies`, `verify`, `trust_env`.
- **requests.session()** – Convenience constructor returning a `requests.Session`.
- **requests.Response** – Response object; key attributes/methods: `.status_code`, `.headers`, `.url`, `.text`, `.content`, `.encoding`, `.json()`, `.raise_for_status()`, `.iter_content()`, `.raw` (with `stream=True`).
- **requests.exceptions.RequestException** – Base exception for Requests errors.
- **requests.Timeout / requests.ConnectTimeout / requests.ReadTimeout** – Timeout exceptions; use `timeout=` to control.
- **requests.ConnectionError** – Network‑level failure.
- **requests.HTTPError** – Raised by `Response.raise_for_status()` on 4xx/5xx.
- **requests.TooManyRedirects** – Raised when redirect limit is exceeded.
- **requests.exceptions.JSONDecodeError** – Raised by `Response.json()` on invalid/empty JSON.
- **requests.codes** – Status code lookup (e.g., `requests.codes.ok == 200`).
- **requests.adapters.HTTPAdapter.get_connection(self, url, proxies=None)** – Deprecated; emits a `DeprecationWarning` and will be removed in a future major release.
- **requests.adapters.HTTPAdapter.get_connection_with_tls_context** – New public method for acquiring a connection with a custom TLS context. Signature: `(self, request, verify, proxies=None, cert=None)`. ⚠️ `get_connection` is deprecated.
- **requests.adapters.HTTPAdapter._get_connection** – Internal helper (generally not overridden).
- **requests.auth.AuthBase** – Base class for custom authentication schemes.
- **requests.auth.HTTPBasicAuth** – Basic authentication helper.
- **requests.auth.HTTPDigestAuth** – Digest authentication helper.
- **requests.sessions.Session.trust_env** – Controls whether the session respects environment variables and `.netrc` for credentials.
- **requests.sessions.Session.verify** – TLS verification setting for the session.

### Documented APIs
```
requests.get
requests.post
requests.put
requests.delete
requests.head
requests.options
requests.request
requests.Session
requests.Response
requests.Response.status_code
requests.Response.headers
requests.Response.url
requests.Response.text
requests.Response.content
requests.Response.encoding
requests.Response.apparent_encoding
requests.Response.json
requests.Response.raise_for_status
requests.Response.iter_content
requests.Response.raw
requests.auth.HTTPBasicAuth
requests.auth.HTTPDigestAuth
requests.auth.AuthBase
requests.adapters.HTTPAdapter
requests.adapters.HTTPAdapter.get_connection
requests.adapters.HTTPAdapter.get_connection_with_tls_context
requests.adapters.HTTPAdapter._get_connection
requests.sessions.Session.trust_env
requests.sessions.Session.verify
```