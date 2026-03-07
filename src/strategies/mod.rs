//! Veiling strategies for intelligent code hiding.
//!
//! Strategies determine how code is veiled based on different
//! criteria (headers only, entrypoints, call graphs, etc.)

use std::path::Path;

use crate::error::Result;
use crate::parser::{CodeIndex, ParsedFile};

mod header;
pub use header::HeaderStrategy;

/// A strategy for veiling code
pub trait VeilStrategy {
    /// Apply veiling to a single file
    ///
    /// Returns the veiled content as a string
    fn veil_file(&self, content: &str, parsed: &ParsedFile) -> Result<String>;

    /// Get a description of what this strategy does
    fn description(&self) -> &'static str;
}

/// Context for veiling operations that span multiple files
pub struct VeilContext<'a> {
    pub code_index: &'a CodeIndex,
    pub root_path: &'a Path,
}

impl<'a> VeilContext<'a> {
    pub fn new(code_index: &'a CodeIndex, root_path: &'a Path) -> Self {
        Self {
            code_index,
            root_path,
        }
    }
}

/// Find the line in content that contains a given line number (1-indexed)
pub fn get_line(content: &str, line_num: usize) -> Option<&str> {
    content.lines().nth(line_num.saturating_sub(1))
}

/// Get a range of lines from content (1-indexed, inclusive)
pub fn get_lines(content: &str, start: usize, end: usize) -> String {
    content
        .lines()
        .skip(start.saturating_sub(1))
        .take(end.saturating_sub(start) + 1)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_line() {
        let content = "line 1\nline 2\nline 3";
        assert_eq!(get_line(content, 1), Some("line 1"));
        assert_eq!(get_line(content, 2), Some("line 2"));
        assert_eq!(get_line(content, 3), Some("line 3"));
        assert_eq!(get_line(content, 4), None);
    }

    #[test]
    fn test_get_lines() {
        let content = "line 1\nline 2\nline 3\nline 4";
        assert_eq!(get_lines(content, 2, 3), "line 2\nline 3");
    }
}
