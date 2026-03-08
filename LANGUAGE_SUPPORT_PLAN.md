# Multi-Language Support Plan

> Branch: `feat/multi-lang-support`  
> Goal: Add tree-sitter based intelligent veiling support for multiple languages

## Overview

This plan extends Funveil's intelligent veiling capabilities to support multiple languages and technologies. Each phase adds specific language parsers and queries for code-aware veiling (showing only signatures, entrypoints, call graphs, etc.)

## Languages to Support

| Phase | Language/Tech | Priority | Tree-sitter Grammar |
|-------|--------------|----------|---------------------|
| 1 | Go | High | `tree-sitter-go` |
| 2 | Zig | High | `tree-sitter-zig` |
| 3 | HTML | Medium | `tree-sitter-html` |
| 4 | React + TypeScript | High | `tree-sitter-typescript` |
| 5 | CSS + TailwindCSS | Medium | `tree-sitter-css` |
| 6 | XML | Low | `tree-sitter-xml` |
| 7 | Markdown | Low | `tree-sitter-markdown` |

---

## Phase 1: Go Support ✅ Complete

### Goals
- [x] Parse Go source files (.go)
- [x] Extract function declarations (with receivers)
- [x] Extract struct/interface declarations
- [x] Extract import statements
- [x] Identify entrypoints (package main, func main())

### Implementation Summary

**Files Created:**
- `src/parser/languages/go.rs` - Go parser implementation with 600+ lines
- `src/parser/languages/mod.rs` - Language module exports

**Files Modified:**
- `Cargo.toml` - Added `tree-sitter-go = "0.25"`
- `src/parser/mod.rs` - Added `Language::Go` variant and `.go` detection
- `src/parser/tree_sitter_parser.rs` - Added Go queries and parser initialization
- `src/analysis/entrypoints.rs` - Added Go entrypoint detection

**Features Implemented:**
- Function and method parsing (including receivers)
- Struct and interface type extraction
- Import statement parsing (single and grouped)
- Function call extraction
- Entrypoint detection: `main()`, `init()`, test functions (`Test*`, `Benchmark*`, `Example*`, `Fuzz*`)
- Test file detection (`*_test.go`)

**Tree-sitter Queries:**
- `GO_FUNCTION_QUERY` - Functions and methods
- `GO_TYPE_QUERY` - Structs and interfaces  
- `GO_IMPORT_QUERY` - Import statements
- `GO_CALL_QUERY` - Function calls

**Tests Added (6):**
- `test_parse_simple_function` - Basic function parsing
- `test_parse_method` - Method with receiver
- `test_parse_struct_and_interface` - Type declarations
- `test_parse_imports` - Import extraction
- `test_parse_test_file` - Test function detection
- `test_is_test_function` - Test naming convention

### Usage
```rust
use funveil::parser::languages::go::parse_go_file;

let parsed = parse_go_file(Path::new("main.go"), source_code)?;

// Access functions
for func in parsed.functions() {
    println!("Function: {}", func.name());
}

// Access imports
for import in &parsed.imports {
    println!("Import: {}", import.path);
}
```

---

## Phase 2: Zig Support

### Goals
- Parse Zig source files (.zig)
- Extract function declarations (including `pub` visibility)
- Extract struct/union/enum declarations
- Extract import statements (`@import`)
- Identify entrypoints (`pub fn main()`)

### File Detection
- Extensions: `.zig`
- Shebang: N/A
- Special: `build.zig` for build configuration

### Tree-sitter Queries Needed
```scheme
; Function declarations
(function_declaration
  name: (identifier) @function.name)

; Public function declarations
(function_declaration
  (visibility_modifier) @visibility
  name: (identifier) @function.name)

; Struct declarations
(container_field
  name: (identifier) @field.name)

; Import statements
(builtin_call
  function: "@import"
  arguments: (string_literal) @import.path)
```

### Entrypoint Detection
- `pub fn main()` function
- Test declarations with `test "name" {}`

### Deliverables
- [ ] Add `tree-sitter-zig` dependency
- [ ] Create `src/parser/languages/zig.rs`
- [ ] Add Zig queries to `src/parser/queries/zig.scm`
- [ ] Implement `ZigParser` with `LanguageParser` trait
- [ ] Add tests for Zig parsing
- [ ] Update language detection

---

## Phase 3: HTML Support

### Goals
- Parse HTML files (.html, .htm)
- Extract tag structure
- Identify script and style blocks
- Extract element attributes (id, class, etc.)
- Support for veiling inline scripts/styles

### File Detection
- Extensions: `.html`, `.htm`, `.xhtml`
- DOCTYPE detection

### Tree-sitter Queries Needed
```scheme
; Tag structure
(element
  (start_tag
    name: (tag_name) @tag.name)
  (end_tag
    name: (tag_name) @tag.name))

; Script blocks
(script_element
  (raw_text) @script.content)

; Style blocks
(style_element
  (raw_text) @style.content)

; Attributes
(attribute
  name: (attribute_name) @attr.name
  value: (quoted_attribute_value)? @attr.value)
```

### Entrypoint Detection
- Main document (no specific entrypoint)
- Script tags with `src` attribute

### Deliverables
- [ ] Add `tree-sitter-html` dependency
- [ ] Create `src/parser/languages/html.rs`
- [ ] Add HTML queries to `src/parser/queries/html.scm`
- [ ] Implement `HtmlParser` with `LanguageParser` trait
- [ ] Add tests for HTML parsing
- [ ] Update language detection

---

## Phase 4: React + TypeScript Support

### Goals
- Parse TypeScript (.ts) and TSX (.tsx) files
- Parse React components (function and class components)
- Extract JSX elements
- Extract hooks usage
- Extract TypeScript types/interfaces
- Extract imports/exports

### File Detection
- Extensions: `.ts`, `.tsx`, `.mts`, `.cts`
- JSX detection in `.tsx` files

### Tree-sitter Queries Needed
```scheme
; TypeScript function declarations
(function_declaration
  name: (identifier) @function.name)

; Arrow functions (common in React)
(lexical_declaration
  (variable_declarator
    name: (identifier) @function.name
    value: (arrow_function)))

; React components (functions returning JSX)
(function_declaration
  name: (identifier) @component.name
  body: (statement_block
    (return_statement
      (jsx_element))))

; Class components
(class_declaration
  name: (type_identifier) @class.name
  (class_heritage
    (extends_clause
      value: (identifier) @extends)))

; JSX elements
(jsx_element
  (jsx_opening_element
    name: (identifier) @jsx.tag))

; TypeScript interfaces
(interface_declaration
  name: (type_identifier) @interface.name)

; Type aliases
(type_alias_declaration
  name: (type_identifier) @type.name)

; Import statements
(import_statement
  (import_clause)? @import.clause
  source: (string) @import.source)

; Hooks detection
(call_expression
  function: (identifier) @hook.name
  (#match? @hook.name "^use[A-Z]"))
```

### Entrypoint Detection
- `ReactDOM.render()` or `createRoot().render()`
- Next.js: `pages/*.tsx`, `app/*.tsx`
- React Native: `App.tsx`

### Deliverables
- [ ] Add `tree-sitter-typescript` dependency
- [ ] Create `src/parser/languages/typescript.rs`
- [ ] Add TypeScript/TSX queries to `src/parser/queries/typescript.scm`
- [ ] Implement `TypeScriptParser` with `LanguageParser` trait
- [ ] Add React-specific query patterns
- [ ] Add tests for TypeScript/React parsing
- [ ] Update language detection

---

## Phase 5: CSS + TailwindCSS Support

### Goals
- Parse CSS files (.css)
- Parse SCSS/Sass files (.scss, .sass)
- Extract CSS rules and selectors
- Identify TailwindCSS directives (`@apply`, `@layer`)
- Parse CSS custom properties (variables)

### File Detection
- Extensions: `.css`, `.scss`, `.sass`, `.less`, `.pcss`
- Style blocks in HTML/JSX

### Tree-sitter Queries Needed
```scheme
; CSS rules
(rule_set
  (selectors
    (selector) @selector)
  (block
    (declaration
      (property_name) @property
      (property_value) @value)))

; At-rules (@media, @import, etc.)
(at_rule
  (at_keyword) @at.keyword
  (prelude)? @at.prelude)

; Tailwind directives
(at_rule
  (at_keyword) @tailwind.keyword
  (#match? @tailwind.keyword "@(apply|layer|config|tailwind)"))

; CSS custom properties
(declaration
  (property_name) @custom.property
  (#match? @custom.property "^--"))

; SCSS nesting
(nested_selector
  (selector) @nested.selector)
```

### Entrypoint Detection
- Main CSS files (no specific entrypoint)
- CSS imports in JS/TS

### Deliverables
- [ ] Add `tree-sitter-css` dependency
- [ ] Create `src/parser/languages/css.rs`
- [ ] Add CSS queries to `src/parser/queries/css.scm`
- [ ] Implement `CssParser` with `LanguageParser` trait
- [ ] Add SCSS/Sass support
- [ ] Add tests for CSS parsing
- [ ] Update language detection

---

## Phase 6: XML Support

### Goals
- Parse XML files (.xml)
- Extract element structure
- Extract namespaces
- Parse XML declarations and DOCTYPE

### File Detection
- Extensions: `.xml`
- MIME type: `application/xml`, `text/xml`

### Tree-sitter Queries Needed
```scheme
; XML declaration
(xml_declaration
  (version) @xml.version)

; Elements
(element
  (start_tag
    name: (identifier) @element.name)
  (end_tag
    name: (identifier) @element.name))

; Attributes
(attribute
  name: (identifier) @attr.name
  value: (string) @attr.value)

; Namespaces
(namespace_declaration
  (identifier) @namespace.name)
```

### Entrypoint Detection
- Root element (no specific entrypoint)

### Deliverables
- [ ] Add `tree-sitter-xml` dependency
- [ ] Create `src/parser/languages/xml.rs`
- [ ] Add XML queries to `src/parser/queries/xml.scm`
- [ ] Implement `XmlParser` with `LanguageParser` trait
- [ ] Add tests for XML parsing
- [ ] Update language detection

---

## Phase 7: Markdown Support

### Goals
- Parse Markdown files (.md, .markdown)
- Extract headings structure
- Extract code blocks with language info
- Extract links and images
- Identify tables and lists

### File Detection
- Extensions: `.md`, `.markdown`, `.mdown`, `.mkd`
- Frontmatter detection (YAML/JSON/TOML)

### Tree-sitter Queries Needed
```scheme
; Headings
(atx_heading
  (atx_marker) @heading.marker
  (heading_content) @heading.content)

; Code blocks
(fenced_code_block
  (info_string) @code.language
  (code_fence_content) @code.content)

; Links
(link
  (link_text) @link.text
  (link_destination) @link.dest)

; Images
(image
  (link_text) @image.alt
  (link_destination) @image.src)

; Tables
(table
  (table_header) @table.header
  (table_row) @table.row)
```

### Entrypoint Detection
- First heading (usually title)
- Frontmatter title

### Deliverables
- [ ] Add `tree-sitter-markdown` dependency
- [ ] Create `src/parser/languages/markdown.rs`
- [ ] Add Markdown queries to `src/parser/queries/markdown.scm`
- [ ] Implement `MarkdownParser` with `LanguageParser` trait
- [ ] Add tests for Markdown parsing
- [ ] Update language detection

---

## Implementation Structure

### New Files to Create

```
src/parser/
├── mod.rs                    # Parser registry, language detection
├── tree_sitter_ext.rs        # Safe wrappers around tree-sitter
├── language.rs               # Language enum and traits
├── languages/
│   ├── mod.rs                # Language module exports
│   ├── go.rs                 # Go parser implementation
│   ├── zig.rs                # Zig parser implementation
│   ├── html.rs               # HTML parser implementation
│   ├── typescript.rs         # TypeScript/React parser
│   ├── css.rs                # CSS/SCSS parser
│   ├── xml.rs                # XML parser
│   └── markdown.rs           # Markdown parser
└── queries/
    ├── go.scm                # Go tree-sitter queries
    ├── zig.scm               # Zig tree-sitter queries
    ├── html.scm              # HTML tree-sitter queries
    ├── typescript.scm        # TypeScript/TSX queries
    ├── css.scm               # CSS/SCSS queries
    ├── xml.scm               # XML tree-sitter queries
    └── markdown.scm          # Markdown tree-sitter queries
```

### Core Traits

```rust
/// Trait for language-specific parsers
pub trait LanguageParser: Send + Sync {
    /// Returns the language identifier
    fn language(&self) -> Language;
    
    /// Parse source code and extract symbols
    fn parse(&self, source: &str) -> Result<ParsedFile, ParseError>;
    
    /// Detect entrypoints in the source
    fn find_entrypoints(&self, source: &str) -> Vec<Entrypoint>;
    
    /// Extract imports/includes
    fn extract_imports(&self, source: &str) -> Vec<Import>;
    
    /// Check if a position is within a symbol body (for header mode)
    fn is_in_body(&self, source: &str, line: usize) -> bool;
}

/// Represents a parsed file
pub struct ParsedFile {
    pub language: Language,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub entrypoints: Vec<Entrypoint>,
}

/// Symbol types
pub enum Symbol {
    Function {
        name: String,
        params: Vec<Parameter>,
        return_type: Option<String>,
        line_range: LineRange,
        is_public: bool,
    },
    Struct {
        name: String,
        fields: Vec<Field>,
        line_range: LineRange,
        is_public: bool,
    },
    Interface {
        name: String,
        methods: Vec<String>,
        line_range: LineRange,
    },
    // ... other symbol types
}
```

## Dependencies to Add

### Cargo.toml Additions

```toml
[dependencies]
# Tree-sitter core
tree-sitter = "0.20"

# Language grammars
tree-sitter-go = "0.20"
tree-sitter-zig = { git = "https://github.com/maxxnino/tree-sitter-zig", branch = "main" }
tree-sitter-html = "0.20"
tree-sitter-typescript = "0.20"
tree-sitter-css = "0.20"
tree-sitter-xml = "0.20"
tree-sitter-markdown = "0.20"

# Additional utilities
once_cell = "1.19"  # For lazy static initialization
```

## Testing Strategy

### Unit Tests
- Test each language parser with sample files
- Test symbol extraction accuracy
- Test line range calculations
- Test edge cases (empty files, syntax errors)

### Integration Tests
- Test full veiling/unveiling workflow
- Test header mode for each language
- Test entrypoint detection

### Sample Files for Testing
```
tests/fixtures/
├── go/
│   ├── simple.go
│   ├── with_tests.go
│   └── module/
│       └── main.go
├── zig/
│   ├── simple.zig
│   └── build.zig
├── html/
│   ├── simple.html
│   └── with_script.html
├── typescript/
│   ├── simple.ts
│   ├── react.tsx
│   └── hooks.tsx
├── css/
│   ├── simple.css
│   ├── tailwind.css
│   └── scss.scss
├── xml/
│   └── simple.xml
└── markdown/
    ├── simple.md
    └── with_code.md
```

## Progress Tracking

| Phase | Language | Status | PR |
|-------|----------|--------|-----|
| 1 | Go | ✅ Complete | a2adc26 |
| 2 | Zig | ✅ Complete | 95728e3 |
| 3 | HTML | ✅ Complete | f76bdc6 |
| 4 | React + TypeScript | ✅ Complete | 416d3e2 |
| 5 | CSS + TailwindCSS | ✅ Complete | 53c6f15 |
| 6 | XML | ✅ Complete | 3b5935f |
| 7 | Markdown | 🔲 Not Started | - |

## Notes

- Tree-sitter grammars may have version compatibility issues - verify with `tree-sitter` core version
- Some grammars (like Zig) may need to be pulled from git rather than crates.io
- Query files (.scm) should be embedded in the binary using `include_str!`
- Consider using a build script to validate queries at compile time
- Test performance with large files (>10k lines) for each language

## References

- [Tree-sitter Documentation](https://tree-sitter.github.io/tree-sitter/)
- [Tree-sitter Query Syntax](https://tree-sitter.github.io/tree-sitter/using-parsers#query-syntax)
- [Tree-sitter Rust Bindings](https://docs.rs/tree-sitter/latest/tree_sitter/)
