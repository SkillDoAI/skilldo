---

name: chi
description: chi is a lightweight, idiomatic Go HTTP router built on net/http, supporting URL parameters, middleware chaining, route grouping, and sub-router mounting with zero external dependencies.
license: MIT
metadata:
  version: "5.2.1"
  ecosystem: go
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```go
import (
    "net/http"

    "github.com/go-chi/chi/v5"
    "github.com/go-chi/chi/v5/middleware"
)
```

## Core Patterns

### Basic Router Setup ✅ Current

Create a router, attach global middleware, register routes, and serve.

```go
package main

import (
    "net/http"

    "github.com/go-chi/chi/v5"
    "github.com/go-chi/chi/v5/middleware"
)

func main() {
    r := chi.NewRouter()

    r.Use(middleware.RequestID)
    r.Use(middleware.RealIP)
    r.Use(middleware.Logger)
    r.Use(middleware.Recoverer)

    r.Get("/", func(w http.ResponseWriter, r *http.Request) {
        w.Write([]byte("hello world"))
    })

    r.Get("/users/{userID}", func(w http.ResponseWriter, r *http.Request) {
        userID := chi.URLParam(r, "userID")
        w.Write([]byte("user: " + userID))
    })

    http.ListenAndServe(":3000", r)
}
```

### Route Grouping and Nesting ✅ Current

Use `Route` for path-prefixed sub-groups and `Group` for same-path grouping with different middleware.

```go
package main

import (
    "net/http"

    "github.com/go-chi/chi/v5"
    "github.com/go-chi/chi/v5/middleware"
)

func main() {
    r := chi.NewRouter()
    r.Use(middleware.Logger)

    r.Route("/api/v1", func(r chi.Router) {
        r.Get("/health", func(w http.ResponseWriter, r *http.Request) {
            w.Write([]byte("ok"))
        })

        r.Route("/articles", func(r chi.Router) {
            r.Get("/", listArticles)
            r.Post("/", createArticle)
            r.Route("/{articleID}", func(r chi.Router) {
                r.Get("/", getArticle)
                r.Put("/", updateArticle)
                r.Delete("/", deleteArticle)
            })
        })
    })

    http.ListenAndServe(":3000", r)
}

func listArticles(w http.ResponseWriter, r *http.Request)   { w.Write([]byte("list")) }
func createArticle(w http.ResponseWriter, r *http.Request)  { w.Write([]byte("create")) }
func getArticle(w http.ResponseWriter, r *http.Request)     { w.Write([]byte("get")) }
func updateArticle(w http.ResponseWriter, r *http.Request)  { w.Write([]byte("update")) }
func deleteArticle(w http.ResponseWriter, r *http.Request)  { w.Write([]byte("delete")) }
```

### Inline Per-Route Middleware with `With` ✅ Current

Apply middleware to individual routes without affecting the entire group.

```go
package main

import (
    "context"
    "net/http"

    "github.com/go-chi/chi/v5"
)

type contextKey string

const articleKey contextKey = "article"

func articleCtx(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        articleID := chi.URLParam(r, "articleID")
        ctx := context.WithValue(r.Context(), articleKey, articleID)
        next.ServeHTTP(w, r.WithContext(ctx))
    })
}

func main() {
    r := chi.NewRouter()

    r.With(articleCtx).Get("/articles/{articleID}", func(w http.ResponseWriter, r *http.Request) {
        id, ok := r.Context().Value(articleKey).(string)
        if !ok {
            http.Error(w, http.StatusText(http.StatusUnprocessableEntity), http.StatusUnprocessableEntity)
            return
        }
        w.Write([]byte("article: " + id))
    })

    http.ListenAndServe(":3000", r)
}
```

### Mounting Sub-Routers ✅ Current

Attach fully independent sub-routers at a path prefix using `Mount`.

```go
package main

import (
    "net/http"
    "os"

    "github.com/go-chi/chi/v5"
    "github.com/go-chi/chi/v5/middleware"
)

func adminRouter() http.Handler {
    r := chi.NewRouter()
    // Load credentials from environment variables; never hardcode credentials in source.
    adminUser := os.Getenv("ADMIN_USER")
    adminPass := os.Getenv("ADMIN_PASS")
    r.Use(middleware.BasicAuth("admin area", map[string]string{
        adminUser: adminPass,
    }))
    r.Get("/", func(w http.ResponseWriter, r *http.Request) {
        w.Write([]byte("admin index"))
    })
    r.Get("/accounts", func(w http.ResponseWriter, r *http.Request) {
        w.Write([]byte("admin accounts"))
    })
    return r
}

func main() {
    r := chi.NewRouter()
    r.Use(middleware.Logger)

    r.Get("/", func(w http.ResponseWriter, r *http.Request) {
        w.Write([]byte("home"))
    })

    r.Mount("/admin", adminRouter())

    http.ListenAndServe(":3000", r)
}
```

### Walking Routes and Custom Error Handlers ✅ Current

Enumerate all registered routes with `chi.Walk` and register custom 404/405 handlers.

```go
package main

import (
    "fmt"
    "net/http"

    "github.com/go-chi/chi/v5"
)

func main() {
    r := chi.NewRouter()

    r.NotFound(func(w http.ResponseWriter, r *http.Request) {
        w.WriteHeader(http.StatusNotFound)
        w.Write([]byte("nothing here"))
    })

    r.MethodNotAllowed(func(w http.ResponseWriter, r *http.Request) {
        w.WriteHeader(http.StatusMethodNotAllowed)
        w.Write([]byte("method not allowed"))
    })

    r.Get("/ping", func(w http.ResponseWriter, r *http.Request) {
        w.Write([]byte("pong"))
    })
    r.Post("/items", func(w http.ResponseWriter, r *http.Request) {
        w.Write([]byte("created"))
    })

    walkFn := func(method, route string, handler http.Handler, middlewares ...func(http.Handler) http.Handler) error {
        fmt.Printf("%s %s\n", method, route)
        return nil
    }

    if err := chi.Walk(r, walkFn); err != nil {
        fmt.Printf("walk error: %s\n", err.Error())
    }

    http.ListenAndServe(":3000", r)
}
```

## Configuration

### Middleware Stack Order

Middleware is executed in registration order. Register global middleware before routes:

```go
r := chi.NewRouter()
// Order matters: RequestID → RealIP → Logger → Recoverer
r.Use(middleware.RequestID)
r.Use(middleware.RealIP)
r.Use(middleware.Logger)
r.Use(middleware.Recoverer)
```

### Compression

```go
// Simple: compress at level 5 for specific content types
r.Use(middleware.Compress(5, "text/html", "application/json"))

// Advanced: custom compressor with pooled encoders
compressor := middleware.NewCompressor(5, "text/html", "text/css", "application/json")
r.Use(compressor.Handler)
```

### Custom HTTP Methods

Register custom methods in `init()` before any router is created:

```go
func init() {
    chi.RegisterMethod("LINK")
    chi.RegisterMethod("UNLINK")
}

func main() {
    r := chi.NewRouter()
    r.MethodFunc("LINK", "/resources/{id}", linkHandler)
    http.ListenAndServe(":3000", r)
}

func linkHandler(w http.ResponseWriter, r *http.Request) {
    w.Write([]byte("linked"))
}
```

### Request ID Header

```go
// Default header is "X-Request-Id". Override before router creation:
middleware.RequestIDHeader = "X-Trace-Id"
r := chi.NewRouter()
r.Use(middleware.RequestID)
r.Get("/", func(w http.ResponseWriter, r *http.Request) {
    reqID := middleware.GetReqID(r.Context())
    w.Write([]byte(reqID))
})
```

### Base Context (stdlib pattern)

```go
package main

import (
    "context"
    "net"
    "net/http"

    "github.com/go-chi/chi/v5"
)

func main() {
    r := chi.NewRouter()

    // Do NOT use the removed chi.ServerBaseContext — use stdlib instead:
    srv := &http.Server{
        Addr:    ":3000",
        Handler: r,
        BaseContext: func(_ net.Listener) context.Context {
            return context.Background()
        },
    }
    srv.ListenAndServe()
}
```

## Pitfalls

### Pitfall 1: Wrong import path for v5

#### Wrong
```go
import "github.com/go-chi/chi"           // pulls v1.x, not v5
import "github.com/go-chi/chi/middleware" // v1.x middleware package
```

#### Right
```go
import "github.com/go-chi/chi/v5"
import "github.com/go-chi/chi/v5/middleware"
```

Go Semantic Import Versioning requires the `/v5` suffix. Without it, `go get` resolves to the old v1 module line, causing type mismatches.

---

### Pitfall 2: Registering middleware after routes

#### Wrong
```go
r := chi.NewRouter()
r.Get("/", handler)
r.Use(middleware.Logger) // too late — Logger does NOT wrap the GET route above
```

#### Right
```go
r := chi.NewRouter()
r.Use(middleware.Logger) // must come before route declarations
r.Get("/", handler)
```

`r.Use()` only wraps routes registered after the call.

---

### Pitfall 3: Unsafe context value type assertion

#### Wrong
```go
// panics if the value is absent or a different type
article := r.Context().Value(articleKey).(*Article)
```

#### Right
```go
article, ok := r.Context().Value(articleKey).(*Article)
if !ok {
    http.Error(w, http.StatusText(http.StatusUnprocessableEntity), http.StatusUnprocessableEntity)
    return
}
```

Always use the comma-ok pattern when asserting context values.

---

### Pitfall 4: Using plain string context keys

#### Wrong
```go
ctx := context.WithValue(r.Context(), "userID", id) // collides with other packages
```

#### Right
```go
type contextKey string
const userIDKey contextKey = "userID"

ctx := context.WithValue(r.Context(), userIDKey, id)
```

Plain string keys can collide across packages. Use an unexported custom type.

---

### Pitfall 5: Mounting a handler that doesn't handle the sub-path

#### Wrong
```go
// handler only matches exactly "/admin", not "/admin/accounts"
r.Mount("/admin", http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
    w.Write([]byte("admin"))
}))
```

#### Right
```go
func adminRouter() http.Handler {
    r := chi.NewRouter()
    r.Get("/", func(w http.ResponseWriter, r *http.Request) { w.Write([]byte("admin")) })
    r.Get("/accounts", func(w http.ResponseWriter, r *http.Request) { w.Write([]byte("accounts")) })
    return r
}

// Mount a full sub-router so all /admin/* paths are handled
r.Mount("/admin", adminRouter())
```

`Mount` strips the prefix and forwards the remainder to the handler; a single `HandlerFunc` will not dispatch sub-paths correctly.

## References

- [Documentation](https://pkg.go.dev/github.com/go-chi/chi/v5)
- [Source](https://github.com/go-chi/chi)

## Migration from v1.x / v4.x

### Import Path (Breaking)

All import paths must include the `/v5` suffix.

```go
// Before
import "github.com/go-chi/chi"
import "github.com/go-chi/chi/middleware"

// After
import "github.com/go-chi/chi/v5"
import "github.com/go-chi/chi/v5/middleware"
```

Update `go.mod`:

```
go get github.com/go-chi/chi/v5
```

### `chi.ServerBaseContext` Removed 🗑️ Removed

Removed in v1.5.1, not present in v5.

```go
// Before (removed — do not use)
h := chi.ServerBaseContext(ctx, r)

// After (stdlib http.Server.BaseContext)
srv := &http.Server{
    Addr:    ":3000",
    Handler: r,
    BaseContext: func(_ net.Listener) context.Context { return ctx },
}
```

All router API signatures (`Router`, `Routes`, `URLParam`, middleware signatures) are otherwise unchanged from v4 to v5 — only the import path changed.

## API Reference

**chi.NewRouter()** - Creates and returns a new `*Mux` router instance. Entry point for all router setup.

**chi.Mux.Use(middlewares ...func(http.Handler) http.Handler)** - Registers global middleware applied to all subsequently registered routes, in order.

**chi.Mux.With(middlewares ...func(http.Handler) http.Handler) Router** - Returns a new inline router with the given middleware applied only to routes registered on the returned router.

**chi.Mux.Group(fn func(r Router)) Router** - Creates an inline sub-router at the same path prefix; routes share the parent middleware stack.

**chi.Mux.Route(pattern string, fn func(r Router)) Router** - Creates a sub-router mounted at `pattern`; routes inside `fn` have the pattern as a prefix.

**chi.Mux.Mount(pattern string, h http.Handler)** - Attaches an `http.Handler` (typically another `chi.Router`) at the given path prefix, stripping the prefix before forwarding.

**chi.Mux.Get / Post / Put / Patch / Delete(pattern string, handlerFn http.HandlerFunc)** - Registers a route for the corresponding HTTP method. `handlerFn` must match `func(http.ResponseWriter, *http.Request)`.

**chi.Mux.Method(method, pattern string, h http.Handler)** - Registers a route for an arbitrary HTTP method string with an `http.Handler` (useful for custom handler types).

**chi.Mux.MethodFunc(method, pattern string, handlerFn http.HandlerFunc)** - Same as `Method` but accepts `http.HandlerFunc`; required for custom methods registered via `chi.RegisterMethod`.

**chi.Mux.HandleFunc(pattern string, handlerFn http.HandlerFunc)** - Registers a handler for all HTTP methods (standard + any custom-registered methods).

**chi.Mux.NotFound(handlerFn http.HandlerFunc)** - Sets a custom handler for requests that match no route (default: 404).

**chi.Mux.MethodNotAllowed(handlerFn http.HandlerFunc)** - Sets a custom handler for requests where the path matches but the method does not (default: 405).

**chi.URLParam(r *http.Request, key string) string** - Extracts a named URL parameter (e.g., `{id}`) from the request. Returns empty string if not found.

**chi.RouteContext(ctx context.Context) *Context** - Retrieves the chi `*Context` from a `context.Context`; use to access `RoutePattern()` and `URLParams`.

**chi.Walk(r Routes, walkFn WalkFunc) error** - Traverses all registered routes calling `walkFn(method, route, handler, middlewares...)` for each; returns the first non-nil error from `walkFn`.

**chi.RegisterMethod(method string)** - Adds a custom HTTP method string to chi's method registry; must be called in `init()` before any router is created.