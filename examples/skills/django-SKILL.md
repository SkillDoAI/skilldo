---

name: django
description: A Python web framework for building database-backed web applications with URL routing, views, templates, and an ORM.
version: unknown
ecosystem: python
license: BSD-3-Clause
generated_with: gpt-5.2
---

## Imports

```python
import django
from django.conf import settings
from django.core.management import execute_from_command_line
from django.http import HttpRequest, HttpResponse, JsonResponse, Http404
from django.urls import include, path, reverse
from django.shortcuts import get_object_or_404, redirect, render
from django.db import models, transaction
from django.contrib import admin
```

## Core Patterns

### Project entrypoint and settings bootstrap ✅ Current
```python
from __future__ import annotations

import os
import sys

import django
from django.conf import settings
from django.core.management import execute_from_command_line


def main() -> None:
    os.environ.setdefault("DJANGO_SETTINGS_MODULE", "mysite.settings")

    # Minimal safety check: ensure settings can be imported/configured.
    django.setup()

    # Run Django's management command runner (e.g., runserver, migrate, test).
    execute_from_command_line(sys.argv)


if __name__ == "__main__":
    main()
```
* Use `DJANGO_SETTINGS_MODULE` to point Django at your settings module, then call `django.setup()` before interacting with models in standalone scripts.
* `execute_from_command_line()` is the standard entrypoint for `manage.py`.

### URL routing + function-based view ✅ Current
```python
from __future__ import annotations

from django.http import HttpRequest, HttpResponse, JsonResponse, Http404
from django.urls import path


def healthz(request: HttpRequest) -> JsonResponse:
    return JsonResponse({"status": "ok"})


def hello(request: HttpRequest, name: str) -> HttpResponse:
    if not name:
        raise Http404("Name is required")
    return HttpResponse(f"Hello, {name}!")


urlpatterns = [
    path("healthz/", healthz, name="healthz"),
    path("hello/<str:name>/", hello, name="hello"),
]
```
* Use `path()` for common URL patterns and typed converters like `<str:name>`.
* Raise `Http404` for “not found” responses; Django will render a 404 page (or JSON if you handle it).

### Render templates + redirects ✅ Current
```python
from __future__ import annotations

from django.http import HttpRequest, HttpResponse
from django.shortcuts import redirect, render
from django.urls import reverse


def index(request: HttpRequest) -> HttpResponse:
    # Renders templates/index.html with context.
    return render(request, "index.html", {"user_agent": request.META.get("HTTP_USER_AGENT", "")})


def go_home(request: HttpRequest) -> HttpResponse:
    # Prefer named URLs for redirects.
    return redirect(reverse("home"))
```
* Use `render()` to return an `HttpResponse` with a template.
* Use `redirect()` with `reverse()` (named routes) to avoid hardcoding URLs.

### Models + ORM queries + transactions ✅ Current
```python
from __future__ import annotations

from django.db import models, transaction


class Article(models.Model):
    title = models.CharField(max_length=200)
    published = models.BooleanField(default=False)

    def __str__(self) -> str:
        return self.title


def publish_article(article_id: int) -> None:
    # Example of an atomic update.
    with transaction.atomic():
        article = Article.objects.select_for_update().get(pk=article_id)
        article.published = True
        article.save(update_fields=["published"])
```
* Define models by subclassing `models.Model`; use `objects` manager for queries.
* Wrap multi-step updates in `transaction.atomic()`; use `select_for_update()` for row locking where supported.

### Admin registration ✅ Current
```python
from __future__ import annotations

from django.contrib import admin
from django.db import models


class Article(models.Model):
    title = models.CharField(max_length=200)
    published = models.BooleanField(default=False)


@admin.register(Article)
class ArticleAdmin(admin.ModelAdmin):
    list_display = ("id", "title", "published")
    list_filter = ("published",)
    search_fields = ("title",)
```
* Register models with the admin via `@admin.register(Model)` and customize with `ModelAdmin`.

## Configuration

- **Settings module**: Set `DJANGO_SETTINGS_MODULE="package.settings"` (commonly via environment variable) before calling `django.setup()` in standalone code.
- **Common settings to customize** (in your settings file):
  - `DEBUG` (bool)
  - `SECRET_KEY` (string)
  - `ALLOWED_HOSTS` (list[str])
  - `INSTALLED_APPS` (list[str])
  - `MIDDLEWARE` (list[str])
  - `ROOT_URLCONF` (string)
  - `TEMPLATES` (list[dict])
  - `DATABASES` (dict)
  - `STATIC_URL` / `STATIC_ROOT`
- **Docs workflow (source + build)**:
  - Django docs are written in reStructuredText and built with Sphinx.
  - Build locally from the `docs/` directory:
    - `python -m pip install Sphinx`
    - `make html` (or `make.bat html` on Windows)
    - open `docs/_build/html/index.html`
- **Tests**: Run Django’s test suite following the official unit test instructions in Django’s contributing docs.

## Pitfalls

### Wrong: Building documentation from the wrong directory
```python
# This is a shell command, not Python; shown here to illustrate the mistake.
# Wrong (run from repo root):
# make html
raise SystemExit("Run `make html` from the `docs/` directory so Sphinx finds its config.")
```

### Right: Build docs from `docs/` so Sphinx finds configuration
```python
# This is a shell command, not Python; shown here to illustrate the fix.
# cd docs
# python -m pip install Sphinx
# make html
# # Windows: make.bat html
raise SystemExit("Build docs from `docs/` (not repo root) to ensure Sphinx config is discovered.")
```

### Wrong: Using ORM models before `django.setup()` in a standalone script
```python
from __future__ import annotations

import os

from django.db import models


os.environ.setdefault("DJANGO_SETTINGS_MODULE", "mysite.settings")


class Article(models.Model):
    title = models.CharField(max_length=200)


# Importing/using models without django.setup() can fail depending on context.
# For example, app registry may not be ready.
raise SystemExit("Call django.setup() before using models in standalone scripts.")
```

### Right: Call `django.setup()` before importing/using app models
```python
from __future__ import annotations

import os

import django

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "mysite.settings")
django.setup()

# Now it's safe to import and use models from installed apps.
raise SystemExit("Django is initialized; you can now import and use models safely.")
```

### Wrong: Hardcoding URLs instead of using `reverse()`
```python
from __future__ import annotations

from django.http import HttpRequest, HttpResponse
from django.shortcuts import redirect


def go_home(request: HttpRequest) -> HttpResponse:
    # Breaks if the URL changes.
    return redirect("/home/")
```

### Right: Use named URLs with `reverse()` (or pass the view name to `redirect()`)
```python
from __future__ import annotations

from django.http import HttpRequest, HttpResponse
from django.shortcuts import redirect
from django.urls import reverse


def go_home(request: HttpRequest) -> HttpResponse:
    return redirect(reverse("home"))
```

### Wrong: Catch-all `Exception` instead of raising `Http404` for missing objects
```python
from __future__ import annotations

from django.http import HttpRequest, HttpResponse
from django.shortcuts import render

# NOTE: In real code, you'd import your model, e.g. from app.models import Article


def detail(request: HttpRequest, pk: int) -> HttpResponse:
    try:
        raise Exception("pretend ORM lookup failed")  # placeholder for a failing lookup
    except Exception:
        # Returns 200 with an error page, not a 404.
        return render(request, "not_found.html", status=200)
```

### Right: Use `get_object_or_404()` (or raise `Http404`) for missing records
```python
from __future__ import annotations

from django.http import HttpRequest, HttpResponse
from django.shortcuts import get_object_or_404, render
from django.db import models


class Article(models.Model):
    title = models.CharField(max_length=200)


def detail(request: HttpRequest, pk: int) -> HttpResponse:
    article = get_object_or_404(Article, pk=pk)
    return render(request, "detail.html", {"article": article})
```

## References

- [Homepage](https://www.djangoproject.com/)
- [Documentation](https://docs.djangoproject.com/)
- [Release notes](https://docs.djangoproject.com/en/stable/releases/)
- [Funding](https://www.djangoproject.com/fundraising/)
- [Source](https://github.com/django/django)
- [Tracker](https://code.djangoproject.com/)
- [New ticket](https://code.djangoproject.com/newticket)
- [Contributing guide](https://docs.djangoproject.com/en/dev/internals/contributing/)

## Migration from v[previous]

Not applicable (no version-specific migration inputs were provided). For upgrade guidance, use Django’s release notes:
https://docs.djangoproject.com/en/stable/releases/

## API Reference

- **django.setup()** - Initialize Django (apps registry, settings); call in standalone scripts before ORM usage.
- **django.conf.settings** - Access configured settings (read-only in most runtime code).
- **django.core.management.execute_from_command_line(argv)** - Run management commands (used by `manage.py`).
- **django.urls.path(route, view, kwargs=None, name=None)** - Define URL patterns with converters.
- **django.urls.include(module)** - Include other URLconfs (apps).
- **django.urls.reverse(viewname, args=None, kwargs=None)** - Build URLs from named routes.
- **django.http.HttpRequest** - Incoming request object (headers in `META`, method, user, etc.).
- **django.http.HttpResponse(content=b"", status=200, ...)** - Basic HTTP response type.
- **django.http.JsonResponse(data, status=200, ...)** - JSON response with proper content type/encoding.
- **django.http.Http404** - Exception to signal a 404 response.
- **django.shortcuts.render(request, template_name, context=None, status=None)** - Render template to `HttpResponse`.
- **django.shortcuts.redirect(to, *args, **kwargs)** - Return an HTTP redirect response.
- **django.shortcuts.get_object_or_404(klass, *args, **kwargs)** - Fetch object or raise `Http404`.
- **django.db.models.Model** - Base class for ORM models.
- **django.db.transaction.atomic()** - Transaction context manager/decorator for atomic DB operations.
- **django.contrib.admin.site / admin.register()** - Admin registration and configuration entrypoints.