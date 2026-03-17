# Pattern and Path Format

Patterns appear in the `whitelist` and `blacklist` arrays of
[`.funveil_config`](config.md) and as arguments to `fv veil` / `fv unveil`.

## Literal Paths

```
secrets.env           # Entire file
src/internal/         # Entire directory (trailing /)
```

## Line Ranges

Appended with `#`, using 1-indexed inclusive ranges:

```
file.py#10-20         # Lines 10 through 20
file.py#10-20,30-40   # Multiple ranges
```

Rules:

- Start must be <= end
- Ranges must not overlap
- Lines beyond file length are clamped
- Binary files cannot have line ranges
- Directories cannot have line ranges

The `#` delimiter is found via `rfind('#')`, so filenames containing `#`
(e.g. `file#name.txt#1-10`) work correctly.

## Regex Patterns

Wrapped in `/`, matched against the full relative path:

```
/.*\.env$/            # All .env files
/test_.*\.py$/        # Test files
/src\/public\/.*/     # All files under src/public/
```

Regex patterns can also have line ranges:

```
/.*\.env$/#10-20
```

## Glob Patterns

Shell-style glob patterns for matching multiple files:

```
*.py                  # All Python files in the current directory
src/**/*.rs           # All Rust files under src/ (recursive)
config/?.yaml         # Single-character YAML files in config/
src/[abc]*.ts         # TypeScript files starting with a, b, or c
```

| Syntax | Meaning |
|--------|---------|
| `*` | Match any characters except `/` |
| `?` | Match exactly one character except `/` |
| `[...]` | Match one character from the set |
| `**` | Match zero or more directories |

Glob patterns are expanded at the time they are used — they match against the
current file tree. Unlike regex patterns, globs are not stored in the config;
they expand to literal paths.

## Path Validation

All paths are relative to the project root.

**Invalid (error):**

- `./README.md` — relative path prefix
- `../config.yaml` — parent traversal
- `.env` — starts with dot without a directory prefix

**Symlinks** are followed but must resolve within the project root.
