# Language Support Plan

> All 7 phases complete ✅  
> Total: 12 supported languages with tree-sitter based intelligent veiling

## Overview

Funveil uses tree-sitter parsers for code-aware veiling - showing only function signatures, tracing call graphs, detecting entrypoints, and understanding code structure.

## Supported Languages

| Language | Extensions | Tree-sitter Grammar | Status |
|----------|------------|---------------------|--------|
| Rust | `.rs` | `tree-sitter-rust` | ✅ |
| TypeScript | `.ts`, `.tsx` | `tree-sitter-typescript` | ✅ |
| Python | `.py` | `tree-sitter-python` | ✅ |
| Bash | `.sh`, `.bash` | `tree-sitter-bash` | ✅ |
| Terraform/HCL | `.tf` | `tree-sitter-hcl` | ✅ |
| Helm/YAML | `.yaml`, `.yml` | `tree-sitter-yaml` | ✅ |
| Go | `.go` | `tree-sitter-go` | ✅ Complete |
| Zig | `.zig` | `tree-sitter-zig` | ✅ Complete |
| HTML | `.html`, `.htm` | `tree-sitter-html` | ✅ Complete |
| CSS/SCSS | `.css`, `.scss`, `.sass` | `tree-sitter-css` | ✅ Complete |
| XML | `.xml` | `tree-sitter-xml` | ✅ Complete |
| Markdown | `.md`, `.markdown` | `tree-sitter-markdown-fork` | ✅ Complete |

## Implementation Summary

### Go (Phase 1)

**Files**: `src/parser/languages/go.rs`

**Features**:
- Function and method parsing (including receivers)
- Struct and interface type extraction
- Import statement parsing (single and grouped)
- Function call extraction
- Entrypoint detection: `main()`, `init()`, test functions
- Test file detection (`*_test.go`)

**Tests**: 6 tests covering functions, methods, types, imports, and tests

### Zig (Phase 2)

**Files**: `src/parser/languages/zig.rs`

**Features**:
- Function declarations with visibility modifiers
- Struct/union/enum declarations
- `@import` statement extraction
- `pub fn main()` entrypoint detection
- Test block detection (`test "name" {}`)

**Tests**: 4 tests covering functions, structs, imports, and entrypoints

### HTML (Phase 3)

**Files**: `src/parser/languages/html.rs`

**Features**:
- Element structure extraction (tags, attributes)
- Script block detection and content extraction
- Style block detection
- Script `src` attribute tracking

**Tests**: 4 tests covering elements, scripts, styles, and attributes

### React + TypeScript (Phase 4)

**Files**: Enhanced `src/parser/languages/typescript.rs`

**Features**:
- TSX component parsing
- React function components
- JSX element extraction
- Hook detection (`use*` pattern)
- TypeScript interfaces and types
- Import/export extraction

**Tests**: 5 tests covering components, JSX, hooks, and types

### CSS + TailwindCSS (Phase 5)

**Files**: `src/parser/languages/css.rs`

**Features**:
- CSS rule and selector extraction
- Tailwind directive detection (`@apply`, `@layer`)
- CSS custom properties (variables)
- SCSS/Sass support (`.scss`, `.sass`)
- Nested selector parsing

**Tests**: 5 tests covering rules, selectors, Tailwind, and SCSS

### XML (Phase 6)

**Files**: `src/parser/languages/xml.rs`

**Features**:
- Element structure extraction
- Attribute parsing
- Namespace handling
- Config file detection (pom.xml, AndroidManifest.xml, etc.)
- CDATA section handling

**Tests**: 3 tests covering elements, attributes, and config files

### Markdown (Phase 7)

**Files**: `src/parser/languages/markdown.rs`

**Features**:
- Heading structure extraction (ATX and Setext)
- Fenced code blocks with language detection
- Link and image extraction
- List and table detection
- Frontmatter support (YAML/TOML/JSON)

**Tests**: 3 tests covering headings, code blocks, and structure

## Architecture

```
src/parser/
├── mod.rs                    # Language enum and detection
├── tree_sitter_parser.rs     # Parser initialization and queries
└── languages/
    ├── mod.rs                # Module exports
    ├── go.rs                 # Go parser
    ├── zig.rs                # Zig parser
    ├── html.rs               # HTML parser
    ├── typescript.rs         # TypeScript/React parser
    ├── css.rs                # CSS parser
    ├── xml.rs                # XML parser
    └── markdown.rs           # Markdown parser
```

## Core Traits

```rust
/// Trait for language-specific parsers
pub trait LanguageParser: Send + Sync {
    fn language(&self) -> Language;
    fn parse(&self, source: &str) -> Result<ParsedFile, ParseError>;
    fn find_entrypoints(&self, source: &str) -> Vec<Entrypoint>;
    fn extract_imports(&self, source: &str) -> Vec<Import>;
}
```

## Testing

Each language has dedicated tests in `tests/`:

| Language | Test File | Test Count |
|----------|-----------|------------|
| All | `parser_integration_test.rs` | 14 integration |
| Core | Various unit tests | 71 unit |
| CLI | `cli_test.rs` | 6 CLI |
| **Total** | | **91 tests** |

## Adding a New Language

1. Add variant to `Language` enum in `src/parser/mod.rs`
2. Add detection in `detect_language()` function
3. Create `src/parser/languages/{lang}.rs` with parsing logic
4. Add queries in `src/parser/tree_sitter_parser.rs`
5. Add entrypoint detection in `src/analysis/entrypoints.rs`
6. Add dependency to `Cargo.toml`
7. Add tests

See the existing language implementations for examples.

## Dependencies

```toml
[dependencies]
tree-sitter = "0.26"
tree-sitter-rust = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-python = "0.23"
tree-sitter-bash = "0.25"
tree-sitter-hcl = "1.1"
tree-sitter-yaml = "0.7"
tree-sitter-go = "0.25"
tree-sitter-zig = "1.1"
tree-sitter-html = "0.23"
tree-sitter-css = "0.25"
tree-sitter-xml = "0.7"
tree-sitter-markdown-fork = "0.7.3"
```

## References

- [Tree-sitter Documentation](https://tree-sitter.github.io/tree-sitter/)
- [Tree-sitter Query Syntax](https://tree-sitter.github.io/tree-sitter/using-parsers#query-syntax)
- [SPEC.md](../SPEC.md) - Funveil specification
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Development guidelines
