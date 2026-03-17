//! Header-only veiling strategy.
//!
//! Shows only function/class signatures, hiding implementations.
//! This is useful for getting an overview of a codebase's public API.

use crate::error::Result;
use crate::parser::{ClassKind, ParsedFile, Symbol};
use crate::strategies::{get_lines, VeilStrategy};

/// Configuration for header mode veiling
#[derive(Debug, Clone)]
pub struct HeaderConfig {
    /// Include docstrings in output
    pub include_docstrings: bool,
    /// Max length for signatures (truncate if longer)
    pub max_signature_length: Option<usize>,
    /// Show class methods
    pub show_methods: bool,
    /// Show class properties
    pub show_properties: bool,
}

impl Default for HeaderConfig {
    fn default() -> Self {
        Self {
            include_docstrings: true,
            max_signature_length: None,
            show_methods: true,
            show_properties: false,
        }
    }
}

/// Find the largest byte index <= `index` that is a valid char boundary.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Veiling strategy that shows only signatures
pub struct HeaderStrategy {
    config: HeaderConfig,
}

impl HeaderStrategy {
    /// Create a new header strategy with default config
    pub fn new() -> Self {
        Self {
            config: HeaderConfig::default(),
        }
    }

    /// Create a new header strategy with custom config
    pub fn with_config(config: HeaderConfig) -> Self {
        Self { config }
    }

    /// Format a function signature for display
    fn format_function(
        &self,
        symbol: &Symbol,
        content: &str,
        language: &crate::parser::Language,
    ) -> String {
        let Symbol::Function {
            name,
            params,
            return_type,
            is_async,
            line_range,
            body_range,
            ..
        } = symbol
        else {
            return String::new();
        };

        // Get the signature (everything before the body)
        let signature_lines = if body_range.start() > line_range.start() {
            get_lines(content, line_range.start(), body_range.start() - 1)
        } else {
            // Fallback: construct from parts
            self.build_signature(name, params, return_type, *is_async)
        };

        // Truncate if needed
        if let Some(configured_max) = self.config.max_signature_length {
            let max_len = configured_max.max(3);
            if signature_lines.len() > max_len {
                let boundary = floor_char_boundary(&signature_lines, max_len.saturating_sub(3));
                return format!("{}...", &signature_lines[..boundary]);
            }
        }

        // Add placeholder for body — language-appropriate syntax
        let body_lines = body_range.len();
        let sig = signature_lines.trim_end();
        if matches!(language, crate::parser::Language::Python) {
            format!("{sig}\n    ...  # {body_lines} lines hidden\n")
        } else {
            format!("{sig} {{ ... {body_lines} lines ... }}\n")
        }
    }

    /// Build a signature from parts when we can't extract from source
    fn build_signature(
        &self,
        name: &str,
        params: &[crate::parser::Param],
        return_type: &Option<String>,
        is_async: bool,
    ) -> String {
        let mut sig = String::new();

        if is_async {
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

        if let Some(ret) = return_type {
            sig.push_str(&format!(" -> {ret}"));
        }

        sig
    }

    /// Format a class/struct for display
    fn format_class(
        &self,
        symbol: &Symbol,
        _content: &str,
        language: &crate::parser::Language,
    ) -> String {
        let Symbol::Class {
            name,
            kind,
            methods,
            properties,
            line_range,
            ..
        } = symbol
        else {
            return String::new();
        };

        let mut result = String::new();

        // Class declaration line
        let kind_str = match kind {
            ClassKind::Class => "class",
            ClassKind::Struct => "struct",
            ClassKind::Trait => "trait",
            ClassKind::Interface => "interface",
            ClassKind::Enum => "enum",
        };

        let is_python = matches!(language, crate::parser::Language::Python);
        if is_python {
            result.push_str(&format!("{kind_str} {name}:\n"));
        } else {
            result.push_str(&format!("{kind_str} {name} {{\n"));
        }

        // Properties (if enabled)
        if self.config.show_properties {
            for prop in properties {
                if let Some(ref ty) = prop.type_annotation {
                    result.push_str(&format!("    {}: {},\n", prop.name, ty));
                } else {
                    result.push_str(&format!("    {},\n", prop.name));
                }
            }
        }

        // Methods (if enabled)
        if self.config.show_methods {
            for method in methods {
                if let Symbol::Function {
                    name,
                    params,
                    return_type,
                    is_async,
                    ..
                } = method
                {
                    let sig = self.build_signature(name, params, return_type, *is_async);
                    if is_python {
                        result.push_str(&format!("    {sig}:\n        ...  # hidden\n"));
                    } else {
                        result.push_str(&format!("    {sig} {{ ... }}\n"));
                    }
                }
            }
        }

        // Show body size
        if is_python {
            result.push_str(&format!("    ...  # {} lines hidden\n", line_range.len()));
        } else {
            result.push_str(&format!("    // ... {} lines ...\n", line_range.len()));
            result.push_str("}\n");
        }

        result
    }
}

impl Default for HeaderStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl VeilStrategy for HeaderStrategy {
    fn veil_file(&self, content: &str, parsed: &ParsedFile) -> Result<String> {
        let mut result = String::new();
        let mut last_end = 1; // 1-indexed line tracking

        // Sort symbols by line number to process in order
        let mut symbols: Vec<_> = parsed.symbols.iter().collect();
        symbols.sort_by_key(|s| s.line_range().start());

        for symbol in symbols {
            let line_range = symbol.line_range();

            // Add content before this symbol
            if line_range.start() > last_end {
                let before = get_lines(content, last_end, line_range.start() - 1);
                if !before.trim().is_empty() {
                    result.push_str(&before);
                    result.push('\n');
                }
            }

            // Add veiled version of symbol
            let veiled = match symbol {
                Symbol::Function { .. } => self.format_function(symbol, content, &parsed.language),
                Symbol::Class { .. } => self.format_class(symbol, content, &parsed.language),
                _ => String::new(),
            };
            result.push_str(&veiled);

            last_end = line_range.end() + 1;
        }

        // Add remaining content after last symbol
        let total_lines = content.lines().count();
        if last_end <= total_lines {
            let after = get_lines(content, last_end, total_lines);
            if !after.trim().is_empty() {
                result.push_str(&after);
                result.push('\n');
            }
        }

        Ok(result)
    }

    fn description(&self) -> &'static str {
        "Shows only function and class signatures, hiding implementations"
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Language, Param, Visibility};
    use crate::types::LineRange;

    #[test]
    fn test_header_strategy_function() {
        let strategy = HeaderStrategy::new();

        let code = r#"fn calculate_sum(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}
"#;

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "calculate_sum".to_string(),
            params: vec![Param {
                name: "numbers".to_string(),
                type_annotation: Some("&[i32]".to_string()),
            }],
            return_type: Some("i32".to_string()),
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let veiled = strategy.veil_file(code, &parsed).unwrap();

        assert!(veiled.contains("fn calculate_sum(numbers: &[i32]) -> i32"));
        assert!(veiled.contains("... 2 lines ..."));
        assert!(!veiled.contains("numbers.iter().sum()")); // Body should be hidden
    }

    #[test]
    fn test_header_strategy_empty_file() {
        let strategy = HeaderStrategy::new();
        let parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));

        let code = "// Just a comment\n";
        let veiled = strategy.veil_file(code, &parsed).unwrap();

        assert!(veiled.contains("// Just a comment"));
    }

    #[test]
    fn test_header_strategy_with_config() {
        let config = HeaderConfig {
            include_docstrings: false,
            max_signature_length: Some(50),
            show_methods: true,
            show_properties: true,
        };
        let strategy = HeaderStrategy::with_config(config);

        let code = "fn test() {}\n";
        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "test".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 1).unwrap(),
            body_range: LineRange::new(1, 1).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let veiled = strategy.veil_file(code, &parsed).unwrap();
        assert!(veiled.contains("fn test"));
    }

    #[test]
    fn test_header_strategy_class_with_methods() {
        let strategy = HeaderStrategy::new();

        let code = "class MyClass {\n  method1() {}\n  method2() {}\n}\n";
        let mut parsed = ParsedFile::new(Language::TypeScript, std::path::PathBuf::from("test.ts"));
        parsed.symbols.push(Symbol::Class {
            name: "MyClass".to_string(),
            kind: ClassKind::Class,
            methods: vec![Symbol::Function {
                name: "method1".to_string(),
                params: vec![],
                return_type: None,
                visibility: Visibility::Public,
                line_range: LineRange::new(2, 2).unwrap(),
                body_range: LineRange::new(2, 2).unwrap(),
                is_async: false,
                attributes: vec![],
            }],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 4).unwrap(),
        });

        let veiled = strategy.veil_file(code, &parsed).unwrap();
        assert!(veiled.contains("class MyClass"));
        assert!(veiled.contains("method1"));
    }

    #[test]
    fn test_header_strategy_class_with_properties() {
        let config = HeaderConfig {
            include_docstrings: true,
            max_signature_length: None,
            show_methods: false,
            show_properties: true,
        };
        let strategy = HeaderStrategy::with_config(config);

        let code = "struct Point { x: i32, y: i32 }\n";
        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Class {
            name: "Point".to_string(),
            kind: ClassKind::Struct,
            methods: vec![],
            properties: vec![
                crate::parser::Property {
                    name: "x".to_string(),
                    type_annotation: Some("i32".to_string()),
                    visibility: Visibility::Public,
                },
                crate::parser::Property {
                    name: "y".to_string(),
                    type_annotation: Some("i32".to_string()),
                    visibility: Visibility::Public,
                },
            ],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 1).unwrap(),
        });

        let veiled = strategy.veil_file(code, &parsed).unwrap();
        assert!(veiled.contains("struct Point"));
    }

    #[test]
    fn test_header_strategy_class_kinds() {
        let strategy = HeaderStrategy::new();

        for kind in [
            ClassKind::Class,
            ClassKind::Struct,
            ClassKind::Trait,
            ClassKind::Interface,
            ClassKind::Enum,
        ] {
            let mut parsed =
                ParsedFile::new(Language::TypeScript, std::path::PathBuf::from("test.ts"));
            parsed.symbols.push(Symbol::Class {
                name: "Test".to_string(),
                kind,
                methods: vec![],
                properties: vec![],
                visibility: Visibility::Public,
                line_range: LineRange::new(1, 1).unwrap(),
            });

            let veiled = strategy.veil_file("", &parsed).unwrap();
            let expected = match kind {
                ClassKind::Class => "class",
                ClassKind::Struct => "struct",
                ClassKind::Trait => "trait",
                ClassKind::Interface => "interface",
                ClassKind::Enum => "enum",
            };
            assert!(veiled.contains(expected));
        }
    }

    #[test]
    fn test_header_strategy_async_function() {
        let strategy = HeaderStrategy::new();

        let code = "async fn fetch() {}\n";
        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "fetch".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 1).unwrap(),
            body_range: LineRange::new(1, 1).unwrap(),
            is_async: true,
            attributes: vec![],
        });

        let veiled = strategy.veil_file(code, &parsed).unwrap();
        assert!(veiled.contains("async fn fetch"));
    }

    #[test]
    fn test_header_strategy_description() {
        let strategy = HeaderStrategy::new();
        assert!(!strategy.description().is_empty());
    }

    #[test]
    fn test_header_strategy_function_with_return_type() {
        let strategy = HeaderStrategy::new();

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "get_value".to_string(),
            params: vec![],
            return_type: Some("i32".to_string()),
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let veiled = strategy
            .veil_file("fn get_value() -> i32 { 42 }", &parsed)
            .unwrap();
        assert!(veiled.contains("-> i32"));
    }

    #[test]
    fn test_header_strategy_property_without_type() {
        let config = HeaderConfig {
            show_properties: true,
            ..Default::default()
        };
        let strategy = HeaderStrategy::with_config(config);

        let mut parsed = ParsedFile::new(Language::TypeScript, std::path::PathBuf::from("test.ts"));
        parsed.symbols.push(Symbol::Class {
            name: "Test".to_string(),
            kind: ClassKind::Class,
            methods: vec![],
            properties: vec![crate::parser::Property {
                name: "value".to_string(),
                type_annotation: None,
                visibility: Visibility::Public,
            }],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 1).unwrap(),
        });

        let veiled = strategy.veil_file("", &parsed).unwrap();
        assert!(veiled.contains("value"));
    }

    #[test]
    fn test_header_strategy_default() {
        let strategy = HeaderStrategy::default();
        assert!(!strategy.description().is_empty());
    }

    #[test]
    fn test_header_strategy_truncate_signature() {
        let config = HeaderConfig {
            max_signature_length: Some(20),
            ..Default::default()
        };
        let strategy = HeaderStrategy::with_config(config);

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "very_long_function_name".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(3, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let code = "fn very_long_function_name() {\n    let x = 1;\n    let y = 2;\n    x + y\n}";
        let veiled = strategy.veil_file(code, &parsed).unwrap();
        assert!(veiled.contains("..."));
    }

    #[test]
    fn test_header_strategy_build_signature_with_return() {
        let strategy = HeaderStrategy::new();

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "test".to_string(),
            params: vec![],
            return_type: Some("String".to_string()),
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 1).unwrap(),
            body_range: LineRange::new(1, 1).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let veiled = strategy
            .veil_file("fn test() -> String {}", &parsed)
            .unwrap();
        assert!(veiled.contains("->"));
    }

    #[test]
    fn test_header_strategy_format_non_function_symbol() {
        let strategy = HeaderStrategy::new();

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Module {
            name: "mymodule".to_string(),
            line_range: LineRange::new(1, 1).unwrap(),
        });

        let veiled = strategy.veil_file("mod mymodule;", &parsed).unwrap();
        assert!(veiled.is_empty() || veiled == "mod mymodule;\n");
    }

    #[test]
    fn test_header_strategy_format_non_class_symbol() {
        let strategy = HeaderStrategy::new();

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Module {
            name: "mymodule".to_string(),
            line_range: LineRange::new(1, 1).unwrap(),
        });

        let veiled = strategy.veil_file("mod mymodule;", &parsed).unwrap();
        assert!(!veiled.contains("class"));
    }

    #[test]
    fn test_header_strategy_with_methods() {
        let config = HeaderConfig {
            show_methods: true,
            ..Default::default()
        };
        let strategy = HeaderStrategy::with_config(config);

        let mut parsed = ParsedFile::new(Language::TypeScript, std::path::PathBuf::from("test.ts"));
        parsed.symbols.push(Symbol::Class {
            name: "TestClass".to_string(),
            kind: ClassKind::Class,
            methods: vec![Symbol::Function {
                name: "method".to_string(),
                params: vec![],
                return_type: None,
                visibility: Visibility::Public,
                line_range: LineRange::new(2, 4).unwrap(),
                body_range: LineRange::new(3, 4).unwrap(),
                is_async: false,
                attributes: vec![],
            }],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
        });

        let veiled = strategy
            .veil_file("class TestClass { method() {} }", &parsed)
            .unwrap();
        assert!(veiled.contains("method"));
    }

    #[test]
    fn test_format_function_with_non_function_symbol() {
        let strategy = HeaderStrategy::new();

        let class_symbol = Symbol::Class {
            name: "TestClass".to_string(),
            kind: ClassKind::Class,
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
        };

        let result = strategy.format_function(&class_symbol, "test content", &Language::Rust);
        assert!(result.is_empty());
    }

    #[test]
    fn test_header_strategy_content_between_symbols() {
        let strategy = HeaderStrategy::new();

        // Code with content between two function definitions
        let code = "fn first() {\n    1\n}\n\n// Some important comment\nconst X: i32 = 42;\n\nfn second() {\n    2\n}\n";
        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "first".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        parsed.symbols.push(Symbol::Function {
            name: "second".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(8, 10).unwrap(),
            body_range: LineRange::new(9, 10).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let veiled = strategy.veil_file(code, &parsed).unwrap();
        // The content between symbols (comment + const) should be included
        assert!(veiled.contains("// Some important comment"));
        assert!(veiled.contains("const X: i32 = 42;"));
        // Function bodies should be veiled
        assert!(!veiled.contains("    1"));
        assert!(!veiled.contains("    2"));
    }

    #[test]
    fn test_format_class_with_non_class_symbol() {
        let strategy = HeaderStrategy::new();

        let func_symbol = Symbol::Function {
            name: "test_func".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        };

        let result = strategy.format_class(&func_symbol, "test content", &Language::Rust);
        assert!(result.is_empty());
    }

    #[test]
    fn test_bug039_truncation_non_ascii_no_panic() {
        let config = HeaderConfig {
            max_signature_length: Some(12),
            ..Default::default()
        };
        let strategy = HeaderStrategy::with_config(config);

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "café_résumé".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                type_annotation: Some("i32".to_string()),
            }],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let code = "fn café_résumé(x: i32) {\n    x\n}\n";
        // Should not panic on non-ASCII truncation
        let veiled = strategy.veil_file(code, &parsed).unwrap();
        assert!(veiled.len() <= code.len());
    }

    // --- Tests targeting specific missed mutants ---

    #[test]
    fn test_floor_char_boundary_at_zero() {
        // Catches: return i → return 1 (or other non-zero)
        assert_eq!(floor_char_boundary("hello", 0), 0);
    }

    #[test]
    fn test_floor_char_boundary_at_ascii_boundary() {
        // Catches: while condition mutations
        assert_eq!(floor_char_boundary("hello", 3), 3);
    }

    #[test]
    fn test_floor_char_boundary_beyond_length() {
        // Catches: return s.len() → return 0
        assert_eq!(floor_char_boundary("hello", 100), 5);
    }

    #[test]
    fn test_floor_char_boundary_mid_multibyte() {
        // "é" is 2 bytes (0xC3, 0xA9). Indexing at byte 1 (middle of é) should floor to 0.
        let s = "é"; // bytes: [0xC3, 0xA9]
        assert_eq!(floor_char_boundary(s, 1), 0);
        // At byte 2 (past é) should return 2
        assert_eq!(floor_char_boundary(s, 2), 2);
    }

    #[test]
    fn test_floor_char_boundary_mid_multibyte_not_at_start() {
        // "aé" = bytes [0x61, 0xC3, 0xA9]. Index 2 is mid-é, should floor to 1.
        let s = "aé";
        assert_eq!(floor_char_boundary(s, 2), 1);
        // Index 1 is start of é, valid boundary
        assert_eq!(floor_char_boundary(s, 1), 1);
    }

    #[test]
    fn test_floor_char_boundary_empty_string() {
        assert_eq!(floor_char_boundary("", 0), 0);
        assert_eq!(floor_char_boundary("", 5), 0);
    }

    #[test]
    fn test_format_function_body_start_equals_line_start_uses_fallback() {
        // When body_range.start() == line_range.start(), the > check is false,
        // so build_signature fallback is used. Catches > → >= mutation.
        let strategy = HeaderStrategy::new();
        let code = "fn test() { body }\n";
        let symbol = Symbol::Function {
            name: "test".to_string(),
            params: vec![],
            return_type: Some("i32".to_string()),
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 1).unwrap(),
            body_range: LineRange::new(1, 1).unwrap(),
            is_async: false,
            attributes: vec![],
        };
        let result = strategy.format_function(&symbol, code, &Language::Rust);
        // Fallback build_signature produces "fn test() -> i32"
        assert!(result.contains("fn test()"));
        assert!(result.contains("-> i32"));
        // Should NOT contain the source line "fn test() { body }" since fallback is used
    }

    #[test]
    fn test_format_function_body_start_one_more_than_line_start() {
        // body_range.start() > line_range.start() → extracts signature from source.
        // Catches > → >= (which would wrongly use fallback here).
        let strategy = HeaderStrategy::new();
        let code = "fn real_sig(x: i32) -> bool\n{\n    true\n}\n";
        let symbol = Symbol::Function {
            name: "real_sig".to_string(),
            params: vec![Param {
                name: "x".to_string(),
                type_annotation: Some("i32".to_string()),
            }],
            return_type: Some("bool".to_string()),
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 4).unwrap(),
            body_range: LineRange::new(2, 4).unwrap(),
            is_async: false,
            attributes: vec![],
        };
        let result = strategy.format_function(&symbol, code, &Language::Rust);
        // Should extract from source: "fn real_sig(x: i32) -> bool"
        assert!(result.contains("fn real_sig(x: i32) -> bool"));
        assert!(result.contains("... 3 lines ..."));
    }

    #[test]
    fn test_truncation_exactly_at_max_len() {
        // Use fallback path (body_range == line_range) so build_signature gives exact "fn ab()" = 7 chars.
        // With max_signature_length=7, `7 > 7` is false → no truncation.
        // Catches > → >= mutation on line 90.
        let config = HeaderConfig {
            max_signature_length: Some(7),
            ..Default::default()
        };
        let strategy = HeaderStrategy::with_config(config);

        let symbol = Symbol::Function {
            name: "ab".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 1).unwrap(),
            body_range: LineRange::new(1, 1).unwrap(),
            is_async: false,
            attributes: vec![],
        };

        let result = strategy.format_function(&symbol, "fn ab() {}", &Language::Rust);
        // "fn ab()" is exactly 7 chars = max_len, so should NOT truncate.
        // Non-truncated output: "fn ab() { ... 1 lines ... }\n"
        // Truncated output would be: "fn a..." (signature cut short)
        assert!(
            result.contains("fn ab()"),
            "should contain full signature, got: {result}"
        );
    }

    #[test]
    fn test_truncation_one_over_max_len() {
        // signature_lines.len() == max_len + 1 → should truncate.
        // With max=6, "fn ab()" (7 chars) should be truncated.
        let config = HeaderConfig {
            max_signature_length: Some(6),
            ..Default::default()
        };
        let strategy = HeaderStrategy::with_config(config);

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "ab".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let code = "fn ab() {\n    1\n}\n";
        let veiled = strategy.veil_file(code, &parsed).unwrap();
        // Should be truncated
        assert!(veiled.contains("..."));
    }

    #[test]
    fn test_max_signature_clamped_to_3() {
        // max_signature_length = Some(1) → clamped to 3 via .max(3).
        // Catches mutations on the .max(3) call at line 89.
        let config = HeaderConfig {
            max_signature_length: Some(1),
            ..Default::default()
        };
        let strategy = HeaderStrategy::with_config(config);

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "test".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let code = "fn test() {\n    1\n}\n";
        let veiled = strategy.veil_file(code, &parsed).unwrap();
        // With clamp to 3, saturating_sub(3) = 0, so boundary=0, result is "..."
        let first_line = veiled.lines().next().unwrap_or("");
        assert_eq!(first_line, "...");
    }

    #[test]
    fn test_veil_file_content_before_first_symbol_preserved() {
        // Catches: line_range.start() > last_end → >= (would skip first content)
        // and line_range.start() - 1 → + 1 or / 1 (would grab wrong range)
        let strategy = HeaderStrategy::new();

        let code = "// header comment\nuse foo;\n\nfn main() {\n    run();\n}\n";
        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "main".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(4, 6).unwrap(),
            body_range: LineRange::new(5, 6).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let veiled = strategy.veil_file(code, &parsed).unwrap();
        // Lines 1-3 should be preserved (before the function at line 4)
        assert!(veiled.contains("// header comment"));
        assert!(veiled.contains("use foo;"));
        // Body hidden
        assert!(!veiled.contains("run()"));
    }

    #[test]
    fn test_veil_file_content_after_last_symbol() {
        // Catches: last_end <= total_lines → < (would skip trailing content when last_end == total_lines)
        // and line_range.end() + 1 mutations
        let strategy = HeaderStrategy::new();

        let code = "fn first() {\n    1\n}\n// trailing\n";
        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "first".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let veiled = strategy.veil_file(code, &parsed).unwrap();
        // Line 4 (trailing comment) should be preserved
        assert!(veiled.contains("// trailing"));
    }

    #[test]
    fn test_veil_file_adjacent_symbols_no_gap() {
        // When line_range.start() == last_end, the > check is false,
        // so no "before" content is added. Catches > → >= mutation on line 218.
        let strategy = HeaderStrategy::new();

        let code = "fn a() {\n    1\n}\nfn b() {\n    2\n}\n";
        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "a".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        parsed.symbols.push(Symbol::Function {
            name: "b".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(4, 6).unwrap(),
            body_range: LineRange::new(5, 6).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let veiled = strategy.veil_file(code, &parsed).unwrap();
        assert!(veiled.contains("fn a()"));
        assert!(veiled.contains("fn b()"));
        // Bodies hidden
        assert!(!veiled.contains("    1"));
        assert!(!veiled.contains("    2"));
    }

    #[test]
    fn test_bug045_truncation_small_max_len() {
        let config = HeaderConfig {
            max_signature_length: Some(2),
            ..Default::default()
        };
        let strategy = HeaderStrategy::with_config(config);

        let mut parsed = ParsedFile::new(Language::Rust, std::path::PathBuf::from("test.rs"));
        parsed.symbols.push(Symbol::Function {
            name: "test".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let code = "fn test() {\n    1\n}\n";
        let veiled = strategy.veil_file(code, &parsed).unwrap();
        // BUG-135 fix: max_signature_length is clamped to 3, so "..." is always produced
        let first_line = veiled.lines().next().unwrap_or("");
        assert_eq!(first_line, "...");
    }
}
