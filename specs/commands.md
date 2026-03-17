# CLI Commands

## Core

```
fv init [--mode blacklist|whitelist]
```

Initialize funveil. Creates `.funveil/` and `.funveil_config`. Default mode:
whitelist.

```
fv veil <pattern>[#ranges] [--mode full|headers] [--symbol <name>] [--unreachable-from <name>] [--reachable-from <name>] [--level 0-3]
```

Hide a file, directory, line range, or regex pattern. In blacklist mode: adds
to blacklist. In whitelist mode: adds as a blacklist exception.

| Flag | Description |
|------|-------------|
| `--mode headers` | Show only function/class signatures (header mode) |
| `--symbol <name>` | Veil by symbol name (uses metadata index to find the file/range) |
| `--unreachable-from <name>` | Veil everything not reachable from the named function |
| `--reachable-from <name>` | Veil all files reachable from the named function (hide the call subtree) |
| `--level <0-3>` | Disclosure level: 0=remove file, 1=signatures only, 2=signatures+called bodies, 3=full source |

```
fv unveil <pattern>[#ranges] [--all] [--symbol <name>] [--callers-of <name>] [--callees-of <name>] [--unreachable-from <name>] [--reachable-from <name>] [--level 0-3]
```

Reveal content. In blacklist mode: removes from blacklist. In whitelist mode:
adds to whitelist. `--all` unveils everything.

| Flag | Description |
|------|-------------|
| `--symbol <name>` | Unveil the file containing the named symbol |
| `--callers-of <name>` | Unveil all files containing functions that call the named symbol |
| `--callees-of <name>` | Unveil all files containing functions called by the named symbol |
| `--unreachable-from <name>` | Unveil all files unreachable from the named function |
| `--reachable-from <name>` | Unveil all files reachable from the named function |
| `--level <0-3>` | Disclosure level (see veil flags above) |

```
fv apply [--dry-run]
```

Re-apply veils to all files. Use after editing unveiled files. Auto-veils new
files matching existing patterns. Migrates legacy `...\n` marker files to
physical removal. Updates metadata index and manifest.

```
fv restore
```

Restore the previous veil state. Use after `unveil --all` + git commit.

```
fv status [--files|--verbose]
```

Show current veil state — mode, listed patterns, veiled/unveiled file counts.
By default shows aggregate counts only. With `--files` (or its alias
`--verbose`), lists individual files with their veil state and `on_disk` status.
Files physically removed from disk are reported as `on_disk: false`.

```
fv show <file> [--expand <name|*>] [--imports] [--docstrings]
```

Display a file in outline mode by default: function/class signatures are shown
but bodies are collapsed. For files not on disk (fully veiled), retrieves
content from CAS and displays with a `[VEILED - not on disk]` header.

| Flag | Description |
|------|-------------|
| `--expand <name\|*>` | Expand a specific function/class body, or `*` for the full file |
| `--imports` | Include import statements in outline |
| `--docstrings` | Include docstrings in outline |

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

## Profiles

```
fv profile save <name>
fv profile load <name>
fv profile list
fv profile delete <name>
```

Save and restore named veil configurations. `save` captures the current
configuration as a profile. `load` replaces the current configuration with the
named profile. Profiles are stored under `.funveil/profiles/`. See
[specs/storage.md](storage.md) for the storage format.

## Code Analysis

```
fv parse <file> [--format summary|detailed] [--imports] [--calls]
```

Parse a file and display its structure (functions, classes, imports). With
`--format detailed`, imports and call sites are hidden by default; use
`--imports` and `--calls` to include them.

| Flag | Description |
|------|-------------|
| `--imports` | Include imports in detailed output |
| `--calls` | Include call sites in detailed output |

```
fv trace --from <func> [--depth N] [--format tree|list|dot] [--no-std]
fv trace --to <func> [--depth N]
fv trace --from-entrypoint [--depth N]
fv trace --focus <file> [--depth N]
```

Trace call graph forward or backward. Default depth is 1. `--focus <file>`
traces from all functions in the given file. See
[docs/LANGUAGE_FEATURES.md](../docs/LANGUAGE_FEATURES.md) for language
support.

```
fv entrypoints [--entry-type main|test|cli|handler|export] [--language <lang>] [--include-tests] [--all]
```

List detected code entrypoints by default. `--all` includes non-code files
(markdown, shell, terraform, etc.). `--include-tests` includes test
entrypoints (excluded by default). See
[docs/LANGUAGE_FEATURES.md](../docs/LANGUAGE_FEATURES.md) for per-language
detection rules.

```
fv veil <file> --mode headers
```

Veil file bodies, keeping only function/class signatures visible.

## Progressive Disclosure

```
fv context <function> [--depth N]
```

Unveil a function and its call graph dependencies up to the specified depth
(default: 2). Uses the metadata index and call graph to determine the minimum
set of files needed to understand the function.

```
fv disclose --budget <tokens> --focus <path> [--focus <path2> ...] [--show] [--strict]
```

Compute a disclosure plan within a token budget. `--focus` is repeatable for
multi-focus disclosure. `--show` emits actual code instead of just the plan.
`--strict` errors if the budget is exceeded instead of truncating. Outputs
which files to disclose at which level:

- Level 3 (full source) for the focus file
- Level 2 (signatures + called bodies) for direct dependencies
- Level 1 (signatures only) for remaining reachable code

See [storage.md](storage.md#metadata) for the metadata system design.

## Undo/Redo

```
fv undo [--force]
fv redo
fv history [--limit N] [--show <id>]
```

Reverse or replay veil/unveil operations. Action history tracks all state
changes with full file snapshots for rollback.

## Maintenance

```
fv doctor
```

Check veil integrity: object hashes, config validity, orphaned objects, file
permissions. Also detects legacy `...\n` marker files and missing metadata.

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

## Global Flags

| Flag | Description |
|------|-------------|
| `--quiet` / `-q` | Suppress output |
| `--json` | Output as JSON (for machine consumption / agent integration) |
| `--log-level <level>` | Log level: trace, debug, info, warn, error, off |

## Pattern Syntax

See [specs/patterns.md](patterns.md) for full pattern, regex, and line range
documentation.
