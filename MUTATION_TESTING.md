# Mutation Testing in Rust

A practical guide to mutation testing for this project, using
[cargo-mutants](https://mutants.rs/).

## Current Status

| Metric | Value |
|--------|-------|
| Total mutants | 1,103 |
| Caught | 868 (85.3%) |
| Missed | 137 |
| Timeout | 13 |
| Unviable | 85 |
| **Catch rate** | **85.3%** |

### Per-file Breakdown

| File | Missed | Notes |
|------|--------|-------|
| veil.rs | 47 | Mostly `!quiet` guards and directory counter mutations |
| main.rs | 35 | CLI output control (`!quiet`), counter increments |
| checkpoint.rs | 14 | `!quiet` guards in save/restore |
| tree\_sitter\_parser.rs | 12 | Match arm deletions, line offset arithmetic |
| patch/parser.rs | 8 | Line counter increments (some cause timeouts) |
| Other files | 21 | 1-5 per file; most are near-equivalent |

### Near-equivalent Mutants

Many remaining missed mutants are **near-equivalent** — they change code that
only affects stderr diagnostic output, not observable behavior:

- **`!quiet` guard deletions**: Removing `!` in `if !quiet { eprintln!(...) }`
  inverts who sees warnings but doesn't change return values or side effects
- **Counter mutations**: `file_errors += 1` → `*= 1` where the counter is only
  used for a warning message and the function always returns `Ok(())`
- **Equivalent guards**: e.g., `suffix == "_original"` where the guarded code
  would fail on a later parse anyway

## What Is Mutation Testing?

Mutation testing injects small bugs ("mutants") into your source code and
checks whether your test suite catches them. Unlike code coverage, which only
tells you a line was *executed*, mutation testing tells you a line was actually
*verified* — that a test fails when the behavior changes.

Each mutant is a small, targeted change to the source code:

- Replacing a function body with a default return value
- Swapping `==` for `!=`, or `&&` for `||`
- Deleting a unary `-` or `!`
- Removing a match arm

If the tests still pass after a mutation, that's a **missed mutant** — a gap
in test coverage worth investigating.

## Tool: cargo-mutants

[cargo-mutants](https://github.com/sourcefrog/cargo-mutants) is the
recommended mutation testing tool for Rust. It is actively maintained, works
on stable Rust, requires no source code changes, and produces actionable
output.

### Why cargo-mutants over mutagen?

| Feature | cargo-mutants | mutagen |
|---------|--------------|---------|
| Requires source changes | No | Yes (`#[mutate]` attribute) |
| Compiler requirement | Stable | Nightly only |
| Maintenance status | Active (2025+) | Unmaintained (3+ years) |
| Setup | `cargo install` + run | Add dependency + annotate code |

## Installation

```bash
cargo install cargo-mutants
```

## Quick Start

```bash
# List all mutants that would be generated (dry run)
cargo mutants --list

# Run mutation testing on the whole project
cargo mutants

# Run only on files changed since main
git diff main... | cargo mutants --in-diff -

# Run on a specific file
cargo mutants -f src/veil.rs

# Run on a specific function (regex match)
cargo mutants --re 'veil_file'
```

## How It Works

1. **Discover**: Scans source files for functions via `cargo metadata`
2. **Generate**: For each function, creates mutations based on its return type
3. **Baseline**: Runs `cargo test` on unmodified code to ensure tests pass
4. **Mutate**: For each mutant:
   - Applies the mutation textually to a copy of the source tree
   - Runs `cargo test --no-run` (compilation check)
   - Runs `cargo test` (if it compiled)
   - Records whether the mutant was caught, missed, unviable, or timed out
5. **Report**: Outputs results to `mutants.out/` and stdout

Mutations are applied *textually* (not at the AST level), so unmutated code
retains its formatting, comments, and line numbers.

## Mutation Types

### Function Return Value Replacement

The primary mutation strategy: replace a function body with a value matching
its return type.

| Return type | Replacement values |
|-------------|-------------------|
| `()` | `()` (no side effects) |
| `bool` | `true`, `false` |
| `i8`..`i128`, `isize` | `0`, `1`, `-1` |
| `u8`..`u128`, `usize` | `0`, `1` |
| `f32`, `f64` | `0.0`, `1.0`, `-1.0` |
| `String` | `String::new()`, `"xyzzy".into()` |
| `&str` | `""`, `"xyzzy"` |
| `Vec<T>` | `vec![]`, `vec![<T replacement>]` |
| `Option<T>` | `None`, `Some(<T replacement>)` |
| `Result<T, E>` | `Ok(<T replacement>)` |
| `Box<T>` | `Box::new(<T replacement>)` |
| `Arc<T>`, `Rc<T>` | `Arc::new(...)`, `Rc::new(...)` |
| `HashMap<K,V>` | `HashMap::new()`, single-entry maps |
| `HashSet<T>` | `HashSet::new()`, single-element sets |
| `Cow<'_, T>` | `Cow::Borrowed(...)`, `Cow::Owned(...)` |
| `(A, B, ...)` | Product of all inner type replacements |
| `[T; N]` | `[<T replacement>; N]` |
| Any other `T` | `Default::default()` |

Nested types are handled recursively. For example, `Result<Option<String>>`
generates `Ok(None)`, `Ok(Some(String::new()))`, `Ok(Some("xyzzy".into()))`.

### Binary Operator Mutations

| Original | Replacements |
|----------|-------------|
| `==` | `!=` |
| `!=` | `==` |
| `&&` | `\|\|` |
| `\|\|` | `&&` |
| `<` | `==`, `>` |
| `>` | `==`, `<` |
| `<=` | `>` |
| `>=` | `<` |
| `+` | `-`, `*` |
| `-` | `+`, `/` |
| `*` | `+`, `/` |
| `/` | `%`, `*` |
| `%` | `/`, `+` |
| `<<` | `>>` |
| `>>` | `<<` |
| `&` | `\|`, `^` |
| `\|` | `&`, `^` |
| `^` | `&`, `\|` |

Assignment operators (`+=`, `-=`, etc.) follow the same rules as their base
operators.

### Unary Operator Mutations

Unary `-` and `!` are *deleted* (not replaced), because replacing them tends
to produce too many unviable mutants.

### Other Mutations

- **Match arms**: Deleted when a wildcard pattern exists
- **Match arm guards**: Replaced with `true` and `false`
- **Struct literal fields**: Deleted when a base expression like
  `..Default::default()` is present

### Dealing with String Return Types

Functions returning `String` or `&str` get mutations to `""` / `String::new()`
and `"xyzzy"` / `"xyzzy".into()`. To catch these:

- **Assert non-empty**: If a function should never return an empty string,
  test that `!result.is_empty()`
- **Assert specific content**: Check for expected substrings or exact values
- **Assert structure**: For formatted output, verify structural properties
  (contains newline, starts with prefix, etc.)

Common pattern for catching string mutants:

```rust
#[test]
fn test_format_output_is_meaningful() {
    let result = format_report(&data);
    // Catches String::new() mutation
    assert!(!result.is_empty());
    // Catches "xyzzy" mutation
    assert!(result.contains("expected_substring"));
}
```

## Interpreting Results

cargo-mutants produces four outcomes:

| Outcome | Meaning | Action |
|---------|---------|--------|
| **Caught** | A test failed when the mutant was applied | None — good coverage |
| **Missed** | No test failed — potential coverage gap | Investigate and add tests |
| **Unviable** | The mutant didn't compile | None — inconclusive |
| **Timeout** | Tests hung or ran too long | Investigate; consider `#[mutants::skip]` |

### What To Do With Missed Mutants

1. **Look for patterns**: Clusters of missed mutants in related functions
   often indicate a missing test for an entire feature
2. **Prioritize by surprise**: Focus on mutations where it's *surprising*
   that tests didn't catch it (e.g., replacing a critical validation with
   `Ok(())`)
3. **Write behavioral tests**: Don't write tests that target the mutation
   itself — write tests that assert correct behavior through the public API
4. **Skip when appropriate**: Some mutations are legitimately equivalent to
   the original code (e.g., changing a log message). Use `#[mutants::skip]`
   for these

### When To Skip Instead of Adding Tests

Use `#[mutants::skip]` for:

- `Debug`, `Display` implementations that are only for logging
- Functions where mutations produce functionally identical code
- Performance-only code paths (caching, pre-allocation)
- Functions that would require prohibitively complex test setups

## Configuration

The project configuration lives in `.cargo/mutants.toml`:

```toml
# Exclude test files and language-specific tree-sitter parsers
exclude_globs = [
    "tests/**/*.rs",
    "src/parser/languages/**/*.rs",
]

# Skip Debug/Display/Default impls — mutations here rarely indicate real gaps
exclude_re = [
    "impl Debug",
    "impl Display",
    "impl fmt::Display",
    "impl Default",
]

# Don't mutate calls to these — side-effect-only functions
skip_calls = [
    "println!", "eprintln!",
    "debug!", "info!", "warn!", "error!", "trace!",
]
skip_calls_defaults = true

# Use leaner profile for faster incremental builds
profile = "mutants"

# Timeout: 5x baseline (some tests do file I/O)
timeout_multiplier = 5.0
minimum_test_timeout = 30.0
```

### Custom Build Profile

Disable debug symbols to speed up incremental builds:

```toml
# In Cargo.toml
[profile.mutants]
inherits = "test"
debug = "none"
```

## Performance Optimization

Mutation testing is inherently slow (one build+test per mutant). Key
strategies:

### 1. Faster Linker

Incremental builds make link time significant. Use
[mold](https://github.com/rui314/mold) or
[wild](https://github.com/niclas-parm/wild):

```toml
# .cargo/config.toml
[target.aarch64-apple-darwin]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

### 2. Skip Doctests

```bash
cargo mutants -- --all-targets
```

### 3. Ramdisk (Linux)

```bash
sudo mkdir /ram && sudo mount -t tmpfs -o size=8G /ram /ram
env TMPDIR=/ram cargo mutants
```

### 4. Sharding

Split work across N machines:

```bash
# On machine k (0-indexed) of N total:
cargo mutants --shard k/N
```

All shards must use identical arguments. Recommended: 8–32 shards with at
least 10 mutants per shard.

### 5. Test Only Changed Code

```bash
# In CI, test only the PR diff
git diff origin/main... | cargo mutants --in-diff -
```

This is typically *much* faster than testing all mutants.

## CI Integration

Mutation testing is **not run in CI** for this project. A full mutation run
takes 40+ minutes (1,100+ mutants × build + test per mutant), which is too
slow for a pull request feedback loop. Instead, mutation testing is run
manually during development:

```bash
# Full run (expect ~40 min)
make mutants

# Diff-only run (much faster — only mutates changed code)
make mutants-diff

# Target a specific file
cargo mutants -f src/veil.rs
```

The results are reviewed locally and used to guide test improvements. The
`mutants.out/` directory is gitignored.

> **Note**: If CI budget allows in the future, diff-only mutation testing with
> sharding could be added. See the cargo-mutants
> [CI guide](https://mutants.rs/ci.html) for a GitHub Actions example using
> `--in-diff` and `--shard`.

## Makefile Targets

```makefile
# Run mutation testing on the full project
mutants:
	cargo mutants -vV

# Run mutation testing only on changed files (vs main)
mutants-diff:
	git diff origin/main... | cargo mutants -vV --in-diff -

# List all mutants without running tests
mutants-list:
	cargo mutants --list
```

## References

- [cargo-mutants user guide](https://mutants.rs/)
- [cargo-mutants GitHub](https://github.com/sourcefrog/cargo-mutants)
- [How it works](https://mutants.rs/how-it-works.html)
- [Mutation types reference](https://mutants.rs/mutants.html)
- [Using results](https://mutants.rs/using-results.html)
- [Performance tips](https://mutants.rs/performance.html)
- [CI integration](https://mutants.rs/ci.html)
- [Sharding](https://mutants.rs/shards.html)
- [cargo-mutants vs mutagen](https://github.com/sourcefrog/cargo-mutants/wiki/Compared)
- [Mutation Testing in Rust (blog)](https://blog.frankel.ch/mutation-testing-rust/)
