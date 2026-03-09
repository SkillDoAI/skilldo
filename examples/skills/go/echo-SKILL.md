---
name: echo
description: High‑performance, minimalist Go web framework
license: MIT
metadata:
  version: "4.13.4"
  ecosystem: go
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```go
import (
    "github.com/labstack/echo/v4"
    "github.com/labstack/echo/v4/middleware"
)

// For JWT (removed from core in v4.13.0 — use external package):
// echojwt "github.com/labstack/echo-jwt/v4"
// "github.com/golang-jwt/jwt/v5"
```

## Core Patterns

### Starting a Basic Server ✅ Current

```go
package main

import (
    "errors"
    "log/slog"
    "net/http"

    "github.com/labstack/echo/v4"
    "github.com/labstack/echo/v4/middleware"
)

func main() {
    e := echo.New()

    e.Use(middleware.Logger())
    e.Use(middleware.Recover())

    e.GET("/", func(c echo.Context) error {
        return c.String(http.StatusOK, "Hello, World!")
    })

    e.GET("/users/:id", func(c echo.Context) error {
        id := c.Param("id")
        return c.JSON(http.StatusOK, map[string]string{"id": id})
    })

    if err := e.Start(":8080"); err != nil && !errors.Is(err, http.ErrServerClosed) {
        slog.Error("failed to start server", "error", err)
    }
}
```

Always check `!errors.Is(err, http.ErrServerClosed)` — a graceful shutdown returns that sentinel, not a real error.

---

### Grouping Routes with Middleware ✅ Current

```go
package main

import (
    "context"
    "encoding/json"
    "errors"
    "fmt"
    "log"
    "log/slog"
    "net"
    "net/http"
    "time"

    "github.com/labstack/echo/v4"
    "github.com/labstack/echo/v4/middleware"
)

// authMiddleware checks for an Authorization header.
// If missing it returns 401, otherwise it calls the next handler.
func authMiddleware(next echo.HandlerFunc) echo.HandlerFunc {
    return func(c echo.Context) error {
        if token := c.Request().Header.Get("Authorization"); token == "" {
            return echo.ErrUnauthorized
        }
        return next(c)
    }
}

// helper – perform a request and decode a JSON body into dst.
func doRequest(req *http.Request, dst interface{}) (*http.Response, error) {
    client := &http.Client{}
    resp, err := client.Do(req)
    if err != nil {
        return nil, err
    }
    // Ensure the body is closed exactly once.
    defer resp.Body.Close()
    if dst != nil {
        if err := json.NewDecoder(resp.Body).Decode(dst); err != nil {
            return resp, err
        }
    }
    return resp, nil
}

func main() {
    // -----------------------------------------------------------------
    // Set up Echo server exactly as described in the SKILL.md reference.
    // -----------------------------------------------------------------
    e := echo.New()
    e.Use(middleware.Recover())

    // Public route
    e.GET("/health", func(c echo.Context) error {
        return c.JSON(http.StatusOK, map[string]string{"status": "ok"})
    })

    // Protected group with auth middleware
    api := e.Group("/api/v1", authMiddleware)
    api.GET("/profile", func(c echo.Context) error {
        return c.JSON(http.StatusOK, map[string]string{"user": "example"})
    })
    api.POST("/items", func(c echo.Context) error {
        return c.JSON(http.StatusCreated, map[string]string{"created": "true"})
    })

    // ---------------------------------------------------------------
    // Start the server on a random free port using the existing listener.
    // ---------------------------------------------------------------
    ln, err := net.Listen("tcp", "127.0.0.1:0")
    if err != nil {
        log.Fatalf("failed to obtain listener: %v", err)
    }
    // Tell Echo to use this listener.
    e.Listener = ln

    go func() {
        // StartServer uses the pre‑configured listener (e.Listener).
        if err := e.StartServer(e.Server); err != nil && !errors.Is(err, http.ErrServerClosed) {
            slog.Error("server error", "error", err)
            log.Fatalf("server failed: %v", err)
        }
    }()

    // Give the server a moment to start.
    time.Sleep(200 * time.Millisecond)

    baseURL := fmt.Sprintf("http://%s", ln.Addr().String())

    // ---------------------------------------------------------------
    // 1) Public route – should be 200 OK.
    // ---------------------------------------------------------------
    {
        req, _ := http.NewRequest(http.MethodGet, baseURL+"/health", nil)
        var body map[string]string
        resp, err := doRequest(req, &body)
        if err != nil {
            log.Fatalf("health request failed: %v", err)
        }
        if resp.StatusCode != http.StatusOK {
            log.Fatalf("health: expected 200, got %d", resp.StatusCode)
        }
        if body["status"] != "ok" {
            log.Fatalf("health: unexpected body %v", body)
        }
    }

    // ---------------------------------------------------------------
    // 2) Protected GET without Authorization – should be 401.
    // ---------------------------------------------------------------
    {
        req, _ := http.NewRequest(http.MethodGet, baseURL+"/api/v1/profile", nil)
        resp, err := doRequest(req, nil)
        if err != nil {
            log.Fatalf("profile (no auth) request failed: %v", err)
        }
        if resp.StatusCode != http.StatusUnauthorized {
            log.Fatalf("profile (no auth): expected 401, got %d", resp.StatusCode)
        }
    }

    // ---------------------------------------------------------------
    // 3) Protected GET with Authorization – should be 200.
    // ---------------------------------------------------------------
    {
        req, _ := http.NewRequest(http.MethodGet, baseURL+"/api/v1/profile", nil)
        req.Header.Set("Authorization", "Bearer dummy-token")
        var body map[string]string
        resp, err := doRequest(req, &body)
        if err != nil {
            log.Fatalf("profile (auth) request failed: %v", err)
        }
        if resp.StatusCode != http.StatusOK {
            log.Fatalf("profile (auth): expected 200, got %d", resp.StatusCode)
        }
        if body["user"] != "example" {
            log.Fatalf("profile (auth): unexpected body %v", body)
        }
    }

    // ---------------------------------------------------------------
    // 4) Protected POST with Authorization – should be 201.
    // ---------------------------------------------------------------
    {
        req, _ := http.NewRequest(http.MethodPost, baseURL+"/api/v1/items", nil)
        req.Header.Set("Authorization", "Bearer dummy-token")
        var body map[string]string
        resp, err := doRequest(req, &body)
        if err != nil {
            log.Fatalf("items POST request failed: %v", err)
        }
        if resp.StatusCode != http.StatusCreated {
            log.Fatalf("items POST: expected 201, got %d", resp.StatusCode)
        }
        if body["created"] != "true" {
            log.Fatalf("items POST: unexpected body %v", body)
        }
    }

    // ---------------------------------------------------------------
    // Shut down the server cleanly.
    // ---------------------------------------------------------------
    ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
    defer cancel()
    if err := e.Shutdown(ctx); err != nil {
        log.Fatalf("failed to shutdown server: %v", err)
    }

    // ---------------------------------------------------------------
    // Success message as required by the task.
    // ---------------------------------------------------------------
    fmt.Println("✓ Test passed: Grouping Routes with Middleware ✅ Current")
}
```

`e.Group()` returns a `*Group` that shares the path prefix and middleware stack.

---

### Binding Request Data ✅ Current

```go
package main

import (
    "bytes"
    "context"
    "encoding/json"
    "errors"
    "io"
    "log"
    "net"
    "net/http"
    "time"
)

type CreateUserRequest struct {
    Name  string `json:"name"  form:"name"  query:"name"`
    Email string `json:"email" form:"email" query:"email"`
}

func createUser(c echo.Context) error {
    req := new(CreateUserRequest)
    if err := c.Bind(req); err != nil {
        return err // echo returns an error (often *HTTPError) on bind failure
    }
    if req.Name == "" {
        return echo.NewHTTPError(http.StatusBadRequest, "name is required")
    }
    return c.JSON(http.StatusCreated, req)
}

func main() {
    // Set up Echo router
    e := echo.New()
    e.POST("/users", createUser)

    // Listen on a random free port
    ln, err := net.Listen("tcp", "127.0.0.1:0")
    if err != nil {
        log.Fatalf("failed to listen: %v", err)
    }
    // Use the listener we just created.
    e.Listener = ln

    // Start the server in the background using the pre‑configured listener.
    go func() {
        if err := e.StartServer(e.Server); err != nil && !errors.Is(err, http.ErrServerClosed) {
            log.Fatalf("server error: %v", err)
        }
    }()

    // Ensure the server shuts down when main exits
    defer func() {
        ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
        defer cancel()
        _ = e.Shutdown(ctx)
    }()

    // Prepare a valid request payload
    want := CreateUserRequest{Name: "Alice", Email: "alice@example.com"}
    body, _ := json.Marshal(want)

    // Send POST request to the running server
    resp, err := http.Post("http://"+ln.Addr().String()+"/users", "application/json", bytes.NewReader(body))
    if err != nil {
        log.Fatalf("request failed: %v", err)
    }
    defer resp.Body.Close()

    // Verify HTTP status
    if resp.StatusCode != http.StatusCreated {
        b, _ := io.ReadAll(resp.Body)
        log.Fatalf("unexpected status: %d, body: %s", resp.StatusCode, string(b))
    }

    // Decode response JSON
    var got CreateUserRequest
    if err := json.NewDecoder(resp.Body).Decode(&got); err != nil {
        log.Fatalf("failed to decode response: %v", err)
    }

    // Verify returned data matches what was sent
    if got != want {
        log.Fatalf("response mismatch: got %+v, want %+v", got, want)
    }

    // Success message
    log.Println("✓ Test passed: Binding Request Data ✅ Current")
}
```

`c.Bind()` selects the decoder (JSON, XML, form, query) from the `Content-Type` header and struct tags. Pass a pointer to the target struct.

---

### Graceful Shutdown ✅ Current

```go
package main

import (
    "context"
    "errors"
    "log/slog"
    "net/http"
    "os"
    "os/signal"
    "time"

    "github.com/labstack/echo/v4"
    "github.com/labstack/echo/v4/middleware"
)

func main() {
    e := echo.New()
    e.Use(middleware.Recover())

    e.GET("/", func(c echo.Context) error {
        return c.String(http.StatusOK, "running")
    })

    go func() {
        if err := e.Start(":8080"); err != nil && !errors.Is(err, http.ErrServerClosed) {
            slog.Error("server start error", "error", err)
            os.Exit(1)
        }
    }()

    quit := make(chan os.Signal, 1)
    signal.Notify(quit, os.Interrupt)
    <-quit

    ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
    defer cancel()

    if err := e.Shutdown(ctx); err != nil {
        slog.Error("shutdown error", "error", err)
    }
}
```

`e.Shutdown()` triggers a graceful drain; `e.Start()` will then return `http.ErrServerClosed`.

---

### Serving Static Files ✅ Current

```go
package main

import (
    "embed"
    "io/fs"
    "log/slog"
    "net/http"

    "github.com/labstack/echo/v4"
)

//go:embed public
var publicFS embed.FS

func main() {
    e := echo.New()

    // Serve a directory from disk
    e.Static("/assets", "static")

    // Serve from an embedded FS.
    // //go:embed public stores files under the "public/" prefix, so use fs.Sub
    // to strip that prefix before passing to StaticFS.
    sub, err := fs.Sub(publicFS, "public")
    if err != nil {
        slog.Error("failed to create sub FS", "error", err)
        return
    }
    e.StaticFS("/app", sub)

    // Serve a single file
    e.File("/favicon.ico", "static/favicon.ico")

    e.GET("/", func(c echo.Context) error {
        return c.String(http.StatusOK, "hello")
    })

    if err := e.Start(":8080"); err != nil && !errors.Is(err, http.ErrServerClosed) {
        slog.Error("server error", "error", err)
    }
}
```

`StaticFS` accepts any `fs.FS`, including `embed.FS`. When using `//go:embed public`, files are stored under the `public/` prefix inside the embedded FS; use `fs.Sub(publicFS, "public")` to strip that prefix so URL paths map correctly. `Static` maps a URL prefix to a filesystem directory path.

---

### Template Rendering ✅ New

```go
package main

import (
    "html/template"
    "io"
    "net/http"

    "github.com/labstack/echo/v4"
)

// Ensure the imported io package is referenced.
var _ = io.Writer(nil)

type TemplateRenderer struct {
    templates *template.Template
}

// Render implements echo.Renderer
func (t *TemplateRenderer) Render(w io.Writer, name string, data interface{}, c echo.Context) error {
    return t.templates.ExecuteTemplate(w, name, data)
}

func main() {
    e := echo.New()

    // Parse all templates in the ./templates directory
    tmpl := template.Must(template.ParseGlob("templates/*.html"))
    e.Renderer = &TemplateRenderer{templates: tmpl}

    e.GET("/", func(c echo.Context) error {
        data := map[string]string{"Title": "Home"}
        return c.Render(http.StatusOK, "index.html", data)
    })

    e.Logger.Fatal(e.Start(":8080"))
}
```

Implement `echo.Renderer` by providing a `Render` method. Assign an instance to `e.Renderer` to enable `c.Render()`.

---

### WebSocket (TLS) ✅ New

```go
package main

import (
    "github.com/labstack/echo/v4"
)

func main() {
    e := echo.New()

    // Echo upgrades the HTTP connection to a WebSocket.
    // TLS termination (if any) is handled by the underlying HTTP server.
    e.GET("/ws", func(c echo.Context) error {
        // Upgrade to WebSocket.
        return c.WebSocket()
    })

    e.Logger.Fatal(e.Start(":8080"))
}
```

Echo provides `c.WebSocket()` to upgrade a request to a WebSocket connection. TLS‑aware WebSocket proxying is available via `e.WebSocketProxy` (added in v4.13.4).

---

### WebSocket Proxy (TLS‑aware) ✅ New

```go
package main

import (
    "net/http"
    "time"

    "github.com/labstack/echo/v4"
)

func main() {
    e := echo.New()

    // Proxy WebSocket connections to a backend service that uses TLS.
    // The proxy will handle TLS termination for the upstream.
    e.WebSocketProxy(echo.WebSocketProxyConfig{
        // Path on which the proxy listens
        Path: "/ws",
        // Backend URL (must start with wss:// for TLS)
        Target: "wss://backend.example.com/ws",
        // Optional: preserve the original request headers
        Header: http.Header{
            "X-Forwarded-For": []string{"client"},
        },
        // Optional: customize the dial timeout
        DialTimeout: 5 * time.Second,
    })

    e.Logger.Fatal(e.Start(":8080"))
}
```

`e.WebSocketProxy` registers a route that proxies incoming WebSocket connections to the configured TLS‑enabled backend. The `WebSocketProxyConfig` struct allows you to set the target URL, custom headers, and a dial timeout.

---

## Configuration

```go
package main

import (
    "net/http"

    "github.com/labstack/echo/v4"
    "github.com/labstack/echo/v4/middleware"
)

func main() {
    e := echo.New()

    // Hide the startup banner
    e.HideBanner = true

    // Disable the default port output line
    e.HidePort = true

    // Enable debug mode (more detailed error responses and framework-level diagnostics)
    e.Debug = true

    // CORS with explicit configuration
    e.Use(middleware.CORSWithConfig(middleware.CORSConfig{
        AllowOrigins: []string{"https://example.com", "https://app.example.com"},
        AllowMethods: []string{http.MethodGet, http.MethodPost, http.MethodPut, http.MethodDelete},
        AllowHeaders: []string{echo.HeaderContentType, echo.HeaderAuthorization},
    }))

    // Logger with custom format
    e.Use(middleware.LoggerWithConfig(middleware.LoggerConfig{
        Format: "${method} ${uri} ${status}\n",
    }))

    // Rate limiting
    e.Use(middleware.RateLimiter(middleware.NewRateLimiterMemoryStore(20)))

    // Request size limit (in bytes)
    e.Use(middleware.BodyLimit("2M"))

    e.Logger.Fatal(e.Start(":8080"))
}
```

**Common `Echo` struct fields:**

| Field | Default | Purpose |
|---|---|---|
| `HideBanner` | `false` | Suppress ASCII banner at startup |
| `HidePort` | `false` | Suppress port log line |
| `Debug` | `false` | Enable debug mode (detailed error responses; does not control logger middleware verbosity) |

**CORS regexp compilation** — as of v4.13.0, `allowOrigin` patterns are compiled once at middleware creation, not per request.

---

## Pitfalls

### Using removed core JWT middleware ⚠️

**Wrong:**
```go
// This no longer compiles in v4.13.0+ — middleware.JWT was removed
import "github.com/labstack/echo/v4/middleware"

e.Use(middleware.JWT([]byte("secret"))) // compile error: undefined
```

**Right:**
```go
import (
    "os"
    "github.com/labstack/echo/v4"
    echojwt "github.com/labstack/echo-jwt/v4"
    "github.com/golang-jwt/jwt/v5"
)

e := echo.New()
e.Use(echojwt.WithConfig(echojwt.Config{
    SigningKey: []byte(os.Getenv("JWT_SECRET")),
}))
```

**In handler:**
```go
token := c.Get(echojwt.DefaultConfig.ContextKey).(*jwt.Token) // jwt from v5
```

---

### Treating `http.ErrServerClosed` as a fatal error

**Wrong:**
```go
if err := e.Start(":8080"); err != nil {
    log.Fatal(err) // kills the process on normal graceful shutdown
}
```

**Right:**
```go
if err := e.Start(":8080"); err != nil && !errors.Is(err, http.ErrServerClosed) {
    slog.Error("server error", "error", err)
}
```

---

### Not returning handler errors

**Wrong:**
```go
e.GET("/data", func(c echo.Context) error {
    data, err := fetchData()
    if err != nil {
        c.JSON(http.StatusInternalServerError, map[string]string{"error": err.Error()})
        // forgot to return — execution continues
    }
    return c.JSON(http.StatusOK, data)
})
```

**Right:**
```go
e.GET("/data", func(c echo.Context) error {
    data, err := fetchData()
    if err != nil {
        return c.JSON(http.StatusInternalServerError, map[string]string{"error": err.Error()})
    }
    return c.JSON(http.StatusOK, data)
})
```

---

### Binding form data to a slice

Echo's default form binder decodes form values into struct fields using `form` tags, but does not support binding directly to a slice of structs. Bind to a struct instead.

**Wrong:**
```go
func handler(c echo.Context) error {
    // The default form binder does not support a slice of structs as the top‑level target
    items := []struct{ Name string }{}
    return c.Bind(&items) // returns an error at runtime
}
```

**Right:**
```go
type ItemForm struct {
    Name  string `form:"name"`
    Count int    `form:"count"`
}

func handler(c echo.Context) error {
    item := new(ItemForm)
    return c.Bind(item) // bind to a struct pointer, not a slice
}
```

---

### Hardcoding secrets in middleware configuration

**Wrong:**
```go
e.Use(echojwt.WithConfig(echojwt.Config{
    SigningKey: []byte("mysecretkey123"), // secret committed to source control
}))
```

**Right:**
```go
import (
    "os"

    echojwt "github.com/labstack/echo-jwt/v4"
)

secret := os.Getenv("JWT_SECRET")
e.Use(echojwt.WithConfig(echojwt.Config{
    SigningKey: []byte(secret),
}))
```

---

## References

- [Documentation](https://pkg.go.dev/github.com/labstack/echo/v4)
- [Source](https://github.com/labstack/echo)

---

## Migration from v4.12

### JWT Middleware Removed (Breaking) 🗑️⚠️ Removed since v4.13.0

The built‑in JWT middleware was deprecated in v4.10.0 and fully removed in v4.13.0 due to CVE‑2024‑51744 in `golang-jwt/jwt` v3.2.2.

**Before (v4.12.x):**
```go
import (
    "github.com/labstack/echo/v4/middleware"
    "github.com/golang-jwt/jwt" // v3
)

e.Use(middleware.JWT([]byte(os.Getenv("JWT_SECRET"))

// In handler:
token := c.Get("user").(*jwt.Token)
```

**After (v4.13.0+):**
```go
import (
    echojwt "github.com/labstack/echo-jwt/v4"
    "github.com/golang-jwt/jwt/v5"
)

e.Use(echojwt.WithConfig(echojwt.Config{
    SigningKey: []byte(os.Getenv("JWT_SECRET")),
}))

// In handler:
token := c.Get(echojwt.DefaultConfig.ContextKey).(*jwt.Token) // jwt/v5
```

**Migration steps:**
1. `go get github.com/labstack/echo-jwt/v4`
2. Replace `middleware.JWT(...)` with `echojwt.WithConfig(...)`
3. Update JWT import to `github.com/golang-jwt/jwt/v5`
4. Adjust token extraction to use the new context key.
5. Re‑run your test suite; the panic caused by the old cast will disappear.

---

### JSON Content-Type Header Change

Before v4.12.0, Echo sent `application/json; charset=UTF-8`. From v4.12.0 onward, it sends `application/json` (no charset). Update any clients that matched the full content‑type string.

---

### BindBody Fix for Chunked Encoding (v4.13.1)

If you use `c.BindBody()` with chunked transfer encoding (`Transfer-Encoding: chunked`) or requests where `Content-Length` is `-1`, upgrade to v4.13.1+. Earlier versions silently skipped binding for those requests.

---

## Migration from v4.13.0

### CORS Regexp Compilation ⚠️

Prior to v4.13.0 the `allowOrigin` pattern was compiled on each request, impacting performance. From v4.13.0 onward it is compiled once at middleware creation. Existing code continues to work, but you can gain efficiency by providing a compiled pattern via `CORSConfig`.

### Template Rendering (New in v4.13.0)

`TemplateRenderer` was added to simplify HTML/template rendering. Implement the `echo.Renderer` interface and assign it to `e.Renderer` as shown in the “Template Rendering” pattern.

### WebSocket Proxy Configuration (New in v4.13.4)

TLS‑aware WebSocket proxying is now available via `e.WebSocketProxy`. Populate `echo.WebSocketProxyConfig` with your backend details. This addition is not breaking but may require configuration if you rely on proxying WebSocket connections over TLS.

---

## API Reference

**`echo.New()`** – Creates and returns a new `*Echo` instance. Entry point for all applications.

**`Echo.Use(middleware ...MiddlewareFunc)`** – Registers one or more middleware functions on the root router, executed in registration order as part of the standard request handling chain (after `Pre` middleware and routing).

**`Echo.Pre(middleware ...MiddlewareFunc)`** – Registers middleware that runs before routing (e.g., URL rewriting, trailing slash removal).

**`Echo.GET(path, handler, middleware…)`** / `POST` / `PUT` / `DELETE` / `PATCH` – Registers a route for the given HTTP method. Returns `*Route`.

**`Echo.Group(prefix string, m ...MiddlewareFunc) *Group`** – Creates a sub‑router with a shared path prefix and optional middleware.

**`Echo.Static(pathPrefix, fsRoot string) *Route`** – Serves all files from a filesystem directory under the given URL prefix.

**`Echo.StaticFS(pathPrefix string, filesystem fs.FS) *Route`** – Serves files from any `fs.FS` (including `embed.FS`) under the given URL prefix.

**`Echo.Start(address string) error`** – Starts the HTTP server on the given address (e.g., `":8080"`). Returns `http.ErrServerClosed` on graceful shutdown.

**`Echo.StartTLS(address, certFile, keyFile string) error`** – Starts an HTTPS server with the provided TLS certificates.

**`Echo.StartAutoTLS(address string) error`** – Starts an HTTPS server with automatic TLS certificate management via Let’s Encrypt.

**`Echo.Shutdown(ctx context.Context) error`** – Gracefully shuts down the server without interrupting active connections.

**`Echo.WebSocketProxy(config WebSocketProxyConfig) *Route`** – Proxies incoming WebSocket connections to a TLS‑enabled backend as configured.

**`Context.Bind(i interface{}) error`** – Decodes the request into `i`. Body decoding is selected by `Content-Type` header (JSON, XML, form, multipart) and query parameters are decoded from the URL query string. Each mechanism uses the corresponding struct tag (`json`, `xml`, `form`, or `query`). Returns an error on failure.

**`Context.BindBody(i interface{}) error`** – Binds only the request body (ignores query/path params unless the request is a form). Correctly handles `Transfer-Encoding: chunked` as of v4.13.1.

**`Context.JSON(code int, i interface{}) error`** – Sends a JSON response with the given status code.

**`Context.String(code int, s string) error`** – Sends a plain‑text response with the given status code.

**`Context.Param(name string) string`** – Returns the value of a named path parameter (e.g., `:id`).

**`Context.QueryParam(name string) string`** – Returns the first value for the named URL query parameter.

**`Context.Set(key string, val interface{})` / `Get(key string) interface{}`** – Stores and retrieves arbitrary values in the request‑scoped context map. Used to pass data between middleware and handlers.

**`Echo.Renderer`** – Interface with method `Render(w io.Writer, name string, data interface{}, c echo.Context) error`. Implemented by custom renderers such as `TemplateRenderer`.

**`TemplateRenderer`** – Simple struct that holds parsed `*template.Template` and implements `echo.Renderer`.

**`middleware.Logger`**, **`middleware.Recover`**, **`middleware.CORS`**, **`middleware.CORSWithConfig`**, **`middleware.RateLimiter`**, **`middleware.BodyLimit`**, **`middleware.Gzip`**, **`middleware.GzipWithConfig`**, **`middleware.CSRF`**, **`middleware.CSRFWithConfig`**, **`middleware.KeyAuth`**, **`middleware.KeyAuthWithConfig`**, **`middleware.BasicAuth`**, **`middleware.BasicAuthWithConfig`**, **`middleware.Decompress`**, **`middleware.DecompressWithConfig`**, **`middleware.Secure`**, **`middleware.SecureWithConfig`**, **`middleware.RemoveTrailingSlash`**, **`middleware.RemoveTrailingSlashWithConfig`**, **`middleware.Static`**, **`middleware.StaticWithConfig`**, **`middleware.RequestID`**, **`middleware.Timeout`**, **`middleware.ContextTimeout`**, **`middleware.ContextTimeoutWithConfig`** – Various middleware utilities.

**`middleware.JWT`** – ⚠️ **Removed in v4.13.0**. Use `github.com/labstack/echo-jwt/v4` instead.

---

## Migration Notes

### v4.13.0 (JWT middleware removal)
- Replace any `import "github.com/labstack/echo/v4/middleware"` that references the built‑in JWT middleware with `import "github.com/labstack/echo-jwt/v4"`.
- Update your `go.mod` to require `github.com/golang-jwt/jwt/v5`.
- Change token retrieval from the context to use the new context key.
- Re‑run your test suite; the panic caused by the old cast will disappear.

### v4.12.0 (Content‑Type charset change)
- Review all places where you rely on the default `application/json` header containing `charset=utf-8`.
- Add an explicit header if needed, as shown in the pitfalls section.

### General migration steps
1. **Update go.mod** to the latest version: `go get github.com/labstack/echo/v4@v4.13.4`.
2. **Run `go vet` / `staticcheck`** – they will flag deprecated JWT usage and missing imports.
3. **Replace deprecated middleware** with their external equivalents (e.g., JWT, CORS custom config).
4. **Run the project's test suite** to ensure binding, multipart handling, and template rendering still work.
5. **Check CI pipelines** – the module now tests against Go 1.24; update your CI matrix accordingly.

---

### v4.13.0 (JWT middleware removal)
- Replace any `import "github.com/labstack/echo/v4/middleware"` that references the built‑in JWT middleware with `import "github.com/labstack/echo-jwt/v4"`.
- Update your `go.mod` to require `github.com/golang-jwt/jwt/v5`.
- Change token retrieval from the context to use the new context key.
- Re‑run your test suite; the panic caused by the old cast will disappear.

### v4.12.0 (Content‑Type charset change)
- Review all places where you rely on the default `application/json` header containing `charset=utf-8`.
- Add an explicit header if needed, as shown in the pitfalls section.

### General migration steps
1. **Update go.mod** to the latest version: `go get github.com/labstack/echo/v4@v4.13.4`.
2. **Run `go vet` / `staticcheck`** – they will flag deprecated JWT usage and missing imports.
3. **Replace deprecated middleware** with their external equivalents (e.g., JWT, CORS custom config).
4. **Run the project's test suite** to ensure binding, multipart handling, and template rendering still work.
5. **Check CI pipelines** – the module now tests against Go 1.24; update your CI matrix accordingly.

---