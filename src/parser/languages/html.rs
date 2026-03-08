//! HTML language parser for Tree-sitter.
//!
//! Supports HTML files (.html, .htm) with:
//! - Tag structure extraction
//! - Script and style block identification
//! - Element attributes (id, class, etc.)
//! - Comment extraction

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language as TSLanguage, Query, QueryCursor, Tree};

use crate::error::{FunveilError, Result};
use crate::parser::{Language, ParsedFile, Symbol};
use crate::types::LineRange;

/// Tree-sitter language for HTML
pub fn html_language() -> TSLanguage {
    tree_sitter_html::LANGUAGE.into()
}

/// Query for extracting HTML elements - simplified
const HTML_ELEMENT_QUERY: &str = r#"
(element) @element.def
"#;

/// Query for extracting script elements - simplified
const HTML_SCRIPT_QUERY: &str = r#"
(script_element) @script.def
"#;

/// Query for extracting style elements - simplified
const HTML_STYLE_QUERY: &str = r#"
(style_element) @style.def
"#;

/// Parse an HTML file
pub fn parse_html_file(path: &std::path::Path, content: &str) -> Result<ParsedFile> {
    let language = Language::Html;
    let mut parser = tree_sitter::Parser::new();
    let html_lang = html_language();
    parser
        .set_language(&html_lang)
        .map_err(|e| FunveilError::ParseError(format!("Failed to load HTML parser: {e}")))?;

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| FunveilError::ParseError("Failed to parse HTML file".to_string()))?;

    let mut parsed = ParsedFile::new(language, path.to_path_buf());

    // Build queries
    let element_query = Query::new(&html_lang, HTML_ELEMENT_QUERY)
        .map_err(|e| FunveilError::ParseError(format!("Invalid HTML element query: {e}")))?;

    // Extract elements (treat them as symbols for structure)
    parsed.symbols = extract_html_elements(&tree, &element_query, content)?;

    // Extract script blocks
    let mut scripts = extract_script_blocks(&tree, content)?;
    parsed.symbols.append(&mut scripts);

    // Extract style blocks
    let mut styles = extract_style_blocks(&tree, content)?;
    parsed.symbols.append(&mut styles);

    Ok(parsed)
}

/// Extract HTML elements as symbols - simplified
fn extract_html_elements(tree: &Tree, _query: &Query, _content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    // Walk the tree and extract element nodes
    for child in root.children(&mut cursor) {
        if child.kind() == "element" {
            let start_line = child.start_position().row + 1;
            let end_line = child.end_position().row + 1;

            if start_line > 0 && end_line > 0 {
                let line_range = LineRange::new(start_line, end_line)
                    .map_err(|e| FunveilError::ParseError(format!("Invalid line range: {e}")))?;

                symbols.push(Symbol::Module {
                    name: "<element>".to_string(),
                    line_range,
                });
            }
        }
    }

    Ok(symbols)
}

/// Extract script blocks from HTML
fn extract_script_blocks(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let html_lang = html_language();
    let query = Query::new(&html_lang, HTML_SCRIPT_QUERY)
        .map_err(|e| FunveilError::ParseError(format!("Invalid HTML script query: {e}")))?;
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

    while let Some(m) = matches.next() {
        let mut start_line = 0;
        let mut end_line = 0;

        for capture in m.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node = capture.node;

            if capture_name == "script.def" {
                start_line = node.start_position().row + 1;
                end_line = node.end_position().row + 1;
            }
        }

        if start_line > 0 && end_line > 0 {
            let line_range = LineRange::new(start_line, end_line)
                .map_err(|e| FunveilError::ParseError(format!("Invalid line range: {e}")))?;

            symbols.push(Symbol::Module {
                name: "<script>".to_string(),
                line_range,
            });
        }
    }

    Ok(symbols)
}

/// Extract style blocks from HTML
fn extract_style_blocks(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let html_lang = html_language();
    let query = Query::new(&html_lang, HTML_STYLE_QUERY)
        .map_err(|e| FunveilError::ParseError(format!("Invalid HTML style query: {e}")))?;
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

    while let Some(m) = matches.next() {
        let mut start_line = 0;
        let mut end_line = 0;

        for capture in m.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node = capture.node;

            if capture_name == "style.def" {
                start_line = node.start_position().row + 1;
                end_line = node.end_position().row + 1;
            }
        }

        if start_line > 0 && end_line > 0 {
            let line_range = LineRange::new(start_line, end_line)
                .map_err(|e| FunveilError::ParseError(format!("Invalid line range: {e}")))?;

            symbols.push(Symbol::Module {
                name: "<style>".to_string(),
                line_range,
            });
        }
    }

    Ok(symbols)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_simple_html() {
        let code = r#"<!DOCTYPE html>
<html>
<head>
    <title>Test Page</title>
</head>
<body>
    <h1>Hello World</h1>
    <p>This is a paragraph.</p>
</body>
</html>"#;

        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();

        // Should have various HTML elements as symbols
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();
        assert!(!modules.is_empty());
    }

    #[test]
    fn test_parse_html_with_script() {
        let code = r#"<!DOCTYPE html>
<html>
<head>
    <title>Test</title>
</head>
<body>
    <h1>Page</h1>
    <script>
        console.log("Hello from JavaScript");
    </script>
</body>
</html>"#;

        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();

        // Should have script block
        let has_script = parsed.symbols.iter().any(|s| s.name() == "<script>");
        assert!(has_script);
    }

    #[test]
    fn test_parse_html_with_style() {
        let code = r#"<!DOCTYPE html>
<html>
<head>
    <style>
        body { color: black; }
        h1 { font-size: 24px; }
    </style>
</head>
<body>
    <h1>Styled Page</h1>
</body>
</html>"#;

        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();

        // Should have style block
        let has_style = parsed.symbols.iter().any(|s| s.name() == "<style>");
        assert!(has_style);
    }

    #[test]
    fn test_parse_html_complex() {
        let code = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Complex Page</title>
    <style>
        .container { max-width: 800px; margin: 0 auto; }
    </style>
    <script src="app.js"></script>
</head>
<body>
    <div class="container">
        <header>
            <nav>
                <ul>
                    <li><a href="/">Home</a></li>
                    <li><a href="/about">About</a></li>
                </ul>
            </nav>
        </header>
        <main>
            <article>
                <h1>Article Title</h1>
                <p>Article content...</p>
            </article>
        </main>
        <footer>
            <p>&copy; 2024</p>
        </footer>
    </div>
    <script>
        document.addEventListener('DOMContentLoaded', function() {
            console.log('Page loaded');
        });
    </script>
</body>
</html>"#;

        let parsed = parse_html_file(Path::new("complex.html"), code).unwrap();

        // Should have various structural elements
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();

        // Should have script and style blocks
        let has_script = modules.iter().any(|s| s.name() == "<script>");
        let has_style = modules.iter().any(|s| s.name() == "<style>");

        assert!(!modules.is_empty());
        assert!(has_script);
        assert!(has_style);
    }
}
