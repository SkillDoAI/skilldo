---

name: zap
description: High-performance, structured logging library for Go with strongly-typed fields and optional printf-style sugar API.
license: MIT
metadata:
  version: "1.27.0"
  ecosystem: go
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```go
import (
    "go.uber.org/zap"
    "go.uber.org/zap/zapcore"
)

// For test helpers:
import "go.uber.org/zap/zaptest"
```

## Core Patterns

### Preset constructors ✅ Current

```go
package main

import "go.uber.org/zap"

func main() {
    // Production: JSON output, info level and above, caller info, stacktraces on warn+
    logger := zap.Must(zap.NewProduction())
    defer logger.Sync()

    // Development: console output, debug level, stacktraces on warn+
    devLogger := zap.Must(zap.NewDevelopment())
    defer devLogger.Sync()

    logger.Info("server started",
        zap.String("addr", ":8080"),
        zap.Int("pid", 42),
    )
    devLogger.Warn("config missing, using defaults")
}
```

Use `zap.Must` to panic on construction failure instead of ignoring errors. `NewProduction` and `NewDevelopment` return `(*Logger, error)`.

---

### Strongly-typed Logger for hot paths ✅ Current

```go
package main

import (
    "errors"
    "time"

    "go.uber.org/zap"
)

func main() {
    logger := zap.Must(zap.NewProduction())
    defer logger.Sync()

    // Add persistent context fields with With
    reqLogger := logger.With(
        zap.String("requestID", "abc-123"),
        zap.String("userID", "u-456"),
    )

    reqLogger.Info("fetching resource",
        zap.String("url", "https://example.com/api"),
        zap.Int("attempt", 1),
        zap.Duration("timeout", 5*time.Second),
    )

    err := errors.New("connection refused")
    reqLogger.Error("fetch failed",
        zap.String("url", "https://example.com/api"),
        zap.Error(err),
    )
}
```

`Logger.With` returns a child logger; it does not mutate the original. Use `zap.Error(err)` to log errors as structured fields.

---

### SugaredLogger for convenience ✅ Current

```go
package main

import (
    "time"

    "go.uber.org/zap"
)

func main() {
    logger := zap.Must(zap.NewProduction())
    defer logger.Sync()

    sugar := logger.Sugar()

    // Printf-style
    sugar.Infof("starting worker %d of %d", 1, 4)

    // Structured key-value pairs (Infow)
    sugar.Infow("request completed",
        "url", "https://example.com",
        "status", 200,
        "duration", 42*time.Millisecond,
    )

    // Convert back to Logger when needed
    plain := sugar.Desugar()
    plain.Info("back to typed fields", zap.Bool("ok", true))
}
```

`SugaredLogger` is 4–10x slower than `Logger`. Use it for non-critical paths; switch back with `Desugar()` for hot loops.

---

### Runtime log level changes with AtomicLevel ✅ Current

```go
package main

import (
    "os"

    "go.uber.org/zap"
    "go.uber.org/zap/zapcore"
)

func main() {
    atom := zap.NewAtomicLevelAt(zap.InfoLevel)

    encoderCfg := zap.NewProductionEncoderConfig()
    encoderCfg.TimeKey = ""

    logger := zap.New(zapcore.NewCore(
        zapcore.NewJSONEncoder(encoderCfg),
        zapcore.Lock(os.Stdout),
        atom,
    ))
    defer logger.Sync()

    logger.Info("info logging enabled")

    atom.SetLevel(zap.ErrorLevel)
    logger.Info("this is suppressed")
    logger.Error("only errors pass now")
}
```

`AtomicLevel` is thread-safe. Call `atom.SetLevel` at runtime (e.g., from an HTTP handler) without restarting the process.

---

### Custom multi-output configuration with zapcore ✅ Current

```go
package main

import (
    "io"
    "os"

    "go.uber.org/zap"
    "go.uber.org/zap/zapcore"
)

func main() {
    highPriority := zap.LevelEnablerFunc(func(lvl zapcore.Level) bool {
        return lvl >= zapcore.ErrorLevel
    })
    lowPriority := zap.LevelEnablerFunc(func(lvl zapcore.Level) bool {
        return lvl < zapcore.ErrorLevel
    })

    consoleEncoder := zapcore.NewConsoleEncoder(zap.NewDevelopmentEncoderConfig())
    jsonEncoder := zapcore.NewJSONEncoder(zap.NewProductionEncoderConfig())

    core := zapcore.NewTee(
        zapcore.NewCore(jsonEncoder, zapcore.AddSync(io.Discard), highPriority),
        zapcore.NewCore(consoleEncoder, zapcore.Lock(os.Stderr), highPriority),
        zapcore.NewCore(consoleEncoder, zapcore.Lock(os.Stdout), lowPriority),
    )

    logger := zap.New(core, zap.AddCaller())
    defer logger.Sync()

    logger.Info("low priority message")
    logger.Error("high priority message")
}
```

`zapcore.NewTee` fans out log entries to multiple cores. `zapcore.Lock` wraps any `io.Writer` with a mutex for concurrent use.

## Configuration

### Config struct with JSON unmarshaling

```go
package main

import (
    "encoding/json"

    "go.uber.org/zap"
)

func main() {
    rawJSON := []byte(`{
      "level": "info",
      "encoding": "json",
      "outputPaths": ["stdout"],
      "errorOutputPaths": ["stderr"],
      "initialFields": {"service": "my-app"},
      "encoderConfig": {
        "messageKey": "msg",
        "levelKey": "level",
        "timeKey": "ts",
        "levelEncoder": "lowercase",
        "timeEncoder": "iso8601"
      }
    }`)

    var cfg zap.Config
    if err := json.Unmarshal(rawJSON, &cfg); err != nil {
        panic(err)
    }

    logger := zap.Must(cfg.Build())
    defer logger.Sync()

    logger.Info("configuration loaded")
}
```

### Named presets as starting points

```go
// Production preset: JSON, info+, timestamps, caller info
cfg := zap.NewProductionConfig()
cfg.Level = zap.NewAtomicLevelAt(zap.DebugLevel) // override level
logger := zap.Must(cfg.Build())

// Development preset: console, debug+, colored levels
devCfg := zap.NewDevelopmentConfig()
devCfg.DisableStacktrace = true
devLogger := zap.Must(devCfg.Build())
```

### Key Config fields

| Field | Type | Default (Production) |
|---|---|---|
| `Level` | `AtomicLevel` | `InfoLevel` |
| `Development` | `bool` | `false` |
| `Encoding` | `string` | `"json"` |
| `OutputPaths` | `[]string` | `["stderr"]` |
| `ErrorOutputPaths` | `[]string` | `["stderr"]` |
| `DisableCaller` | `bool` | `false` |
| `DisableStacktrace` | `bool` | `false` |
| `InitialFields` | `map[string]interface{}` | `nil` |

### Global logger

```go
// Replace the global logger (returns undo function)
logger := zap.Must(zap.NewProduction())
undo := zap.ReplaceGlobals(logger)
defer undo()

// Use via package-level functions
zap.L().Info("uses global Logger")
zap.S().Infow("uses global SugaredLogger", "key", "value")
```

## Pitfalls

### Missing Sync call

**Wrong:**
```go
func main() {
    logger, _ := zap.NewProduction()
    logger.Info("starting")
    // process exits — buffered entries may be lost
}
```

**Right:**
```go
func main() {
    logger := zap.Must(zap.NewProduction())
    defer logger.Sync() // always flush before exit
    logger.Info("starting")
}
```

---

### Ignoring construction errors

**Wrong:**
```go
logger, _ := zap.NewProduction() // silently nil on failure
logger.Info("this may panic")
```

**Right:**
```go
// Option A: explicit check
logger, err := zap.NewProduction()
if err != nil {
    panic("failed to build logger: " + err.Error())
}

// Option B: Must panics on non-nil error
logger := zap.Must(zap.NewProduction())
```

---

### Odd number of key-value arguments in SugaredLogger

**Wrong:**
```go
sugar.Infow("request done", "url", "https://example.com", "status") // missing value for "status"
```

**Right:**
```go
sugar.Infow("request done",
    "url", "https://example.com",
    "status", 200, // always provide key-value pairs
)
```

---

### Using SugaredLogger in a hot path

**Wrong:**
```go
sugar := logger.Sugar()
for i := 0; i < 1_000_000; i++ {
    sugar.Infow("processing", "id", i) // 4-10x overhead per call
}
```

**Right:**
```go
for i := 0; i < 1_000_000; i++ {
    logger.Info("processing", zap.Int("id", i)) // zero extra allocations
}
```

---

### Manually implementing ArrayMarshaler for object slices

**Wrong:**
```go
// Unnecessary boilerplate since v1.22
type addrList []addr

func (al addrList) MarshalLogArray(enc zapcore.ArrayEncoder) error {
    for _, a := range al {
        if err := enc.AppendObject(a); err != nil {
            return err
        }
    }
    return nil
}
logger.Info("addrs", zap.Array("addrs", addrList(items)))
```

**Right:**
```go
// addr implements zapcore.ObjectMarshaler — use zap.Objects directly
logger.Info("addrs", zap.Objects("addrs", items))
// For value receivers (not pointer): use zap.ObjectValues
logger.Info("addrs", zap.ObjectValues("addrs", items))
```

## References

- [Documentation](https://pkg.go.dev/go.uber.org/zap)

## Migration from v1.26

### v1.26 → v1.27 additions

**`SugaredLogger.WithLazy`** — lazily evaluated context fields, mirrors `Logger.WithLazy`:
```go
// Before (v1.26): no lazy sugar variant
sugar = sugar.With("expensive", computeExpensiveValue()) // always evaluated

// After (v1.27): evaluation deferred until entry is written
sugar = sugar.WithLazy("expensive", computeExpensiveValue)
```

**`SugaredLogger.Log`, `Logw`, `Logln`** — dynamic level selection on SugaredLogger:
```go
sugar.Log(zapcore.InfoLevel, "dynamic level message")
sugar.Logw(zapcore.WarnLevel, "structured", "key", "val")
sugar.Logln(zapcore.DebugLevel, "space", "joined", "args")
```

**`WithPanicHook`** — control panic behavior in tests:
```go
// Prevent actual panic in tests
logger := zap.NewExample(zap.WithPanicHook(zapcore.WriteThenNoop))
logger.Panic("would panic in production, noop in test")
```

**`zaptest.NewTestingWriter`** — flexible test writer alternative to `zaptest.NewLogger`:
```go
func TestSomething(t *testing.T) {
    w := zaptest.NewTestingWriter(t)
    logger := zap.New(
        zapcore.NewCore(
            zapcore.NewConsoleEncoder(zap.NewDevelopmentEncoderConfig()),
            w,
            zapcore.DebugLevel,
        ),
    )
    logger.Info("appears in test output on failure")
}
```

## API Reference

**`zap.NewProduction(opts ...Option) (*Logger, error)`** — builds a production-ready JSON logger at info level with timestamps and caller info.

**`zap.NewDevelopment(opts ...Option) (*Logger, error)`** — builds a human-friendly console logger at debug level; stacktraces on warn and above.

**`zap.Must(logger *Logger, err error) *Logger`** — panics if err is non-nil; use to eliminate error-check boilerplate at initialization.

**`zap.New(core zapcore.Core, options ...Option) *Logger`** — constructs a logger from a custom `zapcore.Core`; entry point for advanced configuration.

**`zap.NewNop() *Logger`** — returns a no-op logger that discards all output; useful in tests.

**`Logger.With(fields ...Field) *Logger`** — returns a child logger with additional persistent fields; does not mutate the receiver.

**`Logger.Named(s string) *Logger`** — adds a dot-separated name segment to the logger; chained calls produce `"parent.child"`.

**`Logger.Check(lvl zapcore.Level, msg string) *zapcore.CheckedEntry`** — returns nil if the level is disabled, avoiding field allocation; most performant logging path.

**`Logger.Sugar() *SugaredLogger`** — wraps Logger in a loosely-typed API; cheap conversion, can call `Desugar()` to go back.

**`Logger.Sync() error`** — flushes any buffered log entries; always call before process exit via `defer`.

**`zap.NewAtomicLevelAt(l zapcore.Level) AtomicLevel`** — creates an `AtomicLevel` initialized at the given level for thread-safe runtime level changes.

**`AtomicLevel.SetLevel(l zapcore.Level)`** — atomically updates the log level; affects all loggers sharing this `AtomicLevel`.

**`zap.Config.Build(opts ...Option) (*Logger, error)`** — constructs a logger from the `Config` struct; supports JSON unmarshaling for file-based config.

**`Logger.WithOptions(opts ...Option) *Logger`** — applies options (e.g., `WrapCore`, `AddCallerSkip`) to a copy of the logger without mutation.

**`zap.WrapCore(f func(zapcore.Core) zapcore.Core) Option`** — option to replace or wrap the logger's core; use with `NewTee` to fan out to multiple sinks.