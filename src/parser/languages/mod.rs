//! Language-specific parsers for Tree-sitter.
//!
//! This module contains parsers for individual programming languages.
//! Each parser is responsible for extracting symbols, imports, and calls
//! from source files of its respective language.

pub mod go;

// Re-export commonly used items
pub use go::parse_go_file;
