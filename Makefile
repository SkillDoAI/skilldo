.PHONY: help build test test-fast test-integration test-one clean install lint audit coverage coverage-report coverage-html release docker check-deps changelog setup-mac fmt fmt-check dev ci pre-release stats run example docker-run

# Coverage threshold — keep in sync with .github/workflows/ci.yml
COV_THRESHOLD := 95

# Auto-detect LLVM tools: rustup toolchain has them, Homebrew doesn't
LLVM_BIN := $(shell rustc --print sysroot)/lib/rustlib/$(shell rustc -vV | grep host | cut -d' ' -f2)/bin
RUSTUP_LLVM_BIN := $(HOME)/.rustup/toolchains/stable-$(shell rustc -vV | grep host | cut -d' ' -f2)/lib/rustlib/$(shell rustc -vV | grep host | cut -d' ' -f2)/bin
ifneq ($(wildcard $(LLVM_BIN)/llvm-cov),)
  # Tools in rustc sysroot (rustup-managed or CI)
  export LLVM_COV := $(LLVM_BIN)/llvm-cov
  export LLVM_PROFDATA := $(LLVM_BIN)/llvm-profdata
else ifneq ($(wildcard $(RUSTUP_LLVM_BIN)/llvm-cov),)
  # Homebrew cargo + rustup toolchain with llvm-tools-preview
  export LLVM_COV := $(RUSTUP_LLVM_BIN)/llvm-cov
  export LLVM_PROFDATA := $(RUSTUP_LLVM_BIN)/llvm-profdata
endif

# Default target
help:
	@echo "Skilldo - SKILL.md Generator"
	@echo ""
	@echo "Available targets:"
	@echo "  setup-mac   - Install dev dependencies on macOS (Homebrew + Cargo)"
	@echo "  build       - Build debug binary"
	@echo "  release     - Build optimized release binary"
	@echo "  test        - Run unit tests (requires uv)"
	@echo "  test-fast   - Run unit tests in parallel"
	@echo "  test-integration - Run integration tests (requires Docker/Podman)"
	@echo "  coverage    - Check coverage passes CI threshold ($(COV_THRESHOLD)% lines)"
	@echo "  coverage-report - Show per-file coverage breakdown"
	@echo "  coverage-html - Generate HTML coverage report"
	@echo "  lint        - Run clippy linter"
	@echo "  fmt         - Format code"
	@echo "  fmt-check   - Check formatting"
	@echo "  audit       - Check dependencies for known vulnerabilities"
	@echo "  clean       - Remove build artifacts"
	@echo "  install     - Install binary to ~/.cargo/bin"
	@echo "  changelog   - Generate CHANGELOG.md from conventional commits (requires git-cliff)"
	@echo "  check-deps  - Verify all development dependencies are installed"
	@echo "  docker      - Build Docker container"
	@echo "  run         - Run with example (make run ARGS='generate /path/to/repo')"

# Build debug binary
build:
	cargo build

# Build optimized release binary
release:
	cargo build --release
	@echo ""
	@echo "✅ Binary ready: target/release/skilldo ($(shell ls -lh target/release/skilldo | awk '{print $$5}'))"

# Install dev dependencies on macOS via Homebrew + Cargo
# Run this once on a fresh Mac: make setup-mac
setup-mac:
	@echo "Installing dev dependencies..."
	brew install rustup uv podman git-cliff
	rustup-init -y --no-modify-path
	rustup default stable
	rustup component add llvm-tools-preview
	cargo install cargo-llvm-cov cargo-audit
	@echo ""
	@echo "Done. Ensure your shell has:"
	@echo '  export PATH="$$(brew --prefix rustup)/bin:$$PATH"'
	@echo ""
	@echo "If you had Homebrew rust installed separately, remove it:"
	@echo "  brew uninstall rust"

# Check that dev dependencies are installed
check-deps:
	@echo "Checking development dependencies..."
	@command -v cargo >/dev/null 2>&1 || { echo "❌ cargo not found — run: make setup-mac"; exit 1; }
	@command -v uv >/dev/null 2>&1 || { echo "❌ uv not found — run: make setup-mac"; exit 1; }
	@command -v cargo-llvm-cov >/dev/null 2>&1 || cargo llvm-cov --version >/dev/null 2>&1 || { echo "❌ cargo-llvm-cov not found — run: cargo install cargo-llvm-cov"; exit 1; }
	@echo "✅ All dependencies found"

# Run all tests (requires uv for Agent 5 executor tests)
test: check-deps
	cargo test --all

# Run tests in parallel (faster)
test-fast: check-deps
	cargo test --all -- --test-threads=8

# Run integration tests (requires Docker/Podman + python3)
test-integration:
	cargo test -- --ignored

# Run specific test
test-one:
	cargo test $(TEST)

# Check coverage passes CI threshold (keep COV_THRESHOLD in sync with ci.yml)
coverage: check-deps
	cargo llvm-cov --fail-under-lines $(COV_THRESHOLD)

# Show per-file coverage breakdown sorted by missed lines (most gaps first)
coverage-report: check-deps
	@cargo llvm-cov --json 2>/dev/null | python3 -c "\
	import json,sys; d=json.load(sys.stdin); \
	files=d['data'][0]['files']; \
	total_lines=sum(f['summary']['lines']['count'] for f in files); \
	total_covered=sum(f['summary']['lines']['covered'] for f in files); \
	pct=100*total_covered/total_lines if total_lines else 0; \
	rows=[(f['filename'].split('skilldo/src/')[-1], \
	       f['summary']['lines']['count']-f['summary']['lines']['covered'], \
	       100*f['summary']['lines']['covered']/f['summary']['lines']['count'] if f['summary']['lines']['count'] else 100) \
	      for f in files]; \
	rows.sort(key=lambda r: -r[1]); \
	print(f'\n  TOTAL: {pct:.1f}% ({total_covered}/{total_lines} lines)\n'); \
	print(f'  {\"File\":<45} {\"Missed\":>7} {\"Cover\":>7}'); \
	print(f'  {\"-\"*45} {\"-\"*7} {\"-\"*7}'); \
	[print(f'  {r[0]:<45} {r[1]:>7} {r[2]:>6.1f}%') for r in rows if r[1]>0]; \
	print()"

# Generate HTML coverage report and open in browser
coverage-html: check-deps
	cargo llvm-cov --html --open
	@echo ""
	@echo "✅ Coverage report: target/llvm-cov/html/index.html"

# Run clippy linter
lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Check for known vulnerabilities in dependencies
audit:
	cargo audit

# Run formatter check
fmt-check:
	cargo fmt -- --check

# Format code
fmt:
	cargo fmt

# Clean build artifacts
clean:
	cargo clean
	rm -rf test-outputs/
	rm -f skilldo.toml

# Install to ~/.cargo/bin
install:
	cargo install --path .

# Run the binary
run:
	cargo run -- $(ARGS)

# Example: Generate SKILL.md for a package
example:
	./target/release/skilldo generate /tmp/test-repos/click --output test-outputs/example.md

# Docker build
docker:
	docker build -t skilldo:latest .

# Docker run
docker-run:
	docker run --rm -v $(PWD):/workspace skilldo:latest generate /workspace/$(REPO)

# Generate CHANGELOG.md from conventional commits (requires git-cliff)
changelog:
	@if ! git-cliff --version >/dev/null 2>&1; then \
		echo "Installing git-cliff..."; \
		cargo install git-cliff; \
	fi
	git-cliff -o CHANGELOG.md
	@echo "✅ CHANGELOG.md updated"

# Quick development cycle
dev: fmt lint test

# Full CI/CD pipeline
ci: fmt-check lint audit test coverage

# Check if ready for release
pre-release: clean release test
	@echo "✅ Ready for release!"
	@echo "Binary: target/release/skilldo"
	@echo "Tests: All passing"

# Display project stats
stats:
	@echo "Code Statistics:"
	@echo "==============="
	@echo "Rust files: $(shell find src -name '*.rs' | wc -l)"
	@echo "Lines of code: $(shell find src -name '*.rs' | xargs wc -l | tail -1 | awk '{print $$1}')"
	@echo "Tests: $(shell cargo test --lib 2>&1 | grep 'test result:' | head -1 | awk '{print $$3}')"
	@echo ""
	@echo "Binary size:"
	@echo "  Debug: $(shell ls -lh target/debug/skilldo 2>/dev/null | awk '{print $$5}' || echo 'Not built')"
	@echo "  Release: $(shell ls -lh target/release/skilldo 2>/dev/null | awk '{print $$5}' || echo 'Not built')"
