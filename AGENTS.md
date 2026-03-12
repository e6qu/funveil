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

3. **End-to-End Tests** (fewest)
   - Test complete user workflows through CLI
   - Located in `e2e/` directory
   - Full system tests with real I/O
   - Focus on critical user journeys

### Test Quality

- Tests should be deterministic and isolated.
- Use descriptive test names that explain what is being tested.
- Each test should verify one specific behavior.
- Avoid test interdependencies.

### Coverage Goals

- Aim for high coverage on library code (target: 90%+).
- Focus on meaningful coverage: edge cases, error paths, and complex logic.
- Don't sacrifice test quality for coverage metrics.

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
