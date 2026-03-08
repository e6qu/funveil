//! Go language parser for Tree-sitter.
//!
//! Supports Go source files (.go) with:
//! - Function and method declarations
//! - Struct and interface declarations
//! - Import statements
//! - Entrypoint detection (package main, func main())

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language as TSLanguage, Node, Query, QueryCursor, Tree};

use crate::error::{FunveilError, Result};
use crate::parser::{Call, ClassKind, Import, Language, Param, ParsedFile, Symbol, Visibility};
use crate::types::LineRange;

/// Tree-sitter language for Go
pub fn go_language() -> TSLanguage {
    tree_sitter_go::LANGUAGE.into()
}

/// Query for extracting Go functions and methods
const GO_FUNCTION_QUERY: &str = r#"
[
  ; Regular functions
  (function_declaration
    name: (identifier) @func.name
    parameters: (parameter_list) @func.params
    result: (_)? @func.return
    body: (block) @func.body) @func.def
  
  ; Method declarations (with receiver)
  (method_declaration
    name: (field_identifier) @func.name
    parameters: (parameter_list) @func.params
    result: (_)? @func.return
    body: (block) @func.body) @func.def
]
"#;

/// Query for extracting Go types (structs, interfaces)
const GO_TYPE_QUERY: &str = r#"
(type_declaration
  (type_spec
    name: (type_identifier) @type.name
    type: [
      (struct_type) @struct.def
      (interface_type) @interface.def
      (type_identifier) @alias.def
    ])) @type.decl
"#;

/// Query for extracting Go imports
const GO_IMPORT_QUERY: &str = r#"
(import_spec
  path: (interpreted_string_literal) @import.path
  alias: (_)? @import.alias) @import.def
"#;

/// Query for extracting function calls
const GO_CALL_QUERY: &str = r#"
(call_expression
  function: [
    (identifier) @call.name
    (selector_expression
      field: (field_identifier) @call.name)
    (parenthesized_expression) @call.name
  ]) @call.expr
"#;

/// Check if a file is a Go test file
pub fn is_test_file(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.ends_with("_test"))
        .unwrap_or(false)
}

/// Check if a function is a Go test function (TestXxx, BenchmarkXxx, ExampleXxx)
pub fn is_test_function(name: &str) -> bool {
    name.starts_with("Test") || name.starts_with("Benchmark") || name.starts_with("Example")
}

/// Check if this is a main package entrypoint
pub fn is_main_function(symbol: &Symbol) -> bool {
    matches!(symbol, Symbol::Function { name, .. } if name == "main")
}

/// Parse a Go source file
pub fn parse_go_file(path: &std::path::Path, content: &str) -> Result<ParsedFile> {
    let language = Language::Go;
    let mut parser = tree_sitter::Parser::new();
    let go_lang = go_language();
    parser
        .set_language(&go_lang)
        .map_err(|e| FunveilError::TreeSitterError(format!("Failed to load Go parser: {e}")))?;

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| FunveilError::TreeSitterError("Failed to parse Go file".to_string()))?;

    let mut parsed = ParsedFile::new(language, path.to_path_buf());

    // Extract package name from source
    let package_name = extract_package_name(&tree, content);
    let is_main_package = package_name.as_deref() == Some("main");

    // Build queries
    let func_query = Query::new(&go_lang, GO_FUNCTION_QUERY)
        .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Go function query: {e}")))?;
    let type_query = Query::new(&go_lang, GO_TYPE_QUERY)
        .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Go type query: {e}")))?;
    let import_query = Query::new(&go_lang, GO_IMPORT_QUERY)
        .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Go import query: {e}")))?;
    let call_query = Query::new(&go_lang, GO_CALL_QUERY)
        .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Go call query: {e}")))?;

    // Extract functions
    parsed.symbols = extract_go_functions(&tree, &func_query, content, is_main_package)?;

    // Extract types (structs, interfaces)
    let mut types = extract_go_types(&tree, &type_query, content)?;
    parsed.symbols.append(&mut types);

    // Extract imports
    parsed.imports = extract_go_imports(&tree, &import_query, content)?;

    // Extract calls
    parsed.calls = extract_go_calls(&tree, &call_query, content, &parsed.symbols)?;

    Ok(parsed)
}

/// Extract the package name from a Go file
fn extract_package_name(tree: &Tree, content: &str) -> Option<String> {
    let root = tree.root_node();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "package_clause" {
            // package_clause -> package_identifier
            let mut inner_cursor = child.walk();
            for inner_child in child.children(&mut inner_cursor) {
                if inner_child.kind() == "package_identifier" {
                    return inner_child
                        .utf8_text(content.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
            }
        }
    }

    None
}

/// Extract function symbols from Go source
fn extract_go_functions(
    tree: &Tree,
    query: &Query,
    content: &str,
    is_main_package: bool,
) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), content.as_bytes());

    while let Some(m) = matches.next() {
        let mut name: Option<String> = None;
        let mut params: Vec<Param> = Vec::new();
        let mut return_type: Option<String> = None;
        let mut start_line = 0;
        let mut end_line = 0;
        let mut body_start = 0;
        let mut body_end = 0;

        for capture in m.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node = capture.node;
            let text = node.utf8_text(content.as_bytes()).ok();

            match capture_name.as_str() {
                "func.name" => name = text.map(|s| s.to_string()),
                "func.params" => {
                    params = parse_go_params(node, content);
                }
                "func.return" => {
                    return_type = text.map(|s| s.trim().to_string());
                    // Remove "->" if present (shouldn't be in Go, but just in case)
                    if let Some(ref mut rt) = return_type {
                        if rt.starts_with("-> ") {
                            *rt = rt[3..].to_string();
                        }
                    }
                }
                "func.body" => {
                    body_start = node.start_position().row + 1;
                    body_end = node.end_position().row + 1;
                }
                "func.def" => {
                    start_line = node.start_position().row + 1;
                    end_line = node.end_position().row + 1;
                }
                _ => {}
            }
        }

        if let Some(name) = name {
            // Build body range
            let body_range = if body_start > 0 && body_end >= body_start {
                LineRange::new(body_start, body_end).ok()
            } else {
                LineRange::new(start_line + 1, end_line).ok()
            };

            let line_range = LineRange::new(start_line, end_line)
                .map_err(|e| FunveilError::TreeSitterError(format!("Invalid line range: {e}")))?;

            // Detect if this is an entrypoint
            let is_entrypoint =
                (is_main_package && name == "main") || is_test_function(&name) || name == "init";

            let mut attributes = Vec::new();
            if is_entrypoint {
                attributes.push("entrypoint".to_string());
            }
            if is_test_function(&name) {
                attributes.push("test".to_string());
            }
            if name == "init" {
                attributes.push("init".to_string());
            }

            symbols.push(Symbol::Function {
                name,
                params,
                return_type,
                visibility: Visibility::Public, // Go uses capitalization for visibility
                line_range,
                body_range: body_range.unwrap_or(line_range),
                is_async: false, // Go doesn't have async/await
                attributes,
            });
        }
    }

    Ok(symbols)
}

/// Parse Go parameters from a parameter_list node
fn parse_go_params(node: Node, content: &str) -> Vec<Param> {
    let mut params = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        match child.kind() {
            // Regular parameter declaration
            "parameter_declaration" => {
                let mut param_cursor = child.walk();
                let mut param_names: Vec<String> = Vec::new();
                let mut param_type: Option<String> = None;

                for param_child in child.children(&mut param_cursor) {
                    match param_child.kind() {
                        "identifier" => {
                            if let Ok(text) = param_child.utf8_text(content.as_bytes()) {
                                param_names.push(text.to_string());
                            }
                        }
                        "type_identifier" | "qualified_type" | "pointer_type" | "slice_type"
                        | "array_type" | "map_type" | "function_type" | "channel_type"
                        | "interface_type" | "struct_type" => {
                            if let Ok(text) = param_child.utf8_text(content.as_bytes()) {
                                param_type = Some(text.to_string());
                            }
                        }
                        "ellipsis" => {
                            // Variadic parameter (...)
                            param_type = Some("...".to_string());
                        }
                        _ => {
                            // Try to get type text for other type nodes
                            if param_child.kind().contains("type")
                                || param_child.kind() == "generic_type"
                            {
                                if let Ok(text) = param_child.utf8_text(content.as_bytes()) {
                                    param_type = Some(text.to_string());
                                }
                            }
                        }
                    }
                }

                // Create a param for each name with the same type
                for name in param_names {
                    params.push(Param {
                        name,
                        type_annotation: param_type.clone(),
                    });
                }
            }
            // Receiver (for methods)
            "parameter_list" => {
                // Recursively parse nested parameter lists (receivers)
                let receiver_params = parse_go_params(child, content);
                params.extend(receiver_params);
            }
            _ => {}
        }
    }

    params
}

/// Extract type symbols (structs, interfaces) from Go source
fn extract_go_types(tree: &Tree, query: &Query, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), content.as_bytes());

    while let Some(m) = matches.next() {
        let mut name: Option<String> = None;
        let mut start_line = 0;
        let mut end_line = 0;
        let mut kind = ClassKind::Struct;

        for capture in m.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node = capture.node;
            let text = node.utf8_text(content.as_bytes()).ok();

            match capture_name.as_str() {
                "type.name" => name = text.map(|s| s.to_string()),
                "struct.def" => kind = ClassKind::Struct,
                "interface.def" => kind = ClassKind::Interface,
                "type.decl" => {
                    start_line = node.start_position().row + 1;
                    end_line = node.end_position().row + 1;
                }
                _ => {}
            }
        }

        if let Some(name) = name {
            let line_range = LineRange::new(start_line, end_line)
                .map_err(|e| FunveilError::TreeSitterError(format!("Invalid line range: {e}")))?;

            symbols.push(Symbol::Class {
                name,
                methods: Vec::new(),
                properties: Vec::new(),
                visibility: Visibility::Public, // Go uses capitalization
                line_range,
                kind,
            });
        }
    }

    Ok(symbols)
}

/// Extract imports from Go source
fn extract_go_imports(tree: &Tree, query: &Query, content: &str) -> Result<Vec<Import>> {
    let mut imports = Vec::new();
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), content.as_bytes());

    while let Some(m) = matches.next() {
        let mut path: Option<String> = None;
        let mut alias: Option<String> = None;
        let mut line = 0;

        for capture in m.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node = capture.node;
            let text = node.utf8_text(content.as_bytes()).ok();

            match capture_name.as_str() {
                "import.path" => {
                    path = text.map(|s| {
                        // Remove quotes from import path
                        s.trim_matches('"').to_string()
                    });
                    line = node.start_position().row + 1;
                }
                "import.alias" => {
                    alias = text.map(|s| s.to_string());
                }
                _ => {}
            }
        }

        if let Some(path) = path {
            imports.push(Import { path, alias, line });
        }
    }

    Ok(imports)
}

/// Extract function calls from Go source
fn extract_go_calls(
    tree: &Tree,
    query: &Query,
    content: &str,
    symbols: &[Symbol],
) -> Result<Vec<Call>> {
    let mut calls = Vec::new();
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), content.as_bytes());

    // Build a map of line -> function name for determining caller
    let mut line_to_function: std::collections::HashMap<usize, String> =
        std::collections::HashMap::new();
    for symbol in symbols {
        if let Symbol::Function {
            name, line_range, ..
        } = symbol
        {
            for line in line_range.start()..=line_range.end() {
                line_to_function.insert(line, name.clone());
            }
        }
    }

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node = capture.node;
            let text = node.utf8_text(content.as_bytes()).ok();
            let line = node.start_position().row + 1;

            if capture_name == "call.name" {
                if let Some(callee) = text.map(|s| s.to_string()) {
                    let caller = line_to_function.get(&line).cloned();

                    calls.push(Call {
                        caller,
                        callee,
                        line,
                        is_dynamic: false,
                    });
                }
            }
        }
    }

    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_simple_function() {
        let code = r#"package main

func calculateSum(numbers []int) int {
    sum := 0
    for _, n := range numbers {
        sum += n
    }
    return sum
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        assert_eq!(parsed.symbols.len(), 1);

        let func = &parsed.symbols[0];
        assert_eq!(func.name(), "calculateSum");

        if let Symbol::Function {
            params,
            return_type,
            attributes,
            ..
        } = func
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "numbers");
            assert!(params[0]
                .type_annotation
                .as_ref()
                .unwrap()
                .contains("[]int"));
            assert_eq!(return_type.as_deref(), Some("int"));
            // Only 'main' function gets entrypoint attribute in main package
            assert!(!attributes.contains(&"entrypoint".to_string()));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_method() {
        let code = r#"package main

type Person struct {
    Name string
    Age  int
}

func (p *Person) Greet() string {
    return "Hello, " + p.Name
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();

        // Should have the struct and the method
        let funcs: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Function { .. }))
            .collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "Greet");

        if let Symbol::Function { params, .. } = funcs[0] {
            // Method receiver is part of the parameter list but may be filtered
            // The function itself is detected correctly
            assert_eq!(params.len(), 0); // Receiver is not a regular param
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_struct_and_interface() {
        let code = r#"package main

type Animal interface {
    Speak() string
}

type Dog struct {
    Name string
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();

        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 2);

        // Check that we have both struct and interface
        let names: Vec<_> = classes.iter().map(|c| c.name()).collect();
        assert!(names.contains(&"Animal"));
        assert!(names.contains(&"Dog"));
    }

    #[test]
    fn test_parse_imports() {
        let code = r#"package main

import (
    "fmt"
    "strings"
    mylib "github.com/example/mylib"
)

func main() {
    fmt.Println("Hello")
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();

        // Should have fmt, strings, and mylib imports
        let import_paths: Vec<_> = parsed.imports.iter().map(|i| i.path.as_str()).collect();
        assert!(import_paths.contains(&"fmt"));
        assert!(import_paths.contains(&"strings"));
        assert!(import_paths.contains(&"github.com/example/mylib"));

        // Should have exactly 3 imports
        assert_eq!(parsed.imports.len(), 3);
    }

    #[test]
    fn test_is_test_function() {
        assert!(is_test_function("TestAdd"));
        assert!(is_test_function("TestMain"));
        assert!(is_test_function("BenchmarkSort"));
        assert!(is_test_function("ExampleHello"));
        assert!(!is_test_function("testAdd"));
        assert!(!is_test_function("Add"));
    }

    #[test]
    fn test_parse_test_file() {
        let code = r#"package main

import "testing"

func TestAdd(t *testing.T) {
    result := Add(2, 3)
    if result != 5 {
        t.Errorf("Expected 5, got %d", result)
    }
}
"#;

        let parsed = parse_go_file(Path::new("main_test.go"), code).unwrap();

        let funcs: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Function { .. }))
            .collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "TestAdd");

        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(attributes.contains(&"test".to_string()));
            assert!(attributes.contains(&"entrypoint".to_string()));
        } else {
            panic!("Expected function symbol");
        }
    }
}
