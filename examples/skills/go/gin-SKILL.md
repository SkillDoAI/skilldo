---
name: gin
description: Gin is a high‑performance HTTP web framework written in Go, offering a Martini‑like API with a focus on speed and simplicity.
license: MIT
metadata:
  version: "1.9.1"
  ecosystem: go
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```go
import (
    "github.com/gin-gonic/gin"
)
```

The above is the only required import for Gin itself. Most handlers and examples also require standard library packages depending on what they use:

```go
import (
    "html/template" // when passing *template.Template to Engine.SetHTMLTemplate
    "io"            // for io.ReadAll (replaces deprecated ioutil.ReadAll)
    "net/http"      // for http.Status* constants and http.ResponseWriter
    "path/filepath" // for filepath.Base and filepath.Join (file upload safety)
    "strings"       // for strings.ReplaceAll (normalise backslashes in uploaded filenames)

    "github.com/gin-gonic/gin"
)
```

## Core Patterns

### Create Engine and Register Routes ✅ Current

`gin.Default()` returns an engine with Logger and Recovery middleware attached. Use `gin.New()` for a blank engine. Routes are registered via HTTP‑method methods on the engine or a `RouterGroup`.

```go
package main

import (
    "net/http"

    "github.com/gin-gonic/gin"
)

func main() {
    gin.SetMode(gin.ReleaseMode) // set before creating engine in production

    r := gin.Default() // Logger + Recovery middleware included

    r.GET("/ping", func(c *gin.Context) {
        c.JSON(http.StatusOK, gin.H{"message": "pong"})
    })

    r.POST("/users", func(c *gin.Context) {
        var body struct {
            Name string `json:"name" binding:"required"`
        }
        if err := c.ShouldBindJSON(&body); err != nil {
            c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
            return
        }
        c.JSON(http.StatusCreated, gin.H{"name": body.Name})
    })

    if err := r.Run(":8080"); err != nil {
        panic(err)
    }
}
```

### Route Groups and Middleware ✅ Current

Use `RouterGroup.Group()` to share path prefixes and middleware. Middleware registered on a group applies only to that group's routes.

```go
package main

import (
    "net/http"

    "github.com/gin-gonic/gin"
)

func AuthRequired() gin.HandlerFunc {
    return func(c *gin.Context) {
        token := c.GetHeader("Authorization")
        if token == "" {
            c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "unauthorized"})
            return
        }
        c.Next()
    }
}

func main() {
    r := gin.New()
    r.Use(gin.Logger(), gin.Recovery())

    // Public routes
    r.GET("/health", func(c *gin.Context) {
        c.JSON(http.StatusOK, gin.H{"status": "ok"})
    })

    // Protected API group
    api := r.Group("/api/v1")
    api.Use(AuthRequired())
    {
        api.GET("/profile", func(c *gin.Context) {
            c.JSON(http.StatusOK, gin.H{"user": "alice"})
        })
        api.DELETE("/account", func(c *gin.Context) {
            c.Status(http.StatusNoContent)
        })
    }

    if err := r.Run(":8080"); err != nil {
        panic(err)
    }
}
```

### URL Parameters and Query Strings ✅ Current

Named parameters use `:name` syntax; wildcard parameters use `*name`. Query string values are read via `c.Query()` and `c.DefaultQuery()`.

```go
package main

import (
    "net/http"

    "github.com/gin-gonic/gin"
)

func main() {
    r := gin.Default()

    // URL param: /users/42
    r.GET("/users/:id", func(c *gin.Context) {
        id := c.Param("id")
        verbose := c.DefaultQuery("verbose", "false")
        c.JSON(http.StatusOK, gin.H{"id": id, "verbose": verbose})
    })

    // Wildcard: /files/docs/report.pdf
    r.GET("/files/*filepath", func(c *gin.Context) {
        fp := c.Param("filepath")
        c.String(http.StatusOK, "requested: %s", fp)
    })

    if err := r.Run(":8080"); err != nil {
        panic(err)
    }
}
```

### Request Binding and Validation ✅ Current

Use `ShouldBind*` methods for manual error handling. Use `Bind*` only when you want Gin to automatically write a 400 response on failure. Struct tags `binding:"required"` drive validation.

```go
package main

import (
    "net/http"

    "github.com/gin-gonic/gin"
)

type CreateItemRequest struct {
    Name  string  `json:"name"  binding:"required,max=64"`
    Price float64 `json:"price" binding:"required,gt=0"`
    Tags  []string `json:"tags"`
}

type ItemQuery struct {
    Page  int `form:"page"  binding:"min=1"`
    Limit int `form:"limit" binding:"min=1,max=100"`
}

func main() {
    r := gin.Default()

    r.POST("/items", func(c *gin.Context) {
        var req CreateItemRequest
        if err := c.ShouldBindJSON(&req); err != nil {
            c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
            return
        }
        c.JSON(http.StatusCreated, gin.H{"name": req.Name, "price": req.Price})
    })

    r.GET("/items", func(c *gin.Context) {
        var q ItemQuery
        if err := c.ShouldBindQuery(&q); err != nil {
            c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
            return
        }
        c.JSON(http.StatusOK, gin.H{"page": q.Page, "limit": q.Limit})
    })

    if err := r.Run(":8080"); err != nil {
        panic(err)
    }
}
```

### HTML Templates and Static Files ✅ Current

Load templates with `LoadHTMLGlob` or `LoadHTMLFiles` before starting the server. Serve static directories with `Static` and individual files with `StaticFile`.

```go
package main

import (
    "net/http"

    "github.com/gin-gonic/gin"
)

func main() {
    r := gin.Default()

    r.LoadHTMLGlob("templates/*")          // load all *.html from templates/
    r.Static("/assets", "./static")        // serve ./static at /assets
    r.StaticFile("/favicon.ico", "./static/favicon.ico")

    r.GET("/", func(c *gin.Context) {
        c.HTML(http.StatusOK, "index.html", gin.H{
            "title": "Home",
        })
    })

    if err := r.Run(":8080"); err != nil {
        panic(err)
    }
}
```

### Register Multiple Methods with `Handle` ✅ Updated

Gin does not provide a `Match` method. To bind a single handler to several HTTP methods, call `router.Handle` for each method (or use `router.Any` when you truly want to accept all methods).

```go
package main

import (
    "net/http"

    "github.com/gin-gonic/gin"
)

func main() {
    r := gin.Default()

    handler := func(c *gin.Context) {
        c.JSON(http.StatusOK, gin.H{"msg": "handled"})
    }

    // Register the same handler for GET and POST on /search
    r.Handle(http.MethodGet, "/search", handler)
    r.Handle(http.MethodPost, "/search", handler)

    // Alternatively, accept any method:
    // r.Any("/search", handler)

    if err := r.Run(":8080"); err != nil {
        panic(err)
    }
}
```

## Configuration

```go
package main

import (
    "github.com/gin-gonic/gin"
)

func main() {
    // Mode: gin.DebugMode (default), gin.ReleaseMode, gin.TestMode
    // Set via code or GIN_MODE environment variable before engine creation
    gin.SetMode(gin.ReleaseMode)

    r := gin.New()

    // Multipart memory limit (default: 32 MiB)
    r.MaxMultipartMemory = 8 << 20 // 8 MiB

    // Redirect /path/ to /path (default: true)
    r.RedirectTrailingSlash = true

    // Attempt to fix the path before returning 404 (default: false)
    r.RedirectFixedPath = false

    // Return 405 instead of 404 when method not allowed (default: false)
    r.HandleMethodNotAllowed = true

    // Trust only a specific proxy; pass nil to disable proxy trust entirely
    if err := r.SetTrustedProxies([]string{"192.168.1.1"}); err != nil {
        panic(err)
    }

    // Fall back to request.Context() for key lookups (default: false)
    r.ContextWithFallback = true

    // Custom delimiters for HTML templates
    r.Delims("{{", "}}")

    if err := r.Run(":8080"); err != nil {
        panic(err)
    }
}
```

**Build tags** (set at compile time):

| Tag | Effect |
|-----|--------|
| `jsoniter` | Use json-iterator instead of `encoding/json` |
| `go_json` | Use `goccy/go-json` |
| `sonic` + `avx` | Use `bytedance/sonic` (both tags required; also requires AVX CPU) |
| `nomsgpack` | Disable MsgPack rendering to reduce binary size |

**Package‑level helpers** (call before engine creation):

```go
gin.EnableJsonDecoderUseNumber()                     // decode JSON numbers as json.Number
gin.EnableJsonDecoderDisallowUnknownFields()        // reject unknown JSON fields
gin.DisableBindValidation()                         // disable struct validation on Bind
gin.DisableConsoleColor()                           // turn off ANSI color in logs
gin.ForceConsoleColor()                             // force ANSI color even when not a TTY
```

## Pitfalls

### Using `*gin.Context` directly in a goroutine

**Wrong** — the context is reused by the framework after the handler returns, causing data races:

```go
func handler(c *gin.Context) {
    go func() {
        // c may already be recycled — data race!
        path := c.Request.URL.Path
        _ = path
    }()
}
```

**Right** — call `c.Copy()` before spawning the goroutine:

```go
func handler(c *gin.Context) {
    cCopy := c.Copy()
    go func() {
        path := cCopy.Request.URL.Path
        _ = path
    }()
}
```

### Ignoring the error from `Bind*` methods

**Wrong** — `BindJSON` writes a 400 automatically, but execution continues; invalid data is processed:

```go
func handler(c *gin.Context) {
    var req struct {
        Name string `json:"name" binding:"required"`
    }
    c.BindJSON(&req)        // error ignored
    process(req)            // called even when binding failed
}
```

**Right** — use `ShouldBindJSON` and return on error:

```go
func handler(c *gin.Context) {
    var req struct {
        Name string `json:"name" binding:"required"`
    }
    if err := c.ShouldBindJSON(&req); err != nil {
        c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
        return
    }
    process(req)
}
```

### Trusting all proxies (IP spoofing risk)

**Wrong** — without explicitly calling `SetTrustedProxies`, clients may be able to spoof IP via `X-Forwarded-For`; always configure trusted proxies explicitly:

```go
r := gin.Default()
// SetTrustedProxies never called — proxy trust behavior is uncontrolled
ip := c.ClientIP() // may return attacker‑controlled value
```

**Right** — explicitly configure trusted proxies:

```go
r := gin.Default()
if err := r.SetTrustedProxies([]string{"10.0.0.1"}); err != nil {
    panic(err)
}
// Or disable proxy trust entirely:
// r.SetTrustedProxies(nil)
```

### Using client‑supplied filenames for file uploads

**Wrong** — `file.Filename` is attacker‑controlled and may contain path traversal sequences:

```go
file, _ := c.FormFile("upload")
dst := "/var/uploads/" + file.Filename // path traversal risk: ../../etc/passwd
c.SaveUploadedFile(file, dst)
```

**Right** — normalise backslashes (for Windows‑originated names) then strip all path components before use:

```go
import (
    "path/filepath"
    "strings"
)

file, err := c.FormFile("upload")
if err != nil {
    c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
    return
}
normalized := strings.ReplaceAll(file.Filename, `\`, "/")
safeName := filepath.Base(normalized)
dst := filepath.Join("/var/uploads", safeName)
if err := c.SaveUploadedFile(file, dst); err != nil {
    c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
    return
}
c.Status(http.StatusCreated)
```

### Running `gin.Default()` in production without setting release mode

**Wrong** — debug logging degrades performance and leaks route info:

```go
r := gin.Default() // starts in debug mode — logs every route registration
r.Run(":8080")
```

**Right** — set mode before creating the engine:

```go
gin.SetMode(gin.ReleaseMode) // or export GIN_MODE=release
r := gin.Default()
r.Run(":8080")
```

### ⚠️ Deprecated `HeaderMap` (removed in v1.8.1)

**Wrong** — using the removed `HeaderMap` field causes compilation errors:

```go
c.Request.HeaderMap.Set("X-My-Header", "value")
```

**Right** — use the standard header helpers:

```go
c.Header("X-My-Header", "value")               // set response header
c.Writer.Header().Set("X-My-Header", "value") // set request header if needed
```

### ⚠️ `RemoteIP` signature change (v1.8 → v1.8.1)

**Wrong** — old two‑value signature no longer compiles:

```go
ip, ok := c.RemoteIP()
if ok { /* ... */ }
```

**Right** — the method now returns only `net.IP`. Check for `nil`:

```go
ip := c.RemoteIP()
if ip != nil {
    // use ip
}
```

### ⚠️ Context fallback flag

**Wrong** — assuming Gin automatically falls back to `c.Request.Context()` without enabling the flag:

```go
ctx := c.Request.Context()
// expecting Gin to use this automatically inside handlers
```

**Right** — enable fallback on the engine (or pass the request context explicitly):

```go
engine := gin.Default()
engine.ContextWithFallback = true // enable fallback globally
// or within a handler:
c = c // (no per‑request method exists; use the engine flag)
```

## References

- [Documentation](https://pkg.go.dev/github.com/gin-gonic/gin)
- [Source](https://github.com/gin-gonic/gin)

## Migration from previous versions

### Upgrade to Gin **v1.10.0**

1. **Update `RemoteIP` usage** – replace any `ip, ok := c.RemoteIP()` with `ip := c.RemoteIP()` and guard with `if ip != nil`.
2. **Enable context fallback if needed** – set `engine.ContextWithFallback = true`. *(Gin does not provide a per‑request method for this.)*
3. **Replace deprecated `HeaderMap`** – switch to `c.Header()` / `c.GetHeader()` helpers.
4. **Gin does not provide a `router.Match` API.** To handle multiple HTTP methods, call `router.Handle` for each method (or use `router.Any` to accept all methods).
5. **Sanitize uploaded filenames** – always call `path.Base` (or a custom sanitizer) before `c.SaveUploadedFile`.
6. **Switch from `io/ioutil`** – use `os.ReadFile`, `os.WriteFile`, `io.ReadAll`, etc.
7. **Configure trusted proxies** – if you run behind a proxy, call `router.SetTrustedProxies([]string{"<cidr>"})` early in `main`.
8. **Set `router.MaxMultipartMemory`** when you expect large file uploads to avoid excessive memory consumption.
9. **If you relied on the old panic behaviour** (e.g., custom recovery that expects a panic on missing render), review and simplify – the framework now returns proper errors instead of panicking.
10. **Build‑tag JSON replacements** – optionally add `-tags=jsoniter`, `-tags=go_json`, or `-tags=sonic` to gain performance benefits.

All public APIs listed in `documented_apis` remain stable; only the signatures noted above have changed. Review the changelog for security fixes (e.g., filename escaping, header escaping) and ensure your code does not expose raw client‑provided filenames or headers.

## API Reference

**`gin.New() *Engine`** — creates a blank engine with no middleware attached.

**`gin.Default() *Engine`** — creates an engine with `Logger` and `Recovery` middleware pre‑attached.

**`gin.SetMode(value string)`** — sets the running mode; accepts `gin.DebugMode`, `gin.ReleaseMode`, `gin.TestMode`.

**`Engine.Run(addr ...string) error`** — starts an HTTP server; defaults to `:8080` when no address is given.

**`Engine.RunTLS(addr, certFile, keyFile string) error`** — starts an HTTPS server using the provided certificate and key files.

**`Engine.ServeHTTP(w http.ResponseWriter, req *http.Request)`** — implements `http.Handler`; use with `httptest.NewRecorder` in tests.

**`Engine.NoRoute(handlers ...HandlerFunc)`** — registers handlers called when no route matches (custom 404).

**`Engine.NoMethod(handlers ...HandlerFunc)`** — registers handlers called when the method is not allowed (custom 405).

**`Engine.SetHTMLTemplate(templ *template.Template)`** — sets a pre‑parsed template set for HTML rendering.

**`Engine.LoadHTMLGlob(pattern string)`** — loads and parses HTML templates matching a glob pattern.

**`RouterGroup.Group(relativePath string, handlers ...HandlerFunc) *RouterGroup`** — creates a sub‑group sharing a common prefix and optional middleware.

**`RouterGroup.Use(middleware ...HandlerFunc) IRoutes`** — attaches middleware to the group or engine.

**`RouterGroup.GET / POST / PUT / DELETE / PATCH(path string, handlers ...HandlerFunc) IRoutes`** — registers a route for the respective HTTP method.

**`RouterGroup.Handle(method, relativePath string, handlers ...HandlerFunc) IRoutes`** — registers a handler for a specific HTTP method (use multiple calls to support several methods).

**`Context.ShouldBindJSON(obj any) error`** — decodes the JSON request body into `obj`; returns an error without writing a response.

**`Context.JSON(code int, obj any)`** — writes a JSON response with the given HTTP status code.

**`Context.RemoteIP() net.IP`** — returns the client IP address (no boolean return; `nil` indicates unknown).  

*All other documented methods retain their signatures.*