# Language Features Reference

Funveil supports 12 languages via [tree-sitter](https://tree-sitter.github.io/tree-sitter/) for code-aware veiling — parsing symbols, detecting entrypoints, and tracing call graphs.

For getting started, see [TUTORIAL.md](TUTORIAL.md). For the full specification, see [SPEC.md](../SPEC.md).

---

## Quick Reference Table

| Language | Extensions | Symbols Extracted | Entrypoints Detected |
|----------|-----------|-------------------|----------------------|
| [Rust](#rust-rs) | `.rs` | Functions, Structs, Traits, Imports | Main, Test, CLI |
| [Go](#go-go) | `.go` | Functions, Methods, Structs, Interfaces, Imports | Main, Test |
| [Zig](#zig-zig) | `.zig` | Functions, Structs, Tests, Imports | Main, Test |
| [TypeScript](#typescripttsx-ts-tsx) | `.ts`, `.tsx` | Functions, Components, Classes, JSX, Imports/Exports | Main, Test, Handler |
| [HTML](#html-html-htm) | `.html`, `.htm` | Elements, Script blocks, Style blocks | Main, Handler |
| [CSS/SCSS](#cssscss-css-scss-sass) | `.css`, `.scss`, `.sass` | Rules, Selectors, At-rules, Tailwind directives | Main, Handler |
| [Python](#python-py-pyi) | `.py`, `.pyi` | Functions, Classes, Imports | Main, Test, CLI, Handler |
| [Bash](#bash-sh-bash) | `.sh`, `.bash` | Functions, Scripts | Main |
| [Terraform/HCL](#terraformhcl-tf-tfvars-hcl) | `.tf`, `.tfvars`, `.hcl` | Resources, Modules, Data blocks, Variables | Main, Handler |
| [Helm/YAML](#helmyaml-yaml-yml) | `.yaml`, `.yml` | Templates, Values, Structure | Main, Handler |
| [XML](#xml-xml) | `.xml` | Elements, Attributes, Namespaces | Main |
| [Markdown](#markdown-md-markdown) | `.md`, `.markdown`, `.mdown`, `.mkd` | Headings, Code blocks, Links | Main, Handler |

---

## Systems Languages

### Rust (.rs)

**Symbols extracted:** Functions (with attributes like `#[test]`, `#[tokio::main]`), structs, traits, imports.

**Entrypoints detected:**
- `fn main()` → Main
- `#[test]` functions, `test_*` / `*_test` naming → Test
- `#[tokio::main]`, `#[actix_web::main]` → Main (async)
- `#[derive(Parser)]` (clap) → CLI

```bash
# Parse a Rust file
fv parse src/main.rs --format summary

# Find all test entrypoints in Rust files
fv entrypoints --entry-type test --language rust
```

### Go (.go)

**Symbols extracted:** Functions, methods (with receivers), structs, interfaces, imports (single and grouped), function calls.

**Entrypoints detected:**
- `func main()` in `package main` → Main
- `func init()` → Main
- `TestXxx`, `BenchmarkXxx`, `ExampleXxx`, `FuzzXxx` in `*_test.go` → Test

```bash
# Find Go main entrypoints
fv entrypoints --entry-type main --language go

# Trace from a Go function
fv trace --from HandleRequest --depth 3
```

### Zig (.zig)

**Symbols extracted:** Functions (pub/private), structs, unions, enums, `@import` statements, test declarations, function calls.

**Entrypoints detected:**
- `pub fn main()` → Main
- `test "name" {}` blocks → Test
- `build.zig` → Main

```bash
# Parse a Zig file
fv parse src/main.zig --format detailed

# Find test blocks
fv entrypoints --entry-type test
```

---

## Web & Frontend

### TypeScript/TSX (.ts, .tsx)

**Symbols extracted:** Function declarations, arrow function components, React components (PascalCase), JSX elements, hooks (`use*` pattern), TypeScript interfaces/types, imports/exports.

**Entrypoints detected:**
- React components: `App`, `Main`, `Page` → Main
- Next.js conventions: `page.tsx`, `layout.tsx` → Main
- Functions named `main`, `run`, `start` → Main
- `test*`, `it(`, `describe(` → Test
- JSX elements → Handler
- File-based: `App.tsx` → Main

```bash
# Parse a React component
fv parse src/App.tsx --format summary

# Find all handlers in TypeScript files
fv entrypoints --entry-type handler --language type-script
```

### HTML (.html, .htm)

**Symbols extracted:** HTML elements (tags, attributes), `<script>` blocks (with `src` tracking), `<style>` blocks.

**Entrypoints detected:**
- HTML files → Main (page structure)
- `<script>` blocks → Handler
- `<style>` blocks → Handler

```bash
# Parse an HTML file
fv parse index.html --format summary

# Show only element structure (header mode)
fv veil 'index.html' --mode headers
```

### CSS/SCSS (.css, .scss, .sass)

**Symbols extracted:** CSS rules and selectors, at-rules (`@media`, `@layer`), Tailwind directives (`@tailwind`, `@apply`), CSS custom properties (variables), nested selectors (SCSS).

**Entrypoints detected:**
- Main stylesheets: `main.css`, `index.css`, `styles.css`, `app.css` (and `.scss` variants), files with "tailwind" in name → Main
- Tailwind directives → Handler
- Other CSS files → Handler

```bash
# Parse a stylesheet
fv parse src/styles/main.scss --format summary

# Veil all stylesheets except the main one
fv veil '/.*\.css$/'
fv unveil src/styles/main.css
```

---

## Scripting

### Python (.py, .pyi)

**Symbols extracted:** Functions, classes, imports.

**Entrypoints detected:**
- Functions named `main`, `cli`, `run` → Main
- Functions matching `test_*` → Test
- Functions containing `command` or `cmd` → CLI
- Functions containing `route` or `endpoint` → Handler

```bash
# Find all Python entrypoints
fv entrypoints --language python

# Trace from a handler function
fv trace --from handle_request --depth 2 --format tree
```

### Bash (.sh, .bash)

**Symbols extracted:** Function declarations, scripts (files).

**Entrypoints detected:**
- Shell script files → Main
- Functions named `main`, `run`, `start` → Main

```bash
# Parse a shell script
fv parse scripts/deploy.sh --format summary

# Find all bash entrypoints
fv entrypoints --language bash
```

---

## Infrastructure & Configuration

### Terraform/HCL (.tf, .tfvars, .hcl)

**Symbols extracted:** Resource blocks, module blocks, data blocks, variables.

**Entrypoints detected:**
- Files: `main.tf`, `variables.tf`, `outputs.tf` → Main
- Resource/module/data blocks → Handler
- Root `.tf` files → Main

```bash
# Parse Terraform configuration
fv parse infra/main.tf --format summary

# Find all Terraform entrypoints
fv entrypoints --language terraform
```

### Helm/YAML (.yaml, .yml)

**Symbols extracted:** Hierarchical YAML structure, templates.

**Entrypoints detected:**
- `Chart.yaml` → Main (chart metadata)
- `values.yaml` → Main (configuration)
- Files in `/templates/` → Handler

```bash
# Parse Helm values
fv parse helm/values.yaml --format summary

# Find Helm chart entrypoints
fv entrypoints --language helm
```

### XML (.xml)

**Symbols extracted:** Elements (tag names), attributes, namespaces, CDATA sections.

**Entrypoints detected:**
- Configuration files: `pom.xml`, `AndroidManifest.xml`, `web.xml`, `settings.xml`, `*.config.xml` → Main
- Other XML files → Main

```bash
# Parse a Maven POM
fv parse pom.xml --format summary
```

---

## Documentation

### Markdown (.md, .markdown)

**Symbols extracted:** Headings (ATX and Setext, with levels), fenced code blocks (with language detection), links, images, lists, tables, frontmatter (YAML/TOML/JSON).

**Entrypoints detected:**
- Well-known docs: `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `LICENSE.md`, `INSTALL.md`, `API.md` → Main
- Level 1 heading at line 1 → Main (document title)
- Other `.md` files → Handler

```bash
# Parse document structure
fv parse docs/TUTORIAL.md --format summary

# Show only headings (header mode)
fv veil 'docs/*.md' --mode headers
```

---

## Entrypoint Detection

Funveil detects five types of entrypoints across all supported languages:

| Type | Description | Languages |
|------|-------------|-----------|
| **Main** | Program entry, main functions | Rust, Go, Zig, TypeScript, Python, Bash, Terraform, Helm, HTML, CSS, XML, Markdown |
| **Test** | Test functions and blocks | Rust, Go, Zig, TypeScript, Python |
| **CLI** | CLI command handlers | Rust, Python |
| **Handler** | Request handlers, routes, templates | TypeScript, Python, Terraform, Helm, HTML, CSS, Markdown |
| **Export** | Exported public APIs | TypeScript |

### Usage

```bash
# List all entrypoints in the project
fv entrypoints

# Filter by type
fv entrypoints --entry-type main
fv entrypoints --entry-type test
fv entrypoints --entry-type handler

# Filter by language
fv entrypoints --language rust
fv entrypoints --language python

# Combine filters
fv entrypoints --entry-type test --language go
```

---

## Call Graph Tracing

Trace function call relationships forward (callees) or backward (callers).

### Forward Tracing

```bash
# What does process_payment call?
fv trace --from process_payment --depth 2

# Output as a tree
fv trace --from process_payment --depth 3 --format tree

# Output as a list
fv trace --from process_payment --depth 2 --format list

# Output as DOT graph (for Graphviz)
fv trace --from process_payment --depth 3 --format dot
```

### Backward Tracing

```bash
# What calls validate_token?
fv trace --to validate_token --depth 3
```

### From Entrypoints

```bash
# Trace from all detected entrypoints
fv trace --from-entrypoint --depth 2
```

### Filtering

```bash
# Exclude standard library calls
fv trace --from process_payment --depth 3 --no-std
```

---

## Code Parsing

Parse a file to extract its structure — functions, classes, imports, and other symbols.

```bash
# Summary view: symbol names and line ranges
fv parse src/main.rs --format summary

# Detailed view: full symbol information
fv parse src/main.rs --format detailed

# Default format (summary)
fv parse src/main.rs
```

---

## Header Mode Veiling

Show only function/class signatures while hiding implementations. Useful for giving an AI agent an overview of a file without exposing all the code.

```bash
# Veil files but keep headers visible
fv veil 'src/*.py' --mode headers

# Combines well with parse for exploration
fv parse src/auth.py --format summary
fv veil src/auth.py --mode headers
```

---

## Analysis Cache

Funveil caches parse results and analysis data for performance. Manage the cache with:

```bash
# Check cache status (size, entries)
fv cache status

# Clear entire cache
fv cache clear

# Invalidate cache for changed files
fv cache invalidate
```

---

## See Also

- [TUTORIAL.md](TUTORIAL.md) — Getting started guide
- [SPEC.md](../SPEC.md) — Complete specification
- [CONTRIBUTING.md](CONTRIBUTING.md) — Development setup and guidelines
- [LANGUAGE_SUPPORT_PLAN.md](../LANGUAGE_SUPPORT_PLAN.md) — Implementation status (developer-facing)
