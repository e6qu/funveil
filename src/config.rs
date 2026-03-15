use crate::error::Result;
use crate::types::{ConfigEntry, ConfigKey, ContentHash, LineRange, Mode, ORIGINAL_SUFFIX};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

pub const CONFIG_FILE: &str = ".funveil_config";
pub const DATA_DIR: &str = ".funveil";
pub const OBJECTS_DIR: &str = ".funveil/objects";
pub const CHECKPOINTS_DIR: &str = ".funveil/checkpoints";
pub const LOGS_DIR: &str = ".funveil/logs";
pub const HISTORY_DIR: &str = ".funveil/history";
pub const METADATA_DIR: &str = ".funveil/metadata";

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "rs", "go", "ts", "tsx", "js", "jsx", "py", "sh", "bash", "tf", "tfvars", "hcl", "yaml", "yml",
    "html", "htm", "css", "xml", "md", "zig",
];

/// Check if a file path has a supported source extension.
pub fn is_supported_source(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| SUPPORTED_EXTENSIONS.contains(&ext))
}

/// Standard WalkBuilder with funveil defaults.
/// Returns the builder so callers can chain `.max_depth()` etc.
pub fn walk_files(root: &Path) -> ignore::WalkBuilder {
    let mut wb = ignore::WalkBuilder::new(root);
    wb.hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .require_git(false);
    wb
}

/// Object metadata stored in config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMeta {
    pub hash: String,
    pub permissions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

impl ObjectMeta {
    pub fn new(hash: ContentHash, permissions: u32) -> Self {
        Self {
            hash: hash.full().to_string(),
            permissions: crate::perms::format_mode(permissions),
            owner: None,
        }
    }

    pub fn hash(&self) -> Result<ContentHash> {
        ContentHash::from_string(self.hash.clone())
    }
}

/// The main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub whitelist: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blacklist: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub objects: HashMap<String, ObjectMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            mode: Mode::default(),
            whitelist: Vec::new(),
            blacklist: Vec::new(),
            objects: HashMap::new(),
            log_level: None,
        }
    }
}

impl Config {
    /// Create a new config with the given mode
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            log_level: None,
            ..Default::default()
        }
    }

    /// Load config from the project root
    pub fn load(root: &Path) -> Result<Self> {
        let config_path = root.join(CONFIG_FILE);
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to the project root
    pub fn save(&self, root: &Path) -> Result<()> {
        let config_path = root.join(CONFIG_FILE);
        let content = serde_yaml::to_string(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// Check if config exists
    pub fn exists(root: &Path) -> bool {
        root.join(CONFIG_FILE).exists()
    }

    /// Add an entry to the blacklist
    pub fn add_to_blacklist(&mut self, entry: &str) {
        if !self.blacklist.contains(&entry.to_string()) {
            self.blacklist.push(entry.to_string());
        }
    }

    /// Add an entry to the whitelist
    pub fn add_to_whitelist(&mut self, entry: &str) {
        if !self.whitelist.contains(&entry.to_string()) {
            self.whitelist.push(entry.to_string());
        }
    }

    /// Remove an entry from the blacklist
    pub fn remove_from_blacklist(&mut self, entry: &str) -> bool {
        if let Some(pos) = self.blacklist.iter().position(|e| e == entry) {
            self.blacklist.remove(pos);
            true
        } else {
            false
        }
    }

    /// Remove an entry from the whitelist
    pub fn remove_from_whitelist(&mut self, entry: &str) -> bool {
        if let Some(pos) = self.whitelist.iter().position(|e| e == entry) {
            self.whitelist.remove(pos);
            true
        } else {
            false
        }
    }

    /// Register an object in the index
    pub fn register_object(&mut self, key: String, meta: ObjectMeta) {
        self.objects.insert(key, meta);
    }

    /// Remove an object from the index
    pub fn unregister_object(&mut self, key: &str) -> Option<ObjectMeta> {
        self.objects.remove(key)
    }

    /// Get object metadata
    pub fn get_object(&self, key: &str) -> Option<&ObjectMeta> {
        self.objects.get(key)
    }

    /// Set the mode
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// Get the current mode
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Get parsed blacklist entries
    pub fn parsed_blacklist(&self) -> Result<Vec<ConfigEntry>> {
        self.blacklist
            .iter()
            .map(|e| ConfigEntry::parse(e))
            .collect()
    }

    /// Get parsed whitelist entries
    pub fn parsed_whitelist(&self) -> Result<Vec<ConfigEntry>> {
        self.whitelist
            .iter()
            .map(|e| ConfigEntry::parse(e))
            .collect()
    }

    /// Check if a file is veiled according to current mode
    pub fn is_veiled(&self, file: &str, line: usize) -> Result<bool> {
        let blacklist = self.parsed_blacklist()?;
        let whitelist = self.parsed_whitelist()?;

        match self.mode {
            Mode::Blacklist => {
                for entry in &blacklist {
                    if entry.pattern.matches(file) {
                        if let Some(ranges) = &entry.ranges {
                            return Ok(ranges.iter().any(|r| r.contains(line)));
                        }
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Mode::Whitelist => {
                // First check blacklist exceptions
                for entry in &blacklist {
                    if entry.pattern.matches(file) {
                        if let Some(ranges) = &entry.ranges {
                            if ranges.iter().any(|r| r.contains(line)) {
                                return Ok(true);
                            }
                        } else {
                            return Ok(true);
                        }
                    }
                }

                // Then check whitelist
                for entry in &whitelist {
                    if entry.pattern.matches(file) {
                        if let Some(ranges) = &entry.ranges {
                            return Ok(!ranges.iter().any(|r| r.contains(line)));
                        }
                        return Ok(false); // Full file is whitelisted = not veiled
                    }
                }

                // Default in whitelist mode: veiled
                Ok(true)
            }
        }
    }

    /// Get all veiled ranges for a file
    pub fn veiled_ranges(&self, file: &str) -> Result<Vec<LineRange>> {
        if self.objects.contains_key(file) {
            return Ok(vec![]); // Empty vec indicates full veil
        }

        Ok(self.iter_ranges_for_file(file).map(|(r, _)| r).collect())
    }

    /// Iterate over all range entries for a file, yielding `(LineRange, &ObjectMeta)`.
    pub fn iter_ranges_for_file(
        &self,
        file: &str,
    ) -> impl Iterator<Item = (LineRange, &ObjectMeta)> {
        let prefix = ConfigKey::file_prefix(file);
        let prefix_len = prefix.len();
        self.objects
            .iter()
            .filter(move |(k, _)| k.starts_with(&prefix) && !k.ends_with(ORIGINAL_SUFFIX))
            .filter_map(move |(k, meta)| {
                let range_str = &k[prefix_len..];
                LineRange::from_str(range_str).ok().map(|r| (r, meta))
            })
    }

    /// Iterate over unique file names across all config keys.
    pub fn iter_unique_files(&self) -> impl Iterator<Item = String> + '_ {
        let mut seen = std::collections::HashSet::new();
        self.objects.keys().filter_map(move |key| {
            let file = ConfigKey::parse(key).file().to_string();
            if seen.insert(file.clone()) {
                Some(file)
            } else {
                None
            }
        })
    }

    /// Get the `#_original` entry for a file.
    pub fn get_original(&self, file: &str) -> Option<&ObjectMeta> {
        self.objects.get(&ConfigKey::original_key(file))
    }

    /// Remove the `#_original` entry for a file.
    pub fn unregister_original(&mut self, file: &str) -> Option<ObjectMeta> {
        self.objects.remove(&ConfigKey::original_key(file))
    }

    /// Remove all range entries for a file, returning them.
    pub fn unregister_ranges(&mut self, file: &str) -> Vec<(String, ObjectMeta)> {
        let prefix = ConfigKey::file_prefix(file);
        let keys: Vec<String> = self
            .objects
            .keys()
            .filter(|k| k.starts_with(&prefix) && !k.ends_with(ORIGINAL_SUFFIX))
            .cloned()
            .collect();
        keys.into_iter()
            .filter_map(|k| self.objects.remove(&k).map(|meta| (k, meta)))
            .collect()
    }

    /// Check if a file has any veils (full or partial).
    pub fn has_veils(&self, file: &str) -> bool {
        self.get_object(file).is_some()
            || self.objects.keys().any(|k| {
                k.starts_with(&ConfigKey::file_prefix(file)) && !k.ends_with(ORIGINAL_SUFFIX)
            })
    }
}

const GITIGNORE_MARKER: &str = "# MANAGED BY FUNVEIL";
const GITIGNORE_END_MARKER: &str = "# END MANAGED BY FUNVEIL";

/// Ensure .gitignore contains the managed block for funveil artifacts.
/// Idempotent — does nothing if the block is already intact.
/// Repairs corrupted blocks (BUG-131) and respects CRLF line endings (BUG-132).
pub fn ensure_gitignore(root: &Path) -> Result<()> {
    let gitignore_path = root.join(".gitignore");
    let data_dir_entry = format!("{DATA_DIR}/");

    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;

        if content.contains(GITIGNORE_MARKER)
            && content.contains(GITIGNORE_END_MARKER)
            && content.contains(CONFIG_FILE)
            && content.contains(&data_dir_entry)
        {
            return Ok(());
        }

        let le = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };

        // Strip any partial/corrupted managed block before re-appending
        let cleaned: String = if content.contains(GITIGNORE_MARKER) {
            let lines: Vec<&str> = content.lines().collect();
            let mut result_lines: Vec<&str> = Vec::new();
            let mut in_block = false;
            for line in &lines {
                if *line == GITIGNORE_MARKER {
                    in_block = true;
                    continue;
                }
                if *line == GITIGNORE_END_MARKER {
                    in_block = false;
                    continue;
                }
                if in_block {
                    // Skip managed entries inside the block
                    if *line == CONFIG_FILE || *line == data_dir_entry {
                        continue;
                    }
                    // Preserve non-managed content that was inside the block
                    result_lines.push(line);
                } else {
                    result_lines.push(line);
                }
            }
            // Remove trailing empty lines from cleanup
            while result_lines.last().is_some_and(|l| l.is_empty()) {
                result_lines.pop();
            }
            if result_lines.is_empty() {
                String::new()
            } else {
                let mut s = result_lines.join(le);
                s.push_str(le);
                s
            }
        } else {
            content.clone()
        };

        let separator = if cleaned.is_empty() || cleaned.ends_with(le) {
            le.to_string()
        } else {
            format!("{le}{le}")
        };
        let block = format!(
            "{separator}{GITIGNORE_MARKER}{le}{CONFIG_FILE}{le}{data_dir_entry}{le}{GITIGNORE_END_MARKER}{le}"
        );
        std::fs::write(&gitignore_path, format!("{cleaned}{block}"))?;
    } else {
        let block = format!(
            "{GITIGNORE_MARKER}\n{CONFIG_FILE}\n{data_dir_entry}\n{GITIGNORE_END_MARKER}\n"
        );
        std::fs::write(&gitignore_path, block)?;
    }
    Ok(())
}

/// Load a .gitignore matcher from the project root.
/// Returns an empty matcher if no .gitignore exists.
pub fn load_gitignore(root: &Path) -> ignore::gitignore::Gitignore {
    let gitignore_path = root.join(".gitignore");
    let (gi, _err) = ignore::gitignore::Gitignore::new(&gitignore_path);
    gi
}

/// Check whether a path is gitignored.
pub fn is_gitignored(gitignore: &ignore::gitignore::Gitignore, path: &str, is_dir: bool) -> bool {
    gitignore
        .matched_path_or_any_parents(path, is_dir)
        .is_ignore()
}

/// Ensure the funveil data directory structure exists
pub fn ensure_data_dir(root: &Path) -> Result<()> {
    let objects = root.join(OBJECTS_DIR);
    let checkpoints = root.join(CHECKPOINTS_DIR);
    let logs = root.join(LOGS_DIR);
    let history = root.join(HISTORY_DIR);
    let metadata = root.join(METADATA_DIR);

    std::fs::create_dir_all(&objects)?;
    std::fs::create_dir_all(&checkpoints)?;
    std::fs::create_dir_all(&logs)?;
    std::fs::create_dir_all(&history)?;
    std::fs::create_dir_all(&metadata)?;

    Ok(())
}

pub fn normalize_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_string()
}

/// Check if a path is the config file
pub fn is_config_file(path: &str) -> bool {
    path == CONFIG_FILE
}

/// Check if a path is within the data directory
pub fn is_data_dir(path: &str) -> bool {
    path.starts_with(DATA_DIR) || path.starts_with(&format!("{DATA_DIR}/"))
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_supported_source() {
        assert!(is_supported_source(Path::new("src/main.rs")));
        assert!(is_supported_source(Path::new("app.py")));
        assert!(is_supported_source(Path::new("index.tsx")));
        assert!(is_supported_source(Path::new("deploy.tf")));
        assert!(is_supported_source(Path::new("config.yaml")));
        assert!(is_supported_source(Path::new("main.zig")));
        assert!(!is_supported_source(Path::new("image.png")));
        assert!(!is_supported_source(Path::new("data.json")));
        assert!(!is_supported_source(Path::new("noext")));
    }

    #[test]
    fn test_walk_files_returns_builder() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("test.txt"), "hello").unwrap();
        let entries: Vec<_> = walk_files(temp.path())
            .build()
            .filter_map(|e| e.ok())
            .collect();
        // Should contain at least the root dir entry and test.txt
        assert!(entries.len() >= 2);
    }

    #[test]
    fn test_walk_files_max_depth_chainable() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("a/b/c")).unwrap();
        std::fs::write(temp.path().join("a/b/c/deep.txt"), "deep").unwrap();
        let entries: Vec<_> = walk_files(temp.path())
            .max_depth(Some(1))
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
            .collect();
        // max_depth(1) should not reach a/b/c/deep.txt
        assert!(entries.is_empty());
    }

    #[test]
    fn test_config_save_load() {
        let temp = TempDir::new().unwrap();
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_whitelist("README.md");
        config.add_to_blacklist("secrets.env");

        config.save(temp.path()).unwrap();

        let loaded = Config::load(temp.path()).unwrap();
        assert!(loaded.mode().is_whitelist());
        assert_eq!(loaded.whitelist.len(), 1);
        assert_eq!(loaded.blacklist.len(), 1);
    }

    #[test]
    fn test_is_veiled_blacklist() {
        let mut config = Config::new(Mode::Blacklist);
        config.add_to_blacklist("secrets.env");
        config.add_to_blacklist("src/api.rs#10-20");

        assert!(config.is_veiled("secrets.env", 1).unwrap());
        assert!(!config.is_veiled("other.txt", 1).unwrap());
        assert!(config.is_veiled("src/api.rs", 15).unwrap());
        assert!(!config.is_veiled("src/api.rs", 5).unwrap());
    }

    #[test]
    fn test_is_veiled_whitelist() {
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_whitelist("README.md");
        config.add_to_whitelist("src/public.rs#1-50");

        assert!(!config.is_veiled("README.md", 1).unwrap());
        assert!(config.is_veiled("secret.rs", 1).unwrap());
        assert!(!config.is_veiled("src/public.rs", 25).unwrap());
        assert!(config.is_veiled("src/public.rs", 100).unwrap());
    }

    #[test]
    fn test_object_meta() {
        let hash = ContentHash::from_content(b"test");
        let meta = ObjectMeta::new(hash.clone(), 0o644);
        assert_eq!(meta.hash, hash.full());
        assert_eq!(meta.permissions, "644");
        assert!(meta.owner.is_none());
    }

    #[test]
    fn test_object_meta_hash() {
        let hash = ContentHash::from_content(b"test");
        let meta = ObjectMeta::new(hash.clone(), 0o644);
        let retrieved = meta.hash().unwrap();
        assert_eq!(retrieved.full(), hash.full());
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.version, 1);
        assert!(config.mode.is_whitelist());
        assert!(config.whitelist.is_empty());
        assert!(config.blacklist.is_empty());
        assert!(config.objects.is_empty());
    }

    #[test]
    fn test_config_load_nonexistent() {
        let temp = TempDir::new().unwrap();
        let config = Config::load(temp.path()).unwrap();
        assert!(config.whitelist.is_empty());
    }

    #[test]
    fn test_config_exists() {
        let temp = TempDir::new().unwrap();
        assert!(!Config::exists(temp.path()));

        let config = Config::new(Mode::Whitelist);
        config.save(temp.path()).unwrap();
        assert!(Config::exists(temp.path()));
    }

    #[test]
    fn test_remove_from_blacklist() {
        let mut config = Config::new(Mode::Blacklist);
        config.add_to_blacklist("secrets.env");
        config.add_to_blacklist("other.env");

        assert!(config.remove_from_blacklist("secrets.env"));
        assert!(!config.blacklist.contains(&"secrets.env".to_string()));
        assert!(!config.remove_from_blacklist("nonexistent.env"));
    }

    #[test]
    fn test_remove_from_whitelist() {
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_whitelist("README.md");
        config.add_to_whitelist("LICENSE");

        assert!(config.remove_from_whitelist("README.md"));
        assert!(!config.whitelist.contains(&"README.md".to_string()));
        assert!(!config.remove_from_whitelist("nonexistent.md"));
    }

    #[test]
    fn test_register_unregister_object() {
        let mut config = Config::new(Mode::Whitelist);
        let hash = ContentHash::from_content(b"test");
        let meta = ObjectMeta::new(hash, 0o644);

        config.register_object("test.txt".to_string(), meta);
        assert!(config.get_object("test.txt").is_some());

        let removed = config.unregister_object("test.txt");
        assert!(removed.is_some());
        assert!(config.get_object("test.txt").is_none());

        let not_removed = config.unregister_object("nonexistent.txt");
        assert!(not_removed.is_none());
    }

    #[test]
    fn test_set_mode() {
        let mut config = Config::new(Mode::Whitelist);
        assert!(config.mode().is_whitelist());

        config.set_mode(Mode::Blacklist);
        assert!(config.mode().is_blacklist());
    }

    #[test]
    fn test_parsed_blacklist() {
        let mut config = Config::new(Mode::Blacklist);
        config.add_to_blacklist("secrets.env");
        config.add_to_blacklist("src/api.rs#10-20");

        let entries = config.parsed_blacklist().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_parsed_whitelist() {
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_whitelist("README.md");
        config.add_to_whitelist("LICENSE");

        let entries = config.parsed_whitelist().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_is_veiled_blacklist_with_ranges() {
        let mut config = Config::new(Mode::Blacklist);
        config.add_to_blacklist("src/api.rs#10-20");

        assert!(config.is_veiled("src/api.rs", 15).unwrap());
        assert!(!config.is_veiled("src/api.rs", 5).unwrap());
        assert!(!config.is_veiled("src/api.rs", 25).unwrap());
    }

    #[test]
    fn test_is_veiled_whitelist_with_blacklist_override() {
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_whitelist("src/");
        config.add_to_blacklist("src/secrets.rs");

        assert!(!config.is_veiled("src/main.rs", 1).unwrap());
        assert!(config.is_veiled("src/secrets.rs", 1).unwrap());
    }

    #[test]
    fn test_is_veiled_whitelist_with_blacklist_ranges() {
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_whitelist("src/public.rs");
        config.add_to_blacklist("src/public.rs#10-20");

        assert!(!config.is_veiled("src/public.rs", 5).unwrap());
        assert!(config.is_veiled("src/public.rs", 15).unwrap());
    }

    #[test]
    fn test_veiled_ranges_full_file() {
        let mut config = Config::new(Mode::Whitelist);
        let hash = ContentHash::from_content(b"test");
        config.register_object("test.txt".to_string(), ObjectMeta::new(hash, 0o644));

        let ranges = config.veiled_ranges("test.txt").unwrap();
        assert!(ranges.is_empty());
    }

    #[test]
    fn test_veiled_ranges_partial() {
        let mut config = Config::new(Mode::Whitelist);
        let hash = ContentHash::from_content(b"test");
        config.register_object(
            "test.txt#1-10".to_string(),
            ObjectMeta::new(hash.clone(), 0o644),
        );
        config.register_object("test.txt#20-30".to_string(), ObjectMeta::new(hash, 0o644));

        let ranges = config.veiled_ranges("test.txt").unwrap();
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn test_veiled_ranges_none() {
        let config = Config::new(Mode::Whitelist);
        let ranges = config.veiled_ranges("nonexistent.txt").unwrap();
        assert!(ranges.is_empty());
    }

    #[test]
    fn test_is_config_file() {
        assert!(is_config_file(".funveil_config"));
        assert!(!is_config_file("other.txt"));
    }

    #[test]
    fn test_is_data_dir() {
        assert!(is_data_dir(".funveil/objects"));
        assert!(is_data_dir(".funveil/checkpoints"));
        assert!(!is_data_dir("src/main.rs"));
    }

    #[test]
    fn test_ensure_data_dir() {
        let temp = TempDir::new().unwrap();
        ensure_data_dir(temp.path()).unwrap();

        assert!(temp.path().join(OBJECTS_DIR).exists());
        assert!(temp.path().join(CHECKPOINTS_DIR).exists());
    }

    #[test]
    fn test_config_complex_round_trip() {
        let temp = TempDir::new().unwrap();
        let mut config = Config::new(Mode::Blacklist);

        // Add whitelist entries
        config.add_to_whitelist("README.md");
        config.add_to_whitelist("src/public.rs#1-50");

        // Add blacklist entries
        config.add_to_blacklist("secrets.env");
        config.add_to_blacklist("src/api.rs#10-20,30-40");

        // Add object entries with different permissions
        let hash1 = ContentHash::from_content(b"content1");
        config.register_object(
            "file1.txt".to_string(),
            ObjectMeta::new(hash1.clone(), 0o644),
        );
        let hash2 = ContentHash::from_content(b"content2");
        config.register_object(
            "file2.sh".to_string(),
            ObjectMeta::new(hash2.clone(), 0o755),
        );

        config.save(temp.path()).unwrap();
        let loaded = Config::load(temp.path()).unwrap();

        assert!(loaded.mode().is_blacklist());
        assert_eq!(loaded.whitelist.len(), 2);
        assert!(loaded.whitelist.contains(&"README.md".to_string()));
        assert!(loaded.whitelist.contains(&"src/public.rs#1-50".to_string()));
        assert_eq!(loaded.blacklist.len(), 2);
        assert!(loaded.blacklist.contains(&"secrets.env".to_string()));
        assert!(loaded
            .blacklist
            .contains(&"src/api.rs#10-20,30-40".to_string()));
        assert_eq!(loaded.objects.len(), 2);
        assert_eq!(loaded.get_object("file1.txt").unwrap().hash, hash1.full());
        assert_eq!(loaded.get_object("file1.txt").unwrap().permissions, "644");
        assert_eq!(loaded.get_object("file2.sh").unwrap().hash, hash2.full());
        assert_eq!(loaded.get_object("file2.sh").unwrap().permissions, "755");
    }

    // ── BUG-100: veiled_ranges with '#' in filename ──

    #[test]
    fn test_bug100_veiled_ranges_hash_in_filename() {
        let mut config = Config::new(Mode::Whitelist);
        let hash = ContentHash::from_content(b"test");
        // Register with '#' in filename: "dir/file#name.txt#1-10"
        config.register_object(
            "dir/file#name.txt#1-10".to_string(),
            ObjectMeta::new(hash, 0o644),
        );

        let ranges = config.veiled_ranges("dir/file#name.txt").unwrap();
        assert_eq!(ranges.len(), 1, "should find range for file with # in name");
        assert_eq!(ranges[0].start(), 1);
        assert_eq!(ranges[0].end(), 10);
    }

    #[test]
    fn test_add_duplicate_to_blacklist() {
        let mut config = Config::new(Mode::Blacklist);
        config.add_to_blacklist("secrets.env");
        config.add_to_blacklist("secrets.env");
        assert_eq!(config.blacklist.len(), 1);
    }

    #[test]
    fn test_add_duplicate_to_whitelist() {
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_whitelist("README.md");
        config.add_to_whitelist("README.md");
        assert_eq!(config.whitelist.len(), 1);
    }

    // --- Tests targeting specific missed mutants ---

    #[test]
    fn test_is_veiled_blacklist_no_ranges_returns_true() {
        // Catches: return Ok(true) → return Ok(false) on line 184
        let mut config = Config::new(Mode::Blacklist);
        config.add_to_blacklist("secret.txt");
        assert!(config.is_veiled("secret.txt", 1).unwrap());
        assert!(config.is_veiled("secret.txt", 999).unwrap());
    }

    #[test]
    fn test_is_veiled_blacklist_range_boundary() {
        // Catches: any() → all() mutations and range boundary changes
        let mut config = Config::new(Mode::Blacklist);
        config.add_to_blacklist("file.rs#10-20");
        assert!(!config.is_veiled("file.rs", 9).unwrap());
        assert!(config.is_veiled("file.rs", 10).unwrap());
        assert!(config.is_veiled("file.rs", 20).unwrap());
        assert!(!config.is_veiled("file.rs", 21).unwrap());
    }

    #[test]
    fn test_is_veiled_blacklist_no_match_returns_false() {
        // Catches: Ok(false) → Ok(true) on line 187
        let mut config = Config::new(Mode::Blacklist);
        config.add_to_blacklist("secret.txt");
        assert!(!config.is_veiled("other.txt", 1).unwrap());
    }

    #[test]
    fn test_is_veiled_whitelist_blacklist_range_hit() {
        // In whitelist mode, blacklist with ranges: line inside range → veiled
        // Catches: ranges.iter().any() → all() and return Ok(true) on line 195
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_blacklist("file.rs#5-10");
        assert!(config.is_veiled("file.rs", 7).unwrap());
    }

    #[test]
    fn test_is_veiled_whitelist_blacklist_range_miss() {
        // In whitelist mode, blacklist with ranges: line outside range → falls through to default veiled
        // Catches: the short-circuit behavior after range check
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_blacklist("file.rs#5-10");
        // Line 3 is NOT in blacklist range, so falls through to whitelist check.
        // No whitelist entry for file.rs → default veiled (true)
        assert!(config.is_veiled("file.rs", 3).unwrap());
    }

    #[test]
    fn test_is_veiled_whitelist_no_range_returns_false() {
        // Whitelist entry without ranges → full file not veiled
        // Catches: return Ok(false) → return Ok(true) on line 209
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_whitelist("public.txt");
        assert!(!config.is_veiled("public.txt", 1).unwrap());
    }

    #[test]
    fn test_is_veiled_whitelist_with_range_inverts() {
        // Whitelist with ranges: lines IN range → NOT veiled, lines OUT → veiled
        // Catches: !ranges.iter().any() negation deletion on line 207
        let mut config = Config::new(Mode::Whitelist);
        config.add_to_whitelist("file.rs#10-20");
        assert!(!config.is_veiled("file.rs", 15).unwrap()); // In range → not veiled
        assert!(config.is_veiled("file.rs", 5).unwrap()); // Out of range → veiled
    }

    #[test]
    fn test_is_veiled_whitelist_default_veiled() {
        // No whitelist or blacklist match → default veiled in whitelist mode
        // Catches: Ok(true) → Ok(false) on line 214
        let config = Config::new(Mode::Whitelist);
        assert!(config.is_veiled("unknown.txt", 1).unwrap());
    }

    #[test]
    fn test_veiled_ranges_skips_original_suffix() {
        // suffix == "_original" should be skipped (not parsed as range)
        // Catches: suffix == "_original" → != mutation on line 236
        let mut config = Config::new(Mode::Whitelist);
        let hash = ContentHash::from_content(b"test");
        config.register_object(
            "file.txt#_original".to_string(),
            ObjectMeta::new(hash, 0o644),
        );
        let ranges = config.veiled_ranges("file.txt").unwrap();
        assert!(ranges.is_empty());
    }

    #[test]
    fn test_veiled_ranges_obj_file_mismatch() {
        // Catches: obj_file == file → != mutation on line 238
        let mut config = Config::new(Mode::Whitelist);
        let hash = ContentHash::from_content(b"test");
        config.register_object("other.txt#1-10".to_string(), ObjectMeta::new(hash, 0o644));
        let ranges = config.veiled_ranges("file.txt").unwrap();
        assert!(ranges.is_empty());
    }

    #[test]
    fn test_is_data_dir_exact_match() {
        // Catches: || operator mutation in is_data_dir
        // ".funveil" should match (starts_with DATA_DIR)
        assert!(is_data_dir(".funveil"));
        // ".funveil/" should match the second condition
        assert!(is_data_dir(".funveil/"));
        // ".funveil/objects/abc" should match
        assert!(is_data_dir(".funveil/objects/abc"));
        // "src/main.rs" should NOT match
        assert!(!is_data_dir("src/main.rs"));
    }

    #[test]
    fn test_is_config_file_exact_only() {
        // Catches: == → != on line 366
        assert!(is_config_file(".funveil_config"));
        assert!(!is_config_file(".funveil_config_backup"));
        assert!(!is_config_file(""));
    }

    #[test]
    fn test_ensure_gitignore_creates_new() {
        let temp = TempDir::new().unwrap();
        ensure_gitignore(temp.path()).unwrap();
        let content = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert!(content.contains("# MANAGED BY FUNVEIL"));
        assert!(content.contains("# END MANAGED BY FUNVEIL"));
        assert!(content.contains(CONFIG_FILE));
        assert!(content.contains(&format!("{DATA_DIR}/")));
    }

    #[test]
    fn test_ensure_gitignore_idempotent() {
        // Catches: && → || in the integrity check on lines 265-268
        let temp = TempDir::new().unwrap();
        ensure_gitignore(temp.path()).unwrap();
        let content1 = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        ensure_gitignore(temp.path()).unwrap();
        let content2 = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert_eq!(content1, content2);
    }

    #[test]
    fn test_ensure_gitignore_repairs_partial_block() {
        // Catches: block repair logic with in_block tracking
        let temp = TempDir::new().unwrap();
        // Write a corrupted block (missing end marker)
        std::fs::write(
            temp.path().join(".gitignore"),
            "# MANAGED BY FUNVEIL\n.funveil_config\n",
        )
        .unwrap();
        ensure_gitignore(temp.path()).unwrap();
        let content = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert!(content.contains("# END MANAGED BY FUNVEIL"));
    }

    #[test]
    fn test_ensure_gitignore_crlf() {
        // Catches: line ending detection on line 274
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(".gitignore"), "node_modules\r\n*.log\r\n").unwrap();
        ensure_gitignore(temp.path()).unwrap();
        let content = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert!(content.contains("\r\n"));
        assert!(content.contains("# MANAGED BY FUNVEIL"));
    }

    #[test]
    fn test_ensure_gitignore_appends_to_existing() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(".gitignore"), "node_modules\n").unwrap();
        ensure_gitignore(temp.path()).unwrap();
        let content = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert!(content.contains("node_modules"));
        assert!(content.contains("# MANAGED BY FUNVEIL"));
    }

    #[test]
    fn test_ensure_gitignore_repairs_missing_end_marker() {
        // Catches: && → || on lines 267-268 (integrity check)
        // Only start marker present but not end marker → should repair
        let temp = TempDir::new().unwrap();
        std::fs::write(
            temp.path().join(".gitignore"),
            format!("# MANAGED BY FUNVEIL\n{CONFIG_FILE}\n"),
        )
        .unwrap();
        ensure_gitignore(temp.path()).unwrap();
        let content = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert!(content.contains("# END MANAGED BY FUNVEIL"));
        assert!(content.contains(&format!("{DATA_DIR}/")));
    }

    #[test]
    fn test_ensure_gitignore_repairs_missing_config_entry() {
        // Has markers but missing config file entry → should repair
        let temp = TempDir::new().unwrap();
        std::fs::write(
            temp.path().join(".gitignore"),
            format!("# MANAGED BY FUNVEIL\n{DATA_DIR}/\n# END MANAGED BY FUNVEIL\n"),
        )
        .unwrap();
        ensure_gitignore(temp.path()).unwrap();
        let content = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();
        assert!(content.contains(CONFIG_FILE));
    }

    #[test]
    fn test_is_gitignored_with_pattern() {
        // Catches: replace is_gitignored → true/false (line 348)
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(".gitignore"), "*.log\n").unwrap();
        let gi = load_gitignore(temp.path());
        assert!(is_gitignored(&gi, "test.log", false));
        assert!(!is_gitignored(&gi, "test.txt", false));
    }

    // ── Mutant-targeted: veiled_ranges == vs != for _original (line 236) ──

    #[test]
    fn test_veiled_ranges_original_not_counted_and_range_is() {
        // If == is mutated to != on `suffix == "_original"`, then _original
        // entries would be parsed as ranges (and fail) while real ranges
        // would be skipped.
        let mut config = Config::new(Mode::Whitelist);
        let hash = ContentHash::from_content(b"test");
        config.register_object(
            "file.txt#_original".to_string(),
            ObjectMeta::new(hash.clone(), 0o644),
        );
        config.register_object("file.txt#5-15".to_string(), ObjectMeta::new(hash, 0o644));
        let ranges = config.veiled_ranges("file.txt").unwrap();
        // _original should be excluded, 5-15 should be included
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start(), 5);
        assert_eq!(ranges[0].end(), 15);
    }

    // ── Mutant-targeted: ensure_gitignore block-cleaning || vs && (line 296) ──

    #[test]
    fn test_ensure_gitignore_repairs_block_with_only_config_entry() {
        // Corrupted block has start marker + CONFIG_FILE but missing data_dir_entry
        // and end marker. If || on line 296 is mutated to &&, the CONFIG_FILE
        // line would NOT be stripped during cleanup (it only matches one of the
        // two conditions), leading to a duplicate entry.
        let temp = TempDir::new().unwrap();
        let content = format!("# some existing entry\n# MANAGED BY FUNVEIL\n{CONFIG_FILE}\n");
        std::fs::write(temp.path().join(".gitignore"), &content).unwrap();

        ensure_gitignore(temp.path()).unwrap();
        let result = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();

        // CONFIG_FILE should appear exactly once (inside the managed block)
        let count = result.matches(CONFIG_FILE).count();
        assert_eq!(
            count, 1,
            "CONFIG_FILE should appear exactly once, got {count} in:\n{result}"
        );
        // The block should be complete
        assert!(result.contains("# END MANAGED BY FUNVEIL"));
        assert!(result.contains(&format!("{DATA_DIR}/")));
    }

    #[test]
    fn test_ensure_gitignore_repairs_block_with_only_data_dir() {
        // Corrupted block has start marker + data_dir but no CONFIG_FILE or end marker.
        // Tests the other branch of the || on line 296.
        let temp = TempDir::new().unwrap();
        let data_dir_entry = format!("{DATA_DIR}/");
        let content = format!("# MANAGED BY FUNVEIL\n{data_dir_entry}\n");
        std::fs::write(temp.path().join(".gitignore"), &content).unwrap();

        ensure_gitignore(temp.path()).unwrap();
        let result = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();

        let count = result.matches(&data_dir_entry).count();
        assert_eq!(
            count, 1,
            "data_dir_entry should appear exactly once, got {count} in:\n{result}"
        );
    }

    // ── Mutant-targeted: separator logic || vs && (line 320) ──

    #[test]
    fn test_ensure_gitignore_separator_when_cleaned_empty() {
        // If || on line 320 is mutated to &&, an empty cleaned string would
        // get a double-separator instead of a single one.
        let temp = TempDir::new().unwrap();
        // Write a .gitignore that is ONLY a corrupted managed block (no other content).
        // After cleanup, `cleaned` will be empty.
        let content = "# MANAGED BY FUNVEIL\n";
        std::fs::write(temp.path().join(".gitignore"), content).unwrap();

        ensure_gitignore(temp.path()).unwrap();
        let result = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();

        // The file should not start with multiple newlines
        assert!(
            !result.starts_with("\n\n"),
            "empty cleaned content should not produce double newline prefix, got:\n{result}"
        );
        assert!(result.contains("# MANAGED BY FUNVEIL"));
        assert!(result.contains("# END MANAGED BY FUNVEIL"));
    }

    #[test]
    fn test_ensure_gitignore_preserves_non_managed_lines_inside_block() {
        // Line 365: non-managed content inside the managed block should be preserved
        let temp = TempDir::new().unwrap();
        let content = format!(
            "existing\n# MANAGED BY FUNVEIL\n{CONFIG_FILE}\n{DATA_DIR}/\nuser_added_line\n# END MANAGED BY FUNVEIL\n"
        );
        std::fs::write(temp.path().join(".gitignore"), &content).unwrap();

        // The block is complete so it should be idempotent — but let's
        // corrupt it slightly by removing the data dir entry so it triggers repair
        let content_missing_data = format!(
            "existing\n# MANAGED BY FUNVEIL\n{CONFIG_FILE}\nuser_added_line\n# END MANAGED BY FUNVEIL\n"
        );
        std::fs::write(temp.path().join(".gitignore"), &content_missing_data).unwrap();

        ensure_gitignore(temp.path()).unwrap();
        let result = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();

        // The user_added_line should be preserved (not stripped during cleanup)
        assert!(
            result.contains("user_added_line"),
            "non-managed lines inside block should be preserved, got:\n{result}"
        );
        // The block should be repaired with all entries
        assert!(result.contains(CONFIG_FILE));
        assert!(result.contains(&format!("{DATA_DIR}/")));
        assert!(result.contains("# END MANAGED BY FUNVEIL"));
    }

    #[test]
    fn test_ensure_gitignore_pops_trailing_empty_lines() {
        // Line 372: trailing empty lines after cleanup should be removed
        let temp = TempDir::new().unwrap();
        // Corrupted block followed by empty lines
        let content = format!("# MANAGED BY FUNVEIL\n{CONFIG_FILE}\n\n\n\n");
        std::fs::write(temp.path().join(".gitignore"), &content).unwrap();

        ensure_gitignore(temp.path()).unwrap();
        let result = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();

        // Should not have excessive leading blank lines before the managed block
        assert!(
            !result.starts_with("\n\n\n"),
            "trailing empty lines from cleanup should be popped, got:\n{result}"
        );
        assert!(result.contains("# MANAGED BY FUNVEIL"));
        assert!(result.contains("# END MANAGED BY FUNVEIL"));
    }

    #[test]
    fn test_ensure_gitignore_double_newline_separator() {
        // Line 388: when cleaned content doesn't end with a newline,
        // a double-newline separator should be used
        let temp = TempDir::new().unwrap();
        // Write content that does NOT end with newline and has no managed block
        std::fs::write(temp.path().join(".gitignore"), "node_modules").unwrap();

        ensure_gitignore(temp.path()).unwrap();
        let result = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();

        // There should be a double-newline between the existing content and the block
        assert!(
            result.contains("node_modules\n\n# MANAGED BY FUNVEIL"),
            "should have double-newline separator when content doesn't end with newline, got:\n{result}"
        );
    }

    #[test]
    fn test_ensure_gitignore_separator_when_cleaned_ends_with_newline() {
        // cleaned is non-empty and ends with \n → separator should be single \n.
        // If || becomes &&, the `cleaned.ends_with(le)` check alone would fail
        // (since cleaned is non-empty), giving double-separator.
        let temp = TempDir::new().unwrap();
        // Existing content with trailing newline + a corrupted managed block
        let content = "node_modules\n*.log\n# MANAGED BY FUNVEIL\n";
        std::fs::write(temp.path().join(".gitignore"), content).unwrap();

        ensure_gitignore(temp.path()).unwrap();
        let result = std::fs::read_to_string(temp.path().join(".gitignore")).unwrap();

        // Should not have triple newlines (double separator + existing trailing)
        assert!(
            !result.contains("\n\n\n"),
            "should not have triple newlines, got:\n{result}"
        );
    }
}
