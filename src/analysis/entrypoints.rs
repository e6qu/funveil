//! Entrypoint detection for identifying program entry points.
//!
//! This module detects various types of entrypoints:
//! - Main functions (fn main, if __name__ == "__main__")
//! - Test functions (#[test], def test_*)
//! - CLI handlers (clap, click, argparse)
//! - Web handlers (#[tokio::main], @app.route)

use crate::parser::{Language, ParsedFile, Symbol};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Type of entrypoint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            Language::Bash => Self::detect_bash(file),
            Language::Terraform => Self::detect_terraform(file),
            Language::Helm => Self::detect_helm(file),
            Language::Go => Self::detect_go(file),
            Language::Zig => Self::detect_zig(file),
            Language::Html => Self::detect_html(file),
            Language::Css => Self::detect_css(file),
            Language::Xml => Self::detect_xml(file),
            Language::Markdown => Self::detect_markdown(file),
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
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_tsx = file_name.ends_with(".tsx");

        // Detect React-specific entrypoints
        if is_tsx {
            // Check for Next.js pages
            if file_name == "page.tsx" || file_name == "layout.tsx" {
                entrypoints.push(
                    Entrypoint::new(
                        file_name.to_string(),
                        file.path.clone(),
                        1,
                        EntrypointType::Main,
                        Language::TypeScript,
                    )
                    .with_description("Next.js page/layout"),
                );
            }

            // Check for App.tsx or main React component
            if file_name == "App.tsx" || file_name == "App.ts" {
                entrypoints.push(
                    Entrypoint::new(
                        "App".to_string(),
                        file.path.clone(),
                        1,
                        EntrypointType::Main,
                        Language::TypeScript,
                    )
                    .with_description("React App component"),
                );
            }
        }

        for symbol in &file.symbols {
            match symbol {
                Symbol::Function {
                    name,
                    line_range,
                    attributes,
                    ..
                } => {
                    let line = line_range.start();

                    // React components (PascalCase)
                    if is_tsx && Self::is_pascal_case(name) {
                        let is_entrypoint = name == "App"
                            || name == "Main"
                            || attributes.iter().any(|a| a == "entrypoint");

                        if is_entrypoint {
                            entrypoints.push(
                                Entrypoint::new(
                                    name.clone(),
                                    file.path.clone(),
                                    line,
                                    EntrypointType::Main,
                                    Language::TypeScript,
                                )
                                .with_description("React component"),
                            );
                        }
                        continue;
                    }

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
                    if name == "test"
                        || name.starts_with("test_")
                        || (name.starts_with("test")
                            && name.chars().nth(4).is_some_and(|c| c.is_uppercase()))
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
                    }
                }
                Symbol::Module { name, line_range } => {
                    // JSX elements as handlers
                    if is_tsx && name.starts_with('<') && name.ends_with('>') {
                        let line = line_range.start();
                        entrypoints.push(
                            Entrypoint::new(
                                name.clone(),
                                file.path.clone(),
                                line,
                                EntrypointType::Handler,
                                Language::TypeScript,
                            )
                            .with_description("JSX element"),
                        );
                    }
                }
                _ => {}
            }
        }

        entrypoints
    }

    /// Check if a string is PascalCase
    fn is_pascal_case(s: &str) -> bool {
        s.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
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
                // Heuristic: functions named like commands (word-boundary matching)
                if name
                    .split('_')
                    .any(|part| part == "command" || part == "cmd")
                {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Cli,
                        Language::Python,
                    ));
                }

                // Flask/FastAPI handlers (word-boundary matching)
                if name
                    .split('_')
                    .any(|part| part == "route" || part == "endpoint")
                {
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

    /// Detect Bash entrypoints
    fn detect_bash(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();

        // Bash scripts typically have a "main" function or just execute commands
        // Mark the entire script as an entrypoint if it has executable code
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Check if file is likely executable (ends in .sh or has shebang)
        if file_name.ends_with(".sh") || file_name.ends_with(".bash") {
            entrypoints.push(
                Entrypoint::new(
                    file_name.to_string(),
                    file.path.clone(),
                    1, // Start at line 1 for scripts
                    EntrypointType::Main,
                    Language::Bash,
                )
                .with_description("shell script"),
            );
        }

        // Also check for functions defined in the script
        for symbol in &file.symbols {
            if let Symbol::Function {
                name, line_range, ..
            } = symbol
            {
                let line = line_range.start();

                // Main-like functions
                if name == "main" || name == "run" || name == "start" {
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Main,
                            Language::Bash,
                        )
                        .with_description("script function"),
                    );
                }
            }
        }

        entrypoints
    }

    /// Detect Terraform entrypoints
    fn detect_terraform(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Main Terraform files are entrypoints
        if file_name == "main.tf" || file_name == "variables.tf" || file_name == "outputs.tf" {
            entrypoints.push(
                Entrypoint::new(
                    file_name.to_string(),
                    file.path.clone(),
                    1,
                    EntrypointType::Main,
                    Language::Terraform,
                )
                .with_description("terraform config"),
            );
        }

        // Root module is an entrypoint
        if file_name.ends_with(".tf") {
            // Check for root module indicators
            for symbol in &file.symbols {
                if let Symbol::Function {
                    name, line_range, ..
                } = symbol
                {
                    let line = line_range.start();

                    // Resource and module definitions are entrypoints
                    if name.starts_with("resource")
                        || name.starts_with("module")
                        || name.starts_with("data")
                    {
                        entrypoints.push(
                            Entrypoint::new(
                                format!("{file_name}:{name}"),
                                file.path.clone(),
                                line,
                                EntrypointType::Handler,
                                Language::Terraform,
                            )
                            .with_description("terraform block"),
                        );
                    }
                }
            }
        }

        entrypoints
    }

    /// Detect Helm entrypoints
    fn detect_helm(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Helm chart files
        if file_name == "Chart.yaml" {
            entrypoints.push(
                Entrypoint::new(
                    "Chart.yaml".to_string(),
                    file.path.clone(),
                    1,
                    EntrypointType::Main,
                    Language::Helm,
                )
                .with_description("helm chart metadata"),
            );
        }

        if file_name == "values.yaml" {
            entrypoints.push(
                Entrypoint::new(
                    "values.yaml".to_string(),
                    file.path.clone(),
                    1,
                    EntrypointType::Main,
                    Language::Helm,
                )
                .with_description("helm values"),
            );
        }

        // Template files
        if file.path.to_string_lossy().contains("/templates/") && file_name.ends_with(".yaml") {
            entrypoints.push(
                Entrypoint::new(
                    file_name.to_string(),
                    file.path.clone(),
                    1,
                    EntrypointType::Handler,
                    Language::Helm,
                )
                .with_description("helm template"),
            );
        }

        entrypoints
    }

    /// Detect Go entrypoints
    fn detect_go(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_test_file = file_name.ends_with("_test.go");

        for symbol in &file.symbols {
            if let Symbol::Function {
                name,
                line_range,
                attributes,
                ..
            } = symbol
            {
                let line = line_range.start();

                // Check for main function (only in main package)
                if name == "main" {
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Main,
                            Language::Go,
                        )
                        .with_description("Go main function"),
                    );
                    continue;
                }

                // Check for init function
                if name == "init" {
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Main,
                            Language::Go,
                        )
                        .with_description("Go init function"),
                    );
                    continue;
                }

                // Check for test functions (TestXxx, BenchmarkXxx, ExampleXxx, FuzzXxx)
                if is_test_file
                    && (name.starts_with("Test")
                        || name.starts_with("Benchmark")
                        || name.starts_with("Example")
                        || name.starts_with("Fuzz"))
                {
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Test,
                            Language::Go,
                        )
                        .with_description("Go test/benchmark/example"),
                    );
                    continue;
                }

                // Check for entrypoint attribute (set by parser for main package)
                if attributes.iter().any(|attr| attr == "entrypoint") {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Main,
                        Language::Go,
                    ));
                }
            }
        }

        // Also check for package main (whole file is entrypoint)
        let has_main = entrypoints.iter().any(|ep| ep.name == "main");
        if has_main && !is_test_file {
            // This is a main package executable
            entrypoints.push(
                Entrypoint::new(
                    file_name.to_string(),
                    file.path.clone(),
                    1,
                    EntrypointType::Main,
                    Language::Go,
                )
                .with_description("Go executable"),
            );
        }

        entrypoints
    }

    /// Detect Zig entrypoints
    fn detect_zig(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

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
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Main,
                            Language::Zig,
                        )
                        .with_description("Zig main function"),
                    );
                    continue;
                }

                // Check for test functions
                if name.starts_with("test \"") || attributes.iter().any(|a| a == "test") {
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Test,
                            Language::Zig,
                        )
                        .with_description("Zig test"),
                    );
                    continue;
                }

                // Check for entrypoint attribute
                if attributes.iter().any(|attr| attr == "entrypoint") {
                    entrypoints.push(Entrypoint::new(
                        name.clone(),
                        file.path.clone(),
                        line,
                        EntrypointType::Main,
                        Language::Zig,
                    ));
                }
            }
        }

        // Check for build.zig (special file)
        if file_name == "build.zig" {
            entrypoints.push(
                Entrypoint::new(
                    "build.zig".to_string(),
                    file.path.clone(),
                    1,
                    EntrypointType::Main,
                    Language::Zig,
                )
                .with_description("Zig build script"),
            );
        }

        entrypoints
    }

    /// Detect HTML entrypoints
    fn detect_html(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // HTML files themselves are entrypoints (they define the page structure)
        if file_name.ends_with(".html") || file_name.ends_with(".htm") {
            entrypoints.push(
                Entrypoint::new(
                    file_name.to_string(),
                    file.path.clone(),
                    1,
                    EntrypointType::Main,
                    Language::Html,
                )
                .with_description("HTML document"),
            );
        }

        // Check for script/style blocks
        for symbol in &file.symbols {
            if let Symbol::Module { name, line_range } = symbol {
                let line = line_range.start();

                if name == "<script>" {
                    entrypoints.push(
                        Entrypoint::new(
                            "inline script".to_string(),
                            file.path.clone(),
                            line,
                            EntrypointType::Handler,
                            Language::Html,
                        )
                        .with_description("JavaScript block"),
                    );
                } else if name == "<style>" {
                    entrypoints.push(
                        Entrypoint::new(
                            "inline style".to_string(),
                            file.path.clone(),
                            line,
                            EntrypointType::Handler,
                            Language::Html,
                        )
                        .with_description("CSS block"),
                    );
                }
            }
        }

        entrypoints
    }

    /// Detect CSS entrypoints
    fn detect_css(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // CSS files that define the main styles are entrypoints
        if file_name.ends_with(".css")
            || file_name.ends_with(".scss")
            || file_name.ends_with(".sass")
        {
            let is_main_stylesheet = file_name == "main.css"
                || file_name == "index.css"
                || file_name == "styles.css"
                || file_name == "app.css"
                || file_name == "main.scss"
                || file_name == "index.scss"
                || file_name.contains("tailwind");

            if is_main_stylesheet {
                entrypoints.push(
                    Entrypoint::new(
                        file_name.to_string(),
                        file.path.clone(),
                        1,
                        EntrypointType::Main,
                        Language::Css,
                    )
                    .with_description("Main stylesheet"),
                );
            } else {
                entrypoints.push(
                    Entrypoint::new(
                        file_name.to_string(),
                        file.path.clone(),
                        1,
                        EntrypointType::Handler,
                        Language::Css,
                    )
                    .with_description("CSS module"),
                );
            }
        }

        // Check for Tailwind directives
        for symbol in &file.symbols {
            if let Symbol::Module { name, line_range } = symbol {
                if name.contains("@tailwind") || name.contains("@apply") || name.contains("@layer")
                {
                    let line = line_range.start();
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Handler,
                            Language::Css,
                        )
                        .with_description("Tailwind directive"),
                    );
                }
            }
        }

        entrypoints
    }

    /// Detect XML entrypoints
    fn detect_xml(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // XML files themselves are entrypoints (they define document structure)
        if file_name.ends_with(".xml") {
            entrypoints.push(
                Entrypoint::new(
                    file_name.to_string(),
                    file.path.clone(),
                    1,
                    EntrypointType::Main,
                    Language::Xml,
                )
                .with_description("XML document"),
            );
        }

        // Check for configuration files
        let is_config = file_name == "pom.xml" // Maven
            || file_name == "AndroidManifest.xml"
            || file_name == "web.xml"
            || file_name.ends_with(".config.xml")
            || file_name == "settings.xml";

        if is_config {
            entrypoints.push(
                Entrypoint::new(
                    file_name.to_string(),
                    file.path.clone(),
                    1,
                    EntrypointType::Main,
                    Language::Xml,
                )
                .with_description("XML configuration"),
            );
        }

        entrypoints
    }

    /// Detect Markdown entrypoints
    fn detect_markdown(file: &ParsedFile) -> Vec<Entrypoint> {
        let mut entrypoints = Vec::new();
        let file_name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Markdown documentation files
        if file_name.ends_with(".md") || file_name.ends_with(".markdown") {
            let is_main_doc = file_name == "README.md"
                || file_name == "CONTRIBUTING.md"
                || file_name == "CHANGELOG.md"
                || file_name == "LICENSE.md"
                || file_name == "INSTALL.md"
                || file_name == "API.md"
                || file_name.starts_with("README");

            if is_main_doc {
                entrypoints.push(
                    Entrypoint::new(
                        file_name.to_string(),
                        file.path.clone(),
                        1,
                        EntrypointType::Main,
                        Language::Markdown,
                    )
                    .with_description("Main documentation"),
                );
            } else {
                entrypoints.push(
                    Entrypoint::new(
                        file_name.to_string(),
                        file.path.clone(),
                        1,
                        EntrypointType::Handler,
                        Language::Markdown,
                    )
                    .with_description("Documentation"),
                );
            }
        }

        // Check for headings (main structure)
        for symbol in &file.symbols {
            if let Symbol::Module { name, line_range } = symbol {
                let line = line_range.start();

                // Main title/heading
                if name.starts_with("# ") && line == 1 {
                    entrypoints.push(
                        Entrypoint::new(
                            name.clone(),
                            file.path.clone(),
                            line,
                            EntrypointType::Main,
                            Language::Markdown,
                        )
                        .with_description("Document title"),
                    );
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

    fn create_test_parsed_file_with_path(
        language: Language,
        symbols: Vec<Symbol>,
        path: &str,
    ) -> ParsedFile {
        ParsedFile {
            language,
            path: PathBuf::from(path),
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

    fn create_function_symbol_with_attrs(
        name: &str,
        line_start: usize,
        line_end: usize,
        attributes: Vec<&str>,
    ) -> Symbol {
        Symbol::Function {
            name: name.to_string(),
            params: Vec::new(),
            return_type: None,
            visibility: crate::parser::Visibility::Public,
            line_range: crate::types::LineRange::new(line_start, line_end).unwrap(),
            body_range: crate::types::LineRange::new(line_start + 1, line_end).unwrap(),
            is_async: false,
            attributes: attributes.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn create_module_symbol(name: &str, line_start: usize, line_end: usize) -> Symbol {
        Symbol::Module {
            name: name.to_string(),
            line_range: crate::types::LineRange::new(line_start, line_end).unwrap(),
        }
    }

    #[test]
    fn test_entrypoint_type_display() {
        assert_eq!(format!("{}", EntrypointType::Main), "main");
        assert_eq!(format!("{}", EntrypointType::Test), "test");
        assert_eq!(format!("{}", EntrypointType::Cli), "cli");
        assert_eq!(format!("{}", EntrypointType::Handler), "handler");
        assert_eq!(format!("{}", EntrypointType::Export), "export");
    }

    #[test]
    fn test_entrypoint_with_description() {
        let ep = Entrypoint::new(
            "main",
            PathBuf::from("main.rs"),
            1,
            EntrypointType::Main,
            Language::Rust,
        )
        .with_description("async main");
        assert_eq!(ep.description, Some("async main".to_string()));
    }

    #[test]
    fn test_detect_all() {
        let files = vec![
            create_test_parsed_file(Language::Rust, vec![create_function_symbol("main", 1, 5)]),
            create_test_parsed_file(Language::Python, vec![create_function_symbol("main", 1, 5)]),
        ];
        let entrypoints = EntrypointDetector::detect_all(&files);
        assert_eq!(entrypoints.len(), 2);
    }

    #[test]
    fn test_detect_unknown_language() {
        let file = create_test_parsed_file(Language::Unknown, vec![]);
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints.is_empty());
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
    fn test_detect_rust_test_attribute() {
        let symbols = vec![create_function_symbol_with_attrs(
            "my_test",
            1,
            5,
            vec!["test"],
        )];
        let file = create_test_parsed_file(Language::Rust, symbols);

        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Test);
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
    fn test_detect_rust_async_main() {
        let symbols = vec![create_function_symbol_with_attrs(
            "run",
            1,
            5,
            vec!["tokio::main"],
        )];
        let file = create_test_parsed_file(Language::Rust, symbols);

        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
        assert!(entrypoints[0].description.is_some());
    }

    #[test]
    fn test_detect_rust_clap_cli() {
        let symbols = vec![create_function_symbol_with_attrs(
            "Args",
            1,
            5,
            vec!["derive(Parser)"],
        )];
        let file = create_test_parsed_file(Language::Rust, symbols);

        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Cli);
    }

    #[test]
    fn test_detect_typescript_nextjs_page() {
        let file =
            create_test_parsed_file_with_path(Language::TypeScript, vec![], "src/app/page.tsx");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_typescript_nextjs_layout() {
        let file =
            create_test_parsed_file_with_path(Language::TypeScript, vec![], "src/app/layout.tsx");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_typescript_app_tsx() {
        let file = create_test_parsed_file_with_path(Language::TypeScript, vec![], "src/App.tsx");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].name, "App");
    }

    #[test]
    fn test_detect_typescript_react_component_entrypoint() {
        let symbols = vec![create_function_symbol_with_attrs(
            "Main",
            1,
            5,
            vec!["entrypoint"],
        )];
        let file = create_test_parsed_file_with_path(Language::TypeScript, symbols, "src/Main.tsx");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(!entrypoints.is_empty());
    }

    #[test]
    fn test_detect_typescript_main_function() {
        let symbols = vec![create_function_symbol("main", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::TypeScript, symbols, "src/index.ts");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_typescript_run_start() {
        let symbols = vec![
            create_function_symbol("run", 1, 5),
            create_function_symbol("start", 6, 10),
        ];
        let file = create_test_parsed_file_with_path(Language::TypeScript, symbols, "src/index.ts");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 2);
    }

    #[test]
    fn test_detect_typescript_test_functions() {
        let symbols = vec![
            create_function_symbol("testSomething", 1, 5),
            create_function_symbol("it(", 6, 10),
            create_function_symbol("describe(", 11, 15),
        ];
        let file = create_test_parsed_file_with_path(Language::TypeScript, symbols, "test.ts");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 3);
        assert!(entrypoints
            .iter()
            .all(|ep| ep.entry_type == EntrypointType::Test));
    }

    #[test]
    fn test_detect_typescript_jsx_element() {
        let symbols = vec![create_module_symbol("<Button>", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::TypeScript, symbols, "src/App.tsx");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        let jsx_entrypoints: Vec<_> = entrypoints
            .iter()
            .filter(|ep| ep.entry_type == EntrypointType::Handler)
            .collect();
        assert!(!jsx_entrypoints.is_empty());
    }

    #[test]
    fn test_is_pascal_case() {
        assert!(EntrypointDetector::is_pascal_case("MyComponent"));
        assert!(EntrypointDetector::is_pascal_case("App"));
        assert!(!EntrypointDetector::is_pascal_case("myComponent"));
        assert!(!EntrypointDetector::is_pascal_case("my_component"));
        assert!(EntrypointDetector::is_pascal_case("A"));
        assert!(EntrypointDetector::is_pascal_case("ALLCAPS"));
        assert!(EntrypointDetector::is_pascal_case("X"));
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
    fn test_detect_python_cli() {
        let symbols = vec![create_function_symbol("run_command", 1, 5)];
        let file = create_test_parsed_file(Language::Python, symbols);
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Cli));
    }

    #[test]
    fn test_detect_python_handler() {
        let symbols = vec![create_function_symbol("get_route", 1, 5)];
        let file = create_test_parsed_file(Language::Python, symbols);
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Handler));
    }

    #[test]
    fn test_detect_python_no_false_positive_substring_matches() {
        // BUG-029 regression: substring matching should not trigger on words like
        // "recommend" (contains "cmd"), "enroute" (contains "route"), "endpoint_config"
        let symbols = vec![
            create_function_symbol("recommend", 1, 5),
            create_function_symbol("enroute", 6, 10),
            create_function_symbol("endpoint_config", 11, 15),
        ];
        let file = create_test_parsed_file(Language::Python, symbols);
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        // "recommend" should NOT match as CLI (no word "command"/"cmd")
        assert!(!entrypoints
            .iter()
            .any(|ep| ep.name == "recommend" && ep.entry_type == EntrypointType::Cli));
        // "enroute" should NOT match as Handler (no word "route")
        assert!(!entrypoints
            .iter()
            .any(|ep| ep.name == "enroute" && ep.entry_type == EntrypointType::Handler));
        // "endpoint_config" SHOULD match as Handler ("endpoint" is a full word segment)
        assert!(entrypoints
            .iter()
            .any(|ep| ep.name == "endpoint_config" && ep.entry_type == EntrypointType::Handler));
    }

    #[test]
    fn test_detect_python_run_cli() {
        let symbols = vec![
            create_function_symbol("run", 1, 5),
            create_function_symbol("cli", 6, 10),
        ];
        let file = create_test_parsed_file(Language::Python, symbols);
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 2);
    }

    #[test]
    fn test_detect_bash_script() {
        let file = create_test_parsed_file_with_path(Language::Bash, vec![], "scripts/build.sh");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_bash_bash_extension() {
        let file = create_test_parsed_file_with_path(Language::Bash, vec![], "script.bash");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
    }

    #[test]
    fn test_detect_bash_main_function() {
        let symbols = vec![create_function_symbol("main", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Bash, symbols, "script.sh");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints.iter().any(|ep| ep.name == "main"));
    }

    #[test]
    fn test_detect_bash_run_start() {
        let symbols = vec![
            create_function_symbol("run", 1, 5),
            create_function_symbol("start", 6, 10),
        ];
        let file = create_test_parsed_file_with_path(Language::Bash, symbols, "script.sh");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints.iter().any(|ep| ep.name == "run"));
        assert!(entrypoints.iter().any(|ep| ep.name == "start"));
    }

    #[test]
    fn test_detect_terraform_main() {
        let file = create_test_parsed_file_with_path(Language::Terraform, vec![], "main.tf");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_terraform_variables() {
        let file = create_test_parsed_file_with_path(Language::Terraform, vec![], "variables.tf");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
    }

    #[test]
    fn test_detect_terraform_outputs() {
        let file = create_test_parsed_file_with_path(Language::Terraform, vec![], "outputs.tf");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
    }

    #[test]
    fn test_detect_terraform_resource() {
        let symbols = vec![create_function_symbol("resource_aws_instance", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Terraform, symbols, "infra.tf");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(!entrypoints.is_empty());
    }

    #[test]
    fn test_detect_terraform_module() {
        let symbols = vec![create_function_symbol("module_vpc", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Terraform, symbols, "main.tf");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(!entrypoints.is_empty());
    }

    #[test]
    fn test_detect_terraform_data() {
        let symbols = vec![create_function_symbol("data_aws_ami", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Terraform, symbols, "data.tf");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(!entrypoints.is_empty());
    }

    #[test]
    fn test_detect_helm_chart() {
        let file = create_test_parsed_file_with_path(Language::Helm, vec![], "Chart.yaml");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_helm_template() {
        let file = create_test_parsed_file_with_path(
            Language::Helm,
            vec![],
            "/charts/mychart/templates/deployment.yaml",
        );
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Handler);
    }

    #[test]
    fn test_detect_go_main() {
        let symbols = vec![create_function_symbol("main", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Go, symbols, "main.go");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.name == "main" && ep.entry_type == EntrypointType::Main));
    }

    #[test]
    fn test_detect_go_init() {
        let symbols = vec![create_function_symbol("init", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Go, symbols, "setup.go");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints.iter().any(|ep| ep.name == "init"));
    }

    #[test]
    fn test_detect_go_test() {
        let symbols = vec![create_function_symbol("TestSomething", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Go, symbols, "main_test.go");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Test));
    }

    #[test]
    fn test_detect_go_benchmark() {
        let symbols = vec![create_function_symbol("BenchmarkProcess", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Go, symbols, "bench_test.go");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Test));
    }

    #[test]
    fn test_detect_go_example() {
        let symbols = vec![create_function_symbol("ExampleUsage", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Go, symbols, "example_test.go");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Test));
    }

    #[test]
    fn test_detect_go_fuzz() {
        let symbols = vec![create_function_symbol("FuzzParser", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Go, symbols, "fuzz_test.go");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Test));
    }

    #[test]
    fn test_detect_go_entrypoint_attribute() {
        let symbols = vec![create_function_symbol_with_attrs(
            "handler",
            1,
            5,
            vec!["entrypoint"],
        )];
        let file = create_test_parsed_file_with_path(Language::Go, symbols, "handler.go");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Main));
    }

    #[test]
    fn test_detect_go_main_creates_executable_entry() {
        let symbols = vec![create_function_symbol("main", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Go, symbols, "main.go");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("Go executable")));
    }

    #[test]
    fn test_detect_zig_main() {
        let symbols = vec![create_function_symbol("main", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Zig, symbols, "main.zig");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.name == "main" && ep.entry_type == EntrypointType::Main));
    }

    #[test]
    fn test_detect_zig_test() {
        let symbols = vec![create_function_symbol("test \"addition\"", 1, 5)];
        let file = create_test_parsed_file_with_path(Language::Zig, symbols, "main.zig");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Test));
    }

    #[test]
    fn test_detect_zig_test_attribute() {
        let symbols = vec![create_function_symbol_with_attrs(
            "my_test",
            1,
            5,
            vec!["test"],
        )];
        let file = create_test_parsed_file_with_path(Language::Zig, symbols, "test.zig");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Test));
    }

    #[test]
    fn test_detect_zig_entrypoint_attribute() {
        let symbols = vec![create_function_symbol_with_attrs(
            "handler",
            1,
            5,
            vec!["entrypoint"],
        )];
        let file = create_test_parsed_file_with_path(Language::Zig, symbols, "handler.zig");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Main));
    }

    #[test]
    fn test_detect_zig_build_file() {
        let file = create_test_parsed_file_with_path(Language::Zig, vec![], "build.zig");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints.iter().any(|ep| ep.name == "build.zig"));
    }

    #[test]
    fn test_detect_html_file() {
        let file = create_test_parsed_file_with_path(Language::Html, vec![], "index.html");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_html_htm_extension() {
        let file = create_test_parsed_file_with_path(Language::Html, vec![], "old.htm");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
    }

    #[test]
    fn test_detect_html_script_block() {
        let symbols = vec![create_module_symbol("<script>", 5, 10)];
        let file = create_test_parsed_file_with_path(Language::Html, symbols, "page.html");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints.iter().any(|ep| ep.name == "inline script"));
    }

    #[test]
    fn test_detect_html_style_block() {
        let symbols = vec![create_module_symbol("<style>", 5, 10)];
        let file = create_test_parsed_file_with_path(Language::Html, symbols, "page.html");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints.iter().any(|ep| ep.name == "inline style"));
    }

    #[test]
    fn test_detect_css_main_stylesheet() {
        let file = create_test_parsed_file_with_path(Language::Css, vec![], "main.css");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_css_index_stylesheet() {
        let file = create_test_parsed_file_with_path(Language::Css, vec![], "index.css");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_css_styles_stylesheet() {
        let file = create_test_parsed_file_with_path(Language::Css, vec![], "styles.css");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_css_app_stylesheet() {
        let file = create_test_parsed_file_with_path(Language::Css, vec![], "app.css");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_css_scss_main() {
        let file = create_test_parsed_file_with_path(Language::Css, vec![], "main.scss");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_css_index_scss() {
        let file = create_test_parsed_file_with_path(Language::Css, vec![], "index.scss");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_css_tailwind() {
        let file = create_test_parsed_file_with_path(Language::Css, vec![], "tailwind.config.css");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_css_module() {
        let file = create_test_parsed_file_with_path(Language::Css, vec![], "button.css");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Handler));
    }

    #[test]
    fn test_detect_css_sass_extension() {
        let file = create_test_parsed_file_with_path(Language::Css, vec![], "styles.sass");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(!entrypoints.is_empty());
    }

    #[test]
    fn test_detect_css_tailwind_directive() {
        let symbols = vec![create_module_symbol("@tailwind base;", 1, 1)];
        let file = create_test_parsed_file_with_path(Language::Css, symbols, "tailwind.css");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("Tailwind directive")));
    }

    #[test]
    fn test_detect_css_apply_directive() {
        let symbols = vec![create_module_symbol("@apply flex;", 1, 1)];
        let file = create_test_parsed_file_with_path(Language::Css, symbols, "styles.css");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("Tailwind directive")));
    }

    #[test]
    fn test_detect_css_layer_directive() {
        let symbols = vec![create_module_symbol("@layer components;", 1, 1)];
        let file = create_test_parsed_file_with_path(Language::Css, symbols, "styles.css");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("Tailwind directive")));
    }

    #[test]
    fn test_detect_xml_file() {
        let file = create_test_parsed_file_with_path(Language::Xml, vec![], "config.xml");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_xml_pom() {
        let file = create_test_parsed_file_with_path(Language::Xml, vec![], "pom.xml");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("XML configuration")));
    }

    #[test]
    fn test_detect_xml_android_manifest() {
        let file = create_test_parsed_file_with_path(Language::Xml, vec![], "AndroidManifest.xml");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("XML configuration")));
    }

    #[test]
    fn test_detect_xml_web() {
        let file = create_test_parsed_file_with_path(Language::Xml, vec![], "web.xml");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("XML configuration")));
    }

    #[test]
    fn test_detect_xml_config() {
        let file = create_test_parsed_file_with_path(Language::Xml, vec![], "app.config.xml");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("XML configuration")));
    }

    #[test]
    fn test_detect_xml_settings() {
        let file = create_test_parsed_file_with_path(Language::Xml, vec![], "settings.xml");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("XML configuration")));
    }

    #[test]
    fn test_detect_markdown_main_docs() {
        let file = create_test_parsed_file_with_path(Language::Markdown, vec![], "README.md");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints.len(), 1);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_markdown_contributing() {
        let file = create_test_parsed_file_with_path(Language::Markdown, vec![], "CONTRIBUTING.md");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_markdown_changelog() {
        let file = create_test_parsed_file_with_path(Language::Markdown, vec![], "CHANGELOG.md");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_markdown_license() {
        let file = create_test_parsed_file_with_path(Language::Markdown, vec![], "LICENSE.md");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_markdown_install() {
        let file = create_test_parsed_file_with_path(Language::Markdown, vec![], "INSTALL.md");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_markdown_api() {
        let file = create_test_parsed_file_with_path(Language::Markdown, vec![], "API.md");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_markdown_readme_prefix() {
        let file = create_test_parsed_file_with_path(Language::Markdown, vec![], "README_ja.md");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_detect_markdown_regular_doc() {
        let file = create_test_parsed_file_with_path(Language::Markdown, vec![], "guide.md");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.entry_type == EntrypointType::Handler));
    }

    #[test]
    fn test_detect_markdown_markdown_extension() {
        let file = create_test_parsed_file_with_path(Language::Markdown, vec![], "doc.markdown");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(!entrypoints.is_empty());
    }

    #[test]
    fn test_detect_markdown_title() {
        let symbols = vec![create_module_symbol("# My Project", 1, 1)];
        let file = create_test_parsed_file_with_path(Language::Markdown, symbols, "README.md");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.description.as_deref() == Some("Document title")));
    }

    #[test]
    fn test_group_by_language() {
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
                "main",
                PathBuf::from("main.py"),
                1,
                EntrypointType::Main,
                Language::Python,
            ),
        ];

        let grouped = EntrypointDetector::group_by_language(&entrypoints);
        assert_eq!(grouped.get(&Language::Rust).unwrap().len(), 2);
        assert_eq!(grouped.get(&Language::Python).unwrap().len(), 1);
    }

    #[test]
    fn test_group_refs_by_language() {
        let entrypoints = [
            Entrypoint::new(
                "main",
                PathBuf::from("main.rs"),
                1,
                EntrypointType::Main,
                Language::Rust,
            ),
            Entrypoint::new(
                "test",
                PathBuf::from("test.rs"),
                1,
                EntrypointType::Test,
                Language::Rust,
            ),
        ];
        let refs: Vec<_> = entrypoints.iter().collect();
        let grouped = EntrypointDetector::group_refs_by_language(&refs);
        assert_eq!(grouped.get(&Language::Rust).unwrap().len(), 2);
    }

    #[test]
    fn test_filter_by_language() {
        let entrypoints = vec![
            Entrypoint::new(
                "main",
                PathBuf::from("main.rs"),
                1,
                EntrypointType::Main,
                Language::Rust,
            ),
            Entrypoint::new(
                "test",
                PathBuf::from("test.rs"),
                1,
                EntrypointType::Test,
                Language::Rust,
            ),
            Entrypoint::new(
                "main",
                PathBuf::from("main.py"),
                1,
                EntrypointType::Main,
                Language::Python,
            ),
        ];

        let rust_eps = EntrypointDetector::filter_by_language(&entrypoints, Language::Rust);
        assert_eq!(rust_eps.len(), 2);

        let python_eps = EntrypointDetector::filter_by_language(&entrypoints, Language::Python);
        assert_eq!(python_eps.len(), 1);
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

    #[test]
    fn test_detect_tsx_custom_component_with_entrypoint_attr() {
        let symbols = vec![create_function_symbol_with_attrs(
            "CustomComponent",
            1,
            5,
            vec!["entrypoint"],
        )];
        let file = create_test_parsed_file_with_path(
            Language::TypeScript,
            symbols,
            "src/CustomComponent.tsx",
        );
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints
            .iter()
            .any(|ep| ep.name == "CustomComponent" && ep.entry_type == EntrypointType::Main));
    }

    #[test]
    fn test_detect_tsx_class_symbol() {
        use crate::parser::{ClassKind, Visibility};

        let class_symbol = Symbol::Class {
            name: "ComponentClass".to_string(),
            kind: ClassKind::Class,
            methods: vec![],
            properties: vec![],
            visibility: Visibility::Public,
            line_range: crate::types::LineRange::new(1, 10).unwrap(),
        };
        let file = create_test_parsed_file_with_path(
            Language::TypeScript,
            vec![class_symbol],
            "src/Component.tsx",
        );
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        assert!(entrypoints.is_empty() || entrypoints.iter().all(|ep| ep.name != "ComponentClass"));
    }

    #[test]
    fn test_detect_helm_values_yaml() {
        let file = create_test_parsed_file_with_path(Language::Helm, vec![], "chart/values.yaml");
        let entrypoints = EntrypointDetector::detect_in_file(&file);
        let values_ep: Vec<_> = entrypoints
            .iter()
            .filter(|e| e.name == "values.yaml")
            .collect();
        assert!(!values_ep.is_empty());
        assert_eq!(values_ep[0].entry_type, EntrypointType::Main);
    }

    #[test]
    fn test_bug044_typescript_test_detection_not_too_broad() {
        use crate::parser::Visibility;
        use crate::types::LineRange;

        let mut file = ParsedFile::new(Language::TypeScript, "test.ts".into());
        // "testify_input" should NOT be detected as a test
        file.symbols.push(Symbol::Function {
            name: "testify_input".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        // "testing_helper" should NOT be detected as a test
        file.symbols.push(Symbol::Function {
            name: "testing_helper".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(5, 7).unwrap(),
            body_range: LineRange::new(6, 7).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        // "testLogin" SHOULD be detected as a test (camelCase test name)
        file.symbols.push(Symbol::Function {
            name: "testLogin".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(9, 11).unwrap(),
            body_range: LineRange::new(10, 11).unwrap(),
            is_async: false,
            attributes: vec![],
        });
        // "test_something" SHOULD be detected as a test
        file.symbols.push(Symbol::Function {
            name: "test_something".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(13, 15).unwrap(),
            body_range: LineRange::new(14, 15).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let entrypoints = EntrypointDetector::detect_all(&[file]);
        let test_eps: Vec<_> = entrypoints
            .iter()
            .filter(|e| e.entry_type == EntrypointType::Test)
            .collect();

        let test_names: Vec<&str> = test_eps.iter().map(|e| e.name.as_str()).collect();
        assert!(
            !test_names.contains(&"testify_input"),
            "testify_input should not be a test entrypoint"
        );
        assert!(
            !test_names.contains(&"testing_helper"),
            "testing_helper should not be a test entrypoint"
        );
        assert!(
            test_names.contains(&"testLogin"),
            "testLogin should be a test entrypoint"
        );
        assert!(
            test_names.contains(&"test_something"),
            "test_something should be a test entrypoint"
        );
    }

    // --- Tests targeting specific missed mutants ---

    #[test]
    fn test_rust_main_is_main_type_not_test() {
        use crate::parser::Visibility;
        use crate::types::LineRange;
        // Catches: name == "main" → != mutation and continue deletion (lines 134, 142)
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("main.rs"));
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

        let eps = EntrypointDetector::detect_in_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].entry_type, EntrypointType::Main);
        assert_eq!(eps[0].name, "main");
    }

    #[test]
    fn test_rust_test_attribute_detected() {
        use crate::parser::Visibility;
        use crate::types::LineRange;
        // Catches: attributes.iter().any(|attr| attr.contains("test")) mutation (line 146)
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.symbols.push(Symbol::Function {
            name: "it_works".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec!["test".to_string()],
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].entry_type, EntrypointType::Test);
    }

    #[test]
    fn test_rust_test_name_ends_with_test() {
        use crate::parser::Visibility;
        use crate::types::LineRange;
        // Catches: || in `name.starts_with("test_") || name.ends_with("_test")` (line 158)
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("test.rs"));
        file.symbols.push(Symbol::Function {
            name: "integration_test".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 3).unwrap(),
            body_range: LineRange::new(2, 3).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].entry_type, EntrypointType::Test);
    }

    #[test]
    fn test_rust_clap_cli_detected() {
        use crate::parser::Visibility;
        use crate::types::LineRange;
        // Catches: derive+Parser combined check and continue (lines 185-200)
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("cli.rs"));
        file.symbols.push(Symbol::Function {
            name: "Args".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec!["derive(Parser)".to_string()],
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].entry_type, EntrypointType::Cli);
    }

    #[test]
    fn test_go_test_in_test_file() {
        use crate::parser::Visibility;
        use crate::types::LineRange;
        // Catches: is_test_file && (...) operator (line 600) - must be in _test.go file
        let mut file = ParsedFile::new(Language::Go, PathBuf::from("main_test.go"));
        file.symbols.push(Symbol::Function {
            name: "TestSomething".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        let test_eps: Vec<_> = eps
            .iter()
            .filter(|e| e.entry_type == EntrypointType::Test)
            .collect();
        assert_eq!(test_eps.len(), 1);
    }

    #[test]
    fn test_go_test_not_in_regular_file() {
        use crate::parser::Visibility;
        use crate::types::LineRange;
        // Catches: is_test_file must be true - regular .go file shouldn't detect Test* as tests
        let mut file = ParsedFile::new(Language::Go, PathBuf::from("main.go"));
        file.symbols.push(Symbol::Function {
            name: "TestHelper".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        let test_eps: Vec<_> = eps
            .iter()
            .filter(|e| e.entry_type == EntrypointType::Test)
            .collect();
        assert!(test_eps.is_empty());
    }

    #[test]
    fn test_css_main_stylesheet_vs_module() {
        // Catches: == comparisons for main stylesheets (lines 789-795)
        let main_file = ParsedFile::new(Language::Css, PathBuf::from("main.css"));
        let other_file = ParsedFile::new(Language::Css, PathBuf::from("component.css"));

        let main_eps = EntrypointDetector::detect_in_file(&main_file);
        let other_eps = EntrypointDetector::detect_in_file(&other_file);

        assert!(main_eps
            .iter()
            .any(|e| e.entry_type == EntrypointType::Main));
        assert!(other_eps
            .iter()
            .any(|e| e.entry_type == EntrypointType::Handler));
    }

    #[test]
    fn test_markdown_readme_is_main() {
        // Catches: file_name == "README.md" and starts_with("README") checks (lines 894-900)
        let readme = ParsedFile::new(Language::Markdown, PathBuf::from("README.md"));
        let other = ParsedFile::new(Language::Markdown, PathBuf::from("notes.md"));

        let readme_eps = EntrypointDetector::detect_in_file(&readme);
        let other_eps = EntrypointDetector::detect_in_file(&other);

        assert!(readme_eps
            .iter()
            .any(|e| e.entry_type == EntrypointType::Main));
        assert!(other_eps
            .iter()
            .any(|e| e.entry_type == EntrypointType::Handler));
    }

    #[test]
    fn test_markdown_heading_at_line_1() {
        use crate::types::LineRange;
        // Catches: line == 1 boundary (line 933) - only first line heading is title
        let mut file = ParsedFile::new(Language::Markdown, PathBuf::from("doc.md"));
        file.symbols.push(Symbol::Module {
            name: "# Title".to_string(),
            line_range: LineRange::new(1, 1).unwrap(),
        });
        file.symbols.push(Symbol::Module {
            name: "# Section".to_string(),
            line_range: LineRange::new(5, 5).unwrap(),
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        let title_eps: Vec<_> = eps
            .iter()
            .filter(|e| e.description.as_deref() == Some("Document title"))
            .collect();
        assert_eq!(title_eps.len(), 1);
        assert_eq!(title_eps[0].line, 1);
    }

    #[test]
    fn test_terraform_main_files() {
        // Catches: == comparisons for terraform filenames (line 458)
        for name in ["main.tf", "variables.tf", "outputs.tf"] {
            let file = ParsedFile::new(Language::Terraform, PathBuf::from(name));
            let eps = EntrypointDetector::detect_in_file(&file);
            assert!(!eps.is_empty(), "{name} should produce entrypoints");
        }
        // Non-main tf file should not trigger the main check
        let file = ParsedFile::new(Language::Terraform, PathBuf::from("network.tf"));
        let eps = EntrypointDetector::detect_in_file(&file);
        let main_eps: Vec<_> = eps
            .iter()
            .filter(|e| e.description.as_deref() == Some("terraform config"))
            .collect();
        assert!(main_eps.is_empty());
    }

    #[test]
    fn test_typescript_react_component_app() {
        use crate::parser::Visibility;
        use crate::types::LineRange;
        // Catches: is_tsx && is_pascal_case (line 255), name == "App" (line 256)
        let mut file = ParsedFile::new(Language::TypeScript, PathBuf::from("App.tsx"));
        file.symbols.push(Symbol::Function {
            name: "App".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 10).unwrap(),
            body_range: LineRange::new(2, 10).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        let main_eps: Vec<_> = eps
            .iter()
            .filter(|e| e.entry_type == EntrypointType::Main)
            .collect();
        assert!(
            main_eps.len() >= 1,
            "App component should be detected as main entrypoint"
        );
    }

    #[test]
    fn test_typescript_non_tsx_pascal_case_not_react() {
        use crate::parser::Visibility;
        use crate::types::LineRange;
        // Catches: is_tsx check - .ts file should NOT detect PascalCase as React component
        let mut file = ParsedFile::new(Language::TypeScript, PathBuf::from("utils.ts"));
        file.symbols.push(Symbol::Function {
            name: "MyHelper".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec![],
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        // MyHelper in .ts file should NOT be detected as React component
        let react_eps: Vec<_> = eps
            .iter()
            .filter(|e| e.description.as_deref() == Some("React component"))
            .collect();
        assert!(react_eps.is_empty());
    }

    #[test]
    fn test_typescript_jsx_element_handler() {
        use crate::types::LineRange;
        // Catches: is_tsx && starts_with('<') && ends_with('>') (line 306)
        let mut file = ParsedFile::new(Language::TypeScript, PathBuf::from("page.tsx"));
        file.symbols.push(Symbol::Module {
            name: "<Router>".to_string(),
            line_range: LineRange::new(5, 5).unwrap(),
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        let handler_eps: Vec<_> = eps
            .iter()
            .filter(|e| e.entry_type == EntrypointType::Handler)
            .collect();
        assert!(
            !handler_eps.is_empty(),
            "JSX element should be detected as handler"
        );
    }

    #[test]
    fn test_typescript_jsx_not_in_ts_file() {
        use crate::types::LineRange;
        // In .ts (not .tsx), JSX modules should NOT be detected as handlers
        let mut file = ParsedFile::new(Language::TypeScript, PathBuf::from("utils.ts"));
        file.symbols.push(Symbol::Module {
            name: "<Component>".to_string(),
            line_range: LineRange::new(5, 5).unwrap(),
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        let handler_eps: Vec<_> = eps
            .iter()
            .filter(|e| e.description.as_deref() == Some("JSX element"))
            .collect();
        assert!(handler_eps.is_empty());
    }

    #[test]
    fn test_rust_derive_parser_both_required() {
        use crate::parser::Visibility;
        use crate::types::LineRange;
        // Catches: && → || in derive+Parser check (line 187)
        // An attribute with only "derive" (no "Parser") should NOT trigger CLI detection
        let mut file = ParsedFile::new(Language::Rust, PathBuf::from("types.rs"));
        file.symbols.push(Symbol::Function {
            name: "MyType".to_string(),
            params: vec![],
            return_type: None,
            visibility: Visibility::Public,
            line_range: LineRange::new(1, 5).unwrap(),
            body_range: LineRange::new(2, 5).unwrap(),
            is_async: false,
            attributes: vec!["derive(Debug, Clone)".to_string()],
        });

        let eps = EntrypointDetector::detect_in_file(&file);
        let cli_eps: Vec<_> = eps
            .iter()
            .filter(|e| e.entry_type == EntrypointType::Cli)
            .collect();
        assert!(
            cli_eps.is_empty(),
            "derive without Parser should not be detected as CLI"
        );
    }

    #[test]
    fn test_filter_by_type_returns_correct_subset() {
        // Catches: == → != in filter_by_type (line 993)
        let eps = vec![
            Entrypoint::new(
                "main",
                PathBuf::from("a.rs"),
                1,
                EntrypointType::Main,
                Language::Rust,
            ),
            Entrypoint::new(
                "test_a",
                PathBuf::from("a.rs"),
                5,
                EntrypointType::Test,
                Language::Rust,
            ),
            Entrypoint::new(
                "handler",
                PathBuf::from("b.rs"),
                1,
                EntrypointType::Handler,
                Language::Rust,
            ),
        ];

        let tests = EntrypointDetector::filter_by_type(&eps, EntrypointType::Test);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "test_a");

        let mains = EntrypointDetector::filter_by_type(&eps, EntrypointType::Main);
        assert_eq!(mains.len(), 1);
        assert_eq!(mains[0].name, "main");
    }

    #[test]
    fn test_filter_by_language_returns_correct_subset() {
        // Catches: == → != in filter_by_language (line 1001)
        let eps = vec![
            Entrypoint::new(
                "main",
                PathBuf::from("a.rs"),
                1,
                EntrypointType::Main,
                Language::Rust,
            ),
            Entrypoint::new(
                "main",
                PathBuf::from("b.py"),
                1,
                EntrypointType::Main,
                Language::Python,
            ),
        ];

        let rust = EntrypointDetector::filter_by_language(&eps, Language::Rust);
        assert_eq!(rust.len(), 1);
        assert_eq!(rust[0].file, PathBuf::from("a.rs"));
    }
}
