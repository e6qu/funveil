//! Tree-sitter based code parsing for intelligent veiling.
//!
//! This module provides language-aware parsing using Tree-sitter,
//! enabling funveil to understand code structure (functions, classes,
//! imports, calls) for smart veiling operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::types::LineRange;

mod tree_sitter_parser;
pub use tree_sitter_parser::TreeSitterParser;

/// Language-specific parsers
pub mod languages;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    TypeScript,
    Python,
    Bash,
    Terraform, // Also covers Terragrunt (HCL)
    Helm,      // YAML-based Helm charts
    Go,        // Go language
    Zig,       // Zig language
    Html,      // HTML markup
    Css,       // CSS / SCSS / TailwindCSS
    Xml,       // XML documents
    Markdown,  // Markdown documentation
    Unknown,
}

impl Language {
    /// Get the file extensions associated with this language
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["rs"],
            Language::TypeScript => &["ts", "tsx"],
            Language::Python => &["py", "pyi"],
            Language::Bash => &["sh", "bash"],
            Language::Terraform => &["tf", "tfvars", "hcl"],
            Language::Helm => &["yaml", "yml"], // Helm uses YAML (values.yaml, Chart.yaml)
            Language::Go => &["go"],
            Language::Zig => &["zig"],
            Language::Html => &["html", "htm"],
            Language::Css => &["css", "scss", "sass"],
            Language::Xml => &["xml"],
            Language::Markdown => &["md", "markdown", "mdown", "mkd"],
            Language::Unknown => &[],
        }
    }

    /// Get a display name for the language
    pub fn name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::TypeScript => "TypeScript",
            Language::Python => "Python",
            Language::Bash => "Bash/Shell",
            Language::Terraform => "Terraform/HCL",
            Language::Helm => "Helm/YAML",
            Language::Go => "Go",
            Language::Zig => "Zig",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Xml => "XML",
            Language::Markdown => "Markdown",
            Language::Unknown => "Unknown",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Detect the programming language of a file based on its extension
pub fn detect_language(path: &Path) -> Language {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => Language::Rust,
        Some("ts") | Some("tsx") => Language::TypeScript,
        Some("py") | Some("pyi") => Language::Python,
        Some("sh") | Some("bash") => Language::Bash,
        Some("tf") | Some("tfvars") | Some("hcl") => Language::Terraform,
        Some("yaml") | Some("yml") => Language::Helm,
        Some("go") => Language::Go,
        Some("zig") => Language::Zig,
        Some("html") | Some("htm") => Language::Html,
        Some("css") | Some("scss") | Some("sass") => Language::Css,
        Some("xml") => Language::Xml,
        Some("md") | Some("markdown") | Some("mdown") | Some("mkd") => Language::Markdown,
        _ => Language::Unknown,
    }
}

/// A function or method parameter
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub type_annotation: Option<String>,
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref ty) = self.type_annotation {
            write!(f, "{}: {}", self.name, ty)
        } else {
            write!(f, "{}", self.name)
        }
    }
}

/// A class/trait/struct property or field
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    pub name: String,
    pub type_annotation: Option<String>,
    pub visibility: Visibility,
}

/// Visibility modifier (public, private, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Visibility {
    #[default]
    Private,
    Public,
    Protected,
    Internal, // For languages like C#
}

/// A symbol extracted from source code (function, class, trait, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Symbol {
    /// Function or method
    Function {
        name: String,
        params: Vec<Param>,
        return_type: Option<String>,
        visibility: Visibility,
        line_range: LineRange,
        body_range: LineRange,
        is_async: bool,
        attributes: Vec<String>, // e.g., #[test], @decorator
    },
    /// Class, struct, or interface
    Class {
        name: String,
        methods: Vec<Symbol>,
        properties: Vec<Property>,
        visibility: Visibility,
        line_range: LineRange,
        kind: ClassKind, // class, struct, trait, interface
    },
    /// Module or namespace
    Module { name: String, line_range: LineRange },
}

/// Kind of class-like construct
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassKind {
    Class,
    Struct,
    Trait,
    Interface,
    Enum,
}

impl Symbol {
    /// Get the name of the symbol
    pub fn name(&self) -> &str {
        match self {
            Symbol::Function { name, .. } => name,
            Symbol::Class { name, .. } => name,
            Symbol::Module { name, .. } => name,
        }
    }

    /// Get the line range of the symbol
    pub fn line_range(&self) -> LineRange {
        match self {
            Symbol::Function { line_range, .. } => *line_range,
            Symbol::Class { line_range, .. } => *line_range,
            Symbol::Module { line_range, .. } => *line_range,
        }
    }

    /// Check if this symbol has a specific attribute
    pub fn has_attribute(&self, attr: &str) -> bool {
        match self {
            Symbol::Function { attributes, .. } => attributes.iter().any(|a| a.contains(attr)),
            _ => false,
        }
    }

    /// Get the full signature as a string (for display)
    pub fn signature(&self) -> String {
        match self {
            Symbol::Function {
                name,
                params,
                return_type,
                is_async,
                ..
            } => {
                let mut sig = String::new();
                if *is_async {
                    sig.push_str("async ");
                }
                sig.push_str(&format!("fn {name}("));
                sig.push_str(
                    &params
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                sig.push(')');
                if let Some(ref ret) = return_type {
                    sig.push_str(&format!(" -> {ret}"));
                }
                sig
            }
            Symbol::Class { name, kind, .. } => {
                format!("{kind:?} {name}")
            }
            Symbol::Module { name, .. } => {
                format!("mod {name}")
            }
        }
    }
}

/// An import/use statement
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    pub path: String,
    pub alias: Option<String>,
    pub line: usize,
}

/// A function call
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub caller: Option<String>, // Function containing this call
    pub callee: String,         // Function being called
    pub line: usize,
    pub is_dynamic: bool, // true for callbacks, function pointers, trait objects
}

/// A parsed source file with extracted symbols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedFile {
    pub language: Language,
    pub path: PathBuf,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub calls: Vec<Call>,
}

impl ParsedFile {
    /// Create a new empty parsed file
    pub fn new(language: Language, path: PathBuf) -> Self {
        Self {
            language,
            path,
            symbols: Vec::new(),
            imports: Vec::new(),
            calls: Vec::new(),
        }
    }

    /// Find a symbol by name
    pub fn find_symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.name() == name)
    }

    /// Get all functions in the file
    pub fn functions(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Function { .. }))
    }

    /// Get all classes/structs in the file
    pub fn classes(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols
            .iter()
            .filter(|s| matches!(s, Symbol::Class { .. }))
    }

    /// Find all calls made by a specific function
    pub fn calls_by(&self, function_name: &str) -> Vec<&Call> {
        self.calls
            .iter()
            .filter(|c| c.caller.as_deref() == Some(function_name))
            .collect()
    }
}

/// Index of all symbols across the codebase for cross-file analysis
#[derive(Debug, Default)]
pub struct CodeIndex {
    /// All parsed files
    pub files: HashMap<PathBuf, ParsedFile>,
    /// Symbol name -> locations (for resolving calls)
    pub symbol_table: HashMap<String, Vec<SymbolLocation>>,
}

/// Location of a symbol in the codebase
#[derive(Debug, Clone)]
pub struct SymbolLocation {
    pub file: PathBuf,
    pub line_range: LineRange,
    pub symbol: Symbol,
}

impl CodeIndex {
    /// Build an index from a collection of parsed files
    pub fn build(files: HashMap<PathBuf, ParsedFile>) -> Self {
        let mut symbol_table: HashMap<String, Vec<SymbolLocation>> = HashMap::new();

        for (path, file) in &files {
            for symbol in &file.symbols {
                // Use unqualified name for lookup
                let name = symbol.name().to_string();
                let location = SymbolLocation {
                    file: path.clone(),
                    line_range: symbol.line_range(),
                    symbol: symbol.clone(),
                };
                symbol_table.entry(name).or_default().push(location);
            }
        }

        Self {
            files,
            symbol_table,
        }
    }

    /// Find all locations of a symbol by name
    pub fn find_symbol(&self, name: &str) -> Option<&Vec<SymbolLocation>> {
        // Try exact match first
        if let Some(locations) = self.symbol_table.get(name) {
            return Some(locations);
        }

        // Try unqualified name (last segment)
        let unqualified = name.split("::").last()?;
        self.symbol_table.get(unqualified)
    }

    /// Get the total number of symbols indexed
    pub fn symbol_count(&self) -> usize {
        self.symbol_table.values().map(|v| v.len()).sum()
    }

    /// Get the total number of files indexed
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language(Path::new("main.rs")), Language::Rust);
        assert_eq!(detect_language(Path::new("lib.ts")), Language::TypeScript);
        assert_eq!(detect_language(Path::new("app.tsx")), Language::TypeScript);
        assert_eq!(detect_language(Path::new("script.py")), Language::Python);
        assert_eq!(detect_language(Path::new("script.pyi")), Language::Python);
        assert_eq!(detect_language(Path::new("run.sh")), Language::Bash);
        assert_eq!(detect_language(Path::new("setup.bash")), Language::Bash);
        assert_eq!(detect_language(Path::new("main.tf")), Language::Terraform);
        assert_eq!(
            detect_language(Path::new("vars.tfvars")),
            Language::Terraform
        );
        assert_eq!(
            detect_language(Path::new("config.hcl")),
            Language::Terraform
        );
        assert_eq!(detect_language(Path::new("values.yaml")), Language::Helm);
        assert_eq!(detect_language(Path::new("Chart.yml")), Language::Helm);
        assert_eq!(detect_language(Path::new("main.go")), Language::Go);
        assert_eq!(detect_language(Path::new("main.zig")), Language::Zig);
        assert_eq!(detect_language(Path::new("index.html")), Language::Html);
        assert_eq!(detect_language(Path::new("old.htm")), Language::Html);
        assert_eq!(detect_language(Path::new("styles.css")), Language::Css);
        assert_eq!(detect_language(Path::new("app.scss")), Language::Css);
        assert_eq!(detect_language(Path::new("vars.sass")), Language::Css);
        assert_eq!(detect_language(Path::new("data.xml")), Language::Xml);
        assert_eq!(detect_language(Path::new("README.md")), Language::Markdown);
        assert_eq!(
            detect_language(Path::new("docs.markdown")),
            Language::Markdown
        );
        assert_eq!(
            detect_language(Path::new("guide.mdown")),
            Language::Markdown
        );
        assert_eq!(detect_language(Path::new("doc.mkd")), Language::Markdown);
        assert_eq!(detect_language(Path::new("README.txt")), Language::Unknown);
    }

    #[test]
    fn test_language_extensions() {
        assert_eq!(Language::Rust.extensions(), &["rs"]);
        assert!(Language::TypeScript.extensions().contains(&"ts"));
        assert!(Language::TypeScript.extensions().contains(&"tsx"));
        assert!(Language::Python.extensions().contains(&"py"));
        assert!(Language::Bash.extensions().contains(&"sh"));
        assert!(Language::Terraform.extensions().contains(&"tf"));
        assert!(Language::Helm.extensions().contains(&"yaml"));
        assert_eq!(Language::Go.extensions(), &["go"]);
        assert_eq!(Language::Zig.extensions(), &["zig"]);
        assert!(Language::Html.extensions().contains(&"html"));
        assert!(Language::Css.extensions().contains(&"scss"));
        assert_eq!(Language::Xml.extensions(), &["xml"]);
        assert!(Language::Markdown.extensions().contains(&"md"));
        assert!(Language::Unknown.extensions().is_empty());
    }

    #[test]
    fn test_language_name() {
        assert_eq!(Language::Rust.name(), "Rust");
        assert_eq!(Language::TypeScript.name(), "TypeScript");
        assert_eq!(Language::Python.name(), "Python");
        assert_eq!(Language::Bash.name(), "Bash/Shell");
        assert_eq!(Language::Terraform.name(), "Terraform/HCL");
        assert_eq!(Language::Helm.name(), "Helm/YAML");
        assert_eq!(Language::Go.name(), "Go");
        assert_eq!(Language::Zig.name(), "Zig");
        assert_eq!(Language::Html.name(), "HTML");
        assert_eq!(Language::Css.name(), "CSS");
        assert_eq!(Language::Xml.name(), "XML");
        assert_eq!(Language::Markdown.name(), "Markdown");
        assert_eq!(Language::Unknown.name(), "Unknown");
    }

    #[test]
    fn test_language_display() {
        assert_eq!(format!("{}", Language::Rust), "Rust");
        assert_eq!(format!("{}", Language::Python), "Python");
        assert_eq!(format!("{}", Language::Unknown), "Unknown");
    }

    #[test]
    fn test_param_display() {
        let param_with_type = Param {
            name: "count".to_string(),
            type_annotation: Some("i32".to_string()),
        };
        assert_eq!(format!("{param_with_type}"), "count: i32");

        let param_no_type = Param {
            name: "value".to_string(),
            type_annotation: None,
        };
        assert_eq!(format!("{param_no_type}"), "value");
    }

    #[test]
    fn test_symbol_name() {
        let func = Symbol::Function {
            name: "test".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Private,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        };
        assert_eq!(func.name(), "test");

        let class = Symbol::Class {
            name: "MyClass".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 10).unwrap(),
            kind: ClassKind::Class,
        };
        assert_eq!(class.name(), "MyClass");

        let module = Symbol::Module {
            name: "utils".to_string(),
            line_range: LineRange::new(1, 20).unwrap(),
        };
        assert_eq!(module.name(), "utils");
    }

    #[test]
    fn test_symbol_line_range() {
        let func = Symbol::Function {
            name: "test".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Private,
            line_range: LineRange::new(5, 15).unwrap(),
            body_range: LineRange::new(6, 15).unwrap(),
            is_async: false,
            attributes: vec![],
        };
        assert_eq!(func.line_range(), LineRange::new(5, 15).unwrap());

        let class = Symbol::Class {
            name: "MyClass".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(10, 50).unwrap(),
            kind: ClassKind::Class,
        };
        assert_eq!(class.line_range(), LineRange::new(10, 50).unwrap());

        let module = Symbol::Module {
            name: "utils".to_string(),
            line_range: LineRange::new(1, 100).unwrap(),
        };
        assert_eq!(module.line_range(), LineRange::new(1, 100).unwrap());
    }

    #[test]
    fn test_symbol_has_attribute() {
        let func_with_test = Symbol::Function {
            name: "test_fn".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Private,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec!["test".to_string(), "should_panic".to_string()],
        };
        assert!(func_with_test.has_attribute("test"));
        assert!(func_with_test.has_attribute("panic"));
        assert!(!func_with_test.has_attribute("ignore"));

        let class = Symbol::Class {
            name: "MyClass".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 10).unwrap(),
            kind: ClassKind::Class,
        };
        assert!(!class.has_attribute("test"));
    }

    #[test]
    fn test_symbol_signature_class_and_module() {
        let class = Symbol::Class {
            name: "User".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 10).unwrap(),
            kind: ClassKind::Struct,
        };
        assert_eq!(class.signature(), "Struct User");

        let iface = Symbol::Class {
            name: "Reader".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 10).unwrap(),
            kind: ClassKind::Interface,
        };
        assert_eq!(iface.signature(), "Interface Reader");

        let module = Symbol::Module {
            name: "network".to_string(),
            line_range: LineRange::new(1, 100).unwrap(),
        };
        assert_eq!(module.signature(), "mod network");
    }

    #[test]
    fn test_parsed_file_new() {
        let file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        assert_eq!(file.language, Language::Rust);
        assert!(file.symbols.is_empty());
        assert!(file.imports.is_empty());
        assert!(file.calls.is_empty());
    }

    #[test]
    fn test_parsed_file_find_symbol() {
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.symbols.push(Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        file.symbols.push(Symbol::Function {
            name: "helper".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Private,
            line_range: LineRange::new(7, 10).unwrap(),
            body_range: LineRange::new(8, 10).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        assert!(file.find_symbol("main").is_some());
        assert!(file.find_symbol("helper").is_some());
        assert!(file.find_symbol("nonexistent").is_none());
    }

    #[test]
    fn test_parsed_file_functions() {
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.symbols.push(Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        file.symbols.push(Symbol::Class {
            name: "MyClass".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(7, 20).unwrap(),
            kind: ClassKind::Class,
        });
        file.symbols.push(Symbol::Function {
            name: "helper".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Private,
            line_range: LineRange::new(22, 30).unwrap(),
            body_range: LineRange::new(23, 30).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let functions: Vec<_> = file.functions().collect();
        assert_eq!(functions.len(), 2);
    }

    #[test]
    fn test_parsed_file_classes() {
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.symbols.push(Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        file.symbols.push(Symbol::Class {
            name: "MyStruct".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(7, 20).unwrap(),
            kind: ClassKind::Struct,
        });
        file.symbols.push(Symbol::Class {
            name: "MyTrait".to_string(),
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(22, 30).unwrap(),
            kind: ClassKind::Trait,
        });

        let classes: Vec<_> = file.classes().collect();
        assert_eq!(classes.len(), 2);
    }

    #[test]
    fn test_parsed_file_calls_by() {
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.calls.push(Call {
            caller: Some("main".to_string()),
            callee: "helper".to_string(),
            line: 3,
            is_dynamic: false,
        });
        file.calls.push(Call {
            caller: Some("main".to_string()),
            callee: "process".to_string(),
            line: 4,
            is_dynamic: false,
        });
        file.calls.push(Call {
            caller: Some("helper".to_string()),
            callee: "internal".to_string(),
            line: 10,
            is_dynamic: true,
        });

        let main_calls = file.calls_by("main");
        assert_eq!(main_calls.len(), 2);

        let helper_calls = file.calls_by("helper");
        assert_eq!(helper_calls.len(), 1);

        let none_calls = file.calls_by("nonexistent");
        assert!(none_calls.is_empty());
    }

    #[test]
    fn test_code_index_build() {
        let mut files = HashMap::new();
        let mut file1 = ParsedFile::new(Language::Rust, PathBuf::from("main.rs"));
        file1.symbols.push(Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        files.insert(PathBuf::from("main.rs"), file1);

        let index = CodeIndex::build(files);
        assert_eq!(index.file_count(), 1);
        assert_eq!(index.symbol_count(), 1);
    }

    #[test]
    fn test_code_index_find_symbol() {
        let mut files = HashMap::new();
        let mut file1 = ParsedFile::new(Language::Rust, PathBuf::from("main.rs"));
        file1.symbols.push(Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        files.insert(PathBuf::from("main.rs"), file1);

        let index = CodeIndex::build(files);
        assert!(index.find_symbol("main").is_some());
        assert!(index.find_symbol("nonexistent").is_none());
    }

    #[test]
    fn test_code_index_find_symbol_qualified() {
        let mut files = HashMap::new();
        let mut file1 = ParsedFile::new(Language::Rust, PathBuf::from("lib.rs"));
        file1.symbols.push(Symbol::Function {
            name: "helper".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        files.insert(PathBuf::from("lib.rs"), file1);

        let index = CodeIndex::build(files);
        assert!(index.find_symbol("crate::module::helper").is_some());
    }

    #[test]
    fn test_symbol_signature() {
        let func = Symbol::Function {
            name: "calculate_sum".to_string(),
            params: vec![
                Param {
                    name: "numbers".to_string(),
                    type_annotation: Some("&[i32]".to_string()),
                },
                Param {
                    name: "offset".to_string(),
                    type_annotation: Some("i32".to_string()),
                },
            ],
            return_type: Some("i32".to_string()),
            visibility: Visibility::Public,
            line_range: LineRange::new(10, 20).unwrap(),
            body_range: LineRange::new(11, 20).unwrap(),
            is_async: false,
            attributes: vec![],
        };

        assert_eq!(
            func.signature(),
            "fn calculate_sum(numbers: &[i32], offset: i32) -> i32"
        );

        let async_func = Symbol::Function {
            name: "fetch_data".to_string(),
            params: vec![Param {
                name: "url".to_string(),
                type_annotation: Some("&str".to_string()),
            }],
            return_type: Some("Result<String>".to_string()),
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 10).unwrap(),
            body_range: LineRange::new(2, 10).unwrap(),
            is_async: true,
            attributes: vec![],
        };

        assert_eq!(
            async_func.signature(),
            "async fn fetch_data(url: &str) -> Result<String>"
        );
    }

    #[test]
    fn test_symbol_signature_no_params_no_return() {
        let func = Symbol::Function {
            name: "simple".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Private,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        };
        assert_eq!(func.signature(), "fn simple()");
    }
}
