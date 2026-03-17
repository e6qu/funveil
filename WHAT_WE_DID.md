# What We Did

## 2026-03-17: Implemented all 8 stages of ISSUE.md fixes

### Stage 1 — Veil/Unveil Correctness (BUG-202–206)

- **BUG-202:** Added directory recursion to `fv veil <dir> --mode headers`
- **BUG-203:** `fv show` now displays header-veiled files as `[HEADERS VEILED]` with actual on-disk content
- **BUG-204:** All 7 unveil paths now call `config.remove_from_blacklist()` alongside `add_to_whitelist()`
- **BUG-205:** Added HistoryTracker to `--unreachable-from` veil handler (enables undo)
- **BUG-206:** Removed unconditional `add_to_blacklist` from `--symbol` veil (partial veil entries suffice)

### Stage 2 — Trace Argument Cleanup (BUG-207, 208)

- **BUG-207:** Removed redundant positional `[FUNCTION]` arg from `fv trace`; `--from` is the only way
- **BUG-208:** `--from-entrypoint` now outputs per-entrypoint grouped results, honors `--format tree`

### Stage 3 — Progressive Disclosure Defaults (BUG-214)

- `fv show` defaults to structured outline (signatures + line numbers); `--expand <name|*>`, `--imports`, `--docstrings` flags added
- `fv parse --format detailed` hides imports/calls by default; `--imports`/`--calls` flags added
- `fv trace` depth default changed from 3 to 1
- `fv entrypoints` defaults to code-only languages; `--all` flag includes docs/config/shell
- Added `Language::is_code()` method to parser

### Stage 4 — Language-Aware Veil Annotations (BUG-210)

- Python files now use `...  # N lines hidden` syntax instead of `{ ... N lines ... }`
- Python classes use `:` instead of `{}`
- Rust/Go/TS/other languages keep C-style `{ ... N lines ... }`

### Stage 5 — `fv disclose` Enhancements (BUG-212)

- Added `--show` flag to emit actual code within budget
- Multi-focus: `--focus a.py --focus b.py` (Vec<String>)
- Budget truncation warning: prints dropped token count
- Added `--strict` flag that errors if budget is exceeded

### Stage 6 — Trace and Filter Enhancements (BUG-211)

- `fv trace --focus <file>` traces from all functions in file
- `fv veil --reachable-from <fn>` veils reachable files (inverse of --unreachable-from)
- `fv unveil --unreachable-from` / `--reachable-from` added
- Python builtins (str, int, bool, list, dict, isinstance, etc.) added to `STD_FUNCTIONS`

### Stage 7 — Status, Context, Usability (BUG-209, 213)

- `fv status` now prints veiled/unveiled file counts
- `--verbose` alias for `--files` on status command
- `fv context --help` now shows usage examples

### Stage 8 — New Features: Profiles + Globs

- `fv profile save/load/list/delete <name>` — named config snapshots in `.funveil/profiles/`
- Glob support: patterns with `*`, `?`, `[` are matched via `glob` crate in `collect_affected_files_for_pattern`

### Test results

- All 2143 tests pass (1255 unit + 392 command + 158 CLI + 113 e2e + 203 integration + 22 stress)
- Clippy clean (zero warnings)

## Previous sessions

- PR #88: Fixed BUG-186–201, added AST metadata caching
- PR #87: Removed dead code, fixed BUG-177, closed coverage gaps
- PR #86: Fixed 9 parser bugs and 3 structural correctness issues
- PR #85: Progressive disclosure, query unveiling, budget mode, 10-bug fix pass
- PR #84: Undo/redo system, action history, LLM-agent CLI ergonomics
