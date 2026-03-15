# Contributing to Funveil

Thank you for your interest in contributing to Funveil!

## Documentation Quick Links

- [README.md](README.md) - Project overview and quick start
- [SPEC.md](SPEC.md) - Specification index
- [specs/](specs/) - Detailed specs (config, storage, veil format, CLI, algorithms)
- [docs/TUTORIAL.md](docs/TUTORIAL.md) - User guide for LLM agents
- [docs/LANGUAGE_FEATURES.md](docs/LANGUAGE_FEATURES.md) - Supported languages & analysis features
- [docs/DESIGN_INTELLIGENT_VEILING.md](docs/DESIGN_INTELLIGENT_VEILING.md) - Architecture design
- [LANGUAGE_SUPPORT_PLAN.md](LANGUAGE_SUPPORT_PLAN.md) - Language implementation status (developer-facing)

## Development Setup

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
- Make (optional, for convenience commands)
- [pre-commit](https://pre-commit.com/) (for Git hooks)

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

### Pre-commit Hooks

Install the Git hooks after cloning:

```bash
pre-commit install
```

The following hooks run automatically:

| Hook | Trigger | What it does |
|------|---------|-------------|
| trailing-whitespace | pre-commit | Strips trailing whitespace |
| end-of-file-fixer | pre-commit | Ensures files end with a newline |
| cargo-fmt | pre-commit | Auto-formats Rust code with `cargo fmt` |
| cargo-clippy | pre-commit | Runs `cargo clippy -D warnings` |
| badge-freshness | pre-push | Verifies README badges are current |
| strip-ai-attribution | commit-msg | Strips AI attribution from commit messages |

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
cargo test --test bdd  # BDD acceptance tests (cucumber)

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
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library exports
│   ├── types.rs             # Core types (LineRange, ContentHash, etc.)
│   ├── error.rs             # Error types
│   ├── config.rs            # Configuration management
│   ├── cas.rs               # Content-addressable storage
│   ├── veil.rs              # Veil/unveil operations (physical removal)
│   ├── metadata.rs          # Metadata extraction, indexing, and manifest
│   ├── budget.rs            # Token budget mode for progressive disclosure
│   ├── history.rs           # Undo/redo action history
│   ├── checkpoint.rs        # Checkpoint operations
│   ├── output.rs            # Output formatting
│   ├── logging.rs           # Structured logging
│   ├── perms.rs             # Unix permission handling
│   ├── parser/              # Code parsing (tree-sitter, 12 languages)
│   │   ├── mod.rs
│   │   └── tree_sitter_parser.rs
│   ├── analysis/            # Code analysis (call graphs, entrypoints, cache)
│   │   ├── cache.rs
│   │   ├── call_graph.rs
│   │   └── entrypoints.rs
│   ├── patch/               # Patch parsing and management
│   │   ├── parser.rs
│   │   └── manager.rs
│   └── strategies/          # Veiling strategies (header-only, etc.)
│       ├── mod.rs
│       └── header.rs
├── tests/
│   ├── bdd.rs               # BDD acceptance tests (cucumber-rs)
│   ├── features/            # Gherkin feature files
│   │   ├── physical_removal.feature
│   │   ├── metadata.feature
│   │   ├── query_unveiling.feature
│   │   ├── layered_disclosure.feature
│   │   └── budget_mode.feature
│   ├── cli_test.rs          # CLI integration tests
│   ├── integration_test.rs  # Library integration tests
│   ├── e2e_smoke_test.rs    # End-to-end smoke tests
│   ├── property_test.rs     # Property-based tests
│   └── stress_test.rs       # Stress/performance tests
├── specs/                   # Detailed specifications
├── docs/                    # User-facing documentation
├── .cargo/mutants.toml      # Mutation testing config
├── .pre-commit-config.yaml  # Pre-commit hooks config
├── SPEC.md                  # Specification index
├── MUTATION_TESTING.md      # Mutation testing guide
├── Cargo.toml               # Rust project config
├── deny.toml                # Cargo-deny config
├── rustfmt.toml             # Rustfmt config
└── Makefile                 # Development commands
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

Use `assert_cmd` for CLI tests. Use the `cargo_bin_cmd!` macro (not the
deprecated `Command::cargo_bin()` method):

```rust
use predicates::prelude::*;

#[test]
fn test_cli_help() {
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Usage"));
}
```

### Mutation Testing

We use [cargo-mutants](https://mutants.rs/) to verify test quality beyond code
coverage. Mutation testing is run locally (not in CI — a full run takes ~40
minutes). See [MUTATION_TESTING.md](MUTATION_TESTING.md) for the full guide.

```bash
# Run mutation testing on the full project
make mutants

# Run only on files changed since main (much faster)
make mutants-diff

# Target a specific file
cargo mutants -f src/veil.rs
```

When adding tests, aim to catch mutations in the code you're testing. Focus on
asserting observable behavior — return values, side effects, error conditions —
rather than writing tests that target specific mutation patterns.

## Code Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` to format code
- Use `cargo clippy` to catch common mistakes
- Write documentation comments for public APIs
- Use strong types (avoid raw strings/ints where possible)

### Error Handling

Follow these patterns consistently:

1. **Use `Result<T>` for operations that can fail**:
   - File I/O
   - Network operations
   - Parsing
   - Config loading

2. **Use `Option<T>` for lookups**:
   - Getting items from collections
   - Finding items

3. **Use `bool` for simple checks**:
   - Predicate functions (`is_*`, `has_*`, `can_*`)

4. **Never use `unwrap()` or `expect()` in library code**

### Module Organization

- `parser/` - Parsing source code into structured representations
- `analysis/` - Analysis of parsed code (call graphs, entrypoints)
- `veil/` - Veiling/unveiling operations
- `cas/` - Content-addressable storage
- `patch/` - Patch parsing and management
- `config/` - Configuration management
- `checkpoint/` - Checkpoint save/restore operations

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
