# Funveil Makefile
# Run these same commands locally that CI runs

.PHONY: all check fmt lint test audit deny audit-python build clean help ci update-badges badge-check install-hooks

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

# Run specific test categories
test-unit:
	@echo "==> Running unit tests..."
	cargo test --lib --verbose

test-integration:
	@echo "==> Running integration tests..."
	cargo test --test integration_test --verbose

test-cli:
	@echo "==> Running CLI tests..."
	cargo test --test cli_test --verbose

# Run E2E tests in Docker
test-e2e:
	@echo "==> Running E2E tests in Docker..."
	@docker build -t funveil-e2e -f e2e/Dockerfile . && docker run --rm funveil-e2e

# Run E2E tests locally (requires binary built)
test-e2e-local:
	@echo "==> Running E2E tests locally..."
	@cargo build --release
	@./e2e/run-e2e.sh

# Build E2E Docker image
e2e-build:
	@echo "==> Building E2E Docker image..."
	docker build -t funveil-e2e -f e2e/Dockerfile .

# Run E2E interactive shell
e2e-shell:
	@echo "==> Starting E2E interactive shell..."
	docker-compose -f e2e/docker-compose.yml run --rm e2e-dev /bin/bash

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

# Audit Python dependencies for vulnerabilities
audit-python:
	@echo "==> Auditing Python dependencies..."
	uv run pip-audit

# Run all security checks
security: audit deny audit-python

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

# Install pre-commit hooks (uses uv for Python dependency management)
install-hooks:
	@echo "==> Setting up Python tooling via uv..."
	@if ! command -v uv >/dev/null 2>&1; then \
		echo "uv not installed. Install with: curl -LsSf https://astral.sh/uv/install.sh | sh"; \
		exit 1; \
	fi
	uv sync
	uv run pre-commit install --hook-type pre-commit --hook-type commit-msg --hook-type pre-push

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

# Update README badges with current LOC and test counts
update-badges:
	@echo "==> Updating README badges..."
	@TEST_COUNT=$$(cargo test -- --list 2>/dev/null | grep -c ': test$$'); \
	LOC=$$(find src -name '*.rs' -exec cat {} + | wc -l | tr -d ' '); \
	LOC_FMT=$$(echo "$$LOC" | rev | sed 's/[0-9]\{3\}/&,/g' | rev | sed 's/^,//' | sed 's/,/%2C/g'); \
	perl -i -pe "s|<!-- badge:loc -->.*|<!-- badge:loc -->[![Lines of Code](https://img.shields.io/badge/LOC-$${LOC_FMT}-blue)](https://github.com/e6qu/funveil)|" README.md; \
	perl -i -pe "s|<!-- badge:tests -->.*|<!-- badge:tests -->[![Test Count](https://img.shields.io/badge/Tests-$${TEST_COUNT}-green)](https://github.com/e6qu/funveil)|" README.md; \
	echo "Updated: $$TEST_COUNT tests, $$LOC LOC"

# Verify README badges are up to date (used by CI)
badge-check:
	@echo "==> Checking README badges are up to date..."
	@cp README.md README.md.bak
	@$(MAKE) update-badges --no-print-directory
	@if diff -q README.md README.md.bak >/dev/null 2>&1; then \
		rm README.md.bak; \
		echo "Badges are up to date ✓"; \
	else \
		mv README.md.bak README.md; \
		echo "::error::README badges are stale. Run 'make update-badges' and commit."; \
		exit 1; \
	fi

# Show help
help:
	@echo "Funveil Makefile"
	@echo ""
	@echo "Available targets:"
	@echo ""
	@echo "Code Quality:"
	@echo "  make fmt          - Format code"
	@echo "  make fmt-check    - Check formatting without modifying"
	@echo "  make lint         - Run clippy lints"
	@echo "  make check        - Run all checks (fmt-check + lint + build check)"
	@echo ""
	@echo "Testing:"
	@echo "  make test         - Run all tests (debug and release)"
	@echo "  make test-quick   - Run tests only (faster)"
	@echo "  make test-unit    - Run unit tests only"
	@echo "  make test-integration - Run integration tests only"
	@echo "  make test-cli     - Run CLI tests only"
	@echo "  make test-e2e     - Run E2E tests in Docker"
	@echo "  make test-e2e-local - Run E2E tests locally"
	@echo ""
	@echo "Docker E2E:"
	@echo "  make e2e-build    - Build E2E Docker image"
	@echo "  make e2e-shell    - Start E2E interactive shell"
	@echo ""
	@echo "Security:"
	@echo "  make audit        - Run cargo security audit"
	@echo "  make deny         - Run license/advisory check"
	@echo "  make audit-python - Audit Python dependencies for vulnerabilities"
	@echo "  make security     - Run all security checks (Rust + Python)"
	@echo ""
	@echo "Build:"
	@echo "  make build        - Build debug and release"
	@echo "  make ci           - Full CI pipeline (what GitHub Actions runs)"
	@echo "  make clean        - Clean build artifacts"
	@echo ""
	@echo "Badges:"
	@echo "  make update-badges - Update README LOC and test count badges"
	@echo "  make badge-check  - Verify badges are up to date (CI)"
	@echo ""
	@echo "Hooks:"
	@echo "  make install-hooks - Install pre-commit hooks via uv (fmt, badges, attribution)"
	@echo ""
	@echo "Development Tools:"
	@echo "  make install-tools - Install required development tools"
	@echo "  make outdated     - Check for outdated dependencies"
	@echo "  make help         - Show this help"
