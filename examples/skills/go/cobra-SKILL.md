---

name: cobra
description: Go library for building CLI applications with subcommands, flags, argument validation, and shell completion.
license: Apache-2.0
metadata:
  version: "1.9.1"
  ecosystem: go
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```go
import (
    "github.com/spf13/cobra"
)
```

## Core Patterns

### Basic Root Command with Subcommand ✅ Current

```go
package main

import (
    "fmt"
    "os"

    "github.com/spf13/cobra"
)

func main() {
    rootCmd := &cobra.Command{
        Use:          "myapp",
        Short:        "A brief description of myapp",
        Long:         "A longer description of myapp with details.",
        SilenceUsage: true,
        SilenceErrors: true,
    }

    var name string
    greetCmd := &cobra.Command{
        Use:   "greet [person]",
        Short: "Greet a person",
        Args:  cobra.ExactArgs(1),
        RunE: func(cmd *cobra.Command, args []string) error {
            fmt.Fprintf(cmd.OutOrStdout(), "Hello, %s! (name flag: %s)\n", args[0], name)
            return nil
        },
    }
    greetCmd.Flags().StringVarP(&name, "name", "n", "", "Override the greeting name")

    rootCmd.AddCommand(greetCmd)

    if err := rootCmd.Execute(); err != nil {
        fmt.Fprintln(os.Stderr, err)
        os.Exit(1)
    }
}
```

Define the root command with `Use`, `Short`, and `Long`. Add subcommands via `AddCommand`. Use `RunE` to propagate errors. Set `SilenceUsage` and `SilenceErrors` to control noise in production CLIs.

### Persistent Flags and PreRun Hooks ✅ Current

```go
package main

import (
    "fmt"
    "os"

    "github.com/spf13/cobra"
)

var verbose bool
var cfgFile string

func main() {
    rootCmd := &cobra.Command{
        Use:           "myapp",
        Short:         "myapp CLI",
        SilenceUsage:  true,
        SilenceErrors: true,
        PersistentPreRunE: func(cmd *cobra.Command, args []string) error {
            if verbose {
                fmt.Fprintln(cmd.OutOrStdout(), "verbose mode enabled")
            }
            return nil
        },
    }

    rootCmd.PersistentFlags().BoolVarP(&verbose, "verbose", "v", false, "Enable verbose output")
    rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "Config file path")

    serveCmd := &cobra.Command{
        Use:   "serve",
        Short: "Start the server",
        Args:  cobra.NoArgs,
        RunE: func(cmd *cobra.Command, args []string) error {
            fmt.Fprintln(cmd.OutOrStdout(), "serving...")
            return nil
        },
    }
    rootCmd.AddCommand(serveCmd)

    if err := rootCmd.Execute(); err != nil {
        fmt.Fprintln(os.Stderr, err)
        os.Exit(1)
    }
}
```

Use `PersistentFlags()` for flags that cascade to all subcommands. Use `Flags()` for flags local to one command. `PersistentPreRunE` runs before every command in the tree.

### Argument Validation ✅ Current

```go
package main

import (
    "fmt"
    "os"

    "github.com/spf13/cobra"
)

func main() {
    rootCmd := &cobra.Command{Use: "myapp", Short: "myapp CLI"}

    // Exactly 2 args required
    exactCmd := &cobra.Command{
        Use:  "copy [src] [dst]",
        Args: cobra.ExactArgs(2),
        RunE: func(cmd *cobra.Command, args []string) error {
            fmt.Fprintf(cmd.OutOrStdout(), "copy %s -> %s\n", args[0], args[1])
            return nil
        },
    }

    // Only values from ValidArgs accepted
    deployCmd := &cobra.Command{
        Use:       "deploy [env]",
        Args:      cobra.MatchAll(cobra.ExactArgs(1), cobra.OnlyValidArgs),
        ValidArgs: []string{"staging", "production"},
        RunE: func(cmd *cobra.Command, args []string) error {
            fmt.Fprintf(cmd.OutOrStdout(), "deploying to %s\n", args[0])
            return nil
        },
    }

    rootCmd.AddCommand(exactCmd, deployCmd)

    if err := rootCmd.Execute(); err != nil {
        fmt.Fprintln(os.Stderr, err)
        os.Exit(1)
    }
}
```

Use `MatchAll` to combine validators. `OnlyValidArgs` validates against `ValidArgs`. `ExactArgs`, `MinimumNArgs`, `MaximumNArgs`, `RangeArgs`, `NoArgs`, and `ArbitraryArgs` cover common cases.

### Shell Completion with ValidArgsFunction ✅ Current

```go
package main

import (
    "fmt"
    "os"

    "github.com/spf13/cobra"
)

func main() {
    rootCmd := &cobra.Command{Use: "myapp", Short: "myapp CLI"}

    var region string
    listCmd := &cobra.Command{
        Use:   "list",
        Short: "List resources",
        RunE: func(cmd *cobra.Command, args []string) error {
            fmt.Fprintf(cmd.OutOrStdout(), "listing in region: %s\n", region)
            return nil
        },
    }

    listCmd.Flags().StringVar(&region, "region", "us-east-1", "AWS region")
    _ = listCmd.RegisterFlagCompletionFunc("region", func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
        regions := []string{"us-east-1", "us-west-2", "eu-west-1"}
        return regions, cobra.ShellCompDirectiveNoFileComp
    })

    listCmd.ValidArgsFunction = func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
        return []string{"resource-a", "resource-b"}, cobra.ShellCompDirectiveNoFileComp
    }

    rootCmd.AddCommand(listCmd)

    if err := rootCmd.Execute(); err != nil {
        fmt.Fprintln(os.Stderr, err)
        os.Exit(1)
    }
}
```

Return `ShellCompDirectiveNoFileComp` when only specific values are valid to suppress file completion. Use `RegisterFlagCompletionFunc` for flag value completion.

### Context Propagation and Testing ✅ Current

```go
package main

import (
    "bytes"
    "context"
    "fmt"
    "os"

    "github.com/spf13/cobra"
)

func buildRootCmd() *cobra.Command {
    rootCmd := &cobra.Command{
        Use:          "myapp",
        Short:        "myapp CLI",
        SilenceUsage: true,
    }

    runCmd := &cobra.Command{
        Use:  "run",
        Args: cobra.NoArgs,
        RunE: func(cmd *cobra.Command, args []string) error {
            ctx := cmd.Context()
            select {
            case <-ctx.Done():
                return ctx.Err()
            default:
                fmt.Fprintln(cmd.OutOrStdout(), "running")
                return nil
            }
        },
    }
    rootCmd.AddCommand(runCmd)
    return rootCmd
}

func main() {
    ctx := context.Background()
    rootCmd := buildRootCmd()
    if err := rootCmd.ExecuteContext(ctx); err != nil {
        fmt.Fprintln(os.Stderr, err)
        os.Exit(1)
    }
}

// In tests:
func runCmdInTest() (string, error) {
    buf := new(bytes.Buffer)
    rootCmd := buildRootCmd()
    rootCmd.SetOut(buf)
    rootCmd.SetErr(buf)
    rootCmd.SetArgs([]string{"run"})
    err := rootCmd.Execute()
    return buf.String(), err
}
```

Use `ExecuteContext` to pass a `context.Context` through the command tree. Use `SetOut`, `SetErr`, `SetIn`, and `SetArgs` in tests to isolate I/O without touching `os.Stdout`.

## Configuration

```go
// Global behavior flags (set before Execute())
cobra.EnableCommandSorting = false   // preserve AddCommand insertion order in help
cobra.EnablePrefixMatching = true    // allow unambiguous prefix matching of subcommands

// Per-root-command settings
rootCmd.SilenceUsage  = true  // don't print usage on RunE error
rootCmd.SilenceErrors = true  // don't print error message automatically

// Completion options
rootCmd.CompletionOptions.DisableDefaultCmd   = true  // hide the built-in "completion" command
rootCmd.CompletionOptions.HiddenDefaultCmd    = true  // keep completion cmd but hide from help
rootCmd.CompletionOptions.DisableDescriptions = true  // omit completion descriptions

// Command grouping
rootCmd.AddGroup(&cobra.Group{ID: "core", Title: "Core Commands:"})
subCmd.GroupID = "core"

// Version flag
rootCmd.Version = "1.2.3" // adds --version/-v flag automatically
rootCmd.SetVersionTemplate("{{.Name}} version {{.Version}}\n")

// Custom usage/help templates
rootCmd.SetUsageTemplate(myUsageTemplate)
rootCmd.SetHelpTemplate(myHelpTemplate)

// Custom help function
rootCmd.SetHelpFunc(func(cmd *cobra.Command, args []string) {
    fmt.Fprintf(cmd.OutOrStdout(), "Custom help for %s\n", cmd.Name())
})

// Flag relationship constraints
rootCmd.MarkFlagsRequiredTogether("username", "password")
rootCmd.MarkFlagsMutuallyExclusive("json", "yaml")
rootCmd.MarkFlagsOneRequired("file", "stdin")
```

`SilenceUsage` and `SilenceErrors` are commonly set on the root command for production CLIs. `CompletionOptions` fields control the auto-generated shell completion command.

## Pitfalls

### Wrong: Local flags used when persistent flags are needed

```go
// Wrong: child commands cannot see this flag
rootCmd.Flags().StringVar(&cfgFile, "config", "", "config file")
```

### Right: Use PersistentFlags for flags shared with children

```go
// Right: all subcommands inherit this flag
rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "config file")
```

---

### Wrong: Run ignores errors silently

```go
// Wrong: errors from doWork are silently swallowed
cmd.Run = func(cmd *cobra.Command, args []string) {
    doWork() // error return discarded
}
```

### Right: Use RunE to propagate errors

```go
// Right: error surfaces through Execute()
cmd.RunE = func(cmd *cobra.Command, args []string) error {
    return doWork()
}
```

---

### Wrong: No argument validator allows unexpected input

```go
// Wrong: command silently accepts any number of positional args
cmd := &cobra.Command{
    Use: "serve",
    RunE: func(cmd *cobra.Command, args []string) error {
        return startServer()
    },
}
```

### Right: Declare explicit argument constraints

```go
// Right: unexpected args produce a clear error
cmd := &cobra.Command{
    Use:  "serve",
    Args: cobra.NoArgs,
    RunE: func(cmd *cobra.Command, args []string) error {
        return startServer()
    },
}
```

---

### Wrong: ShellCompDirectiveDefault when only fixed values are valid

```go
// Wrong: mixes file completion with your custom list
return []string{"staging", "production"}, cobra.ShellCompDirectiveDefault
```

### Right: Suppress file completion with NoFileComp

```go
// Right: only the listed values appear as completions
return []string{"staging", "production"}, cobra.ShellCompDirectiveNoFileComp
```

---

### Wrong: Using os.Stdout directly makes commands untestable

```go
// Wrong: output cannot be captured in tests
cmd.Run = func(cmd *cobra.Command, args []string) {
    fmt.Println("result")
}
```

### Right: Write to cmd.OutOrStdout() for testable I/O

```go
// Right: redirect with cmd.SetOut(buf) in tests
cmd.RunE = func(cmd *cobra.Command, args []string) error {
    fmt.Fprintln(cmd.OutOrStdout(), "result")
    return nil
}
```

## References

- [Documentation](https://pkg.go.dev/github.com/spf13/cobra)
- [Source](https://github.com/spf13/cobra)

## Migration from v1.x

No breaking changes are documented in the provided source material for v1.9.1 relative to earlier v1.x releases. The following APIs were added in specific v1.x minor versions and may not be present in older installations:

- `AddGroup` / `GroupID` — introduced in v1.6.0; use `go get github.com/spf13/cobra@latest` if missing.
- `MarkFlagsRequiredTogether`, `MarkFlagsMutuallyExclusive`, `MarkFlagsOneRequired` — added in v1.x; replace manual flag validation logic.
- `MatchAll` — replace `ExactValidArgs` (deprecated) with `MatchAll(ExactArgs(N), OnlyValidArgs)`.

**ExactValidArgs** ❌ Hard Deprecation — replace before relying on new cobra versions:

```go
// Before (deprecated):
cmd.Args = cobra.ExactValidArgs(2)

// After (current):
cmd.Args = cobra.MatchAll(cobra.ExactArgs(2), cobra.OnlyValidArgs)
```

**SetOutput** ⚠️ Soft Deprecation — still works, prefer explicit methods for new code:

```go
// Before (soft deprecated):
cmd.SetOutput(writer)

// After (current):
cmd.SetOut(writer)
cmd.SetErr(writer)
```

## API Reference

**Command** — Core struct. Fields: `Use`, `Short`, `Long`, `Example`, `Args`, `Run`, `RunE`, `PersistentPreRunE`, `ValidArgs`, `ValidArgsFunction`, `Hidden`, `Deprecated`, `Version`, `SilenceUsage`, `SilenceErrors`.

**Command.AddCommand(cmds ...*Command)** — Attaches subcommands to a parent command.

**Command.Execute() error** — Parses `os.Args` and dispatches to the matched command; call on the root command from `main`.

**Command.ExecuteContext(ctx context.Context) error** — Like `Execute` but injects a `context.Context` accessible via `cmd.Context()`.

**Command.RunE** — Field `func(cmd *Command, args []string) error`; preferred over `Run` when the command can return an error.

**Command.PersistentFlags() *flag.FlagSet** — Returns the flag set inherited by all descendant commands.

**Command.Flags() *flag.FlagSet** — Returns the flag set local to this command only.

**Command.SetOut(newOut io.Writer)** — Redirects standard output; use in tests with `bytes.Buffer`.

**Command.SetErr(newErr io.Writer)** — Redirects error output; use in tests with `bytes.Buffer`.

**Command.SetArgs(a []string)** — Overrides `os.Args`; use in tests to supply arguments.

**Command.Context() context.Context** — Returns the context set by `ExecuteContext` or `SetContext`.

**NoArgs / ExactArgs / MinimumNArgs / MaximumNArgs / RangeArgs / ArbitraryArgs / OnlyValidArgs** — `PositionalArgs` validators assigned to `Command.Args`.

**MatchAll(validators ...PositionalArgs) PositionalArgs** — Combines multiple validators; all must pass.

**ShellCompDirectiveNoFileComp** — `ShellCompDirective` constant; suppresses file completion when returning fixed completion values.

**Command.AddGroup(groups ...*Group)** — Registers command groups for organized help output; assign `GroupID` on subcommands to place them.