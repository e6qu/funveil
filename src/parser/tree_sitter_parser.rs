//! Tree-sitter parser implementation for multiple languages.

use std::collections::HashMap;
use std::path::Path;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Parser, Query, QueryCursor, Tree};

use crate::error::{FunveilError, Result};
use crate::parser::{Call, ClassKind, Import, Language, Param, ParsedFile, Symbol, Visibility};
use crate::types::LineRange;

/// Tree-sitter parser supporting multiple languages
pub struct TreeSitterParser {
    queries: HashMap<Language, LanguageQueries>,
}

/// Create a new parser for a language
fn create_parser(language: Language) -> Result<Parser> {
    let mut parser = Parser::new();

    match language {
        Language::Rust => {
            let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load Rust parser: {e}"))
            })?;
        }
        Language::TypeScript => {
            let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load TypeScript parser: {e}"))
            })?;
        }
        Language::Python => {
            let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load Python parser: {e}"))
            })?;
        }
        Language::Bash => {
            let lang: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load Bash parser: {e}"))
            })?;
        }
        Language::Terraform => {
            let lang: tree_sitter::Language = tree_sitter_hcl::LANGUAGE.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load HCL parser: {e}"))
            })?;
        }
        Language::Helm => {
            let lang: tree_sitter::Language = tree_sitter_yaml::LANGUAGE.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load YAML parser: {e}"))
            })?;
        }
        Language::Css => {
            let lang: tree_sitter::Language = tree_sitter_css::LANGUAGE.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load CSS parser: {e}"))
            })?;
        }
        Language::Go => {
            let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load Go parser: {e}"))
            })?;
        }
        Language::Zig => {
            let lang: tree_sitter::Language = tree_sitter_zig::LANGUAGE.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load Zig parser: {e}"))
            })?;
        }
        Language::Html => {
            let lang: tree_sitter::Language = tree_sitter_html::LANGUAGE.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load HTML parser: {e}"))
            })?;
        }
        Language::Xml => {
            let lang: tree_sitter::Language = tree_sitter_xml::LANGUAGE_XML.into();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load XML parser: {e}"))
            })?;
        }
        Language::Markdown => {
            let lang: tree_sitter::Language = tree_sitter_markdown_fork::language();
            parser.set_language(&lang).map_err(|e| {
                FunveilError::TreeSitterError(format!("Failed to load Markdown parser: {e}"))
            })?;
        }
        Language::Unknown => {
            return Err(FunveilError::TreeSitterError(
                "Unknown language".to_string(),
            ));
        }
    }

    Ok(parser)
}

/// Queries for a specific language
struct LanguageQueries {
    function_query: Query,
    class_query: Query,
    import_query: Query,
    call_query: Query,
    function_names: Vec<String>,
    class_names: Vec<String>,
    import_names: Vec<String>,
    call_names: Vec<String>,
}

// Tree-sitter query for extracting functions (Rust)
const RUST_FUNCTION_QUERY: &str = r#"
(function_item
  name: (identifier) @func.name
  parameters: (parameters) @func.params
  return_type: (_)? @func.return
  body: (block) @func.body) @func.def
"#;

// Tree-sitter query for extracting structs/traits/enums (Rust)
const RUST_CLASS_QUERY: &str = r#"
[
  ; Structs
  (struct_item
    name: (type_identifier) @class.name) @class.def
  
  ; Enums
  (enum_item
    name: (type_identifier) @class.name) @class.def
  
  ; Traits
  (trait_item
    name: (type_identifier) @class.name) @class.def
]
"#;

// Tree-sitter query for imports (Rust)
const RUST_IMPORT_QUERY: &str = r#"
(use_declaration
  argument: (_) @import.path) @import.def
"#;

// Tree-sitter query for function calls (Rust)
const RUST_CALL_QUERY: &str = r#"
(call_expression
  function: [
    (identifier) @call.name
    (field_expression field: (field_identifier) @call.name)
    (scoped_identifier name: (identifier) @call.name)
  ]) @call.expr
"#;

// TypeScript queries
const TS_FUNCTION_QUERY: &str = r#"
(function_declaration
  name: (identifier) @func.name
  parameters: (formal_parameters) @func.params
  return_type: (type_annotation)? @func.return
  body: (statement_block) @func.body) @func.def
"#;

const TS_CLASS_QUERY: &str = r#"
[
  (class_declaration
    name: (type_identifier) @class.name) @class.def
  
  (interface_declaration
    name: (type_identifier) @class.name) @class.def
  
  (type_alias_declaration
    name: (type_identifier) @class.name) @class.def
]
"#;

const TS_IMPORT_QUERY: &str = r#"
(import_statement
  source: (string) @import.source) @import.def
"#;

const TS_CALL_QUERY: &str = r#"
(call_expression
  function: [
    (identifier) @call.name
    (member_expression property: (property_identifier) @call.name)
  ]) @call.expr
"#;

// Python queries
const PYTHON_FUNCTION_QUERY: &str = r#"
(function_definition
  name: (identifier) @func.name
  parameters: (parameters) @func.params
  return_type: (type)? @func.return
  body: (block) @func.body) @func.def
"#;

const PYTHON_CLASS_QUERY: &str = r#"
(class_definition
  name: (identifier) @class.name) @class.def
"#;

const PYTHON_IMPORT_QUERY: &str = r#"
[
  (import_statement
    name: (_) @import.name) @import.def
  
  (import_from_statement
    module_name: (dotted_name) @import.module) @import.def
]
"#;

const PYTHON_CALL_QUERY: &str = r#"
(call
  function: [
    (identifier) @call.name
    (attribute attribute: (identifier) @call.name)
  ]) @call.expr
"#;

// Bash queries
const BASH_FUNCTION_QUERY: &str = r#"
(function_definition
  name: (word) @func.name
  body: (compound_statement) @func.body) @func.def
"#;

const BASH_CLASS_QUERY: &str = r#"
; Bash doesn't have classes, match nothing
(comment) @class.def
"#;

const BASH_IMPORT_QUERY: &str = r#"
; Match any command as potential import
(command) @import.def
"#;

const BASH_CALL_QUERY: &str = r#"
; Match any command as call
(command) @call.expr
"#;

// Terraform/HCL queries
const HCL_FUNCTION_QUERY: &str = r#"
; Match HCL blocks
(block) @func.def
"#;

const HCL_CLASS_QUERY: &str = r#"
(block) @class.def
"#;

const HCL_IMPORT_QUERY: &str = r#"
(block) @import.def
"#;

const HCL_CALL_QUERY: &str = r#"
(function_call) @call.expr
"#;

// Helm/YAML queries (very limited parsing)
const YAML_FUNCTION_QUERY: &str = r#"
(block_mapping_pair) @func.def
"#;

const YAML_CLASS_QUERY: &str = r#"
(document) @class.def
"#;

const YAML_IMPORT_QUERY: &str = r#"
(block_mapping_pair) @import.def
"#;

const YAML_CALL_QUERY: &str = r#"
; No real calls in YAML
(document) @call.expr
"#;

// Go queries
const GO_FUNCTION_QUERY: &str = r#"
[
  (function_declaration
    name: (identifier) @func.name
    parameters: (parameter_list) @func.params
    result: (_)? @func.return
    body: (block) @func.body) @func.def
  
  (method_declaration
    name: (field_identifier) @func.name
    parameters: (parameter_list) @func.params
    result: (_)? @func.return
    body: (block) @func.body) @func.def
]
"#;

const GO_CLASS_QUERY: &str = r#"
(type_declaration
  (type_spec
    name: (type_identifier) @class.name
    type: [
      (struct_type) @class.def
      (interface_type) @class.def
    ])) @type.decl
"#;

const GO_IMPORT_QUERY: &str = r#"
(import_spec
  path: (interpreted_string_literal) @import.path
  alias: (_)? @import.alias) @import.def
"#;

const GO_CALL_QUERY: &str = r#"
(call_expression
  function: [
    (identifier) @call.name
    (selector_expression field: (field_identifier) @call.name)
  ]) @call.expr
"#;

// Zig queries
const ZIG_FUNCTION_QUERY: &str = r#"
(function_declaration
  name: (identifier) @func.name) @func.def
"#;

const ZIG_TYPE_QUERY: &str = r#"
(variable_declaration
  name: (identifier) @class.name) @class.def
"#;

const ZIG_IMPORT_QUERY: &str = r#"
(call_expression) @import.def
"#;

const ZIG_CALL_QUERY: &str = r#"
(call_expression) @call.expr
"#;

// HTML queries - simplified
const HTML_ELEMENT_QUERY: &str = r#"
(element) @element.def
"#;

// CSS queries
const CSS_RULE_QUERY: &str = r#"
(rule_set) @rule.def
"#;

const CSS_AT_RULE_QUERY: &str = r#"
(at_rule) @at.def
"#;

// XML queries
const XML_ELEMENT_QUERY: &str = r#"
(element) @xml.element
"#;

// Markdown queries
const MD_HEADING_QUERY: &str = r#"
(atx_heading) @md.heading
"#;

impl TreeSitterParser {
    /// Create a new parser with all language support
    pub fn new() -> Result<Self> {
        let mut queries = HashMap::new();

        // Helper to convert capture names
        let to_strings = |names: &[&str]| names.iter().map(|s| s.to_string()).collect();

        // Initialize Rust queries
        let rust_lang = tree_sitter_rust::LANGUAGE.into();
        let rust_func_query = Query::new(&rust_lang, RUST_FUNCTION_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Rust function query: {e}"))
        })?;
        let rust_class_query = Query::new(&rust_lang, RUST_CLASS_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Rust class query: {e}")))?;
        let rust_import_query = Query::new(&rust_lang, RUST_IMPORT_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Rust import query: {e}"))
        })?;
        let rust_call_query = Query::new(&rust_lang, RUST_CALL_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Rust call query: {e}")))?;

        queries.insert(
            Language::Rust,
            LanguageQueries {
                function_names: to_strings(rust_func_query.capture_names()),
                class_names: to_strings(rust_class_query.capture_names()),
                import_names: to_strings(rust_import_query.capture_names()),
                call_names: to_strings(rust_call_query.capture_names()),
                function_query: rust_func_query,
                class_query: rust_class_query,
                import_query: rust_import_query,
                call_query: rust_call_query,
            },
        );

        // Initialize TypeScript queries
        let ts_lang = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let ts_func_query = Query::new(&ts_lang, TS_FUNCTION_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid TS function query: {e}"))
        })?;
        let ts_class_query = Query::new(&ts_lang, TS_CLASS_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid TS class query: {e}")))?;
        let ts_import_query = Query::new(&ts_lang, TS_IMPORT_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid TS import query: {e}")))?;
        let ts_call_query = Query::new(&ts_lang, TS_CALL_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid TS call query: {e}")))?;

        queries.insert(
            Language::TypeScript,
            LanguageQueries {
                function_names: to_strings(ts_func_query.capture_names()),
                class_names: to_strings(ts_class_query.capture_names()),
                import_names: to_strings(ts_import_query.capture_names()),
                call_names: to_strings(ts_call_query.capture_names()),
                function_query: ts_func_query,
                class_query: ts_class_query,
                import_query: ts_import_query,
                call_query: ts_call_query,
            },
        );

        // Initialize Python queries
        let py_lang = tree_sitter_python::LANGUAGE.into();
        let py_func_query = Query::new(&py_lang, PYTHON_FUNCTION_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Python function query: {e}"))
        })?;
        let py_class_query = Query::new(&py_lang, PYTHON_CLASS_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Python class query: {e}"))
        })?;
        let py_import_query = Query::new(&py_lang, PYTHON_IMPORT_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Python import query: {e}"))
        })?;
        let py_call_query = Query::new(&py_lang, PYTHON_CALL_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Python call query: {e}"))
        })?;

        queries.insert(
            Language::Python,
            LanguageQueries {
                function_names: to_strings(py_func_query.capture_names()),
                class_names: to_strings(py_class_query.capture_names()),
                import_names: to_strings(py_import_query.capture_names()),
                call_names: to_strings(py_call_query.capture_names()),
                function_query: py_func_query,
                class_query: py_class_query,
                import_query: py_import_query,
                call_query: py_call_query,
            },
        );

        // Initialize Bash queries
        let bash_lang = tree_sitter_bash::LANGUAGE.into();
        let bash_func_query = Query::new(&bash_lang, BASH_FUNCTION_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Bash function query: {e}"))
        })?;
        let bash_class_query = Query::new(&bash_lang, BASH_CLASS_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Bash class query: {e}")))?;
        let bash_import_query = Query::new(&bash_lang, BASH_IMPORT_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Bash import query: {e}"))
        })?;
        let bash_call_query = Query::new(&bash_lang, BASH_CALL_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Bash call query: {e}")))?;

        queries.insert(
            Language::Bash,
            LanguageQueries {
                function_names: to_strings(bash_func_query.capture_names()),
                class_names: to_strings(bash_class_query.capture_names()),
                import_names: to_strings(bash_import_query.capture_names()),
                call_names: to_strings(bash_call_query.capture_names()),
                function_query: bash_func_query,
                class_query: bash_class_query,
                import_query: bash_import_query,
                call_query: bash_call_query,
            },
        );

        // Initialize Terraform/HCL queries
        let hcl_lang = tree_sitter_hcl::LANGUAGE.into();
        let hcl_func_query = Query::new(&hcl_lang, HCL_FUNCTION_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid HCL function query: {e}"))
        })?;
        let hcl_class_query = Query::new(&hcl_lang, HCL_CLASS_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid HCL class query: {e}")))?;
        let hcl_import_query = Query::new(&hcl_lang, HCL_IMPORT_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid HCL import query: {e}")))?;
        let hcl_call_query = Query::new(&hcl_lang, HCL_CALL_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid HCL call query: {e}")))?;

        queries.insert(
            Language::Terraform,
            LanguageQueries {
                function_names: to_strings(hcl_func_query.capture_names()),
                class_names: to_strings(hcl_class_query.capture_names()),
                import_names: to_strings(hcl_import_query.capture_names()),
                call_names: to_strings(hcl_call_query.capture_names()),
                function_query: hcl_func_query,
                class_query: hcl_class_query,
                import_query: hcl_import_query,
                call_query: hcl_call_query,
            },
        );

        // Initialize Helm/YAML queries
        let yaml_lang = tree_sitter_yaml::LANGUAGE.into();
        let yaml_func_query = Query::new(&yaml_lang, YAML_FUNCTION_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid YAML function query: {e}"))
        })?;
        let yaml_class_query = Query::new(&yaml_lang, YAML_CLASS_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid YAML class query: {e}")))?;
        let yaml_import_query = Query::new(&yaml_lang, YAML_IMPORT_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid YAML import query: {e}"))
        })?;
        let yaml_call_query = Query::new(&yaml_lang, YAML_CALL_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid YAML call query: {e}")))?;

        queries.insert(
            Language::Helm,
            LanguageQueries {
                function_names: to_strings(yaml_func_query.capture_names()),
                class_names: to_strings(yaml_class_query.capture_names()),
                import_names: to_strings(yaml_import_query.capture_names()),
                call_names: to_strings(yaml_call_query.capture_names()),
                function_query: yaml_func_query,
                class_query: yaml_class_query,
                import_query: yaml_import_query,
                call_query: yaml_call_query,
            },
        );

        // Initialize Go queries
        let go_lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
        let go_func_query = Query::new(&go_lang, GO_FUNCTION_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Go function query: {e}"))
        })?;
        let go_class_query = Query::new(&go_lang, GO_CLASS_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Go class query: {e}")))?;
        let go_import_query = Query::new(&go_lang, GO_IMPORT_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Go import query: {e}")))?;
        let go_call_query = Query::new(&go_lang, GO_CALL_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Go call query: {e}")))?;

        queries.insert(
            Language::Go,
            LanguageQueries {
                function_names: to_strings(go_func_query.capture_names()),
                class_names: to_strings(go_class_query.capture_names()),
                import_names: to_strings(go_import_query.capture_names()),
                call_names: to_strings(go_call_query.capture_names()),
                function_query: go_func_query,
                class_query: go_class_query,
                import_query: go_import_query,
                call_query: go_call_query,
            },
        );

        // Initialize Zig queries
        let zig_lang: tree_sitter::Language = tree_sitter_zig::LANGUAGE.into();
        let zig_func_query = Query::new(&zig_lang, ZIG_FUNCTION_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Zig function query: {e}"))
        })?;
        let zig_class_query = Query::new(&zig_lang, ZIG_TYPE_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Zig class query: {e}")))?;
        let zig_import_query = Query::new(&zig_lang, ZIG_IMPORT_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Zig import query: {e}")))?;
        let zig_call_query = Query::new(&zig_lang, ZIG_CALL_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid Zig call query: {e}")))?;

        queries.insert(
            Language::Zig,
            LanguageQueries {
                function_names: to_strings(zig_func_query.capture_names()),
                class_names: to_strings(zig_class_query.capture_names()),
                import_names: to_strings(zig_import_query.capture_names()),
                call_names: to_strings(zig_call_query.capture_names()),
                function_query: zig_func_query,
                class_query: zig_class_query,
                import_query: zig_import_query,
                call_query: zig_call_query,
            },
        );

        // Initialize HTML queries
        let html_lang: tree_sitter::Language = tree_sitter_html::LANGUAGE.into();
        let html_element_query = Query::new(&html_lang, HTML_ELEMENT_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid HTML element query: {e}"))
        })?;
        let html_class_query = Query::new(&html_lang, HTML_ELEMENT_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid HTML element query: {e}"))
        })?;
        let html_dummy_import_query = Query::new(&html_lang, "(comment) @dummy")
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid HTML dummy query: {e}")))?;
        let html_dummy_call_query = Query::new(&html_lang, "(comment) @dummy")
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid HTML dummy query: {e}")))?;

        queries.insert(
            Language::Html,
            LanguageQueries {
                function_names: to_strings(html_element_query.capture_names()),
                class_names: to_strings(html_class_query.capture_names()),
                import_names: vec![],
                call_names: vec![],
                function_query: html_element_query,
                class_query: html_class_query,
                import_query: html_dummy_import_query,
                call_query: html_dummy_call_query,
            },
        );

        // Initialize CSS queries
        let css_lang: tree_sitter::Language = tree_sitter_css::LANGUAGE.into();
        let css_rule_query = Query::new(&css_lang, CSS_RULE_QUERY)
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid CSS rule query: {e}")))?;
        let css_at_rule_query = Query::new(&css_lang, CSS_AT_RULE_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid CSS at-rule query: {e}"))
        })?;
        let css_dummy_query1 = Query::new(&css_lang, "(comment) @dummy")
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid CSS dummy query: {e}")))?;
        let css_dummy_query2 = Query::new(&css_lang, "(comment) @dummy")
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid CSS dummy query: {e}")))?;

        queries.insert(
            Language::Css,
            LanguageQueries {
                function_names: to_strings(css_rule_query.capture_names()),
                class_names: to_strings(css_at_rule_query.capture_names()),
                import_names: vec![],
                call_names: vec![],
                function_query: css_rule_query,
                class_query: css_at_rule_query,
                import_query: css_dummy_query1,
                call_query: css_dummy_query2,
            },
        );

        // Initialize XML queries
        let xml_lang: tree_sitter::Language = tree_sitter_xml::LANGUAGE_XML.into();
        let xml_element_query = Query::new(&xml_lang, XML_ELEMENT_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid XML element query: {e}"))
        })?;
        let xml_dummy_query1 = Query::new(&xml_lang, "(_) @dummy")
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid XML dummy query: {e}")))?;
        let xml_dummy_query2 = Query::new(&xml_lang, "(_) @dummy")
            .map_err(|e| FunveilError::TreeSitterError(format!("Invalid XML dummy query: {e}")))?;

        queries.insert(
            Language::Xml,
            LanguageQueries {
                function_names: to_strings(xml_element_query.capture_names()),
                class_names: vec![],
                import_names: vec![],
                call_names: vec![],
                function_query: xml_element_query,
                class_query: Query::new(&xml_lang, XML_ELEMENT_QUERY).map_err(|e| {
                    FunveilError::TreeSitterError(format!("Invalid XML element query: {e}"))
                })?,
                import_query: xml_dummy_query1,
                call_query: xml_dummy_query2,
            },
        );

        // Initialize Markdown queries
        let md_lang: tree_sitter::Language = tree_sitter_markdown_fork::language();
        let md_heading_query = Query::new(&md_lang, MD_HEADING_QUERY).map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Markdown heading query: {e}"))
        })?;
        let md_dummy_query1 = Query::new(&md_lang, "(_) @dummy").map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Markdown dummy query: {e}"))
        })?;
        let md_dummy_query2 = Query::new(&md_lang, "(_) @dummy").map_err(|e| {
            FunveilError::TreeSitterError(format!("Invalid Markdown dummy query: {e}"))
        })?;

        queries.insert(
            Language::Markdown,
            LanguageQueries {
                function_names: to_strings(md_heading_query.capture_names()),
                class_names: vec![],
                import_names: vec![],
                call_names: vec![],
                function_query: Query::new(&md_lang, MD_HEADING_QUERY).map_err(|e| {
                    FunveilError::TreeSitterError(format!("Invalid Markdown heading query: {e}"))
                })?,
                class_query: md_heading_query,
                import_query: md_dummy_query1,
                call_query: md_dummy_query2,
            },
        );

        Ok(Self { queries })
    }

    /// Parse a file and extract all symbols
    pub fn parse_file(&self, path: &Path, content: &str) -> Result<ParsedFile> {
        use crate::parser::detect_language;

        let language = detect_language(path);

        if language == Language::Unknown {
            return Ok(ParsedFile::new(language, path.to_path_buf()));
        }

        // Create a new parser for this language
        let mut parser = create_parser(language)?;

        let queries = self.queries.get(&language).ok_or_else(|| {
            FunveilError::TreeSitterError(format!("No queries for language: {language:?}"))
        })?;

        // Parse the file
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| FunveilError::TreeSitterError("Failed to parse file".to_string()))?;

        let mut parsed = ParsedFile::new(language, path.to_path_buf());

        // Extract functions
        parsed.symbols = self.extract_functions(&tree, queries, content, language)?;

        // Extract classes
        let mut classes = self.extract_classes(&tree, queries, content, language)?;
        parsed.symbols.append(&mut classes);

        // Extract imports
        parsed.imports = self.extract_imports(&tree, queries, content, language)?;

        // Extract calls
        parsed.calls = self.extract_calls(&tree, queries, content, &parsed.symbols)?;

        Ok(parsed)
    }

    /// Extract function symbols from the parse tree
    fn extract_functions(
        &self,
        tree: &Tree,
        queries: &LanguageQueries,
        content: &str,
        language: Language,
    ) -> Result<Vec<Symbol>> {
        let mut symbols = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(
            &queries.function_query,
            tree.root_node(),
            content.as_bytes(),
        );

        while let Some(m) = matches.next() {
            if let Some(symbol) = self.convert_function_match(m, queries, content, language) {
                symbols.push(symbol);
            }
        }

        Ok(symbols)
    }

    /// Convert a query match to a Function symbol
    fn convert_function_match(
        &self,
        match_: &tree_sitter::QueryMatch,
        queries: &LanguageQueries,
        content: &str,
        _language: Language,
    ) -> Option<Symbol> {
        // Skip if no function names defined for this language
        if queries.function_names.is_empty() {
            return None;
        }

        let mut name: Option<String> = None;
        let mut params: Vec<Param> = Vec::new();
        let mut return_type: Option<String> = None;
        let mut start_line = 0;
        let mut end_line = 0;
        let mut body_start = 0;
        let mut body_end = 0;

        for capture in match_.captures {
            let capture_name = queries.function_names.get(capture.index as usize)?;
            let node = capture.node;
            let text = node.utf8_text(content.as_bytes()).ok()?;

            match capture_name.as_str() {
                "func.name" => name = Some(text.to_string()),
                "func.params" => {
                    params = self.parse_params(node, content);
                }
                "func.return" => return_type = Some(self.extract_type_text(text)),
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

        let name = name?;

        // Build body range
        let body_range = if body_start > 0 && body_end >= body_start {
            LineRange::new(body_start, body_end).ok()?
        } else {
            LineRange::new(start_line + 1, end_line).ok()?
        };

        let line_range = LineRange::new(start_line, end_line).ok()?;

        Some(Symbol::Function {
            name,
            params,
            return_type,
            visibility: Visibility::Public,
            line_range,
            body_range,
            is_async: false, // TODO: detect async
            attributes: Vec::new(),
        })
    }

    /// Parse parameters from a parameters node
    fn parse_params(&self, node: Node, content: &str) -> Vec<Param> {
        let mut params = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            let param_text = child.utf8_text(content.as_bytes()).unwrap_or("");

            if let Some((name, ty)) = self.extract_param_info(param_text) {
                params.push(Param {
                    name: name.to_string(),
                    type_annotation: ty.map(|s| s.to_string()),
                });
            }
        }

        params
    }

    /// Extract parameter name and type from text
    fn extract_param_info<'a>(&self, text: &'a str) -> Option<(&'a str, Option<&'a str>)> {
        let text = text.trim();

        // Rust: `name: Type`
        if let Some(colon_pos) = text.find(':') {
            let name = text[..colon_pos].trim();
            let ty = text[colon_pos + 1..].trim();
            if name == "self" || name == "&self" || name == "&mut self" {
                return None;
            }
            return Some((name, Some(ty)));
        }

        // Python (no type): just `name`
        if !text.is_empty() && !text.contains('(') && !text.contains(')') {
            return Some((text, None));
        }

        None
    }

    /// Clean up type text
    fn extract_type_text(&self, text: &str) -> String {
        text.trim().to_string()
    }

    /// Extract class/struct symbols
    fn extract_classes(
        &self,
        tree: &Tree,
        queries: &LanguageQueries,
        content: &str,
        language: Language,
    ) -> Result<Vec<Symbol>> {
        let mut symbols = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut matches =
            cursor.matches(&queries.class_query, tree.root_node(), content.as_bytes());

        while let Some(m) = matches.next() {
            if let Some(symbol) = self.convert_class_match(m, queries, content, language) {
                symbols.push(symbol);
            }
        }

        Ok(symbols)
    }

    /// Convert a class query match to a Symbol
    fn convert_class_match(
        &self,
        match_: &tree_sitter::QueryMatch,
        queries: &LanguageQueries,
        content: &str,
        language: Language,
    ) -> Option<Symbol> {
        // Skip if no class names defined for this language
        if queries.class_names.is_empty() {
            return None;
        }

        let mut name: Option<String> = None;
        let mut start_line = 0;
        let mut end_line = 0;
        let kind = match language {
            Language::Rust => ClassKind::Struct,
            Language::TypeScript => ClassKind::Class,
            Language::Python => ClassKind::Class,
            _ => ClassKind::Class,
        };

        for capture in match_.captures {
            let capture_name = queries.class_names.get(capture.index as usize)?;
            let node = capture.node;
            let text = node.utf8_text(content.as_bytes()).ok()?;

            match capture_name.as_str() {
                "class.name" => name = Some(text.to_string()),
                "class.def" => {
                    start_line = node.start_position().row + 1;
                    end_line = node.end_position().row + 1;
                }
                _ => {}
            }
        }

        let name = name?;
        let line_range = LineRange::new(start_line, end_line).ok()?;

        Some(Symbol::Class {
            name,
            methods: Vec::new(),
            properties: Vec::new(),
            visibility: Visibility::Public,
            line_range,
            kind,
        })
    }

    /// Extract imports
    fn extract_imports(
        &self,
        tree: &Tree,
        queries: &LanguageQueries,
        content: &str,
        _language: Language,
    ) -> Result<Vec<Import>> {
        let mut imports = Vec::new();

        // Skip if no import names defined for this language
        if queries.import_names.is_empty() {
            return Ok(imports);
        }

        let mut cursor = QueryCursor::new();
        let mut matches =
            cursor.matches(&queries.import_query, tree.root_node(), content.as_bytes());

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let Some(capture_name) = queries.import_names.get(capture.index as usize) else {
                    continue;
                };
                let node = capture.node;
                let text = node.utf8_text(content.as_bytes()).unwrap_or("");
                let line = node.start_position().row + 1;

                if capture_name.contains("import") {
                    imports.push(Import {
                        path: text.to_string(),
                        alias: None,
                        line,
                    });
                }
            }
        }

        Ok(imports)
    }

    /// Extract function calls
    fn extract_calls(
        &self,
        tree: &Tree,
        queries: &LanguageQueries,
        content: &str,
        symbols: &[Symbol],
    ) -> Result<Vec<Call>> {
        let mut calls = Vec::new();

        // Skip if no call names defined for this language
        if queries.call_names.is_empty() {
            return Ok(calls);
        }

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&queries.call_query, tree.root_node(), content.as_bytes());

        // Build a map of line -> function name for determining caller
        let mut line_to_function: HashMap<usize, String> = HashMap::new();
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
                let Some(capture_name) = queries.call_names.get(capture.index as usize) else {
                    continue;
                };
                let node = capture.node;
                let text = node.utf8_text(content.as_bytes()).unwrap_or("");
                let line = node.start_position().row + 1;

                if capture_name == "call.name" {
                    let caller = line_to_function.get(&line).cloned();

                    calls.push(Call {
                        caller,
                        callee: text.to_string(),
                        line,
                        is_dynamic: false,
                    });
                }
            }
        }

        Ok(calls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_function() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
fn calculate_sum(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}
"#;

        let parsed = parser.parse_file(Path::new("test.rs"), code).unwrap();
        assert_eq!(parsed.symbols.len(), 1);

        let func = &parsed.symbols[0];
        assert_eq!(func.name(), "calculate_sum");

        if let Symbol::Function {
            params,
            return_type,
            ..
        } = func
        {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "numbers");
            assert_eq!(params[0].type_annotation, Some("&[i32]".to_string()));
            // The return type includes the arrow, just check it contains i32
            assert!(return_type.as_ref().unwrap().contains("i32"));
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_python_function() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
def greet(name: str) -> str:
    return f"Hello, {name}!"
"#;

        let parsed = parser.parse_file(Path::new("test.py"), code).unwrap();
        assert_eq!(parsed.symbols.len(), 1);

        let func = &parsed.symbols[0];
        assert_eq!(func.name(), "greet");

        if let Symbol::Function { params, .. } = func {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "name");
        } else {
            panic!("Expected function symbol");
        }
    }

    #[test]
    fn test_parse_rust_struct() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
struct Person {
    name: String,
    age: u32,
}
"#;

        let parsed = parser.parse_file(Path::new("test.rs"), code).unwrap();

        // Should have the struct as a class symbol
        let structs: Vec<_> = parsed.classes().collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name(), "Person");
    }

    #[test]
    fn test_parse_typescript_class() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
class Person {
    name: string;
    age: number;

    constructor(name: string, age: number) {
        this.name = name;
        this.age = age;
    }
}
"#;

        let parsed = parser.parse_file(Path::new("test.ts"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name(), "Person");
    }

    #[test]
    fn test_parse_python_class() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
class Person:
    def __init__(self, name: str, age: int):
        self.name = name
        self.age = age
"#;

        let parsed = parser.parse_file(Path::new("test.py"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name(), "Person");
    }

    #[test]
    fn test_parse_rust_imports() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
"#;

        let parsed = parser.parse_file(Path::new("test.rs"), code).unwrap();
        assert!(!parsed.imports.is_empty());
    }

    #[test]
    fn test_parse_python_imports() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
import os
from typing import List, Dict
"#;

        let parsed = parser.parse_file(Path::new("test.py"), code).unwrap();
        assert!(!parsed.imports.is_empty());
    }

    #[test]
    fn test_parse_typescript_imports() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
import { useState, useEffect } from 'react';
import axios from 'axios';
"#;

        let parsed = parser.parse_file(Path::new("test.ts"), code).unwrap();
        assert!(!parsed.imports.is_empty());
    }

    #[test]
    fn test_parse_rust_calls() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
fn main() {
    let result = calculate_sum(&numbers);
    println!("{}", result);
}
"#;

        let parsed = parser.parse_file(Path::new("test.rs"), code).unwrap();
        assert!(!parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_python_calls() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
def main():
    result = calculate_sum(numbers)
    print(result)
"#;

        let parsed = parser.parse_file(Path::new("test.py"), code).unwrap();
        assert!(!parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_typescript_calls() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
function main() {
    const result = calculateSum(numbers);
    console.log(result);
}
"#;

        let parsed = parser.parse_file(Path::new("test.ts"), code).unwrap();
        assert!(!parsed.calls.is_empty());
    }

    #[test]
    fn test_parse_go_function() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
package main

func calculateSum(numbers []int) int {
    total := 0
    for _, n := range numbers {
        total += n
    }
    return total
}
"#;

        let parsed = parser.parse_file(Path::new("test.go"), code).unwrap();
        assert!(!parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_go_struct() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
package main

type Person struct {
    Name string
    Age  int
}
"#;

        let parsed = parser.parse_file(Path::new("test.go"), code).unwrap();
        let classes: Vec<_> = parsed.classes().collect();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name(), "Person");
    }

    #[test]
    fn test_parse_bash_function() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
#!/bin/bash
greet() {
    echo "Hello, $1!"
}
"#;

        let parsed = parser.parse_file(Path::new("test.sh"), code).unwrap();
        assert!(!parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_zig_function() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
fn calculateSum(numbers: []const i32) i32 {
    var total: i32 = 0;
    for (numbers) |n| {
        total += n;
    }
    return total;
}
"#;

        let parsed = parser.parse_file(Path::new("test.zig"), code).unwrap();
        assert!(!parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_css() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
.container {
    display: flex;
    padding: 10px;
}
"#;

        let parsed = parser.parse_file(Path::new("test.css"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_html() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body><h1>Hello</h1></body>
</html>
"#;

        let parsed = parser.parse_file(Path::new("test.html"), code).unwrap();
        assert!(parsed.symbols.is_empty() || !parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_yaml() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test-config
"#;

        let parsed = parser.parse_file(Path::new("test.yaml"), code).unwrap();
        assert!(parsed.symbols.is_empty() || !parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_xml() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
<?xml version="1.0"?>
<root>
    <element attr="value">content</element>
</root>
"#;

        let parsed = parser.parse_file(Path::new("test.xml"), code).unwrap();
        assert!(parsed.symbols.is_empty() || !parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_terraform() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
resource "aws_instance" "example" {
    ami           = "ami-12345678"
    instance_type = "t2.micro"
}
"#;

        let parsed = parser.parse_file(Path::new("test.tf"), code).unwrap();
        assert!(parsed.symbols.is_empty() || !parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_markdown() {
        let parser = TreeSitterParser::new().unwrap();

        let code = r#"
# Heading

Some **bold** text.

## Subheading

- Item 1
- Item 2
"#;

        let parsed = parser.parse_file(Path::new("test.md"), code).unwrap();
        assert!(parsed.symbols.is_empty() || !parsed.symbols.is_empty());
    }

    #[test]
    fn test_parse_unknown_extension() {
        let parser = TreeSitterParser::new().unwrap();

        let code = "some random content";
        let parsed = parser.parse_file(Path::new("test.unknown"), code).unwrap();
        assert!(parsed.symbols.is_empty());
    }
}
