---
name: viper
description: Viper is a complete configuration solution for Go applications.
license: MIT
metadata:
  version: "1.20.1"
  ecosystem: go
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```go
// Use only the imports required by your file. Common combinations:

// Basic config file usage:
import "github.com/spf13/viper"

// With file-change watching:
import (
    "github.com/fsnotify/fsnotify"
    "github.com/spf13/viper"
)

// For remote config support (etcd, consul, etc.), add a blank import:
import _ "github.com/spf13/viper/remote"
```

## Core Patterns

### Read config file with defaults and env variables ✅ Current

```go
package main

import (
    "log"
    "os"

    "github.com/spf13/viper"
)

func main() {
    v := viper.New()

    // Set defaults
    v.SetDefault("server.port", 8080)
    v.SetDefault("server.host", "localhost")
    v.SetDefault("log.level", "info")

    // Config file discovery
    v.SetConfigName("config")         // name without extension
    v.SetConfigType("yaml")           // required when no extension
    v.AddConfigPath("/etc/myapp/")
    v.AddConfigPath("$HOME/.myapp")
    v.AddConfigPath(".")

    // Environment variable support
    v.SetEnvPrefix("MYAPP")
    v.AutomaticEnv()

    if err := v.ReadInConfig(); err != nil {
        if _, ok := err.(viper.ConfigFileNotFoundError); ok {
            // No config file found; defaults and env vars still apply
            log.Println("No config file found, using defaults and env")
        } else {
            log.Fatalf("Fatal error reading config: %v", err)
        }
    }

    port := v.GetInt("server.port")
    host := v.GetString("server.host")
    log.Printf("Starting on %s:%d", host, port)
    _ = os.Stdout // suppress unused import
}
```

Config files are discovered in order. `AutomaticEnv` maps `MYAPP_SERVER_PORT` to `server.port`.

---

### Read config from an io.Reader (stream) ✅ Current

```go
package main

import (
    "log"
    "strings"

    "github.com/spf13/viper"
)

func main() {
    v := viper.New()
    v.SetConfigType("json") // required when reading from a stream

    jsonConfig := strings.NewReader(`{"database": {"host": "db.example.com", "port": 5432}}`)

    if err := v.ReadConfig(jsonConfig); err != nil {
        log.Fatalf("error reading config: %v", err)
    }

    log.Printf("DB host: %s", v.GetString("database.host"))
    log.Printf("DB port: %d", v.GetInt("database.port"))
}
```

`SetConfigType` is mandatory when no file extension is available to infer the format.

---

### Unmarshal config into a struct ✅ Current

```go
package main

import (
    "log"
    "strings"

    "github.com/spf13/viper"
)

type ServerConfig struct {
    Host string
    Port int
}

type Config struct {
    Server ServerConfig
    Debug  bool
}

func main() {
    v := viper.New()
    v.SetConfigType("yaml")

    yamlData := strings.NewReader("server:\n  host: localhost\n  port: 9090\ndebug: true\n")
    if err := v.ReadConfig(yamlData); err != nil {
        log.Fatalf("error reading config: %v", err)
    }

    var cfg Config
    if err := v.Unmarshal(&cfg); err != nil {
        log.Fatalf("error unmarshalling config: %v", err)
    }

    log.Printf("Server: %s:%d, Debug: %v", cfg.Server.Host, cfg.Server.Port, cfg.Debug)
}
```

Use `v.UnmarshalKey("server", &serverCfg)` to unmarshal only a sub‑tree.

---

### Watch for config file changes ✅ Current

```go
package main

import (
    "log"

    "github.com/fsnotify/fsnotify"
    "github.com/spf13/viper"
)

func main() {
    v := viper.New()
    v.SetConfigName("config")
    v.SetConfigType("yaml")
    v.AddConfigPath(".")

    if err := v.ReadInConfig(); err != nil {
        log.Fatalf("error reading config: %v", err)
    }

    // Register callback BEFORE WatchConfig
    v.OnConfigChange(func(e fsnotify.Event) {
        log.Println("Config file changed, reloading")
    })

    // Call WatchConfig AFTER all AddConfigPath calls
    v.WatchConfig()

    log.Println("Watching for config changes. Press Ctrl+C to stop.")
    select {} // block forever in a real app
}
```

Always call `AddConfigPath` and `OnConfigChange` before `WatchConfig`. Paths registered after `WatchConfig` are not watched.

---

### Isolated Viper instance with custom options ✅ Current

```go
package main

import (
    "log"
    "strings"

    "github.com/spf13/viper"
)

func main() {
    // Create a logger that satisfies Viper's Logger interface.
    logger := log.Default() // *log.Logger implements Printf

    // Create an isolated instance with a custom logger.
    v := viper.NewWithOptions(
        viper.WithLogger(logger),
    )

    // Apply a key replacer for environment variables.
    v.SetEnvKeyReplacer(strings.NewReplacer("-", "_", ".", "_"))

    v.SetEnvPrefix("APP")
    v.AutomaticEnv()
    v.SetDefault("feature-flag.enabled", false)

    enabled := v.GetBool("feature-flag.enabled")
    log.Printf("Feature flag enabled: %v", enabled)
}
```

Prefer `viper.New()` or `viper.NewWithOptions()` over the global instance in libraries and multi‑tenant applications to avoid shared state.

## Configuration

### Precedence order (highest to lowest)

1. Explicit `v.Set(key, value)`
2. Flag (pflag / `BindPFlag`)
3. Environment variable (`BindEnv` / `AutomaticEnv`)
4. Config file (`ReadInConfig`)
5. Key/value store (`ReadRemoteConfig`)
6. Default (`SetDefault`)

### Supported config file formats

```
json  toml  yaml  yml  properties  props  prop  env  dotenv
```

### Environment variable conventions

```go
v := viper.New()
v.SetEnvPrefix("APP")       // APP_PORT maps to "port"
v.AutomaticEnv()           // enable auto-mapping

// BindEnv with no explicit env name: prefix IS prepended automatically
v.BindEnv("port")            // reads APP_PORT

// BindEnv with explicit env name: prefix is NOT prepended
v.BindEnv("port", "APP_PORT") // reads APP_PORT (must be fully qualified)
```

### Codec registry customization

```go
registry := viper.NewCodecRegistry()
// Register a custom codec for a new format
// (myCodec must implement viper.Codec: Encode + Decode)
// err := registry.RegisterCodec("myformat", myCodec{})

v := viper.NewWithOptions(
    viper.WithCodecRegistry(registry),
)
_ = v
```

### Write config to file

```go
// Create-or-truncate
v.WriteConfig()
v.WriteConfigAs("/path/to/config.yaml")

// Create-only (error if file exists)
v.SafeWriteConfig()
v.SafeWriteConfigAs("/path/to/config.yaml")
```

## Pitfalls

### Key case sensitivity causes unexpected shadowing

**Wrong**
```go
v := viper.New()
v.Set("MyKey", 1)
val := v.GetInt("mykey") // developer assumes no match
// val == 1 — unexpected
```

**Right**
```go
v := viper.New()
// Treat all keys as lowercase throughout the codebase
v.Set("mykey", 1)
val := v.GetInt("mykey") // val == 1 — explicit and clear
_ = val
```

Viper keys are always case‑insensitive. `"MyKey"` and `"mykey"` refer to the same config entry.

---

### ENV prefix not applied when explicit name is passed to BindEnv

**Wrong**
```go
v := viper.New()
v.SetEnvPrefix("APP")
v.BindEnv("id", "id")    // expects APP_ID; actually looks for literal "id"
os.Setenv("APP_ID", "42")
val := v.GetString("id") // "" — not found
```

**Right**
```go
v := viper.New()
v.SetEnvPrefix("APP")

// Option 1: omit explicit name; Viper auto‑prepends prefix
v.BindEnv("id")           // resolves to APP_ID automatically
os.Setenv("APP_ID", "42")
val := v.GetString("id")  // "42"

// Option 2: provide fully qualified env name
v.BindEnv("id", "APP_ID")
_ = val
```

---

### Missing SetConfigType for stream or remote sources

**Wrong**
```go
v := viper.New()
// No SetConfigType — viper cannot infer format from a reader
r := strings.NewReader(`{"port": 8080}`)
err := v.ReadConfig(r) // error: config type not set
_ = err
```

**Right**
```go
v := viper.New()
v.SetConfigType("json") // required for streams and remote sources
r := strings.NewReader(`{"port": 8080}`)
if err := v.ReadConfig(r); err != nil {
    log.Fatalf("error: %v", err)
}
log.Println(v.GetInt("port")) // 8080
```

---

### WatchConfig called before AddConfigPath

**Wrong**
```go
v := viper.New()
v.WatchConfig()              // watchers set up here — paths not yet registered
v.AddConfigPath("/etc/app")  // too late; this path is not watched
```

**Right**
```go
v := viper.New()
v.SetConfigName("config")
v.SetConfigType("yaml")
v.AddConfigPath("/etc/app")  // register paths first
v.OnConfigChange(func(e fsnotify.Event) { log.Println("config changed") })
v.WatchConfig()              // then start watching
```

---

### Using the global instance in a library

**Wrong**
```go
// Inside a library package — pollutes global state shared by the entire process
func Configure() {
    viper.SetDefault("timeout", 30) // conflicts with application config
}
```

**Right**
```go
// Libraries should create and return their own isolated Viper instance
func NewConfig() *viper.Viper {
    v := viper.New()
    v.SetDefault("timeout", 30)
    return v
}
```

## References

- [Documentation](https://pkg.go.dev/github.com/spf13/viper)
- [Source](https://github.com/spf13/viper)

## Migration from v1.x

No breaking API changes are documented between v1.x minor versions for v1.20.1. The following transitions are recommended for new code:

| Old pattern | New pattern |
|---|---|
| `v.SetEnvKeyReplacer(strings.NewReplacer(...))` | `viper.NewWithOptions(viper.WithEnvKeyReplacer(replacer))` for custom `StringReplacer` interface |
| Global `viper.Set / viper.Get` in libraries | `viper.New()` isolated instance |

**`SetEnvKeyReplacer` vs `WithEnvKeyReplacer` option**

```go
// Old: accepts only *strings.Replacer, limited flexibility
v := viper.New()
v.SetEnvKeyReplacer(strings.NewReplacer("-", "_"))

// New: accepts any type implementing StringReplacer interface
v2 := viper.NewWithOptions(
    viper.WithEnvKeyReplacer(strings.NewReplacer("-", "_", ".", "_")),
)
_ = v2
```

Note: Viper v2 is in active development. Monitor https://github.com/spf13/viper for breaking changes before upgrading.

## Migration Notes (breaking changes)

| Version | Change | Migration |
|---|---|---|
| **1.6.0** | `ReadInConfig` now requires an explicit `SetConfigType` call for configuration files that lack an extension. | Whenever you load a file without an extension, add `viper.SetConfigType("yaml"|"json"|…)` before `ReadInConfig`. |
| **1.10.0** | `WriteConfig` and `WriteConfigAs` now truncate existing files instead of appending. | Switch to `SafeWriteConfig` / `SafeWriteConfigAs` if you need to preserve an existing file, or explicitly check for existence before calling the unsafe variants. |
| **1.14.0** | `SetEnvKeyReplacer` now accepts a `strings.Replacer` **or** any type implementing the `StringReplacer` interface via `NewWithOptions`. | If you provide a custom replacer, ensure it implements `Replace(string) string`. No change needed for standard `strings.NewReplacer` usage. |

### From pre‑1.6 to ≥ 1.6
* Add `viper.SetConfigType` for any config file that does not have an extension (e.g., `.bashrc`, `config`).  
* The call must happen **before** `ReadInConfig` or `ReadRemoteConfig`.

### From < 1.10 to ≥ 1.10
* Review all uses of `WriteConfig` / `WriteConfigAs`. If the target file may already exist, replace the call with the `Safe*` equivalents or add an existence check.

### Remote configuration changes
* The remote provider APIs (`AddRemoteProvider`, `AddSecureRemoteProvider`) remain stable, but the recommended pattern is to create a dedicated Viper instance (`runtimeViper := viper.New()`) for background workers to avoid polluting the global singleton.  
* When using encrypted remote configs, always provide the GPG keyring path; otherwise, the call will fail with a permission error.

### Environment variable handling
* After setting a prefix with `SetEnvPrefix`, always call `AutomaticEnv()` **after** the prefix is set.  
* When you need to bind a specific env var name, remember that the prefix is **not** added automatically – either omit the explicit name or prepend the prefix yourself.

For a full changelog see the module’s `CHANGELOG.md` – look for entries prefixed with `[BREAKING]`, `[DEPRECATED]`, or `[NEW API]` for additional migration guidance.

## API Reference

**`viper.New()`** – Creates and returns an isolated `*Viper` instance. Preferred over the global instance in libraries.

**`viper.NewWithOptions(opts ...Option)`** – Creates a `*Viper` instance with functional options such as `WithLogger`, `WithCodecRegistry`, `WithEnvKeyReplacer`, `WithFinder`, `ExperimentalFinder`.

**`(*Viper).SetDefault(key string, value any)`** – Sets the lowest‑precedence default value for a key. Never overrides env, flag, or file values.

**`(*Viper).Set(key string, value any)`** – Sets the highest‑precedence override value for a key, superseding all other sources.

**`(*Viper).Get(key string) any`** – Returns the value for a key across all config sources in precedence order. Use typed variants (`GetString`, `GetInt`, etc.) for type safety.

**`(*Viper).ReadInConfig() error`** – Finds and reads the config file. Returns `ConfigFileNotFoundError` if no file is found, or a parse/IO error otherwise.

**`(*Viper).ReadConfig(in io.Reader) error`** – Reads config from an arbitrary `io.Reader`. Requires `SetConfigType` to be called first.

**`(*Viper).SetConfigName(in string)`** – Sets the config file name to search for (without extension).

**`(*Viper).SetConfigType(in string)`** – Sets the config format explicitly. Required for streams, remote sources, and extension‑less files.

**`(*Viper).AddConfigPath(in string)`** – Adds a directory to search for the config file. Multiple paths may be added; called before `ReadInConfig`.

**`(*Viper).AutomaticEnv()`** – Enables automatic mapping of environment variables to config keys using the env prefix.

**`(*Viper).SetEnvPrefix(in string)`** – Sets a prefix for environment variables to avoid collisions (e.g., `"APP"` maps `APP_PORT` to `"port"`).

**`(*Viper).BindEnv(input ...string) error`** – Binds a config key to an environment variable. Without explicit env name, the prefix is auto‑prepended.

**`(*Viper).Unmarshal(rawVal any, opts ...DecoderConfigOption) error`** – Decodes the entire config into a struct. Uses `mapstructure` tags.

**`(*Viper).WatchConfig()`** – Starts watching the config file for live changes. Call after all `AddConfigPath` and `OnConfigChange` registrations.

**`(*Viper).OnConfigChange(fn func(e fsnotify.Event))`** – Registers a callback invoked when the watched config file changes.

**`viper.WithLogger(l interface{ Printf(format string, v ...any) }) Option`** – Functional option to set a custom logger on a new Viper instance. Accepts any logger implementing a `Printf` method (e.g., `*log.Logger`).

**`viper.WithCodecRegistry(r CodecRegistry) Option`** – Functional option to provide a custom codec registry.

**`viper.WithFinder(f Finder) Option`** – Functional option to integrate a custom finder.

**`viper.ExperimentalFinder() Option`** – Enables experimental finder integration.

**`viper.ExperimentalBindStruct() Option`** – Enables experimental struct binding (future‑proof API).

**`viper.NewCodecRegistry() CodecRegistry`** – Returns a new codec registry for registering custom encoders/decoders.

**`viper.RegisterAlias(oldKey, newKey string)`** – Creates an alias so both keys resolve to the same value.

**`viper.AllowEmptyEnv()`** – Allows empty environment variables to be considered set.

**`viper.BindPFlag(key string, flag *pflag.Flag) error`** – Binds a specific pflag to a config key.

**`viper.BindPFlags(flags *pflag.FlagSet) error`** – Binds all flags in a FlagSet.

**`viper.BindFlagValue(key string, fv FlagValue) error`** – Binds a custom flag value implementation.

**`viper.BindFlagValues(fvs FlagValueSet) error`** – Binds a set of custom flag values.

**`viper.AddRemoteProvider(provider RemoteProvider, endpoint, path string) error`** – Adds a remote configuration provider.

**`viper.AddSecureRemoteProvider(provider RemoteProvider, endpoint, path, certFile, keyFile, caFile string) error`** – Adds a secure remote configuration provider.

**`viper.ReadRemoteConfig() error`** – Reads configuration from a remote provider.

**`viper.WatchRemoteConfig() error`** – Watches remote configuration for changes.

**`viper.WriteConfig() error`, **`viper.WriteConfigAs(filename string) error`** – Writes the current configuration to a file (overwrites if it exists).

**`viper.SafeWriteConfig() error`, **`viper.SafeWriteConfigAs(filename string) error`** – Writes the configuration only if the target file does not already exist.

---