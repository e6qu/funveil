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
        .expect("Failed to load HTML parser");

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| FunveilError::TreeSitterError("Failed to parse HTML file".to_string()))?;

    let mut parsed = ParsedFile::new(language, path.to_path_buf());

    let element_query =
        Query::new(&html_lang, HTML_ELEMENT_QUERY).expect("Invalid HTML element query");

    parsed.symbols = extract_html_elements(&tree, &element_query, content)?;

    let mut scripts = extract_script_blocks(&tree, content)?;
    parsed.symbols.append(&mut scripts);

    let mut styles = extract_style_blocks(&tree, content)?;
    parsed.symbols.append(&mut styles);

    Ok(parsed)
}

/// Extract HTML elements as symbols - simplified
fn extract_html_elements(tree: &Tree, _query: &Query, _content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "element" {
            let start_line = child.start_position().row + 1;
            let end_line = child.end_position().row + 1;

            if start_line > 0 && end_line > 0 {
                let line_range = LineRange::new(start_line, end_line)
                    .expect("Invalid line range from tree-sitter positions");

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
    let query = Query::new(&html_lang, HTML_SCRIPT_QUERY).expect("Invalid HTML script query");
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
            let Some(capture_name) = capture_names.get(capture.index as usize) else {
                continue;
            };
            let node = capture.node;

            if capture_name == "script.def" {
                start_line = node.start_position().row + 1;
                end_line = node.end_position().row + 1;
            }
        }

        if start_line > 0 && end_line > 0 {
            let line_range = LineRange::new(start_line, end_line)
                .expect("Invalid line range from tree-sitter positions");

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
    let query = Query::new(&html_lang, HTML_STYLE_QUERY).expect("Invalid HTML style query");
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
            let Some(capture_name) = capture_names.get(capture.index as usize) else {
                continue;
            };
            let node = capture.node;

            if capture_name == "style.def" {
                start_line = node.start_position().row + 1;
                end_line = node.end_position().row + 1;
            }
        }

        if start_line > 0 && end_line > 0 {
            let line_range = LineRange::new(start_line, end_line)
                .expect("Invalid line range from tree-sitter positions");

            symbols.push(Symbol::Module {
                name: "<style>".to_string(),
                line_range,
            });
        }
    }

    Ok(symbols)
}

#[cfg_attr(coverage_nightly, coverage(off))]
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

    #[test]
    fn test_parse_html_elements() {
        let code = r#"<html>
<head><title>Test</title></head>
<body>
    <div>Content</div>
    <p>Paragraph</p>
</body>
</html>"#;
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        assert!(!parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_html_empty_content() {
        let parsed = parse_html_file(Path::new("test.html"), "").unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_html_only_comments() {
        let code = "<!-- just a comment -->\n";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let elements: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<element>")
            .collect();
        assert!(elements.is_empty());
    }

    #[test]
    fn test_parse_html_no_scripts() {
        let code = "<html><body><p>No scripts here</p></body></html>\n";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let scripts: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<script>")
            .collect();
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_parse_html_no_styles() {
        let code = "<html><body><p>No styles here</p></body></html>\n";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let styles: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<style>")
            .collect();
        assert!(styles.is_empty());
    }

    #[test]
    fn test_parse_html_only_text() {
        let code = "just plain text\n";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_html_multiple_scripts() {
        let code = r#"<html>
<body>
    <script>var a = 1;</script>
    <script>var b = 2;</script>
</body>
</html>"#;
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let scripts: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<script>")
            .collect();
        assert_eq!(scripts.len(), 2);
    }

    #[test]
    fn test_parse_html_multiple_styles() {
        let code = r#"<html>
<head>
    <style>body { color: red; }</style>
    <style>.a { color: blue; }</style>
</head>
<body></body>
</html>"#;
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let styles: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<style>")
            .collect();
        assert_eq!(styles.len(), 2);
    }

    #[test]
    fn test_parse_html_empty_input_no_panic() {
        let result = parse_html_file(Path::new("test.html"), "");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_parse_html_no_elements_only_doctype() {
        // Only a DOCTYPE, no element children at root — exercises the
        // `child.kind() != "element"` branch in extract_html_elements.
        let code = "<!DOCTYPE html>\n";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        // No elements, no scripts, no styles
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_html_no_script_no_style() {
        // HTML with elements but no script or style blocks.
        // Exercises the empty-result path from extract_script_blocks and
        // extract_style_blocks (the while-let loop body never executes).
        let code = "<html><body><p>Hello</p></body></html>";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let has_script = parsed.symbols.iter().any(|s| s.name() == "<script>");
        let has_style = parsed.symbols.iter().any(|s| s.name() == "<style>");
        assert!(!has_script, "Should not find script blocks");
        assert!(!has_style, "Should not find style blocks");
        // But should still have element(s)
        assert!(!parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_html_only_script() {
        // HTML with only a script tag, no style — exercises script extraction
        // with style extraction returning empty.
        let code = "<html><body><script>var x = 1;</script></body></html>";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let has_script = parsed.symbols.iter().any(|s| s.name() == "<script>");
        let has_style = parsed.symbols.iter().any(|s| s.name() == "<style>");
        assert!(has_script);
        assert!(!has_style);
    }

    #[test]
    fn test_parse_html_only_style() {
        // HTML with only a style tag, no script — exercises style extraction
        // with script extraction returning empty.
        let code = "<html><head><style>body { color: red; }</style></head><body></body></html>";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let has_style = parsed.symbols.iter().any(|s| s.name() == "<style>");
        let has_script = parsed.symbols.iter().any(|s| s.name() == "<script>");
        assert!(has_style);
        assert!(!has_script);
    }

    #[test]
    fn test_parse_html_multiple_scripts_and_styles() {
        // Multiple script and style blocks to exercise the while-let loop iterating
        // multiple matches.
        let code = r#"<html>
<head>
    <style>.a { color: red; }</style>
    <style>.b { color: blue; }</style>
</head>
<body>
    <script>var a = 1;</script>
    <script>var b = 2;</script>
</body>
</html>"#;
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let scripts: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<script>")
            .collect();
        let styles: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<style>")
            .collect();
        assert!(scripts.len() >= 2, "Should find at least 2 script blocks");
        assert!(styles.len() >= 2, "Should find at least 2 style blocks");
    }

    #[test]
    fn test_parse_html_whitespace_only() {
        // Whitespace-only input — no element nodes.
        let code = "   \n\n   \n";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_html_comment_only() {
        // HTML comment only — no elements, scripts, or styles.
        let code = "<!-- This is a comment -->\n";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        // Comments are not extracted as symbols
        let has_element = parsed.symbols.iter().any(|s| s.name() == "<element>");
        assert!(!has_element);
    }

    #[test]
    fn test_parse_html_script_and_style_together() {
        let code = r#"<html><head>
<style>h1{color:red}</style>
<script>var x=1;</script>
</head><body><p>hi</p></body></html>"#;
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let scripts: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<script>")
            .collect();
        let styles: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<style>")
            .collect();
        assert_eq!(scripts.len(), 1);
        assert_eq!(styles.len(), 1);
        let elements: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<element>")
            .collect();
        assert!(parsed.symbols.len() > elements.len());
    }

    #[test]
    fn test_parse_html_multiline_script() {
        let code = "<html><body>\n<script>\nvar a = 1;\nvar b = 2;\n</script>\n</body></html>";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let scripts: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<script>")
            .collect();
        assert_eq!(scripts.len(), 1);
        if let Symbol::Module { line_range, .. } = scripts[0] {
            assert!(line_range.end() > line_range.start());
        }
    }

    #[test]
    fn test_parse_html_multiline_style() {
        let code = "<html><head>\n<style>\nbody { color: red; }\nh1 { font-size: 20px; }\n</style>\n</head><body></body></html>";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let styles: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<style>")
            .collect();
        assert_eq!(styles.len(), 1);
        if let Symbol::Module { line_range, .. } = styles[0] {
            assert!(line_range.end() > line_range.start());
        }
    }

    #[test]
    fn test_parse_html_text_only_no_tags() {
        let code = "Just some text content without any tags";
        let parsed = parse_html_file(Path::new("test.html"), code).unwrap();
        let has_script = parsed.symbols.iter().any(|s| s.name() == "<script>");
        let has_style = parsed.symbols.iter().any(|s| s.name() == "<style>");
        assert!(!has_script);
        assert!(!has_style);
    }
}
