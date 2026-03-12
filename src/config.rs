use crate::error::Result;
use crate::types::{ConfigEntry, ContentHash, LineRange, Mode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

pub const CONFIG_FILE: &str = ".funveil_config";
pub const DATA_DIR: &str = ".funveil";
pub const OBJECTS_DIR: &str = ".funveil/objects";
pub const CHECKPOINTS_DIR: &str = ".funveil/checkpoints";

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
            permissions: format!("{permissions:o}"),
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            mode: Mode::default(), // Whitelist
            whitelist: Vec::new(),
            blacklist: Vec::new(),
            objects: HashMap::new(),
        }
    }
}

impl Config {
    /// Create a new config with the given mode
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
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
                // Check if explicitly blacklisted
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
        let mut ranges = Vec::new();

        // Check if there's a full-file veil
        let key = file.to_string();
        if self.objects.contains_key(&key) {
            // Full file is veiled
            return Ok(vec![]); // Empty vec indicates full veil
        }

        // Check for partial veils
        // BUG-100: Use rfind('#') with suffix validation for filenames containing '#'
        for key in self.objects.keys() {
            if let Some(pos) = key.rfind('#') {
                let suffix = &key[pos + 1..];
                // Validate suffix looks like a range spec (e.g., "1-5") or _original
                if suffix == "_original" || LineRange::from_str(suffix).is_ok() {
                    let obj_file = &key[..pos];
                    if obj_file == file {
                        if let Ok(range) = LineRange::from_str(suffix) {
                            ranges.push(range);
                        }
                    }
                }
            }
        }

        Ok(ranges)
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

        // BUG-131: Check full block integrity, not just start marker
        if content.contains(GITIGNORE_MARKER)
            && content.contains(GITIGNORE_END_MARKER)
            && content.contains(CONFIG_FILE)
            && content.contains(&data_dir_entry)
        {
            return Ok(());
        }

        // BUG-132: Detect line ending style
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

    std::fs::create_dir_all(&objects)?;
    std::fs::create_dir_all(&checkpoints)?;

    Ok(())
}

/// Check if a path is the config file
pub fn is_config_file(path: &str) -> bool {
    path == CONFIG_FILE
}

/// Check if a path is within the data directory
pub fn is_data_dir(path: &str) -> bool {
    path.starts_with(DATA_DIR) || path.starts_with(&format!("{DATA_DIR}/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
}
