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

### Fixed

- ~~**BUG-153:** Early returns in main.rs skip update check — Fixed by extracting `run_command()` so `main()` always reaches the update check after `run_command()` returns. (`main.rs`)~~

- ~~**BUG-154:** `force` parameter in update check is captured but never used — Fixed by tracking `was_cached` boolean; in non-force mode, notice is suppressed on fresh fetch (shown next run). (`update.rs`)~~

- ~~**BUG-144:** Init command saves config before ensuring data dir and gitignore — Fixed by reordering to: `ensure_data_dir` → `ensure_gitignore` → `config.save`. (`main.rs`)~~

- ~~**BUG-145:** Headers veil mode missing symlink/path validation — Fixed by adding `validate_path_within_root` call after the exists check. (`main.rs`)~~

- ~~**BUG-146:** V1 full unveil silently skips ranges with missing CAS entries — Fixed by replacing `if let Ok` with `map_err` + `?` to propagate CAS errors as `ObjectNotFound`. (`veil.rs`)~~

- ~~**BUG-137:** v1 fallback partial unveil drops non-veiled lines in specified range — Fixed by replacing `if let Some(meta)` with `ok_or_else` returning `CorruptedMarker` error. (`veil.rs`)~~

- ~~**BUG-138:** Patch hunk offset clamping silently misplaces hunks — Fixed by replacing `.max(1)` clamp with an error when `adjusted_start < 0`. (`patch/manager.rs`)~~

- ~~**BUG-128:** Binary file full veil fails with opaque UTF-8 error — line 140 only guards partial veils with `if ranges.is_some() && is_binary_file(...)`. Full veils pass through to `fs::read_to_string()` at line 144. Fixed by adding `BinaryFileVeil` error variant and checking `is_binary_file` before `read_to_string`. (`veil.rs:140-144`)~~

- ~~**BUG-129:** Checkpoint name allows path traversal — `root.join(CHECKPOINTS_DIR).join(name)` with no validation. Fixed by adding `validate_checkpoint_name()` rejecting `/`, `\`, `..`, empty, and control characters in all four checkpoint functions. (`checkpoint.rs:55`)~~

- ~~**BUG-125:** `unveil_file` missing `validate_path_within_root` symlink escape check — `veil_file` calls `validate_path_within_root` to prevent symlink escape attacks, but `unveil_file` did not. An attacker could place a symlink pointing outside the project root and use `fv unveil <symlink>` to overwrite arbitrary files. Fixed by adding the same `validate_path_within_root` call after the exists check. (`veil.rs:460`)~~

- ~~**BUG-122:** `extract_imports` unchecked array indexing — `queries.import_names[capture.index as usize]` panics on out-of-bounds. Same pattern as BUG-060 which was fixed in language-specific parsers but missed here. Fixed by replacing with `.get()` and `continue` on `None`. (`tree_sitter_parser.rs:999`)~~

- ~~**BUG-123:** `extract_calls` unchecked array indexing — `queries.call_names[capture.index as usize]` panics on out-of-bounds. Identical pattern to BUG-122. Fixed by replacing with `.get()` and `continue` on `None`. (`tree_sitter_parser.rs:1050`)~~

- ~~**BUG-110:** New veil ranges not checked for overlap with existing veils — `veil_file` checks for exact duplicate keys but not range overlap. Veiling `3-8` when `1-5` is already veiled succeeds, double-storing overlapping lines and producing incorrect markers. Fixed by checking new ranges against existing ranges (and each other) using `LineRange::overlaps()` before registering. (`veil.rs:143-161`)~~

- ~~**BUG-095:** Patch apply_file_patch allows absolute paths — path traversal check only looks for `ParentDir` components, but absolute paths like `/etc/passwd` bypass the check since `project_root.join(absolute_path)` returns the absolute path on Unix. Fixed by adding `if path.is_absolute()` check before the component traversal validation. (`patch/manager.rs:244`)~~

- ~~**BUG-089:** Patch apply_hunk panics when hunk old_start exceeds file length — `result.extend_from_slice(&lines[..start_idx])` panics with slice index out of bounds when a malformed patch specifies `old_start` greater than the file's line count. Fixed by clamping `start_idx` to `lines.len()`. (`patch/manager.rs:293`)~~

- ~~**BUG-079:** GC command aborts on first invalid hash — `.collect::<Result<_, _>>()?` aborts entire GC on one bad hash. Same pattern as BUG-057/058/065. Fixed by replacing with explicit loop that skips bad hashes with a warning. (`main.rs:1074-1078`)~~

- ~~**BUG-065:** Doctor command aborts on first invalid hash — `ContentHash::from_string(meta.hash.clone())?` inside a `for` loop over `config.objects` aborts the entire integrity check on the first corrupted hash. Same pattern as BUG-057 (Apply) and BUG-058 (Checkpoint restore). A diagnostic command should report the bad entry as an issue and continue checking remaining objects. (`main.rs:1029`)~~

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

### Fixed

- ~~**BUG-155:** Update check `is_newer` fails on pre-release versions — Fixed by stripping pre-release suffixes (everything after `-`) before parsing each version component. (`update.rs`)~~

- ~~**BUG-156:** Unveil command uses `process::exit(1)` instead of returning error — Fixed by replacing with `return Err(anyhow::anyhow!(...))`. (`main.rs`)~~

- ~~**BUG-147:** Veil regex missing feedback when files match but none are veiled — Fixed by adding `else if !veiled_any` branch printing "No files could be veiled for pattern". (`main.rs`)~~

- ~~**BUG-148:** Checkpoint restore missing path traversal validation — Fixed by checking for absolute paths and `..` components before `root.join(path)`, skipping unsafe paths with a warning. (`checkpoint.rs`)~~

- ~~**BUG-139:** Out-of-bounds partial veil range silently skipped — Fixed by replacing `continue` with `InvalidLineRange` error. (`veil.rs`)~~

- ~~**BUG-140:** Show command missing symlink/path validation — Fixed by adding `validate_path_within_root` call after the exists check. (`main.rs`)~~

- ~~**BUG-141:** CRLF line endings lost during partial veil roundtrip — Fixed by detecting CRLF at start of partial veil/unveil and using the detected line ending for all join/push operations. (`veil.rs`)~~

- ~~**BUG-142:** `unveil_all` fails entirely on first file error — Fixed by collecting errors per-file and returning `PartialRestore` when some files fail. (`veil.rs`)~~

- ~~**BUG-143:** Regex veil/unveil `max_depth(10)` silently misses deep files — Fixed by replacing `max_depth(Some(10))` with `max_depth(None)`. (`main.rs`)~~

- ~~**BUG-150:** CachedParser.get_or_parse panics on metadata retrieval failure — `insert()` silently drops the parsed file when `get_file_info()` returns `None` (file became inaccessible between parsing and caching). `get_or_parse()` then calls `.unwrap()` on the missing cache entry. Fixed by replacing `.unwrap()` with `.ok_or_else()` that returns a `CacheError`. (`analysis/cache.rs:277`)~~

- ~~**BUG-130:** Show command veil marker detection has false positives — `line.contains("...[") && line.contains("]")` matches normal code. Fixed by replacing with regex `r"^\.\.\.\[[a-f0-9]{7}\]"` that only matches actual veil markers. (`main.rs:1040`)~~

- ~~**BUG-131:** `ensure_gitignore` only checks start marker, not block integrity — `content.contains(GITIGNORE_MARKER)` returns early even if block is corrupted. Fixed by checking for end marker and both managed entries; strips corrupted block and re-appends a fresh one if incomplete. (`config.rs:260`)~~

- ~~**BUG-132:** `ensure_gitignore` produces mixed line endings on CRLF files — appended block always uses `\n`. Fixed by detecting CRLF in existing content and using `\r\n` consistently throughout the managed block. (`config.rs:263-271`)~~

- ~~**BUG-133:** `veil_directory`/`unveil_directory` ignore nested `.gitignore` files — `load_gitignore(root)` loads only root-level `.gitignore`. Fixed by replacing `fs::read_dir` recursion with `ignore::WalkBuilder` which handles nested `.gitignore` files automatically. (`veil.rs:136,478`)~~

- ~~**BUG-126:** `unveil_file` missing protected file/directory guards — `veil_file` checks `is_config_file`, `is_data_dir`, `is_funveil_protected`, and `is_vcs_directory` but `unveil_file` had none of these guards. A corrupted or hand-edited config could reference protected files and unveil would process them. Fixed by adding the same guard checks to `unveil_file` after `validate_filename`. (`veil.rs:442-452`)~~

- ~~**BUG-121:** Missing whitelist update for '#' pattern in Unveil command — when unveiling with a line-range pattern like `fv unveil "file.txt#1-5"`, `config.add_to_whitelist(file)` was never called. The literal path and regex path both correctly add to whitelist on success. Same class as BUG-112 (veil blacklist). Fixed by adding `config.add_to_whitelist(file)` after successful `unveil_file` call. (`main.rs:799`)~~

- ~~**BUG-124:** `ConfigEntry::parse` literal path uses `rfind('#')` without suffix validation — if a filename with `#` (like `file#name.txt`) is in the whitelist/blacklist, `rfind('#')` splits at it and tries to parse `name.txt` as a range spec, failing. Same class as BUG-107 and BUG-100. Fixed by attempting `parse_ranges` on the suffix and falling through to treat entire string as literal filename if parsing fails. (`types.rs:272`)~~

- ~~**BUG-112:** Missing blacklist update for '#' pattern in Veil command — when veiling with a line-range pattern like `fv veil "file.txt#1-5"`, `config.add_to_blacklist(file)` was never called. The literal path and regex path both correctly add to blacklist on success. Fixed by adding `config.add_to_blacklist(file)` after successful `veil_file` call. (`main.rs:293`)~~

- ~~**BUG-113:** Unveil regex `matched` flag produces misleading success message — `matched = true` was set whenever a file matches the regex, regardless of whether `unveil_file` succeeds. This caused "Unveiled: {pattern}" to print even when ALL operations failed. Fixed by adding `unveiled_any` variable, set only in `Ok()` arm, and gating "Unveiled:" message on `unveiled_any`. (`main.rs:836,844`)~~

- ~~**BUG-114:** Veiled file partial output always adds trailing newline — the partial veil output loop appended `\n` after every line unconditionally. Files without a trailing newline gained one. Fixed by stripping final `\n` if `!had_trailing_newline`. (`veil.rs:311-345`)~~

- ~~**BUG-115:** v1 legacy unveil reconstruction adds trailing newline — in the v1 legacy reconstruction path (no `_original` key), `output.push('\n')` was appended unconditionally. Fixed by checking `veiled_content.ends_with('\n')` and stripping final `\n` if original didn't have one. (`veil.rs:529-552`)~~

- ~~**BUG-116:** v2 partial unveil fallback adds trailing newline — same pattern as BUG-115 in the v2 partial unveil fallback path (when `_original` key is missing). Fixed by checking trailing newline on veiled file content and conditionally stripping final `\n`. (`veil.rs:680-704`)~~

- ~~**BUG-099:** Apply command splits config key on first '#' — `key.find('#')` extracts file path from config key. For `"dir/file#name.txt#1-5"`, produces `"dir/file"` instead of `"dir/file#name.txt"`. Fixed by using `rfind('#')` with suffix validation. (`main.rs:884`)~~

- ~~**BUG-100:** `veiled_ranges()` splits config key on first '#' — `key.find('#')` in `veiled_ranges()` truncates `obj_file` for filenames with `#`, causing ranges to be silently missed. Fixed by using `rfind('#')` with suffix validation. (`config.rs:231`)~~

- ~~**BUG-101:** v1 partial key parsing uses first '#' — In `unveil_file`'s v1 reconstruction path, `key.find('#')` splits partial keys at wrong position for `#`-containing filenames. Fixed by using prefix length approach instead. (`veil.rs:376`)~~

- ~~**BUG-102:** Partial veil range checking uses first '#' — In the partial unveil path, `key.find('#')` splits at wrong `#`. Fixed by using prefix length approach (same as `find_veiled_range_for_line`). (`veil.rs:450`)~~

- ~~**BUG-103:** `garbage_collect` warning not gated on quiet — `eprintln!("Warning: failed to delete unreferenced object...")` prints unconditionally. Fixed by adding `quiet: bool` parameter and gating on `!quiet`. Updated caller in main.rs. (`cas.rs:174-179`)~~

- ~~**BUG-104:** `restore_checkpoint` per-file errors not gated on quiet — Five `eprintln!` calls inside the restore loop print unconditionally despite function having a `quiet` parameter. Fixed by gating all five on `!quiet`. (`checkpoint.rs:228-261`)~~

- ~~**BUG-105:** No validation that file content doesn't contain veil markers — `veil_file` doesn't check whether file content contains text matching veil marker patterns. Fixed by adding `check_marker_collision` that returns `FunveilError::MarkerCollision` when content lines match `^\.\.\.\[[0-9a-f]+\]\.{0,3}$`. (`veil.rs`)~~

- ~~**BUG-106:** No validation for unsupported characters in filenames — `veil_file` and `unveil_file` accept any filename including those with null bytes, newlines, or control characters. Fixed by adding `validate_filename` check that rejects control characters (0x00-0x1F except tab). (`veil.rs`)~~

- ~~**BUG-108:** Headers mode registers config before file write — `config.register_object()` and `config.add_to_blacklist()` run before `fs::write()`. If write fails, config references a file never written. Fixed by moving config updates after successful write. (`main.rs:381-385`)~~

- ~~**BUG-111:** No on-disk marker integrity check when adding veils to veiled file — When `has_existing_veils` is true, the code doesn't verify that on-disk markers match config expectations. Fixed by adding `check_marker_integrity` that verifies marker hashes match before proceeding. Returns `FunveilError::MarkerIntegrityError` on mismatch. (`veil.rs:85-119`)~~

- ~~**BUG-090:** Trace DOT format output not gated on quiet — `println!("{}", graph.to_dot())` prints unconditionally in the `TraceFormat::Dot` arm. Fixed by wrapping in `if !quiet`. (`main.rs:594`)~~

- ~~**BUG-091:** Trace Tree/List format output not gated on quiet — `println!("{output}")` prints unconditionally in the `TraceFormat::Tree | TraceFormat::List` arm. Fixed by wrapping in `if !quiet`. (`main.rs:609`)~~

- ~~**BUG-092:** veil_directory per-file error warnings not gated on quiet — two `eprintln!` calls in `veil_directory` print unconditionally. Fixed by adding `quiet: bool` parameter and gating both `eprintln!` on `!quiet`. (`veil.rs:249, 256`)~~

- ~~**BUG-093:** unveil_directory per-file error warnings not gated on quiet — same pattern as BUG-092 in `unveil_directory`. Fixed by adding `quiet: bool` parameter and gating both `eprintln!` on `!quiet`. (`veil.rs:642, 649`)~~

- ~~**BUG-094:** unveil_file v1 reconstruction warning not gated on quiet — `eprintln!("Warning: Partial veil created before v2...")` prints unconditionally. Fixed by adding `quiet: bool` parameter to `unveil_file` and gating on `!quiet`. (`veil.rs:359-362`)~~

- ~~**BUG-096:** unveil_all splits filename on first '#' — `key.find('#')` takes the first `#` position, so filenames containing `#` (e.g., `"dir/file#name.txt#1-5"`) split at the wrong position. Fixed by using `key.rfind('#')` and validating the suffix looks like a valid range spec or `_original` before splitting. (`veil.rs:659`)~~

- ~~**BUG-081:** Trace command warning not gated on quiet — two `eprintln!` lines when target function isn't in call graph print unconditionally. Fixed by wrapping in `if !quiet`. (`main.rs:578-579`)~~

- ~~**BUG-082:** Trace cycle-detected note not gated on quiet — `eprintln!("\nNote: Cycle detected in call graph")` prints unconditionally. Fixed by adding `&& !quiet` to condition. (`main.rs:608`)~~

- ~~**BUG-083:** Trace "not found" message not gated on quiet — `eprintln!("Function '{target}' not found in the codebase")` prints unconditionally. Fixed by wrapping in `if !quiet`. (`main.rs:611`)~~

- ~~**BUG-084:** Veil regex per-file error warnings not gated on quiet — two `eprintln!` calls print warnings unconditionally when individual files fail to veil in regex mode. Fixed by wrapping in `if !quiet`. (`main.rs:324, 337`)~~

- ~~**BUG-085:** Unveil regex per-file error warnings not gated on quiet — same pattern as BUG-084 in the unveil regex path. Fixed by wrapping in `if !quiet`. (`main.rs:817, 836`)~~

- ~~**BUG-086:** Apply command error messages not gated on quiet — three `eprintln!` calls in the Apply command print failure diagnostics unconditionally. Fixed by wrapping each in `if !quiet`. (`main.rs:904, 914, 928`)~~

- ~~**BUG-087:** GC invalid-hash warning not gated on quiet — `eprintln!("Warning: skipping invalid hash...")` added in BUG-079 fix was not gated on quiet. Fixed by wrapping in `if !quiet`. (`main.rs:1078`)~~

- ~~**BUG-072:** Veil non-regex adds to blacklist before verifying veil succeeds — `config.add_to_blacklist(&pattern)` runs before `veil_file()`. The regex path correctly adds to blacklist only after successful veil. Fixed by swapping order: veil first, then add to blacklist. (`main.rs:340-341`)~~

- ~~**BUG-073:** GC command outputs in quiet mode — `else { println!("{deleted} {freed}"); }` prints even when quiet=true. Fixed by removing the else branch. (`main.rs:1085-1086`)~~

- ~~**BUG-074:** show_checkpoint prints unconditionally — all `println!` calls ignore quiet flag. Fixed by adding `quiet: bool` parameter and wrapping output in `if !quiet`. (`checkpoint.rs:181-197`)~~

- ~~**BUG-075:** save_checkpoint prints unconditionally — `println!` at end ignores quiet flag. Fixed by adding `quiet: bool` parameter and wrapping output in `if !quiet`. (`checkpoint.rs:111`)~~

- ~~**BUG-076:** delete_checkpoint prints unconditionally — `println!` ignores quiet flag. Fixed by adding `quiet: bool` parameter and wrapping output in `if !quiet`. (`checkpoint.rs:282`)~~

- ~~**BUG-077:** restore_checkpoint prints unconditionally — `println!` ignores quiet flag. Fixed by adding `quiet: bool` parameter and wrapping output in `if !quiet`. (`checkpoint.rs:264`)~~

- ~~**BUG-078:** parse_pattern accepts empty file path — pattern `"#1-5"` produces empty file path. Fixed by validating that file path is non-empty after splitting on `#`. (`main.rs:1143-1144`)~~

- ~~**BUG-066:** Show command ignores quiet flag — all `println!` calls in `Commands::Show` are unconditional. Other display commands like `Status` (line 262) properly gate output on `!quiet`. Should wrap all output in `if !quiet { ... }`. (`main.rs:945-987`)~~

- ~~**BUG-067:** Parse command ignores quiet flag — all `println!` calls in `Commands::Parse` (both Summary and Detailed formats) are unconditional. Should gate output on `!quiet`. (`main.rs:392-451`)~~

- ~~**BUG-068:** Entrypoints command ignores quiet flag for non-empty results — BUG-064 fixed the empty-results path (line 681), but when entrypoints ARE found, lines 716-735 print group headers, entrypoint details, and totals unconditionally. The fix was incomplete — the output path was not addressed. Should gate on `!quiet`. (`main.rs:716-735`)~~

- ~~**BUG-069:** Cache Status ignores quiet flag — `CacheCmd::Status` prints unconditionally at line 745, while `CacheCmd::Clear` (line 751) and `CacheCmd::Invalidate` (line 759) in the same command group correctly check `!quiet`. Inconsistent. Should gate on `!quiet`. (`main.rs:745`)~~

- ~~**BUG-070:** Doctor command ignores quiet flag for results — the initial "Running integrity checks..." message (line 1020) correctly checks `!quiet`, but the results output at lines 1035-1041 (both "All checks passed" and issue listing) prints unconditionally. Should gate on `!quiet`. (`main.rs:1035-1041`)~~

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

### Fixed

- ~~**BUG-157:** Update check test uses thread-unsafe `set_var`/`remove_var` — Fixed by refactoring `check_and_notify` to accept a `check_disabled: bool` parameter. Tests call it directly instead of using env vars. (`update.rs`)~~

- ~~**BUG-149:** Partial veil marker silently drops line when config lookup fails — Fixed by replacing `if let Some(meta)` with `ok_or_else` returning `CorruptedMarker` error. (`veil.rs`)~~

- ~~**BUG-152:** ContentHash::from_string accepts arbitrary-length hex strings — Only validated `len() >= 6` and hex characters, with no upper bound. Fixed by enforcing exact SHA-256 length (64 hex chars) with a minimum of 7 chars for short hashes. (`types.rs:106`)~~

- ~~**BUG-151:** Unveil command prints misleading message when no pattern or --all specified — When `fv unveil` is called with neither `--all` nor a pattern, the else branch prints "No veiled files matched the pattern." which is misleading. Fixed by replacing with a clear usage error message and exit code 1. (`main.rs:891-892`)~~

- ~~**BUG-134:** Unveil regex matches files but gives no feedback when none are veiled — when `matched && !unveiled_any`, user gets no output. Fixed by adding `else if matched && !unveiled_any && !quiet` branch printing "No veiled files matched pattern". (`main.rs:871-875`)~~

- ~~**BUG-135:** `max_signature_length=0` produces empty string with no truncation indicator — returns `""` instead of `"..."`. Fixed by clamping `max_signature_length` to minimum of 3 at point of use with `.max(3)`, and removing the now-unnecessary `max_len < 3` branch. (`strategies/header.rs:88-92`)~~

- ~~**BUG-136:** `parse_file_line` silently accepts unclosed quoted paths — `unwrap_or(inner.len())` takes entire remaining string on missing close quote. Fixed by replacing `unwrap_or` with `?` to return `None` when closing quote is not found. (`patch/parser.rs:414-421`)~~

- ~~**BUG-127:** `save_checkpoint` per-entry warning ignores quiet flag — `eprintln!("Warning: skipping directory entry: {e}")` printed unconditionally while the summary warning correctly gated on `!quiet`. Fixed by wrapping the per-entry warning in `if !quiet`. (`checkpoint.rs:66`)~~

- ~~**BUG-117:** Show command with --quiet skips all validation — when `quiet=true`, the entire Show block was skipped, including file existence checks. Fixed by moving file existence validation outside the quiet block. (`main.rs:988`)~~

- ~~**BUG-118:** `check_marker_collision` compiles regex on every call — `Regex::new().unwrap()` was called inside `check_marker_collision()` on every `veil_file` invocation. Fixed by using `std::sync::LazyLock` to compile the regex once as a static. (`veil.rs:33`)~~

- ~~**BUG-119:** `veil_file` accepts empty ranges slice — passing `Some(&[])` to `veil_file` skipped all range processing but still registered the `#_original` key. Fixed by adding early check: if `ranges.is_empty()`, return `InvalidLineRange` error. (`veil.rs:178`)~~

- ~~**BUG-120:** `veil_directory`/`unveil_directory` `strip_prefix` fallback passes absolute path — `path.strip_prefix(root).unwrap_or(&path)` fell back to the absolute path. Protection checks assume relative paths. Fixed by replacing `unwrap_or` with error handling that logs a warning and continues. (`veil.rs:371,773`)~~

- ~~**BUG-107:** `parse_pattern` splits on first '#' — `pattern.find('#')` splits user input `"dir/file#name.txt#1-5"` at the first `#`. Fixed by using `rfind('#')` and validating suffix is a parseable range spec; if not, treating entire pattern as filename. (`main.rs:1166`)~~

- ~~**BUG-109:** Redundant `.min(lines.len())` in veil partial — `lines[start..end.min(lines.len())]` but `end` was already clamped to `lines.len()`. Fixed by removing redundant `.min()`. (`veil.rs:151`)~~

- ~~**BUG-097:** veil_file and unveil_file lack quiet parameter for internal warnings — `veil_file` (public) didn't take a `quiet` parameter, so internal calls to `veil_directory` couldn't suppress warnings. Fixed by adding `quiet: bool` to `veil_file` and threading through all callers. (`veil.rs`)~~

- ~~**BUG-098:** unveil_all doesn't support quiet — `unveil_all` calls `unveil_file` in a loop but had no quiet parameter. Fixed by adding `quiet: bool` parameter and threading through from the sole caller in main.rs. (`veil.rs:655`)~~

- ~~**BUG-088:** parse_pattern allows empty range after '#' — pattern `"file.txt#"` (trailing `#`, no range spec) falls through to the range-parsing loop producing an unclear error. Fixed by adding early check for empty `ranges_str`. (`main.rs:1152`)~~

- ~~**BUG-080:** save_checkpoint walk_errors warning ignores quiet flag — `eprintln!("Warning: {walk_errors} entries...")` prints unconditionally. Fixed by gating on `!quiet` (covered by quiet parameter added in BUG-075). (`checkpoint.rs:101-104`)~~

- ~~**BUG-071:** Trace from-entrypoint "no entrypoints" message ignores quiet flag — `eprintln!("No entrypoints detected in the codebase")` is unconditional, while the subsequent progress messages at lines 521-526 and 567-568 correctly check `!quiet`. Inconsistent within the same command. Should gate on `!quiet`. (`main.rs:517`)~~

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
