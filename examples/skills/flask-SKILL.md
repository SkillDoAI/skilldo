---

name: flask
description: A WSGI web framework for building HTTP applications with routing, request/response handling, templates, and modular blueprints.
version: 3.2.0
ecosystem: python
license: BSD-3-Clause
generated_with: gpt-5.2
---

## Imports

```python
import flask
from flask import (
    Flask,
    Blueprint,
    request,
    render_template,
    jsonify,
    redirect,
    url_for,
    abort,
    send_file,
    stream_with_context,
)
from flask import AppContext, RequestContext, Request
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
def echo() -> tuple[dict[str, Any], int]:
    data = request.get_json(silent=True) or {}
    if "message" not in data:
        abort(400, description="Missing 'message'")
    return jsonify(received=data["message"]), 200


@app.post("/submit")
def submit() -> "flask.wrappers.Response":
    # Flask 3.2: redirect() defaults to 303 unless code is provided.
    return redirect(url_for("index"))


if __name__ == "__main__":
    # Prefer: `flask --app app run --debug`
    app.run(debug=True)
```
* Use `Flask` to create an app, decorators to add routes, `request` for input, and `jsonify` / `redirect` / `abort` for common responses.
* Note: `redirect(...)` default status is 303 in Flask 3.2.0 (see Migration section).

### Blueprints for Modular Routing ✅ Current
```python
from __future__ import annotations

from flask import Blueprint, Flask, jsonify, url_for

app = Flask(__name__)

api = Blueprint("api", __name__, url_prefix="/api")


@api.get("/health")
def health() -> tuple[dict[str, str], int]:
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
* Use `url_for(".endpoint")` to reference endpoints within the same blueprint without hard-coding the blueprint name.

### Templates + Namespacing ✅ Current
```python
from __future__ import annotations

from flask import Blueprint, Flask, render_template

app = Flask(__name__)

admin = Blueprint("admin", __name__, url_prefix="/admin", template_folder="templates")


@admin.get("/")
def dashboard() -> str:
    # Recommended: templates/admin/index.html referenced as "admin/index.html"
    return render_template("admin/index.html")


app.register_blueprint(admin)

if __name__ == "__main__":
    app.run(debug=True)
```
* Use `render_template` for Jinja templates.
* Namespace blueprint templates (e.g., `templates/admin/...`) to avoid collisions with app templates or other blueprints.

### Streaming Responses with Request Context ✅ Current
```python
from __future__ import annotations

from collections.abc import Iterator

from flask import Flask, stream_with_context

app = Flask(__name__)


@app.get("/stream")
def stream() -> Iterator[bytes]:
    @stream_with_context
    def generate() -> Iterator[bytes]:
        yield b"line 1\n"
        yield b"line 2\n"

    return generate()


if __name__ == "__main__":
    app.run(debug=True)
```
* Use `stream_with_context` so generators can access request context during streaming.

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
    with app.open_resource("pyproject.toml", mode="r") as f:
        return f.readline().strip()


@bp.get("/bp-resource")
def bp_resource() -> str:
    # Reads relative to the blueprint's root_path.
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
* Use `send_file` to return a file response (downloads, static artifacts, etc.).

## Configuration

- Prefer running the dev server via CLI:
  - `flask --app app run --debug`
- If calling `Flask.run()` in code, guard with:
  - `if __name__ == "__main__": app.run(debug=True)`
- Common security / deployment-related configuration (Flask 3.1+ conventions):
  - `SECRET_KEY`: primary secret.
  - `SECRET_KEY_FALLBACKS`: rotate secrets (extensions must support it).
  - `SESSION_COOKIE_PARTITIONED`: enable CHIPS cookie attribute when needed.
  - `TRUSTED_HOSTS`: enable host validation; accessible via `Request.trusted_hosts`.
- Request limits:
  - `Request.max_content_length`: can be used to enforce max request size (often configured via `MAX_CONTENT_LENGTH`).
- `.env` support:
  - `flask.load_dotenv()` can load environment variables from a dotenv file (commonly used by the CLI workflow).

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

### Wrong: Using the built-in dev server in production
```python
from flask import Flask

app = Flask(__name__)

# Wrong: the built-in server is not for production.
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

### Right: Register 404/405 handlers at the app level (optionally scope by path)
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

## References

- [Donate](https://palletsprojects.com/donate)
- [Documentation](https://flask.palletsprojects.com/)
- [Changes](https://flask.palletsprojects.com/page/changes/)
- [Source](https://github.com/pallets/flask/)
- [Chat](https://discord.gg/pallets)

## Migration from v3.1.x

- Python support:
  - 🗑️ Removed: Python 3.9 support dropped in 3.2.0.
  - Action: upgrade runtime to Python 3.10+.
- Version attribute:
  - 🗑️ Removed: `flask.__version__` removed in 3.2.0.
  - Use package metadata instead:
    ```python
    from importlib.metadata import version

    print(version("flask"))
    ```
- Context changes:
  - ⚠️ Soft Deprecation: `RequestContext` merged with `AppContext`; `RequestContext` is a deprecated alias.
    - Deprecated since: 3.2.0
    - Still works: True (alias during deprecation period)
    - Modern alternative: `flask.AppContext`
    - Migration guidance: prefer app-context patterns; avoid relying on distinct `RequestContext` behavior or implicit reuse of an already-pushed app context during request dispatch.
- Subclassing / dispatch internals:
  - ⚠️ Soft Deprecation: older override signatures for dispatch-related `Flask` methods.
    - Deprecated since: 3.2.0
    - Still works: True (warns when old signature detected)
    - Modern alternative: update overridden methods to accept the current `AppContext` as the first parameter (`app_ctx`).
    - Migration guidance: update method signatures and use `app_ctx` rather than proxy objects.
- Teardown behavior:
  - ⚠️ Soft Deprecation: `should_ignore_error` is deprecated.
    - Deprecated since: 3.2.0
    - Still works: True (during deprecation period)
    - Modern alternative: handle conditional teardown error logic directly in teardown handlers.
- Redirect status code:
  - ✅ Current: `flask.redirect(...)` now defaults to HTTP 303 (instead of 302).
  - Migration guidance:
    ```python
    from flask import redirect

    resp = redirect("/next")
    assert resp.status_code == 303

    resp_legacy = redirect("/next", code=302)
    assert resp_legacy.status_code == 302
    ```

## API Reference

- **Flask(import_name)** - Create the WSGI application; `import_name` usually `__name__`.
- **Blueprint(name, import_name, url_prefix=..., template_folder=..., static_folder=...)** - Modular group of routes, templates, and static files.
- **Flask.run(host=None, port=None, debug=None, ...)** - Run the built-in dev server (development only; prefer `flask run`).
- **Flask.register_blueprint(blueprint, url_prefix=None, ...)** - Attach blueprint routes and handlers to the app.
- **Flask.open_resource(resource, mode="rb")** - Open a resource file relative to the app’s `root_path`.
- **Flask.open_instance_resource(resource, mode="rb")** - Open a resource file relative to the instance folder.
- **Blueprint.open_resource(resource, mode="rb")** - Open a resource file relative to the blueprint’s `root_path`.
- **Blueprint.route(rule, **options)** - Decorator to register a view function on a blueprint (use `.get`, `.post` variants when available).
- **Blueprint.register_blueprint(child, url_prefix=None, ...)** - Nest a blueprint under another blueprint.
- **Blueprint.errorhandler(code_or_exception)** - Register an error handler scoped to errors raised from within that blueprint’s views.
- **Blueprint.root_path** - Filesystem path used as base for blueprint resources.
- **render_template(template_name, **context)** - Render a Jinja template to a response body.
- **abort(code, description=None)** - Raise an HTTP exception (e.g., 400, 404, 403).
- **url_for(endpoint, **values)** - Build a URL for an endpoint; use `url_for(".name")` within a blueprint.
- **redirect(location, code=303)** - Return a redirect response (default 303 in Flask 3.2.0).
- **jsonify(*args, **kwargs)** - Create a JSON response with correct mimetype.
- **request** - Request proxy for accessing headers, args, JSON, form data, etc.
- **stream_with_context(generator_or_function)** - Ensure a generator runs with the current request context.
- **send_file(path_or_file, as_attachment=False, download_name=None, ...)** - Send a file-like object or path as a response.
- **load_dotenv(path=None, **kwargs)** - Load environment variables from a dotenv file (commonly used with CLI).
- **AppContext** - Application context object (preferred over `RequestContext` going forward).
- **RequestContext** - ⚠️ Soft Deprecation: deprecated alias of `AppContext` in 3.2.0.
- **Request** - Request class; includes attributes like `max_content_length` and `trusted_hosts`.
- **Request.max_content_length** - Maximum request body size for the current request (if configured).
- **Request.trusted_hosts** - Host validation configuration for the current request.
- **flask.cli_runner.invoke(args, **kwargs)** - Invoke Flask CLI commands in tests (via the CLI runner).