# Contributing to Funveil

Thank you for your interest in contributing to Funveil!

## Development Setup

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
- Make (optional, for convenience commands)

### Install Development Tools

```bash
make install-tools
```

Or manually:

```bash
cargo install cargo-audit --locked
cargo install cargo-deny --locked
cargo install cargo-outdated --locked
cargo install cargo-semver-checks --locked
```

### Build

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Using Make
make build
```

## Development Workflow

### Running Tests

```bash
# Run all tests (debug and release)
make test

# Quick tests only
cargo test

# Test categories
make test-unit         # Unit tests only
make test-integration  # Integration tests only
make test-cli          # CLI tests only
make test-e2e          # E2E tests in Docker
make test-e2e-local    # E2E tests locally (requires binary)

# Specific test
cargo test test_name
```

#### E2E Tests

E2E tests run in Docker to ensure a clean environment:

```bash
# Build and run E2E tests
make test-e2e

# Build E2E Docker image
make e2e-build

# Interactive E2E shell for debugging
make e2e-shell

# Or manually with docker-compose
cd e2e
docker-compose up --build e2e-test
```

### Code Quality

We use the following tools to ensure code quality:

```bash
# Format code
make fmt

# Run clippy lints
make lint

# Run all checks (what CI runs)
make check
```

### Security Checks

```bash
# Run security audit
make audit

# Check licenses and advisories
make deny

# Run all security checks
make security
```

### Full CI Pipeline

Before submitting a PR, run the full CI pipeline locally:

```bash
make ci
```

This runs:
1. Format checking
2. Clippy lints
3. Build check
4. Tests (debug and release)
5. Security audit
6. License/advisory check
7. Release build

## Project Structure

```
.
├── src/
│   ├── main.rs       # CLI entry point
│   ├── lib.rs        # Library exports
│   ├── types.rs      # Core types (LineRange, ContentHash, etc.)
│   ├── error.rs      # Error types
│   ├── config.rs     # Configuration management
│   ├── cas.rs        # Content-addressable storage
│   ├── veil.rs       # Veil/unveil operations
│   └── checkpoint.rs # Checkpoint operations
├── tests/
│   ├── integration_test.rs  # Integration tests
│   └── cli_test.rs          # CLI tests
├── SPEC.md           # Specification
├── Cargo.toml        # Rust project config
├── deny.toml         # Cargo-deny config
├── rustfmt.toml      # Rustfmt config
└── Makefile          # Development commands
```

## Writing Tests

### Unit Tests

Place unit tests in the same file as the code they test, in a `#[cfg(test)]` module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        assert_eq!(1 + 1, 2);
    }
}
```

### Integration Tests

Place integration tests in `tests/` directory:

```rust
use funveil::Config;
use tempfile::TempDir;

#[test]
fn test_config_save_load() {
    let temp = TempDir::new().unwrap();
    let config = Config::new(Mode::Whitelist);
    config.save(temp.path()).unwrap();
    
    let loaded = Config::load(temp.path()).unwrap();
    assert!(loaded.mode().is_whitelist());
}
```

### CLI Tests

Use `assert_cmd` for CLI tests:

```rust
use assert_cmd::Command;

#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.arg("--help");
    cmd.assert().success();
}
```

## Code Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` to format code
- Use `cargo clippy` to catch common mistakes
- Write documentation comments for public APIs
- Use strong types (avoid raw strings/ints where possible)

## Commit Messages

Follow conventional commits format:

- `feat: add new feature`
- `fix: fix bug`
- `docs: update documentation`
- `refactor: refactor code`
- `test: add tests`
- `chore: update dependencies`

## CI/CD

The project uses GitHub Actions for CI. The workflow runs:

1. **Check**: Formatting, clippy, build check
2. **Test**: Tests on Ubuntu and macOS
3. **Security**: `cargo audit` and `cargo deny`
4. **SAST**: Semver checks and outdated dependencies
5. **Build**: Release builds on both platforms

## License

By contributing, you agree that your contributions will be licensed under the AGPL-3.0 License.
