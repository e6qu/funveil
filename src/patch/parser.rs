//! PEG Parser for patch formats
//!
//! Supports unified diff and git diff formats

#[allow(unused_imports)]
use pest::Parser;
use pest_derive::Parser;
use std::path::PathBuf;

use crate::error::{FunveilError, Result};

#[derive(Parser)]
#[grammar = "patch/grammar.pest"]
pub struct PatchParser;

/// A parsed patch containing multiple file patches
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPatch {
    pub files: Vec<FilePatch>,
    pub format: PatchFormat,
}

/// Format of the patch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchFormat {
    UnifiedDiff,
    GitDiff,
}

/// Patch for a single file
#[derive(Debug, Clone, PartialEq)]
pub struct FilePatch {
    pub old_path: Option<PathBuf>,
    pub new_path: Option<PathBuf>,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    pub is_new_file: bool,
    pub is_deleted: bool,
    pub is_rename: bool,
    pub is_copy: bool,
    pub is_binary: bool,
    pub hunks: Vec<Hunk>,
    pub similarity: Option<u8>,
}

/// A hunk within a file patch
#[derive(Debug, Clone, PartialEq)]
pub struct Hunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub section: Option<String>,
    pub lines: Vec<Line>,
}

/// A line in a hunk
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Context(String),
    Delete(String),
    Add(String),
    NoNewline,
}

impl PatchParser {
    /// Parse a patch string, auto-detecting the format
    pub fn parse_patch(input: &str) -> Result<ParsedPatch> {
        let format = Self::detect_format(input);

        // For now, use a simple line-based parser instead of full PEG
        // This is more robust and easier to understand
        let files = Self::parse_simple(input)?;

        Ok(ParsedPatch { files, format })
    }

    /// Simple line-based parser for patches
    fn parse_simple(input: &str) -> Result<Vec<FilePatch>> {
        let mut files = Vec::new();
        let lines: Vec<&str> = input.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // Git diff format
            if line.starts_with("diff --git") {
                let (file, new_i) = Self::parse_git_diff(&lines, i)?;
                files.push(file);
                i = new_i;
                continue;
            }

            // Unified diff format
            if line.starts_with("--- ") {
                // Check if next line is +++
                if i + 1 < lines.len() && lines[i + 1].starts_with("+++ ") {
                    let (file, new_i) = Self::parse_unified_diff(&lines, i)?;
                    files.push(file);
                    i = new_i;
                    continue;
                }
            }

            i += 1;
        }

        Ok(files)
    }

    /// Parse a git diff section
    fn parse_git_diff(lines: &[&str], start: usize) -> Result<(FilePatch, usize)> {
        let mut i = start;
        let mut old_path = None;
        let mut new_path = None;
        let mut old_mode = None;
        let mut new_mode = None;
        let mut is_new_file = false;
        let mut is_deleted = false;
        let mut is_rename = false;
        let mut is_copy = false;
        let mut is_binary = false;
        let mut similarity = None;
        let mut hunks = Vec::new();

        // Parse diff --git line
        if let Some(line) = lines.get(i) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                old_path = Self::clean_path(parts[2]);
                new_path = Self::clean_path(parts[3]);
            }
            i += 1;
        }

        // Parse extended headers
        while i < lines.len() {
            let line = lines[i];

            if let Some(stripped) = line.strip_prefix("old mode ") {
                old_mode = Some(stripped.to_string());
            } else if let Some(stripped) = line.strip_prefix("new mode ") {
                new_mode = Some(stripped.to_string());
            } else if let Some(stripped) = line.strip_prefix("deleted file mode ") {
                is_deleted = true;
                old_mode = Some(stripped.to_string());
            } else if let Some(stripped) = line.strip_prefix("new file mode ") {
                is_new_file = true;
                new_mode = Some(stripped.to_string());
            } else if let Some(stripped) = line.strip_prefix("rename from ") {
                is_rename = true;
                old_path = Self::clean_path(stripped);
            } else if let Some(stripped) = line.strip_prefix("rename to ") {
                new_path = Self::clean_path(stripped);
            } else if let Some(stripped) = line.strip_prefix("copy from ") {
                is_copy = true;
                old_path = Self::clean_path(stripped);
            } else if let Some(stripped) = line.strip_prefix("copy to ") {
                new_path = Self::clean_path(stripped);
            } else if let Some(stripped) = line.strip_prefix("similarity index ") {
                if let Ok(num) = stripped.trim_end_matches('%').parse() {
                    similarity = Some(num);
                }
            } else if line.starts_with("index ") {
                // Index line, skip
            } else if line.starts_with("Binary files ") {
                is_binary = true;
                i += 1;
                break;
            } else if line.starts_with("--- ") {
                // Start of text diff
                break;
            } else if line.starts_with("diff --git") {
                // Start of next file
                break;
            } else {
                // Unknown line, might be end of headers
                break;
            }
            i += 1;
        }

        // Parse text diff if present
        if i < lines.len() && lines[i].starts_with("--- ") {
            // Parse old file line
            let line_text = lines[i];
            if line_text.starts_with("--- /dev/null") {
                old_path = None; // Explicitly set to None for new files
            } else {
                old_path = Self::parse_file_line(line_text, "--- ").or(old_path);
            }
            i += 1;

            // Parse new file line
            if i < lines.len() && lines[i].starts_with("+++ ") {
                let line_text = lines[i];
                if line_text.starts_with("+++ /dev/null") {
                    new_path = None; // Explicitly set to None for deleted files
                } else {
                    new_path = Self::parse_file_line(line_text, "+++ ").or(new_path);
                }
                i += 1;
            }

            // Parse hunks
            while i < lines.len() && lines[i].starts_with("@@") {
                let (hunk, new_i) = Self::parse_hunk(lines, i)?;
                hunks.push(hunk);
                i = new_i;
            }
        }

        let file = FilePatch {
            old_path,
            new_path,
            old_mode,
            new_mode,
            is_new_file,
            is_deleted,
            is_rename,
            is_copy,
            is_binary,
            hunks,
            similarity,
        };

        Ok((file, i))
    }

    /// Parse a unified diff section
    fn parse_unified_diff(lines: &[&str], start: usize) -> Result<(FilePatch, usize)> {
        let mut i = start;

        // Parse --- line
        let old_path = Self::parse_file_line(lines[i], "--- ");
        i += 1;

        // Parse +++ line
        let new_path = if i < lines.len() {
            Self::parse_file_line(lines[i], "+++ ")
        } else {
            None
        };
        i += 1;

        let is_new_file = old_path.is_none()
            || old_path
                .as_ref()
                .is_some_and(|p| p.to_string_lossy() == "/dev/null");
        let is_deleted = new_path.is_none()
            || new_path
                .as_ref()
                .is_some_and(|p| p.to_string_lossy() == "/dev/null");

        // Parse hunks
        let mut hunks = Vec::new();
        while i < lines.len() && lines[i].starts_with("@@") {
            let (hunk, new_i) = Self::parse_hunk(lines, i)?;
            hunks.push(hunk);
            i = new_i;
        }

        let file = FilePatch {
            old_path,
            new_path,
            old_mode: None,
            new_mode: None,
            is_new_file,
            is_deleted,
            is_rename: false,
            is_copy: false,
            is_binary: false,
            hunks,
            similarity: None,
        };

        Ok((file, i))
    }

    /// Parse a hunk
    fn parse_hunk(lines: &[&str], start: usize) -> Result<(Hunk, usize)> {
        let header = lines[start];

        // Parse hunk header: @@ -start,count +start,count @@ section
        let header_parts: Vec<&str> = header.split("@@").collect();
        if header_parts.len() < 2 {
            return Err(FunveilError::TreeSitterError(format!(
                "Invalid hunk header: {header}"
            )));
        }

        let ranges = header_parts[1].trim();
        let (old_start, old_count, new_start, new_count) = Self::parse_hunk_ranges(ranges)?;

        let section = if header_parts.len() > 2 {
            Some(header_parts[2].trim().to_string())
        } else {
            None
        };

        // Parse hunk lines
        let mut hunk_lines = Vec::new();
        let mut i = start + 1;

        while i < lines.len() {
            let line = lines[i];

            // Stop at next hunk or file
            if line.starts_with("@@") || line.starts_with("diff --git") || line.starts_with("--- ")
            {
                break;
            }

            if let Some(stripped) = line.strip_prefix(' ') {
                hunk_lines.push(Line::Context(stripped.to_string()));
            } else if let Some(stripped) = line.strip_prefix('-') {
                hunk_lines.push(Line::Delete(stripped.to_string()));
            } else if let Some(stripped) = line.strip_prefix('+') {
                hunk_lines.push(Line::Add(stripped.to_string()));
            } else if line.starts_with("\\ No newline") {
                hunk_lines.push(Line::NoNewline);
            } else if line.is_empty() {
                // Empty line in context
                hunk_lines.push(Line::Context(String::new()));
            }

            i += 1;
        }

        let hunk = Hunk {
            old_start,
            old_count,
            new_start,
            new_count,
            section,
            lines: hunk_lines,
        };

        Ok((hunk, i))
    }

    /// Parse hunk ranges like "-1,5 +1,5"
    fn parse_hunk_ranges(ranges: &str) -> Result<(usize, usize, usize, usize)> {
        let parts: Vec<&str> = ranges.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(FunveilError::TreeSitterError(format!(
                "Invalid hunk ranges: {ranges}"
            )));
        }

        let (old_start, old_count) = Self::parse_range(parts[0], '-')?;
        let (new_start, new_count) = Self::parse_range(parts[1], '+')?;

        Ok((old_start, old_count, new_start, new_count))
    }

    /// Parse a single range like "-50,10" -> (50, 10)
    fn parse_range(range: &str, prefix: char) -> Result<(usize, usize)> {
        if !range.starts_with(prefix) {
            return Err(FunveilError::TreeSitterError(format!(
                "Range should start with {prefix}: {range}"
            )));
        }

        let rest = &range[1..];
        let parts: Vec<&str> = rest.split(',').collect();

        let start = parts[0].parse().map_err(|_| {
            FunveilError::TreeSitterError(format!("Invalid range number: {}", parts[0]))
        })?;

        let count = if parts.len() > 1 {
            parts[1].parse().map_err(|_| {
                FunveilError::TreeSitterError(format!("Invalid range count: {}", parts[1]))
            })?
        } else {
            1 // Default count is 1
        };

        Ok((start, count))
    }

    /// Detect the format of a patch
    pub fn detect_format(input: &str) -> PatchFormat {
        let trimmed = input.trim_start();

        if trimmed.starts_with("diff --git") {
            PatchFormat::GitDiff
        } else {
            PatchFormat::UnifiedDiff
        }
    }

    /// Clean path (remove a/ or b/ prefix)
    fn clean_path(path: &str) -> Option<PathBuf> {
        let cleaned = path
            .trim_start_matches("a/")
            .trim_start_matches("b/")
            .trim_matches('"');

        if cleaned == "/dev/null" {
            None
        } else {
            Some(PathBuf::from(cleaned))
        }
    }

    /// Parse file line (--- or +++)
    fn parse_file_line(line: &str, prefix: &str) -> Option<PathBuf> {
        let rest = line.strip_prefix(prefix)?;

        // Handle quoted paths
        if rest.starts_with('"') {
            let end = rest.rfind('"').unwrap_or(rest.len());
            let path = &rest[1..end];
            if path == "/dev/null" {
                None
            } else {
                Some(PathBuf::from(path))
            }
        } else {
            let path = rest.split('\t').next()?;
            if path == "/dev/null" {
                None
            } else {
                Some(PathBuf::from(
                    path.trim_start_matches("a/").trim_start_matches("b/"),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_unified_diff() {
        let patch = r#"--- a/file.txt
+++ b/file.txt
@@ -1,5 +1,5 @@
 line 1
 line 2
-line 3
+line 3 modified
 line 4
 line 5
"#;
        let result = PatchParser::parse_patch(patch);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let parsed = result.unwrap();
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.format, PatchFormat::UnifiedDiff);

        let file = &parsed.files[0];
        assert_eq!(file.old_path, Some(PathBuf::from("file.txt")));
        assert_eq!(file.new_path, Some(PathBuf::from("file.txt")));
        assert_eq!(file.hunks.len(), 1);

        let hunk = &file.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.new_start, 1);
        // Hunk has 6 lines: context, context, delete, add, context, context
        assert_eq!(hunk.lines.len(), 6);
    }

    #[test]
    fn test_parse_git_diff() {
        let patch = r#"diff --git a/src/main.rs b/src/main.rs
index a3f5d2e..b8e9c4f 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,7 +10,8 @@ fn main() {
     println!("Hello");
-    let x = 5;
+    let x = 10;
+    let y = 20;
     println!("{}", x);
 }
"#;
        let result = PatchParser::parse_patch(patch);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let parsed = result.unwrap();
        assert_eq!(parsed.format, PatchFormat::GitDiff);
        assert_eq!(parsed.files.len(), 1);
    }

    #[test]
    fn test_parse_multi_file_diff() {
        let patch = r#"diff --git a/file1.txt b/file1.txt
index 111..222 100644
--- a/file1.txt
+++ b/file1.txt
@@ -1 +1 @@
-old
+new

diff --git a/file2.txt b/file2.txt
index 333..444 100644
--- a/file2.txt
+++ b/file2.txt
@@ -1 +1 @@
-foo
+bar
"#;
        let result = PatchParser::parse_patch(patch);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let parsed = result.unwrap();
        assert_eq!(parsed.files.len(), 2);
    }

    #[test]
    fn test_parse_file_rename() {
        let patch = r#"diff --git a/old_name.txt b/new_name.txt
similarity index 98%
rename from old_name.txt
rename to new_name.txt
index a3f5d2e..b8e9c4f 100644
--- a/old_name.txt
+++ b/new_name.txt
@@ -5,3 +5,3 @@
 unchanged
-old content
+new content
 unchanged
"#;
        let result = PatchParser::parse_patch(patch);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let parsed = result.unwrap();
        let file = &parsed.files[0];
        assert!(file.is_rename);
        assert_eq!(file.similarity, Some(98));
    }

    #[test]
    fn test_parse_new_file() {
        let patch = r#"diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..a3f5d2e
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,3 @@
+line 1
+line 2
+line 3
"#;
        let result = PatchParser::parse_patch(patch);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let parsed = result.unwrap();
        let file = &parsed.files[0];
        assert!(file.is_new_file);
        assert_eq!(file.old_path, None);
        assert_eq!(file.new_path, Some(PathBuf::from("new.txt")));
    }

    #[test]
    fn test_parse_deleted_file() {
        let patch = r#"diff --git a/deleted.txt b/deleted.txt
deleted file mode 100644
index a3f5d2e..0000000
--- a/deleted.txt
+++ /dev/null
@@ -1,3 +0,0 @@
-line 1
-line 2
-line 3
"#;
        let result = PatchParser::parse_patch(patch);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let parsed = result.unwrap();
        let file = &parsed.files[0];
        assert!(file.is_deleted);
        assert_eq!(file.new_path, None);
    }

    #[test]
    fn test_parse_binary_diff() {
        let patch = r#"diff --git a/image.png b/image.png
index a3f5d2e..b8e9c4f 100644
Binary files a/image.png and b/image.png differ
"#;
        let result = PatchParser::parse_patch(patch);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let parsed = result.unwrap();
        let file = &parsed.files[0];
        assert!(file.is_binary);
    }

    #[test]
    fn test_parse_empty_file_creation() {
        let patch = r#"diff --git a/empty.txt b/empty.txt
new file mode 100644
index 0000000..e69de29
--- /dev/null
+++ b/empty.txt
"#;
        let result = PatchParser::parse_patch(patch);
        assert!(result.is_ok(), "Error: {:?}", result.err());

        let parsed = result.unwrap();
        assert!(parsed.files[0].is_new_file);
    }

    #[test]
    fn test_detect_format() {
        assert_eq!(
            PatchParser::detect_format("diff --git a/f b/f"),
            PatchFormat::GitDiff
        );
        assert_eq!(
            PatchParser::detect_format("--- a/f\n+++ b/f"),
            PatchFormat::UnifiedDiff
        );
    }

    #[test]
    fn test_parse_empty_input() {
        let patch = "";
        let result = PatchParser::parse_patch(patch);
        // Empty input returns empty file list (not an error)
        assert!(result.is_ok());
        assert_eq!(result.unwrap().files.len(), 0);
    }
}
