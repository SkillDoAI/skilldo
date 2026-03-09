---
name: django
description: A high‑level Python web framework that promotes rapid development and clean, pragmatic design.
license: BSD-3-Clause
metadata:
  version: "latest"
  ecosystem: python
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```python
import django
from django.conf import settings
from django.core.management import execute_from_command_line

# HTTP utilities
from django.http import Http404
from django.http.request import HttpRequest
from django.http.response import HttpResponse, JsonResponse

# URL routing helpers
from django.urls import include, path, reverse

# Shortcut helpers
from django.shortcuts import get_object_or_404, redirect, render

# ORM base classes and transaction handling
from django.db import models, transaction

# Admin site registration (imported via correct alias)
from django.contrib import admin

# Application configuration utilities
from django.apps import AppConfig, apps

# Core exception types
from django.core.exceptions import ImproperlyConfigured, ObjectDoesNotExist

# Testing utilities
from django.test import TestCase, override_settings

# Generic class‑based view base
from django.views.generic.base import View
from django.views.generic.list import ListView
from django.views.generic.detail import DetailView

# Dynamic import helper
from django.utils.module_loading import import_string
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

### URL routing + function‑based view ✅ Current
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
* Use `redirect()` with `reverse()` (named routes) to avoid hard‑coding URLs.

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
* Wrap multi‑step updates in `transaction.atomic()`; use `select_for_update()` for row locking where supported.

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
* Register your `AppConfig` in `INSTALLED_APPS` as `"myapp.apps.MyAppConfig"`.
* The `name` attribute must match an importable Python package path in your project.
* Use `ready()` for one‑time initialization like registering signal handlers (be careful not to import models at module level to avoid import cycles).

### App configuration ✅ Current
```python
from __future__ import annotations

from django.apps import AppConfig


class MyAppConfig(AppConfig):
    default_auto_field = "django.db.models.BigAutoField"
    name = "myapp"
    verbose_name = "My Application"

    def ready(self) -> None:
        # Import signal handlers, register checks, etc.
        # Only runs once when Django starts.
        pass  # import myapp.signals  # noqa: F401
```
* Subclass `AppConfig` to customize app metadata and perform initialization in `ready()`.
* Register your `AppConfig` in `INSTALLED_APPS` as `"myapp.apps.MyAppConfig"`.
* The `name` attribute must match an importable Python package path in your project.
* Use `ready()` for one‑time initialization like registering signal handlers (be careful not to import models at module level to avoid import cycles).

### Class‑based views (ListView) ✅ Current
```python
from __future__ import annotations

from django.views.generic.list import ListView
from django.db import models


class Author(models.Model):
    name = models.CharField(max_length=200)
    slug = models.SlugField(unique=True)


class AuthorListView(ListView):
    model = Author
    template_name = "authors/list.html"
    context_object_name = "authors"
    paginate_by = 30
    ordering = ["name"]
```
* Use `ListView` for listing querysets with built‑in pagination support.
* Set `paginate_by` to enable pagination; Django adds `page_obj`, `paginator`, and `is_paginated` to context.

### Class‑based views (DetailView) ✅ Current
```python
from __future__ import annotations

from django.views.generic.detail import DetailView
from django.db import models


class Author(models.Model):
    name = models.CharField(max_length=200)
    slug = models.SlugField(unique=True)


class AuthorDetailView(DetailView):
    model = Author
    template_name = "authors/detail.html"
    context_object_name = "author"
    slug_field = "slug"
    slug_url_kwarg = "slug"
```
* Use `DetailView` for displaying single object details.
* Django automatically fetches the object by `pk` or a custom slug field.

### Dynamic import helper ✅ New
```python
from __future__ import annotations

from django.utils.module_loading import import_string


def load_view(dotted_path: str):
    """
    Resolve a view (or any callable) from its dotted import path.
    """
    view = import_string(dotted_path)
    return view
```
* `import_string()` safely imports a Python object given its dotted path, useful for lazily loading views, serializers, or admin actions.

### Admin autodiscover ✅ New
```python
from __future__ import annotations

from django.contrib import admin


def autodiscover_admin():
    """
    Auto‑discover admin modules across INSTALLED_APPS.
    Equivalent to calling `admin.autodiscover()`.
    """
    admin.autodiscover()
```
* `admin.autodiscover()` scans each app for an `admin.py` module and registers any `ModelAdmin` classes it contains.

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
  - `DEFAULT_FILE_STORAGE` (string, for custom file storage backends)
  - `STATICFILES_STORAGE` (string, for custom static files storage)
- **Docs workflow (source + build)**:
  - Django docs are written in reStructuredText and built with Sphinx.
  - Build locally from the `docs/` directory:
    ```bash
    python -m pip install Sphinx
    make html          # or `make.bat html` on Windows
    ```
  - Open `docs/_build/html/index.html`.
- **Tests**: Run Django’s test suite with the standard `manage.py test` command or via `django.test.TestCase` classes.

### Migration ⚠️

- **Deprecated `django.conf.urls` helpers**: `include`, `handler400`, `handler403`, `handler404`, and `handler500` have been deprecated in favor of their equivalents in `django.urls`. Existing code should migrate to `from django.urls import include, path, reverse` and configure error handlers in the root URLconf (`handler404 = "myapp.views.my_404"` etc.) using the `django.urls` namespace.
- **`django.contrib.admin.action` signature change**: The helper now accepts `description` and optional `permissions` keyword arguments (`action(description='', permissions=None)`). Update any custom admin actions accordingly.
- No breaking changes to core ORM, class‑based views, or settings handling in the latest release.

## API Reference

**Core Initialization**

- `django.setup(set_prefix: bool = True)` — Initialize Django (apps registry, settings); call in standalone scripts before ORM usage.
- `django.VERSION` — Tuple representing Django version: `(major, minor, micro, releaselevel, serial)`.
- `django.__version__` — String representation of Django version.

**Configuration**

- `django.conf.settings` — Access configured settings (lazy proxy, read‑only in most runtime code).
- `django.conf.LazySettings.configure(default_settings, **options)` — Manually configure settings without a settings module.
- `django.conf.LazySettings.configured` — Property; returns `True` if settings have been configured.
- `django.conf.Settings.is_overridden(setting)` — Check if a setting has been overridden from defaults.

**Apps**

- `django.apps.AppConfig` — Base class for Django application configuration.
- `django.apps.AppConfig.ready()` — Hook called when the app registry is fully populated.
- `django.apps.AppConfig.get_model(model_name, require_ready=True)` — Return the model with the given name.
- `django.apps.apps.get_model(app_label, model_name=None)` — Look up a model by app label and name.
- `django.apps.apps.get_app_config(app_label)` — Return the `AppConfig` for the given app label.
- `django.apps.apps.is_installed(app_name)` — Return `True` if the named app is installed.

**URLs**

- `django.urls.path(route, view, kwargs=None, name=None)` — Define URL patterns with converters.
- `django.urls.include(arg, namespace=None)` — Include other URLconfs (apps).  
  ⚠️ *Deprecated*: `django.conf.urls.include` – migrate to `django.urls.include`.
- `django.urls.reverse(viewname, args=None, kwargs=None)` — Build URLs from named routes.

**HTTP**

- `django.http.HttpRequest` — Incoming request object (headers in `META`, method, user, etc.).
- `django.http.HttpResponse(content=b"", status=200)` — Basic HTTP response type.
- `django.http.JsonResponse(data, status=200)` — JSON response with proper content type/encoding.
- `django.http.Http404` — Exception to signal a 404 response.

**Shortcuts**

- `django.shortcuts.render(request, template_name, context=None, status=None)` — Render template to an `HttpResponse`.
- `django.shortcuts.redirect(to, *args, **kwargs)` — Return an HTTP redirect response.
- `django.shortcuts.get_object_or_404(klass, *args, **kwargs)` — Fetch object or raise `Http404`.

**Models & Database**

- `django.db.models.Model` — Base class for ORM models.
- `django.db.transaction.atomic(using=None, savepoint=True, durable=False)` — Transaction context manager/decorator for atomic DB operations.

**Admin**

`django.contrib.admin` provides the following key symbols:

| Symbol | Description |
|---|---|
| `admin.site` | Default `AdminSite` instance. |
| `admin.ModelAdmin` | Base class for customizing model admin interface. |
| `admin.ModelAdmin.save_model(request, obj, form, change)` | Hook for saving model instances. |
| `admin.ModelAdmin.get_queryset(request)` | Return the base queryset for the admin list view. |
| `admin.ModelAdmin.has_add_permission(request)` | Return `True` if adding is permitted. |
| `admin.ModelAdmin.has_change_permission(request, obj=None)` | Return `True` if changing is permitted. |
| `admin.ModelAdmin.has_delete_permission(request, obj=None)` | Return `True` if deleting is permitted. |
| `admin.ModelAdmin.has_view_permission(request, obj=None)` | Return `True` if viewing is permitted. |
| `admin.StackedInline` | Inline admin with stacked layout. |
| `admin.TabularInline` | Inline admin with tabular layout. |
| `admin.SimpleListFilter` | Base class for custom list filters in admin. |
| `admin.action(function=None, *, permissions=None, description=None)` | Decorator for admin action functions. ⚠️ *Signature updated* |
| `admin.display(function=None, *, boolean=None, description=None, ordering=None)` | Decorator for admin display callables. |
| `admin.register(*models, site=None)` | Decorator to register models with the admin. |
| `admin.HORIZONTAL` / `admin.VERTICAL` | Constants for many‑to‑many widget layout. |
| `admin.AdminSite(name='admin')` | Admin site class; default name is `'admin'`. |
| `admin.autodiscover()` | Auto‑discover admin modules across `INSTALLED_APPS`. |

**Auth**

- `django.contrib.auth.SESSION_KEY` — Session key for the authenticated user ID.
- `django.contrib.auth.BACKEND_SESSION_KEY` — Session key for the auth backend path.
- `django.contrib.auth.HASH_SESSION_KEY` — Session key for the password hash.
- `django.contrib.auth.REDIRECT_FIELD_NAME` — Default name of the redirect GET parameter.
- `django.contrib.auth.load_backend(path: str) -> object` — Load an authentication backend by dotted path.

**Utilities**

- `django.utils.module_loading.import_string(dotted_path: str) -> Any` — Import an object by its dotted path.

**Testing**

- `django.test.TestCase` — Base test case class with database isolation and fixtures.
- `django.test.override_settings(**kwargs)` — Decorator/context manager for temporarily overriding settings in tests.

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
raise SystemExit("Call `django.setup()` before using models in standalone scripts.")
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

### Wrong: Catch‑all `Exception` instead of raising `Http404` for missing objects
```python
from __future__ import annotations

from django.http import HttpRequest, HttpResponse
from django.shortcuts import render


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

### Wrong: ListView without model or queryset
```python
from __future__ import annotations

from django.views.generic.list import ListView


class AuthorListView(ListView):
    template_name = "authors/list.html"
    # Missing: model, queryset, or get_queryset() override
```
* Django will raise `ImproperlyConfigured`.

### Right: ListView with model or queryset defined
```python
from __future__ import annotations

from django.views.generic.list import ListView
from django.db import models


class Author(models.Model):
    name = models.CharField(max_length=200)


class AuthorListView(ListView):
    model = Author  # or: queryset = Author.objects.all()
    template_name = "authors/list.html"
```

### Wrong: Accessing settings before configuration
```python
from __future__ import annotations

from django.conf import settings

# Accessing settings before django.setup() or settings.configure()
print(settings.DEBUG)  # May raise ImproperlyConfigured
```

### Right: Configure or setup before accessing settings
```python
from __future__ import annotations

import os
import django

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "mysite.settings")
django.setup()

from django.conf import settings
print(settings.DEBUG)  # Safe after setup
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
- [Django Discord](https://chat.djangoproject.com)
- [Django Forum](https://forum.djangoproject.com/)

For upgrade guidance, consult Django’s [release notes](https://docs.djangoproject.com/en/stable/releases/). Key migration steps: review `INSTALLED_APPS` for proper `AppConfig` classes; check for deprecated settings or middleware; run `python manage.py check`; test pagination and class‑based views; review custom storage backends if using `DEFAULT_FILE_STORAGE` or `STATICFILES_STORAGE`.