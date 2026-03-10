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

### BUG-024: `ContentHash::from_string` accepts arbitrary strings; `path_components` panics
**File:** `src/types.rs:105-107, 120-123`
**Description:** `ContentHash::from_string(hash)` stores any string with zero validation. Downstream, `path_components()` calls `assert!(self.0.len() >= 6)` and slices at fixed indices — panics if the hash is short or empty. This can happen when loading corrupted config files or manually edited metadata (hashes come from `serde_yaml` deserialization of user-editable YAML).

### BUG-025: `Pattern::parse` panics on minimal regex entries
**File:** `src/types.rs:244-259`
**Description:** When `entry = "/"` (starts and ends with `/`), `pattern_str` is `"/"` and the slice `&pattern_str[1..pattern_str.len() - 1]` is `&s[1..0]` — panics (start > end). Similarly, `entry = "/#1-5"` gives `pattern_str = ""` via `split_at(0)`, and `&""[1..]` panics. No minimum-length check guards the regex extraction, unlike `main.rs` which checks `pattern.len() > 2`.

### BUG-026: `veil_directory` silently discards per-file errors
**File:** `src/veil.rs:247`
**Description:** `let _ = veil_file(root, config, &path_str, ranges);` discards errors for individual files during directory veiling. If a file fails (e.g., permission denied, binary file with ranges), the directory veil reports success while that file is left un-veiled. No warning is emitted to the user.

### BUG-027: `unveil_directory` silently discards per-file errors
**File:** `src/veil.rs:632`
**Description:** Same pattern as BUG-026 but for unveiling: `let _ = unveil_file(root, config, &path_str, ranges);`. Failed file unveils within a directory are silently swallowed, potentially leaving files in a veiled state without user awareness.

### BUG-031: Zig parser hardcodes visibility to `Public` for all functions
**File:** `src/parser/languages/zig.rs:141`
**Description:** All Zig functions are marked `Visibility::Public` regardless of whether they use the `pub` keyword. The comment on line 141 says "Zig uses pub keyword" but the code never checks for it. Same class of bug as BUG-011 (Go visibility), but for Zig.

### BUG-032: `Apply` command stores new content in CAS but never updates config
**File:** `src/main.rs:817-875`
**Description:** The `Apply` command detects files whose content hash has changed and stores the new content in CAS (line 857), but never updates `config.objects` with the new hash and never calls `config.save()`. Config is loaded as immutable (`let config = Config::load(&root)?;` on line 818). After Apply, the config still references old hashes while CAS has new ones — an inconsistent state that breaks subsequent `unveil` operations.

### BUG-033: `is_react_component` rejects single uppercase letters
**File:** `src/parser/languages/typescript.rs:416-422`
**Description:** Same pattern as BUG-028 (`is_pascal_case`) but in a separate function. Requires both an uppercase first char AND at least one lowercase char. Single-letter component names like `"A"` or `"X"` (valid React/TSX components) return `false`, so PascalCase-filtered extraction in `extract_ts_functions` (line 141) doesn't skip them, and `extract_react_components` doesn't pick them up either, leading to misclassification.

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

### BUG-028: `is_pascal_case` rejects single uppercase letters
**File:** `src/analysis/entrypoints.rs:325-328`
**Description:** `is_pascal_case` requires both an uppercase first char AND at least one lowercase char (`s.chars().any(|c| c.is_lowercase())`). Single-letter component names like `"A"` or `"X"` (valid React/TSX components) return `false`, causing the entrypoint detector to miss them.

### BUG-029: Python entrypoint detection matches substrings too broadly
**File:** `src/analysis/entrypoints.rs:369, 380`
**Description:** CLI handler detection uses `name.contains("command")` / `name.contains("cmd")`, and web handler detection uses `name.contains("route")` / `name.contains("endpoint")`. This matches user functions like `get_command_line()`, `recommend()` (contains "cmd"), `enroute()` (contains "route"), or `endpoint_config()`. Should use word-boundary matching or require the keyword as a prefix/suffix.

### BUG-030: CSS/Markdown truncation produces meaningless `"..."` for whitespace-only names
**File:** `src/parser/languages/css.rs:89-92`, `src/parser/languages/markdown.rs:67-70`
**Description:** When a CSS selector or markdown heading is longer than 50 chars but consists entirely of whitespace, `chars().take(47)` collects only whitespace and the result is `"   ..."`. The symbol name carries no useful identifying information. Should skip or use a placeholder like `"<empty>"`.

### BUG-034: CAS `store` has TOCTOU race in deduplication check
**File:** `src/cas.rs:29-30`
**Description:** `store()` checks `if !path.exists()` then calls `fs::write(&path, content)`. Between the check and the write, another process could create the file. In practice this is harmless for CAS (same hash = same bytes), but concurrent writes could interfere. Should use atomic creation (`OpenOptions::new().create_new(true)`) or write-to-temp-then-rename.

### BUG-035: Checkpoint restore silently discards permission restoration errors
**File:** `src/checkpoint.rs:247-248`
**Description:** `let _ = fs::set_permissions(&file_path, ...)` discards errors. If permission restoration fails (e.g., insufficient privileges), the file is left with default permissions without user awareness. The file counts as "restored" in the summary even though permissions are wrong.

### BUG-036: `garbage_collect` undercounts `freed_bytes` when metadata fails
**File:** `src/cas.rs:159-162`
**Description:** `freed_bytes` is accumulated from `fs::metadata(&path)` BEFORE deletion. If metadata fails (returns `Err`), the `if let Ok` silently skips it, but `store.delete()` on line 162 still proceeds and succeeds. The deleted file's size is not counted in `freed_bytes`, making the reported statistic inaccurate.

### BUG-037: `parse_range` doesn't validate that start > 0
**File:** `src/patch/parser.rs:350-372`
**Description:** The unified diff format uses 1-indexed line numbers, but `parse_range` accepts `start = 0` without error. A hunk header like `@@ -0,5 +1,5 @@` would be parsed successfully despite being invalid, potentially causing off-by-one errors or panics downstream when used as a line index.
