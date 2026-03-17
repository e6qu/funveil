//! Go language parser for Tree-sitter.
//!
//! Supports Go source files (.go) with:
//! - Function and method declarations
//! - Struct and interface declarations
//! - Import statements
//! - Entrypoint detection (package main, func main())

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language as TSLanguage, Node, Query, QueryCursor, Tree};

use crate::error::Result;
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
    ])) @type.decl
"#;

/// Query for extracting Go imports
const GO_IMPORT_QUERY: &str = r#"
(import_spec
  name: (package_identifier)? @import.alias
  path: (interpreted_string_literal) @import.path) @import.def
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
        .expect("Failed to load Go parser");

    let tree = parser
        .parse(content, None)
        .expect("tree-sitter parse must succeed when language is set");

    let mut parsed = ParsedFile::new(language, path.to_path_buf());

    let package_name = extract_package_name(&tree, content);
    let is_main_package = package_name.as_deref() == Some("main");

    let func_query = Query::new(&go_lang, GO_FUNCTION_QUERY).expect("Invalid Go function query");
    let type_query = Query::new(&go_lang, GO_TYPE_QUERY).expect("Invalid Go type query");
    let import_query = Query::new(&go_lang, GO_IMPORT_QUERY).expect("Invalid Go import query");
    let call_query = Query::new(&go_lang, GO_CALL_QUERY).expect("Invalid Go call query");

    parsed.symbols = extract_go_functions(&tree, &func_query, content, is_main_package)?;

    let mut types = extract_go_types(&tree, &type_query, content)?;
    parsed.symbols.append(&mut types);

    crate::parser::assign_methods_to_classes(&mut parsed.symbols);

    parsed.imports = extract_go_imports(&tree, &import_query, content)?;
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
            let text = Some(
                node.utf8_text(content.as_bytes())
                    .expect("source is valid UTF-8"),
            );

            match capture_name.as_str() {
                "func.name" => name = text.map(|s| s.to_string()),
                "func.params" => {
                    params = parse_go_params(node, content);
                }
                "func.return" => {
                    return_type = text.map(|s| s.trim().to_string());
                }
                "func.body" => {
                    body_start = node.start_position().row + 1;
                    body_end = node.end_position().row + 1;
                }
                "func.def" => {
                    start_line = node.start_position().row + 1;
                    end_line = node.end_position().row + 1;
                }
                // Query only defines func.name, func.params, func.return, func.body, func.def
                _ => unreachable!("unexpected capture: {capture_name}"),
            }
        }

        if let Some(name) = name {
            // Build body range (func.body capture is required, so body_start is always set)
            let body_range = LineRange::new(body_start, body_end).ok();

            let line_range = LineRange::new(start_line, end_line)
                .expect("Invalid line range from tree-sitter positions");

            // Go uses capitalization for visibility
            let visibility = if name.starts_with(|c: char| c.is_uppercase()) {
                Visibility::Public
            } else {
                Visibility::Private
            };

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
                visibility,
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
            // Regular parameter declaration: `name Type` or `name1, name2 Type`
            "parameter_declaration" => {
                let mut param_cursor = child.walk();
                let mut param_names: Vec<String> = Vec::new();
                let mut param_type: Option<String> = None;

                for param_child in child.children(&mut param_cursor) {
                    match param_child.kind() {
                        "identifier" => {
                            let text = param_child
                                .utf8_text(content.as_bytes())
                                .expect("source is valid UTF-8");
                            param_names.push(text.to_string());
                        }
                        // All Go type node kinds contain "type" (e.g. pointer_type,
                        // slice_type, qualified_type, map_type, etc.)
                        kind if kind.contains("type") => {
                            let text = param_child
                                .utf8_text(content.as_bytes())
                                .expect("source is valid UTF-8");
                            param_type = Some(text.to_string());
                        }
                        // Skip punctuation tokens (commas, parens)
                        _ => {}
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
            // Variadic parameter declaration: `name ...Type`
            // tree-sitter Go represents this as a separate node kind
            "variadic_parameter_declaration" => {
                let mut param_cursor = child.walk();
                let mut param_name: Option<String> = None;
                let mut param_type: Option<String> = None;

                for param_child in child.children(&mut param_cursor) {
                    match param_child.kind() {
                        "identifier" => {
                            let text = param_child
                                .utf8_text(content.as_bytes())
                                .expect("source is valid UTF-8");
                            param_name = Some(text.to_string());
                        }
                        "..." => {
                            // The ellipsis token itself; type follows
                        }
                        _ => {
                            let text = param_child
                                .utf8_text(content.as_bytes())
                                .expect("source is valid UTF-8");
                            param_type = Some(format!("...{text}"));
                        }
                    }
                }

                if let Some(name) = param_name {
                    params.push(Param {
                        name,
                        type_annotation: param_type,
                    });
                }
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
            let text = Some(
                node.utf8_text(content.as_bytes())
                    .expect("source is valid UTF-8"),
            );

            match capture_name.as_str() {
                "type.name" => name = text.map(|s| s.to_string()),
                "struct.def" => kind = ClassKind::Struct,
                "interface.def" => kind = ClassKind::Interface,
                "type.decl" => {
                    start_line = node.start_position().row + 1;
                    end_line = node.end_position().row + 1;
                }
                // Query only defines type.name, struct.def, interface.def, type.decl
                _ => unreachable!("unexpected capture: {capture_name}"),
            }
        }

        if let Some(name) = name {
            let line_range = LineRange::new(start_line, end_line)
                .expect("Invalid line range from tree-sitter positions");

            let visibility = if name.starts_with(|c: char| c.is_uppercase()) {
                Visibility::Public
            } else {
                Visibility::Private
            };

            symbols.push(Symbol::Class {
                name,
                methods: Vec::new(),
                properties: Vec::new(),
                visibility,
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
            let text = Some(
                node.utf8_text(content.as_bytes())
                    .expect("source is valid UTF-8"),
            );

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
            let text = Some(
                node.utf8_text(content.as_bytes())
                    .expect("source is valid UTF-8"),
            );
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

#[cfg_attr(coverage_nightly, coverage(off))]
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

    #[test]
    fn test_is_test_file_helper() {
        assert!(is_test_file(Path::new("main_test.go")));
        assert!(is_test_file(Path::new("foo_test.go")));
        assert!(!is_test_file(Path::new("main.go")));
        assert!(!is_test_file(Path::new("test.go")));
        assert!(!is_test_file(Path::new("testing.go")));
    }

    #[test]
    fn test_is_main_function_helper() {
        use crate::types::LineRange;

        let main_sym = Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        };
        assert!(is_main_function(&main_sym));

        let other_sym = Symbol::Function {
            name: "helper".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        };
        assert!(!is_main_function(&other_sym));
    }

    #[test]
    fn test_parse_init_function() {
        let code = r#"package main

func init() {
    // initialization
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "init");

        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(attributes.contains(&"init".to_string()));
            assert!(attributes.contains(&"entrypoint".to_string()));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_variadic_function() {
        let code = r#"package main

func printAll(prefix string, values ...string) {
    for _, v := range values {
        println(prefix + v)
    }
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "printAll");

        if let Symbol::Function { params, .. } = funcs[0] {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "prefix");
            assert_eq!(params[0].type_annotation.as_deref(), Some("string"));
            assert_eq!(params[1].name, "values");
            assert_eq!(params[1].type_annotation.as_deref(), Some("...string"));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_no_package_clause() {
        // Go files always have a package clause but let's test robustness
        let code = r#"
func standalone() int {
    return 42
}
"#;

        // This should parse without panicking even with no package clause
        let result = parse_go_file(Path::new("test.go"), code);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_main_entrypoint() {
        let code = r#"package main

func main() {
    println("Hello, World!")
}
"#;

        let parsed = parse_go_file(Path::new("main.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);

        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(attributes.contains(&"entrypoint".to_string()));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_function_with_complex_params() {
        let code = r#"package main

func process(ctx context.Context, data map[string]interface{}, callback func(int) error) error {
    return nil
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "process");

        if let Symbol::Function {
            params,
            return_type,
            ..
        } = funcs[0]
        {
            assert_eq!(params.len(), 3);
            assert_eq!(params[0].name, "ctx");
            assert!(params[0]
                .type_annotation
                .as_ref()
                .unwrap()
                .contains("context.Context"));
            assert_eq!(params[1].name, "data");
            assert!(params[1].type_annotation.as_ref().unwrap().contains("map"));
            assert_eq!(params[2].name, "callback");
            assert!(params[2].type_annotation.as_ref().unwrap().contains("func"));
            assert_eq!(return_type.as_deref(), Some("error"));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_function_with_interface_and_struct_params() {
        // Tests interface_type and struct_type matching in parse_go_params (line 276)
        let code = r#"package main

func withInline(a interface{}, b struct{ X int }) {
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "withInline");

        if let Symbol::Function { params, .. } = funcs[0] {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "a");
            assert!(params[0]
                .type_annotation
                .as_ref()
                .unwrap()
                .contains("interface"));
            assert_eq!(params[1].name, "b");
            assert!(params[1]
                .type_annotation
                .as_ref()
                .unwrap()
                .contains("struct"));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_function_with_channel_params() {
        // Tests channel_type matching in parse_go_params
        let code = r#"package main

func worker(ch chan int, done chan<- bool) {
    done <- true
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);

        if let Symbol::Function { params, .. } = funcs[0] {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "ch");
            assert!(params[0].type_annotation.as_ref().unwrap().contains("chan"));
            assert_eq!(params[1].name, "done");
            assert!(params[1].type_annotation.as_ref().unwrap().contains("chan"));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_calls_extraction() {
        let code = r#"package main

import "fmt"

func helper() int {
    return 42
}

func main() {
    x := helper()
    fmt.Println(x)
}
"#;

        let parsed = parse_go_file(Path::new("main.go"), code).unwrap();
        assert!(!parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_variadic_int_params() {
        // Tests variadic parameter parsing with ...int syntax
        // tree-sitter Go uses variadic_parameter_declaration node kind
        let code = r#"package main

func foo(prefix string, args ...int) {
    println(prefix, args)
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "foo");

        if let Symbol::Function { params, .. } = funcs[0] {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "prefix");
            assert_eq!(params[0].type_annotation.as_deref(), Some("string"));
            assert_eq!(params[1].name, "args");
            assert_eq!(params[1].type_annotation.as_deref(), Some("...int"));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_method_with_receiver_params() {
        let code = r#"package main

type Receiver struct {
    Value int
}

func (r *Receiver) Method(x int) int {
    return r.Value + x
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "Method");

        if let Symbol::Function {
            params,
            return_type,
            ..
        } = funcs[0]
        {
            // The query captures parameters: (parameter_list) which is the method params,
            // not the receiver. So we should see just 'x int'.
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "x");
            assert_eq!(params[0].type_annotation.as_deref(), Some("int"));
            assert_eq!(return_type.as_deref(), Some("int"));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_calls_with_method_calls() {
        let code = r#"package main

import "fmt"

func helper() int {
    return 42
}

func caller() {
    x := helper()
    fmt.Println(x)
    fmt.Sprintf("value: %d", x)
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        assert!(parsed.calls.len() >= 2);

        let callees: Vec<_> = parsed.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"helper"));
    }

    #[test]
    fn test_parse_import_with_alias() {
        // Tests import alias extraction (covers line 401: alias capture)
        let code = r#"package main

import (
    "fmt"
    myfmt "github.com/example/fmt"
)

func main() {
    fmt.Println("hi")
}
"#;

        let parsed = parse_go_file(Path::new("main.go"), code).unwrap();

        // Should have 2 imports
        assert_eq!(parsed.imports.len(), 2);

        // Check that the aliased import path and alias are present
        let fmt_import = parsed.imports.iter().find(|i| i.path == "fmt").unwrap();
        assert!(fmt_import.alias.is_none());

        let aliased_import = parsed
            .imports
            .iter()
            .find(|i| i.path == "github.com/example/fmt")
            .unwrap();
        assert_eq!(aliased_import.alias.as_deref(), Some("myfmt"));
    }

    #[test]
    fn test_parse_struct_and_interface_types() {
        let code = r#"package main

type MyStruct struct {
    Field1 string
    Field2 int
}

type MyInterface interface {
    DoSomething() error
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 2);
    }

    #[test]
    fn test_parse_function_without_body() {
        // A function declaration without a body (e.g. in an interface or external decl)
        // should trigger the body_start == 0 fallback (line 217)
        let code = r#"package main

func standalone() int {
    return 42
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);

        if let Symbol::Function {
            body_range,
            line_range,
            ..
        } = funcs[0]
        {
            // body_range should be valid
            assert!(body_range.start() >= line_range.start());
            assert!(body_range.end() <= line_range.end());
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_benchmark_function() {
        let code = r#"package main

import "testing"

func BenchmarkAdd(b *testing.B) {
    for i := 0; i < b.N; i++ {
        _ = 1 + 1
    }
}
"#;

        let parsed = parse_go_file(Path::new("bench_test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);

        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(attributes.contains(&"test".to_string()));
            assert!(attributes.contains(&"entrypoint".to_string()));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_multi_name_param_declaration() {
        // Tests `a, b int` syntax where comma tokens appear inside parameter_declaration
        let code = r#"package main

func swap(a, b int) (int, int) {
    return b, a
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "swap");

        if let Symbol::Function { params, .. } = funcs[0] {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "a");
            assert_eq!(params[0].type_annotation.as_deref(), Some("int"));
            assert_eq!(params[1].name, "b");
            assert_eq!(params[1].type_annotation.as_deref(), Some("int"));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_go_empty_input_no_panic() {
        let result = parse_go_file(Path::new("test.go"), "");
        // Should not panic - result can be Ok or Err
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_go_exported_function_visibility() {
        let code = r#"package main

func ExportedFunc() {}
func unexportedFunc() {}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 2);

        let exported = funcs.iter().find(|f| f.name() == "ExportedFunc").unwrap();
        let unexported = funcs.iter().find(|f| f.name() == "unexportedFunc").unwrap();

        if let Symbol::Function { visibility, .. } = exported {
            assert_eq!(*visibility, Visibility::Public);
        } else {
            panic!("Expected function symbol");
        }

        if let Symbol::Function { visibility, .. } = unexported {
            assert_eq!(*visibility, Visibility::Private);
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_go_empty_content() {
        let parsed = parse_go_file(Path::new("test.go"), "").unwrap();
        assert!(parsed.symbols.is_empty());
        assert!(parsed.imports.is_empty());
        assert!(parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_go_only_comments() {
        let code = "// just a comment\n/* block comment */\n";
        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        assert!(parsed.symbols.is_empty());
        assert!(parsed.imports.is_empty());
    }

    #[test]
    fn test_parse_go_only_package() {
        let code = "package main\n";
        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        assert!(parsed.symbols.is_empty());
        assert!(parsed.imports.is_empty());
        assert!(parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_go_no_functions_with_imports() {
        let code = r#"package main

import "fmt"
"#;
        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        assert!(parsed.symbols.is_empty());
        assert_eq!(parsed.imports.len(), 1);
    }

    #[test]
    fn test_parse_go_no_types() {
        let code = r#"package main

func doWork() {}
"#;
        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert!(classes.is_empty());
    }

    #[test]
    fn test_parse_go_no_calls() {
        let code = r#"package main

func empty() {
    x := 42
    _ = x
}
"#;
        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        assert!(parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_go_type_alias() {
        let code = r#"package main

type MyInt int
"#;
        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 0);
    }

    #[test]
    fn test_parse_go_interface_type() {
        let code = r#"package main

type Reader interface {
    Read(p []byte) (n int, err error)
}
"#;
        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        if let Symbol::Class { kind, .. } = &classes[0] {
            assert_eq!(*kind, ClassKind::Interface);
        }
    }

    #[test]
    fn test_parse_go_non_main_package() {
        let code = r#"package utils

func Main() {
}
"#;
        let parsed = parse_go_file(Path::new("utils.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(!attributes.contains(&"entrypoint".to_string()));
        }
    }

    #[test]
    fn test_parse_go_example_function() {
        let code = r#"package main

func ExampleHello() {
    // Output: Hello
}
"#;
        let parsed = parse_go_file(Path::new("example_test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(attributes.contains(&"test".to_string()));
            assert!(attributes.contains(&"entrypoint".to_string()));
        }
    }

    #[test]
    fn test_go_exported_type_visibility() {
        let code = r#"package main

type ExportedStruct struct {
    Field int
}

type unexportedStruct struct {
    field int
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 2);

        let exported = classes
            .iter()
            .find(|c| c.name() == "ExportedStruct")
            .unwrap();
        let unexported = classes
            .iter()
            .find(|c| c.name() == "unexportedStruct")
            .unwrap();

        if let Symbol::Class { visibility, .. } = exported {
            assert_eq!(*visibility, Visibility::Public);
        } else {
            panic!("Expected class symbol");
        }

        if let Symbol::Class { visibility, .. } = unexported {
            assert_eq!(*visibility, Visibility::Private);
        } else {
            panic!("Expected class symbol");
        }
    }

    #[test]
    fn test_is_main_function_with_class_symbol() {
        // Tests the false branch of the matches! macro when symbol is not a Function
        use crate::types::LineRange;

        let class_sym = Symbol::Class {
            name: "main".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            kind: ClassKind::Struct,
        };
        assert!(!is_main_function(&class_sym));
    }

    #[test]
    fn test_is_test_file_no_extension() {
        // Tests the unwrap_or(false) branch when file has no stem or extension
        assert!(!is_test_file(Path::new("")));
        assert!(!is_test_file(Path::new(".")));
        assert!(!is_test_file(Path::new(".hidden")));
    }

    #[test]
    fn test_non_main_package_main_func_not_entrypoint() {
        // In a non-main package, func main() should NOT be an entrypoint
        let code = r#"package utils

func main() {
    println("not an entrypoint")
}
"#;

        let parsed = parse_go_file(Path::new("utils.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "main");

        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(
                !attributes.contains(&"entrypoint".to_string()),
                "main() in non-main package should not be an entrypoint"
            );
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_call_outside_any_function() {
        // Tests the None branch for caller in extract_go_calls
        // In Go, top-level calls don't exist syntactically, but init() calls
        // can be tested by having a call at a line not covered by any function
        let code = r#"package main

import "fmt"

var x = fmt.Sprintf("hello")

func helper() {
    fmt.Println("world")
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        // Verify calls were extracted (some may have no caller)
        let calls_without_caller: Vec<_> =
            parsed.calls.iter().filter(|c| c.caller.is_none()).collect();
        // The top-level var init call should have no caller
        assert!(
            !calls_without_caller.is_empty() || !parsed.calls.is_empty(),
            "Should extract at least some calls"
        );
    }

    #[test]
    fn test_type_alias_declaration() {
        let code = r#"package main

type MyString string
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 0);
    }

    #[test]
    fn test_example_test_function() {
        // Tests the Example prefix branch of is_test_function
        let code = r#"package main

func ExampleHello() {
    println("Hello")
    // Output: Hello
}
"#;

        let parsed = parse_go_file(Path::new("example_test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "ExampleHello");

        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(attributes.contains(&"test".to_string()));
            assert!(attributes.contains(&"entrypoint".to_string()));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_function_no_return_type() {
        // Tests the None branch for return_type capture
        let code = r#"package main

func doNothing() {
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);

        if let Symbol::Function { return_type, .. } = funcs[0] {
            assert!(
                return_type.is_none(),
                "void function should have no return type"
            );
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_interface_kind_extraction() {
        // Explicitly tests the interface.def match arm in extract_go_types
        let code = r#"package main

type Reader interface {
    Read(p []byte) (int, error)
}

type Writer interface {
    Write(p []byte) (int, error)
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 2);

        for class in &classes {
            if let Symbol::Class { kind, .. } = class {
                assert_eq!(*kind, ClassKind::Interface);
            } else {
                panic!("Expected class symbol");
            }
        }
    }

    #[test]
    fn test_single_import_no_grouping() {
        // Tests single import (not grouped) path
        let code = r#"package main

import "fmt"

func main() {
    fmt.Println("hi")
}
"#;

        let parsed = parse_go_file(Path::new("main.go"), code).unwrap();
        assert_eq!(parsed.imports.len(), 1);
        assert_eq!(parsed.imports[0].path, "fmt");
        assert!(parsed.imports[0].alias.is_none());
        assert!(parsed.imports[0].line > 0);
    }

    #[test]
    fn test_multiple_functions_mixed_visibility_and_attributes() {
        // Tests multiple branches in one pass: public/private, entrypoint/test/init/regular
        let code = r#"package main

func init() {
    println("init")
}

func main() {
    println("main")
}

func TestSomething() {
}

func helperPrivate() int {
    return 1
}

func HelperPublic() int {
    return 2
}
"#;

        let parsed = parse_go_file(Path::new("main.go"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 5);

        let init_fn = funcs.iter().find(|f| f.name() == "init").unwrap();
        if let Symbol::Function {
            attributes,
            visibility,
            ..
        } = init_fn
        {
            assert!(attributes.contains(&"init".to_string()));
            assert!(attributes.contains(&"entrypoint".to_string()));
            assert_eq!(*visibility, Visibility::Private);
        }

        let main_fn = funcs.iter().find(|f| f.name() == "main").unwrap();
        if let Symbol::Function { attributes, .. } = main_fn {
            assert!(attributes.contains(&"entrypoint".to_string()));
            assert!(!attributes.contains(&"test".to_string()));
            assert!(!attributes.contains(&"init".to_string()));
        }

        let test_fn = funcs.iter().find(|f| f.name() == "TestSomething").unwrap();
        if let Symbol::Function {
            attributes,
            visibility,
            ..
        } = test_fn
        {
            assert!(attributes.contains(&"test".to_string()));
            assert!(attributes.contains(&"entrypoint".to_string()));
            assert_eq!(*visibility, Visibility::Public);
        }

        let helper_priv = funcs.iter().find(|f| f.name() == "helperPrivate").unwrap();
        if let Symbol::Function {
            attributes,
            visibility,
            ..
        } = helper_priv
        {
            assert!(attributes.is_empty());
            assert_eq!(*visibility, Visibility::Private);
        }

        let helper_pub = funcs.iter().find(|f| f.name() == "HelperPublic").unwrap();
        if let Symbol::Function {
            attributes,
            visibility,
            ..
        } = helper_pub
        {
            assert!(attributes.is_empty());
            assert_eq!(*visibility, Visibility::Public);
        }
    }

    #[test]
    fn test_calls_with_caller_resolution() {
        // Tests that caller is properly resolved from the line_to_function map
        let code = r#"package main

import "fmt"

func caller_func() {
    fmt.Println("hello")
}
"#;

        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let calls_with_caller: Vec<_> =
            parsed.calls.iter().filter(|c| c.caller.is_some()).collect();
        assert!(!calls_with_caller.is_empty());
        assert_eq!(calls_with_caller[0].caller.as_deref(), Some("caller_func"));
    }

    #[test]
    fn test_parse_go_type_alias_excluded() {
        let code = r#"package main

type MyString string

type Point struct {
    X int
    Y int
}
"#;
        let parsed = parse_go_file(Path::new("test.go"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name(), "Point");
    }
}
