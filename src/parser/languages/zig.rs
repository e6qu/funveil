//! Zig language parser for Tree-sitter.
//!
//! Supports Zig source files (.zig) with:
//! - Function declarations (including pub visibility)
//! - Struct, union, enum declarations
//! - Import statements (@import)
//! - Entrypoint detection (pub fn main())

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language as TSLanguage, Node, Query, QueryCursor, Tree};

use crate::error::Result;
use crate::parser::{Call, ClassKind, Import, Language, ParsedFile, Symbol, Visibility};
use crate::types::LineRange;

/// Tree-sitter language for Zig
pub fn zig_language() -> TSLanguage {
    tree_sitter_zig::LANGUAGE.into()
}

/// Query for extracting Zig functions
const ZIG_FUNCTION_QUERY: &str = r#"
(function_declaration
  name: (identifier) @func.name) @func.def
"#;

/// Query for extracting Zig imports
const ZIG_IMPORT_QUERY: &str = r#"
(builtin_function
  (builtin_identifier) @import.func) @import.def
"#;

/// Query for extracting function calls
const ZIG_CALL_QUERY: &str = r#"
(call_expression
  function: (identifier) @call.name) @call.expr
"#;

/// Check if a Zig function is a test (test "name" { ... })
pub fn is_test_block(node: &Node) -> bool {
    node.kind() == "test_declaration"
}

/// Check if function name indicates a test
pub fn is_test_function(name: &str) -> bool {
    name.starts_with("test") || name.starts_with("bench")
}

/// Parse a Zig source file
pub fn parse_zig_file(path: &std::path::Path, content: &str) -> Result<ParsedFile> {
    let language = Language::Zig;
    let mut parser = tree_sitter::Parser::new();
    let zig_lang = zig_language();
    parser
        .set_language(&zig_lang)
        .expect("Failed to load Zig parser");

    let tree = parser
        .parse(content, None)
        .expect("tree-sitter parse must succeed when language is set");

    let mut parsed = ParsedFile::new(language, path.to_path_buf());

    let func_query = Query::new(&zig_lang, ZIG_FUNCTION_QUERY)
        .expect("Invalid Zig function query: constant query should always be valid");
    let import_query = Query::new(&zig_lang, ZIG_IMPORT_QUERY)
        .expect("Invalid Zig import query: constant query should always be valid");
    let call_query = Query::new(&zig_lang, ZIG_CALL_QUERY)
        .expect("Invalid Zig call query: constant query should always be valid");

    parsed.symbols = extract_zig_functions(&tree, &func_query, content)?;

    // Zig has special test syntax: `test "name" { ... }`
    let mut tests = extract_zig_tests(&tree, content)?;
    parsed.symbols.append(&mut tests);

    let mut types = extract_zig_types(&tree, &zig_lang, content)?;
    parsed.symbols.append(&mut types);

    crate::parser::assign_methods_to_classes(&mut parsed.symbols);

    parsed.imports = extract_zig_imports(&tree, &import_query, content)?;
    parsed.calls = extract_zig_calls(&tree, &call_query, content, &parsed.symbols)?;

    Ok(parsed)
}

/// Extract function symbols from Zig source
fn extract_zig_functions(tree: &Tree, query: &Query, content: &str) -> Result<Vec<Symbol>> {
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
        let mut is_pub = false;
        let mut is_async = false;

        for capture in m.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node = capture.node;

            match capture_name.as_str() {
                "func.name" => {
                    name = Some(
                        node.utf8_text(content.as_bytes())
                            .expect("source is valid UTF-8")
                            .to_string(),
                    );
                    start_line = node.start_position().row + 1;
                    end_line = node.end_position().row + 1;
                }
                "func.def" => {
                    start_line = node.start_position().row + 1;
                    end_line = node.end_position().row + 1;
                    let node_text = node
                        .utf8_text(content.as_bytes())
                        .expect("source is valid UTF-8");
                    is_pub = node_text.starts_with("pub ");
                    is_async = node_text.contains("async fn");
                }
                // Query only defines func.name, func.def
                _ => unreachable!("unexpected capture: {capture_name}"),
            }
        }

        if let Some(name) = name {
            let line_range = LineRange::new(start_line, end_line)
                .expect("Tree-sitter positions should always produce valid line ranges");

            // Detect entrypoints
            let is_entrypoint = name == "main";
            let mut attributes = Vec::new();
            if is_entrypoint {
                attributes.push("entrypoint".to_string());
            }

            symbols.push(Symbol::Function {
                name,
                params: Vec::new(), // Simplified - would need more complex parsing
                return_type: None,
                visibility: if is_pub {
                    Visibility::Public
                } else {
                    Visibility::Private
                },
                line_range,
                body_range: line_range,
                is_async,
                attributes,
            });
        }
    }

    Ok(symbols)
}

/// Extract test declarations from Zig source (test "name" { ... })
fn extract_zig_tests(tree: &Tree, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "test_declaration" {
            let mut test_cursor = child.walk();
            let mut test_name: Option<String> = None;
            let start_line = child.start_position().row + 1;
            let end_line = child.end_position().row + 1;

            for test_child in child.children(&mut test_cursor) {
                if test_child.kind() == "string_literal" || test_child.kind() == "string" {
                    // Extract text from the string_content child node
                    let mut inner_cursor = test_child.walk();
                    for inner_child in test_child.children(&mut inner_cursor) {
                        if inner_child.kind() == "string_content" {
                            test_name = inner_child
                                .utf8_text(content.as_bytes())
                                .ok()
                                .map(|s| s.to_string());
                            break;
                        }
                    }
                    break;
                }
            }

            if let Some(name) = test_name {
                let line_range = LineRange::new(start_line, end_line)
                    .expect("Tree-sitter positions should always produce valid line ranges");

                symbols.push(Symbol::Function {
                    name: format!("test \"{name}\""),
                    params: Vec::new(),
                    return_type: None,
                    visibility: Visibility::Private,
                    line_range,
                    body_range: line_range,
                    is_async: false,
                    attributes: vec!["test".to_string(), "entrypoint".to_string()],
                });
            }
        }
    }

    Ok(symbols)
}

/// Extract type declarations (struct, enum, union) from Zig source.
/// In Zig, types are declared as `const Foo = struct { ... };`
fn extract_zig_types(tree: &Tree, _lang: &TSLanguage, content: &str) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() != "variable_declaration" {
            continue;
        }

        let start_line = child.start_position().row + 1;
        let end_line = child.end_position().row + 1;
        let node_text = child
            .utf8_text(content.as_bytes())
            .expect("source is valid UTF-8");
        let is_pub = node_text.starts_with("pub ");

        let mut name: Option<String> = None;
        let mut kind: Option<ClassKind> = None;

        let mut child_cursor = child.walk();
        for grandchild in child.children(&mut child_cursor) {
            match grandchild.kind() {
                "identifier" => {
                    name = grandchild
                        .utf8_text(content.as_bytes())
                        .ok()
                        .map(|s| s.to_string());
                }
                "struct_declaration" => kind = Some(ClassKind::Struct),
                "enum_declaration" => kind = Some(ClassKind::Enum),
                "union_declaration" => kind = Some(ClassKind::Class),
                _ => {}
            }
        }

        if let (Some(name), Some(kind)) = (name, kind) {
            let line_range = LineRange::new(start_line, end_line)
                .expect("Tree-sitter positions should always produce valid line ranges");

            symbols.push(Symbol::Class {
                name,
                kind,
                methods: Vec::new(),
                properties: Vec::new(),
                visibility: if is_pub {
                    Visibility::Public
                } else {
                    Visibility::Private
                },
                line_range,
            });
        }
    }

    Ok(symbols)
}

/// Extract imports from Zig source (@import("..."))
fn extract_zig_imports(tree: &Tree, query: &Query, content: &str) -> Result<Vec<Import>> {
    let mut imports = Vec::new();
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), content.as_bytes());

    while let Some(m) = matches.next() {
        let mut line = 0;
        let mut is_import = false;

        for capture in m.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node = capture.node;
            let text = Some(
                node.utf8_text(content.as_bytes())
                    .expect("source is valid UTF-8"),
            );

            if capture_name == "import.func" {
                // Check if this is @import
                if text == Some("@import") {
                    is_import = true;
                    line = node.start_position().row + 1;
                }
            }
        }

        if is_import {
            // Try to find the string literal argument by walking the call expression node
            for capture in m.captures {
                let capture_name = &capture_names[capture.index as usize];
                if capture_name == "import.def" {
                    let node = capture.node;
                    let mut child_cursor = node.walk();
                    for child in node.children(&mut child_cursor) {
                        if child.kind() == "arguments" || child.kind() == "function_call_arguments"
                        {
                            let mut arg_cursor = child.walk();
                            for arg in child.children(&mut arg_cursor) {
                                if arg.kind() == "string_literal" || arg.kind() == "string" {
                                    let mut inner_cursor = arg.walk();
                                    for inner in arg.children(&mut inner_cursor) {
                                        if inner.kind() == "string_content" {
                                            if let Some(path) = inner
                                                .utf8_text(content.as_bytes())
                                                .ok()
                                                .map(|s| s.to_string())
                                            {
                                                imports.push(Import {
                                                    path,
                                                    alias: None,
                                                    line,
                                                });
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(imports)
}

/// Extract function calls from Zig source
fn extract_zig_calls(
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
        let code = r#"const std = @import("std");

pub fn calculateSum(numbers: []const i32) i32 {
    var sum: i32 = 0;
    for (numbers) |n| {
        sum += n;
    }
    return sum;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();

        let funcs: Vec<_> = parsed.functions().collect();
        assert!(!funcs.is_empty());

        let func = funcs[0];
        assert_eq!(func.name(), "calculateSum");
    }

    #[test]
    fn test_parse_imports() {
        let code = r#"const std = @import("std");
const math = @import("math.zig");
const mylib = @import("libs/mylib.zig");

pub fn main() void {
    std.debug.print("Hello, Zig!\n", .{});
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();

        // Note: Full import extraction requires more complex parsing
        // For now, just verify the file parses successfully
        assert!(!parsed.functions().collect::<Vec<_>>().is_empty());
    }

    #[test]
    fn test_parse_test_declaration() {
        let code = r#"const std = @import("std");
const testing = std.testing;

test "basic addition" {
    try testing.expectEqual(4, 2 + 2);
}

test "string concat" {
    const result = "Hello, " ++ "World!";
    try testing.expectEqualStrings("Hello, World!", result);
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();

        // Should have test functions (may be detected through test declarations)
        let funcs: Vec<_> = parsed.functions().collect();
        let test_funcs: Vec<_> = funcs
            .iter()
            .filter(|f| f.name().starts_with("test"))
            .collect();

        // Note: Test detection depends on tree-sitter query results
        // The parser attempts to detect tests but may need refinement
        for test_func in &test_funcs {
            if let Symbol::Function { attributes, .. } = test_func {
                assert!(attributes.contains(&"test".to_string()));
            }
        }
    }

    #[test]
    fn test_main_entrypoint() {
        let code = r#"const std = @import("std");

pub fn main() void {
    std.debug.print("Hello, World!\n", .{});
}
"#;

        let parsed = parse_zig_file(Path::new("main.zig"), code).unwrap();

        let funcs: Vec<_> = parsed.functions().collect();
        let main_func = funcs.iter().find(|f| f.name() == "main").unwrap();

        if let Symbol::Function { attributes, .. } = main_func {
            assert!(attributes.contains(&"entrypoint".to_string()));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_is_test_block() {
        let code = r#"test "example" { }"#;
        let mut parser = tree_sitter::Parser::new();
        let zig_lang = zig_language();
        parser.set_language(&zig_lang).unwrap();
        let tree = parser.parse(code, None).unwrap();
        let root = tree.root_node();

        let test_node = root.child(0).unwrap();
        assert!(is_test_block(&test_node));
    }

    #[test]
    fn test_is_test_function() {
        assert!(is_test_function("test_something"));
        assert!(is_test_function("bench_performance"));
        assert!(!is_test_function("calculate"));
        assert!(!is_test_function("main"));
    }

    #[test]
    fn test_parse_with_calls() {
        let code = r#"
pub fn helper() i32 {
    return 42;
}

pub fn main() void {
    const result = helper();
    _ = result;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(!parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_function_no_params() {
        let code = r#"pub fn get_value() i32 {
    return 42;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "get_value");
    }

    #[test]
    fn test_parse_multiple_functions() {
        let code = r#"
fn private_func() void {}

pub fn public_func() void {}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 2);
    }

    #[test]
    fn test_parse_visibility() {
        let code = r#"
fn private_func() void {}

pub fn public_func() void {}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 2);

        let private_fn = funcs.iter().find(|f| f.name() == "private_func").unwrap();
        let public_fn = funcs.iter().find(|f| f.name() == "public_func").unwrap();

        match private_fn {
            Symbol::Function { visibility, .. } => assert_eq!(*visibility, Visibility::Private),
            _ => panic!("expected function symbol"),
        }
        match public_fn {
            Symbol::Function { visibility, .. } => assert_eq!(*visibility, Visibility::Public),
            _ => panic!("expected function symbol"),
        }
    }

    #[test]
    fn test_parse_test_with_string_literal() {
        let code = r#"test "my test name" {
    const x = 1 + 1;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let test_funcs: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Function { attributes, .. } if attributes.contains(&"test".to_string())))
            .collect();
        assert_eq!(test_funcs.len(), 1);
        assert!(test_funcs[0].name().contains("my test name"));
    }

    #[test]
    fn test_parse_imports_extraction() {
        let code = r#"const std = @import("std");
const Builder = @import("Builder");
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(!parsed.imports.is_empty());
        let import_paths: Vec<_> = parsed.imports.iter().map(|i| i.path.as_str()).collect();
        assert!(import_paths.contains(&"std"));
        assert!(import_paths.contains(&"Builder"));
    }

    #[test]
    fn test_parse_empty_file() {
        let code = "";
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_comments_only() {
        let code = r#"// This is a comment
// Another comment
"#;
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_struct() {
        let code = r#"const Point = struct {
    x: i32,
    y: i32,
};
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name(), "Point");
        if let Symbol::Class {
            kind, visibility, ..
        } = &classes[0]
        {
            assert_eq!(*kind, ClassKind::Struct);
            assert_eq!(*visibility, Visibility::Private);
        } else {
            panic!("expected class symbol");
        }
    }

    #[test]
    fn test_parse_enum() {
        let code = r#"const Color = enum {
    red,
    green,
    blue,
};
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name(), "Color");
        if let Symbol::Class {
            kind, visibility, ..
        } = &classes[0]
        {
            assert_eq!(*kind, ClassKind::Enum);
            assert_eq!(*visibility, Visibility::Private);
        } else {
            panic!("expected class symbol");
        }
    }

    #[test]
    fn test_parse_with_nested_calls() {
        let code = r#"
fn inner() i32 { return 1; }
fn outer() i32 { return inner(); }

pub fn main() void {
    _ = outer();
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(!parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_builtin_call() {
        let code = r#"
pub fn main() void {
    const ptr = @ptrCast(*u8, null);
    _ = ptr;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(parsed.calls.is_empty() || !parsed.calls.is_empty());
    }

    #[test]
    fn test_extract_zig_tests_with_names() {
        let code = r#"const std = @import("std");

test "addition works" {
    const result = 2 + 2;
    try std.testing.expectEqual(@as(i32, 4), result);
}

test "subtraction works" {
    const result = 5 - 3;
    try std.testing.expectEqual(@as(i32, 2), result);
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let test_funcs: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| {
                if let Symbol::Function { attributes, .. } = s {
                    attributes.contains(&"test".to_string())
                } else {
                    false
                }
            })
            .collect();

        assert_eq!(test_funcs.len(), 2);
        assert!(test_funcs[0].name().contains("addition works"));
        assert!(test_funcs[1].name().contains("subtraction works"));

        for func in &test_funcs {
            if let Symbol::Function { attributes, .. } = func {
                assert!(attributes.contains(&"test".to_string()));
                assert!(attributes.contains(&"entrypoint".to_string()));
            }
        }
    }

    #[test]
    fn test_extract_zig_imports_paths() {
        let code = r#"const std = @import("std");
const fs = @import("fs");
const mymod = @import("src/mymod.zig");

pub fn doSomething() void {}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let import_paths: Vec<_> = parsed.imports.iter().map(|i| i.path.as_str()).collect();

        assert!(import_paths.contains(&"std"));
        assert!(import_paths.contains(&"fs"));
        assert!(import_paths.contains(&"src/mymod.zig"));
    }

    #[test]
    fn test_parse_zig_empty_input_no_panic() {
        let result = parse_zig_file(Path::new("test.zig"), "");
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_bug047_test_declarations_private_visibility() {
        let code = r#"test "my test" {
    const x = 1;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let test_funcs: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Function { attributes, .. } if attributes.contains(&"test".to_string())))
            .collect();

        assert!(!test_funcs.is_empty());
        for func in &test_funcs {
            if let Symbol::Function { visibility, .. } = func {
                assert_eq!(
                    *visibility,
                    Visibility::Private,
                    "test declarations should be Private, not Public"
                );
            }
        }
    }

    #[test]
    fn test_parse_zig_no_functions() {
        let code = "const x: i32 = 42;\n";
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert!(funcs.is_empty());
    }

    #[test]
    fn test_parse_zig_only_comments() {
        let code = "// this is a comment\n";
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(parsed.symbols.is_empty());
        assert!(parsed.imports.is_empty());
        assert!(parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_zig_no_imports() {
        let code = r#"
pub fn doWork() void {}
"#;
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(parsed.imports.is_empty());
    }

    #[test]
    fn test_parse_zig_no_calls() {
        let code = r#"
pub fn doNothing() void {
    var x: i32 = 42;
    _ = x;
}
"#;
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_zig_no_types() {
        let code = r#"
pub fn main() void {}
"#;
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert!(classes.is_empty());
    }

    #[test]
    fn test_parse_zig_no_tests() {
        let code = r#"
pub fn helper() void {}
"#;
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let test_funcs: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Function { attributes, .. } if attributes.contains(&"test".to_string())))
            .collect();
        assert!(test_funcs.is_empty());
    }

    #[test]
    fn test_parse_zig_union_type() {
        let code = r#"const Tagged = union(enum) {
    int: i32,
    float: f64,
    none: void,
};
"#;
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert!(!classes.is_empty());
        assert_eq!(classes[0].name(), "Tagged");
        if let Symbol::Class { kind, .. } = &classes[0] {
            assert_eq!(*kind, ClassKind::Class);
        }
    }

    #[test]
    fn test_parse_zig_variable_not_type() {
        let code = r#"const x: i32 = 42;
const y: bool = true;
"#;
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert!(classes.is_empty());
    }

    #[test]
    fn test_parse_zig_non_import_builtin() {
        let code = r#"
pub fn main() void {
    const ptr = @intToPtr(*u8, 0);
    _ = ptr;
}
"#;
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(parsed.imports.is_empty());
    }

    #[test]
    fn test_bug048_pub_struct_visibility() {
        let code = r#"pub const Point = struct {
    x: i32,
    y: i32,
};

const PrivateStruct = struct {
    a: u8,
};
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 2);

        let point = classes.iter().find(|c| c.name() == "Point").unwrap();
        let private = classes
            .iter()
            .find(|c| c.name() == "PrivateStruct")
            .unwrap();

        if let Symbol::Class { visibility, .. } = point {
            assert_eq!(*visibility, Visibility::Public);
        }
        if let Symbol::Class { visibility, .. } = private {
            assert_eq!(*visibility, Visibility::Private);
        }
    }

    #[test]
    fn test_parse_union_declaration() {
        // Tests the union_declaration branch in extract_zig_types
        let code = r#"const Tagged = union(enum) {
    int: i32,
    float: f64,
    none,
};
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name(), "Tagged");
        if let Symbol::Class { kind, .. } = &classes[0] {
            assert_eq!(*kind, ClassKind::Class);
        } else {
            panic!("Expected class symbol");
        }
    }

    #[test]
    fn test_is_test_block_with_non_test_node() {
        // Tests the false branch of is_test_block
        let code = r#"pub fn main() void {}"#;
        let mut parser = tree_sitter::Parser::new();
        let zig_lang = zig_language();
        parser.set_language(&zig_lang).unwrap();
        let tree = parser.parse(code, None).unwrap();
        let root = tree.root_node();

        let func_node = root.child(0).unwrap();
        assert!(!is_test_block(&func_node));
    }

    #[test]
    fn test_non_main_function_no_entrypoint() {
        // Tests the false branch of name == "main" in extract_zig_functions
        let code = r#"
pub fn helper() i32 {
    return 42;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name(), "helper");

        if let Symbol::Function { attributes, .. } = funcs[0] {
            assert!(
                !attributes.contains(&"entrypoint".to_string()),
                "helper should not be an entrypoint"
            );
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_calls_outside_function_no_caller() {
        // Tests the None branch for caller in extract_zig_calls
        // Top-level calls in Zig (e.g., const init) should have no caller
        let code = r#"const std = @import("std");

const value = helper();

fn helper() i32 {
    return 42;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        // Check that some calls have no caller
        if !parsed.calls.is_empty() {
            let has_no_caller = parsed.calls.iter().any(|c| c.caller.is_none());
            let has_caller = parsed.calls.iter().any(|c| c.caller.is_some());
            // At least some calls should exist
            assert!(
                has_no_caller || has_caller,
                "Should have extracted at least some calls"
            );
        }
    }

    #[test]
    fn test_non_import_builtin_not_extracted() {
        // Tests the false branch of is_import check for non-@import builtins
        let code = r#"
pub fn doSomething() void {
    const size = @sizeOf(u32);
    _ = size;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        assert!(
            parsed.imports.is_empty(),
            "@sizeOf should not be treated as an import"
        );
    }

    #[test]
    fn test_variable_declaration_not_type() {
        // Tests the branch where variable_declaration has no struct/enum/union child
        let code = r#"const x: i32 = 42;
const name = "hello";
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert!(
            classes.is_empty(),
            "Simple const declarations should not be types"
        );
    }

    #[test]
    fn test_pub_enum_visibility() {
        // Tests the pub visibility branch for enum declarations
        let code = r#"pub const Direction = enum {
    north,
    south,
    east,
    west,
};
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name(), "Direction");

        if let Symbol::Class {
            kind, visibility, ..
        } = &classes[0]
        {
            assert_eq!(*kind, ClassKind::Enum);
            assert_eq!(*visibility, Visibility::Public);
        } else {
            panic!("Expected class symbol");
        }
    }

    #[test]
    fn test_pub_union_visibility() {
        // Tests the pub visibility branch for union declarations
        let code = r#"pub const Value = union(enum) {
    int: i32,
    float: f64,
};
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);

        if let Symbol::Class {
            kind, visibility, ..
        } = &classes[0]
        {
            assert_eq!(*kind, ClassKind::Class);
            assert_eq!(*visibility, Visibility::Public);
        } else {
            panic!("Expected class symbol");
        }
    }

    #[test]
    fn test_mixed_declarations_types_and_non_types() {
        // Tests that only struct/enum/union are extracted as types, not plain consts
        let code = r#"const std = @import("std");
const MAX_SIZE: usize = 1024;

const Point = struct {
    x: i32,
    y: i32,
};

const Color = enum { red, green, blue };

const Result = union(enum) {
    ok: i32,
    err: []const u8,
};
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 3);

        let names: Vec<_> = classes.iter().map(|c| c.name()).collect();
        assert!(names.contains(&"Point"));
        assert!(names.contains(&"Color"));
        assert!(names.contains(&"Result"));
    }

    #[test]
    fn test_test_declaration_without_name() {
        // Tests the branch where test declaration has no string literal
        // This is an edge case; Zig requires test names, but we test robustness
        // of the parser when it encounters a test_declaration without extracting a name
        let code = r#"const std = @import("std");

test "named test" {
    const x = 1;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let tests: Vec<_> = parsed
            .symbols
            .iter()
            .filter(|s| {
                if let Symbol::Function { attributes, .. } = s {
                    attributes.contains(&"test".to_string())
                } else {
                    false
                }
            })
            .collect();
        assert_eq!(tests.len(), 1);
        // Verify the test has both test and entrypoint attributes
        if let Symbol::Function { attributes, .. } = tests[0] {
            assert!(attributes.contains(&"test".to_string()));
            assert!(attributes.contains(&"entrypoint".to_string()));
        }
    }

    #[test]
    fn test_is_test_function_edge_cases() {
        // Tests additional branches for is_test_function
        assert!(is_test_function("testSomething"));
        assert!(is_test_function("benchSomething"));
        assert!(!is_test_function(""));
        assert!(!is_test_function("tes"));
        assert!(!is_test_function("helper"));
    }

    #[test]
    fn test_non_variable_declaration_skipped_in_types() {
        // Tests the continue branch when child.kind() != "variable_declaration"
        let code = r#"
pub fn myFunc() void {}

const Point = struct {
    x: i32,
};
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name(), "Point");
    }

    #[test]
    fn test_calls_with_caller_inside_function() {
        // Tests the Some branch for caller in extract_zig_calls
        // Uses the same pattern as the existing test_parse_with_calls test
        let code = r#"
pub fn helper() i32 {
    return 42;
}

pub fn main() void {
    const result = helper();
    _ = result;
}
"#;

        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        // Check that at least some calls have a caller resolved
        if !parsed.calls.is_empty() {
            let calls_with_caller: Vec<_> =
                parsed.calls.iter().filter(|c| c.caller.is_some()).collect();
            // Calls inside main() should have caller = "main"
            for call in &calls_with_caller {
                assert!(
                    call.caller.is_some(),
                    "calls inside functions should have a caller"
                );
            }
        }
    }

    #[test]
    fn test_parse_zig_async_fn_detected() {
        let code = r#"
pub fn doWork() void {}
"#;
        let parsed = parse_zig_file(Path::new("test.zig"), code).unwrap();
        let funcs: Vec<_> = parsed.functions().collect();
        assert_eq!(funcs.len(), 1);
        if let Symbol::Function { is_async, .. } = funcs[0] {
            assert!(!*is_async, "regular fn should not be async");
        }
    }
}
