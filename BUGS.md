# Known Bugs

## Critical (Fixed)

- ~~**BUG-001:** Unicode panic in CSS/Markdown selector truncation (`css.rs:90`, `markdown.rs:68`)~~
- ~~**BUG-002:** Patch `apply_hunk` silently skips delete lines on mismatch (`patch/manager.rs:288-292`)~~
- ~~**BUG-003:** Patch `apply_hunk` destroys trailing newlines (`patch/manager.rs:310`)~~
- ~~**BUG-004:** Multiple hunks applied sequentially produce wrong offsets (`patch/manager.rs:250-253`)~~
- ~~**BUG-005:** `Veil` in `Headers` mode destroys original content with no restore path (`main.rs:338-361`)~~

## High (Fixed)

- ~~**BUG-006:** `parse_*_file` functions panic on unparseable input (all language parsers)~~
- ~~**BUG-007:** `clean_path` strips repeated `a/` or `b/` prefixes (`patch/parser.rs:388-390`)~~
- ~~**BUG-008:** `is_binary_file` reads entire file into memory (`types.rs:378`)~~
- ~~**BUG-009:** Unveil hardcodes permissions to 0o644 before restoring content (`veil.rs:278`)~~
- ~~**BUG-010:** `show_checkpoint` panics on short hashes (`checkpoint.rs:179,183`)~~
- ~~**BUG-011:** Go visibility is always `Public`, ignoring capitalization convention (`go.rs:229,365`)~~

## Medium

### Fixed

- ~~**BUG-012:** `filter_std_functions` invalidates petgraph node indices during removal (`call_graph.rs:546-582`)~~
- ~~**BUG-013:** `is_std_function` over-aggressively filters user functions (`call_graph.rs:153-168`)~~
- ~~**BUG-014:** TypeScript parser extracts nothing from `.ts` files (`typescript.rs:78-88`)~~
- ~~**BUG-015:** `save_checkpoint` silently drops files on WalkDir errors (`checkpoint.rs:61-63`)~~
- ~~**BUG-016:** `is_vcs_directory` has wrong entries (`types.rs:326-328`)~~
- ~~**BUG-017:** Veil regex mode prints contradictory messages on no match (`main.rs:322-336`)~~

### Open

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

### Fixed

- ~~**BUG-018:** `Unveil` with no pattern and `all=false` silently does nothing (`main.rs:736-797`)~~
- ~~**BUG-019:** `Restore` command ignores `quiet` flag (`main.rs:862`)~~
- ~~**BUG-020:** `parse_file_line` produces empty path for malformed quoted strings (`patch/parser.rs:405-407`)~~
- ~~**BUG-021:** Rust parser classifies all enums and traits as `Struct` (`tree_sitter_parser.rs:887-892`)~~
- ~~**BUG-022:** `veil_file` pos_in_range off-by-one for multi-line range marker (`veil.rs:188,196`)~~
- ~~**BUG-023:** Yank does not remove conflicting patches from the queue (`patch/manager.rs:160-180`)~~

### Open

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
