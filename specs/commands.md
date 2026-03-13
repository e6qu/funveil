# CLI Commands

## Core

```
fv init [--mode blacklist|whitelist]
```

Initialize funveil. Creates `.funveil/` and `.funveil_config`. Default mode:
whitelist.

```
fv veil <pattern>[#ranges]
```

Hide a file, directory, line range, or regex pattern. In blacklist mode: adds
to blacklist. In whitelist mode: adds as a blacklist exception.

```
fv unveil <pattern>[#ranges]
fv unveil --all
```

Reveal content. In blacklist mode: removes from blacklist. In whitelist mode:
adds to whitelist. `--all` unveils everything.

```
fv apply
```

Re-apply veils to all files. Use after editing unveiled files. Auto-veils new
files matching existing patterns.

```
fv restore
```

Restore the previous veil state. Use after `unveil --all` + git commit.

```
fv status
```

Show current veil state — mode, listed patterns, veiled file counts.

```
fv show <file>
```

Display a file with veil annotations showing which lines are veiled.

```
fv mode [blacklist|whitelist]
```

Show or change the configuration mode.

## Checkpoints

```
fv checkpoint save <name>
fv checkpoint restore <name>
fv checkpoint list
fv checkpoint show <name>
```

Save/restore complete veil state snapshots. Restore auto-saves current state
as `auto-before-restore` first. See [specs/storage.md](storage.md) for the
checkpoint format.

## Code Analysis

```
fv parse <file> [--format summary|detailed]
```

Parse a file and display its structure (functions, classes, imports).

```
fv trace --from <func> [--depth N] [--format tree|list|dot] [--no-std]
fv trace --to <func> [--depth N]
fv trace --from-entrypoint [--depth N]
```

Trace call graph forward or backward. See
[docs/LANGUAGE_FEATURES.md](../docs/LANGUAGE_FEATURES.md) for language
support.

```
fv entrypoints [--entry-type main|test|cli|handler|export] [--language <lang>]
```

List detected entrypoints. See
[docs/LANGUAGE_FEATURES.md](../docs/LANGUAGE_FEATURES.md) for per-language
detection rules.

```
fv veil <file> --mode headers
```

Veil file bodies, keeping only function/class signatures visible.

## Maintenance

```
fv doctor
```

Check veil integrity: object hashes, config validity, orphaned objects, file
permissions.

```
fv gc
```

Garbage-collect unreferenced objects from CAS. See
[specs/storage.md](storage.md).

```
fv clean
```

Remove all funveil data. Unveil first.

```
fv cache status|clear|invalidate
```

Manage the analysis cache. See [specs/storage.md](storage.md).

## Pattern Syntax

See [specs/patterns.md](patterns.md) for full pattern, regex, and line range
documentation.
