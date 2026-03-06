# Funveil Makefile
# Run these same commands locally that CI runs

.PHONY: all check fmt lint test audit deny build clean help ci

# Default target runs all checks
all: check lint test

# Format code
fmt:
	@echo "==> Formatting code..."
	cargo fmt --all

# Check formatting without modifying
fmt-check:
	@echo "==> Checking formatting..."
	cargo fmt --all -- --check

# Run clippy lints
lint:
	@echo "==> Running clippy..."
	cargo clippy --all-targets --all-features -- -D warnings

# Run all checks (format, clippy, build)
check: fmt-check lint
	@echo "==> Checking build..."
	cargo check --all-targets --all-features

# Run tests
test:
	@echo "==> Running tests (debug)..."
	cargo test --all-features --verbose
	@echo "==> Running tests (release)..."
	cargo test --release --all-features --verbose

# Run tests only (quick)
test-quick:
	@echo "==> Running tests..."
	cargo test --all-features

# Security audit with cargo-audit
audit:
	@echo "==> Running cargo audit..."
	@if ! command -v cargo-audit >/dev/null 2>&1; then \
		echo "cargo-audit not installed. Install with: cargo install cargo-audit"; \
		exit 1; \
	fi
	cargo audit

# Dependency and license check with cargo-deny
deny:
	@echo "==> Running cargo deny..."
	@if ! command -v cargo-deny >/dev/null 2>&1; then \
		echo "cargo-deny not installed. Install with: cargo install cargo-deny"; \
		exit 1; \
	fi
	cargo deny check

# Run all security checks
security: audit deny

# Build debug and release
build:
	@echo "==> Building debug..."
	cargo build --verbose
	@echo "==> Building release..."
	cargo build --release --verbose

# Clean build artifacts
clean:
	@echo "==> Cleaning build artifacts..."
	cargo clean

# Full CI pipeline (what GitHub Actions runs)
ci: check test security build
	@echo "==> All CI checks passed!"

# Install development tools
install-tools:
	@echo "==> Installing development tools..."
	cargo install cargo-audit --locked
	cargo install cargo-deny --locked
	cargo install cargo-outdated --locked
	cargo install cargo-semver-checks --locked

# Check for outdated dependencies
outdated:
	@echo "==> Checking for outdated dependencies..."
	@if ! command -v cargo-outdated >/dev/null 2>&1; then \
		echo "cargo-outdated not installed. Install with: cargo install cargo-outdated"; \
		exit 1; \
	fi
	cargo outdated

# Show help
help:
	@echo "Funveil Makefile"
	@echo ""
	@echo "Available targets:"
	@echo "  make fmt          - Format code"
	@echo "  make fmt-check    - Check formatting without modifying"
	@echo "  make lint         - Run clippy lints"
	@echo "  make check        - Run all checks (fmt-check + lint + build check)"
	@echo "  make test         - Run tests (debug and release)"
	@echo "  make test-quick   - Run tests only (faster)"
	@echo "  make audit        - Run security audit"
	@echo "  make deny         - Run license/advisory check"
	@echo "  make security     - Run all security checks"
	@echo "  make build        - Build debug and release"
	@echo "  make ci           - Full CI pipeline (what GitHub Actions runs)"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make install-tools - Install required development tools"
	@echo "  make outdated     - Check for outdated dependencies"
	@echo "  make help         - Show this help"
