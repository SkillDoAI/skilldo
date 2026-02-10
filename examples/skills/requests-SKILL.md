---

name: requests
description: Synchronous HTTP client for sending HTTP requests and handling responses.
version: 2.32.5
ecosystem: python
license: MIT
generated_with: gpt-5.2
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

import requests


def create_widget(api_base: str, token: str, name: str) -> dict:
    url = f"{api_base}/widgets"
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
    # Example only; requires a real API server.
    print("Define api_base/token to run against a real service.")
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
    r1 = requests.get("https://httpbin.org/basic-auth/user/pass", auth=HTTPBasicAuth("user", "pass"), timeout=10)
    print(r1.status_code)

    # Digest auth
    r2 = requests.get("https://httpbin.org/digest-auth/auth/user/pass", auth=HTTPDigestAuth("user", "pass"), timeout=10)
    print(r2.status_code)

    # Custom auth
    r3 = requests.get("https://httpbin.org/bearer", auth=BearerAuth("secret-token"), timeout=10)
    print(r3.status_code)


if __name__ == "__main__":
    demo_auth()
```
* Use `auth=("user","pass")` or `HTTPBasicAuth` for Basic; `HTTPDigestAuth` for Digest; subclass `AuthBase` for custom schemes.

## Configuration

- **Timeouts**: No default timeout; always pass `timeout=` (float seconds or `(connect, read)` tuple) to avoid hanging indefinitely.
- **Redirects**: Followed by default for GET/OPTIONS; can control with `allow_redirects=` (e.g., `requests.get(..., allow_redirects=False)`).
- **TLS verification**: Verified by default (`verify=True`). Override per request with `verify=False` (discouraged) or `verify="/path/to/ca-bundle.pem"`.
- **Proxies**: Configure via `proxies=` dict or environment variables (e.g., `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY`) when `Session.trust_env` is `True`.
- **Environment / netrc**: By default, Requests may consult environment and `.netrc` for credentials when `auth=` is not provided. Disable by setting `Session.trust_env = False`.
- **Response encoding**: Requests infers encoding from headers; set `Response.encoding` manually only when you know the correct encoding before accessing `Response.text`.

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

### Wrong: calling Response.json() on empty/invalid JSON (e.g., 204)
```python
from __future__ import annotations

import requests

r = requests.get("https://httpbin.org/status/204", timeout=10)
print(r.json())  # raises requests.exceptions.JSONDecodeError
```

### Right: handle 204 and JSONDecodeError explicitly
```python
from __future__ import annotations

import requests
from requests.exceptions import JSONDecodeError

r = requests.get("https://httpbin.org/status/204", timeout=10)

if r.status_code == 204:
    print(None)
else:
    try:
        print(r.json())
    except JSONDecodeError:
        print(None)
```

### Wrong: using Response.raw without stream=True
```python
from __future__ import annotations

import requests

r = requests.get("https://httpbin.org/bytes/10", timeout=10)
raw_bytes = r.raw.read()  # not the intended pattern without stream=True
print(raw_bytes)
```

### Right: stream=True and iter_content()
```python
from __future__ import annotations

import requests

with requests.get("https://httpbin.org/bytes/10", stream=True, timeout=10) as r:
    r.raise_for_status()
    data = b"".join(r.iter_content(chunk_size=4))
    print(data)
```

### Wrong: unexpected credential sending via netrc/env when auth= is not set
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

## Migration from v2.31.x

- **Python support**: Requests 2.32.5 drops Python 3.8 support. If you must run on 3.8, pin `<2.32.5` temporarily and upgrade Python.
- **TLS/adapter changes (2.32.0–2.32.2)**: If you maintain a custom `HTTPAdapter`, Requests introduced a new public method for connection acquisition with TLS context (`get_connection_with_tls_context`) and considers `get_connection` deprecated in Requests >=2.32.0. Update adapter overrides accordingly.
- **TLS behavior (2.32.5)**: SSLContext caching introduced in 2.32.0 was reverted in 2.32.5; retest performance and any assumptions about TLS context reuse.
- **Security (2.31.0)**: If you use proxy URLs with credentials, ensure you are on `>=2.31.0` to avoid potential `Proxy-Authorization` leakage on HTTPS redirects; rotate proxy credentials after upgrading.

## API Reference

- **requests.get(url, params=None, \*\*kwargs)** - Send a GET request; common kwargs: `headers`, `timeout`, `auth`, `cookies`, `allow_redirects`, `proxies`, `verify`, `cert`, `stream`.
- **requests.post(url, data=None, json=None, \*\*kwargs)** - Send a POST request; prefer `json=` for JSON bodies.
- **requests.put(url, data=None, \*\*kwargs)** - Send a PUT request.
- **requests.patch(url, data=None, \*\*kwargs)** - Send a PATCH request.
- **requests.delete(url, \*\*kwargs)** - Send a DELETE request.
- **requests.head(url, \*\*kwargs)** - Send a HEAD request (often with `allow_redirects=`).
- **requests.options(url, \*\*kwargs)** - Send an OPTIONS request.
- **requests.request(method, url, \*\*kwargs)** - Generic request entry point for custom/variable methods.
- **requests.Session()** - Persistent session with connection pooling; use `.get()/.post()` etc.; configure `headers`, `cookies`, `proxies`, `verify`, `trust_env`.
- **requests.session()** - Convenience constructor returning a `requests.Session`.
- **requests.Response** - Response object; key attributes/methods: `.status_code`, `.headers`, `.url`, `.text`, `.content`, `.encoding`, `.json()`, `.raise_for_status()`, `.iter_content()`, `.raw` (with `stream=True`).
- **requests.RequestException** - Base exception for Requests errors.
- **requests.Timeout / requests.ConnectTimeout / requests.ReadTimeout** - Timeout exceptions; use `timeout=` to control.
- **requests.ConnectionError** - Network-level failure.
- **requests.HTTPError** - Raised by `Response.raise_for_status()` on 4xx/5xx.
- **requests.TooManyRedirects** - Raised when redirect limit is exceeded.
- **requests.exceptions.JSONDecodeError** - Raised by `Response.json()` on invalid/empty JSON.
- **requests.codes** - Status code lookup (e.g., `requests.codes.ok`).