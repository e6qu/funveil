use std::fs;
use tempfile::TempDir;

// Import the library
use funveil::{Config, ConfigEntry, ContentStore, LineRange, Mode, Pattern};

/// Helper to create a test project structure
fn setup_test_project() -> TempDir {
    let temp = TempDir::new().unwrap();

    // Create some test files
    fs::write(
        temp.path().join("README.md"),
        "# Test Project\n\nThis is a test.\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("main.rs"),
        "fn main() {\n    println!(\"Hello\");\n}\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    )
    .unwrap();

    // Create subdirectory
    fs::create_dir(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("src").join("utils.rs"),
        "pub fn helper() {}\n",
    )
    .unwrap();

    temp
}

#[test]
fn test_config_save_load() {
    let temp = setup_test_project();
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
fn test_content_store() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());

    let content = b"hello world";
    let hash = store.store(content).unwrap();

    assert!(store.exists(&hash));

    let retrieved = store.retrieve(&hash).unwrap();
    assert_eq!(retrieved, content);
}

#[test]
fn test_deduplication() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());

    let content = b"duplicate content";
    let hash1 = store.store(content).unwrap();
    let hash2 = store.store(content).unwrap();

    assert_eq!(hash1.full(), hash2.full());

    // Should only have one file
    let all = store.list_all().unwrap();
    assert_eq!(all.len(), 1);
}

#[test]
fn test_line_range_validation() {
    // Valid ranges
    assert!(LineRange::new(1, 10).is_ok());
    assert!(LineRange::new(5, 5).is_ok()); // Single line

    // Invalid ranges
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

    // Should fail on relative paths
    assert!(ConfigEntry::parse("./relative.rs").is_err());
    assert!(ConfigEntry::parse("../parent.rs").is_err());
}

#[test]
fn test_vcs_detection() {
    use funveil::types::is_vcs_directory;

    assert!(is_vcs_directory(".git/config"));
    assert!(is_vcs_directory("src/.git/objects"));
    assert!(is_vcs_directory(".svn/entries"));
    assert!(!is_vcs_directory("src/main.rs"));
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
fn test_has_veils() {
    use funveil::config::ObjectMeta;
    use funveil::{has_veils, ContentHash};

    let mut config = Config::new(Mode::Blacklist);

    assert!(!has_veils(&config, "secrets.env"));

    let hash = ContentHash::from_content(b"test");
    config.register_object(
        "secrets.env".to_string(),
        ObjectMeta::new(hash.clone(), 0o644),
    );
    assert!(has_veils(&config, "secrets.env"));

    config.register_object("api.py#10-20".to_string(), ObjectMeta::new(hash, 0o644));
    assert!(has_veils(&config, "api.py"));
    assert!(!has_veils(&config, "other.txt"));
}

#[test]
fn test_content_hash() {
    use funveil::ContentHash;

    let hash = ContentHash::from_content(b"test content");
    assert_eq!(hash.short().len(), 7);
    assert_eq!(hash.full().len(), 64); // SHA-256 hex = 64 chars

    let (a, b, c) = hash.path_components();
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 2);
    assert!(!c.is_empty());
}

#[test]
fn test_binary_detection() {
    use funveil::types::is_binary_file;

    let temp = TempDir::new().unwrap();

    // Text file
    let text_file = temp.path().join("test.txt");
    fs::write(&text_file, "Hello, World!\n").unwrap();
    assert!(!is_binary_file(&text_file));

    // File with null bytes (binary)
    let bin_file = temp.path().join("test.bin");
    fs::write(&bin_file, b"Hello\x00World").unwrap();
    assert!(is_binary_file(&bin_file));

    // File with binary extension
    let png_file = temp.path().join("test.png");
    fs::write(&png_file, "not really a png").unwrap();
    assert!(is_binary_file(&png_file));
}

#[test]
fn test_data_dir_creation() {
    let temp = TempDir::new().unwrap();

    funveil::config::ensure_data_dir(temp.path()).unwrap();

    assert!(temp.path().join(".funveil").exists());
    assert!(temp.path().join(".funveil/objects").exists());
    assert!(temp.path().join(".funveil/checkpoints").exists());
}

#[test]
fn test_full_workflow() {
    let temp = setup_test_project();

    // Initialize config
    let mut config = Config::new(Mode::Whitelist);
    config.add_to_whitelist("README.md");
    config.save(temp.path()).unwrap();

    // Ensure data dir exists
    funveil::config::ensure_data_dir(temp.path()).unwrap();

    // Verify structure
    assert!(temp.path().join(".funveil_config").exists());
    assert!(temp.path().join(".funveil").exists());

    // Reload and verify
    let loaded = Config::load(temp.path()).unwrap();
    assert_eq!(loaded.whitelist.len(), 1);
}

#[test]
fn test_veil_config_file_fails() {
    let temp = TempDir::new().unwrap();
    let mut config = Config::new(Mode::Blacklist);

    let result = funveil::veil_file(temp.path(), &mut config, ".funveil_config", None);
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("protected"));
}

#[test]
fn test_veil_data_directory_fails() {
    let temp = TempDir::new().unwrap();
    let mut config = Config::new(Mode::Blacklist);

    let result = funveil::veil_file(temp.path(), &mut config, ".funveil/", None);
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("protected"));
}

#[test]
fn test_veil_vcs_directory_fails() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    let mut config = Config::new(Mode::Blacklist);

    let result = funveil::veil_file(temp.path(), &mut config, ".git/config", None);
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("VCS") || err.contains("git"));
}

#[test]
fn test_veil_binary_file_partial_fails() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("image.png"), b"\x89PNG\r\n\x1a\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);

    let result = funveil::veil_file(
        temp.path(),
        &mut config,
        "image.png",
        Some(&[LineRange::new(1, 5).unwrap()]),
    );
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("binary"));
}

#[test]
fn test_veil_binary_file_full_fails() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("image.png"), b"\x89PNG\r\n\x1a\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);

    let result = funveil::veil_file(temp.path(), &mut config, "image.png", None);
    assert!(result.is_err());
}

#[test]
fn test_veil_nonexistent_file_fails() {
    let temp = TempDir::new().unwrap();
    let mut config = Config::new(Mode::Blacklist);

    let result = funveil::veil_file(temp.path(), &mut config, "nonexistent.txt", None);
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("not found"));
}

#[test]
fn test_unveil_non_veiled_file_fails() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("visible.txt"), "content").unwrap();

    let mut config = Config::new(Mode::Blacklist);

    let result = funveil::unveil_file(temp.path(), &mut config, "visible.txt", None);
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("not veiled"));
}

#[test]
fn test_veil_already_veiled_file_fails() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("file.txt"), "content").unwrap();

    let mut config = Config::new(Mode::Blacklist);

    funveil::veil_file(temp.path(), &mut config, "file.txt", None).unwrap();

    let result = funveil::veil_file(temp.path(), &mut config, "file.txt", None);
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("already veiled"));
}

#[test]
fn test_veil_empty_file_partial_fails() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("empty.txt"), "").unwrap();

    let mut config = Config::new(Mode::Blacklist);

    let result = funveil::veil_file(
        temp.path(),
        &mut config,
        "empty.txt",
        Some(&[LineRange::new(1, 5).unwrap()]),
    );
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("empty"));
}

#[test]
fn test_unicode_content_preserved() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("file.txt"), "Hello 世界 🌍\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "file.txt", None).unwrap();
    funveil::unveil_file(temp.path(), &mut config, "file.txt", None).unwrap();

    let content = fs::read_to_string(temp.path().join("file.txt")).unwrap();
    assert_eq!(content, "Hello 世界 🌍\n");
}

#[test]
fn test_config_malformed_yaml_fails() {
    let temp = TempDir::new().unwrap();

    fs::write(temp.path().join(".funveil_config"), "invalid: [yaml").unwrap();

    let result = Config::load(temp.path());
    assert!(result.is_err());
}

#[test]
fn test_config_invalid_mode_fails() {
    let temp = TempDir::new().unwrap();

    let yaml = r#"
version: 1
mode: invalid_mode
"#;
    fs::write(temp.path().join(".funveil_config"), yaml).unwrap();

    let result = Config::load(temp.path());
    assert!(result.is_err());
}

#[test]
fn test_config_entry_invalid_range() {
    let result = ConfigEntry::parse("file.txt#20-10");
    assert!(result.is_err());

    let result = ConfigEntry::parse("file.txt#0-5");
    assert!(result.is_err());
}
