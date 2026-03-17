# AGENTS.md

## Coding Style

### Architecture

- **Functional Core, Imperative Shell**: Keep business logic pure and functional. Side effects (I/O, mutations) should be isolated to the outer shell.
- **Early Exit**: Return early from functions to reduce nesting and cognitive complexity.
- **Inverted Conditionals**: Use inverted `if` conditions where they reduce indentation and improve readability.

### Type Safety

- **Strong Types**: Prefer strongly-typed structures over primitives. Use newtypes or enums to encode domain concepts.
- **Strong Type Checking**: Leverage the type system to catch errors at compile time. Avoid `unwrap()` in production code; use `?` or explicit error handling.
- **Formal Verification**: Where applicable, use type-level guarantees and invariants that the compiler can verify.

### Error Handling

- Use `Result<T, E>` with custom error types.
- Propagate errors with `?` operator.
- Avoid panics in library code.

### Memory Management

- **Generator Style**: Process data lazily using iterators and generators where possible.
- **Streaming**: Avoid loading entire datasets into memory. Stream data when processing large inputs.
- **Ownership**: Prefer borrowing over cloning. Use references when data doesn't need to be owned.

## Testing Strategy

### Test Pyramid

Follow the standard test pyramid:

1. **Unit Tests** (most numerous)
   - Test individual functions and modules in isolation
   - Fast execution, no external dependencies
   - Located in `#[cfg(test)]` modules within source files
   - Focus on edge cases and error paths

2. **Integration Tests** (moderate number)
   - Test interactions between components
   - Located in `tests/` directory
   - May use test fixtures and temporary directories
   - Focus on component boundaries and real usage patterns

3. **BDD Acceptance Tests**
   - Gherkin feature files in `tests/features/`
   - Step definitions in `tests/bdd.rs` using cucumber-rs
   - Pin down requirements for physical removal, metadata, query-based unveiling, layered disclosure, and budget mode
   - Run with `cargo test --test bdd`

4. **End-to-End Tests** (fewest)
   - Test complete user workflows through CLI
   - Located in `e2e/` directory
   - Full system tests with real I/O
   - Focus on critical user journeys

### Test Quality

- Tests should be deterministic and isolated.
- Use descriptive test names that explain what is being tested.
- Each test should verify one specific behavior.
- Avoid test interdependencies.

### Coverage Floors (Sacred)

CI enforces absolute coverage floors. These are **non-negotiable** and must never be
lowered for any reason:

- **96% line coverage**
- **87% branch coverage**

Measured with `cargo +nightly llvm-cov --all-features --branch`.

Rules:

- **Never lower the floors.** Not temporarily, not "just for this PR", not ever.
- **No workarounds.** Do not use `#[cfg_attr(coverage_nightly, coverage(off))]` on
  production code to game the numbers. That attribute is only for test harness code
  (e.g., `main()` in `tests/bdd.rs`).
- **Coverage must be real.** Every percentage point must come from actual test
  execution of production code paths, not from excluding code from measurement.
- If a new feature drops coverage below the floors, add tests before merging.
- **Uncoverable lines are a smell.** When reviewing coverage reports, treat lines
  that seem impossible to cover as signals of dead code or latent bugs — not as
  acceptable gaps. A branch that "can't be reached" in tests often means the
  condition is always true/false in practice, which means the code is either dead
  (remove it) or hiding a bug (the condition should fire but doesn't due to an
  upstream mistake). Always investigate before accepting uncovered lines.

## Code Organization

- Keep modules focused and cohesive.
- Prefer small functions with clear responsibilities.
- Use descriptive names that reveal intent.
- Document public APIs with doc comments (`///`).
- Avoid unnecessary comments; code should be self-documenting.

## Rust-Specific Guidelines

- Use `clippy` with `-D warnings` to catch common issues.
- Format code with `rustfmt`.
- Prefer standard library types over external dependencies when reasonable.
- Use `Option` and `Result` explicitly rather than throwing exceptions.
- Leverage the borrow checker to enforce ownership semantics.
