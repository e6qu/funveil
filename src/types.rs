use crate::error::{FunveilError, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

/// A validated line range (1-indexed, start <= end)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LineRange {
    start: usize,
    end: usize,
}

impl LineRange {
    /// Create a new LineRange, validating that start <= end and both are >= 1
    pub fn new(start: usize, end: usize) -> Result<Self> {
        if start < 1 {
            return Err(FunveilError::InvalidLineRange {
                range: format!("{start}-{end}"),
                reason: "line numbers are 1-indexed, minimum is 1".to_string(),
            });
        }
        if start > end {
            return Err(FunveilError::InvalidLineRange {
                range: format!("{start}-{end}"),
                reason: "start must be <= end".to_string(),
            });
        }
        Ok(Self { start, end })
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn contains(&self, line: usize) -> bool {
        self.start <= line && line <= self.end
    }

    /// Check if this range overlaps with another
    pub fn overlaps(&self, other: &LineRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }

    /// Number of lines in this range
    pub fn len(&self) -> usize {
        self.end - self.start + 1
    }

    pub fn is_empty(&self) -> bool {
        false // LineRange always has at least 1 line
    }
}

impl fmt::Display for LineRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

impl FromStr for LineRange {
    type Err = FunveilError;

    fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 2 {
            return Err(FunveilError::InvalidLineRange {
                range: s.to_string(),
                reason: "expected format: start-end".to_string(),
            });
        }
        let start = parts[0]
            .parse::<usize>()
            .map_err(|_| FunveilError::InvalidLineRange {
                range: s.to_string(),
                reason: "start must be a number".to_string(),
            })?;
        let end = parts[1]
            .parse::<usize>()
            .map_err(|_| FunveilError::InvalidLineRange {
                range: s.to_string(),
                reason: "end must be a number".to_string(),
            })?;
        Self::new(start, end)
    }
}

/// A content hash (SHA-256 truncated to first 7 chars for display)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn from_content(content: &[u8]) -> Self {
        use sha2::Digest;
        let hash = Sha256::digest(content);
        Self(hex::encode(&hash[..]))
    }

    pub fn from_string(hash: String) -> Result<Self> {
        if hash.len() < 7 || hash.len() > 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(FunveilError::InvalidHash(hash));
        }
        Ok(Self(hash))
    }

    /// Get the full hash string
    pub fn full(&self) -> &str {
        &self.0
    }

    /// Get the short hash (first 7 chars) for display
    pub fn short(&self) -> &str {
        &self.0[..7.min(self.0.len())]
    }

    /// Get the 3-level prefix path components
    pub fn path_components(&self) -> (&str, &str, &str) {
        assert!(self.0.len() >= 7);
        (&self.0[0..2], &self.0[2..4], &self.0[4..])
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.short())
    }
}

/// Pattern type for matching files
#[derive(Debug, Clone)]
pub enum Pattern {
    Literal(String),
    Regex(Regex),
}

impl Pattern {
    pub fn from_literal(path: String) -> Self {
        Pattern::Literal(path)
    }

    pub fn from_regex(pattern: &str) -> Result<Self> {
        let regex = Regex::new(pattern).map_err(|e| FunveilError::InvalidRegex(e.to_string()))?;
        Ok(Pattern::Regex(regex))
    }

    /// Check if a file path matches this pattern
    pub fn matches(&self, file: &str) -> bool {
        match self {
            Pattern::Literal(path) => {
                if path.ends_with('/') {
                    // Directory match
                    file.starts_with(path)
                } else {
                    file == path
                }
            }
            Pattern::Regex(regex) => regex.is_match(file),
        }
    }

    pub fn is_literal(&self) -> bool {
        matches!(self, Pattern::Literal(_))
    }

    pub fn is_regex(&self) -> bool {
        matches!(self, Pattern::Regex(_))
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pattern::Literal(s) => write!(f, "{s}"),
            Pattern::Regex(r) => write!(f, "/{}/", r.as_str()),
        }
    }
}

/// Mode of operation
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Whitelist,
    Blacklist,
}

impl Mode {
    pub fn is_whitelist(&self) -> bool {
        matches!(self, Mode::Whitelist)
    }

    pub fn is_blacklist(&self) -> bool {
        matches!(self, Mode::Blacklist)
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Whitelist => write!(f, "whitelist"),
            Mode::Blacklist => write!(f, "blacklist"),
        }
    }
}

/// Parsed config entry (pattern + optional line ranges)
#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub pattern: Pattern,
    pub ranges: Option<Vec<LineRange>>,
}

impl ConfigEntry {
    pub fn new(pattern: Pattern, ranges: Option<Vec<LineRange>>) -> Self {
        Self { pattern, ranges }
    }

    pub fn parse(entry: &str) -> Result<Self> {
        if entry.starts_with("./") || entry.starts_with("../") {
            return Err(FunveilError::RelativePath(entry.to_string()));
        }

        if entry.starts_with('.') && !entry.starts_with('/') {
            return Err(FunveilError::HiddenFileWithoutPath(entry.to_string()));
        }

        if entry.starts_with('/') {
            let (pattern_str, ranges) = if let Some(pos) = entry.rfind("/#") {
                let (pat, rng) = entry.split_at(pos);
                let ranges_str = &rng[2..]; // Skip "/#"
                let ranges = Self::parse_ranges(ranges_str)?;
                (pat, Some(ranges))
            } else if entry.ends_with('/') {
                (entry, None)
            } else {
                return Err(FunveilError::InvalidRegex(
                    "regex patterns must end with /".to_string(),
                ));
            };

            if pattern_str.len() < 3 {
                return Err(FunveilError::InvalidRegex(
                    "empty regex pattern".to_string(),
                ));
            }
            let inner = &pattern_str[1..pattern_str.len() - 1];
            let pattern = Pattern::from_regex(inner)?;
            Ok(Self::new(pattern, ranges))
        } else {
            // Literal pattern
            let (path, ranges) = if let Some(pos) = entry.rfind('#') {
                let (path, rng) = entry.split_at(pos);
                let ranges_str = &rng[1..]; // Skip '#'
                match Self::parse_ranges(ranges_str) {
                    Ok(ranges) => (path.to_string(), Some(ranges)),
                    Err(_) => (entry.to_string(), None),
                }
            } else {
                (entry.to_string(), None)
            };

            // Check if it's a directory with ranges (error)
            if path.ends_with('/') && ranges.is_some() {
                return Err(FunveilError::DirectoryWithLineRanges(path));
            }

            Ok(Self::new(Pattern::Literal(path), ranges))
        }
    }

    fn parse_ranges(ranges_str: &str) -> Result<Vec<LineRange>> {
        let mut ranges = Vec::new();
        for range_str in ranges_str.split(',') {
            let parts: Vec<&str> = range_str.split('-').collect();
            if parts.len() != 2 {
                return Err(FunveilError::InvalidLineRange {
                    range: range_str.to_string(),
                    reason: "expected format: start-end".to_string(),
                });
            }
            let start = parts[0]
                .parse::<usize>()
                .map_err(|_| FunveilError::InvalidLineRange {
                    range: range_str.to_string(),
                    reason: "start must be a number".to_string(),
                })?;
            let end = parts[1]
                .parse::<usize>()
                .map_err(|_| FunveilError::InvalidLineRange {
                    range: range_str.to_string(),
                    reason: "end must be a number".to_string(),
                })?;
            ranges.push(LineRange::new(start, end)?);
        }

        // Check for overlaps
        for i in 0..ranges.len() {
            for j in (i + 1)..ranges.len() {
                if ranges[i].overlaps(&ranges[j]) {
                    return Err(FunveilError::OverlappingRanges);
                }
            }
        }

        Ok(ranges)
    }
}

/// Check if a path is a protected VCS directory
pub fn is_vcs_directory(path: &str) -> bool {
    const VCS_DIRS: &[&str] = &[
        ".git/",
        ".svn/",
        ".hg/",
        ".cvs/",
        ".bzr/",
        ".fslckout/",
        "_FOSSIL_/",
        "_darcs/",
        "CVS/",
    ];

    VCS_DIRS
        .iter()
        .any(|&vcs| path.starts_with(vcs) || path.contains(&format!("/{vcs}")))
}

/// Check if path is the funveil config or data directory
pub fn is_funveil_protected(path: &str) -> bool {
    path == ".funveil_config"
        || path.starts_with(".funveil/")
        || path.starts_with(".funveil_config/")
}

/// Validate that a path resolves within the root directory.
/// Returns an error if the path is a symlink that escapes the root.
pub fn validate_path_within_root(path: &Path, root: &Path) -> std::io::Result<()> {
    let canonical_path = path.canonicalize()?;
    let canonical_root = root.canonicalize()?;

    if !canonical_path.starts_with(&canonical_root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Path '{}' resolves outside project root", path.display()),
        ));
    }

    Ok(())
}

/// Check if a file is binary (simple heuristic)
pub fn is_binary_file(path: &Path) -> bool {
    // Check common binary extensions
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        let binary_exts = [
            "exe", "dll", "so", "dylib", "bin", "o", "a", "lib", "png", "jpg", "jpeg", "gif",
            "bmp", "ico", "webp", "mp3", "mp4", "avi", "mov", "mkv", "wav", "flac", "zip", "tar",
            "gz", "bz2", "xz", "7z", "rar", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
            "sqlite", "db", "mdb",
        ];
        if binary_exts.contains(&ext.as_str()) {
            return true;
        }
    }

    // Check for null bytes in first 8KB
    if let Ok(file) = std::fs::File::open(path) {
        use std::io::Read;
        let mut buf = vec![0u8; 8192];
        if let Ok(n) = file.take(8192).read(&mut buf) {
            return buf[..n].contains(&0);
        }
    }

    false
}

pub const ORIGINAL_SUFFIX: &str = "#_original";

/// Parsed representation of a config object key.
///
/// Keys can be:
/// - `file` → full-file veil
/// - `file#1-10` → range veil
/// - `file#_original` → original content backup
///
/// Handles files with `#` in their name (BUG-100) by using `rfind('#')`
/// and validating the suffix.
#[derive(Debug, PartialEq, Eq)]
pub enum ConfigKey<'a> {
    FullVeil { file: &'a str },
    Range { file: &'a str, range: LineRange },
    Original { file: &'a str },
}

impl<'a> ConfigKey<'a> {
    /// Parse a config key string into its components.
    pub fn parse(key: &'a str) -> Self {
        if let Some(pos) = key.rfind('#') {
            let suffix = &key[pos + 1..];
            if suffix == "_original" {
                ConfigKey::Original { file: &key[..pos] }
            } else if let Ok(range) = LineRange::from_str(suffix) {
                ConfigKey::Range {
                    file: &key[..pos],
                    range,
                }
            } else {
                ConfigKey::FullVeil { file: key }
            }
        } else {
            ConfigKey::FullVeil { file: key }
        }
    }

    /// Extract the filename from any variant.
    pub fn file(&self) -> &str {
        match self {
            ConfigKey::FullVeil { file } => file,
            ConfigKey::Range { file, .. } => file,
            ConfigKey::Original { file } => file,
        }
    }

    /// Build a range key string: `"file#1-10"`.
    pub fn range_key(file: &str, range: &LineRange) -> String {
        format!("{file}#{range}")
    }

    /// Build an original key string: `"file#_original"`.
    pub fn original_key(file: &str) -> String {
        format!("{file}{ORIGINAL_SUFFIX}")
    }

    /// Build a file prefix for iteration: `"file#"`.
    pub fn file_prefix(file: &str) -> String {
        format!("{file}#")
    }
}

// Hex encoding helper
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            write!(&mut s, "{b:02x}").unwrap();
        }
        s
    }
}

pub fn parse_pattern(pattern: &str) -> Result<(&str, Option<Vec<LineRange>>)> {
    if let Some(pos) = pattern.rfind('#') {
        let file = &pattern[..pos];
        let ranges_str = &pattern[pos + 1..];

        if file.is_empty() {
            return Err(FunveilError::InvalidLineRange {
                range: pattern.to_string(),
                reason: "Empty file path in pattern".to_string(),
            });
        }
        if ranges_str.is_empty() {
            return Err(FunveilError::InvalidLineRange {
                range: pattern.to_string(),
                reason: "Empty range specification after '#'".to_string(),
            });
        }

        let mut ranges = Vec::new();
        let mut valid_ranges = true;
        for range_str in ranges_str.split(',') {
            let parts: Vec<&str> = range_str.split('-').collect();
            if parts.len() != 2 {
                valid_ranges = false;
                break;
            }
            match (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                (Ok(start), Ok(end)) => match LineRange::new(start, end) {
                    Ok(range) => ranges.push(range),
                    Err(_) => {
                        valid_ranges = false;
                        break;
                    }
                },
                _ => {
                    valid_ranges = false;
                    break;
                }
            }
        }

        if valid_ranges {
            Ok((file, Some(ranges)))
        } else {
            Ok((pattern, None))
        }
    } else {
        Ok((pattern, None))
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_key_parse_full_veil() {
        assert_eq!(
            ConfigKey::parse("src/main.rs"),
            ConfigKey::FullVeil {
                file: "src/main.rs"
            }
        );
    }

    #[test]
    fn test_config_key_parse_range() {
        let parsed = ConfigKey::parse("src/main.rs#1-10");
        match parsed {
            ConfigKey::Range { file, range } => {
                assert_eq!(file, "src/main.rs");
                assert_eq!(range.start(), 1);
                assert_eq!(range.end(), 10);
            }
            _ => panic!("expected Range variant"),
        }
    }

    #[test]
    fn test_config_key_parse_original() {
        assert_eq!(
            ConfigKey::parse("src/main.rs#_original"),
            ConfigKey::Original {
                file: "src/main.rs"
            }
        );
    }

    #[test]
    fn test_config_key_parse_hash_in_filename() {
        // BUG-100: file with # in name but invalid suffix → FullVeil
        assert_eq!(
            ConfigKey::parse("dir/file#name.txt"),
            ConfigKey::FullVeil {
                file: "dir/file#name.txt"
            }
        );
    }

    #[test]
    fn test_config_key_parse_hash_in_filename_with_range() {
        // BUG-100: "dir/file#name.txt#1-10" → Range with file="dir/file#name.txt"
        let parsed = ConfigKey::parse("dir/file#name.txt#1-10");
        match parsed {
            ConfigKey::Range { file, range } => {
                assert_eq!(file, "dir/file#name.txt");
                assert_eq!(range.start(), 1);
                assert_eq!(range.end(), 10);
            }
            _ => panic!("expected Range variant"),
        }
    }

    #[test]
    fn test_config_key_file() {
        assert_eq!(ConfigKey::parse("foo.rs").file(), "foo.rs");
        assert_eq!(ConfigKey::parse("foo.rs#1-5").file(), "foo.rs");
        assert_eq!(ConfigKey::parse("foo.rs#_original").file(), "foo.rs");
    }

    #[test]
    fn test_config_key_builders() {
        let range = LineRange::new(1, 10).unwrap();
        assert_eq!(ConfigKey::range_key("f.rs", &range), "f.rs#1-10");
        assert_eq!(ConfigKey::original_key("f.rs"), "f.rs#_original");
        assert_eq!(ConfigKey::file_prefix("f.rs"), "f.rs#");
    }

    #[test]
    fn test_line_range_validation() {
        assert!(LineRange::new(1, 10).is_ok());
        assert!(LineRange::new(5, 5).is_ok()); // Single line
        assert!(LineRange::new(0, 10).is_err()); // 0 is invalid
        assert!(LineRange::new(10, 5).is_err()); // start > end
    }

    #[test]
    fn test_line_range_overlap() {
        let r1 = LineRange::new(1, 10).unwrap();
        let r2 = LineRange::new(5, 15).unwrap();
        let r3 = LineRange::new(11, 20).unwrap();

        assert!(r1.overlaps(&r2));
        assert!(r2.overlaps(&r1));
        assert!(!r1.overlaps(&r3));
    }

    #[test]
    fn test_content_hash() {
        let hash = ContentHash::from_content(b"hello world");
        assert_eq!(hash.short().len(), 7);
        assert_eq!(hash.full().len(), 64); // SHA-256 hex = 64 chars

        let (a, b, c) = hash.path_components();
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        assert!(!c.is_empty());
    }

    #[test]
    fn test_pattern_matching() {
        let lit = Pattern::Literal("src/main.rs".to_string());
        assert!(lit.matches("src/main.rs"));
        assert!(!lit.matches("src/other.rs"));

        let dir = Pattern::Literal("src/".to_string());
        assert!(dir.matches("src/main.rs"));
        assert!(dir.matches("src/lib/helper.rs"));
        assert!(!dir.matches("other/src/file.rs"));

        let regex = Pattern::from_regex(r".*\.rs$").unwrap();
        assert!(regex.matches("main.rs"));
        assert!(regex.matches("src/lib.rs"));
        assert!(!regex.matches("main.py"));
    }

    #[test]
    fn test_config_entry_parsing() {
        let entry = ConfigEntry::parse("src/main.rs#10-20").unwrap();
        assert!(entry.pattern.is_literal());
        assert!(entry.ranges.is_some());

        let entry = ConfigEntry::parse("/.*\\.env$/").unwrap();
        assert!(entry.pattern.is_regex());
        assert!(entry.ranges.is_none());

        assert!(ConfigEntry::parse("./relative.rs").is_err());
        assert!(ConfigEntry::parse("../parent.rs").is_err());
        assert!(ConfigEntry::parse("src/#10-20").is_err()); // directory with ranges
    }

    #[test]
    fn test_vcs_detection() {
        assert!(is_vcs_directory(".git/config"));
        assert!(is_vcs_directory("src/.git/objects"));
        assert!(is_vcs_directory(".svn/entries"));
        assert!(!is_vcs_directory("src/main.rs"));
    }

    #[test]
    fn test_line_range_display() {
        let range = LineRange::new(5, 10).unwrap();
        assert_eq!(format!("{range}"), "5-10");
    }

    #[test]
    fn test_line_range_from_str() {
        let range: LineRange = "5-10".parse().unwrap();
        assert_eq!(range.start(), 5);
        assert_eq!(range.end(), 10);
    }

    #[test]
    fn test_line_range_from_str_errors() {
        assert!("5".parse::<LineRange>().is_err());
        assert!("5-10-15".parse::<LineRange>().is_err());
        assert!("abc-10".parse::<LineRange>().is_err());
        assert!("5-xyz".parse::<LineRange>().is_err());
        assert!("0-10".parse::<LineRange>().is_err());
        assert!("10-5".parse::<LineRange>().is_err());
    }

    #[test]
    fn test_line_range_is_empty() {
        let range = LineRange::new(1, 1).unwrap();
        assert!(!range.is_empty());
    }

    #[test]
    fn test_pattern_display() {
        let lit = Pattern::Literal("src/main.rs".to_string());
        assert_eq!(format!("{lit}"), "src/main.rs");

        let regex = Pattern::from_regex(r".*\.rs$").unwrap();
        assert_eq!(format!("{regex}"), "/.*\\.rs$/");
    }

    #[test]
    fn test_pattern_is_literal_is_regex() {
        let lit = Pattern::Literal("test".to_string());
        assert!(lit.is_literal());
        assert!(!lit.is_regex());

        let regex = Pattern::from_regex("test").unwrap();
        assert!(!regex.is_literal());
        assert!(regex.is_regex());
    }

    #[test]
    fn test_config_entry_hidden_file() {
        let result = ConfigEntry::parse(".env");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_entry_regex_with_ranges() {
        let entry = ConfigEntry::parse("/.*\\.rs/#10-20").unwrap();
        assert!(entry.pattern.is_regex());
        assert!(entry.ranges.is_some());
    }

    #[test]
    fn test_config_entry_regex_without_ending_slash() {
        let result = ConfigEntry::parse("/.*\\.rs");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_entry_invalid_regex() {
        let result = ConfigEntry::parse("/[invalid/");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_entry_overlapping_ranges() {
        // BUG-124: overlapping ranges now fall through to literal filename
        let entry = ConfigEntry::parse("file.txt#1-10,5-15").unwrap();
        assert!(entry.pattern.is_literal());
        assert!(entry.ranges.is_none());
    }

    #[test]
    fn test_config_entry_directory_with_ranges() {
        let result = ConfigEntry::parse("src/#10-20");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_funveil_protected() {
        assert!(is_funveil_protected(".funveil_config"));
        assert!(is_funveil_protected(".funveil/objects"));
        assert!(is_funveil_protected(".funveil_config/backup"));
        assert!(!is_funveil_protected("src/main.rs"));
    }

    #[test]
    fn test_is_vcs_directory_all_types() {
        assert!(is_vcs_directory(".git/HEAD"));
        assert!(is_vcs_directory(".svn/entries"));
        assert!(is_vcs_directory(".hg/store"));
        assert!(is_vcs_directory(".cvs/Root"));
        assert!(is_vcs_directory("_darcs/patches"));
        assert!(is_vcs_directory("CVS/Entries"));
        assert!(is_vcs_directory("project/.git/hooks"));
        assert!(!is_vcs_directory("gitignore"));
    }

    #[test]
    fn test_validate_path_within_root() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();
        let valid_path = root.join("src/main.rs");
        std::fs::create_dir_all(valid_path.parent().unwrap()).unwrap();
        std::fs::write(&valid_path, "").unwrap();

        assert!(validate_path_within_root(&valid_path, root).is_ok());
    }

    #[test]
    fn test_is_binary_file_by_extension() {
        let temp = tempfile::TempDir::new().unwrap();

        let exe = temp.path().join("program.exe");
        std::fs::write(&exe, "text").unwrap();
        assert!(is_binary_file(&exe));

        let png = temp.path().join("image.png");
        std::fs::write(&png, "text").unwrap();
        assert!(is_binary_file(&png));

        let pdf = temp.path().join("doc.pdf");
        std::fs::write(&pdf, "text").unwrap();
        assert!(is_binary_file(&pdf));

        let txt = temp.path().join("readme.txt");
        std::fs::write(&txt, "text").unwrap();
        assert!(!is_binary_file(&txt));
    }

    #[test]
    fn test_is_binary_file_by_content() {
        let temp = tempfile::TempDir::new().unwrap();

        let binary = temp.path().join("unknown");
        std::fs::write(&binary, b"hello\x00world").unwrap();
        assert!(is_binary_file(&binary));

        let text = temp.path().join("unknown2");
        std::fs::write(&text, "hello world").unwrap();
        assert!(!is_binary_file(&text));
    }

    #[test]
    fn test_is_binary_file_nonexistent() {
        let path = std::path::Path::new("/nonexistent/file");
        assert!(!is_binary_file(path));
    }

    #[test]
    fn test_mode_display() {
        assert_eq!(format!("{}", Mode::Whitelist), "whitelist");
        assert_eq!(format!("{}", Mode::Blacklist), "blacklist");
    }

    #[test]
    fn test_mode_checks() {
        assert!(Mode::Whitelist.is_whitelist());
        assert!(!Mode::Whitelist.is_blacklist());
        assert!(!Mode::Blacklist.is_whitelist());
        assert!(Mode::Blacklist.is_blacklist());
    }

    #[test]
    fn test_content_hash_from_string() {
        let valid = "a".repeat(64);
        let hash = ContentHash::from_string(valid.clone()).unwrap();
        assert_eq!(hash.full(), valid);
    }

    #[test]
    fn test_content_hash_from_string_too_short() {
        assert!(ContentHash::from_string("".to_string()).is_err());
        assert!(ContentHash::from_string("ab".to_string()).is_err());
        assert!(ContentHash::from_string("abc".to_string()).is_err());
        assert!(ContentHash::from_string("abc123".to_string()).is_err()); // 6 chars, min is 7
    }

    #[test]
    fn test_content_hash_from_string_non_hex() {
        assert!(ContentHash::from_string("ghijkl".to_string()).is_err());
        assert!(ContentHash::from_string("abc12z".to_string()).is_err());
        assert!(ContentHash::from_string("abc 12".to_string()).is_err());
        // 64 chars but non-hex
        assert!(ContentHash::from_string("z".repeat(64)).is_err());
    }

    #[test]
    fn test_content_hash_display() {
        let hash = ContentHash::from_content(b"test");
        let short = format!("{hash}");
        assert_eq!(short.len(), 7);
    }

    #[test]
    fn test_config_entry_literal_with_ranges() {
        let entry = ConfigEntry::parse("file.txt#1-10,20-30").unwrap();
        assert!(entry.pattern.is_literal());
        let ranges = entry.ranges.unwrap();
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn test_config_entry_literal_directory() {
        let entry = ConfigEntry::parse("src/").unwrap();
        assert!(entry.pattern.is_literal());
        assert!(entry.ranges.is_none());
    }

    #[test]
    fn test_config_entry_regex_directory() {
        let entry = ConfigEntry::parse("/src/.*/").unwrap();
        assert!(entry.pattern.is_regex());
        assert!(entry.ranges.is_none());
    }

    #[test]
    fn test_config_entry_empty_regex_slash() {
        let result = ConfigEntry::parse("/");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_entry_empty_regex_double_slash() {
        let result = ConfigEntry::parse("//");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_entry_empty_regex_with_ranges() {
        let result = ConfigEntry::parse("/#1-5");
        assert!(result.is_err());
    }

    #[test]
    fn test_pattern_from_literal() {
        let pattern = Pattern::from_literal("test.txt".to_string());
        assert!(pattern.is_literal());
        assert!(!pattern.is_regex());
    }

    #[test]
    fn test_config_entry_invalid_range_format() {
        // BUG-124: invalid suffix falls through to literal filename
        let entry = ConfigEntry::parse("file.txt#invalid").unwrap();
        assert!(entry.pattern.is_literal());
        assert!(entry.ranges.is_none());
    }

    #[test]
    fn test_config_entry_invalid_start_number() {
        // BUG-124: invalid suffix falls through to literal filename
        let entry = ConfigEntry::parse("file.txt#abc-10").unwrap();
        assert!(entry.pattern.is_literal());
        assert!(entry.ranges.is_none());
    }

    #[test]
    fn test_config_entry_invalid_end_number() {
        // BUG-124: invalid suffix falls through to literal filename
        let entry = ConfigEntry::parse("file.txt#10-xyz").unwrap();
        assert!(entry.pattern.is_literal());
        assert!(entry.ranges.is_none());
    }

    #[test]
    fn test_config_entry_invalid_range_no_dash() {
        // BUG-124: invalid suffix falls through to literal filename
        let entry = ConfigEntry::parse("file.txt#10").unwrap();
        assert!(entry.pattern.is_literal());
        assert!(entry.ranges.is_none());
    }

    // ── BUG-124: ConfigEntry::parse with '#' in filename ──

    #[test]
    fn test_bug124_config_entry_hash_in_filename() {
        // "file#name.txt" — '#' followed by non-range suffix should be treated as literal filename
        let entry = ConfigEntry::parse("file#name.txt").unwrap();
        assert!(entry.pattern.is_literal());
        assert!(entry.ranges.is_none());
        match &entry.pattern {
            Pattern::Literal(p) => assert_eq!(p, "file#name.txt"),
            _ => panic!("expected literal pattern"),
        }
    }

    #[test]
    fn test_bug124_config_entry_hash_in_filename_with_ranges() {
        // "file#name.txt#1-5" — last '#' should split correctly
        let entry = ConfigEntry::parse("file#name.txt#1-5").unwrap();
        assert!(entry.pattern.is_literal());
        assert!(entry.ranges.is_some());
        match &entry.pattern {
            Pattern::Literal(p) => assert_eq!(p, "file#name.txt"),
            _ => panic!("expected literal pattern"),
        }
    }

    #[test]
    fn test_is_binary_file_text() {
        let temp = tempfile::TempDir::new().unwrap();
        let text_file = temp.path().join("text_no_ext");
        std::fs::write(&text_file, "hello world, just text").unwrap();
        assert!(!is_binary_file(&text_file));
    }

    #[test]
    fn test_is_binary_file_with_null_byte() {
        let temp = tempfile::TempDir::new().unwrap();
        let bin_file = temp.path().join("has_null");
        std::fs::write(&bin_file, b"hello\x00world").unwrap();
        assert!(is_binary_file(&bin_file));
    }

    #[test]
    fn test_is_binary_file_null_after_8kb() {
        // BUG-008 regression: null byte only at position 8500 should not be detected
        // because we only check the first 8KB
        let temp = tempfile::TempDir::new().unwrap();
        let big_file = temp.path().join("big_file");
        let mut content = vec![b'A'; 8500];
        content[8500 - 1] = 0; // null byte at position 8500 (past 8192)
        std::fs::write(&big_file, &content).unwrap();
        assert!(!is_binary_file(&big_file));
    }

    #[test]
    fn test_is_vcs_directory_bzr() {
        assert!(is_vcs_directory(".bzr/config"));
        assert!(!is_vcs_directory("bzr/something"));
    }

    #[test]
    fn test_is_vcs_directory_fossil() {
        assert!(is_vcs_directory("_FOSSIL_/db"));
        assert!(!is_vcs_directory("_FOSSIL_data.txt"));
    }

    // --- Tests targeting specific missed mutants ---

    #[test]
    fn test_line_range_overlaps_adjacent_not_overlapping() {
        // 1-5 and 6-10 should NOT overlap. Catches <= → < mutation on line 48.
        let a = LineRange::new(1, 5).unwrap();
        let b = LineRange::new(6, 10).unwrap();
        assert!(!a.overlaps(&b));
        assert!(!b.overlaps(&a));
    }

    #[test]
    fn test_line_range_overlaps_touching() {
        // 1-5 and 5-10 should overlap. Catches <= → < mutation.
        let a = LineRange::new(1, 5).unwrap();
        let b = LineRange::new(5, 10).unwrap();
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
    }

    #[test]
    fn test_line_range_contains_boundaries() {
        // Catches <= → < mutations on both comparisons in contains()
        let r = LineRange::new(5, 10).unwrap();
        assert!(r.contains(5)); // start boundary
        assert!(r.contains(10)); // end boundary
        assert!(!r.contains(4)); // just below
        assert!(!r.contains(11)); // just above
    }

    #[test]
    fn test_validate_path_escape() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();

        let outside_dir = tempfile::TempDir::new().unwrap();
        let outside_file = outside_dir.path().join("outside.txt");
        std::fs::write(&outside_file, "test").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = root.join("escape_link");
            let _ = symlink(&outside_file, &link);

            if link.exists() {
                let result = validate_path_within_root(&link, root);
                assert!(result.is_err());
            }
        }
    }
}
