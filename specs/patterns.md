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

## Path Validation

All paths are relative to the project root.

**Invalid (error):**

- `./README.md` — relative path prefix
- `../config.yaml` — parent traversal
- `.env` — starts with dot without a directory prefix

**Symlinks** are followed but must resolve within the project root.
