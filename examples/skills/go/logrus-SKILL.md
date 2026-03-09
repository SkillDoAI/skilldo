---

name: logrus
description: Structured logging for Go with leveled logs, fields, hooks, and pluggable formatters (text/JSON) for application and library instrumentation.
license: MIT
metadata:
  version: "1.9.3"
  ecosystem: go
  generated-by: skilldo/gpt-5.2 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```go
import (
	"github.com/sirupsen/logrus"
)
```

```go
import (
	"github.com/sirupsen/logrus/hooks/syslog"
)
```

```go
import (
	"github.com/sirupsen/logrus/hooks/test"
)
```

```go
import (
	"github.com/sirupsen/logrus/hooks/writer"
)
```

## Core Patterns

### Configure a logger + structured fields (JSON for production) ✅ Current

```go
package main

import (
	"os"

	"github.com/sirupsen/logrus"
)

func main() {
	log := logrus.New()
	log.SetOutput(os.Stdout)
	log.SetLevel(logrus.InfoLevel)
	log.SetFormatter(&logrus.JSONFormatter{
		DisableTimestamp: true,
	})

	reqLog := logrus.NewEntry(log).WithFields(logrus.Fields{
		"service":    "billing",
		"request_id": "req-123",
	})

	reqLog.WithField("user_id", 42).Info("request started")
	reqLog.WithField("latency_ms", 17).Warn("slow downstream")
}
```

Use `logrus.New()` to create an isolated `*logrus.Logger` (preferred for libraries/services). Create a contextual `*logrus.Entry` via `WithFields` and reuse it to keep common fields consistent.

---

### Add a Hook to enrich entries at log time ✅ Current

```go
package main

import (
	"os"

	"github.com/sirupsen/logrus"
)

type DefaultFieldHook struct{}

func (h *DefaultFieldHook) Levels() []logrus.Level {
	return logrus.AllLevels
}

func (h *DefaultFieldHook) Fire(e *logrus.Entry) error {
	// Hooks may mutate the entry fields for this log event.
	e.Data["component"] = "api"
	return nil
}

func main() {
	log := logrus.New()
	log.SetOutput(os.Stdout)
	log.SetFormatter(&logrus.TextFormatter{
		DisableColors:    true,
		DisableTimestamp: true,
	})

	log.AddHook(&DefaultFieldHook{})

	log.WithField("path", "/v1/health").Info("request")
}
```

Hooks implement `logrus.Hook` and can add/modify `Entry.Data` during `Fire`. Hook firing is level-filtered via `Levels()` and occurs during logging.

---

### Capture logs in tests with hooks/test ✅ Current

```go
package main

import (
	"fmt"

	"github.com/sirupsen/logrus"
	"github.com/sirupsen/logrus/hooks/test"
)

func main() {
	logger, hook := test.NewNullLogger()

	logger.WithField("case", "smoke").Warn("hello warning")
	last := hook.LastEntry()
	if last == nil {
		panic("expected a log entry")
	}

	fmt.Printf("level=%s msg=%q case=%v\n", last.Level.String(), last.Message, last.Data["case"])
}
```

`hooks/test` provides a hook that records entries, making it easy to assert on `Level`, `Message`, and `Data` without parsing output.

---

### Register process exit handlers for Fatal paths ✅ Current

```go
package main

import (
	"fmt"

	"github.com/sirupsen/logrus"
)

func main() {
	// Exit handlers run when logrus triggers an exit (e.g., via Fatal on the standard logger).
	logrus.RegisterExitHandler(func() {
		fmt.Println("cleanup: flushing metrics")
	})

	// Avoid calling logrus.Fatal in this example because it would terminate the process.
	// In real code, prefer Fatal only for unrecoverable errors.
	logrus.WithField("reason", "example").Error("would exit on Fatal in real usage")
}
```

Use `logrus.RegisterExitHandler(func())` to register cleanup logic that should run when Logrus exits (e.g., `Fatal`). Handlers run in registration order; panics inside one handler should not prevent others from running (per tests).

## Configuration

- **Logger creation**
  - `logrus.New()` creates a new `*logrus.Logger`.
  - `logrus.StandardLogger()` returns the package-level global logger.

- **Output**
  - Global: `logrus.SetOutput(io.Writer)`
  - Per-logger: `logger.SetOutput(io.Writer)` (preferred when you control the logger instance)

- **Formatter**
  - Global: `logrus.SetFormatter(logrus.Formatter)`
  - Per-logger: `logger.SetFormatter(logrus.Formatter)`
  - Common formatters:
    - `&logrus.JSONFormatter{...}` for structured logs; note reserved keys such as `"time"`, `"msg"`, `"level"` are handled by prefixing colliding user field keys with `"fields."` in JSON output (e.g., `"fields.time"`).
    - `&logrus.TextFormatter{...}` for human-readable logs.

- **Levels**
  - Set: `logrus.SetLevel(logrus.Level)` or `logger.SetLevel(logrus.Level)`
  - Check: `logrus.IsLevelEnabled(level)` or `logger.IsLevelEnabled(level)`
  - Parse from config: `logrus.ParseLevel("info")`

- **Caller reporting**
  - Enable globally: `logrus.SetReportCaller(true)`
  - Enable per logger: `logger.SetReportCaller(true)`
  - Adds overhead; gate behind configuration.

- **Global error field key**
  - `logrus.ErrorKey` controls the field name used by `WithError(err)` / `Entry.WithError(err)`.
  - If you change it, restore it in tests to avoid cross-test contamination.

## Pitfalls

### Wrong: Mixed-case import path causes conflicts

```go
package main

import (
	log "github.com/Sirupsen/logrus"
)

func main() {
	log.Info("hello")
}
```

### Right: Use the canonical lower-case module path

```go
package main

import (
	log "github.com/sirupsen/logrus"
)

func main() {
	log.Info("hello")
}
```

---

### Wrong: Forgetting to close a pipe writer from Logger.Writer()

```go
package main

import (
	"log"

	"github.com/sirupsen/logrus"
)

func main() {
	l := logrus.New()
	w := l.Writer()
	// Missing: w.Close()
	std := log.New(w, "", 0)
	std.Print("hello from stdlib log")
}
```

### Right: Close the writer to avoid leaking resources

```go
package main

import (
	"log"

	"github.com/sirupsen/logrus"
)

func main() {
	l := logrus.New()

	w := l.Writer()
	defer func() {
		_ = w.Close()
	}()

	std := log.New(w, "", 0)
	std.Print("hello from stdlib log")
}
```

---

### Wrong: Using Fatal in code paths where you still need cleanup

```go
package main

import (
	"github.com/sirupsen/logrus"
)

func main() {
	// This exits the process; deferred functions in main won't run.
	logrus.Fatal("unrecoverable")
}
```

### Right: Prefer returning errors; if you must exit, use RegisterExitHandler

```go
package main

import (
	"fmt"

	"github.com/sirupsen/logrus"
)

func main() {
	logrus.RegisterExitHandler(func() {
		fmt.Println("cleanup ran before exit")
	})

	// Example keeps running; in real code Fatal would exit.
	logrus.Error("handle error without exiting in this example")
}
```

---

### Wrong: Expecting WithContext to mutate an Entry in place

```go
package main

import (
	"context"
	"fmt"

	"github.com/sirupsen/logrus"
)

func main() {
	e := logrus.NewEntry(logrus.New())
	ctx := context.WithValue(context.Background(), "k", "v")

	e.WithContext(ctx) // return value ignored
	fmt.Println(e.Context == ctx)
}
```

### Right: Use the returned *logrus.Entry (it is a copy)

```go
package main

import (
	"context"
	"fmt"

	"github.com/sirupsen/logrus"
)

func main() {
	e := logrus.NewEntry(logrus.New())
	ctx := context.WithValue(context.Background(), "k", "v")

	e2 := e.WithContext(ctx)
	fmt.Println(e2.Context == ctx)
}
```

---

### Wrong: Assuming hooks are isolated from each other’s mutations

```go
package main

import (
	"os"

	"github.com/sirupsen/logrus"
)

type OverwriteHook struct{}

func (h *OverwriteHook) Levels() []logrus.Level { return []logrus.Level{logrus.InfoLevel} }
func (h *OverwriteHook) Fire(e *logrus.Entry) error {
	e.Data["wow"] = "whale"
	return nil
}

func main() {
	l := logrus.New()
	l.SetOutput(os.Stdout)
	l.SetFormatter(&logrus.JSONFormatter{DisableTimestamp: true})
	l.AddHook(&OverwriteHook{})

	// Hook will overwrite "wow".
	l.WithField("wow", "elephant").Info("test")
}
```

### Right: Pick stable field semantics and document hook behavior

```go
package main

import (
	"os"

	"github.com/sirupsen/logrus"
)

type DefaultOnlyHook struct{}

func (h *DefaultOnlyHook) Levels() []logrus.Level { return []logrus.Level{logrus.InfoLevel} }
func (h *DefaultOnlyHook) Fire(e *logrus.Entry) error {
	if _, ok := e.Data["wow"]; !ok {
		e.Data["wow"] = "whale"
	}
	return nil
}

func main() {
	l := logrus.New()
	l.SetOutput(os.Stdout)
	l.SetFormatter(&logrus.JSONFormatter{DisableTimestamp: true})
	l.AddHook(&DefaultOnlyHook{})

	l.WithField("wow", "elephant").Info("test")
}
```

## References

- [Documentation](https://pkg.go.dev/github.com/sirupsen/logrus)
- [Source](https://github.com/sirupsen/logrus)

## Migration from v1.0

### TextFormatter newline change (<=1.0.0 -> 1.1.0)

`TextFormatter` no longer appends an extra newline at the end of the message. If you asserted exact output strings, update expectations.

**Before (tests expecting extra newline):**
```go
package main

import (
	"bytes"
	"fmt"

	"github.com/sirupsen/logrus"
)

func main() {
	var b bytes.Buffer
	l := logrus.New()
	l.SetOutput(&b)
	l.SetFormatter(&logrus.TextFormatter{DisableTimestamp: true, DisableColors: true})

	l.Info("hello")
	fmt.Print(b.String())
}
```

**After (update golden strings / expectations):**
- Expect the formatter’s current output (no extra newline beyond the line terminator it writes for the entry).
- If you need an extra blank line, add it explicitly in your own output handling (not recommended for structured logging).

### Import path standardization

Ensure all code uses:
- `github.com/sirupsen/logrus` (lower-case)

Avoid historical mixed-case imports to prevent module/path conflicts.

## API Reference

**Exit(code int)** - Exits the process using Logrus’ exit function; triggers registered exit handlers.  
**RegisterExitHandler(handler func())** - Registers a handler to run on exit (e.g., during `Fatal`); registration order is preserved.  
**StandardLogger()** - Returns the package-level global `*logrus.Logger`.  
**SetOutput(out io.Writer)** - Sets the output for the global logger.  
**SetFormatter(formatter Formatter)** - Sets the formatter for the global logger (`TextFormatter`, `JSONFormatter`, or custom).  
**SetReportCaller(include bool)** - Enables/disables caller reporting for the global logger.  
**SetLevel(level Level)** - Sets the global logger level threshold.  
**GetLevel()** - Returns the current global logger level.  
**IsLevelEnabled(level Level)** - Reports whether `level` would be logged by the global logger.  
**AddHook(hook Hook)** - Adds a hook to the global logger.  
**WithField(key string, value interface{})** - Creates an `*Entry` from the global logger with one structured field.  
**WithFields(fields Fields)** - Creates an `*Entry` from the global logger with multiple structured fields.  
**WithError(err error)** - Creates an `*Entry` with the error stored under `ErrorKey`.  
**ParseLevel(lvl string)** - Parses a textual level (e.g., `"info"`, `"warn"`) into a `Level`.  
**JSONFormatter.Format(entry *Entry)** - Formats an entry as JSON; stringifies `error` values and resolves reserved key clashes by prefixing colliding user field keys with `"fields."` in the output map (e.g., `"fields.time"`).