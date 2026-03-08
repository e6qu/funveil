//! Language-specific parsers for Tree-sitter.
//!
//! This module contains parsers for individual programming languages.
//! Each parser is responsible for extracting symbols, imports, and calls
//! from source files of its respective language.

pub mod css;
pub mod go;
pub mod html;
pub mod typescript;
pub mod xml;
pub mod zig;

// Re-export commonly used items
pub use css::{has_tailwind, is_scss, parse_css_file};
pub use go::parse_go_file;
pub use html::parse_html_file;
pub use typescript::{is_react_component, is_react_hook, is_tsx, parse_typescript_file};
pub use xml::parse_xml_file;
pub use zig::parse_zig_file;
