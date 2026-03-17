# Storage

## Content-Addressable Storage (CAS)

Veiled content is stored by SHA-256 hash under `.funveil/objects/` with a
two-level directory prefix:

```
.funveil/objects/
├── a3/
│   └── f7/
│       └── d2e1234567890abcdef...   # remaining 60 chars of hash
└── b8/
    └── 2e/
        └── 9c4f1a...
```

For hash `a3f7d2e1...`:

- Level 1: first 2 chars (`a3`)
- Level 2: next 2 chars (`f7`)
- Filename: remaining 60 chars

Content is stored as raw bytes (no compression, no framing). Writes use
`create_new(true)` for idempotent deduplication — if the hash already exists,
the write is skipped.

### Hash Validation

SHA-256, represented as lowercase hex. Valid lengths: 7–64 characters. The
7-character short form (matching git's short hash) is used in
[veil markers](veil-format.md). The full 64-character hash is used for
storage paths and config entries.

## Metadata

Parsed symbol metadata is stored alongside CAS objects at
`.funveil/metadata/` using the same hash-based directory structure:

```
.funveil/metadata/
├── a3/
│   └── f7/
│       └── d2e1234567890abcdef...   # JSON metadata for that CAS object
├── index.json                        # Consolidated symbol index
```

Each metadata file is a JSON document containing:

- Language, file path
- Symbols: name, kind (function/class/struct/trait/enum/module), visibility,
  signature, line range, parameters, return type
- Imports

Metadata is extracted at veil time using the tree-sitter parser and stored
automatically for [supported source files](../docs/LANGUAGE_FEATURES.md).

### Metadata Index

`.funveil/metadata/index.json` provides O(1) symbol lookup by name:

```json
{
  "symbols": {
    "verify_token": [{"file": "src/auth.rs", "kind": "Function", "hash": "...", "signature": "..."}]
  },
  "files": {
    "src/auth.rs": {"hash": "...", "language": "rust", "symbol_count": 5}
  }
}
```

Rebuilt on every veil/unveil operation. Used by `--symbol`, `--callers-of`,
`--callees-of`, and `fv context` commands (see
[commands.md](commands.md#progressive-disclosure)).

### Manifest

`.funveil/manifest.json` is a snapshot of the current disclosure state,
readable without running any command:

```json
{
  "mode": "whitelist",
  "veiled_files": [{"path": "src/auth.rs", "veil_type": "full", "on_disk": false}],
  "unveiled_count": 45,
  "totals": {"veiled": 12, "unveiled": 45, "total": 57}
}
```

Auto-generated on every veil/unveil/apply operation. Not a source of truth
(config + CAS are), just a convenience projection for agents that can read
files but not run commands.

## History

Action history is stored at `.funveil/history/` and tracks all veil/unveil
operations with full file snapshots for undo/redo. See
[commands.md](commands.md#undoredo).

## Checkpoints

Checkpoints are saved under `.funveil/checkpoints/<name>/manifest.yaml`:

```yaml
created: 2026-03-13T15:30:45.123456Z
mode: whitelist
files:
  src/main.rs:
    hash: "a3f7d2e1234567890abcdef..."
    lines: null
    permissions: "644"
  api.py:
    hash: "b4f8e3f1234567890abcdef..."
    lines: [[10, 20], [50, 75]]
    permissions: "755"
```

| Field | Description |
|-------|-------------|
| `created` | ISO 8601 UTC timestamp |
| `mode` | Mode at checkpoint time |
| `files.<path>.hash` | SHA-256 of file content |
| `files.<path>.lines` | Veiled ranges as `[start, end]` pairs, or `null` for full veil |
| `files.<path>.permissions` | Original octal permissions |

Checkpoint names are validated: no slashes, no `..`, no control characters.

All checkpoints reference the shared CAS — identical files across checkpoints
are stored once.

### Excluded from Checkpoints

- `.funveil/` (data directory)
- `.funveil_config` (config file)
- `.git/` (version control)

## Profiles

Profiles are saved under `.funveil/profiles/<name>/` and store a snapshot of
the veil configuration. `fv profile save` captures the current whitelist,
blacklist, and mode; `fv profile load` replaces them. Profiles reference the
shared CAS — no content is duplicated. See
[commands.md](commands.md#profiles) for the CLI reference.

## Analysis Cache

Parse results are cached at `.funveil/analysis/index.bin` using `postcard`
binary serialization.

| Field | Description |
|-------|-------------|
| `version` | Cache schema version (currently `1`); mismatch resets cache |
| `created_at` | Unix timestamp |
| `entries` | Map of file path to `{mtime, size, content_hash, parsed_data}` |

Invalidation: if a file's mtime or size differs from the cached entry, it is
reparsed. Version mismatches discard the entire cache.

## Garbage Collection

`fv gc` removes unreferenced objects from CAS. An object is referenced if it
appears in:

- The current config's `objects` map
- Any checkpoint manifest

See [specs/algorithms.md](algorithms.md) for the GC algorithm.
