# Tasks 014-017: Remove Dead Code and Improve Architecture

**Priority**: P4 - Code Quality  
**Severity**: INFO  
**Type**: Technical Debt  
**Estimated Time**: 2-3 hours total  
**Dependencies**: None

## Summary

This task group addresses unused code and architectural improvements identified in the code review. These are **low priority** and can be deferred to post-release or during refactoring cycles.

## Individual Tasks

### Task 014: Remove Unused Language-Specific Parser Files

**File**: Multiple files in `src/parser/languages/`  
**Issue**: Functions like `parse_go_file`, `parse_typescript_file`, etc. are defined but never called  
**Time**: 30 min

#### Problem

Language-specific parsing functions exist but are not integrated:

```rust
// src/parser/languages/go.rs
pub fn parse_go_file(path: &std::path::Path, content: &str) -> Result<ParsedFile> {
    // 642 lines of implementation
}
```

But `TreeSitterParser::parse_file` never calls these functions:

```rust
// src/parser/tree_sitter_parser.rs
pub fn parse_file(&self, path: &Path, content: &str) -> Result<ParsedFile> {
    // Uses inline queries instead
    // Never calls parse_go_file, parse_typescript_file, etc.
}
```

#### Options

**Option A: Remove Dead Code**
```bash
rm src/parser/languages/*.rs
# Keep mod.rs with re-exports if any are actually used
```

**Option B: Integrate into TreeSitterParser**
```rust
impl TreeSitterParser {
    pub fn parse_file(&self, path: &Path, content: &str) -> Result<ParsedFile> {
        match detect_language(path) {
            Language::Go => parse_go_file(path, content),
            Language::TypeScript => parse_typescript_file(path, content),
            // ... etc
            _ => self.parse_generic(path, content),
        }
    }
}
```

**Option C: Document as Experimental**
```rust
/// Parse a Go source file.
///
/// **Note**: This is an experimental standalone parser.
/// For most uses, prefer `TreeSitterParser::parse_file` which
/// handles all languages uniformly.
pub fn parse_go_file(...) -> Result<ParsedFile>
```

#### Recommendation

**Option C** - Document as experimental. This preserves the code for future use while making it clear it's not currently integrated.

---

### Task 015: Remove Unused PEG Grammar File

**File**: `src/patch/grammar.pest`  
**Issue**: Complete PEG grammar defined but parser uses hand-written parser instead  
**Time**: 15 min

#### Problem

```pest
// src/patch/grammar.pest
// 108 lines of PEG grammar
patch = { (git_file | unified_file)+ }
// ... complete grammar
```

But the actual parser ignores it:

```rust
// src/patch/parser.rs:71-75
pub fn parse_patch(input: &str) -> Result<ParsedPatch> {
    let format = Self::detect_format(input);
    
    // For now, use a simple line-based parser instead of full PEG
    // This is more robust and easier to understand
    let files = Self::parse_simple(input)?;
    
    Ok(ParsedPatch { files, format })
}
```

#### Options

**Option A: Remove PEG Grammar**
```bash
rm src/patch/grammar.pest
# Remove pest_derive dependency from Cargo.toml
```

**Option B: Use PEG Parser**
```rust
pub fn parse_patch(input: &str) -> Result<ParsedPatch> {
    let pairs = PatchParser::parse(Rule::patch, input)
        .map_err(|e| FunveilError::ParseError {
            line: e.line(),
            column: e.column(),
            message: e.to_string(),
        })?;
    
    Self::build_ast(pairs)
}
```

**Option C: Keep as Future Enhancement**
```bash
# Move to docs/
mv src/patch/grammar.pest docs/patch-grammar.pest.example
```

#### Recommendation

**Option C** - Move to docs. The grammar is valuable for future development but shouldn't be in the codebase if unused.

---

### Task 016: Standardize Error Handling Patterns

**Files**: Multiple  
**Issue**: Inconsistent error handling (Result vs Option, error types)  
**Time**: 1 hour

#### Problem

Mixed error handling approaches:

```rust
// Some functions return Result
pub fn retrieve(&self, hash: &ContentHash) -> Result<Vec<u8>>

// Others return Option
pub fn get_object(&self, key: &str) -> Option<&ObjectMeta>

// Others return bool
pub fn has_veils(config: &Config, file: &str) -> bool
```

#### Guidelines

1. **Use `Result<T>` for operations that can fail**:
   - File I/O
   - Network operations
   - Parsing
   - Config loading

2. **Use `Option<T>` for lookups**:
   - Getting items from collections
   - Finding items

3. **Use `bool` for simple checks**:
   - Predicate functions (`is_*`, `has_*`, `can_*`)

4. **Never use `unwrap()` or `expect()` in library code**

#### Implementation

Document these guidelines in `CONTRIBUTING.md` and create a code review checklist.

---

### Task 017: Clarify Module Responsibilities

**Files**: `src/parser/languages/mod.rs`, `src/lib.rs`  
**Issue**: Modules mix parsing, analysis, and I/O concerns  
**Time**: 1 hour

#### Problem

`src/parser/languages/mod.rs` exports functions that aren't used:

```rust
// src/parser/languages/mod.rs
pub use css::{has_tailwind, is_scss, parse_css_file};
pub use go::parse_go_file;
pub use html::parse_html_file;
// ... etc
```

But these are re-exported at crate root without clear purpose:

```rust
// src/lib.rs
pub use parser::{Language, ParsedFile, Symbol, TreeSitterParser};
// Language-specific parsers NOT exported here
```

#### Solution

1. **Document module purposes**:
   ```markdown
   ## Module Organization
   
   - `parser/` - Parsing source code into structured representations
   - `analysis/` - Analysis of parsed code (call graphs, entrypoints)
   - `veil/` - Veiling/unveiling operations
   - `cas/` - Content-addressable storage
   - `patch/` - Patch parsing and management
   - `config/` - Configuration management
   ```

2. **Clarify exports**:
   ```rust
   // src/lib.rs
   //! Funveil - File visibility control for AI agent workspaces
   //!
   //! ## Organization
   //! - [`parser`] - Source code parsing
   //! - [`analysis`] - Code analysis (call graphs, entrypoints)
   //! - [`veil`] - Veiling operations
   //! - [`cas`] - Content-addressable storage
   //! - [`config`] - Configuration management
   //! - [`patch`] - Patch parsing and management
   
   pub mod analysis;
   pub mod cas;
   // ...
   
   // Public API - only export what's needed
   pub use analysis::{...};
   pub use cas::ContentStore;
   // ...
   
   // Experimental API - language-specific parsers
   #[doc(hidden)]
   pub mod language_parsers {
       pub use parser::languages::*;
   }
   ```

---

## Combined Implementation Plan

### Phase 1: Cleanup (30 min)

```bash
# Move unused grammar to docs
git mv src/patch/grammar.pest docs/patch-grammar.pest.example

# Remove pest_derive if unused
# Check Cargo.toml for other pest usage
```

### Phase 2: Documentation (30 min)

```markdown
# CONTRIBUTING.md additions

## Code Style

### Error Handling
- Use `Result<T>` for fallible operations
- Use `Option<T>` for lookups
- Use `bool` for predicates
- Never use `unwrap()` in library code

### Module Organization
- `parser/` - Parsing only
- `analysis/` - Analysis only
- `veil/` - Veil operations only
- `cas/` - Storage operations only

## Experimental Features

Functions marked as experimental should:
1. Be documented with `**Experimental**` note
2. Not be exported at crate root
3. Be in a `#[doc(hidden)]` module
```

### Phase 3: Language Parsers (30 min)

```rust
// src/parser/languages/mod.rs
//! Language-specific parsers (Experimental)
//!
//! These standalone parsers are experimental. For most uses,
//! prefer `TreeSitterParser::parse_file` which handles all
//! languages uniformly.

// Mark all exports as experimental
#[doc(hidden)]
pub mod css {
    //! CSS parser (Experimental)
    // ...
}
```

### Phase 4: lib.rs Reorganization (30 min)

```rust
// src/lib.rs
//! Funveil - File visibility control for AI agent workspaces
//!
//! ## Quick Start
//! ```rust
//! use funveil::{Config, ContentStore, veil_file, unveil_file};
//!
//! let mut config = Config::new(Mode::Blacklist);
//! let store = ContentStore::new(&root);
//! ```

// Modules
pub mod analysis;
// ... (keep existing)

// Public API
pub use analysis::{...};
// ... (keep existing)

// Experimental API - not part of stable API
#[doc(hidden)]
pub mod experimental {
    //! Experimental features - may change without notice
    
    pub use crate::parser::languages::*;
}
```

## Testing Requirements

- [ ] All existing tests still pass
- [ ] No new warnings from cargo clippy
- [ ] cargo doc builds without warnings
- [ ] CONTRIBUTING.md updated with guidelines
- [ ] README.md updated with module organization

## Acceptance Criteria

- [ ] Dead code removed or documented
- [ ] Module responsibilities documented
- [ ] Error handling patterns documented
- [ ] Experimental code marked as such
- [ ] All tests pass
- [ ] Documentation updated
- [ ] No clippy warnings

## Notes

- These are **low priority** tasks
- Can be done during refactoring cycles
- Focus on documentation first, code changes later
- Consider creating `ARCHITECTURE.md` for detailed module docs

## Related Issues

- Issue 14: Unused language-specific parsers
- Issue 15: Unused PEG grammar
- Issue 16: Inconsistent error handling
- Issue 17: Mixed module concerns
