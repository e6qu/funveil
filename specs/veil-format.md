# Veil Format

How files look on disk when veiled, and how content is preserved.

## Full Veil

The file is **physically removed from disk**. Original content is stored in
[CAS](storage.md) and tracked in the [config](config.md) `objects` map. Parsed
symbol metadata is stored alongside the CAS object (see
[storage.md](storage.md#metadata)).

On unveil, the file is restored from CAS with its original permissions.

### Legacy Markers

Older versions replaced the file with a single-line `...\n` marker instead of
removing it. These legacy markers are recognized on read and can be migrated to
physical removal with `fv apply`. The `fv doctor` command detects legacy
markers and recommends migration.

## Partial Veil (Markers)

Veiled line ranges are replaced with markers that preserve line count.

**Single line** (e.g. line 4):

```
...[a3f7d2e]...
```

**Multiple lines** (e.g. lines 4–7):

```
...[a3f7d2e]
                ← blank lines preserve count

...
```

| Position | Format |
|----------|--------|
| First (and only) line | `...[hash]...` |
| First of multiple | `...[hash]` |
| Middle lines | blank |
| Last line | `...` |

The hash is the first 7 characters of the SHA-256, matching git's short hash
convention (~268M combinations).

### Marker Regex

```regex
^\.\.\.\[[0-9a-f]+\]\.{0,3}$
```

### Marker Integrity

Before veiling, funveil checks:

1. **Collision detection**: file content must not already contain lines
   matching the marker pattern (prevents confusion with real content)
2. **Existing marker validation**: if the file already has veils, all markers
   must match the config before adding new ones

## Write Protection

Files with any veiled content are set to `chmod 444` (read-only). Original
permissions are stored in the config and restored on unveil.

## Binary Files

Detected by extension (common binary types) or content analysis (first 8KB
checked for null bytes). Binary files can only be veiled in full — partial
line ranges are rejected.

## Line Preservation

Veiled files maintain the same total line count. Visible content retains its
original line numbers. File size changes (markers differ from original
content).

## Line Endings

Markers use the same line ending style (LF or CRLF) as the original file.
Original line endings are restored from CAS on unveil.

## Language-Aware Annotations

When veiling in header/outline mode, funveil uses language-appropriate
annotations to indicate hidden content:

**Python:**

```python
def process_payment(amount, currency):
    ... # 15 lines hidden
```

**C-style languages** (Rust, Go, TypeScript, Java, C, C++):

```rust
fn process_payment(amount: f64, currency: &str) -> Result<Receipt> { ... 15 lines ... }
```

The annotation style is determined by the file's detected language. Files
without a recognized language fall back to the `...` marker style.

## Default Exclusions

Always excluded from veiling:

- VCS directories: `.git/`, `.svn/`, `.hg/`, `.cvs/`, `bzr/`, `.fslckout/`,
  `_FOSSIL_`, `_darcs/`, `CVS/`
- Funveil's own files: `.funveil/`, `.funveil_config`
