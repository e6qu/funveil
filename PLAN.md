# Plan

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
