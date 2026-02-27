.PHONY: help build test clean install lint audit run-tests coverage release docker check-deps changelog

# Default target
help:
	@echo "Skilldo - SKILL.md Generator"
	@echo ""
	@echo "Available targets:"
	@echo "  build       - Build debug binary"
	@echo "  release     - Build optimized release binary"
	@echo "  test        - Run all tests (requires uv)"
	@echo "  test-fast   - Run tests in parallel"
	@echo "  coverage    - Generate HTML test coverage report (requires cargo-llvm-cov, uv)"
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

# Check that dev dependencies are installed
check-deps:
	@echo "Checking development dependencies..."
	@command -v cargo >/dev/null 2>&1 || { echo "❌ cargo not found (install Rust: https://rustup.rs)"; exit 1; }
	@command -v uv >/dev/null 2>&1 || { echo "❌ uv not found (install: pip install uv or brew install uv)"; exit 1; }
	@echo "✅ All dependencies found"

# Run all tests (requires uv for Agent 5 executor tests)
test: check-deps
	cargo test --all

# Run tests in parallel (faster)
test-fast: check-deps
	cargo test --all -- --test-threads=8

# Run specific test
test-one:
	cargo test $(TEST)

# Generate HTML coverage report (requires cargo-llvm-cov and uv)
coverage: check-deps
	@if ! cargo llvm-cov --version >/dev/null 2>&1; then \
		echo "Installing cargo-llvm-cov..."; \
		cargo install cargo-llvm-cov; \
	fi
	cargo llvm-cov --html
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
