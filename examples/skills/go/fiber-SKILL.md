---

name: fiber
description: Fiber is an Express.js-inspired Go web framework built on top of fasthttp, providing high-performance HTTP routing, middleware chaining, and request/response handling.
license: MIT
metadata:
  version: "2.52.6"
  ecosystem: go
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```go
import (
    "github.com/gofiber/fiber/v2"
)
```

## Core Patterns

### Basic App Setup and Routing ✅ Current

```go
package main

import (
    "log"

    "github.com/gofiber/fiber/v2"
)

func main() {
    app := fiber.New(fiber.Config{
        AppName:               "MyService v1.0",
        DisableStartupMessage: false,
    })

    app.Get("/", func(c *fiber.Ctx) error {
        return c.SendString("Hello, World!")
    })

    app.Post("/users", func(c *fiber.Ctx) error {
        type User struct {
            Name string `json:"name"`
        }
        var u User
        if err := c.BodyParser(&u); err != nil {
            return c.Status(fiber.StatusBadRequest).JSON(fiber.Map{
                "error": err.Error(),
            })
        }
        return c.Status(fiber.StatusCreated).JSON(u)
    })

    app.Get("/users/:id", func(c *fiber.Ctx) error {
        id := c.Params("id")
        return c.JSON(fiber.Map{"id": id})
    })

    log.Fatal(app.Listen(":3000"))
}
```

Register all routes before calling `app.Listen()`. Routes are matched in declaration order.

### Middleware and Route Groups ✅ Current

```go
package main

import (
    "log"
    "time"

    "github.com/gofiber/fiber/v2"
)

func logger(c *fiber.Ctx) error {
    start := time.Now()
    err := c.Next() // must call Next to continue the chain
    log.Printf("%s %s %d %s", c.Method(), c.Path(), c.Response().StatusCode(), time.Since(start))
    return err
}

func authRequired(c *fiber.Ctx) error {
    token := c.Get("Authorization")
    if token == "" {
        return c.Status(fiber.StatusUnauthorized).JSON(fiber.Map{
            "error": "missing authorization header",
        })
    }
    return c.Next()
}

func main() {
    app := fiber.New()

    // Global middleware
    app.Use(logger)

    // Public routes
    app.Get("/health", func(c *fiber.Ctx) error {
        return c.SendString("ok")
    })

    // Protected group
    api := app.Group("/api/v1", authRequired)
    api.Get("/profile", func(c *fiber.Ctx) error {
        return c.JSON(fiber.Map{"user": "example"})
    })
    api.Delete("/account", func(c *fiber.Ctx) error {
        return c.SendStatus(fiber.StatusNoContent)
    })

    log.Fatal(app.Listen(":3000"))
}
```

`app.Use()` applies middleware to all routes with the given prefix. Middleware must call `c.Next()` to pass control downstream.

### Centralized Error Handling ✅ Current

```go
package main

import (
    "errors"
    "log"

    "github.com/gofiber/fiber/v2"
)

type AppError struct {
    Code    int
    Message string
}

func (e *AppError) Error() string { return e.Message }

func main() {
    app := fiber.New(fiber.Config{
        ErrorHandler: func(c *fiber.Ctx, err error) error {
            code := fiber.StatusInternalServerError
            msg := "internal server error"

            var ae *AppError
            if errors.As(err, &ae) {
                code = ae.Code
                msg = ae.Message
            }

            var fe *fiber.Error
            if errors.As(err, &fe) {
                code = fe.Code
                msg = fe.Message
            }

            return c.Status(code).JSON(fiber.Map{"error": msg})
        },
    })

    app.Get("/items/:id", func(c *fiber.Ctx) error {
        id := c.Params("id")
        if id == "0" {
            return &AppError{Code: fiber.StatusNotFound, Message: "item not found"}
        }
        return c.JSON(fiber.Map{"id": id})
    })

    log.Fatal(app.Listen(":3000"))
}
```

Register a single `ErrorHandler` in `fiber.Config` to centralize all error formatting. Return errors from handlers; do not write error responses inline unless necessary.

### Route Naming and Route Groups ✅ Current

```go
package main

import (
    "log"

    "github.com/gofiber/fiber/v2"
)

func main() {
    app := fiber.New()

    handler := func(c *fiber.Ctx) error { return c.SendStatus(fiber.StatusOK) }

    app.Get("/home", handler).Name("home")

    users := app.Group("/users").Name("users.")
    users.Get("/", handler).Name("list")
    users.Get("/:id", handler).Name("detail")

    // Lookup registered route by name
    r := app.GetRoute("users.detail")
    log.Printf("route: %s %s", r.Method, r.Path)

    log.Fatal(app.Listen(":3000"))
}
```

Use `.Name()` on a route to assign a lookup key. Prefix group names with a dot separator so nested names compose cleanly (e.g., `"users.detail"`).

### Testing with App.Test ✅ Current

```go
package main

import (
    "io"
    "net/http/httptest"
    "testing"

    "github.com/gofiber/fiber/v2"
)

func TestGetHealth(t *testing.T) {
    app := fiber.New(fiber.Config{DisableStartupMessage: true})

    app.Get("/health", func(c *fiber.Ctx) error {
        return c.JSON(fiber.Map{"status": "ok"})
    })

    req := httptest.NewRequest("GET", "/health", nil)
    resp, err := app.Test(req)
    if err != nil {
        t.Fatalf("app.Test: %v", err)
    }
    if resp.StatusCode != fiber.StatusOK {
        t.Errorf("expected 200, got %d", resp.StatusCode)
    }

    body, err := io.ReadAll(resp.Body)
    if err != nil {
        t.Fatalf("read body: %v", err)
    }
    t.Logf("body: %s", body)
}
```

`app.Test()` does not require a running server. Pass `msTimeout` as a second argument to override the default test timeout (in milliseconds).

## Configuration

```go
app := fiber.New(fiber.Config{
    // Application
    AppName:               "my-app",
    DisableStartupMessage: false,

    // Routing
    StrictRouting:  false, // true: /foo and /foo/ are different routes
    CaseSensitive:  false, // true: /Foo and /foo are different routes
    UnescapePath:   false, // true: decode percent-encoded paths before matching

    // Request limits
    BodyLimit:      4 * 1024 * 1024, // default: 4 MiB
    Concurrency:    256 * 1024,       // max concurrent connections

    // Timeouts
    ReadTimeout:  0, // 0 = no timeout (time.Duration)
    WriteTimeout: 0,
    IdleTimeout:  0,

    // Only accept GET requests
    GETOnly: false,

    // Buffer sizes
    ReadBufferSize:  4096, // increase for large request headers
    WriteBufferSize: 4096,

    // Proxies
    ProxyHeader:             "", // e.g., "X-Forwarded-For"
    EnableTrustedProxyCheck: false,
    TrustedProxies:          []string{},

    // Custom HTTP methods (adds to defaults)
    RequestMethods: fiber.DefaultMethods,

    // Custom error handler
    ErrorHandler: fiber.DefaultErrorHandler,

    // Custom JSON encoder/decoder
    // JSONEncoder: json.Marshal,
    // JSONDecoder: json.Unmarshal,
})
```

All fields are optional; `fiber.New()` with no arguments uses safe defaults. Access the active config at runtime via `app.Config()`.

## Pitfalls

### Variable route declared before static route

#### Wrong
```go
app := fiber.New()
app.Get("/users/:name", func(c *fiber.Ctx) error {
    return c.SendString("user: " + c.Params("name"))
})
// Never reached — ":name" matches "profile" first
app.Get("/users/profile", func(c *fiber.Ctx) error {
    return c.SendString("profile page")
})
```

#### Right
```go
app := fiber.New()
// Static routes BEFORE parameter routes
app.Get("/users/profile", func(c *fiber.Ctx) error {
    return c.SendString("profile page")
})
app.Get("/users/:name", func(c *fiber.Ctx) error {
    return c.SendString("user: " + c.Params("name"))
})
```

### Middleware that does not call c.Next()

#### Wrong
```go
app.Use(func(c *fiber.Ctx) error {
    c.Set("X-Request-ID", "abc123")
    return nil // chain stops here; route handler never runs
})
app.Get("/", func(c *fiber.Ctx) error {
    return c.SendString("hello") // never executed
})
```

#### Right
```go
app.Use(func(c *fiber.Ctx) error {
    c.Set("X-Request-ID", "abc123")
    return c.Next() // continues to next handler
})
app.Get("/", func(c *fiber.Ctx) error {
    return c.SendString("hello")
})
```

### Registering routes after app.Listen()

#### Wrong
```go
app := fiber.New()
app.Get("/existing", handler)
go app.Listen(":3000") // server starts

// Routes added after Listen do not work reliably
app.Get("/late-route", handler)
```

#### Right
```go
app := fiber.New()
// Register ALL routes before Listen
app.Get("/existing", handler)
app.Get("/late-route", handler)

app.Listen(":3000") // start server last
```

### Literal colon in route path not escaped

#### Wrong
```go
// ":customVerb" is parsed as a parameter, not a literal segment
app.Get("/resource/name:customVerb", handler)
```

#### Right
```go
// Use a raw string literal and escape the colon
app.Get(`/resource/name\:customVerb`, handler)
```

### Expecting route constraint failure to return 400

#### Wrong
```go
// Expecting 400 Bad Request when the constraint fails
app.Get("/:age<min(18)>", func(c *fiber.Ctx) error {
    return c.SendString(c.Params("age"))
})
// GET /5 → expects 400, actually returns 404
```

#### Right
```go
// Constraints return 404 on mismatch — use them for routing only.
// Perform business validation inside the handler.
app.Get("/:age", func(c *fiber.Ctx) error {
    age, err := c.ParamsInt("age")
    if err != nil || age < 18 {
        return c.Status(fiber.StatusBadRequest).JSON(fiber.Map{
            "error": "age must be >= 18",
        })
    }
    return c.SendString("welcome")
})
```

## References

- [Documentation](https://pkg.go.dev/github.com/gofiber/fiber/v2)
- [Source](https://github.com/gofiber/fiber)

## Migration from v1

Fiber v2 introduced breaking changes from v1. Key items:

**Handler signature changed**

```go
// v1
app.Get("/", func(c *fiber.Ctx) {
    c.Send("hello")
})

// v2 — handler must return error
app.Get("/", func(c *fiber.Ctx) error {
    return c.SendString("hello")
})
```

**ErrorHandler is now in Config**

```go
// v2
app := fiber.New(fiber.Config{
    ErrorHandler: func(c *fiber.Ctx, err error) error {
        return c.Status(fiber.StatusInternalServerError).SendString(err.Error())
    },
})
```

**Route constraints added (v2.37.0)**

Angle-bracket constraint syntax (`:param<constraint>`) was not available in v1. Failing constraints return 404, not 400.

**No dynamic route registration**

Routes must be registered before `app.Listen()`. This constraint exists in both v1 and v2 but is stricter in v2.

## API Reference

- **fiber.New(config ...Config) \*App** — Creates a new Fiber application. Accepts an optional `Config` struct.
- **App.Get / Post / Put / Delete / Patch(path string, handlers ...Handler) Router** — Registers a route for the given HTTP method. Returns `Router` for chaining `.Name()`.
- **App.Use(args ...interface{}) Router** — Mounts middleware for all methods. `args` may be a path string, `[]string` of paths, or `Handler` funcs.
- **App.Group(prefix string, handlers ...Handler) Router** — Creates a sub-router sharing a common path prefix and optional middleware.
- **App.Listen(addr string) error** — Starts the HTTP server on the given address (e.g., `":3000"`). Blocks until shutdown.
- **App.Shutdown() error** — Gracefully shuts down the server without interrupting active connections.
- **App.ShutdownWithTimeout(timeout time.Duration) error** — Graceful shutdown with a deadline; returns `context.DeadlineExceeded` if connections remain open.
- **App.Test(req \*http.Request, msTimeout ...int) (\*http.Response, error)** — Executes a request against the app without a live server, for unit testing.
- **App.Config() Config** — Returns the active configuration struct.
- **App.GetRoute(name string) Route** — Looks up a named route registered with `.Name()`.
- **fiber.Config** — Configuration struct. Key fields: `ErrorHandler`, `StrictRouting`, `CaseSensitive`, `BodyLimit`, `ReadTimeout`, `WriteTimeout`, `GETOnly`, `AppName`, `RequestMethods`.
- **Ctx.BodyParser(out interface{}) error** — Decodes the request body (JSON, XML, form) into `out` based on `Content-Type`.
- **Ctx.Params(key string, defaultValue ...string) string** — Returns the URL route parameter value for `key`.
- **Ctx.JSON(data interface{}) error** — Serializes `data` as JSON and sets `Content-Type: application/json`.
- **Ctx.Next() error** — Passes control to the next matching handler in the middleware chain.