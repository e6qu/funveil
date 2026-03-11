# Known Bugs

## Critical

### Open

### Fixed

- ~~**BUG-049:** Apply command has inverted hash comparison — `current_hash.full() == meta.hash` treats matching-original as "already veiled" and skips, but matching means the file is unveiled and needs re-veiling (`main.rs:875`)~~

- ~~**BUG-038:** Path traversal vulnerability in patch application — user-controlled path joined without `..` validation (`patch/manager.rs:241`)~~
- ~~**BUG-039:** Header signature truncation panics on non-ASCII content — byte-slices mid-UTF-8 character (`strategies/header.rs:78`)~~

- ~~**BUG-001:** Unicode panic in CSS/Markdown selector truncation (`css.rs:90`, `markdown.rs:68`)~~
- ~~**BUG-002:** Patch `apply_hunk` silently skips delete lines on mismatch (`patch/manager.rs:288-292`)~~
- ~~**BUG-003:** Patch `apply_hunk` destroys trailing newlines (`patch/manager.rs:310`)~~
- ~~**BUG-004:** Multiple hunks applied sequentially produce wrong offsets (`patch/manager.rs:250-253`)~~
- ~~**BUG-005:** `Veil` in `Headers` mode destroys original content with no restore path (`main.rs:338-361`)~~

## High

### Open

- ~~**BUG-065:** Doctor command aborts on first invalid hash — `ContentHash::from_string(meta.hash.clone())?` inside a `for` loop over `config.objects` aborts the entire integrity check on the first corrupted hash. Same pattern as BUG-057 (Apply) and BUG-058 (Checkpoint restore). A diagnostic command should report the bad entry as an issue and continue checking remaining objects. (`main.rs:1029`)~~

### Fixed

- ~~**BUG-057:** Apply command aborts on first invalid config hash — `ContentHash::from_string(meta.hash.clone())?` inside a `for` loop returns from the entire function on the first invalid hash, killing the apply operation for all remaining files. Should skip the bad entry with a warning and continue. (`main.rs:889`)~~

- ~~**BUG-058:** Checkpoint restore aborts on first invalid hash — `ContentHash::from_string(file_info.hash.clone())?` inside a `for` loop over manifest files aborts the entire restore on one corrupted hash. Should skip with a warning, increment `failed`, and continue. (`checkpoint.rs:220`)~~

- ~~**BUG-059:** Call graph falsely reports cycles when depth limit reached — when BFS traversal hits `max_depth` with unprocessed nodes remaining, sets `cycle_detected = true`, but remaining nodes just mean the call chain exceeds the limit, not that a cycle exists. Actual cycles are already detected at line 442. The false positive at lines 454-457 should be removed. (`call_graph.rs:454-457`)~~

- ~~**BUG-060:** Language-specific parsers panic on unexpected capture index — `capture_names[capture.index as usize]` uses direct array indexing without bounds checking. If tree-sitter returns a capture with index >= `capture_names.len()`, the code panics. The generic `tree_sitter_parser.rs` already uses safe `.get()` (line 793), but the language-specific parsers don't. Should use `.get()` with `continue` on `None`. (`go.rs:178`, `zig.rs:110`, `typescript.rs:121`, `html.rs:114`)~~

- ~~**BUG-050:** Veil regex adds to blacklist before verifying veil succeeds — if `veil_file` fails, file is on blacklist but not veiled (`main.rs:317`)~~
- ~~**BUG-051:** Apply removes config entry without rollback on failure — `config.objects.remove(key)` before `veil_file`; if re-veil fails, CAS reference is permanently lost (`main.rs:885`)~~
- ~~**BUG-052:** Unveil non-regex adds to whitelist before verifying — `config.add_to_whitelist` runs before `has_veils` check and `unveil_file`, inconsistent with regex path fixed in BUG-046 (`main.rs:821`)~~

- ~~**BUG-040:** Regex veil silently discards per-file errors (`main.rs:317`)~~
- ~~**BUG-041:** Regex unveil silently discards per-file errors (`main.rs:789`)~~
- ~~**BUG-042:** Apply command overwrites original CAS content with veiled placeholder when hash mismatches (`main.rs:862-874`)~~

- ~~**BUG-006:** `parse_*_file` functions panic on unparseable input (all language parsers)~~
- ~~**BUG-007:** `clean_path` strips repeated `a/` or `b/` prefixes (`patch/parser.rs:388-390`)~~
- ~~**BUG-008:** `is_binary_file` reads entire file into memory (`types.rs:378`)~~
- ~~**BUG-009:** Unveil hardcodes permissions to 0o644 before restoring content (`veil.rs:278`)~~
- ~~**BUG-010:** `show_checkpoint` panics on short hashes (`checkpoint.rs:179,183`)~~
- ~~**BUG-011:** Go visibility is always `Public`, ignoring capitalization convention (`go.rs:229,365`)~~

## Medium

### Open

- ~~**BUG-066:** Show command ignores quiet flag — all `println!` calls in `Commands::Show` are unconditional. Other display commands like `Status` (line 262) properly gate output on `!quiet`. Should wrap all output in `if !quiet { ... }`. (`main.rs:945-987`)~~

- ~~**BUG-067:** Parse command ignores quiet flag — all `println!` calls in `Commands::Parse` (both Summary and Detailed formats) are unconditional. Should gate output on `!quiet`. (`main.rs:392-451`)~~

- ~~**BUG-068:** Entrypoints command ignores quiet flag for non-empty results — BUG-064 fixed the empty-results path (line 681), but when entrypoints ARE found, lines 716-735 print group headers, entrypoint details, and totals unconditionally. The fix was incomplete — the output path was not addressed. Should gate on `!quiet`. (`main.rs:716-735`)~~

- ~~**BUG-069:** Cache Status ignores quiet flag — `CacheCmd::Status` prints unconditionally at line 745, while `CacheCmd::Clear` (line 751) and `CacheCmd::Invalidate` (line 759) in the same command group correctly check `!quiet`. Inconsistent. Should gate on `!quiet`. (`main.rs:745`)~~

- ~~**BUG-070:** Doctor command ignores quiet flag for results — the initial "Running integrity checks..." message (line 1020) correctly checks `!quiet`, but the results output at lines 1035-1041 (both "All checks passed" and issue listing) prints unconditionally. Should gate on `!quiet`. (`main.rs:1035-1041`)~~

### Fixed

- ~~**BUG-061:** Checkpoint List ignores quiet flag and prints header for empty list — the `else` branch fires when `!empty || quiet`, so when quiet=true and list is empty it prints "Checkpoints:" with nothing under it, and when quiet=true and list is non-empty it prints everything ignoring the quiet flag. Should separate the conditions: check `is_empty()` first, then gate output on `!quiet`. (`main.rs:990-997`)~~

- ~~**BUG-062:** Checkpoint restore returns success when files fail — `Ok(())` is always returned regardless of how many files failed to restore. If `failed > 0`, the caller has no programmatic way to know restoration was incomplete (error is only printed to stderr). Should return `Err` when `failed > 0`. (`checkpoint.rs:257-259`)~~

- ~~**BUG-063:** `veil_file` registers config entry before file write succeeds — `config.register_object` runs at line 75 before `fs::write` at line 78. If the write fails, the in-memory config references a veiled file that was never actually written, leaving config in an inconsistent state. Should move `register_object` after the file write succeeds. (`veil.rs:75-78`)~~

- ~~**BUG-053:** TreeSitterParser hardcodes function visibility to `Public` — generic `convert_function_match` doesn't detect visibility modifiers for Rust, Python, Bash, etc. (`tree_sitter_parser.rs:791`)~~
- ~~**BUG-054:** TreeSitterParser hardcodes class visibility to `Public` — `convert_class_match` always sets `Visibility::Public` for all classes/structs/traits/enums (`tree_sitter_parser.rs:924`)~~
- ~~**BUG-055:** Apply stores potentially modified content as original when CAS entry missing — current content may be veiled placeholder or corrupted, but is recorded as canonical without verification (`main.rs:896-907`)~~
- ~~**BUG-056:** Veil regex reports success even when all file operations fail — `matched = true` set on regex match regardless of `veil_file` outcome, printing misleading success message (`main.rs:322`)~~

- ~~**BUG-012:** `filter_std_functions` invalidates petgraph node indices during removal (`call_graph.rs:546-582`)~~
- ~~**BUG-013:** `is_std_function` over-aggressively filters user functions (`call_graph.rs:153-168`)~~
- ~~**BUG-014:** TypeScript parser extracts nothing from `.ts` files (`typescript.rs:78-88`)~~
- ~~**BUG-015:** `save_checkpoint` silently drops files on WalkDir errors (`checkpoint.rs:61-63`)~~
- ~~**BUG-016:** `is_vcs_directory` has wrong entries (`types.rs:326-328`)~~
- ~~**BUG-017:** Veil regex mode prints contradictory messages on no match (`main.rs:322-336`)~~
- ~~**BUG-024:** `ContentHash::from_string` accepts arbitrary strings; `path_components` panics (`types.rs:105-107`)~~
- ~~**BUG-025:** `Pattern::parse` panics on minimal regex entries like `"/"` (`types.rs:244-259`)~~
- ~~**BUG-026:** `veil_directory` silently discards per-file errors (`veil.rs:247`)~~
- ~~**BUG-027:** `unveil_directory` silently discards per-file errors (`veil.rs:632`)~~
- ~~**BUG-031:** Zig parser hardcodes visibility to `Public` for all functions (`zig.rs:141`)~~
- ~~**BUG-032:** `Apply` command stores CAS content but never updates config (`main.rs:817-875`)~~
- ~~**BUG-033:** `is_react_component` rejects single uppercase letters (`typescript.rs:416-422`)~~

### Fixed (cont.)

- ~~**BUG-043:** Body range fallback silently drops single-line functions — creates invalid range when `start_line == end_line` (`tree_sitter_parser.rs:782`)~~
- ~~**BUG-044:** TypeScript test entrypoint detection matches non-test functions like `testify_input` (`entrypoints.rs:288`)~~
- ~~**BUG-045:** Header truncation violates `max_signature_length` for values < 3 — produces `"..."` exceeding limit (`strategies/header.rs:76-78`)~~
- ~~**BUG-046:** Unveil regex updates whitelist before confirming file operation succeeds (`main.rs:787-789`)~~

## Low

### Open

- ~~**BUG-071:** Trace from-entrypoint "no entrypoints" message ignores quiet flag — `eprintln!("No entrypoints detected in the codebase")` is unconditional, while the subsequent progress messages at lines 521-526 and 567-568 correctly check `!quiet`. Inconsistent within the same command. Should gate on `!quiet`. (`main.rs:517`)~~

### Fixed

- ~~**BUG-064:** Entrypoints command ignores quiet flag — `println!("No entrypoints detected")` prints without checking `quiet`, inconsistent with other commands (e.g., Checkpoint List checks `!quiet`, Restore was fixed in BUG-019). Should wrap in `if !quiet { ... }`. (`main.rs:681`)~~

- ~~**BUG-018:** `Unveil` with no pattern and `all=false` silently does nothing (`main.rs:736-797`)~~
- ~~**BUG-019:** `Restore` command ignores `quiet` flag (`main.rs:862`)~~
- ~~**BUG-020:** `parse_file_line` produces empty path for malformed quoted strings (`patch/parser.rs:405-407`)~~
- ~~**BUG-021:** Rust parser classifies all enums and traits as `Struct` (`tree_sitter_parser.rs:887-892`)~~
- ~~**BUG-022:** `veil_file` pos_in_range off-by-one for multi-line range marker (`veil.rs:188,196`)~~
- ~~**BUG-023:** Yank does not remove conflicting patches from the queue (`patch/manager.rs:160-180`)~~

### Fixed (cont.)

- ~~**BUG-047:** Zig test declarations hardcoded to `Public` visibility instead of `Private` (`zig.rs:198`)~~
- ~~**BUG-048:** Zig type declarations missing visibility detection — no `pub` prefix check (`zig.rs`)~~

### Fixed

- ~~**BUG-028:** `is_pascal_case` rejects single uppercase letters (`entrypoints.rs:325-328`)~~
- ~~**BUG-029:** Python entrypoint detection matches substrings too broadly (`entrypoints.rs:369,380`)~~
- ~~**BUG-030:** CSS/Markdown truncation produces meaningless `"..."` for whitespace-only names (`css.rs:89-92`, `markdown.rs:67-70`)~~
- ~~**BUG-034:** CAS `store` has TOCTOU race in deduplication check (`cas.rs:29-30`)~~
- ~~**BUG-035:** Checkpoint restore silently discards permission restoration errors (`checkpoint.rs:247-248`)~~
- ~~**BUG-036:** `garbage_collect` undercounts `freed_bytes` when metadata fails (`cas.rs:159-162`)~~
- ~~**BUG-037:** `parse_range` doesn't validate that start > 0 (`patch/parser.rs:350-372`)~~
