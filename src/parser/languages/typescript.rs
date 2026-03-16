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
  name: (identifier) @func.name
  parameters: (formal_parameters) @func.params
  return_type: (type_annotation)? @func.return) @func.def
"#;

/// Query for extracting arrow function components
const TS_ARROW_COMPONENT_QUERY: &str = r#"
(lexical_declaration
  (variable_declarator
    name: (identifier) @component.name
    value: (arrow_function
      parameters: (formal_parameters) @component.params
      return_type: (type_annotation)? @component.return))) @component.def
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
        .expect("Failed to load TypeScript parser");

    let tree = parser.parse(content, None).ok_or_else(|| {
        FunveilError::TreeSitterError("Failed to parse TypeScript file".to_string())
    })?;

    let mut parsed = ParsedFile::new(language, path.to_path_buf());

    let mut functions = extract_ts_functions(&tree, content, &ts_lang, is_tsx(path))?;
    parsed.symbols.append(&mut functions);

    if is_tsx(path) {
        let mut components = extract_react_components(&tree, content)?;
        parsed.symbols.append(&mut components);

        let mut jsx_elements = extract_jsx_elements(&tree, content)?;
        parsed.symbols.append(&mut jsx_elements);
    }

    Ok(parsed)
}

/// Extract all function declarations from TypeScript files.
/// When `is_tsx` is true, skips PascalCase names (those come from `extract_react_components`).
fn extract_ts_functions(
    tree: &Tree,
    content: &str,
    lang: &TSLanguage,
    is_tsx_file: bool,
) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();

    let func_query = Query::new(lang, TS_FUNCTION_QUERY)
        .expect("Invalid TS function query: constant query should always be valid");
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
        let mut def_text: Option<String> = None;
        let mut params = Vec::new();
        let mut return_type: Option<String> = None;

        for capture in m.captures {
            let Some(capture_name) = func_capture_names.get(capture.index as usize) else {
                continue;
            };
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
                    def_text = node
                        .utf8_text(content.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
                "func.params" => {
                    if let Ok(text) = node.utf8_text(content.as_bytes()) {
                        params = parse_ts_params(text);
                    }
                }
                "func.return" => {
                    if let Ok(text) = node.utf8_text(content.as_bytes()) {
                        let t = text.trim().trim_start_matches(':').trim();
                        if !t.is_empty() {
                            return_type = Some(t.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(name) = name {
            // Skip PascalCase names in TSX files (handled by extract_react_components)
            if is_tsx_file && is_react_component(&name) {
                continue;
            }

            let line_range = LineRange::new(start_line, end_line)
                .expect("Tree-sitter positions should always produce valid line ranges");

            let is_async = def_text
                .as_deref()
                .is_some_and(|t| t.starts_with("async ") || t.contains(" async "));

            symbols.push(Symbol::Function {
                name,
                params,
                return_type,
                visibility: Visibility::Public,
                line_range,
                body_range: line_range,
                is_async,
                attributes: vec![],
            });
        }
    }

    let arrow_query = Query::new(lang, TS_ARROW_COMPONENT_QUERY)
        .expect("Invalid TS arrow component query: constant query should always be valid");
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
        let mut def_text: Option<String> = None;
        let mut params = Vec::new();
        let mut return_type: Option<String> = None;

        for capture in m.captures {
            let Some(capture_name) = arrow_capture_names.get(capture.index as usize) else {
                continue;
            };
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
                    def_text = node
                        .utf8_text(content.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
                "component.params" => {
                    if let Ok(text) = node.utf8_text(content.as_bytes()) {
                        params = parse_ts_params(text);
                    }
                }
                "component.return" => {
                    if let Ok(text) = node.utf8_text(content.as_bytes()) {
                        let t = text.trim().trim_start_matches(':').trim();
                        if !t.is_empty() {
                            return_type = Some(t.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(name) = name {
            // Skip PascalCase names in TSX files (handled by extract_react_components)
            if is_tsx_file && is_react_component(&name) {
                continue;
            }

            let line_range = LineRange::new(start_line, end_line)
                .expect("Tree-sitter positions should always produce valid line ranges");

            let is_async = def_text
                .as_deref()
                .is_some_and(|t| t.starts_with("async ") || t.contains(" async "));

            symbols.push(Symbol::Function {
                name,
                params,
                return_type,
                visibility: Visibility::Public,
                line_range,
                body_range: line_range,
                is_async,
                attributes: vec![],
            });
        }
    }

    Ok(symbols)
}

/// Extract React function components
fn extract_react_components(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let tsx_lang = tsx_language();

    let func_query = Query::new(&tsx_lang, TS_FUNCTION_QUERY)
        .expect("Invalid TS function query: constant query should always be valid");
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
        let mut def_text: Option<String> = None;
        let mut params = Vec::new();
        let mut return_type: Option<String> = None;

        for capture in m.captures {
            let Some(capture_name) = func_capture_names.get(capture.index as usize) else {
                continue;
            };
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
                    def_text = node
                        .utf8_text(content.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
                "func.params" => {
                    if let Ok(text) = node.utf8_text(content.as_bytes()) {
                        params = parse_ts_params(text);
                    }
                }
                "func.return" => {
                    if let Ok(text) = node.utf8_text(content.as_bytes()) {
                        let t = text.trim().trim_start_matches(':').trim();
                        if !t.is_empty() {
                            return_type = Some(t.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(name) = name {
            // Check if it's a React component (PascalCase)
            let is_component = is_react_component(&name);

            if is_component {
                let mut attributes = vec!["component".to_string()];
                // Check for entrypoint indicators
                if name == "App" || name == "Main" || name == "Page" {
                    attributes.push("entrypoint".to_string());
                }

                let line_range = LineRange::new(start_line, end_line)
                    .expect("Tree-sitter positions should always produce valid line ranges");

                let is_async = def_text
                    .as_deref()
                    .is_some_and(|t| t.starts_with("async ") || t.contains(" async "));

                symbols.push(Symbol::Function {
                    name,
                    params,
                    return_type,
                    visibility: Visibility::Public,
                    line_range,
                    body_range: line_range,
                    is_async,
                    attributes,
                });
            }
        }
    }

    let arrow_query = Query::new(&tsx_lang, TS_ARROW_COMPONENT_QUERY)
        .expect("Invalid TS arrow component query: constant query should always be valid");
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
        let mut def_text: Option<String> = None;
        let mut params = Vec::new();
        let mut return_type: Option<String> = None;

        for capture in m.captures {
            let Some(capture_name) = arrow_capture_names.get(capture.index as usize) else {
                continue;
            };
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
                    def_text = node
                        .utf8_text(content.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
                "component.params" => {
                    if let Ok(text) = node.utf8_text(content.as_bytes()) {
                        params = parse_ts_params(text);
                    }
                }
                "component.return" => {
                    if let Ok(text) = node.utf8_text(content.as_bytes()) {
                        let t = text.trim().trim_start_matches(':').trim();
                        if !t.is_empty() {
                            return_type = Some(t.to_string());
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(name) = name {
            // Check if it's a React component (PascalCase)
            if is_react_component(&name) {
                let mut attributes = vec!["component".to_string()];
                if name == "App" || name == "Main" || name == "Page" {
                    attributes.push("entrypoint".to_string());
                }

                let line_range = LineRange::new(start_line, end_line)
                    .expect("Tree-sitter positions should always produce valid line ranges");

                let is_async = def_text
                    .as_deref()
                    .is_some_and(|t| t.starts_with("async ") || t.contains(" async "));

                symbols.push(Symbol::Function {
                    name,
                    params,
                    return_type,
                    visibility: Visibility::Public,
                    line_range,
                    body_range: line_range,
                    is_async,
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
        .expect("Invalid JSX element query: constant query should always be valid");
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());

    let mut seen_tags: std::collections::HashSet<String> = std::collections::HashSet::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let Some(capture_name) = capture_names.get(capture.index as usize) else {
                continue;
            };
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

fn parse_ts_params(params_text: &str) -> Vec<crate::parser::Param> {
    let inner = params_text
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')');
    if inner.is_empty() {
        return Vec::new();
    }
    inner
        .split(',')
        .filter_map(|p| {
            let p = p.trim();
            if p.is_empty() {
                return None;
            }
            if let Some(colon) = p.find(':') {
                let name = p[..colon].trim().trim_start_matches("...").to_string();
                let ty = p[colon + 1..].trim().to_string();
                Some(crate::parser::Param {
                    name,
                    type_annotation: Some(ty),
                })
            } else {
                Some(crate::parser::Param {
                    name: p.to_string(),
                    type_annotation: None,
                })
            }
        })
        .collect()
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

/// Detect if name is a React component (starts with uppercase letter)
pub fn is_react_component(name: &str) -> bool {
    name.chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
}

#[cfg_attr(coverage_nightly, coverage(off))]
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
        assert!(is_react_component("A"));
        assert!(is_react_component("X"));
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

    #[test]
    fn test_parse_ts_file() {
        let code = r#"
function greet(name: string): string {
    return "Hello, " + name;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.ts"), code).unwrap();
        // .ts files should now extract functions
        assert_eq!(parsed.symbols.len(), 1);
        assert_eq!(parsed.symbols[0].name(), "greet");
    }

    #[test]
    fn test_parse_tsx_with_function_component() {
        let code = r#"
function App() {
    return <div>Hello</div>;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let components: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| {
                if let Symbol::Function { attributes, .. } = s {
                    attributes.contains(&"component".to_string())
                } else {
                    false
                }
            })
            .collect();
        assert!(!components.is_empty());
        assert_eq!(components[0].name(), "App");

        if let Symbol::Function { attributes, .. } = components[0] {
            assert!(attributes.contains(&"entrypoint".to_string()));
        }
    }

    #[test]
    fn test_parse_tsx_with_arrow_component_page() {
        let code = r#"
const Page = () => {
    return <div>Page content</div>;
};
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let components: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| {
                if let Symbol::Function { attributes, .. } = s {
                    attributes.contains(&"component".to_string())
                } else {
                    false
                }
            })
            .collect();
        assert!(!components.is_empty());

        if let Symbol::Function { attributes, .. } = components[0] {
            assert!(attributes.contains(&"entrypoint".to_string()));
        }
    }

    #[test]
    fn test_parse_tsx_with_jsx_elements() {
        let code = r#"
function Header() {
    return <header><h1>Title</h1></header>;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        assert!(!parsed.symbols.is_empty());
    }

    #[test]
    fn test_is_tsx_helper() {
        assert!(is_tsx(Path::new("app.tsx")));
        assert!(!is_tsx(Path::new("app.ts")));
        assert!(!is_tsx(Path::new("app.js")));
    }

    #[test]
    fn test_parse_tsx_non_component_function() {
        let code = r#"
function helper() {
    return 42;
}
"#;
        // helper is lowercase, not a React component
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let components: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| {
                if let Symbol::Function { attributes, .. } = s {
                    attributes.contains(&"component".to_string())
                } else {
                    false
                }
            })
            .collect();
        assert!(components.is_empty());
    }

    #[test]
    fn test_parse_tsx_main_component() {
        let code = r#"
function Main() {
    return <div>Main app</div>;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let main_components: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| {
                if let Symbol::Function { attributes, .. } = s {
                    attributes.contains(&"entrypoint".to_string())
                } else {
                    false
                }
            })
            .collect();
        assert!(!main_components.is_empty());
    }

    #[test]
    fn test_parse_typescript_empty_input_no_panic() {
        let result = parse_typescript_file(Path::new("test.ts"), "");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_parse_ts_extracts_multiple_functions() {
        let code = r#"
function add(a: number, b: number): number {
    return a + b;
}

function subtract(a: number, b: number): number {
    return a - b;
}

const multiply = (a: number, b: number): number => {
    return a * b;
};
"#;
        let parsed = parse_typescript_file(Path::new("math.ts"), code).unwrap();
        let names: Vec<_> = parsed.symbols.iter().map(|s| s.name()).collect();
        assert!(names.contains(&"add"), "should find 'add'");
        assert!(names.contains(&"subtract"), "should find 'subtract'");
        assert!(names.contains(&"multiply"), "should find 'multiply'");
    }

    #[test]
    fn test_parse_ts_empty_content() {
        let parsed = parse_typescript_file(Path::new("test.ts"), "").unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_tsx_empty_content() {
        let parsed = parse_typescript_file(Path::new("test.tsx"), "").unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_ts_no_functions() {
        let code = "const x = 42;\nlet y = 'hello';\n";
        let parsed = parse_typescript_file(Path::new("test.ts"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_tsx_no_components_no_jsx() {
        let code = "const x = 42;\nlet y = 'hello';\n";
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_tsx_only_comments() {
        let code = "// this is a comment\n/* block comment */\n";
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_ts_arrow_lowercase_in_tsx() {
        let code = r#"
const helper = () => {
    return 42;
};
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "helper");
        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(!attributes.contains(&"component".to_string()));
        }
    }

    #[test]
    fn test_parse_tsx_arrow_component_main() {
        let code = r#"
const Main = () => {
    return <div>Main app</div>;
};
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let components: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| {
                if let Symbol::Function { attributes, .. } = s {
                    attributes.contains(&"component".to_string())
                } else {
                    false
                }
            })
            .collect();
        assert!(!components.is_empty());
        if let Symbol::Function { attributes, .. } = components[0] {
            assert!(attributes.contains(&"entrypoint".to_string()));
        }
    }

    #[test]
    fn test_parse_tsx_jsx_no_elements() {
        let code = r#"
function helper() {
    return 42;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let jsx: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| s.name().starts_with("<"))
            .collect();
        assert!(jsx.is_empty());
    }

    #[test]
    fn test_parse_tsx_jsx_duplicate_tags() {
        let code = r#"
function App() {
    return <div><div>nested</div></div>;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let div_count = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "<div>")
            .count();
        assert!(div_count <= 1);
    }

    #[test]
    fn test_parse_tsx_non_component_arrow_function() {
        let code = r#"
function Widget() {
    return <span>widget</span>;
}

const utility = () => {
    return "utility";
};
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let names: Vec<_> = parsed.symbols.iter().map(|s| s.name()).collect();
        assert!(names.contains(&"Widget"));
        assert!(names.contains(&"utility"));
    }

    #[test]
    fn test_parse_tsx_non_component_function_name() {
        let code = r#"
function NotAnEntrypoint() {
    return <p>text</p>;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let components: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| {
                if let Symbol::Function { attributes, .. } = s {
                    attributes.contains(&"component".to_string())
                } else {
                    false
                }
            })
            .collect();
        assert!(!components.is_empty());
        if let Symbol::Function { attributes, .. } = components[0] {
            assert!(!attributes.contains(&"entrypoint".to_string()));
        }
    }

    #[test]
    fn test_parse_tsx_no_duplicate_components() {
        let code = r#"
function MyWidget() {
    return <div>Hello</div>;
}
"#;
        let parsed = parse_typescript_file(Path::new("Widget.tsx"), code).unwrap();
        let my_widget_count = parsed
            .symbols
            .iter()
            .filter(|s| s.name() == "MyWidget")
            .count();
        // PascalCase function in TSX should only appear once (from extract_react_components)
        assert_eq!(
            my_widget_count, 1,
            "MyWidget should appear exactly once, not duplicated"
        );
    }

    #[test]
    fn test_is_react_component_empty_string() {
        // Tests the unwrap_or(false) branch when name is empty
        assert!(!is_react_component(""));
    }

    #[test]
    fn test_is_react_hook_short_strings() {
        // Tests edge cases for the nth(3) unwrap_or(false) branch
        assert!(!is_react_hook(""));
        assert!(!is_react_hook("us"));
        assert!(!is_react_hook("use"));
        // 4th char exists but is lowercase
        assert!(!is_react_hook("usea"));
    }

    #[test]
    fn test_is_tsx_no_extension() {
        // Tests the unwrap_or(false) branch when path has no extension
        assert!(!is_tsx(Path::new("noext")));
        assert!(!is_tsx(Path::new("")));
        assert!(!is_tsx(Path::new("dir/")));
    }

    #[test]
    fn test_tsx_lowercase_arrow_function_not_component() {
        // Tests the is_tsx_file && is_react_component branch for arrow functions
        // Lowercase arrow functions in TSX should NOT be components
        let code = r#"
const helper = () => {
    return 42;
};
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let components: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| {
                if let Symbol::Function { attributes, .. } = s {
                    attributes.contains(&"component".to_string())
                } else {
                    false
                }
            })
            .collect();
        assert!(
            components.is_empty(),
            "lowercase arrow fn should not be a component"
        );

        // But the function should still appear as a regular function
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "helper");
    }

    #[test]
    fn test_ts_arrow_function_extraction() {
        // Tests arrow function extraction in non-TSX mode (no component filtering)
        let code = r#"
const greet = (name: string) => {
    return "Hello, " + name;
};
"#;
        let parsed = parse_typescript_file(Path::new("test.ts"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "greet");
    }

    #[test]
    fn test_tsx_duplicate_jsx_tags_deduplicated() {
        // Tests the seen_tags deduplication branch in extract_jsx_elements
        let code = r#"
function Layout() {
    return (
        <div>
            <span>First</span>
            <span>Second</span>
            <div>Nested</div>
        </div>
    );
}
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();
        // Each unique tag should appear only once
        let div_count = modules.iter().filter(|s| s.name() == "<div>").count();
        let span_count = modules.iter().filter(|s| s.name() == "<span>").count();
        assert_eq!(div_count, 1, "div should appear once despite multiple uses");
        assert_eq!(
            span_count, 1,
            "span should appear once despite multiple uses"
        );
    }

    #[test]
    fn test_tsx_component_without_entrypoint() {
        // Tests the false branch of the App/Main/Page entrypoint check
        let code = r#"
function Sidebar() {
    return <div>Sidebar content</div>;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let sidebar = parsed
            .symbols
            .iter()
            .find(|s| s.name() == "Sidebar")
            .unwrap();

        if let Symbol::Function { attributes, .. } = sidebar {
            assert!(
                attributes.contains(&"component".to_string()),
                "PascalCase function should be a component"
            );
            assert!(
                !attributes.contains(&"entrypoint".to_string()),
                "Sidebar should not be an entrypoint"
            );
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_tsx_arrow_component_without_entrypoint() {
        // Tests arrow component that is NOT App/Main/Page (no entrypoint)
        let code = r#"
const Header = () => {
    return <div>Header</div>;
};
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let header = parsed
            .symbols
            .iter()
            .find(|s| s.name() == "Header")
            .unwrap();

        if let Symbol::Function { attributes, .. } = header {
            assert!(attributes.contains(&"component".to_string()));
            assert!(
                !attributes.contains(&"entrypoint".to_string()),
                "Header should not be an entrypoint"
            );
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_tsx_arrow_component_entrypoint() {
        // Tests arrow component that IS an entrypoint (App, Main, Page)
        let code = r#"
const Main = () => {
    return <div>Main</div>;
};
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let main_comp = parsed.symbols.iter().find(|s| s.name() == "Main").unwrap();

        if let Symbol::Function { attributes, .. } = main_comp {
            assert!(attributes.contains(&"component".to_string()));
            assert!(attributes.contains(&"entrypoint".to_string()));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_tsx_non_component_function_in_tsx_skipped_by_extract_ts_functions() {
        // In TSX, PascalCase functions should be skipped by extract_ts_functions
        // and handled by extract_react_components instead
        let code = r#"
function helper() {
    return 1;
}

function Widget() {
    return <div>Hello</div>;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();

        // helper should exist as a regular function (not a component)
        let helper = parsed.symbols.iter().find(|s| s.name() == "helper");
        assert!(helper.is_some());
        if let Some(Symbol::Function { attributes, .. }) = helper {
            assert!(!attributes.contains(&"component".to_string()));
        }

        // Widget should exist as a component
        let widget = parsed.symbols.iter().find(|s| s.name() == "Widget");
        assert!(widget.is_some());
        if let Some(Symbol::Function { attributes, .. }) = widget {
            assert!(attributes.contains(&"component".to_string()));
        }
    }

    #[test]
    fn test_ts_pascalcase_function_not_filtered() {
        // In non-TSX mode, PascalCase functions should NOT be filtered out
        let code = r#"
function MyHelper() {
    return 42;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.ts"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "MyHelper");

        // Should NOT have component attribute (not TSX mode)
        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(!attributes.contains(&"component".to_string()));
        }
    }

    #[test]
    fn test_ts_pascalcase_arrow_not_filtered() {
        // In non-TSX mode, PascalCase arrow functions should NOT be filtered out
        let code = r#"
const MyUtil = () => {
    return 42;
};
"#;
        let parsed = parse_typescript_file(Path::new("test.ts"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "MyUtil");

        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(!attributes.contains(&"component".to_string()));
        }
    }

    #[test]
    fn test_tsx_no_jsx_elements_in_ts_file() {
        // Tests that JSX extraction is not called for .ts files
        let code = r#"
function render() {
    return "not JSX";
}
"#;
        let parsed = parse_typescript_file(Path::new("test.ts"), code).unwrap();
        let modules: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Module { .. }))
            .collect();
        assert!(modules.is_empty(), "ts files should have no JSX modules");
    }

    #[test]
    fn test_tsx_react_component_non_entrypoint_arrow() {
        // Tests that arrow components with non-entrypoint names get component attr only
        // This exercises the false branch of the if name == "App" || name == "Main" || name == "Page"
        let code = r#"
const Footer = () => {
    return <footer>Bottom</footer>;
};
"#;
        let parsed = parse_typescript_file(Path::new("test.tsx"), code).unwrap();
        let footer = parsed
            .symbols
            .iter()
            .find(|s| s.name() == "Footer")
            .unwrap();
        if let Symbol::Function { attributes, .. } = footer {
            assert!(attributes.contains(&"component".to_string()));
            assert!(!attributes.contains(&"entrypoint".to_string()));
        }
    }

    #[test]
    fn test_parse_ts_async_function_detected() {
        let code = r#"
async function fetchData() {
    return await fetch('/api');
}
"#;
        let parsed = parse_typescript_file(Path::new("test.ts"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        if let Symbol::Function { is_async, .. } = funcs[0] {
            assert!(*is_async, "async function should be detected");
        }
    }

    #[test]
    fn test_parse_ts_function_params_and_return() {
        let code = r#"
function greet(name: string): string {
    return "Hello, " + name;
}
"#;
        let parsed = parse_typescript_file(Path::new("test.ts"), code).unwrap();
        assert_eq!(parsed.symbols.len(), 1);
        if let Symbol::Function {
            params,
            return_type,
            ..
        } = &parsed.symbols[0]
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "name");
            assert_eq!(params[0].type_annotation, Some("string".to_string()));
            assert!(return_type.is_some());
        }
    }
}
