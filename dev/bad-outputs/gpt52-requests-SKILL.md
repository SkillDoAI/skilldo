---
name: requests
description: Synchronous (blocking) HTTP client for sending requests and handling responses.
version: 2.32.5
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
import requests
from requests import Session
from requests.auth import AuthBase, HTTPBasicAuth, HTTPDigestAuth
```

## Core Patterns

The right way to use the main APIs. Show 3-5 most common patterns.

### Basic GET with query params ✅ Current
```python
import requests

def fetch_users(base_url: str, page: int) -> dict:
    # Prefer params= over manual query string building
    # Use an endpoint that exists on common test servers like httpbin
    r = requests.get(f"{base_url}/get", params={"page": page})
    r.raise_for_status()  # HTTP success is separate from JSON parsing
    return r.json()
```
* Uses `requests.get(..., params=...)` and validates HTTP success via `Response.raise_for_status()`.
* **Status**: Current, stable

### POST JSON and read response ✅ Current
```python
import requests

def create_user(base_url: str, name: str) -> dict:
    # Use a known JSON-echo endpoint (e.g., httpbin /post) for examples/tests
    r = requests.post(
        f"{base_url}/post",
        json={"name": name},
        headers={"Accept": "application/json"},
    )
    r.raise_for_status()
    return r.json()
```
* Uses `requests.post(..., json=...)` to send JSON and `Response.json()` to parse.
* **Status**: Current, stable

### Session for connection reuse + disable environment trust ✅ Current
```python
import requests

def fetch_private_resource(url: str, username: str, password: str) -> str:
    s = requests.Session()
    # Disable environment-derived config (.netrc, proxies, etc.) when you need predictable behavior
    s.trust_env = False

    r = s.get(url, auth=(username, password))
    r.raise_for_status()
    return r.text
```
* Uses `requests.Session()` for persistent settings and connection reuse; controls `Session.trust_env`.
* **Status**: Current, stable

### Streaming download to file ✅ Current
```python
import requests

def download_file(url: str, filename: str) -> None:
    # stream=True is required for safe incremental reading and for using Response.raw as intended
    r = requests.get(url, stream=True)
    r.raise_for_status()

    with open(filename, "wb") as f:
        for chunk in r.iter_content(chunk_size=128):
            if chunk:  # filter out keep-alive chunks
                f.write(chunk)
```
* Uses `stream=True` and `Response.iter_content()` to avoid loading large responses into memory.
* **Status**: Current, stable

### Custom authentication via AuthBase ✅ Current
```python
import requests
from requests.auth import AuthBase

class TokenAuth(AuthBase):
    def __init__(self, token: str) -> None:
        self._token = token

    def __call__(self, r: requests.PreparedRequest) -> requests.PreparedRequest:
        # Mutate the outgoing request
        r.headers["Authorization"] = f"Bearer {self._token}"
        return r

def get_with_token(url: str, token: str) -> int:
    r = requests.get(url, auth=TokenAuth(token))
    return r.status_code
```
* Subclasses `requests.auth.AuthBase` and implements `__call__` to attach auth to requests.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Default values
  - TLS verification is enabled by default (`verify=True` unless explicitly set otherwise).
  - Requests is synchronous/blocking (no `async` / `await`).
- Common customizations
  - Authentication:
    - Basic: `auth=("user", "pass")` or `requests.auth.HTTPBasicAuth("user","pass")`
    - Digest: `requests.auth.HTTPDigestAuth("user","pass")`
  - Response decoding:
    - `Response.text` uses guessed encoding; override with `Response.encoding = "..."` if you know better.
  - Sessions:
    - Use `requests.Session()` for connection reuse and shared settings.
    - Control environment-derived behavior with `Session.trust_env` (set to `False` to disable).
- Environment variables
  - Requests may consult environment-derived configuration when `Session.trust_env` is `True` (default). If this causes surprises (e.g., `.netrc`), set `trust_env=False` on a `Session`.

## Pitfalls

### Wrong: Assume JSON parse means request succeeded
```python
import requests

def load_data(url: str) -> dict:
    r = requests.get(url)
    return r.json()  # may parse even when HTTP status is 4xx/5xx
```

### Right: Check HTTP success separately (raise_for_status)
```python
import requests

def load_data(url: str) -> dict:
    r = requests.get(url)
    r.raise_for_status()
    return r.json()
```

### Wrong: Manually build query strings
```python
import requests

def search(base_url: str) -> str:
    url = f"{base_url}/search?q=a+b&lang=en"  # error-prone encoding/escaping
    r = requests.get(url)
    return r.text
```

### Right: Use params= for correct encoding
```python
import requests

def search(base_url: str) -> str:
    r = requests.get(f"{base_url}/search", params={"q": "a+b", "lang": "en"})
    r.raise_for_status()
    return r.text
```

### Wrong: Read Response.raw without stream=True
```python
import requests

def peek(url: str) -> bytes:
    r = requests.get(url)
    return r.raw.read(1024)  # raw is intended for streaming usage
```

### Right: Stream and use iter_content (or raw with stream=True)
```python
import requests

def peek(url: str) -> bytes:
    r = requests.get(url, stream=True)
    r.raise_for_status()
    for chunk in r.iter_content(chunk_size=1024):
        return chunk
    return b""
```

### Wrong: Unexpected credentials/config from environment (.netrc, proxies)
```python
import requests

def fetch(url: str) -> int:
    # May consult environment-derived sources if auth isn't provided
    r = requests.get(url)
    return r.status_code
```

### Right: Use a Session and disable trust_env when needed
```python
import requests

def fetch(url: str) -> int:
    s = requests.Session()
    s.trust_env = False
    r = s.get(url)
    return r.status_code
```

### Wrong: Assume Response.text decoding is always correct
```python
import requests

def get_page(url: str) -> str:
    r = requests.get(url)
    return r.text  # encoding guess may be wrong for some HTML/XML
```

### Right: Override Response.encoding when you know the correct encoding
```python
import requests

def get_page(url: str) -> str:
    r = requests.get(url)
    # If you determine encoding from bytes (custom logic), set it explicitly
    r.encoding = "ISO-8859-1"
    return r.text
```

## References

CRITICAL: Include ALL provided URLs below (do NOT skip this section):

- [Documentation](https://requests.readthedocs.io)
- [Source](https://github.com/psf/requests)

## Migration from v2.32.4

What changed in this version (if applicable):
- Breaking changes / behavior changes
  - **SSLContext caching reverted**: 2.32.5 reverts an SSLContext caching feature introduced in 2.32.0 because it caused issues. Re-test performance/behavior if you noticed differences in 2.32.0–2.32.4; do not depend on caching behavior.
  - **Python support**: 2.32.5 drops Python 3.8 support (and adds Python 3.14). Run on Python 3.9+ (project README may state 3.10+ as officially supported).
- Deprecated → Current mapping
  - No user-facing deprecations in the provided API list.
- Before/after code examples
  - No code changes required specifically for 2.32.5 based on provided notes; verify runtime and re-test TLS/performance-sensitive paths.

Additional notes from 2.32.x line (relevant when upgrading from older versions):
- **TLS verification carry-over fixed (2.32.0)**: do not rely on a prior `verify=False` request affecting later requests; pass `verify=` explicitly per request if needed.
- **Character detection optionality (2.32.0)**: if `chardet`/`charset_normalizer` aren’t present, `Response.text` may default to UTF-8; set `Response.encoding` explicitly if required.
- **.netrc/trust_env security fix (2.32.4)**: consider `Session.trust_env=False` in high-risk environments.

## API Reference

Brief reference of the most important public APIs:

- **requests.get(url, params=..., headers=..., auth=..., stream=...)** - Send an HTTP GET request.
- **requests.post(url, json=..., data=..., headers=..., auth=..., stream=...)** - Send an HTTP POST request.
- **requests.put(url, json=..., data=..., headers=..., auth=..., stream=...)** - Send an HTTP PUT request.
- **requests.delete(url, headers=..., auth=..., stream=...)** - Send an HTTP DELETE request.
- **requests.head(url, headers=..., auth=...)** - Send an HTTP HEAD request (no response body expected).
- **requests.options(url, headers=..., auth=...)** - Send an HTTP OPTIONS request.
- **requests.Session()** - Create a session for connection reuse and shared configuration.
- **Session.get(url, ...)** - Session-bound GET (same parameters as top-level helpers).
- **Session.trust_env** - Boolean controlling whether environment-derived configuration (e.g., `.netrc`) is used.
- **requests.Response** - Response object returned by requests.
- **Response.status_code** - Integer HTTP status code.
- **Response.headers** - Mapping of response headers.
- **Response.encoding** - Text encoding used for `Response.text` decoding (can be overridden).
- **Response.text** - Response body decoded to `str` using `Response.encoding`.
- **Response.content** - Response body as `bytes`.
- **Response.json()** - Parse response body as JSON (does not imply HTTP success).
- **Response.raise_for_status()** - Raise an exception on 4xx/5xx responses.
- **Response.raw** - Underlying urllib3 response; intended to be used with `stream=True`.
- **Response.iter_content(chunk_size=...)** - Iterate response body in chunks (useful for streaming downloads).
- **requests.auth.AuthBase** - Base class for custom authentication; implement `__call__(self, r)`.
- **requests.auth.HTTPBasicAuth(user, pass)** - Basic Auth helper.
- **requests.auth.HTTPDigestAuth(user, pass)** - Digest Auth helper.