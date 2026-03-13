# Veil Format

How files look on disk when veiled, and how content is preserved.

## Full Veil

The entire file is replaced with a single line:

```
...
```

Original content is stored in [CAS](storage.md) and tracked in the
[config](config.md) `objects` map.

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

## Default Exclusions

Always excluded from veiling:

- VCS directories: `.git/`, `.svn/`, `.hg/`, `.cvs/`, `bzr/`, `.fslckout/`,
  `_FOSSIL_`, `_darcs/`, `CVS/`
- Funveil's own files: `.funveil/`, `.funveil_config`
