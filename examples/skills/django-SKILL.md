---
name: django
description: A Python web framework for building database-backed web applications with an ORM, URL routing, views, templates, and an admin site.
version: unknown
ecosystem: python
license: BSD-3-Clause
---

## Imports

Show the standard import patterns. Most common first:
```python
from django.conf import settings
from django.urls import path, include
from django.http import HttpRequest, HttpResponse, JsonResponse
from django.shortcuts import render, get_object_or_404, redirect
from django.views import View
from django.contrib import admin
from django.db import models, transaction
from django.core.management import execute_from_command_line
```

## Core Patterns

### URL routing + function-based view ✅ Current
```python
from django.http import HttpRequest, HttpResponse
from django.urls import path

def hello(request: HttpRequest) -> HttpResponse:
    return HttpResponse("Hello, world")

urlpatterns = [
    path("hello/", hello, name="hello"),
]
```
* Define URL patterns with `django.urls.path()` and map them to callables (views).
* **Status**: Current, stable

### Class-based view (CBV) with `View` ✅ Current
```python
from django.http import HttpRequest, HttpResponse
from django.urls import path
from django.views import View

class PingView(View):
    def get(self, request: HttpRequest) -> HttpResponse:
        return HttpResponse("pong")

urlpatterns = [
    path("ping/", PingView.as_view(), name="ping"),
]
```
* Use CBVs for organizing HTTP-method handlers (`get`, `post`, etc.).
* **Status**: Current, stable

### Django ORM: model definition + CRUD ✅ Current
```python
from django.db import models

class Author(models.Model):
    name = models.CharField(max_length=100)

class Post(models.Model):
    author = models.ForeignKey(Author, on_delete=models.CASCADE, related_name="posts")
    title = models.CharField(max_length=200)
    body = models.TextField()

# Typical queries (run inside Django context, e.g., manage.py shell)
# posts = Post.objects.filter(title__icontains="django")
# post = Post.objects.get(pk=1)
# created = Post.objects.create(author=author, title="Hello", body="Text")
```
* Define models with `django.db.models.Model` and query via `Model.objects`.
* **Status**: Current, stable

### Transaction management with `transaction.atomic()` ✅ Current
```python
from django.db import transaction
from django.db import models

class Counter(models.Model):
    value = models.IntegerField(default=0)

def increment(counter: Counter) -> None:
    # Ensures all DB operations inside either commit together or roll back together.
    with transaction.atomic():
        counter.value += 1
        counter.save(update_fields=["value"])
```
* Use `transaction.atomic()` to group DB writes safely.
* **Status**: Current, stable

### Management command entrypoint (`execute_from_command_line`) ✅ Current
```python
import os
import sys
from django.core.management import execute_from_command_line

def main() -> None:
    os.environ.setdefault("DJANGO_SETTINGS_MODULE", "mysite.settings")
    execute_from_command_line(sys.argv)

if __name__ == "__main__":
    main()
```
* Standard pattern for `manage.py` to run Django commands.
* **Status**: Current, stable

## Routing Patterns

```python
from django.http import HttpRequest, HttpResponse, JsonResponse
from django.urls import path

def item_detail(request: HttpRequest, item_id: int) -> JsonResponse:
    # Path parameter is passed as a function argument.
    return JsonResponse({"item_id": item_id})

def create_item(request: HttpRequest) -> HttpResponse:
    if request.method != "POST":
        return HttpResponse(status=405)
    return HttpResponse("created", status=201)

urlpatterns = [
    path("items/<int:item_id>/", item_detail, name="item-detail"),
    path("items/", create_item, name="item-create"),
]
```

## Request Handling

```python
from django.http import HttpRequest, JsonResponse, HttpResponseBadRequest

def search(request: HttpRequest) -> JsonResponse:
    # Query params: /search/?q=abc&page=2
    q = request.GET.get("q", "")
    page = int(request.GET.get("page", "1"))
    return JsonResponse({"q": q, "page": page})

def parse_json(request: HttpRequest) -> JsonResponse:
    if request.content_type != "application/json":
        return JsonResponse({"error": "expected application/json"}, status=415)
    try:
        import json
        payload = json.loads(request.body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return HttpResponseBadRequest("invalid JSON")
    return JsonResponse({"received": payload})
```

## Response Handling

```python
from django.http import HttpRequest, HttpResponse, JsonResponse
from django.shortcuts import render

def json_ok(request: HttpRequest) -> JsonResponse:
    resp = JsonResponse({"ok": True}, status=200)
    resp["X-Request-ID"] = "example"
    return resp

def html_page(request: HttpRequest) -> HttpResponse:
    # Requires a template file on disk, e.g. templates/home.html
    return render(request, "home.html", context={"title": "Home"})
```

## Middleware/Dependencies

```python
from typing import Callable
from django.http import HttpRequest, HttpResponse

class RequestIDMiddleware:
    def __init__(self, get_response: Callable[[HttpRequest], HttpResponse]) -> None:
        self.get_response = get_response

    def __call__(self, request: HttpRequest) -> HttpResponse:
        response = self.get_response(request)
        response["X-Request-ID"] = request.headers.get("X-Request-ID", "missing")
        return response
```
* Middleware is a callable that receives `request` and returns `response`.
* Configure in `MIDDLEWARE = [...]` in settings.

## Error Handling

```python
from django.http import HttpRequest, JsonResponse
from django.http import Http404
from django.shortcuts import get_object_or_404
from django.db import models

class Widget(models.Model):
    name = models.CharField(max_length=100)

def widget_detail(request: HttpRequest, widget_id: int) -> JsonResponse:
    widget = get_object_or_404(Widget, pk=widget_id)  # raises Http404 if missing
    return JsonResponse({"id": widget.id, "name": widget.name})

def handler404(request: HttpRequest, exception: Exception) -> JsonResponse:
    return JsonResponse({"error": "not found"}, status=404)
```
* Use `get_object_or_404()` for common “missing row” handling.
* Custom error handlers are configured as module-level callables (e.g., `handler404`) in your root URLconf.

## Background Tasks

Django doesn’t include a built-in background task queue. Typical patterns:
- Use a task queue (e.g., Celery, RQ) for async/background work.
- For small deferred work, trigger via management commands + external scheduler (cron/systemd).

## WebSocket Patterns

Django core does not provide WebSocket support; common approach is Django Channels (separate project).

## Configuration

Standard configuration and setup:
- Settings are a Python module referenced by `DJANGO_SETTINGS_MODULE` (e.g., `mysite.settings`).
- Common environment variables:
  - `DJANGO_SETTINGS_MODULE`: settings module path
  - `DJANGO_DEBUG` is not a Django standard; use `DEBUG = ...` in settings (often driven by env vars).
- Typical settings you will customize:
  - `DEBUG`, `ALLOWED_HOSTS`
  - `DATABASES`
  - `INSTALLED_APPS`, `MIDDLEWARE`
  - `TEMPLATES`
  - `STATIC_URL`, `STATIC_ROOT`, `MEDIA_URL`, `MEDIA_ROOT`
- Docs workflow (Django’s own docs):
  - Docs are ReST built with Sphinx; run `make html` from `docs/` after installing Sphinx.

## Pitfalls

### Wrong: Building Django docs without Sphinx installed
```bash
cd docs
make html
```

### Right: Install Sphinx first, then build
```bash
python -m pip install Sphinx
cd docs
make html  # or: make.bat html (Windows)
```

### Wrong: Using Unix `make` on Windows for building docs
```bat
cd docs
make html
```

### Right: Use the Windows batch file entrypoint
```bat
cd docs
make.bat html
```

### Wrong: Returning a plain dict from a Django view (not a valid HttpResponse)
```python
from django.http import HttpRequest

def bad_view(request: HttpRequest):
    return {"ok": True}  # Django expects an HttpResponse subclass
```

### Right: Return `JsonResponse` (or `HttpResponse`) from views
```python
from django.http import HttpRequest, JsonResponse

def good_view(request: HttpRequest) -> JsonResponse:
    return JsonResponse({"ok": True})
```

### Wrong: Doing multiple DB writes without an atomic transaction when they must succeed/fail together
```python
from django.db import models

class Account(models.Model):
    balance = models.IntegerField(default=0)

def transfer(a: Account, b: Account, amount: int) -> None:
    a.balance -= amount
    a.save(update_fields=["balance"])
    b.balance += amount
    b.save(update_fields=["balance"])
```

### Right: Wrap multi-step writes in `transaction.atomic()`
```python
from django.db import transaction, models

class Account(models.Model):
    balance = models.IntegerField(default=0)

def transfer(a: Account, b: Account, amount: int) -> None:
    with transaction.atomic():
        a.balance -= amount
        a.save(update_fields=["balance"])
        b.balance += amount
        b.save(update_fields=["balance"])
```

## References

- [Homepage](https://www.djangoproject.com/)
- [Documentation](https://docs.djangoproject.com/)
- [Release notes](https://docs.djangoproject.com/en/stable/releases/)
- [Funding](https://www.djangoproject.com/fundraising/)
- [Source](https://github.com/django/django)
- [Tracker](https://code.djangoproject.com/)

## Migration from v[previous]

No changelog/version-specific content was provided in the supplied context, so version-to-version breaking changes and deprecations can’t be listed here. Use Django’s release notes for the specific versions you are upgrading between:
- https://docs.djangoproject.com/en/stable/releases/

## API Reference

Brief reference of the most important public APIs:

- **django.urls.path(route, view, kwargs=None, name=None)** - Map a URL pattern to a view.
- **django.urls.include(module)** - Include another URLconf.
- **django.http.HttpRequest** - Incoming request object (`GET`, `POST`, `headers`, `body`, etc.).
- **django.http.HttpResponse(content=b"", status=200, content_type=None, ...)** - Base HTTP response type.
- **django.http.JsonResponse(data, encoder=DjangoJSONEncoder, safe=True, json_dumps_params=None, ...)** - JSON response helper.
- **django.shortcuts.render(request, template_name, context=None, content_type=None, status=None, using=None)** - Render template to `HttpResponse`.
- **django.shortcuts.redirect(to, *args, **kwargs)** - Return an HTTP redirect response.
- **django.shortcuts.get_object_or_404(klass, *args, **kwargs)** - Fetch object or raise `Http404`.
- **django.views.View.as_view(**initkwargs)** - Convert a CBV into a callable view.
- **django.db.models.Model** - Base class for ORM models.
- **Model.objects.filter(**lookups)** - Query for multiple rows.
- **Model.objects.get(**lookups)** - Fetch a single row (raises `Model.DoesNotExist` / `Model.MultipleObjectsReturned`).
- **Model.objects.create(**fields)** - Create and save a row.
- **django.db.transaction.atomic(using=None, savepoint=True, durable=False)** - Transaction context manager/decorator.
- **django.core.management.execute_from_command_line(argv=None)** - Run management commands (manage.py entrypoint).