# Language Support

Skilldo auto-detects the language from project files. Use `--language` to override.

## Supported Ecosystems

| Language | Status | Package Manager | Detection Files | Test Validation |
|----------|--------|-----------------|-----------------|-----------------|
| Python | Full support | pip/uv | `*.py`, `pyproject.toml`, `setup.py` | uv bare-metal or container |
| Go | Full support | go modules | `*.go`, `go.mod` | go toolchain bare-metal or container |
| JavaScript/TypeScript | Full support | npm | `*.js`, `*.ts`, `package.json` | node+npm bare-metal or container |
| Rust | Full support | cargo | `*.rs`, `Cargo.toml` | cargo bare-metal or container |
| Java | Full support | Maven/Gradle | `*.java`, `pom.xml`, `build.gradle`, `build.gradle.kts` | javac bare-metal or Maven container |

## Test Validation

Each language has a dedicated executor that runs generated test code:

### Python
- **Bare-metal**: Creates an isolated `uv` virtual environment, installs the library, runs test code
- **Container**: `ghcr.io/astral-sh/uv:python3.11-bookworm-slim`
- **Requirements**: `uv` and `python3` installed locally (bare-metal) or Docker/Podman (container)

### Go
- **Bare-metal**: Creates an isolated temp directory with `GOPATH`/`GOCACHE` confined, runs `go run`
- **Container**: `golang:1.25-alpine`
- **Requirements**: `go` 1.21+ installed locally (bare-metal) or Docker/Podman (container)

### JavaScript/TypeScript
- **Language aliases**: `javascript`, `js`, `typescript`, `ts`, `node`, `npm`
- **Bare-metal**: Creates an isolated temp directory with `npm_config_cache` confined, runs `node`
- **Container**: `node:24-alpine`
- **Requirements**: `node` 18+ and `npm` installed locally (bare-metal) or Docker/Podman (container)

### Rust
- **Bare-metal**: Creates an isolated temp directory with `CARGO_HOME` confined, runs `cargo run`
- **Container**: `rust:1.87-slim` — generates Cargo.toml with deps, runs `cargo run`
- **Requirements**: Rust toolchain installed locally (bare-metal) or Docker/Podman (container)

### Java
- **Bare-metal**: Creates an isolated temp directory, writes `Main.java`, compiles with `javac`, runs with `java`. If Maven is available, generates `pom.xml` and downloads dependencies.
- **Container**: `maven:3-eclipse-temurin-21-alpine`
- **Requirements**: JDK 17+ installed locally (bare-metal) or Docker/Podman (container)
- **Build systems**: Maven (`pom.xml`) and Gradle (`build.gradle`, `build.gradle.kts`) are both detected and parsed

## Local Source Mounting

When `install_source` is set to `"local-install"` or `"local-mount"` in config, the local repository is mounted at `/src` inside the container. Each language wires `/src` into its import resolution:

| Language | `local-install` | `local-mount` |
|----------|-----------------|---------------|
| **Python** | `pip install /src` | Adds `/src` to `PYTHONPATH` |
| **Go** | `go mod edit -replace` pointing at `/src` | Same as local-install |
| **JavaScript** | `npm install /src` | Same as local-install |
| **Java** | Copies jars from `/src/target` to classpath | Same as local-install |
| **Rust** | Sets `path = "/src"` in `Cargo.toml` dependency | Same as local-install |

This lets the test agent validate generated code against the local (possibly unpublished) version of the library rather than the registry version.

## Ecosystem Handlers

Each language has a handler in `src/ecosystems/` that provides:

- **File discovery** — finds source files, test files, documentation, changelogs, examples
- **Version detection** — extracts version from package metadata (pyproject.toml, Cargo.toml, package.json, go.mod tags, pom.xml, build.gradle, build.gradle.kts)
- **License detection** — reads license from metadata or LICENSE files
- **Dependency parsing** — extracts dependencies for the test agent
- **Project URL extraction** — finds homepage, repository, documentation links

## Adding a New Language

The pattern for each new language:

1. `src/ecosystems/{lang}.rs` — handler implementing `EcosystemHandler`
2. `src/test_agent/{lang}_parser.rs` — parser implementing `LanguageParser`
3. `src/test_agent/{lang}_code_gen.rs` — code generator implementing `LanguageCodeGenerator`
4. `src/test_agent/executor.rs` — executor (bare-metal) or `container_executor.rs` (container)
5. `src/detector.rs` — add detection entry
6. Prompt hints in `src/llm/prompts_v2.rs`
