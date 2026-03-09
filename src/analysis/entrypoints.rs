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
            && s.chars().any(|c| c.is_lowercase())
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
        assert!(!EntrypointDetector::is_pascal_case("A"));
        assert!(!EntrypointDetector::is_pascal_case("ALLCAPS"));
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
}
