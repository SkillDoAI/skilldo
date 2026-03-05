---
name: fmt
version: unknown
language: go
---

# fmt

Go's formatted I/O package for printing, scanning, and string formatting.

## Imports

```go
import "fmt"
```

## Core Patterns

### Basic Printing

Print formatted output to stdout.

```go
fmt.Println("Hello, World!")
fmt.Printf("Name: %s, Age: %d\n", "Alice", 30)
```

### String Formatting

Format strings without printing them.

```go
s := fmt.Sprintf("Hello, %s! You are %d years old.", "Bob", 25)
fmt.Println(s)
```

### Error Formatting

Create formatted error values using Errorf.

```go
err := fmt.Errorf("failed to open file %q: %w", "data.txt", fmt.Errorf("not found"))
fmt.Println(err)
```

## Pitfalls

- `Printf` requires matching format verbs to argument types
- `Errorf` with `%w` wraps errors (Go 1.13+)
- `Sprintf` returns a string, does not print

## API Reference

- `fmt.Println(a ...any)` — print with newline
- `fmt.Printf(format string, a ...any)` — formatted print
- `fmt.Sprintf(format string, a ...any) string` — format to string
- `fmt.Errorf(format string, a ...any) error` — format error
- `fmt.Fprintf(w io.Writer, format string, a ...any)` — print to writer
- `fmt.Sscanf(str string, format string, a ...any)` — scan from string
