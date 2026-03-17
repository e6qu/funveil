# Plan

## Current: ISSUE.md Multi-Stage Fix (8 stages)

See `.claude/plans/pure-weaving-goblet.md` for full implementation details.

### Stage 1 — Veil/Unveil Correctness Bugs (BUG-202–206) ✅

- BUG-202: `fv veil <dir> --mode headers` errors "Is a directory" — add recursion
- BUG-203: `fv show` reports FULLY VEILED for header-veiled files — show actual content
- BUG-204: `fv unveil` dual-lists file in both blacklist and whitelist — remove from blacklist
- BUG-205: `--unreachable-from` has no HistoryTracker — add undo support
- BUG-206: `--symbol` veil blacklists entire file — only register partial entry

### Stage 2 — `fv trace` Argument Cleanup (BUG-207, 208) ✅

- BUG-207: Remove redundant positional [FUNCTION] arg, keep `--from` only
- BUG-208: Rewrite `--from-entrypoint` to output per-entrypoint grouped results, honor `--format tree`

### Stage 3 — Progressive Disclosure Defaults (BUG-214) ✅

- `fv show` defaults to outline, add `--expand <name|*>`, `--imports`, `--docstrings`
- `fv parse --format detailed` hides imports/calls by default, add `--imports`/`--calls`
- `fv trace` depth default 3→1
- `fv entrypoints` default to code-only, add `--all` for docs/config

### Stage 4 — Language-Aware Veil Annotations (BUG-210) ✅

- Python: `def foo(...):\n    ...  # N lines hidden`
- Rust/Go/TS: keep `{ ... N lines ... }`

### Stage 5 — `fv disclose` Enhancements (BUG-212) ✅

- Add `--show` flag to emit actual code within budget
- Multi-focus: `--focus a.py --focus b.py`
- Budget truncation warning + `--strict` flag

### Stage 6 — Trace and Filter Enhancements (BUG-211) ✅

- `fv trace --focus <file>` — trace from all functions in a file
- `fv veil --reachable-from` — inverse of --unreachable-from
- `fv unveil --unreachable-from` / `--reachable-from`
- `--no-std` includes Python builtins

### Stage 7 — Status, Context, Usability (BUG-209, 213) ✅

- `fv status` prints veiled/unveiled file counts (already computed, never printed)
- `--verbose` alias for `--files`
- `fv context --help` usage examples

### Stage 8 — New Features: Profiles + Globs ✅

- `fv profile save/load/list/delete <name>` — named veil configurations
- Glob support: `fv veil '**/*_test.py'`

---

## Done: Static analysis sees through veils

All static analysis commands now read original content from CAS for veiled files and from disk for unveiled files, via the new `parse_all_sources()` function in `metadata.rs`.

### Changes made

1. **`parse_all_sources()`** (`metadata.rs`) — New function that collects `ParsedFile`s from both CAS (veiled/partially veiled) and disk (unveiled). Deduplicates by relative path.

2. **`rebuild_index()`** (`metadata.rs`) — Rewritten to use `parse_all_sources()`. Index now includes symbols from all files regardless of veil state.

3. **`build_call_graph_from_metadata()`** (`metadata.rs`) — Rewritten to use `parse_all_sources()`. Call graph now has complete coverage.

4. **`trace` command** (`commands.rs`) — Replaced disk-walking with `parse_all_sources()`. Trace now sees through veils.

5. **`entrypoints` command** (`commands.rs`) — Replaced disk-walking with `parse_all_sources()`. Entrypoints now detected in veiled files.

6. **`compute_disclosure_plan()`** (`budget.rs`) — Added `read_file_content()` helper that reads from CAS if veiled, disk if unveiled. All three content-reading blocks updated.

### Bugs fixed: BUG-186, BUG-187, BUG-188, BUG-189

## Done: `--include-tests` flag for `fv entrypoints`

Test entrypoints are now excluded by default. Added `--include-tests` flag to `Commands::Entrypoints` that opts them back in. Explicit `--type test` always shows tests regardless of the flag.
