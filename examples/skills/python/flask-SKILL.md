---
name: flask
description: A lightweight WSGI web application framework for Python that provides routing, templating, and request handling.
license: BSD-3-Clause
metadata:
  version: "3.1.3"
  ecosystem: python
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```python
from flask import (
    Flask,
    Blueprint,
    request,
    render_template,
    render_template_string,
    jsonify,
    redirect,
    url_for,
    abort,
    send_file,
    send_from_directory,
    stream_with_context,
    flash,
    get_flashed_messages,
    session,
    g,
    current_app,
    make_response,
    after_this_request,
    copy_current_request_context,
    has_app_context,
    has_request_context,
    get_template_attribute,
)
from flask import Request, Response, Config
import flask.json
```

## Core Patterns

### Application + Routes + Responses ✅ Current
```python
from __future__ import annotations

from typing import Any

from flask import Flask, abort, jsonify, redirect, request, url_for

app = Flask(__name__)

@app.get("/")
def index() -> str:
    return "OK"


@app.post("/echo")
def echo() -> tuple[Any, int]:
    data = request.get_json(silent=True) or {}
    if "message" not in data:
        abort(400, description="Missing 'message'")
    return jsonify(received=data["message"]), 200


@app.post("/submit")
def submit():
    # Use a relative URL to keep the Location header simple.
    return redirect(url_for("index", _external=False))


if __name__ == "__main__":
    client = app.test_client()

    # Test GET /
    r1 = client.get("/")
    assert r1.status_code == 200
    assert r1.get_data(as_text=True) == "OK"

    # Test POST /echo with valid payload
    r2 = client.post("/echo", json={"message": "hello"})
    assert r2.status_code == 200
    assert r2.get_json() == {"received": "hello"}

    # Test POST /echo with missing message
    r3 = client.post("/echo", json={})
    assert r3.status_code == 400
    assert "Bad Request" in r3.get_data(as_text=True)

    # Test POST /submit redirects to index
    r4 = client.post("/submit", follow_redirects=False)
    assert r4.status_code == 302
    with app.app_context():
        expected = url_for("index", _external=False)
    assert r4.headers["Location"] == expected

    print("✓ Test passed: Application + Routes + Responses ✅ Current")
```
* Use `Flask` to create an app, decorators to add routes, `request` for input, and `jsonify` / `redirect` / `abort` for common responses.
* Note: `redirect(...)` default status is 302 in Flask 3.1.3.
* Note: `url_for()` requires an active application context; outside of a request, use `with app.app_context():`.

### Blueprints for Modular Routing ✅ Current
```python
from __future__ import annotations

from flask import Blueprint, Flask, jsonify, url_for

app = Flask(__name__)

api = Blueprint("api", __name__, url_prefix="/api")


@api.get("/health")
def health() -> tuple["flask.wrappers.Response", int]:
    return jsonify(status="ok"), 200


@api.get("/links")
def links() -> dict[str, str]:
    # Prefer blueprint-relative endpoint references within the same blueprint.
    return {"health": url_for(".health", _external=False)}


app.register_blueprint(api)

if __name__ == "__main__":
    app.run(debug=True)
```
* Use `Blueprint` to group routes and register them with `Flask.register_blueprint`.
* Use `url_for(".endpoint")` to reference endpoints within the same blueprint without hard‑coding the blueprint name.

### Templates + Namespacing ✅ Current
```python
from __future__ import annotations

from flask import Blueprint, Flask, render_template

app = Flask(__name__)

# Blueprint templates are resolved relative to the app's template folder by default
admin = Blueprint("admin", __name__, url_prefix="/admin")


@admin.get("/")
def dashboard() -> str:
    # Recommended: organize templates in app's templates/ folder as admin/index.html
    # This avoids collisions with other templates
    return render_template("admin/index.html")


app.register_blueprint(admin)

if __name__ == "__main__":
    app.run(debug=True)
```
* Use `render_template` for Jinja templates.
* Namespace blueprint templates (e.g., `templates/admin/...`) to avoid collisions with app templates or other blueprints.
* By default, blueprints share the app's template folder; organize files in subdirectories for clarity.

### Streaming Responses with Request Context ✅ Current
```python
from __future__ import annotations

from collections.abc import Iterator

from flask import Flask, stream_with_context

app = Flask(__name__)


@app.get("/stream")
def stream() -> "flask.wrappers.Response":
    @stream_with_context
    def generate() -> Iterator[bytes]:
        yield b"line 1\n"
        yield b"line 2\n"

    return app.response_class(generate())


if __name__ == "__main__":
    app.run(debug=True)
```
* Use `stream_with_context` so generators can access request context during streaming.
* As of Flask 3.1.2+, `stream_with_context` works correctly inside async views.

### Async Streaming Responses ✅ New
```python
from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator

from flask import Flask, stream_with_context

app = Flask(__name__)


@app.get("/async-stream")
async def async_stream() -> "flask.wrappers.Response":
    @stream_with_context
    async def generate() -> AsyncIterator[bytes]:
        for i in range(3):
            await asyncio.sleep(0.1)
            yield f"chunk {i}\n".encode()

    return app.response_class(generate())


if __name__ == "__main__":
    app.run(debug=True)
```
* `stream_with_context` also works with async generators (Flask 3.1.2+).  
* Define the view as `async def` and return a response built from the async generator.

### Files and Resources (app / blueprint) ✅ Current
```python
from __future__ import annotations

from pathlib import Path

from flask import Blueprint, Flask, send_file

app = Flask(__name__)
bp = Blueprint("assets", __name__, url_prefix="/assets")


@app.get("/app-resource")
def app_resource() -> str:
    # Reads from the application's root_path (package/module location).
    # open_resource accepts an 'encoding' parameter in text mode (defaults to 'utf-8').
    with app.open_resource("pyproject.toml", mode="r") as f:
        return f.readline().strip()


@bp.get("/bp-resource")
def bp_resource() -> str:
    # Reads relative to the blueprint's root_path.
    # Blueprint.open_resource defaults to UTF‑8 encoding for text mode.
    with bp.open_resource("README.md", mode="r") as f:
        return f.readline().strip()


@app.get("/download")
def download() -> "flask.wrappers.Response":
    path = Path(__file__).resolve()
    return send_file(path, as_attachment=True, download_name=path.name)


app.register_blueprint(bp)

if __name__ == "__main__":
    app.run(debug=True)
```
* Use `Flask.open_resource` / `Blueprint.open_resource` to read packaged resources.  
* Both accept an `encoding` parameter when opening in text mode (defaults to `'utf-8'` for Blueprint).  
* Use `send_file` to return a file response (downloads, static artifacts, etc.).

### Flash Messages ✅ Current
```python
from __future__ import annotations

import os
from flask import Flask, flash, get_flashed_messages, redirect, render_template, request, url_for

app = Flask(__name__)
# WARNING: Use a strong random secret key in production; never hardcode secrets.
# Retrieve the secret from an environment variable or a secure vault.
app.secret_key = os.getenv("FLASK_SECRET_KEY", "replace-with-strong-secret")


@app.post("/submit")
def submit() -> "flask.wrappers.Response":
    # Flash a message to be displayed on the next request
    flash("Form submitted successfully!")
    flash("Warning: check your input", "warning")
    return redirect(url_for("index"))


@app.get("/")
def index() -> str:
    # Retrieve flashed messages with categories
    messages = get_flashed_messages(with_categories=True)
    return render_template("index.html", messages=messages)


if __name__ == "__main__":
    app.run(debug=True)
```
* Use `flash()` to store messages in the session for display on the next request.
* Use `get_flashed_messages(with_categories=True)` to retrieve messages with their categories.

### Session Management ✅ Current
```python
from __future__ import annotations

import os
from flask import Flask, session, redirect, url_for

app = Flask(__name__)
# WARNING: Use a strong random secret key in production; never hardcode secrets.
# Retrieve the secret from an environment variable or a secure vault.
app.secret_key = os.getenv("FLASK_SECRET_KEY", "replace-with-strong-secret")


@app.post("/login")
def login() -> "flask.wrappers.Response":
    session["user_id"] = 123
    session["username"] = "alice"
    # Make session persist beyond browser closure
    session.permanent = True
    return redirect(url_for("index"))


@app.get("/profile")
def profile() -> tuple[str, int] | str:
    user_id = session.get("user_id")
    if user_id is None:
        return "Not logged in", 401
    return f"User ID: {user_id}"


@app.post("/logout")
def logout() -> "flask.wrappers.Response":
    session.clear()
    return redirect(url_for("index"))


if __name__ == "__main__":
    app.run(debug=True)
```
* Use `session` to store user data across requests (stored in signed cookies by default).
* Set `session.permanent = True` to make the session persist beyond browser closure.
* In Flask 3.1.3+, key‑only access operations like `in` and `len` correctly mark the session as accessed and set the `Vary: Cookie` header (security fix GHSA-68rp-wp8r-4726).

### Secret Key Rotation ✅ Current
```python
from __future__ import annotations

import os
from flask import Flask, session

app = Flask(__name__)

# WARNING: Use strong random secret keys in production; never hardcode secrets.
# Primary signing key (read from env or secure source)
app.secret_key = os.getenv("FLASK_SECRET_KEY", "replace-with-primary-key")

# Old keys that can still be used to verify existing sessions
app.config["SECRET_KEY_FALLBACKS"] = ["old-key", "older-key"]


@app.post("/")
def set_session() -> str:
    session["user"] = "alice"
    return ""


@app.get("/")
def get_session() -> dict:
    return dict(session)


if __name__ == "__main__":
    app.run(debug=True)
```
* Use `SECRET_KEY_FALLBACKS` as a list of old keys to support session key rotation.
* Sessions signed with a fallback key are still readable; new sessions are signed with `SECRET_KEY`.
* Flask 3.1.1 fixed a security issue (GHSA-4grg-w6v8-c28g) in signing key selection order — upgrade if using this feature.

## Configuration

- Prefer running the dev server via CLI:
  - `flask --app app run --debug`
- If calling `Flask.run()` in code, guard with:
  - `if __name__ == "__main__": app.run(debug=True)`
- Common security / deployment‑related configuration (Flask 3.1+ conventions):
  - `SECRET_KEY`: primary secret for session signing.
  - `SECRET_KEY_FALLBACKS`: rotate secrets (extensions must support it).
  - `SESSION_COOKIE_PARTITIONED`: enable CHIPS cookie attribute when needed.
  - `TRUSTED_HOSTS`: enable host validation; accessible via `Request.trusted_hosts`.
- Request limits:
  - `MAX_CONTENT_LENGTH`: enforce max request size.
  - `MAX_FORM_MEMORY_SIZE`: limit form data memory usage.
- `.env` support:
  - `flask.cli.load_dotenv()` can load environment variables from a dotenv file (commonly used by the CLI workflow).
  - The `-e path` CLI option takes precedence over default `.env` and `.flaskenv` files.

## Pitfalls

### Wrong: Running the dev server at import time (breaks CLI discovery / production imports)
```python
from flask import Flask

app = Flask(__name__)

# Wrong: executed on import.
app.run(debug=True)
```

### Right: Guard `run()` with a main block (or prefer `flask run`)
```python
from flask import Flask

app = Flask(__name__)

if __name__ == "__main__":
    app.run(debug=True)
```

### Wrong: Using the built‑in dev server in production
```python
from flask import Flask

app = Flask(__name__)

# Wrong: the built‑in server is not for production.
app.run(host="0.0.0.0", port=80)
```

### Right: Use the CLI for development; use a production WSGI server for deployment
```python
from flask import Flask

app = Flask(__name__)

# Development:
#   $ flask --app app run --debug
# Production:
#   Run behind a production WSGI server (see Flask deploying docs).
```

### Wrong: Assuming a blueprint 404 handler catches unknown URLs outside its routes
```python
from flask import Blueprint

bp = Blueprint("bp", __name__)

@bp.errorhandler(404)
def not_found(e):  # noqa: ANN001
    return "Not found", 404

# Wrong expectation: this will handle /does-not-exist even if it never matched bp routes.
```

### Right: Register 404/405 handlers at the app level (optional scope by path)
```python
from __future__ import annotations

from flask import Flask, jsonify, request

app = Flask(__name__)

@app.errorhandler(404)
@app.errorhandler(405)
def handle_api_errors(ex):  # noqa: ANN001
    if request.path.startswith("/api/"):
        return jsonify(error=str(ex)), ex.code
    return ex
```

### Wrong: Blueprint static files without `url_prefix` (shadowed by app `/static`)
```python
from flask import Blueprint, Flask

app = Flask(__name__)
admin = Blueprint("admin", __name__, static_folder="static")

app.register_blueprint(admin)

# Wrong expectation: /static/... will serve admin's static files.
```

### Right: Give the blueprint a `url_prefix` so its static route is reachable
```python
from flask import Blueprint, Flask

app = Flask(__name__)
admin = Blueprint("admin", __name__, static_folder="static", url_prefix="/admin")

app.register_blueprint(admin)

# Blueprint static is served under /admin/static/...
```

### Wrong: Template path collisions between app and blueprint templates
```python
from flask import Blueprint, Flask, render_template

app = Flask(__name__)
admin = Blueprint("admin", __name__, url_prefix="/admin")

@admin.get("/")
def index() -> str:
    # Wrong: can resolve to app's templates/index.html instead of the blueprint's.
    return render_template("index.html")

app.register_blueprint(admin)
```

### Right: Namespace blueprint templates under a subfolder and reference it explicitly
```python
from flask import Blueprint, Flask, render_template

app = Flask(__name__)
admin = Blueprint("admin", __name__, url_prefix="/admin")

@admin.get("/")
def index() -> str:
    return render_template("admin/index.html")

app.register_blueprint(admin)
```

### Wrong: Assuming session key‑only access operations do not trigger Vary: Cookie
```python
from flask import Flask, session

app = Flask(__name__)
app.secret_key = "key"

@app.get("/")
def index() -> str:
    # In Flask < 3.1.3: 'in' did NOT mark session as accessed,
    # so Vary: Cookie header was not set — a security issue.
    if "user_id" in session:
        return "Logged in"
    return "Not logged in"
```

### Right: In Flask 3.1.3+, all session access operations correctly set Vary: Cookie
```python
from flask import Flask, session

app = Flask(__name__)
app.secret_key = "key"

@app.get("/")
def index() -> str:
    # In Flask 3.1.3+, 'in' and 'len' mark session as accessed,
    # correctly setting Vary: Cookie header (security fix GHSA-68rp-wp8r-4726).
    if "user_id" in session:
        return "Logged in"
    return "Not logged in"
```

## References

- [Donate](https://palletsprojects.com/donate)
- [Documentation](https://flask.palletsprojects.com/)
- [Changes](https://flask.palletsprojects.com/page/changes/)
- [Source](https://github.com/pallets/flask/)
- [Chat](https://discord.gg/pallets)

## Migration from v3.0.x

- Python support:
  - 🗑️ Removed: Python 3.8 support dropped in 3.1.0.
  - Action: upgrade runtime to Python 3.9+.
- Version attribute:
  - ⚠️ Hard Deprecation: `flask.__version__` deprecated in 3.1.0 (will be removed in 3.2.0).
    - Deprecated since: 3.1.0
    - Still works: True (with deprecation warning)
    - Modern alternative: `import importlib.metadata; importlib.metadata.version("flask")`
    - Migration guidance:
      ```python
      from importlib.metadata import version

      print(version("flask"))
      ```
- Dependency updates:
  - Minimum Werkzeug version: 3.0+
  - Minimum ItsDangerous version: 2.2+
  - Minimum Blinker version: 1.6+
- `SERVER_NAME` behavior change:
  - ⚠️ Breaking: `SERVER_NAME` no longer restricts requests to only that domain.
  - Migration guidance: use `TRUSTED_HOSTS` config for host validation.
- Environment file precedence:
  - ⚠️ Breaking: `-e path` CLI option now takes precedence over default `.env` and `.flaskenv` files.
  - Migration guidance: if relying on default files to override explicit `-e path`, restructure configuration. Use `load_defaults=False` with `load_dotenv()` if needed.
- Session access behavior:
  - ⚠️ Security fix (3.1.3): operations like `in` and `len` on session now mark the session as accessed and correctly set `Vary: Cookie` header (GHSA-68rp-wp8r-4726).
  - Migration guidance: no code changes needed; this is a behavioral fix.
- Secret key rotation security fix (3.1.1):
  - ⚠️ Security fix: signing key selection order corrected when using `SECRET_KEY_FALLBACKS` (GHSA-4grg-w6v8-c28g).
  - Migration guidance: upgrade immediately if using `SECRET_KEY_FALLBACKS`.
- `stream_with_context` in async views (3.1.2):
  - Fixed: `stream_with_context` now works correctly inside async views.
- Test client `follow_redirects` (3.1.2):
  - Fixed: `may ​the ​test ​client ​correctly ​reports ​the ​final ​session ​state`.

## Migration from v3.1.2 → v3.1.3

- **Session access marking** – Membership checks (`in`) and length checks (`len(session)`) now mark the session as accessed, ensuring the `Vary: Cookie` header is set.  
  *If your code relied on the previous lazy behavior, adjust any caching or response‑header logic accordingly.*

## API Reference

- **Flask(import_name)** – Create the WSGI application; `import_name` usually `__name__`.
- **Blueprint(name, import_name, …)** – Modular group of routes, templates, and static files.
- **Flask.run(host=None, port=None, debug=None, …)** – Run the built‑in dev server (development only; prefer `flask run`).
- **Flask.register_blueprint(blueprint, url_prefix=None, …)** – Attach blueprint routes and handlers to the app.
- **Flask.open_resource(resource, mode="rb", encoding=None)** – Open a resource file relative to the app's `root_path`; `encoding` applies in text mode (defaults to `None` → binary mode).
- **Flask.open_instance_resource(resource, mode="rb", encoding=None)** – Open a resource file relative to the instance folder; `encoding` applies in text mode (defaults to `'utf-8'`).
- **Blueprint.open_resource(resource, mode="r", encoding='utf-8')** – Open a resource file relative to the blueprint's `root_path`; defaults to UTF‑8 encoding for text mode.
- **Blueprint.route(rule, **options)** – Decorator to register a view function on a blueprint (use `.get`, `.post` variants when available).
- **Blueprint.register_blueprint(child, url_prefix=None, …)** – Nest a blueprint under another blueprint.
- **Blueprint.errorhandler(code_or_exception)** – Register an error handler scoped to errors raised from within that blueprint's views.
- **Blueprint.root_path** – Filesystem path used as base for blueprint resources.
- **render_template(template_name_or_list, **context)** – Render a Jinja template to a response body.
- **render_template_string(source, **context)** – Render a Jinja template from a string.
- **stream_template(template_name, **context)** – Render a Jinja template as a stream.
- **stream_template_string(source, **context)** – Render a Jinja template from a string as a stream.
- **abort(code, description=None)** – Raise an HTTP exception (e.g., 400, 404, 403).
- **url_for(endpoint, **values)** – Build a URL for an endpoint; use `url_for(".name")` within a blueprint. Requires an active application context.
- **redirect(location, code=302, Response=None)** – Return a redirect response (default 302 in Flask 3.1.3).
- **jsonify(*args, **kwargs)** – Create a JSON response with correct mimetype.
- **flask.json.dumps(obj, **kwargs)** – Serialize data as JSON string.
- **flask.json.dump(obj, fp, **kwargs)** – Serialize data as JSON and write to file.
- **flask.json.loads(s, **kwargs)** – Deserialize JSON string to Python object.
- **flask.json.load(fp, **kwargs)** – Deserialize JSON from file to Python object.
- **request** – Request proxy for accessing headers, args, JSON, form data, etc.
- **session** – Session proxy for accessing session data stored in signed cookies. In 3.1.3+, key‑only operations (`in`, `len`) correctly mark the session as accessed.
- **g** – Application context global object for storing data during a request.
- **current_app** – Proxy to the current Flask application.
- **stream_with_context(generator_or_function)** – Ensure a generator runs with the current request context. Works correctly in async views as of 3.1.2.
- **send_file(path_or_file, mimetype=None, as_attachment=False, download_name=None, conditional=True, etag=True, last_modified=None, max_age=None)** – Send a file-like object or path as a response.
- **send_from_directory(directory, path, **options)** – Send a file from a directory (safely validates path).
- **flash(message, category="message")** – Store a message in the session for display on the next request.
- **get_flashed_messages(with_categories=False, category_filter=())** – Retrieve flashed messages from the session.
- **make_response(*args)** – Convert return values to Response objects.
- **after_this_request(f)** – Register a function to run after the current request.
- **copy_current_request_context(f)** – Copy the current request context to a function.
- **has_app_context()** – Check if an application context is active.
- **has_request_context()** – Check if a request context is active.
- **flask.cli.load_dotenv(path=None, **kwargs)** – Load environment variables from a dotenv file (commonly used with CLI).
- **get_template_attribute(template_name, attribute)** – Load a macro or variable from a template.
- **Request** – Request class; includes attributes like `max_content_length` and `trusted_hosts`.
- **Request.max_content_length** – Maximum request body size for the current request (if configured); can be customized per request.
- **Request.trusted_hosts** – Host validation configuration for the current request.
- **Response** – Response class for creating custom responses.
- **Config** – Configuration dictionary subclass with `from_file`, `from_object` methods.
- **flask.__version__** – ⚠️ Deprecated in 3.1.0 (removed in 3.2.0); use `importlib.metadata.version("flask")` instead.

### Signals
- **template_rendered** – Signal sent when a template is rendered.
- **before_render_template** – Signal sent before template rendering.
- **request_started** – Signal sent when request processing starts.
- **request_finished** – Signal sent when request processing finishes.
- **request_tearing_down** – Signal sent when request context is torn down.
- **got_request_exception** – Signal sent when an exception occurs during request processing.
- **appcontext_pushed** – Signal sent when application context is pushed.
- **appcontext_popped** – Signal sent when application context is popped.
- **appcontext_tearing_down** – Signal sent when application context is torn down.
- **message_flashed** – Signal sent when a message is flashed.