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
- ~~**BUG-024:** `ContentHash::from_string` accepts arbitrary strings; `path_components` panics (`types.rs:105-107`)~~
- ~~**BUG-025:** `Pattern::parse` panics on minimal regex entries like `"/"` (`types.rs:244-259`)~~
- ~~**BUG-026:** `veil_directory` silently discards per-file errors (`veil.rs:247`)~~
- ~~**BUG-027:** `unveil_directory` silently discards per-file errors (`veil.rs:632`)~~
- ~~**BUG-031:** Zig parser hardcodes visibility to `Public` for all functions (`zig.rs:141`)~~
- ~~**BUG-032:** `Apply` command stores CAS content but never updates config (`main.rs:817-875`)~~
- ~~**BUG-033:** `is_react_component` rejects single uppercase letters (`typescript.rs:416-422`)~~

### Open

## Low

### Fixed

- ~~**BUG-018:** `Unveil` with no pattern and `all=false` silently does nothing (`main.rs:736-797`)~~
- ~~**BUG-019:** `Restore` command ignores `quiet` flag (`main.rs:862`)~~
- ~~**BUG-020:** `parse_file_line` produces empty path for malformed quoted strings (`patch/parser.rs:405-407`)~~
- ~~**BUG-021:** Rust parser classifies all enums and traits as `Struct` (`tree_sitter_parser.rs:887-892`)~~
- ~~**BUG-022:** `veil_file` pos_in_range off-by-one for multi-line range marker (`veil.rs:188,196`)~~
- ~~**BUG-023:** Yank does not remove conflicting patches from the queue (`patch/manager.rs:160-180`)~~

### Open

### Fixed

- ~~**BUG-028:** `is_pascal_case` rejects single uppercase letters (`entrypoints.rs:325-328`)~~
- ~~**BUG-029:** Python entrypoint detection matches substrings too broadly (`entrypoints.rs:369,380`)~~
- ~~**BUG-030:** CSS/Markdown truncation produces meaningless `"..."` for whitespace-only names (`css.rs:89-92`, `markdown.rs:67-70`)~~
- ~~**BUG-034:** CAS `store` has TOCTOU race in deduplication check (`cas.rs:29-30`)~~
- ~~**BUG-035:** Checkpoint restore silently discards permission restoration errors (`checkpoint.rs:247-248`)~~
- ~~**BUG-036:** `garbage_collect` undercounts `freed_bytes` when metadata fails (`cas.rs:159-162`)~~
- ~~**BUG-037:** `parse_range` doesn't validate that start > 0 (`patch/parser.rs:350-372`)~~
