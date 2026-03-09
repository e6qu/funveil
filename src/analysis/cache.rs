//! Caching layer for parsed code analysis data.
//!
//! This module provides efficient caching of parsing results to avoid
//! re-parsing unchanged files. Uses mtime + content hash for invalidation.

use crate::parser::ParsedFile;
use crate::types::ContentHash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Version of the cache format (for migration support)
const CACHE_VERSION: u32 = 1;

/// Default cache directory name
const CACHE_DIR: &str = ".funveil/analysis";

/// Default cache file name
const CACHE_FILE: &str = "index.bin";

/// Metadata for a cached file entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileMetadata {
    /// File modification time (seconds since epoch)
    mtime: u64,
    /// File size in bytes
    size: u64,
    /// Content hash for validation
    content_hash: String,
    /// Cached parsed data
    parsed: ParsedFile,
}

/// The analysis cache
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalysisCache {
    /// Cache format version
    version: u32,
    /// Creation timestamp
    created_at: u64,
    /// Map from file path to cached data
    entries: HashMap<PathBuf, FileMetadata>,
}

impl Default for AnalysisCache {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            version: CACHE_VERSION,
            created_at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            entries: HashMap::new(),
        }
    }

    /// Get the cache directory for a project
    fn cache_dir(root: &Path) -> PathBuf {
        root.join(CACHE_DIR)
    }

    /// Get the cache file path
    fn cache_path(root: &Path) -> PathBuf {
        Self::cache_dir(root).join(CACHE_FILE)
    }

    /// Load cache from disk
    pub fn load(root: &Path) -> crate::error::Result<Self> {
        let cache_path = Self::cache_path(root);

        if !cache_path.exists() {
            return Ok(Self::new());
        }

        let data = fs::read(&cache_path)?;
        let cache: AnalysisCache = postcard::from_bytes(&data)
            .map_err(|e| crate::error::FunveilError::CacheError(format!("deserialize: {e}")))?;

        // Check version compatibility
        if cache.version != CACHE_VERSION {
            // Version mismatch - return empty cache
            return Ok(Self::new());
        }

        Ok(cache)
    }

    /// Save cache to disk
    pub fn save(&self, root: &Path) -> crate::error::Result<()> {
        let cache_dir = Self::cache_dir(root);
        let cache_path = cache_dir.join(CACHE_FILE);

        // Create cache directory if it doesn't exist
        fs::create_dir_all(&cache_dir)?;

        let data = postcard::to_allocvec(self)
            .map_err(|e| crate::error::FunveilError::CacheError(format!("serialize: {e}")))?;

        let mut file = fs::File::create(cache_path)?;
        file.write_all(&data)?;

        Ok(())
    }

    /// Get file metadata (mtime, size)
    fn get_file_info(path: &Path) -> Option<(u64, u64, String)> {
        let metadata = fs::metadata(path).ok()?;
        let mtime = metadata
            .modified()
            .ok()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs();
        let size = metadata.len();

        // Compute content hash
        let content = fs::read(path).ok()?;
        let hash = ContentHash::from_content(&content);

        Some((mtime, size, hash.to_string()))
    }

    /// Check if a file needs to be re-parsed
    fn is_stale(&self, path: &Path) -> bool {
        let Some(entry) = self.entries.get(path) else {
            return true; // Not in cache
        };

        let Some((mtime, size, hash)) = Self::get_file_info(path) else {
            return true; // File not accessible
        };

        // Check if file has changed
        if mtime != entry.mtime || size != entry.size {
            return true;
        }

        // Double-check with content hash
        hash != entry.content_hash
    }

    /// Get a cached entry if valid
    pub fn get(&self, path: &Path) -> Option<&ParsedFile> {
        if self.is_stale(path) {
            None
        } else {
            self.entries.get(path).map(|e| &e.parsed)
        }
    }

    /// Insert a parsed file into the cache
    pub fn insert(&mut self, path: PathBuf, parsed: ParsedFile) {
        if let Some((mtime, size, content_hash)) = Self::get_file_info(&path) {
            self.entries.insert(
                path,
                FileMetadata {
                    mtime,
                    size,
                    content_hash,
                    parsed,
                },
            );
        }
    }

    /// Remove a file from the cache
    pub fn remove(&mut self, path: &Path) {
        self.entries.remove(path);
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            version: self.version,
            created_at: self.created_at,
            entry_count: self.entries.len(),
            total_size_bytes: self.entries.values().map(|e| e.size).sum(),
        }
    }

    /// Get all valid cached entries
    pub fn get_all_valid(&self, _root: &Path) -> Vec<(PathBuf, &ParsedFile)> {
        self.entries
            .iter()
            .filter(|(path, _)| !self.is_stale(path))
            .map(|(path, entry)| (path.clone(), &entry.parsed))
            .collect()
    }

    /// Invalidate stale entries
    pub fn invalidate_stale(&mut self) {
        let stale_paths: Vec<_> = self
            .entries
            .keys()
            .filter(|path| self.is_stale(path))
            .cloned()
            .collect();

        for path in stale_paths {
            self.entries.remove(&path);
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Cache format version
    pub version: u32,
    /// Creation timestamp
    pub created_at: u64,
    /// Number of cached entries
    pub entry_count: usize,
    /// Total size of cached source files
    pub total_size_bytes: u64,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Cache Statistics:")?;
        writeln!(f, "  Version: {}", self.version)?;
        writeln!(f, "  Entries: {}", self.entry_count)?;
        writeln!(f, "  Total source size: {} bytes", self.total_size_bytes)?;
        let created = std::time::UNIX_EPOCH + std::time::Duration::from_secs(self.created_at);
        writeln!(f, "  Created: {created:?}")
    }
}

/// Cache-aware parser wrapper
pub struct CachedParser {
    cache: AnalysisCache,
    root: PathBuf,
}

impl CachedParser {
    /// Create a new cached parser
    pub fn new(root: &Path) -> crate::error::Result<Self> {
        let cache = AnalysisCache::load(root)?;
        Ok(Self {
            cache,
            root: root.to_path_buf(),
        })
    }

    /// Get a parsed file (from cache or parse fresh)
    pub fn get_or_parse(
        &mut self,
        path: &Path,
        content: &str,
        parser: &crate::parser::TreeSitterParser,
    ) -> crate::error::Result<&ParsedFile> {
        // Check if we need to parse (not in cache or stale)
        let needs_parse = self.cache.get(path).is_none();

        if needs_parse {
            // Parse fresh
            let parsed = parser.parse_file(path, content)?;
            self.cache.insert(path.to_path_buf(), parsed);
        }

        // Return reference to the cached entry
        Ok(self.cache.get(path).unwrap())
    }

    /// Save the cache
    pub fn save(&self) -> crate::error::Result<()> {
        self.cache.save(&self.root)
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Invalidate stale entries
    pub fn invalidate_stale(&mut self) {
        self.cache.invalidate_stale();
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Language;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_cache_new() {
        let cache = AnalysisCache::new();
        let stats = cache.stats();
        assert_eq!(stats.version, CACHE_VERSION);
        assert_eq!(stats.entry_count, 0);
    }

    #[test]
    fn test_cache_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create a cache with some data
        let cache = AnalysisCache::new();

        // Save empty cache
        cache.save(root).unwrap();

        // Load it back
        let loaded = AnalysisCache::load(root).unwrap();
        assert_eq!(loaded.stats().entry_count, 0);
    }

    #[test]
    fn test_cache_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Loading non-existent cache should return empty cache
        let cache = AnalysisCache::load(root).unwrap();
        assert_eq!(cache.stats().entry_count, 0);
    }

    #[test]
    fn test_cache_stale_detection() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        // Create a test file
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"fn main() {}").unwrap();

        // Create cache with entry
        let mut cache = AnalysisCache::new();
        let parsed = ParsedFile::new(Language::Rust, file_path.clone());
        cache.insert(file_path.clone(), parsed);

        // Entry should be fresh
        assert!(!cache.is_stale(&file_path));
        assert!(cache.get(&file_path).is_some());

        // Modify the file
        std::thread::sleep(std::time::Duration::from_millis(100));
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"fn main() { println!(\"hello\"); }")
            .unwrap();

        // Entry should now be stale
        assert!(cache.is_stale(&file_path));
        assert!(cache.get(&file_path).is_none());
    }

    #[test]
    fn test_cache_clear() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        // Create a test file (required for insert to work)
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"fn main() {}").unwrap();

        let mut cache = AnalysisCache::new();
        let parsed = ParsedFile::new(Language::Rust, file_path.clone());

        cache.insert(file_path.clone(), parsed);
        assert_eq!(cache.stats().entry_count, 1);

        cache.clear();
        assert_eq!(cache.stats().entry_count, 0);
    }

    #[test]
    fn test_cache_remove() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"fn main() {}").unwrap();

        let mut cache = AnalysisCache::new();
        let parsed = ParsedFile::new(Language::Rust, file_path.clone());

        cache.insert(file_path.clone(), parsed);
        assert_eq!(cache.stats().entry_count, 1);

        cache.remove(&file_path);
        assert_eq!(cache.stats().entry_count, 0);
    }

    #[test]
    fn test_cache_get_all_valid() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"fn main() {}").unwrap();

        let mut cache = AnalysisCache::new();
        let parsed = ParsedFile::new(Language::Rust, file_path.clone());

        cache.insert(file_path.clone(), parsed);

        let valid = cache.get_all_valid(temp_dir.path());
        assert_eq!(valid.len(), 1);
    }

    #[test]
    fn test_cache_invalidate_stale() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"fn main() {}").unwrap();

        let mut cache = AnalysisCache::new();
        let parsed = ParsedFile::new(Language::Rust, file_path.clone());

        cache.insert(file_path.clone(), parsed);
        assert_eq!(cache.stats().entry_count, 1);

        // Modify the file to make it stale
        std::thread::sleep(std::time::Duration::from_millis(100));
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"fn main() { println!(\"modified\"); }")
            .unwrap();

        cache.invalidate_stale();
        assert_eq!(cache.stats().entry_count, 0);
    }

    #[test]
    fn test_cache_stats_display() {
        let cache = AnalysisCache::new();
        let stats = cache.stats();
        let display = format!("{stats}");
        assert!(display.contains("Cache Statistics"));
        assert!(display.contains("Entries: 0"));
    }

    #[test]
    fn test_cache_stats_total_size() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let content = b"fn main() { println!(\"hello world\"); }";
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(content).unwrap();

        let mut cache = AnalysisCache::new();
        let parsed = ParsedFile::new(Language::Rust, file_path.clone());
        cache.insert(file_path, parsed);

        let stats = cache.stats();
        assert!(stats.total_size_bytes > 0);
    }

    #[test]
    fn test_cached_parser_new() {
        let temp_dir = TempDir::new().unwrap();
        let parser = CachedParser::new(temp_dir.path());
        assert!(parser.is_ok());
    }

    #[test]
    fn test_cached_parser_stats() {
        let temp_dir = TempDir::new().unwrap();
        let parser = CachedParser::new(temp_dir.path()).unwrap();
        let stats = parser.stats();
        assert_eq!(stats.entry_count, 0);
    }

    #[test]
    fn test_cached_parser_save() {
        let temp_dir = TempDir::new().unwrap();
        let parser = CachedParser::new(temp_dir.path()).unwrap();
        let result = parser.save();
        assert!(result.is_ok());
    }

    #[test]
    fn test_cached_parser_clear() {
        let temp_dir = TempDir::new().unwrap();
        let mut parser = CachedParser::new(temp_dir.path()).unwrap();
        parser.clear();
        assert_eq!(parser.stats().entry_count, 0);
    }

    #[test]
    fn test_cached_parser_invalidate_stale() {
        let temp_dir = TempDir::new().unwrap();
        let mut parser = CachedParser::new(temp_dir.path()).unwrap();
        parser.invalidate_stale();
        assert_eq!(parser.stats().entry_count, 0);
    }

    #[test]
    fn test_cached_parser_get_or_parse() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        fs::write(&file_path, "fn main() {}").unwrap();

        let mut parser = CachedParser::new(temp_dir.path()).unwrap();
        let ts_parser = crate::parser::TreeSitterParser::new().unwrap();
        let result = parser.get_or_parse(&file_path, "fn main() {}", &ts_parser);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cache_default() {
        let cache = AnalysisCache::default();
        assert_eq!(cache.stats().entry_count, 0);
    }

    #[test]
    fn test_cache_load_version_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join(CACHE_DIR).join(CACHE_FILE);
        fs::create_dir_all(cache_path.parent().unwrap()).unwrap();

        let bad_cache = AnalysisCache {
            version: 999,
            created_at: 0,
            entries: HashMap::new(),
        };
        let encoded = postcard::to_allocvec(&bad_cache).unwrap();
        fs::write(&cache_path, encoded).unwrap();

        let loaded = AnalysisCache::load(temp_dir.path()).unwrap();
        assert_eq!(loaded.stats().entry_count, 0);
    }

    #[test]
    fn test_cache_is_stale_file_not_accessible() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        // Create the file first
        fs::write(&file_path, "fn main() {}").unwrap();

        let mut cache = AnalysisCache::new();
        let parsed = ParsedFile::new(Language::Rust, file_path.clone());
        cache.insert(file_path.clone(), parsed);

        // Delete the file
        fs::remove_file(&file_path).unwrap();

        assert!(cache.is_stale(&file_path));
    }
}
