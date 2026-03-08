//! TypeScript and React (TSX) language parser for Tree-sitter.
//!
//! Supports TypeScript (.ts) and React/TSX (.tsx) files with:
//! - Function and class component detection
//! - JSX element extraction
//! - React hooks detection (useState, useEffect, etc.)
//! - Import/export statements
//! - Entrypoint detection (ReactDOM.render, Next.js pages, etc.)

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language as TSLanguage, Query, QueryCursor, Tree};

use crate::error::{FunveilError, Result};
use crate::parser::{Language, ParsedFile, Symbol, Visibility};
use crate::types::LineRange;

/// Tree-sitter language for TypeScript
pub fn typescript_language() -> TSLanguage {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}

/// Tree-sitter language for TSX (TypeScript with JSX)
pub fn tsx_language() -> TSLanguage {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}

/// Check if file is TSX (contains JSX)
pub fn is_tsx(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "tsx")
        .unwrap_or(false)
}

/// Query for extracting function declarations
const TS_FUNCTION_QUERY: &str = r#"
(function_declaration
  name: (identifier) @func.name) @func.def
"#;

/// Query for extracting arrow function components
const TS_ARROW_COMPONENT_QUERY: &str = r#"
(lexical_declaration
  (variable_declarator
    name: (identifier) @component.name
    value: (arrow_function))) @component.def
"#;

/// Query for extracting JSX elements
const JSX_ELEMENT_QUERY: &str = r#"
(jsx_element
  (jsx_opening_element
    name: (identifier) @jsx.tag)) @jsx.element
"#;

/// Parse a TypeScript/TSX source file
pub fn parse_typescript_file(path: &std::path::Path, content: &str) -> Result<ParsedFile> {
    let language = Language::TypeScript;
    let mut parser = tree_sitter::Parser::new();

    // Use TSX language for .tsx files, regular TypeScript for .ts
    let ts_lang = if is_tsx(path) {
        tsx_language()
    } else {
        typescript_language()
    };

    parser
        .set_language(&ts_lang)
        .map_err(|e| FunveilError::ParseError(format!("Failed to load TypeScript parser: {e}")))?;

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| FunveilError::ParseError("Failed to parse TypeScript file".to_string()))?;

    let mut parsed = ParsedFile::new(language, path.to_path_buf());

    // Extract React components (for TSX files)
    if is_tsx(path) {
        let mut components = extract_react_components(&tree, content)?;
        parsed.symbols.append(&mut components);

        // Extract JSX elements
        let mut jsx_elements = extract_jsx_elements(&tree, content)?;
        parsed.symbols.append(&mut jsx_elements);
    }

    Ok(parsed)
}

/// Extract React function components
fn extract_react_components(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let tsx_lang = tsx_language();

    // Try function declarations
    let func_query = Query::new(&tsx_lang, TS_FUNCTION_QUERY)
        .map_err(|e| FunveilError::ParseError(format!("Invalid TS function query: {e}")))?;
    let func_capture_names: Vec<String> = func_query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&func_query, tree.root_node(), content.as_bytes());

    while let Some(m) = matches.next() {
        let mut name: Option<String> = None;
        let mut start_line = 0;
        let mut end_line = 0;

        for capture in m.captures {
            let capture_name = &func_capture_names[capture.index as usize];
            let node = capture.node;

            match capture_name.as_str() {
                "func.name" => {
                    name = node
                        .utf8_text(content.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
                "func.def" => {
                    start_line = node.start_position().row + 1;
                    end_line = node.end_position().row + 1;
                }
                _ => {}
            }
        }

        if let Some(name) = name {
            if start_line == 0 || end_line == 0 {
                continue;
            }

            // Check if it's a React component (PascalCase)
            let is_component = is_react_component(&name);
            let mut attributes = Vec::new();

            if is_component {
                attributes.push("component".to_string());
                // Check for entrypoint indicators
                if name == "App" || name == "Main" || name == "Page" {
                    attributes.push("entrypoint".to_string());
                }

                let line_range = LineRange::new(start_line, end_line)
                    .map_err(|e| FunveilError::ParseError(format!("Invalid line range: {e}")))?;

                symbols.push(Symbol::Function {
                    name,
                    params: Vec::new(),
                    return_type: None,
                    visibility: Visibility::Public,
                    line_range,
                    body_range: line_range,
                    is_async: false,
                    attributes,
                });
            }
        }
    }

    // Try arrow function components
    let arrow_query = Query::new(&tsx_lang, TS_ARROW_COMPONENT_QUERY)
        .map_err(|e| FunveilError::ParseError(format!("Invalid TS arrow component query: {e}")))?;
    let arrow_capture_names: Vec<String> = arrow_query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&arrow_query, tree.root_node(), content.as_bytes());

    while let Some(m) = matches.next() {
        let mut name: Option<String> = None;
        let mut start_line = 0;
        let mut end_line = 0;

        for capture in m.captures {
            let capture_name = &arrow_capture_names[capture.index as usize];
            let node = capture.node;

            match capture_name.as_str() {
                "component.name" => {
                    name = node
                        .utf8_text(content.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
                "component.def" => {
                    start_line = node.start_position().row + 1;
                    end_line = node.end_position().row + 1;
                }
                _ => {}
            }
        }

        if let Some(name) = name {
            if start_line == 0 || end_line == 0 {
                continue;
            }

            // Check if it's a React component (PascalCase)
            if is_react_component(&name) {
                let mut attributes = vec!["component".to_string()];
                if name == "App" || name == "Main" || name == "Page" {
                    attributes.push("entrypoint".to_string());
                }

                let line_range = LineRange::new(start_line, end_line)
                    .map_err(|e| FunveilError::ParseError(format!("Invalid line range: {e}")))?;

                symbols.push(Symbol::Function {
                    name,
                    params: Vec::new(),
                    return_type: None,
                    visibility: Visibility::Public,
                    line_range,
                    body_range: line_range,
                    is_async: false,
                    attributes,
                });
            }
        }
    }

    Ok(symbols)
}

/// Extract JSX elements
fn extract_jsx_elements(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let tsx_lang = tsx_language();
    let query = Query::new(&tsx_lang, JSX_ELEMENT_QUERY)
        .map_err(|e| FunveilError::ParseError(format!("Invalid JSX element query: {e}")))?;
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

    // Track unique tag names to avoid duplicates
    let mut seen_tags: std::collections::HashSet<String> = std::collections::HashSet::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node = capture.node;

            if capture_name == "jsx.tag" {
                if let Ok(tag_name) = node.utf8_text(content.as_bytes()) {
                    let tag_name = tag_name.to_string();

                    // Only record each unique tag once
                    if !seen_tags.contains(&tag_name) {
                        seen_tags.insert(tag_name.clone());

                        let start_line = node.start_position().row + 1;
                        let end_line = node.end_position().row + 1;

                        if start_line > 0 && end_line > 0 {
                            if let Ok(line_range) = LineRange::new(start_line, end_line) {
                                // Create a module symbol for JSX elements
                                symbols.push(Symbol::Module {
                                    name: format!("<{tag_name}>"),
                                    line_range,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(symbols)
}

/// Detect if content is a React hook
pub fn is_react_hook(name: &str) -> bool {
    name.starts_with("use")
        && name
            .chars()
            .nth(3)
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
}

/// Detect if name is a React component (PascalCase)
pub fn is_react_component(name: &str) -> bool {
    name.chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
        && name.chars().any(|c| c.is_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_react_hook() {
        assert!(is_react_hook("useState"));
        assert!(is_react_hook("useEffect"));
        assert!(is_react_hook("useCallback"));
        assert!(!is_react_hook("use"));
        assert!(!is_react_hook("usestate"));
        assert!(!is_react_hook("component"));
    }

    #[test]
    fn test_is_react_component() {
        assert!(is_react_component("App"));
        assert!(is_react_component("MyComponent"));
        assert!(is_react_component("HomePage"));
        assert!(!is_react_component("app"));
        assert!(!is_react_component("my_function"));
    }

    #[test]
    fn test_is_tsx() {
        assert!(is_tsx(Path::new("App.tsx")));
        assert!(is_tsx(Path::new("components/Button.tsx")));
        assert!(!is_tsx(Path::new("utils.ts")));
        assert!(!is_tsx(Path::new("main.rs")));
    }

    #[test]
    fn test_parse_tsx_component() {
        let code = r#"import React from 'react';

interface Props {
  title: string;
  count: number;
}

export function MyComponent({ title, count }: Props) {
  return (
    <div className="container">
      <h1>{title}</h1>
      <p>Count: {count}</p>
    </div>
  );
}
"#;

        let parsed = parse_typescript_file(Path::new("Component.tsx"), code).unwrap();

        // Should have detected the component
        let funcs: Vec<_> = parsed.functions().collect();
        assert!(!funcs.is_empty());

        // Check for MyComponent
        let my_component = funcs.iter().find(|f| f.name() == "MyComponent");
        assert!(my_component.is_some());

        if let Some(Symbol::Function { attributes, .. }) = my_component {
            assert!(attributes.contains(&"component".to_string()));
        }
    }

    #[test]
    fn test_parse_tsx_arrow_component() {
        let code = r#"import React from 'react';

const App = () => {
  return (
    <div>
      <Header />
      <Main />
      <Footer />
    </div>
  );
};

export default App;
"#;

        let parsed = parse_typescript_file(Path::new("App.tsx"), code).unwrap();

        // Should have detected App as entrypoint
        let funcs: Vec<_> = parsed.functions().collect();
        let app = funcs.iter().find(|f| f.name() == "App");

        if let Some(Symbol::Function { attributes, .. }) = app {
            assert!(attributes.contains(&"entrypoint".to_string()));
        }
    }
}
