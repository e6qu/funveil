# Known Bugs

## Critical

### BUG-001: Unicode panic in CSS/Markdown selector truncation
**Files:** `src/parser/languages/css.rs:90`, `src/parser/languages/markdown.rs:68`
**Description:** `&selector_text[..47]` uses byte indexing. If the string contains multi-byte UTF-8 characters, slicing at byte 47 can land mid-character, causing a panic. Should use `chars().take()` or a char-boundary-aware truncation.

### BUG-002: Patch `apply_hunk` silently skips delete lines on mismatch
**File:** `src/patch/manager.rs:288-292`
**Description:** When a `Line::Delete` doesn't match the current file line, the code does nothing — `old_pos` is not advanced, so the line that should be deleted stays in the output, and all subsequent lines shift. This causes silent data corruption.

### BUG-003: Patch `apply_hunk` destroys trailing newlines
**File:** `src/patch/manager.rs:310`
**Description:** `content.lines()` strips trailing newlines, and `result.join("\n")` omits the final newline. Every patch application silently removes trailing newlines, causing diff noise and breaking POSIX text file expectations.

### BUG-004: Multiple hunks applied sequentially produce wrong offsets
**File:** `src/patch/manager.rs:250-253`
**Description:** When a file patch has multiple hunks, they are applied sequentially. After the first hunk shifts line numbers, subsequent hunks still reference original-file line numbers, producing incorrect results.

### BUG-005: `Veil` in `Headers` mode destroys original content with no restore path
**File:** `src/main.rs:338-361`
**Description:** Header-mode veil writes veiled content directly to disk without storing original content in CAS, registering in config, or updating blacklist. The original file content is lost and `fv unveil` cannot restore it.

## High

### BUG-006: `parse_*_file` functions panic on unparseable input
**Files:** `src/parser/languages/{go,typescript,zig,css,html,markdown,xml}.rs`
**Description:** All language-specific parsers use `.expect()` on `parser.parse()`, which can return `None` for malformed input or resource exhaustion. These are user-facing code paths that should return `Result::Err`, not panic. The main `tree_sitter_parser.rs` correctly uses `.ok_or_else()`.

### BUG-007: `clean_path` strips repeated `a/` or `b/` prefixes
**File:** `src/patch/parser.rs:388-390`
**Description:** `trim_start_matches("a/")` strips *all* leading occurrences. A path like `a/a/foo.txt` becomes `foo.txt` instead of `a/foo.txt`. Should use `strip_prefix("a/").unwrap_or(path)`.

### BUG-008: `is_binary_file` reads entire file into memory
**File:** `src/types.rs:378`
**Description:** `std::fs::read(path)` loads the whole file just to check the first 8KB for null bytes. For large binary files without recognized extensions, this is an unbounded allocation. Should use `File::open` + `Read::take(8192)`.

### BUG-009: Unveil hardcodes permissions to 0o644 before restoring content
**File:** `src/veil.rs:278`
**Description:** Permissions are set to 0o644 immediately, before content restoration. If CAS retrieval fails afterward, the file is left writable but still veiled — an inconsistent state. Permissions should be restored after successful content restoration.

### BUG-010: `show_checkpoint` panics on short hashes
**File:** `src/checkpoint.rs:179,183`
**Description:** `&file.hash[..7]` panics if the hash is shorter than 7 characters (e.g., corrupted or manually edited manifest).

## Medium

### BUG-011: Go visibility is always `Public`, ignoring capitalization convention
**File:** `src/parser/languages/go.rs:229,365`
**Description:** All Go functions and types are marked `Visibility::Public` regardless of whether their name starts with an uppercase letter. Go exports are determined by capitalization — lowercase names should be `Private`.

### BUG-012: `filter_std_functions` invalidates petgraph node indices during removal
**File:** `src/analysis/call_graph.rs:546-582`
**Description:** `remove_node()` in petgraph swaps the last node into the removed slot, invalidating previously-collected `NodeIndex` values. Subsequent removals in the loop may operate on wrong nodes.

### BUG-013: `is_std_function` over-aggressively filters user functions
**File:** `src/analysis/call_graph.rs:153-168`
**Description:** The heuristic filters any short lowercase function starting with `get_`, `set_`, `new_`, `with_`, `is_`, `has_`, `as_`, `to_`. This incorrectly filters common user functions like `get_users`, `set_config`, `new_connection`, `is_valid`, `has_permission`, etc.

### BUG-014: TypeScript parser extracts nothing from `.ts` files
**File:** `src/parser/languages/typescript.rs:78-88`
**Description:** Symbol extraction is gated by `is_tsx(path)`. Regular `.ts` files get an empty `ParsedFile` with no functions, imports, or calls. The language-specific parser is effectively a no-op for non-TSX TypeScript files.

### BUG-015: `save_checkpoint` silently drops files on WalkDir errors
**File:** `src/checkpoint.rs:61-63`
**Description:** `.filter_map(|e| e.ok())` silently discards directory traversal errors (permission denied, broken symlinks). Checkpoints may be incomplete without user awareness.

### BUG-016: `is_vcs_directory` has wrong entries
**File:** `src/types.rs:326-328`
**Description:** `"bzr/"` should be `".bzr/"` (missing dot). `"_FOSSIL_"` lacks trailing slash, so it matches any path containing that substring as a prefix (e.g., `_FOSSIL_data.txt`).

### BUG-017: Veil regex mode prints contradictory messages on no match
**File:** `src/main.rs:322-336`
**Description:** When a regex pattern matches no files, both "No files matched pattern" (line 323) and "Veiling: {pattern}" (line 335) are printed. The unconditional "Veiling" message should be gated on `matched`.

## Low

### BUG-018: `Unveil` with no pattern and `all=false` silently does nothing
**File:** `src/main.rs:736-797`
**Description:** Running `fv unveil` without arguments or `--all` silently succeeds. Should show a usage hint or error.

### BUG-019: `Restore` command ignores `quiet` flag
**File:** `src/main.rs:862`
**Description:** `println!("Restoring from latest checkpoint: {name}")` is not gated by `if !quiet`.

### BUG-020: `parse_file_line` produces empty path for malformed quoted strings
**File:** `src/patch/parser.rs:405-407`
**Description:** For `"a/file.txt` (opening quote, no closing), `rfind('"')` returns index 0, producing `&rest[1..0]` — an empty path, silently accepted.

### BUG-021: Rust parser classifies all enums and traits as `Struct`
**File:** `src/parser/tree_sitter_parser.rs:887-892`
**Description:** `convert_class_match` hard-codes `Rust => ClassKind::Struct`, but the Rust class query also matches `enum_item` and `trait_item`.

### BUG-022: `veil_file` pos_in_range off-by-one for multi-line range marker
**File:** `src/veil.rs:188,196`
**Description:** `pos_in_range = line_num - range.start()` is 0-based, but the hash marker is emitted when `pos_in_range == 1` (the second line). The first line outputs a blank line instead of the marker.

### BUG-023: Yank does not remove conflicting patches from the queue
**File:** `src/patch/manager.rs:160-180`
**Description:** When yanking a patch, if subsequent patches fail to re-apply, they remain in the queue despite not being applied to the working tree, leaving the queue in an inconsistent state.
