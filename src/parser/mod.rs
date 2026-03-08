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
        assert_eq!(detect_language(Path::new("main.go")), Language::Go);
        assert_eq!(detect_language(Path::new("README.md")), Language::Unknown);
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
}
