# Algorithms

## Veil Resolution

**Blacklist mode** — a line is veiled if any blacklist entry matches:

```
for entry in blacklist:
    if matches(file, entry):
        if entry has no ranges → veil entire file
        if line in entry's ranges → veil line
not veiled
```

**Whitelist mode** — blacklist exceptions are checked first, then whitelist:

```
for entry in blacklist:
    if matches(file, entry) and (no ranges or line in ranges):
        → veiled
for entry in whitelist:
    if matches(file, entry) and (no ranges or line in ranges):
        → not veiled
→ veiled (default)
```

Entries are applied in config file order.

## Veil Operation

1. Read file, determine which lines to veil
2. Extract hidden content, compute SHA-256
3. Store content in [CAS](storage.md)
4. Record hash + permissions in [config](config.md) `objects`
5. Replace lines with [markers](veil-format.md)
6. Set file read-only (`chmod 444`)

## Unveil Operation

1. Look up hash from config `objects`
2. Retrieve content from CAS
3. Restore content in working file
4. Restore original permissions
5. Remove entry from config `objects`
6. If no veils remain, file becomes writable

Content is not deleted from CAS (use `fv gc` separately).

## Checkpoint Save

1. Compute SHA-256 for each project file
2. Store content in CAS if not present
3. Write manifest with paths, hashes, permissions, veil ranges

## Checkpoint Restore

1. Auto-save current state as `auto-before-restore`
2. Read manifest, retrieve content from CAS
3. Overwrite working files, restore permissions
4. Re-apply veil state from manifest

## Garbage Collection

1. Collect all hashes referenced by current config and all checkpoint manifests
2. Walk `.funveil/objects/` and delete unreferenced objects
3. Report freed space

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Empty file, partial veil | Error: cannot partially veil empty file |
| Empty file, full veil | Allowed (displays as `...`) |
| CRLF line endings | Markers use CRLF; original endings restored from CAS |
| Duplicate config entries | Allowed but redundant |
| Overlapping ranges | Last range wins |
| Lines beyond file length | Clamped to file length |
| Interrupted operation | Run `fv doctor`; restore from checkpoint if needed |
| Concurrent access | No locking; last writer wins |
| Symlinks | Followed; must resolve within project root |
| Hard links | Standard behavior; only the referenced path is affected |
| Extended attributes | Not preserved; only Unix mode is stored |
| File mtime | Not preserved (veiled file has new content) |
