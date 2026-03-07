//! Entrypoint detection for identifying program entry points.
//!
//! This module detects various types of entrypoints:
//! - Main functions (fn main, if __name__ == "__main__")
//! - Test functions (#[test], def test_*)
//! - CLI handlers (clap, click, argparse)
//! - Web handlers (#[tokio::main], @app.route)

use crate::parser::{Language, ParsedFile, Symbol};
use std::collections::HashMap;
use std::path::PathBuf;

/// Type of entrypoint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntrypointType {
    /// Main entry point (fn main, __main__)
    Main,
    /// Test function
    Test,
    /// CLI command handler
    Cli,
    /// Web/API handler
    Handler,
    /// Library export
    Export,
}

impl std::fmt::Display for EntrypointType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntrypointType::Main => write!(f, "main"),
            EntrypointType::Test => write!(f, "test"),
            EntrypointType::Cli => write!(f, "cli"),
            EntrypointType::Handler => write!(f, "handler"),
            EntrypointType::Export => write!(f, "export"),
        }
    }
}

/// A detected entrypoint
#[derive(Debug, Clone)]
pub struct Entrypoint {
    /// Function name
    pub name: String,
    /// File path
    pub file: PathBuf,
    /// Line number
    pub line: usize,
    /// Type of entrypoint
    pub entry_type: EntrypointType,
    /// Language
    pub language: Language,
    /// Optional description
    pub description: Option<String>,
}

impl Entrypoint {
    /// Create a new entrypoint
    pub fn new(
        name: impl Into<String>,
        file: PathBuf,
        line: usize,
        entry_type: EntrypointType,
        language: Language,
    ) -> Self {
        Self {
            name: name.into(),
            file,
            line,
            entry_type,
            language,
            description: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// Detects entrypoints in parsed files
pub struct EntrypointDetector;

impl EntrypointDetector {
    /// Detect all entrypoints in a collection of parsed files
    pub fn detect_all(files: &[ParsedFile]) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();

        for file in files {
            let mut file_entrypoints = Self::detect_in_file(file);
            entrypoints.append(&mut file_entrypoints);
        }

        entrypoints
    }

    /// Detect entrypoints in a single file
    pub fn detect_in_file(file: &ParsedFile) -> Vec<Entrypoint> {
        match file.language {
            Language::Rust => Self::detect_rust(file),
            Language::TypeScript => Self::detect_typescript(file),
            Language::Python => Self::detect_python(file),
            Language::Unknown => Vec::new(),
        }
    }

    /// Detect Rust entrypoints
    fn detect_rust(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();

        for symbol in &file.symbols {
            if let Symbol::Function {
                name,
                line_range,
                attributes,
                ..
            } = symbol
            {
                let line = line_range.start();

                // Check for main function
                if name == "main" {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Main,
                        Language::Rust,
                    ));
                    continue;
                }

                // Check for #[test] attribute
                if attributes.iter().any(|attr| attr.contains("test")) {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Test,
                        Language::Rust,
                    ));
                    continue;
                }

                // Check for test functions by naming convention
                if name.starts_with("test_") || name.ends_with("_test") {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Test,
                        Language::Rust,
                    ));
                    continue;
                }

                // Check for async main (#[tokio::main], #[actix_web::main], etc.)
                if attributes.iter().any(|attr| attr.contains("main")) {
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Main,
                            Language::Rust,
                        )
                        .with_description("async main"),
                    );
                    continue;
                }

                // Check for CLI handlers (using clap)
                if attributes
                    .iter()
                    .any(|attr| attr.contains("derive") && attr.contains("Parser"))
                {
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Cli,
                            Language::Rust,
                        )
                        .with_description("clap CLI"),
                    );
                    continue;
                }
            }
        }

        entrypoints
    }

    /// Detect TypeScript entrypoints
    fn detect_typescript(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();

        for symbol in &file.symbols {
            if let Symbol::Function {
                name, line_range, ..
            } = symbol
            {
                let line = line_range.start();

                // Common CLI patterns
                if name == "main" || name == "run" || name == "start" {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Main,
                        Language::TypeScript,
                    ));
                    continue;
                }

                // Test functions
                if name.starts_with("test")
                    || name.starts_with("it(")
                    || name.starts_with("describe(")
                {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Test,
                        Language::TypeScript,
                    ));
                    continue;
                }

                // Handler functions (Express, etc.)
                if name.contains("Handler") || name.contains("Controller") {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Handler,
                        Language::TypeScript,
                    ));
                }
            }
        }

        entrypoints
    }

    /// Detect Python entrypoints
    fn detect_python(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();

        for symbol in &file.symbols {
            if let Symbol::Function {
                name, line_range, ..
            } = symbol
            {
                let line = line_range.start();

                // Check for __main__ block indicator
                // Note: We'd need to parse the actual content to detect
                // `if __name__ == "__main__":` blocks
                if name == "main" || name == "cli" || name == "run" {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Main,
                        Language::Python,
                    ));
                    continue;
                }

                // Test functions (pytest style)
                if name.starts_with("test_") {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Test,
                        Language::Python,
                    ));
                    continue;
                }

                // CLI handlers (click, argparse)
                // Heuristic: functions named like commands
                if name.contains("command") || name.contains("cmd") {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Cli,
                        Language::Python,
                    ));
                }

                // Flask/FastAPI handlers
                if name.contains("route") || name.contains("endpoint") {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Handler,
                        Language::Python,
                    ));
                }
            }
        }

        entrypoints
    }

    /// Group entrypoints by type
    pub fn group_by_type(entrypoints: &[Entrypoint]) -> HashMap<EntrypointType, Vec<&Entrypoint>> {
        let mut grouped: HashMap<EntrypointType, Vec<&Entrypoint>> = HashMap::new();

        for ep in entrypoints {
            grouped.entry(ep.entry_type).or_default().push(ep);
        }

        grouped
    }

    /// Group entrypoints by language
    pub fn group_by_language(entrypoints: &[Entrypoint]) -> HashMap<Language, Vec<&Entrypoint>> {
        let mut grouped: HashMap<Language, Vec<&Entrypoint>> = HashMap::new();

        for ep in entrypoints {
            grouped.entry(ep.language).or_default().push(ep);
        }

        grouped
    }

    /// Group a slice of entrypoint references by language
    pub fn group_refs_by_language<'a>(
        entrypoints: &'a [&'a Entrypoint],
    ) -> HashMap<Language, Vec<&'a Entrypoint>> {
        let mut grouped: HashMap<Language, Vec<&'a Entrypoint>> = HashMap::new();

        for ep in entrypoints {
            grouped.entry(ep.language).or_default().push(ep);
        }

        grouped
    }

    /// Filter entrypoints by type
    pub fn filter_by_type(
        entrypoints: &[Entrypoint],
        entry_type: EntrypointType,
    ) -> Vec<&Entrypoint> {
        entrypoints
            .iter()
            .filter(|ep| ep.entry_type == entry_type)
            .collect()
    }

    /// Filter entrypoints by language
    pub fn filter_by_language(entrypoints: &[Entrypoint], language: Language) -> Vec<&Entrypoint> {
        entrypoints
            .iter()
            .filter(|ep| ep.language == language)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_parsed_file(language: Language, symbols: Vec<Symbol>) -> ParsedFile {
        ParsedFile {
            language,
            path: PathBuf::from("test.rs"),
            symbols,
            imports: Vec::new(),
            calls: Vec::new(),
        }
    }

    fn create_function_symbol(name: &str, line_start: usize, line_end: usize) -> Symbol {
        Symbol::Function {
            name: name.to_string(),
            params: Vec::new(),
            return_type: None,
            visibility: crate::parser::Visibility::Public,
            line_range: crate::types::LineRange::new(line_start, line_end).unwrap(),
            body_range: crate::types::LineRange::new(line_start + 1, line_end).unwrap(),
            is_async: false,
            attributes: Vec::new(),
        }
    }

    #[test]
    fn test_detect_rust_main() {
        let symbols = vec![create_function_symbol("main", 1, 5)];
        let file = create_test_parsed_file(Language::Rust, symbols);

        let entrypoints = EntrypointDetector::detect_in_file(&file);

        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].name, "main");
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_rust_test_by_convention() {
        let symbols = vec![
            create_function_symbol("test_addition", 1, 5),
            create_function_symbol("helper", 6, 10),
            create_function_symbol("something_test", 11, 15),
        ];
        let file = create_test_parsed_file(Language::Rust, symbols);

        let entrypoints = EntrypointDetector::detect_in_file(&file);

        assert_eq!(entrypoints.len(), 2);
        assert!(entrypoints
            .iter()
            .all(|ep| ep.entry_type == EntrypointType::Test));
    }

    #[test]
    fn test_detect_python_main() {
        let symbols = vec![
            create_function_symbol("main", 1, 5),
            create_function_symbol("test_something", 6, 10),
        ];
        let file = create_test_parsed_file(Language::Python, symbols);

        let entrypoints = EntrypointDetector::detect_in_file(&file);

        assert_eq!(entrypoints.len(), 2);
        let types: Vec<_> = entrypoints.iter().map(|ep| ep.entry_type).collect();
        assert!(types.contains(&EntrypointType::Main));
        assert!(types.contains(&EntrypointType::Test));
    }

    #[test]
    fn test_filter_by_type() {
        let entrypoints = vec![
            Entrypoint::new(
                "main",
                PathBuf::from("main.rs"),
                1,
                EntrypointType::Main,
                Language::Rust,
            ),
            Entrypoint::new(
                "test_1",
                PathBuf::from("test.rs"),
                1,
                EntrypointType::Test,
                Language::Rust,
            ),
            Entrypoint::new(
                "test_2",
                PathBuf::from("test.rs"),
                2,
                EntrypointType::Test,
                Language::Rust,
            ),
        ];

        let tests = EntrypointDetector::filter_by_type(&entrypoints, EntrypointType::Test);
        assert_eq!(tests.len(), 2);

        let mains = EntrypointDetector::filter_by_type(&entrypoints, EntrypointType::Main);
        assert_eq!(mains.len(), 1);
    }

    #[test]
    fn test_group_by_type() {
        let entrypoints = vec![
            Entrypoint::new(
                "main",
                PathBuf::from("main.rs"),
                1,
                EntrypointType::Main,
                Language::Rust,
            ),
            Entrypoint::new(
                "test_1",
                PathBuf::from("test.rs"),
                1,
                EntrypointType::Test,
                Language::Rust,
            ),
            Entrypoint::new(
                "test_2",
                PathBuf::from("test.rs"),
                2,
                EntrypointType::Test,
                Language::Rust,
            ),
        ];

        let grouped = EntrypointDetector::group_by_type(&entrypoints);
        assert_eq!(grouped.get(&EntrypointType::Main).unwrap().len(), 1);
        assert_eq!(grouped.get(&EntrypointType::Test).unwrap().len(), 2);
    }
}
