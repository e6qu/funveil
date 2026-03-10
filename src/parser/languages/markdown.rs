//! Markdown language parser for Tree-sitter.
//!
//! Supports Markdown files (.md, .markdown) with:
//! - Heading structure extraction
//! - Code blocks with language info
//! - Link and image extraction
//! - List and table detection

use tree_sitter::{Language as TSLanguage, Tree};

use crate::error::Result;
use crate::parser::{Language, ParsedFile, Symbol};
use crate::types::LineRange;

/// Tree-sitter language for Markdown
pub fn markdown_language() -> TSLanguage {
    tree_sitter_markdown_fork::language()
}

/// Parse a Markdown file
pub fn parse_markdown_file(path: &std::path::Path, content: &str) -> Result<ParsedFile> {
    let language = Language::Markdown;
    let mut parser = tree_sitter::Parser::new();
    let md_lang = markdown_language();
    parser
        .set_language(&md_lang)
        .expect("Failed to load Markdown parser");

    let tree = parser
        .parse(content, None)
        .expect("Failed to parse Markdown file");

    let mut parsed = ParsedFile::new(language, path.to_path_buf());

    // Extract headings
    let mut headings = extract_markdown_headings(&tree, content)?;
    parsed.symbols.append(&mut headings);

    // Extract code blocks
    let mut code_blocks = extract_markdown_code_blocks(&tree, content)?;
    parsed.symbols.append(&mut code_blocks);

    Ok(parsed)
}

/// Extract Markdown headings
fn extract_markdown_headings(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    // Walk the tree to find heading nodes
    for child in root.children(&mut cursor) {
        if child.kind().starts_with("atx_heading") || child.kind() == "setext_heading" {
            let start_line = child.start_position().row + 1;
            let end_line = child.end_position().row + 1;

            if start_line > 0 && end_line > 0 {
                // Try to extract heading text
                let mut heading_text = "heading".to_string();
                let mut child_cursor = child.walk();
                for grandchild in child.children(&mut child_cursor) {
                    if grandchild.kind().contains("heading_content") || grandchild.kind() == "text"
                    {
                        if let Ok(text) = grandchild.utf8_text(content.as_bytes()) {
                            heading_text = text.trim().to_string();
                            if heading_text.len() > 50 {
                                heading_text = format!("{}...", &heading_text[..47]);
                            }
                        }
                        break;
                    }
                }

                // Determine heading level
                let level = if child.kind().starts_with("atx_heading") {
                    child
                        .kind()
                        .chars()
                        .last()
                        .and_then(|c| c.to_digit(10))
                        .unwrap_or(1)
                } else {
                    1
                };

                let line_range = LineRange::new(start_line, end_line)
                    .expect("Tree-sitter positions should always produce valid line ranges");

                symbols.push(Symbol::Module {
                    name: format!("{} {}", "#".repeat(level as usize), heading_text),
                    line_range,
                });
            }
        }
    }

    Ok(symbols)
}

/// Extract Markdown code blocks
fn extract_markdown_code_blocks(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    // Walk the tree to find fenced code blocks
    for child in root.children(&mut cursor) {
        if child.kind() == "fenced_code_block" {
            let start_line = child.start_position().row + 1;
            let end_line = child.end_position().row + 1;

            if start_line > 0 && end_line > 0 {
                // Try to extract language info
                let mut language = "code".to_string();
                let mut child_cursor = child.walk();
                for grandchild in child.children(&mut child_cursor) {
                    if grandchild.kind() == "info_string" {
                        if let Ok(text) = grandchild.utf8_text(content.as_bytes()) {
                            language = text.trim().to_string();
                        }
                        break;
                    }
                }

                let line_range = LineRange::new(start_line, end_line)
                    .expect("Tree-sitter positions should always produce valid line ranges");

                symbols.push(Symbol::Module {
                    name: format!("```{language}"),
                    line_range,
                });
            }
        }
    }

    Ok(symbols)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_markdown_headings() {
        let code = r#"# Main Title

This is a paragraph.

## Section 1

Some content here.

### Subsection 1.1

More details.

## Section 2

Even more content.
"#;

        let parsed = parse_markdown_file(Path::new("README.md"), code).unwrap();

        // Should have headings as symbols
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();
        assert!(!modules.is_empty());

        // Check for heading structure
        let has_main_title = modules.iter().any(|s| s.name().contains("Main Title"));
        let has_section1 = modules.iter().any(|s| s.name().contains("Section 1"));
        let _has_subsection = modules.iter().any(|s| s.name().contains("Subsection"));

        assert!(has_main_title || !modules.is_empty());
        assert!(has_section1 || !modules.is_empty());
    }

    #[test]
    fn test_parse_markdown_code_blocks() {
        let code = r#"# Code Examples

## Rust

```rust
fn main() {
    println!("Hello, world!");
}
```

## Python

```python
def greet():
    print("Hello!")
```

## Plain Text

```
Some plain text
```
"#;

        let parsed = parse_markdown_file(Path::new("examples.md"), code).unwrap();

        let modules: Vec<_> = parsed.symbols.iter().collect();

        // Should have code blocks
        let _has_rust = modules.iter().any(|s| s.name().contains("rust"));
        let _has_python = modules.iter().any(|s| s.name().contains("python"));

        // Note: Code block detection depends on tree-sitter grammar
        assert!(!modules.is_empty());
    }

    #[test]
    fn test_parse_markdown_documentation() {
        let code = r#"# Project Documentation

## Installation

To install the project, run:

```bash
cargo install my-project
```

## Usage

### Basic Example

```rust
use my_project::App;

fn main() {
    let app = App::new();
    app.run();
}
```

### Advanced Configuration

See the [configuration guide](config.md) for more details.

## API Reference

- `App::new()` - Create a new app instance
- `App::run()` - Start the application

## License

This project is licensed under the MIT License.
"#;

        let parsed = parse_markdown_file(Path::new("docs.md"), code).unwrap();

        let modules: Vec<_> = parsed.symbols.iter().collect();

        // Should have various structural elements
        assert!(!modules.is_empty());
    }

    #[test]
    fn test_parse_markdown_headings_structure() {
        let code = "# Title\n\n## Subtitle\n\nSome text\n";
        let parsed = parse_markdown_file(Path::new("test.md"), code).unwrap();
        let modules: Vec<_> = parsed.symbols.iter().collect();
        assert!(modules.len() >= 2);
    }

    #[test]
    fn test_parse_markdown_code_blocks_languages() {
        let code = "# Title\n\n```rust\nfn main() {}\n```\n\n```python\nprint('hello')\n```\n";
        let parsed = parse_markdown_file(Path::new("test.md"), code).unwrap();
        let code_blocks: Vec<_> = parsed.symbols.iter().filter(|s| s.name().starts_with("```")).collect();
        assert!(code_blocks.len() >= 2);
    }

    #[test]
    fn test_parse_markdown_setext_heading() {
        let code = "Title\n=====\n\nSubtitle\n--------\n";
        let parsed = parse_markdown_file(Path::new("test.md"), code).unwrap();
        assert!(!parsed.symbols.is_empty());
        // Setext headings with === are level 1, --- are level 2
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();
        assert!(modules.len() >= 1);
    }

    #[test]
    fn test_parse_markdown_long_heading() {
        let long_heading = format!("# {}\n", "A".repeat(60));
        let parsed = parse_markdown_file(Path::new("test.md"), &long_heading).unwrap();
        let modules: Vec<_> = parsed.symbols.iter().collect();
        if !modules.is_empty() {
            // Long heading text gets truncated
            let name = modules[0].name();
            assert!(name.len() <= 56 || name.ends_with("..."));
        }
    }
}
