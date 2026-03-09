# Funveil Intelligent Veiling - Historical Development Plan

> **Status**: All 8 chunks implemented ✅  
> **Result**: 12 supported languages, entrypoint detection, call graph analysis

**Documentation**: [README.md](README.md) | [SPEC.md](SPEC.md) | [TUTORIAL.md](docs/TUTORIAL.md) | [LANGUAGE_SUPPORT_PLAN.md](LANGUAGE_SUPPORT_PLAN.md)

## Overview
This plan was used to extend funveil with automatic code-aware veiling modes that help LLMs understand codebases efficiently by showing/hiding code based on semantic analysis rather than just file paths.

**All planned features have been implemented.**

## Core Technologies
- **Tree-sitter**: Incremental parsing for multiple languages
- **LSP (Language Server Protocol)**: For deep semantic analysis
- **tower-lsp**: Rust LSP framework

---

## Chunk 1: Foundation - Tree-sitter Integration
**Goal**: Parse code and extract basic structure

### Tasks
1. Add tree-sitter dependencies:
   ```toml
   tree-sitter = "0.20"
   tree-sitter-javascript = "0.20"
   tree-sitter-typescript = "0.20"
   tree-sitter-python = "0.20"
   tree-sitter-rust = "0.20"
   tree-sitter-java = "0.20"
   tree-sitter-bash = "0.20"
   tree-sitter-zig = "0.20"  # or use C grammar as fallback
   ```

2. Create `src/parser/` module:
   - `mod.rs`: Language detection, parser registry
   - `tree_sitter_ext.rs`: Safe Rust wrappers around tree-sitter C API
   - `queries/`: Tree-sitter query files (.scm)

3. Implement language detection from file extension/shebang

4. Create basic query patterns for each language:
   - Function declarations
   - Class/struct declarations  
   - Import statements

### Output Example
```rust
pub struct ParsedFile {
    pub language: Language,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
}

pub enum Symbol {
    Function { name: String, params: Vec<String>, line_range: LineRange },
    Class { name: String, methods: Vec<String>, line_range: LineRange },
    // ...
}
```

### Success Criteria
- Can parse all 6 target languages
- Extract function/class names and line ranges
- Handle syntax errors gracefully (tree-sitter is error-resilient)

---

## Chunk 2: Header/Signature Mode
**Goal**: Show only function/class signatures, hide implementations

### Tasks
1. Create `src/veil_strategies/header.rs`
2. For each supported construct:
   - Function: Show `fn name(params) -> ReturnType;` (or `{ ... }`)
   - Class: Show class name, method names, property names (no bodies)
   - Interface/Trait: Show methods without implementations

3. Implement "folding" of code blocks:
   - Find function body block
   - Replace with placeholder showing line count: `{ ... 45 lines ... }`

4. Add CLI command: `fv veil --mode headers src/`

### Example Transformation
**Before:**
```python
def calculate_sum(numbers: List[int]) -> int:
    """Calculate sum of numbers."""
    total = 0
    for n in numbers:
        total += n
    return total
```

**After veiling:**
```python
def calculate_sum(numbers: List[int]) -> int:  # ... 6 lines ...
```

### Success Criteria
- Can veil entire directory showing only "public API"
- Preserves type signatures for type checking
- Configurable: show docstrings or not

---

## Chunk 3: Entrypoint Detection
**Goal**: Automatically identify and highlight entrypoints

### Tasks
1. Define entrypoint patterns per language:
   - **Rust**: `fn main()`, `#[test]` functions, `lib.rs` exports
   - **Python**: `if __name__ == "__main__"`, `__init__.py` exports
   - **JS/TS**: Package.json `main`/`module`, CLI bin entries
   - **Java**: `public static void main()`, `@SpringBootApplication`
   - **Bash**: Top-level commands, functions called at script end
   - **Zig**: `pub fn main()`

2. Create `src/analysis/entrypoints.rs`
3. Add query patterns for each entrypoint type
4. Implement "entrypoint veil mode":
   - Show entrypoints fully
   - Show direct callers of entrypoints (1 level up)
   - Hide everything else as headers only

### CLI
```bash
fv veil --mode entrypoints  # Auto-detect and show entrypoints
fv unveil --entrypoint calculate_sum  # Focus on specific function
```

### Success Criteria
- Correctly identifies entrypoints in all 6 languages
- Shows call graph 1 level above entrypoints
- Works with async/await and callback patterns

---

## Chunk 4: Call Graph Analysis (Trace Forward/Backward)
**Goal**: Trace function calls from a starting point

### Tasks
1. Create `src/analysis/call_graph.rs`
2. Parse call expressions for each language:
   - Direct calls: `foo()`
   - Method calls: `obj.method()`
   - Static calls: `Class::method()`
   - Dynamic calls: function pointers, callbacks (best effort)

3. Build in-memory call graph:
   ```rust
   pub struct CallGraph {
       nodes: HashMap<SymbolId, Symbol>,
       edges: HashMap<SymbolId, Vec<SymbolId>>, // caller -> callees
       reverse_edges: HashMap<SymbolId, Vec<SymbolId>>, // callee -> callers
   }
   ```

4. Implement traversal algorithms:
   - `trace_forward(symbol, depth)`: Show what this function calls
   - `trace_backward(symbol, depth)`: Show what calls this function

5. Add CLI commands:
   ```bash
   fv trace-forward calculate_sum --depth 2
   fv trace-backward calculate_sum --depth 3
   ```

### Output Format
```
Entry: calculate_sum
├─ calls: validate_input
│  └─ calls: is_numeric
├─ calls: sum_range
│  └─ calls: range_iter
└─ calls: format_result
```

### Success Criteria
- Build call graph for entire codebase
- Trace forward/backward N levels
- Handle circular references
- Visualize as tree or graph

---

## Chunk 5: Dataflow Analysis
**Goal**: Trace variable/data flow through the codebase

### Tasks
1. Create `src/analysis/dataflow.rs`
2. Implement basic dataflow tracking:
   - Variable definitions and usages
   - Parameter flow through function calls
   - Return value flow

3. For a given variable/function:
   - `trace-dataflow var_name`: Show all places var is read/written
   - `trace-dataflow --function process_data`: Show how return values are used

4. Simple taint analysis:
   - Mark external inputs as "tainted"
   - Track propagation through assignments and calls
   - Show potential sink points

### Example
```bash
$ fv trace-dataflow user_input
user_input (main:23) - read from request.body
  └─ passed to: validate() (auth:45)
      └─ passed to: sanitize() (utils:12)
          └─ used in: query() (db:88) ⚠️ POTENTIAL SQL INJECTION
```

### Success Criteria
- Track variable flow within functions
- Track parameter flow across function boundaries
- Identify when sensitive data reaches "sinks"

---

## Chunk 6: LSP Integration Foundation
**Goal**: Connect to language servers for deeper analysis

### Tasks
1. Add dependencies:
   ```toml
   tower-lsp = "0.20"
   tokio = { version = "1", features = ["full"] }
   lsp-types = "0.94"
   ```

2. Create `src/lsp/` module:
   - `client.rs`: LSP client wrapper
   - `manager.rs`: Manage multiple language server processes

3. Implement LSP client capabilities:
   - Initialize/Shutdown
   - textDocument/definition (Go to definition)
   - textDocument/references (Find all references)
   - textDocument/hover (Type info)
   - workspace/symbol (Global symbol search)

4. Launch and manage language servers:
   - rust-analyzer (Rust)
   - pylsp/pyright (Python)
   - typescript-language-server (JS/TS)
   - jdtls (Java)
   - bash-language-server (Bash)

### Architecture
```rust
pub struct LspManager {
    servers: HashMap<Language, LspClient>,
}

impl LspManager {
    pub async fn goto_definition(&self, file: &Path, pos: Position) -> Vec<Location>;
    pub async fn find_references(&self, file: &Path, pos: Position) -> Vec<Location>;
}
```

### Success Criteria
- Auto-detect and start appropriate LSP for file type
- Basic "go to definition" works across files
- "Find references" returns accurate results

---

## Chunk 7: LSP-Powered Smart Veiling
**Goal**: Use LSP for precise cross-file analysis

### Tasks
1. Implement "Smart Veil" mode:
   - User selects a symbol (function/class)
   - Use LSP to find all references across codebase
   - Veil everything except:
     - The selected symbol
     - Files that reference it
     - Call chain up to entrypoints

2. Implement "Impact Analysis" veil:
   - Given a function, show:
     - Its implementation (full)
     - All callers (signature only, with line numbers)
     - All functions it calls (signature only)

3. Task-aware veiling (experimental):
   - Parse natural language task: "Fix bug in user authentication"
   - Use LSP to find auth-related symbols
   - Heuristics: symbols with "auth", "login", "user" in name
   - Auto-veil to show relevant code

### CLI Examples
```bash
# Show only code related to calculate_sum
fv smart-veil --symbol calculate_sum

# Show auth-related code
fv task-veil "Fix authentication bug"

# Impact analysis
fv impact-analysis --function process_payment
```

### Success Criteria
- Cross-file reference tracking works
- Smart veil reduces codebase view by 80%+ for focused tasks
- Task-aware veiling finds relevant code

---

## Chunk 8: Interactive Mode (Future)
**Goal**: Real-time veiling as user navigates

### Tasks
1. Create funveil daemon (`fv daemon`)
2. Implement file watcher for changes
3. WebSocket/stdio interface for editors
4. Editor plugins (VS Code, Neovim):
   - Show veiled view by default
   - Click to unveil specific functions
   - "Peek" at veiled code without full unveil

### Success Criteria
- Daemon maintains parsed AST in memory
- Sub-second veiling/unveiling
- Editor integration feels native

---

## Technical Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        CLI Layer                            │
│  (veil, unveil, trace-forward, trace-backward, smart-veil)  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    Strategy Layer                           │
│  (HeaderStrategy, EntrypointStrategy, TraceStrategy, etc.)  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────┬───────────────────────────────┐
│      Analysis Layer         │        LSP Layer              │
│  (CallGraph, Dataflow,      │  (LspClient, LspManager)      │
│   EntrypointDetector)       │                               │
└─────────────────────────────┴───────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    Parser Layer                             │
│  (TreeSitterParser, LanguageDetector, QueryEngine)          │
└─────────────────────────────────────────────────────────────┘
```

---

## Questions for Discussion

1. **Language Priority**: Which 2-3 languages should we implement first? (I'd suggest Rust, Python, TypeScript)

2. **LSP Dependency**: Should smart veiling work standalone (tree-sitter only) with LSP as optional enhancement, or require LSP?

3. **Query Storage**: Store tree-sitter queries as embedded strings or external `.scm` files?

4. **Cross-file Analysis**: Cache parsed results in `.funveil/analysis/` or keep in memory only?

5. **Performance**: For large codebases (10k+ files), what's an acceptable analysis time? Should we support incremental updates?

6. **LLM Integration**: Should we provide an OpenAI/Anthropic API integration to help interpret "task descriptions" for task-aware veiling?

7. **Output Formats**: Besides veiling files, should we generate:
   - Graphviz DOT files for call graphs?
   - JSON for integration with other tools?
   - Markdown documentation from headers?

---

## Dependencies Summary

```toml
[dependencies]
# Core parsing
tree-sitter = "0.20"
tree-sitter-rust = "0.20"
tree-sitter-python = "0.20"
tree-sitter-javascript = "0.20"
tree-sitter-typescript = "0.20"
tree-sitter-java = "0.20"
tree-sitter-bash = "0.20"

# LSP (optional feature)
tower-lsp = { version = "0.20", optional = true }
tokio = { version = "1", features = ["full"], optional = true }
lsp-types = { version = "0.94", optional = true }

# Graph algorithms (for call graph)
petgraph = "0.6"

# Async runtime for LSP
async-trait = { version = "0.1", optional = true }

[features]
default = ["tree-sitter-parsers"]
tree-sitter-parsers = []
lsp = ["tower-lsp", "tokio", "lsp-types", "async-trait"]
full = ["tree-sitter-parsers", "lsp"]
```

---

## Next Steps

1. Review this plan and answer discussion questions
2. Decide on Chunk 1 scope (which languages first)
3. Create POC for tree-sitter integration
4. Iterate on header/signature mode
5. Add LSP integration
6. Build smart veiling on top

---

## Phase 9: Patch-Based Editing with PEG Parser and Rich Feedback

**Status**: Planned  
**Goal**: Enable safe editing of veiled files through patch-based workflows with excellent error messages

### Design Philosophy

**All edits to veiled files MUST go through funveil.**

Instead of instructing users/tools to unveil-edit-veil, we provide a first-class patch-based editing experience:

1. **LLM generates patch**: The AI assistant creates a patch file describing desired changes
2. **`fv apply-patch` validates and applies**: Funveil checks the patch against veiled regions
3. **Rich feedback**: Detailed error messages guide the LLM to create valid patches
4. **Multiple format support**: Unified diff, git diff, ed scripts via PEG parser
5. **Line-level permissions**: Clear reporting of which lines are editable

### Why This Approach?

- **Safer**: Cannot accidentally modify veiled lines (enforced at application time)
- **Transparent**: LLM sees exactly what it can and cannot edit
- **Reversible**: Patches can be reviewed before application
- **Audit trail**: All changes go through funveil's logging
- **Tool agnostic**: Works with any editor or AI system that can generate patches

### Use Cases

```bash
# LLM wants to edit a veiled file
# Instead of: "Please unveil the file first"
# We say: "Generate a patch and use fv apply-patch"

# LLM generates patch (via thought process or tool use)
cat > llm-changes.patch << 'EOF'
--- a/api.py
+++ b/api.py
@@ -45,7 +45,8 @@
 def public_api():
     """Public API endpoint"""
-    return {"status": "ok"}
+    result = process_data()
+    return {"status": "ok", "data": result}
 
 # Lines 50-100 are veiled (implementation details)
EOF

# Apply the patch through funveil
fv apply-patch llm-changes.patch

# Funveil responds with rich feedback:
✗ Patch cannot be applied
  
  File: api.py
  Issue: Hunk at lines 45-48 overlaps with veiled region
  
  Patch context:
    @@ -45,7 +45,8 @@
     def public_api():
         """Public API endpoint"""
    -    return {"status": "ok"}
    +    result = process_data()
    +    return {"status": "ok", "data": result}
  
  Problem:
    Line 45 is inside veiled range 50-100 (implementation details)
  
  Available options:
    1. Edit only visible lines (1-49 and 101+)
    2. Request unveiling: fv unveil api.py#50-100
    3. Generate new patch for visible regions only
    
  Visible lines in api.py you can edit:
    - Lines 1-20: Imports and module docstring
    - Lines 21-49: Public function signatures
    - Lines 101-150: Public exports

# LLM generates corrected patch for visible lines only
cat > llm-changes-v2.patch << 'EOF'
--- a/api.py
+++ b/api.py
@@ -25,7 +25,7 @@
-def helper():
+def helper() -> dict:
     pass
EOF

fv apply-patch llm-changes-v2.patch
✓ Patch applied successfully
  Modified: api.py (lines 25-25)
  Veiled regions preserved: lines 50-100

# Or apply from stdin
cat changes.patch | fv apply-patch -

# Preview mode (dry run)
fv apply-patch --dry-run changes.patch
# Shows what would change without modifying files

# Get edit capabilities report
fv edit-report api.py
# Shows which lines are editable and why others are veiled
```

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Patch Application Flow                   │
│                                                             │
│  1. Parse patch (PEG parser)                               │
│     ├── Support: unified diff, git diff, ed, wdiff         │
│     └── Rich error messages for syntax errors              │
│                                                             │
│  2. Validate against veil config                           │
│     ├── Check each hunk against veiled ranges              │
│     └── Generate detailed conflict report                  │
│                                                             │
│  3. Apply or reject                                        │
│     ├── Success: Apply patch, log action                   │
│     └── Failure: Rich error with remediation               │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    Feedback Generation                      │
│                                                             │
│  - Context-aware error messages                            │
│  - Suggest alternative edit locations                      │
│  - Show veil boundaries in patch context                   │
│  - Line-by-line editability report                         │
└─────────────────────────────────────────────────────────────┘
```

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Patch Application Flow                   │
│                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │ PEG Parser  │ -> │  Validator  │ -> │   Applier   │     │
│  │             │    │             │    │             │     │
│  │ • unified   │    │ Check vs    │    │ Apply to    │     │
│  │ • git diff  │    │ veiled      │    │ working     │     │
│  │ • ed script │    │ ranges      │    │ tree        │     │
│  │ • wdiff     │    │             │    │             │     │
│  └─────────────┘    └─────────────┘    └─────────────┘     │
│          │                 │                  │              │
│          v                 v                  v              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Rich Feedback Generator                │   │
│  │  • Syntax errors with line numbers                  │   │
│  │  • Veiled line conflicts with context               │   │
│  │  • Suggest alternative edit locations               │   │
│  │  • Show editable line ranges                        │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    File System Watcher                      │
│  - Detect direct file modifications (bypassing fv)          │
│  - Log integrity violations                                 │
│  - Suggest using fv apply-patch instead                   │
└─────────────────────────────────────────────────────────────┘
```

### Implementation Plan

#### Task 1: PEG Parser for Patch Formats

Implement a robust PEG (Parsing Expression Grammar) parser for multiple patch formats with excellent error messages.

```rust
// src/patch/grammar.pest
// PEG grammar for unified diff format

patch = { file_header+ }

file_header = {
    "--- " ~ filepath ~ timestamp? ~ "\n" ~
    "+++ " ~ filepath ~ timestamp? ~ "\n" ~
    hunk+
}

filepath = { ("a/" | "b/" | "") ~ (!whitespace ~ any)+ }
timestamp = { whitespace ~ "\t" ~ (!"\n" ~ any)* }

hunk = {
    "@@ " ~ range ~ " " ~ range ~ " @@" ~ section? ~ "\n" ~
    line*
}

range = { "-" ~ number ~ "," ~ number | "+" ~ number ~ "," ~ number }
section = { whitespace ~ (!"\n" ~ any)* }

line = {
    " " ~ (!"\n" ~ any)* ~ "\n" |    // Context line
    "-" ~ (!"\n" ~ any)* ~ "\n" |    // Deleted line
    "+" ~ (!"\n" ~ any)* ~ "\n" |    // Added line
    "\\" ~ (!"\n" ~ any)* ~ "\n"     // "No newline at end of file"
}

number = @{ ASCII_DIGIT+ }
whitespace = _{ " " | "\t" }
```

```rust
// src/patch/parser.rs
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "patch/grammar.pest"]
pub struct PatchParser;

pub struct ParsedPatch {
    pub files: Vec<FilePatch>,
}

pub struct FilePatch {
    pub old_path: String,
    pub new_path: String,
    pub hunks: Vec<Hunk>,
}

pub struct Hunk {
    pub old_range: Range,
    pub new_range: Range,
    pub lines: Vec<Line>,
}

pub enum Line {
    Context(String),
    Delete(String),
    Add(String),
    NoNewline,
}

impl PatchParser {
    /// Parse with rich error messages
    pub fn parse_with_errors(input: &str) -> Result<ParsedPatch, Vec<ParseError>> {
        // Returns detailed errors with line/column info
    }
    
    /// Detect patch format automatically
    pub fn detect_format(input: &str) -> PatchFormat {
        // Unified diff, Git diff, Ed script, etc.
    }
}

/// Rich parse error with suggestions
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub expected: Vec<String>,
    pub found: String,
    pub suggestion: Option<String>,
}
```

**Error message examples**:
```
Error: Invalid patch format at line 42, column 1

  @@ -50,5 +50,5 @@
  
  The hunk header ' @@ -50,5 +50,5 @@' has an extra leading space.
  
  Expected: '@@ -50,5 +50,5 @@'
  Found:    ' @@ -50,5 +50,5 @@'
            ^
  
  Fix: Remove the leading space:
    @@ -50,5 +50,5 @@

---

Error: Malformed range at line 10

  @@ -50,5 +50,6 @@
              ^^^^^
  
  The new file range '-50,6' is invalid. Expected format: '+start,count'
  
  Fix: Ensure the count matches the number of '+' lines in the hunk.
  This hunk has 5 '+' lines but claims to add 6.

---

Error: Missing newline at end of file marker

  The patch appears to be missing '\ No newline at end of file'
  which is required when the original file doesn't end with a newline.
```

**Files to create**:
- `src/patch/grammar.pest`: PEG grammar definitions
- `src/patch/parser.rs`: Pest-based parser
- `src/patch/mod.rs`: Module exports
- `src/patch/error.rs`: Rich error types

**Dependencies**:
```toml
[dependencies]
pest = "2.7"
pest_derive = "2.7"
pest_meta = "2.7"
```

#### Task 2: Patch Validator

Validate parsed patches against veiled regions with detailed conflict reporting.

```rust
// src/patch/validator.rs

pub struct PatchValidator {
    config: Config,
}

pub struct ValidationReport {
    pub is_valid: bool,
    pub can_apply_safely: bool,
    pub files: Vec<FileValidation>,
    pub summary: ValidationSummary,
}

pub struct FileValidation {
    pub path: PathBuf,
    pub status: FileStatus,
    pub hunks: Vec<HunkValidation>,
    pub veiled_ranges: Vec<LineRange>,
    pub editable_ranges: Vec<LineRange>,
}

pub struct HunkValidation {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub status: HunkStatus,
    pub conflict_details: Option<ConflictDetails>,
}

pub enum HunkStatus {
    Safe,           // No overlap with veiled lines
    WouldModifyVeiled,  // Tries to modify veiled lines
    ContextInVeiled,    // Context lines are in veiled region (ok)
    OutOfBounds,    // Line numbers don't match file
}

pub struct ConflictDetails {
    pub veiled_range: LineRange,
    pub patch_range: LineRange,
    pub overlapping_lines: Vec<usize>,
    pub suggestion: String,
}

impl PatchValidator {
    /// Validate a patch and generate rich report
    pub fn validate(&self, patch: &ParsedPatch) -> ValidationReport {
        // For each file in patch:
        //   1. Check file exists (unless created by patch)
        //   2. For each hunk, check against veiled ranges
        //   3. Generate detailed conflict info
    }
    
    /// Generate editability report for a file
    pub fn edit_report(&self, file: &Path) -> EditReport {
        // Show which lines are editable and why
    }
}

/// Human-readable report generation
impl ValidationReport {
    pub fn to_human_readable(&self) -> String {
        // Generate beautiful, actionable output
    }
    
    pub fn to_json(&self) -> serde_json::Value {
        // Machine-readable format
    }
}
```

**Validation output example**:
```
$ fv apply-patch changes.patch --dry-run

═══════════════════════════════════════════════════════════════
                    PATCH VALIDATION REPORT
═══════════════════════════════════════════════════════════════

File: src/api.py
───────────────────────────────────────────────────────────────
Status: ⚠️  CONFLICTS DETECTED

Hunk 1 (lines 45-48 → 45-49):
  @@ -45,7 +45,8 @@
   def public_api():
       """Public API endpoint"""
  -    return {"status": "ok"}
  +    result = process_data()
  +    return {"status": "ok", "data": result}
  
  ❌ CONFLICT: Patch modifies veiled lines
     
     Patch context extends to line 45
     Veiled range: lines 50-100 (implementation details)
     
     The hunk header context (7 lines) extends into the veiled region.
     
     Options:
       1. Reduce context lines in the patch header
       2. Edit only lines 1-49 (before veiled region)
       3. Request access: fv unveil api.py#50-100

Editable regions in src/api.py:
  ✅ Lines 1-20:   Imports and module docstring
  ✅ Lines 21-49:  Public API definitions
  ⛔ Lines 50-100: [VEILED] Implementation details
  ✅ Lines 101-150: Public exports

═══════════════════════════════════════════════════════════════

To apply this patch to visible regions only, regenerate with:
  
  # Edit lines 25-30 (all visible)
  --- a/src/api.py
  +++ b/src/api.py
  @@ -25,7 +25,7 @@
  -def helper():
  +def helper() -> dict:
       pass

Or request access to veiled regions:
  fv unveil api.py#50-100
  # Then re-run: fv apply-patch changes.patch
```

#### Task 3: File System Watcher

Implement `fv watch` with logging:

```rust
// src/watch.rs
use notify::{Watcher, RecursiveMode, DebouncedEvent};
use std::sync::mpsc::channel;
use std::time::Duration;

pub struct FileWatcher {
    config: Config,
    logger: VeilLogger,
}

impl FileWatcher {
    pub fn watch(&self, project_root: &Path) -> Result<()> {
        let (tx, rx) = channel();
        let mut watcher = notify::watcher(tx, Duration::from_secs(1))?;
        
        // Watch all veiled files
        for (path, _) in &self.config.objects {
            let full_path = project_root.join(path);
            watcher.watch(&full_path, RecursiveMode::NonRecursive)?;
        }
        
        loop {
            match rx.recv() {
                Ok(event) => self.handle_event(event)?,
                Err(e) => error!("Watch error: {}", e),
            }
        }
    }
    
    fn handle_event(&self, event: DebouncedEvent) -> Result<()> {
        match event {
            DebouncedEvent::Write(path) => {
                // Check if veiled lines were modified
                let integrity = check_integrity(&path)?;
                if !integrity.is_valid {
                    self.logger.log_integrity_violation(&path, &integrity.modified_lines);
                }
            }
            _ => {}
        }
        Ok(())
    }
}
```

**CLI interface**:
```bash
# Start watching in foreground
fv watch

# Start watching in background (daemon)
fv watch --daemon
fv watch -d

# Specify log directory
fv watch --logs .funveil/logs/

# Stop watching
fv watch --stop
```

**Files to create/modify**:
- `src/watch.rs`: New module for file watching
- `src/cli.rs`: Add `watch` subcommand
- `Cargo.toml`: Add `notify` dependency

#### Task 4: Enhanced Error Messages with Remediation

Update all errors to include actionable guidance:

```rust
// src/error.rs
pub enum FunveilError {
    // ... existing errors
    FileIsVeiled {
        path: String,
        ranges: Vec<LineRange>,
    },
    VeiledLinesWouldBeModified {
        path: String,
        veiled_ranges: Vec<LineRange>,
        patch_ranges: Vec<LineRange>,
    },
}

impl fmt::Display for FunveilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunveilError::FileIsVeiled { path, ranges } => {
                write!(f, "{} is veiled (lines {:?}).\n", path, ranges)?;
                write!(f, "To edit this file:\n")?;
                write!(f, "  1. Unveil: fv unveil {}\n", path)?;
                write!(f, "  2. Make your edits\n")?;
                write!(f, "  3. Re-veil: fv veil {}\n", path)?;
                write!(f, "\nOr use patch-check to validate changes:\n")?;
                write!(f, "  git diff | fv patch-check")
            }
            // ... other errors
        }
    }
}
```

**Files to modify**:
- `src/error.rs`: Add new error types with remediation messages
- `src/veil.rs`: Update to use new errors

#### Task 5: Config v2 with Line-Level Tracking

Track veiled ranges per file for accurate validation:

```yaml
# .funveil_config (v2)
version: 2
mode: blacklist
objects:
  api.py:
    hash: b8e9c4f1a...
    permissions: "644"
    line_hashes:  # Hash per line for integrity checking
      "50": "a3f5d2e..."
      "51": "b8e2c9a..."
      # ...
    ranges:
      - start: 50
        end: 100
        hash: "c9d8e7f..."  # Combined hash for range
  secrets.env:
    hash: a3f5d2e9c...
    permissions: "600"
    ranges: null  # null = fully veiled
```

**Files to modify**:
- `src/types.rs`: Add line-level tracking structs
- `src/config.rs`: Update serialization, add migration
- `src/veil.rs`: Store line hashes when veiling

#### Task 6: Tool Hooks and Integration

Provide hooks for common tools:

```bash
# Git integration
fv hooks install-git  # Installs pre-commit hook

# Generated pre-commit hook:
#!/bin/bash
# Check if patch would modify veiled lines
if ! git diff | fv patch-check --quiet; then
    echo "Commit blocked: would modify veiled lines"
    git diff | fv patch-check
    exit 1
fi
```

**Files to create/modify**:
- `src/hooks.rs`: New module for tool integration
- `src/cli.rs`: Add `hooks` subcommand

### Configuration Changes

```yaml
version: 2
mode: blacklist
objects:
  api.py:
    hash: b8e9c4f1a...
    permissions: "644"
    ranges:
      - start: 50
        end: 100
        hash: "c9d8e7f..."
logs:
  enabled: true
  directory: ".funveil/logs"
  max_files: 7  # Keep 7 days of logs
  level: "info"  # debug, info, warn, error
```

### CLI Commands

```bash
# Patch validation
fv patch-check [OPTIONS] [FILE]
  --file, -f <FILE>     Read patch from file instead of stdin
  --json                Output JSON for machine parsing
  --summary             Human-readable output (default)
  --quiet, -q           Exit 0 if valid, non-zero if would affect veiled lines

# File system watching
fv watch [OPTIONS]
  --daemon, -d          Run in background
  --logs <DIR>          Log directory (default: .funveil/logs)
  --stop                Stop watching

# Enhanced doctor
fv doctor
  --check-integrity     Check all veiled files for modifications
  --fix                 Attempt to fix integrity issues (re-apply veils)

# Tool hooks
fv hooks install-git    # Install git pre-commit hook
fv hooks uninstall-git  # Remove git hook
```

### Testing Plan

Unit tests:
```rust
#[test]
fn test_patch_validation_no_overlap() {
    // Patch modifies lines 10-20, veiled is 50-100
    // Should pass
}

#[test]
fn test_patch_validation_with_overlap() {
    // Patch modifies lines 60-80, veiled is 50-100
    // Should fail with remediation instructions
}

#[test]
fn test_patch_syntax_error() {
    // Invalid patch format
    // Should return parse error
}

#[test]
fn test_logger_creates_log_file() {
    // Verify log file is created in .funveil/logs/
}

#[test]
fn test_logger_json_format() {
    // Verify log entries are valid JSON
}
```

E2E tests:
```bash
# Test patch-check with safe patch
echo "--- a/api.py
+++ b/api.py
@@ -10,5 +10,5 @@
 def helper():
-    pass
+    return 42" | fv patch-check
# Should pass

# Test patch-check with unsafe patch
echo "--- a/api.py
+++ b/api.py
@@ -60,5 +60,5 @@
 def secret():
-    pass
+    return 42" | fv patch-check
# Should fail - lines 60 are in veiled range 50-100

# Test watch mode
fv watch &
echo "modified" >> api.py  # Modify veiled file
# Check log file for warning
```

### Dependencies

```toml
[dependencies]
# File system watching
notify = "6.0"

# Structured logging
tracing = "0.1"
tracing-subscriber = "0.3"

# JSON serialization (already have serde, but ensure features)
serde_json = "1.0"

# Time for log rotation
chrono = "0.4"

# Daemonize for background mode
daemonize = "0.5"  # optional
```

### Success Criteria

- [ ] `fv patch-check` validates patches without applying them
- [ ] Patches that would modify veiled lines are rejected with clear remediation
- [ ] File system watcher logs all changes to veiled files
- [ ] Log files are structured (JSON) and rotated
- [ ] All error messages include actionable guidance
- [ ] Git pre-commit hook can be installed/uninstalled
- [ ] Config v2 supports line-level hash tracking
- [ ] Read-only protection remains active
- [ ] All existing tests pass
- [ ] New E2E tests for patch validation and watching

### Remediation Message Templates

```rust
const EDIT_VEILED_FILE_INSTRUCTIONS: &str = r#"
This file is currently veiled. To make edits:

  1. Unveil the file:   fv unveil {file}
  2. Make your edits
  3. Re-veil:          fv veil {file}

Or use the watcher mode for automatic re-veiling:

  fv watch

For patches/diffs, validate first:

  git diff | fv patch-check
"#;

const PATCH_WOULD_MODIFY_VEILED_INSTRUCTIONS: &str = r#"
This patch would modify veiled lines in {file}.

Veiled range:   {veiled_range}
Patch affects:  {patch_range}

Options:
  1. Unveil affected range:   fv unveil {file}#{veiled_range}
  2. Apply your patch
  3. Re-veil:                fv veil {file}#{veiled_range}

  4. Or edit the patch to avoid lines {veiled_range}
"#;

const INTEGRITY_VIOLATION_INSTRUCTIONS: &str = r#"
WARNING: Veiled lines in {file} were modified.

Modified lines: {lines}

Run 'fv apply' to restore veils to their original state.
If you intended to modify these lines, unveil first:
  fv unveil {file}#{range}
"#;
```

### Open Questions

1. **Patch format support**: Support git diff only, or also unified diff, ed diff, etc.?
2. **Binary patches**: How to handle binary file patches?
3. **Watch performance**: Debounce settings for large repos?
4. **Log retention**: How long to keep logs? Size-based rotation?
5. **Remote editing**: How to handle edits via SSH/remote FS?

### Next Steps

1. Implement logging infrastructure with rotation
2. Implement patch parser and validator
3. Add `fv patch-check` command
4. Implement file system watcher
5. Add `fv watch` command
6. Update error messages with remediation
7. Create git hook installer
8. Test with real patches from git workflows


#### Task 3: Patch Applier

Apply validated patches atomically with rollback support.

```rust
// src/patch/applier.rs

pub struct PatchApplier {
    config: Config,
    store: ContentStore,
}

pub struct ApplyResult {
    pub success: bool,
    pub applied_files: Vec<AppliedFile>,
    pub failed_files: Vec<FailedFile>,
    pub rollback_performed: bool,
}

pub struct AppliedFile {
    pub path: PathBuf,
    pub hunks_applied: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub lines_modified: usize,
}

impl PatchApplier {
    /// Apply patch with full validation and atomic operation
    pub fn apply(&self, patch: &ParsedPatch, dry_run: bool) -> Result<ApplyResult> {
        // 1. Validate all hunks first
        // 2. Create backups in CAS
        // 3. Apply hunks
        // 4. If any fails, rollback all changes
        // 5. Update config
    }
    
    fn apply_hunk(&self, file: &Path, hunk: &Hunk) -> Result<HunkResult> {
        // Apply line-by-line changes
    }
    
    fn rollback(&self, applied: &[AppliedFile]) -> Result<()> {
        // Restore from CAS backups
    }
}
```

**CLI**:
```bash
# Apply patch from file
fv apply-patch changes.patch

# Apply from stdin
cat changes.patch | fv apply-patch -

# Dry run (validate only)
fv apply-patch --dry-run changes.patch

# Show detailed progress
fv apply-patch --verbose changes.patch
```

**Output**:
```
$ fv apply-patch changes.patch

Analyzing patch...
✓ Patch format valid (unified diff)
✓ 2 files affected
✓ All changes in visible regions

Applying patch...
✓ src/api.py: 1 hunk applied (lines 25-25)
✓ src/utils.py: 1 hunk applied (lines 10-15)

Summary:
  Files modified: 2
  Lines added: 3
  Lines removed: 2
  Veiled regions: Preserved (0 conflicts)

Done. Run 'fv status' to see current state.
```

**Files to create/modify**:
- `src/patch/applier.rs`: Apply logic
- `src/cli.rs`: Add `apply-patch` command

#### Task 4: File System Watcher (Bypass Detection)

Detect direct modifications and guide users to use `apply-patch`.

```rust
// src/watch.rs
pub struct BypassWatcher {
    config: Config,
    logger: VeilLogger,
}

impl BypassWatcher {
    pub fn handle_modification(&self, path: &Path) -> WatchResult {
        // Check if file was modified outside of fv apply-patch
        // Log guidance on proper workflow
    }
}
```

**Watch output**:
```
$ fv watch

[2024-03-08 10:30:15] INFO: Watching 5 veiled files

[2024-03-08 10:35:22] WARNING: Direct modification detected
  File: api.py was modified without using fv apply-patch
  
  Recommended workflow:
    1. Generate a patch describing your changes
    2. Apply through funveil: fv apply-patch changes.patch
    3. Funveil validates and applies only safe changes
  
  To restore veils: fv apply
```

**CLI**:
```bash
fv watch              # Start watching
fv watch --stop       # Stop watching
```

**Files to create/modify**:
- `src/watch.rs`: Watch module
- `Cargo.toml`: Add `notify` dependency

#### Task 5: Edit Report Command

Show which lines are editable in a file.

```bash
fv edit-report api.py

File: api.py
═══════════════════════════════════════════════════════════════

Editable regions:
  ✅ Lines 1-20:   Imports and module docstring
  ✅ Lines 21-49:  Public API definitions
  ⛔ Lines 50-100: [VEILED] Implementation details
                   Hash: c9d8e7f...
  ✅ Lines 101-150: Public exports

To edit veiled region:
  fv unveil api.py#50-100
  # Make your edits
  fv veil api.py#50-100

Or create a patch:
  cat > changes.patch << 'EOF'
  --- a/api.py
  +++ b/api.py
  @@ -25,7 +25,7 @@
   def helper():
  -    pass
  +    return 42
  EOF
  fv apply-patch changes.patch
```

**Files to modify**:
- `src/cli.rs`: Add `edit-report` command

#### Task 6: Config v2 with Line-Level Tracking

```yaml
version: 2
mode: blacklist
objects:
  api.py:
    hash: b8e9c4f1a...
    permissions: "644"
    ranges:
      - start: 50
        end: 100
        hash: "c9d8e7f..."
```

**Files to modify**:
- `src/types.rs`: Add tracking structs
- `src/config.rs`: Update serialization

### CLI Commands Summary

```bash
# Core patch workflow
fv apply-patch <file>        # Apply patch file
fv apply-patch -             # Apply from stdin
fv apply-patch --dry-run     # Validate without applying

# Edit guidance
fv edit-report <file>        # Show editable lines

# Watching
fv watch                     # Watch for bypasses

# Existing commands enhanced
fv doctor --check-integrity  # Check patch-applied files
```

### Dependencies

```toml
[dependencies]
pest = "2.7"
pest_derive = "2.7"
notify = "6.0"
tracing = "0.1"
serde_json = "1.0"
chrono = "0.4"
```

### Success Criteria

- [ ] PEG parser handles unified diff, git diff with excellent errors
- [ ] `fv apply-patch` validates and applies patches atomically
- [ ] Patches affecting veiled lines are rejected with rich context
- [ ] `fv edit-report` shows editable regions with explanations
- [ ] File system watcher detects bypasses and guides users
- [ ] All patches are logged for audit trail
- [ ] Config v2 tracks line-level hashes
- [ ] Read-only protection remains active
- [ ] All existing tests pass
- [ ] E2E tests for patch application scenarios

### Remediation Templates

```rust
const PATCH_CONFLICT_MESSAGE: &str = r#"
❌ Patch cannot be applied to veiled lines

File: {file}
Veiled range: {veiled_range}
Patch hunk: {hunk_range}

The patch tries to modify lines that are currently veiled.

Options:
  1. Edit only visible lines:
     - Lines you CAN edit: {editable_ranges}
     - Generate new patch avoiding {veiled_range}
     
  2. Request access to veiled region:
     fv unveil {file}#{veiled_range}
     fv apply-patch your.patch
     fv veil {file}#{veiled_range}
     
  3. Use edit-report to see all editable regions:
     fv edit-report {file}
"#;

const APPLY_PATCH_GUIDANCE: &str = r#"
💡 To edit this file, use the patch workflow:

  1. Create a patch describing your changes:
     cat > changes.patch << 'EOF'
     --- a/{file}
     +++ b/{file}
     @@ -10,3 +10,4 @@
      line 10
      line 11
     +line 12 (your addition)
     EOF
     
  2. Apply through funveil:
     fv apply-patch changes.patch
     
  3. Funveil will validate and apply only safe changes

This ensures veiled regions remain protected.
"#;
```

### Open Questions

1. **Patch formats**: Support unified diff, git diff, ed scripts, wdiff?
2. **Binary patches**: Handle binary file diffs?
3. **Fuzzy matching**: Support patch(1)-style fuzzy application?
4. **3-way merge**: Support for merging patches?

### Next Steps

1. Implement PEG grammar for patch formats
2. Build rich error reporter
3. Implement `fv apply-patch` with validation
4. Add `fv edit-report` for visibility
5. Create file system watcher
6. Write comprehensive tests


---

## Phase 10: Patch Management System with History and Yank

**Status**: Planned  
**Goal**: Full patch management with apply/unapply, history tracking, and yank capability

### Design Principles

1. **Patch History**: All applied patches are tracked with ordering
2. **Reversible**: Patches can be unapplied in reverse order
3. **Yank-able**: Non-latest patches can be removed and subsequent patches re-applied
4. **Conflict Detection**: Verify re-applicability after yank
5. **Atomic Operations**: All patch operations are transactional

### Use Cases

```bash
# Apply patches in sequence
fv apply-patch feature-a.patch    # Applied as patch #1
fv apply-patch feature-b.patch    # Applied as patch #2
fv apply-patch feature-c.patch    # Applied as patch #3

# View patch history
fv patch-list
# Output:
# Patch History (newest first):
#   3. feature-c.patch (2024-03-08 14:30) - files: api.py, utils.py
#   2. feature-b.patch (2024-03-08 14:25) - files: api.py
#   1. feature-a.patch (2024-03-08 14:20) - files: config.py

# Unapply latest patch (reverse of apply)
fv patch-unapply 3
# Restores files to state before feature-c.patch
# Patches #1 and #2 remain applied

# Re-apply it
fv patch-reapply 3

# Yank a patch from the middle
# This removes patch #2 and re-applies patch #3 on top of patch #1
fv patch-yank 2
# Output:
# Yanking patch #2 (feature-b.patch)...
# Unapplying patch #3 (feature-c.patch)...
# Removing patch #2...
# Re-applying patch #3 (feature-c.patch)...
# ✓ Success. New order: #1 (feature-a), #2 (feature-c)

# Check if a patch can be yanked without conflicts
fv patch-yank --dry-run 2
# Shows what would happen without making changes

# Show patch details
fv patch-show 2
# Shows:
# - Patch content
# - Files affected
# - Dependencies on other patches
# - Can be yanked: yes/no
```

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Patch Management Layer                   │
│                                                             │
│  Patch Queue (ordered list):                                │
│    [#1: feature-a] -> [#2: feature-b] -> [#3: feature-c]   │
│                                                             │
│  Operations:                                                │
│    - apply: Add to end of queue                            │
│    - unapply: Remove from end (reverse apply)              │
│    - yank: Remove from middle, re-apply subsequent         │
│    - reapply: Re-apply after unapply                       │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                    Patch Storage Layer                      │
│                                                             │
│  .funveil/patches/                                          │
│    ├── 0001-feature-a.patch/                                │
│    │   ├── patch.raw        # Original patch content        │
│    │   ├── patch.parsed     # Parsed representation         │
│    │   ├── metadata.json    # Timestamp, files, author      │
│    │   └── reverse.patch    # Generated reverse patch       │
│    ├── 0002-feature-b.patch/                                │
│    └── 0003-feature-c.patch/                                │
│                                                             │
│  .funveil/patch-queue.yaml  # Ordered list of applied       │
└─────────────────────────────────────────────────────────────┘
```

### Data Structures

```rust
// src/patch/manager.rs

/// Patch with metadata
pub struct Patch {
    pub id: PatchId,           // Sequential number (1, 2, 3...)
    pub name: String,          // User-friendly name
    pub raw_content: String,   // Original patch text
    pub parsed: ParsedPatch,   // PEG-parsed representation
    pub metadata: PatchMetadata,
    pub reverse: ReversePatch, // Generated for unapply
}

pub struct PatchMetadata {
    pub applied_at: DateTime<Utc>,
    pub applied_by: Option<String>,  // For multi-user scenarios
    pub files_affected: Vec<PathBuf>,
    pub description: Option<String>,
}

/// Patch queue maintains ordering
pub struct PatchQueue {
    patches: Vec<Patch>,
    current_seq: u64,
}

impl PatchQueue {
    /// Apply new patch (adds to end)
    pub fn apply(&mut self, patch: Patch) -> Result<PatchId> {
        // Validate against current state
        // Apply to files
        // Generate reverse patch
        // Store in .funveil/patches/
        // Add to queue
    }
    
    /// Unapply latest patch (reverse apply)
    pub fn unapply(&mut self, id: PatchId) -> Result<()> {
        // Must be the latest patch
        // Apply reverse patch
        // Mark as unapplied (keep in history)
    }
    
    /// Yank a patch from the middle
    pub fn yank(&mut self, id: PatchId) -> Result<YankResult> {
        // 1. Find patch in queue
        // 2. Identify subsequent patches
        // 3. Unapply subsequent patches (in reverse order)
        // 4. Unapply target patch
        // 5. Delete target patch
        // 6. Re-apply subsequent patches (in original order)
        // 7. Handle conflicts during re-apply
    }
    
    /// Check if yank would succeed
    pub fn can_yank(&self, id: PatchId) -> Result<YankFeasibility> {
        // Simulate yank without applying
        // Report potential conflicts
    }
}

/// Result of yank operation
pub struct YankResult {
    pub yanked_patch: PatchId,
    pub re_applied: Vec<PatchId>,
    pub conflicts: Vec<YankConflict>,
}

pub struct YankFeasibility {
    pub can_yank: bool,
    pub would_reapply: Vec<PatchId>,
    pub potential_conflicts: Vec<String>,
    pub estimated_changes: usize,
}
```

### Implementation Plan

#### Task 1: Complete PEG Parser with Extensive Tests

Finish the PEG parser for all patch formats:

```rust
// src/patch/grammar.pest

////////////////////
// Unified Diff
////////////////////

unified_diff = { file_header+ }

file_header = {
    (old_file_line ~ new_file_line | git_file_line) ~
    hunk+
}

old_file_line = { "--- " ~ filename ~ ("\t" ~ timestamp)? ~ "\n" }
new_file_line = { "+++ " ~ filename ~ ("\t" ~ timestamp)? ~ "\n" }
git_file_line = { "diff --git " ~ filename ~ " " ~ filename ~ "\n" ~ 
                  (index_line | similarity_line)* ~
                  (old_file_line ~ new_file_line)? }

index_line = { "index " ~ hash ~ ".." ~ hash ~ (" " ~ mode)? ~ "\n" }
similarity_line = { "similarity index " ~ number ~ "%\n" }

timestamp = { date ~ " " ~ time ~ (" " ~ timezone)? }
date = { year ~ "-" ~ month ~ "-" ~ day }
time = { hour ~ ":" ~ minute ~ ":" ~ second }
timezone = { ("+" | "-") ~ hour ~ minute }

hunk = { hunk_header ~ hunk_body }
hunk_header = { "@@ " ~ old_range ~ " " ~ new_range ~ " @@" ~ section? ~ "\n" }
old_range = { "-" ~ start_line ~ "," ~ line_count }
new_range = { "+" ~ start_line ~ "," ~ line_count }
section = { " " ~ (!"\n" ~ any)* }

hunk_body = { (context_line | delete_line | add_line | no_newline)+ }
context_line = { " " ~ line_content ~ "\n" }
delete_line = { "-" ~ line_content ~ "\n" }
add_line = { "+" ~ line_content ~ "\n" }
no_newline = { "\\ No newline at end of file" ~ "\n" }

line_content = { (!"\n" ~ any)* }

////////////////////
// Git Diff (extended)
////////////////////

git_diff = { git_file_header+ }

git_file_header = {
    "diff --git " ~ filename ~ " " ~ filename ~ "\n" ~
    ("old mode " ~ mode ~ "\n" ~ "new mode " ~ mode ~ "\n")? ~
    ("deleted file mode " ~ mode ~ "\n")? ~
    ("new file mode " ~ mode ~ "\n")? ~
    ("similarity index " ~ number ~ "%\n")? ~
    ("rename from " ~ filename ~ "\n" ~ "rename to " ~ filename ~ "\n")? ~
    index_line? ~
    ("--- " ~ (filename | "\"/dev/null\"")) ~ "\n" ~
    ("+++ " ~ (filename | "\"/dev/null\"")) ~ "\n" ~
    hunk*
}

////////////////////
// Ed Script
////////////////////

ed_script = { ed_command+ }

ed_command = {
    line_range ~ "d" ~ "\n" |     // Delete
    line ~ "a" ~ "\n" ~ text ~ ".\n" |  // Append
    line ~ "c" ~ "\n" ~ text ~ ".\n" |  // Change
    line ~ "i" ~ "\n" ~ text ~ ".\n"    // Insert
}

line_range = { line ~ ("," ~ line)? }
line = { number | "$" }
text = { (!".\n" ~ any)* }

////////////////////
// Common Tokens
////////////////////

filename = { (!"\t" ~ !"\n" ~ any)+ }
hash = { hex_digit{7,40} }
mode = { "100644" | "100755" | "120000" | "160000" | "040000" }
number = { ASCII_DIGIT+ }
year = { ASCII_DIGIT{4} }
month = { ASCII_DIGIT{2} }
day = { ASCII_DIGIT{2} }
hour = { ASCII_DIGIT{2} }
minute = { ASCII_DIGIT{2} }
second = { ASCII_DIGIT{2} }
hex_digit = { ASCII_HEX_DIGIT }
```

**Extensive Test Suite**:

```rust
// src/patch/tests.rs

#[cfg(test)]
mod tests {
    use super::*;

    /////////////////
    // Valid patches
    /////////////////

    #[test]
    fn test_parse_simple_unified_diff() {
        let patch = r#"--- a/file.txt
+++ b/file.txt
@@ -1,5 +1,5 @@
 line 1
 line 2
-line 3
+line 3 modified
 line 4
 line 5
"#;
        let result = PatchParser::parse(Rule::unified_diff, patch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_git_diff() {
        let patch = r#"diff --git a/src/main.rs b/src/main.rs
index a3f5d2e..b8e9c4f 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,7 +10,8 @@ fn main() {
     println!("Hello");
-    let x = 5;
+    let x = 10;
+    let y = 20;
     println!("{}", x);
 }
"#;
        let result = PatchParser::parse(Rule::git_diff, patch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_multi_file_diff() {
        let patch = r#"diff --git a/file1.txt b/file1.txt
index 111..222 100644
--- a/file1.txt
+++ b/file1.txt
@@ -1 +1 @@
-old
+new

diff --git a/file2.txt b/file2.txt
index 333..444 100644
--- a/file2.txt
+++ b/file2.txt
@@ -1 +1 @@
-foo
+bar
"#;
        let parsed = PatchParser::parse(Rule::git_diff, patch).unwrap();
        assert_eq!(parsed.files.len(), 2);
    }

    #[test]
    fn test_parse_file_rename() {
        let patch = r#"diff --git a/old_name.txt b/new_name.txt
similarity index 98%
rename from old_name.txt
rename to new_name.txt
index a3f5d2e..b8e9c4f 100644
--- a/old_name.txt
+++ b/new_name.txt
@@ -5,3 +5,3 @@
 unchanged
-old content
+new content
 unchanged
"#;
        let result = PatchParser::parse(Rule::git_diff, patch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_binary_diff() {
        let patch = r#"diff --git a/image.png b/image.png
index a3f5d2e..b8e9c4f 100644
Binary files a/image.png and b/image.png differ
"#;
        let result = PatchParser::parse(Rule::git_diff, patch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_new_file() {
        let patch = r#"diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..a3f5d2e
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,3 @@
+line 1
+line 2
+line 3
"#;
        let result = PatchParser::parse(Rule::git_diff, patch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_deleted_file() {
        let patch = r#"diff --git a/deleted.txt b/deleted.txt
deleted file mode 100644
index a3f5d2e..0000000
--- a/deleted.txt
+++ /dev/null
@@ -1,3 +0,0 @@
-line 1
-line 2
-line 3
"#;
        let result = PatchParser::parse(Rule::git_diff, patch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_ed_script() {
        let patch = r#"10d
20a
inserted line
another line
.
30c
changed line
.
"#;
        let result = PatchParser::parse(Rule::ed_script, patch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_huge_patch() {
        // Test with 10,000 line patch
        let mut patch = String::new();
        patch.push_str("--- a/big.txt\n+++ b/big.txt\n@@ -1,10000 +1,10000 @@\n");
        for i in 0..10000 {
            if i == 5000 {
                patch.push_str("-old line\n");
                patch.push_str("+new line\n");
            } else {
                patch.push_str(&format!(" unchanged line {}\n", i));
            }
        }
        let result = PatchParser::parse(Rule::unified_diff, &patch);
        assert!(result.is_ok());
    }

    /////////////////
    // Invalid patches (expect errors)
    /////////////////

    #[test]
    fn test_parse_missing_hunk_header() {
        let patch = r#"--- a/file.txt
+++ b/file.txt
 line 1
-line 2
+line 2 modified
"#;
        let result = PatchParser::parse(Rule::unified_diff, patch);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_malformed_range() {
        let patch = r#"--- a/file.txt
+++ b/file.txt
@@ -1,abc +1,3 @@
 line 1
"#;
        let result = PatchParser::parse(Rule::unified_diff, patch);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_inconsistent_line_count() {
        let patch = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,5 @@
 line 1
 line 2
"#;
        // 2 lines but header says 5
        let result = PatchParser::parse(Rule::unified_diff, patch);
        // Parser succeeds, validator catches this
        let parsed = result.unwrap();
        let validation = validate_line_counts(&parsed);
        assert!(validation.is_err());
    }

    #[test]
    fn test_parse_empty_patch() {
        let patch = "";
        let result = PatchParser::parse(Rule::unified_diff, patch);
        assert!(result.is_err());
    }

    /////////////////
    // Edge cases
    /////////////////

    #[test]
    fn test_parse_no_newline_at_eof() {
        let patch = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line 1
 line 2
 line 3
\ No newline at end of file
"#;
        let result = PatchParser::parse(Rule::unified_diff, patch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_empty_file_creation() {
        let patch = r#"diff --git a/empty.txt b/empty.txt
new file mode 100644
index 0000000..e69de29
--- /dev/null
+++ b/empty.txt
"#;
        let result = PatchParser::parse(Rule::git_diff, patch);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_special_characters_in_content() {
        let patch = r#"--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-foo	bar
+foo    bar
"#;
        let result = PatchParser::parse(Rule::unified_diff, patch);
        assert!(result.is_ok());
    }

    /////////////////
    // Error messages
    /////////////////

    #[test]
    fn test_error_message_has_line_number() {
        let patch = r#"--- a/file.txt
+++ b/file.txt
invalid line here
@@ -1,3 +1,3 @@
"#;
        let err = PatchParser::parse(Rule::unified_diff, patch).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("line 3") || msg.contains("3:"));
    }

    #[test]
    fn test_error_message_has_suggestion() {
        let patch = r#"--- a/file.txt
+++ b/file.txt
 @@ -1,3 +1,3 @@
"#;
        // Leading space in hunk header
        let err = PatchParser::parse(Rule::unified_diff, patch).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("remove") || msg.contains("leading space"));
    }
}
```

**Files to create/modify**:
- `src/patch/grammar.pest`: Complete PEG grammar
- `src/patch/parser.rs`: Parser implementation
- `src/patch/tests.rs`: Extensive test suite
- `Cargo.toml`: Add pest dependencies

#### Task 2: Patch Storage and Metadata

Create patch storage system:

```rust
// src/patch/storage.rs

pub struct PatchStorage {
    base_path: PathBuf,
}

impl PatchStorage {
    pub fn store(&self, patch: &Patch) -> Result<()> {
        // Store in .funveil/patches/0001-name.patch/
    }
    
    pub fn load(&self, id: PatchId) -> Result<Patch> {
        // Load from storage
    }
    
    pub fn generate_reverse(&self, patch: &Patch) -> Result<ReversePatch> {
        // Generate reverse patch for unapply
    }
    
    pub fn delete(&self, id: PatchId) -> Result<()> {
        // Remove patch directory
    }
}
```

**Storage layout**:
```
.funveil/
├── patches/
│   ├── 0001-add-feature.patch/
│   │   ├── patch.raw
│   │   ├── patch.json (parsed)
│   │   ├── metadata.json
│   │   └── reverse.patch
│   ├── 0002-fix-bug.patch/
│   └── 0003-refactor.patch/
└── patch-queue.yaml
```

**Files to create/modify**:
- `src/patch/storage.rs`: Storage management
- `src/patch/metadata.rs`: Metadata structures

#### Task 3: Patch Queue Manager

Implement the patch queue with all operations:

```rust
// src/patch/queue.rs

pub struct PatchQueueManager {
    queue: PatchQueue,
    storage: PatchStorage,
    validator: PatchValidator,
    applier: PatchApplier,
}

impl PatchQueueManager {
    /// Apply a new patch
    pub fn apply(&mut self, patch_content: &str, name: &str) -> Result<PatchId> {
        // 1. Parse patch
        // 2. Validate against current state
        // 3. Apply to working tree
        // 4. Generate reverse patch
        // 5. Store patch
        // 6. Add to queue
        // 7. Update patch-queue.yaml
    }
    
    /// Unapply latest patch
    pub fn unapply(&mut self, id: PatchId) -> Result<()> {
        // Verify id is latest
        // Apply reverse patch
        // Update queue state
    }
    
    /// Yank a patch (remove from middle)
    pub fn yank(&mut self, id: PatchId) -> Result<YankReport> {
        // 1. Get patches after target
        // 2. Unapply subsequent patches (reverse order)
        // 3. Unapply target
        // 4. Delete target storage
        // 5. Re-apply subsequent patches
        // 6. Renumber remaining patches
    }
    
    /// Check yank feasibility
    pub fn can_yank(&self, id: PatchId) -> Result<CanYankReport> {
        // Simulate without applying
    }
    
    /// Re-apply a previously unapplied patch
    pub fn reapply(&mut self, id: PatchId) -> Result<()> {
        // Apply patch at current state
    }
    
    /// List patches in order
    pub fn list(&self) -> Vec<PatchSummary> {
        // Return ordered list
    }
    
    /// Show patch details
    pub fn show(&self, id: PatchId) -> Result<PatchDetails> {
        // Full patch info
    }
}
```

**Files to create/modify**:
- `src/patch/queue.rs`: Queue manager
- `src/patch/yank.rs`: Yank logic with conflict handling

#### Task 4: CLI Commands

```bash
# Apply new patch
fv patch-apply <file> [--name <name>] [--description <desc>]

# List patches
fv patch-list [--all] [--applied-only] [--unapplied-only]

# Show patch details
fv patch-show <id>

# Unapply (revert) latest
fv patch-unapply [<id>]  # Default: latest

# Re-apply previously unapplied
fv patch-reapply <id>

# Yank (remove) a patch
fv patch-yank <id> [--force] [--dry-run]

# Check if yank would work
fv patch-can-yank <id>

# Reorder patches (advanced)
fv patch-reorder <id> --before <other-id>

# Export patch queue as combined diff
fv patch-export --all > all-changes.patch
```

**Files to modify**:
- `src/cli.rs`: Add patch subcommands
- `src/main.rs`: Wire up commands

#### Task 5: Yank with Conflict Resolution

Handle conflicts during yank:

```rust
pub enum YankStrategy {
    /// Fail on any conflict
    Strict,
    /// Attempt 3-way merge
    Merge,
    /// Force re-apply, marking conflicts
    Force,
}

pub struct YankReport {
    pub yanked: PatchId,
    pub unapplied: Vec<PatchId>,
    pub reapplied: Vec<ReapplyResult>,
    pub conflicts: Vec<Conflict>,
    pub resolution_required: bool,
}

pub struct Conflict {
    pub file: PathBuf,
    pub hunks: Vec<ConflictingHunk>,
    pub resolution_options: Vec<ResolutionOption>,
}
```

**Conflict resolution UI**:
```bash
$ fv patch-yank 2

Yanking patch #2 (fix-bug.patch)...

Unapplying subsequent patches:
  ✓ Unapplied #3 (refactor.patch)
  ✓ Unapplied #4 (add-feature.patch)

Removing patch #2...
  ✓ Deleted from storage

Re-applying patches:
  ✓ Re-applied #3 (refactor.patch) - 2 hunks
  ⚠ Conflict in #4 (add-feature.patch):
    
    File: src/api.py
    Lines: 45-50
    
    The hunk context no longer matches after removing #2.
    
    Options:
      1. View diff:       fv patch-show 4
      2. Skip this patch: fv patch-yank 2 --skip-subsequent=4
      3. Force apply:     fv patch-yank 2 --force
      4. Cancel:          fv patch-yank --abort
```

**Files to create/modify**:
- `src/patch/conflict.rs`: Conflict detection and resolution
- `src/patch/merge.rs`: 3-way merge for conflicting hunks

#### Task 6: Integration with Veiled Files

Patches must respect veiled regions:

```rust
impl PatchValidator {
    /// Check if patch would modify veiled lines
    pub fn check_veiled_regions(&self, patch: &Patch) -> Result<VeiledCheckResult> {
        // For each hunk in patch:
        //   - Check if any modified line is in a veiled range
        //   - Collect all conflicts
    }
}

/// Integration with apply
impl PatchQueueManager {
    pub fn apply(&mut self, patch_content: &str, name: &str) -> Result<PatchId> {
        // Parse patch
        let patch = self.parser.parse(patch_content)?;
        
        // Check veiled regions
        let veiled_check = self.validator.check_veiled_regions(&patch)?;
        if !veiled_check.is_safe {
            return Err(FunveilError::PatchWouldModifyVeiled {
                conflicts: veiled_check.conflicts,
            });
        }
        
        // Continue with apply...
    }
}
```

### Configuration

```yaml
# .funveil/patch-queue.yaml
version: 1
patches:
  - id: 1
    name: "add-feature-a"
    status: applied
    applied_at: "2024-03-08T14:20:00Z"
    files:
      - config.py
    
  - id: 2
    name: "fix-bug"
    status: unapplied  # Was applied, then unapplied
    applied_at: "2024-03-08T14:25:00Z"
    unapplied_at: "2024-03-08T15:00:00Z"
    files:
      - api.py
    
  - id: 3
    name: "add-feature-b"
    status: applied
    applied_at: "2024-03-08T14:30:00Z"
    files:
      - api.py
      - utils.py

current_sequence: 4  # Next patch ID
```

### Testing

```rust
// Integration tests
#[test]
fn test_patch_apply_unapply_sequence() {
    // Apply 3 patches
    // Unapply latest
    // Verify file state
    // Re-apply
    // Verify restored
}

#[test]
fn test_patch_yank_middle() {
    // Apply patches A, B, C
    // Yank B
    // Verify C still works on top of A
}

#[test]
fn test_patch_yank_with_conflict() {
    // Apply patches that depend on each other
    // Yank middle patch
    // Verify conflict detection
}

#[test]
fn test_patch_respects_veiled_regions() {
    // Veil lines in file
    // Apply patch that would modify veiled lines
    // Verify rejected with clear error
}
```

### Dependencies

```toml
[dependencies]
pest = "2.7"
pest_derive = "2.7"
chrono = { version = "0.4", features = ["serde"] }
serde_yaml = "0.9"
```

### Success Criteria

- [ ] PEG parser handles all patch formats with excellent errors
- [ ] 100+ parser tests covering edge cases
- [ ] Patches stored in `.funveil/patches/` with metadata
- [ ] Patch queue maintains ordering
- [ ] `patch-apply` adds to queue
- [ ] `patch-unapply` reverses latest
- [ ] `patch-yank` removes from middle with re-apply
- [ ] Conflicts during yank are detected and reported
- [ ] Veiled regions respected during patch application
- [ ] All operations are logged


---

## Phase 11: Patch-Level Undo for All Operations (Unified Change Tracking)

**Status**: Planned  
**Goal**: All funveil operations (veil, unveil, apply, etc.) are recorded as patches in the same queue, enabling unified undo/redo

### Design Philosophy

**Every state change is a patch.**

Whether the user applies a patch file or funveil modifies a file (veiling/unveiling), the change is recorded as a patch in the unified queue. This provides:

1. **Unified history**: One timeline of all changes
2. **Granular undo**: Undo any operation (user patch or funveil operation)
3. **Audit trail**: Complete history of all modifications
4. **Reproducibility**: Can replay entire session

### Use Cases

```bash
# User applies a patch
fv patch-apply feature.patch
# -> Creates patch #1 in queue

# User veils some lines
fv veil api.py#50-100
# -> Creates synthetic patch #2 "veil api.py#50-100"
#    This patch records: "lines 50-100 replaced with ...[hash]..."

# User applies another patch
fv patch-apply fix.patch  
# -> Creates patch #3 in queue

# User unveils
fv unveil api.py#50-100
# -> Creates synthetic patch #4 "unveil api.py#50-100"

# View unified history
fv patch-list
# 1. feature.patch (user)
# 2. veil api.py#50-100 (funveil)
# 3. fix.patch (user)
# 4. unveil api.py#50-100 (funveil)

# Undo the unveil (re-veils the lines)
fv patch-unapply 4

# Undo the veil (unveils the lines)
fv patch-unapply 2

# Undo a user patch
fv patch-unapply 1
# -> Reverts feature.patch changes

# Redo (re-apply)
fv patch-reapply 1
```

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  Unified Patch Queue                        │
│                                                             │
│   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐      │
│   │ Patch 1 │->│ Patch 2 │->│ Patch 3 │->│ Patch 4 │      │
│   │ (user)  │  │(funveil)│  │ (user)  │  │(funveil)│      │
│   └─────────┘  └─────────┘  └─────────┘  └─────────┘      │
│                                                             │
│   Each patch:                                               │
│   - ID: Sequential number                                   │
│   - Type: User | Veil | Unveil | Apply | Config            │
│   - Forward patch: Apply this change                        │
│   - Reverse patch: Undo this change                         │
│   - Metadata: timestamp, description, parent state          │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                  Operation Types                            │
│                                                             │
│   1. User Patch (PATCH_USER)                                │
│      - From: fv patch-apply <file>                          │
│      - Content: User-supplied diff                          │
│      - Undo: Apply reverse patch                            │
│                                                             │
│   2. Veil Operation (PATCH_VEIL)                            │
│      - From: fv veil <file>#<range>                         │
│      - Content: Synthetic patch showing                     │
│        "replace lines X-Y with veil markers"                │
│      - Undo: Replace veil markers with original content     │
│                                                             │
│   3. Unveil Operation (PATCH_UNVEIL)                        │
│      - From: fv unveil <file>#<range>                       │
│      - Content: Synthetic patch showing                     │
│        "replace veil markers with original content"         │
│      - Undo: Re-veil the lines                              │
│                                                             │
│   4. Apply Operation (PATCH_APPLY)                          │
│      - From: fv apply                                       │
│      - Content: All veil/unveil changes made by apply       │
│      - Undo: Reverse all those changes                      │
│                                                             │
│   5. Config Change (PATCH_CONFIG)                           │
│      - From: fv mode, fv unveil --all, etc.                 │
│      - Content: Config diff                                 │
│      - Undo: Restore previous config                        │
└─────────────────────────────────────────────────────────────┘
```

### Data Model Extensions

```rust
// src/patch/manager.rs

/// Type of patch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchType {
    User,       // User-supplied patch file
    Veil,       // fv veil operation
    Unveil,     // fv unveil operation  
    Apply,      // fv apply operation
    Config,     // Configuration change
}

/// Extended patch with operation type
#[derive(Debug, Clone)]
pub struct Patch {
    pub id: PatchId,
    pub patch_type: PatchType,
    pub name: String,
    pub description: String,
    pub raw_content: String,
    pub parsed: ParsedPatch,
    pub reverse_parsed: ParsedPatch,  // Pre-computed reverse
    pub metadata: PatchMetadata,
}

/// Metadata with operation context
#[derive(Debug, Clone)]
pub struct PatchMetadata {
    pub applied_at: chrono::DateTime<chrono::Utc>,
    pub files_affected: Vec<PathBuf>,
    pub description: Option<String>,
    pub parent_state: ContentHash,  // State before this patch
    pub resulting_state: ContentHash,  // State after this patch
}

/// Manager with unified tracking
pub struct PatchManager {
    queue: VecDeque<Patch>,
    storage: PatchStorage,
    next_id: u64,
    mode: TrackingMode,  // How to track operations
}

/// Tracking modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingMode {
    /// Only track user patches (default)
    UserOnly,
    /// Track user patches + veil/unveil operations
    WithVeilOps,
    /// Track everything including apply/config
    Full,
}
```

### Synthetic Patch Generation

```rust
/// Generate a synthetic patch for a veil operation
pub fn generate_veil_patch(
    file: &Path,
    ranges: &[LineRange],
    original_content: &str,
    veiled_content: &str,
) -> ParsedPatch {
    // Create a patch that shows:
    // - Old: original lines
    // - New: veil markers (...[hash]...)
    
    let mut hunks = Vec::new();
    let mut line_num = 1;
    
    for range in ranges {
        // Build hunk for this range
        let old_lines: Vec<&str> = original_content
            .lines()
            .skip(range.start() - 1)
            .take(range.len())
            .collect();
        
        let new_lines = vec![format!("...[hash]...")];
        
        let hunk_lines: Vec<Line> = old_lines
            .iter()
            .map(|l| Line::Delete(l.to_string()))
            .chain(new_lines.iter().map(|l| Line::Add(l.clone())))
            .collect();
        
        hunks.push(Hunk {
            old_start: range.start(),
            old_count: range.len(),
            new_start: range.start(),
            new_count: 1,
            section: None,
            lines: hunk_lines,
        });
    }
    
    ParsedPatch {
        files: vec![FilePatch {
            old_path: Some(file.to_path_buf()),
            new_path: Some(file.to_path_buf()),
            old_mode: None,
            new_mode: None,
            is_new_file: false,
            is_deleted: false,
            is_rename: false,
            is_copy: false,
            is_binary: false,
            hunks,
            similarity: None,
        }],
        format: PatchFormat::Synthetic,
    }
}

/// Generate synthetic patch for unveil operation
pub fn generate_unveil_patch(
    file: &Path,
    ranges: &[LineRange],
    veiled_content: &str,
    original_content: &str,
) -> ParsedPatch {
    // Reverse of veil: delete markers, add original lines
    // ... similar structure ...
}
```

### Integration Points

```rust
// src/veil.rs

/// Veil file with patch tracking
pub fn veil_file(
    root: &Path,
    config: &mut Config,
    patch_manager: &mut PatchManager,
    file: &str,
    ranges: Option<&[LineRange]>,
) -> Result<()> {
    // 1. Read original content
    let original = fs::read_to_string(root.join(file))?;
    
    // 2. Perform veil operation (existing logic)
    let veiled = perform_veil(root, config, file, ranges)?;
    
    // 3. Generate synthetic patch
    if patch_manager.mode() == TrackingMode::WithVeilOps {
        let synthetic_patch = generate_veil_patch(
            file,
            ranges.unwrap_or(&[]),
            &original,
            &veiled,
        );
        
        patch_manager.apply_synthetic(
            synthetic_patch,
            PatchType::Veil,
            &format!("veil {}{}", file, range_str(ranges)),
        )?;
    }
    
    Ok(())
}

/// Unveil file with patch tracking
pub fn unveil_file(
    root: &Path,
    config: &mut Config,
    patch_manager: &mut PatchManager,
    file: &str,
    ranges: Option<&[LineRange]>,
) -> Result<()> {
    // Similar structure: perform operation, generate synthetic patch
}
```

### CLI Extensions

```bash
# Configuration
fv config tracking-mode user-only      # Default
fv config tracking-mode with-veil-ops  # Include veil/unveil
fv config tracking-mode full           # Everything

# Enhanced patch-list shows operation type
fv patch-list
# ID  Type     Name                    Files    Date
# 1   user     feature-a.patch         api.py   2024-03-08 10:00
# 2   veil     veil api.py#50-100      api.py   2024-03-08 10:05
# 3   user     bugfix.patch            utils.py 2024-03-08 10:10
# 4   unveil   unveil api.py#50-100    api.py   2024-03-08 10:15

# Show details including synthetic patches
fv patch-show 2
# Type: veil
# Description: veil api.py#50-100
# 
# Diff:
# --- a/api.py
# +++ b/api.py
# @@ -50,50 +100,1 @@
# -def internal():
# -    # implementation
# -    ...
# +...[a3f5d2e]...
#
# Original content stored in: .funveil/patches/0002-veil-api-py-50-100/original/

# Undo any operation
fv patch-unapply 2
# This re-veils the lines (restores veil markers)
```

### State Snapshots

For proper undo, each patch records the state before and after:

```rust
/// State snapshot for a file
pub struct FileState {
    pub path: PathBuf,
    pub content_hash: ContentHash,
    pub is_veiled: bool,
    pub veiled_ranges: Vec<LineRange>,
}

/// Snapshot of entire project state
pub struct ProjectState {
    pub files: HashMap<PathBuf, FileState>,
    pub config_hash: ContentHash,
}

impl PatchManager {
    /// Record state before applying patch
    fn record_pre_state(&mut self, files: &[PathBuf]) -> Result<ProjectState> {
        // Capture state of all affected files
    }
    
    /// Verify state matches expectation before undo/redo
    fn verify_state(&self, expected: &ProjectState) -> Result<bool> {
        // Check current state matches expected
        // Used before unapply to ensure clean undo
    }
}
```

### Storage Layout

```
.funveil/
├── patches/
│   ├── 0001-feature-a.patch/
│   │   ├── patch.raw           # Original user patch
│   │   ├── metadata.yaml
│   │   └── forward/            # Applied state (optional)
│   │
│   ├── 0002-veil-api-py-50-100/
│   │   ├── patch.synthetic     # Generated patch
│   │   ├── metadata.yaml
│   │   ├── original/           # Original content backup
│   │   │   └── api.py          # Full file before veil
│   │   └── forward/            # Veiled content
│   │       └── api.py          # Full file after veil
│   │
│   ├── 0003-bugfix.patch/
│   │   └── ...
│   │
│   └── queue.yaml              # Ordered list with types
│
└── states/                     # State snapshots
    ├── state-0001-before.yaml
    ├── state-0001-after.yaml
    └── ...
```

### Conflict Resolution

When undoing a funveil operation:

```rust
pub enum UndoResult {
    Success,
    Conflict(Vec<UndoConflict>),
    StateMismatch { expected: String, actual: String },
}

pub struct UndoConflict {
    pub file: PathBuf,
    pub conflict_type: ConflictType,
    pub resolution_options: Vec<ResolutionOption>,
}

pub enum ConflictType {
    /// File was modified since patch was applied
    FileModified,
    /// Veil markers were manually edited
    VeilMarkersChanged,
    /// File was deleted
    FileDeleted,
}
```

### Implementation Tasks

1. **Extend Patch Data Model**
   - Add `PatchType` enum
   - Add `reverse_parsed` to Patch
   - Add state hashes to metadata

2. **Synthetic Patch Generation**
   - `generate_veil_patch()`
   - `generate_unveil_patch()`
   - `generate_apply_patch()` (multiple changes)

3. **Integration with Veil/Unveil**
   - Modify `veil_file()` to optionally create patches
   - Modify `unveil_file()` to optionally create patches
   - Add `TrackingMode` to Config

4. **State Snapshots**
   - `ProjectState` struct
   - Snapshot before/after each operation
   - Verification before undo

5. **CLI Updates**
   - `fv config tracking-mode`
   - Enhanced `fv patch-list` with types
   - Enhanced `fv patch-show` for synthetic patches

6. **Tests**
   - Test veil/unveil patch generation
   - Test undo of veil operation
   - Test conflict detection
   - Test state snapshots

### Success Criteria

- [ ] User patches tracked as before
- [ ] Veil operations create synthetic patches
- [ ] Unveil operations create synthetic patches
- [ ] Can undo any operation via `patch-unapply`
- [ ] Can redo via `patch-reapply`
- [ ] State snapshots verify clean undo
- [ ] Conflicts detected and reported
- [ ] Tracking mode configurable
- [ ] All existing tests pass
- [ ] New tests for patch-level undo

### Open Questions

1. **Granularity**: One patch per veil operation, or batch multiple?
2. **Storage**: Store full file snapshots or just diffs?
3. **Performance**: Snapshot overhead acceptable?
4. **Cleanup**: When to purge old state snapshots?
5. **Branching**: Support patch "branches" or linear only?
