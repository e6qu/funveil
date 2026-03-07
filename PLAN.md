# Funveil Intelligent Veiling - Development Plan

## Overview
Extend funveil with automatic code-aware veiling modes that help LLMs understand codebases efficiently by showing/hiding code based on semantic analysis rather than just file paths.

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
