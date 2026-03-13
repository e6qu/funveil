# Config Format

The configuration file `.funveil_config` lives in the project root. It is
protected and cannot be veiled.

## Format

YAML, serialized via `serde_yaml`. Current version: **1**.

```yaml
version: 1
mode: whitelist
whitelist:
  - README.md
  - src/public_api.py
  - src/api.rs#10-20,50-75
  - /src\/public\/.*/
blacklist:
  - secrets.env
  - .env#1-5
objects:
  secrets.env:
    hash: "a3f7d2e1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
    permissions: "644"
  src/api.rs#10-20:
    hash: "b4f8e3f1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
    permissions: "755"
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `version` | `u32` | Schema version (currently `1`) |
| `mode` | `string` | `whitelist` (default) or `blacklist` |
| `whitelist` | `[string]` | Entries visible in whitelist mode |
| `blacklist` | `[string]` | Entries hidden (or exceptions in whitelist mode) |
| `objects` | `map` | Tracks stored content per veiled path |

### Object Entries

Each key in `objects` maps a file path (with optional `#range` suffix) to:

| Field | Type | Description |
|-------|------|-------------|
| `hash` | `string` | Full SHA-256 hex string (64 chars) |
| `permissions` | `string` | Original Unix permissions as octal (e.g. `"644"`) |
| `owner` | `string?` | Reserved, currently unused |

The special suffix `#_original` stores the original full-file content when a
file has partial veils applied. This key is internal and never parsed as a
line range.

## Entry Format

See [specs/patterns.md](patterns.md) for the full pattern and line range
syntax used in `whitelist` and `blacklist` arrays.

## Gitignore Management

`fv init` adds a managed block to `.gitignore`:

```
# MANAGED BY FUNVEIL
.funveil_config
.funveil/
# END MANAGED BY FUNVEIL
```

Behavior:

- Idempotent: repeated calls produce the same result
- Repairs partial or corrupted blocks automatically
- Preserves existing `.gitignore` entries and line ending style (LF/CRLF)

## Mode Semantics

**Blacklist**: everything visible except listed entries.
**Whitelist**: everything veiled except listed entries. Blacklist entries act
as exceptions (override whitelist).

See [specs/algorithms.md](algorithms.md) for the resolution logic.
