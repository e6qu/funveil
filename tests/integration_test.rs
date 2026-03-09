use std::fs;
use tempfile::TempDir;

// Import the library
use funveil::{Config, ConfigEntry, ContentHash, ContentStore, LineRange, Mode, Pattern};

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
fn test_symlink_escape_detection() {
    use funveil::types::validate_path_within_root;
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();

    fs::write(outside.path().join("secret.txt"), "secret data").unwrap();

    let link_path = temp.path().join("link");
    symlink(outside.path().join("secret.txt"), &link_path).unwrap();

    let result = validate_path_within_root(&link_path, temp.path());
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.to_string().contains("outside"));
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

#[test]
#[cfg(unix)]
fn test_veil_unveil_preserves_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let path = temp.path().join("script.sh");
    fs::write(&path, "#!/bin/bash\necho hello").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "script.sh", None).unwrap();
    funveil::unveil_file(temp.path(), &mut config, "script.sh", None).unwrap();

    let metadata = fs::metadata(&path).unwrap();
    let mode = metadata.permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
}

#[test]
fn test_round_trip_preserves_content_integrity() {
    let temp = TempDir::new().unwrap();

    let original = "line1\nline2\nline3\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "test.txt", None).unwrap();
    funveil::unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, original);
}

#[test]
fn test_partial_veil_round_trip_integrity() {
    let temp = TempDir::new().unwrap();

    let original = "1\n2\n3\n4\n5\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges = vec![LineRange::new(2, 4).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();
    funveil::unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, original);
}

#[test]
fn test_cas_store_empty_content() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());

    let hash = store.store(b"").unwrap();
    assert!(!hash.full().is_empty());

    let retrieved = store.retrieve(&hash).unwrap();
    assert!(retrieved.is_empty());
}

#[test]
fn test_cas_store_large_content() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());

    let large_content = vec![0u8; 1024 * 1024];
    let hash = store.store(&large_content).unwrap();

    let retrieved = store.retrieve(&hash).unwrap();
    assert_eq!(retrieved.len(), large_content.len());
}

#[test]
fn test_cas_nonexistent_hash() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());

    let fake_hash = ContentHash::from_string(
        "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
    );
    let result = store.retrieve(&fake_hash);
    assert!(result.is_err());
}

#[test]
fn test_config_whitelist_mode_default() {
    let temp = TempDir::new().unwrap();

    let config = Config::new(Mode::Whitelist);
    config.save(temp.path()).unwrap();

    let loaded = Config::load(temp.path()).unwrap();
    assert_eq!(loaded.mode, Mode::Whitelist);
    assert!(loaded.whitelist.is_empty());
}

#[test]
fn test_config_blacklist_mode_default() {
    let temp = TempDir::new().unwrap();

    let config = Config::new(Mode::Blacklist);
    config.save(temp.path()).unwrap();

    let loaded = Config::load(temp.path()).unwrap();
    assert_eq!(loaded.mode, Mode::Blacklist);
    assert!(loaded.blacklist.is_empty());
}

#[test]
fn test_config_add_multiple_whitelist_entries() {
    let mut config = Config::new(Mode::Whitelist);

    config.add_to_whitelist("file1.txt");
    config.add_to_whitelist("file2.txt");
    config.add_to_whitelist("file3.txt");

    assert_eq!(config.whitelist.len(), 3);
}

#[test]
fn test_config_add_multiple_blacklist_entries() {
    let mut config = Config::new(Mode::Blacklist);

    config.add_to_blacklist("secret1.env");
    config.add_to_blacklist("secret2.env");
    config.add_to_blacklist("secret3.env");

    assert_eq!(config.blacklist.len(), 3);
}

#[test]
fn test_veil_file_read_only_after_veil() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "content").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let metadata = fs::metadata(temp.path().join("test.txt")).unwrap();
    assert!(metadata.permissions().readonly());
}

#[test]
fn test_unveil_file_writable_after_unveil() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "content").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "test.txt", None).unwrap();
    funveil::unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let metadata = fs::metadata(temp.path().join("test.txt")).unwrap();
    assert!(!metadata.permissions().readonly());
}

#[test]
fn test_partial_veil_single_line() {
    let temp = TempDir::new().unwrap();

    let original = "line1\nline2\nline3\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges = vec![LineRange::new(2, 2).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("line1"));
    assert!(veiled.contains("line3"));
    assert!(veiled.contains("..."));
}

#[test]
fn test_partial_veil_first_line() {
    let temp = TempDir::new().unwrap();

    let original = "line1\nline2\nline3\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges = vec![LineRange::new(1, 1).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("line2"));
    assert!(veiled.contains("line3"));
}

#[test]
fn test_partial_veil_last_line() {
    let temp = TempDir::new().unwrap();

    let original = "line1\nline2\nline3\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges = vec![LineRange::new(3, 3).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("line1"));
    assert!(veiled.contains("line2"));
}

#[test]
fn test_veil_directory_recursive() {
    let temp = TempDir::new().unwrap();

    fs::create_dir_all(temp.path().join("subdir")).unwrap();
    fs::write(temp.path().join("subdir/file1.txt"), "content1").unwrap();
    fs::write(temp.path().join("subdir/file2.txt"), "content2").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let result = funveil::veil_file(temp.path(), &mut config, "subdir/", None);
    assert!(result.is_ok());
}

#[test]
fn test_content_hash_from_content() {
    let hash1 = ContentHash::from_content(b"hello");
    let hash2 = ContentHash::from_content(b"hello");
    let hash3 = ContentHash::from_content(b"world");

    assert_eq!(hash1.full(), hash2.full());
    assert_ne!(hash1.full(), hash3.full());
}

#[test]
fn test_content_hash_short() {
    let hash = ContentHash::from_content(b"test");
    assert_eq!(hash.short().len(), 7);
}

#[test]
fn test_line_range_contains() {
    let range = LineRange::new(5, 10).unwrap();

    assert!(!range.contains(4));
    assert!(range.contains(5));
    assert!(range.contains(7));
    assert!(range.contains(10));
    assert!(!range.contains(11));
}

#[test]
fn test_line_range_len() {
    let range1 = LineRange::new(1, 1).unwrap();
    assert_eq!(range1.len(), 1);

    let range2 = LineRange::new(1, 10).unwrap();
    assert_eq!(range2.len(), 10);

    let range3 = LineRange::new(5, 15).unwrap();
    assert_eq!(range3.len(), 11);
}

#[test]
fn test_config_blacklist_parsing() {
    let mut config = Config::new(Mode::Blacklist);

    config.add_to_blacklist("test.txt#5-10");
    config.add_to_blacklist("test.txt#20-30");

    assert_eq!(config.blacklist.len(), 2);
    assert!(config.blacklist.contains(&"test.txt#5-10".to_string()));
    assert!(config.blacklist.contains(&"test.txt#20-30".to_string()));
}

#[test]
fn test_pattern_literal_matching() {
    use funveil::Pattern;

    let pattern = Pattern::from_literal("test.txt".to_string());
    assert!(pattern.matches("test.txt"));
    assert!(!pattern.matches("other.txt"));
}

#[test]
fn test_pattern_regex_matching() {
    use funveil::Pattern;

    let pattern = Pattern::from_regex(r".*\.txt$").unwrap();
    assert!(pattern.matches("test.txt"));
    assert!(pattern.matches("other.txt"));
    assert!(!pattern.matches("test.rs"));
}

#[test]
fn test_unveil_all_empty_config() {
    let temp = TempDir::new().unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let result = funveil::unveil_all(temp.path(), &mut config);
    assert!(result.is_ok());
}

#[test]
fn test_has_veils_empty_config() {
    let temp = TempDir::new().unwrap();

    let config = Config::new(Mode::Blacklist);
    assert!(!funveil::has_veils(&config, "test.txt"));
}

#[test]
fn test_config_register_and_unregister_object() {
    let mut config = Config::new(Mode::Blacklist);
    let hash = ContentHash::from_content(b"test");

    config.register_object(
        "test.txt".to_string(),
        funveil::config::ObjectMeta::new(hash.clone(), 0o644),
    );
    assert!(config.get_object("test.txt").is_some());

    config.unregister_object("test.txt");
    assert!(config.get_object("test.txt").is_none());
}

#[test]
fn test_config_remove_from_blacklist() {
    let mut config = Config::new(Mode::Blacklist);

    config.add_to_blacklist("secret.env");
    assert!(config.blacklist.contains(&"secret.env".to_string()));

    let removed = config.remove_from_blacklist("secret.env");
    assert!(removed);
    assert!(!config.blacklist.contains(&"secret.env".to_string()));

    let removed_again = config.remove_from_blacklist("secret.env");
    assert!(!removed_again);
}

#[test]
fn test_config_remove_from_whitelist() {
    let mut config = Config::new(Mode::Whitelist);

    config.add_to_whitelist("public.txt");
    assert!(config.whitelist.contains(&"public.txt".to_string()));

    let removed = config.remove_from_whitelist("public.txt");
    assert!(removed);
    assert!(!config.whitelist.contains(&"public.txt".to_string()));
}

#[test]
fn test_checkpoint_save_restore_cycle() {
    let temp = TempDir::new().unwrap();

    fs::write(temp.path().join("file1.txt"), "content1").unwrap();
    fs::write(temp.path().join("file2.txt"), "content2").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    config.save(temp.path()).unwrap();

    funveil::save_checkpoint(temp.path(), &config, "test-cycle").unwrap();

    let checkpoints = funveil::list_checkpoints(temp.path()).unwrap();
    assert!(checkpoints.contains(&"test-cycle".to_string()));

    fs::write(temp.path().join("file1.txt"), "modified1").unwrap();
    fs::write(temp.path().join("file2.txt"), "modified2").unwrap();

    funveil::restore_checkpoint(temp.path(), "test-cycle").unwrap();

    let restored1 = fs::read_to_string(temp.path().join("file1.txt")).unwrap();
    let restored2 = fs::read_to_string(temp.path().join("file2.txt")).unwrap();

    assert_eq!(restored1, "content1");
    assert_eq!(restored2, "content2");
}

#[test]
fn test_veil_unveil_multiple_ranges() {
    let temp = TempDir::new().unwrap();

    let original = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();

    let mut config = Config::new(Mode::Blacklist);

    let ranges = vec![
        LineRange::new(2, 3).unwrap(),
        LineRange::new(6, 7).unwrap(),
        LineRange::new(9, 10).unwrap(),
    ];

    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("1"));
    assert!(veiled.contains("4"));
    assert!(veiled.contains("5"));
    assert!(veiled.contains("8"));
    assert!(veiled.contains("..."));

    funveil::unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, original);
}

#[test]
fn test_line_range_overlapping() {
    let range1 = LineRange::new(1, 10).unwrap();
    let range2 = LineRange::new(5, 15).unwrap();
    let range3 = LineRange::new(20, 30).unwrap();

    assert!(range1.overlaps(&range2));
    assert!(range2.overlaps(&range1));
    assert!(!range1.overlaps(&range3));
    assert!(!range3.overlaps(&range1));
}

#[test]
fn test_config_load_missing_file() {
    let temp = TempDir::new().unwrap();

    let result = Config::load(temp.path());
    assert!(result.is_ok());

    let config = result.unwrap();
    assert_eq!(config.mode, Mode::Whitelist);
    assert!(config.blacklist.is_empty());
    assert!(config.whitelist.is_empty());
}

#[test]
fn test_veil_file_with_special_characters() {
    let temp = TempDir::new().unwrap();

    let content = "Hello, 世界! 🌍\nПривет мир\n";
    fs::write(temp.path().join("unicode.txt"), content).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "unicode.txt", None).unwrap();
    funveil::unveil_file(temp.path(), &mut config, "unicode.txt", None).unwrap();

    let restored = fs::read_to_string(temp.path().join("unicode.txt")).unwrap();
    assert_eq!(restored, content);
}

#[test]
fn test_veil_empty_file_full() {
    let temp = TempDir::new().unwrap();

    fs::write(temp.path().join("empty.txt"), "").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let result = funveil::veil_file(temp.path(), &mut config, "empty.txt", None);
    assert!(result.is_ok());
}

#[test]
fn test_content_hash_consistency() {
    let content = b"test content for hashing";

    let hash1 = ContentHash::from_content(content);
    let hash2 = ContentHash::from_content(content);
    let hash3 = ContentHash::from_content(b"different content");

    assert_eq!(hash1.full(), hash2.full());
    assert_ne!(hash1.full(), hash3.full());
    assert!(!hash1.full().is_empty());
}

#[test]
fn test_content_hash_path_components() {
    let hash = ContentHash::from_content(b"test");

    let (a, b, c) = hash.path_components();
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 2);
    assert!(!c.is_empty());
}

#[test]
fn test_checkpoint_show_nonexistent() {
    let temp = TempDir::new().unwrap();

    let result = funveil::show_checkpoint(temp.path(), "nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_checkpoint_delete_nonexistent() {
    let temp = TempDir::new().unwrap();

    let result = funveil::delete_checkpoint(temp.path(), "nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_cas_store_and_retrieve_unicode() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());

    let unicode_content = "Hello 世界 🌍 Привет мир";
    let hash = store.store(unicode_content.as_bytes()).unwrap();

    let retrieved = store.retrieve(&hash).unwrap();
    let retrieved_str = String::from_utf8(retrieved).unwrap();

    assert_eq!(retrieved_str, unicode_content);
}

#[test]
fn test_config_mode_switching() {
    let mut config = Config::new(Mode::Whitelist);
    assert_eq!(config.mode, Mode::Whitelist);

    config.mode = Mode::Blacklist;
    assert_eq!(config.mode, Mode::Blacklist);
}

#[test]
fn test_config_is_veiled_blacklist_mode() {
    let mut config = Config::new(Mode::Blacklist);

    config.add_to_blacklist("secret.env");

    assert!(config.is_veiled("secret.env", 1).unwrap());
    assert!(!config.is_veiled("public.txt", 1).unwrap());
}

#[test]
fn test_config_is_veiled_whitelist_mode() {
    let mut config = Config::new(Mode::Whitelist);

    config.add_to_whitelist("public.txt");

    assert!(!config.is_veiled("public.txt", 1).unwrap());
    assert!(config.is_veiled("secret.env", 1).unwrap());
}

#[test]
fn test_config_is_veiled_with_ranges() {
    let mut config = Config::new(Mode::Blacklist);

    config.add_to_blacklist("test.txt#10-20");

    assert!(!config.is_veiled("test.txt", 5).unwrap());
    assert!(config.is_veiled("test.txt", 15).unwrap());
    assert!(!config.is_veiled("test.txt", 25).unwrap());
}

#[test]
fn test_config_empty_blacklist() {
    let config = Config::new(Mode::Blacklist);
    assert!(config.blacklist.is_empty());
}

#[test]
fn test_config_empty_whitelist() {
    let config = Config::new(Mode::Whitelist);
    assert!(config.whitelist.is_empty());
}

#[test]
fn test_line_range_new_valid() {
    let range = LineRange::new(1, 10);
    assert!(range.is_ok());
    let r = range.unwrap();
    assert_eq!(r.start(), 1);
    assert_eq!(r.end(), 10);
}

#[test]
fn test_line_range_new_invalid() {
    let range1 = LineRange::new(0, 10);
    assert!(range1.is_err());

    let range2 = LineRange::new(10, 5);
    assert!(range2.is_err());
}

#[test]
fn test_line_range_single_line() {
    let range = LineRange::new(5, 5).unwrap();
    assert_eq!(range.len(), 1);
    assert!(range.contains(5));
    assert!(!range.contains(4));
    assert!(!range.contains(6));
}

#[test]
fn test_content_hash_from_string() {
    let hash_str = "a3f7d2e9c4b1a8f6e5d3c2b4a1f7e8d9c6b3a5f2e1d4c7b8a9f6e3d2c1b5a4f8";
    let hash = ContentHash::from_string(hash_str.to_string());
    assert_eq!(hash.full(), hash_str);
}

#[test]
fn test_content_hash_equality() {
    let hash1 = ContentHash::from_content(b"test");
    let hash2 = ContentHash::from_content(b"test");
    let hash3 = ContentHash::from_content(b"other");

    assert_eq!(hash1.full(), hash2.full());
    assert_ne!(hash1.full(), hash3.full());
}

#[test]
fn test_config_entry_parse_literal() {
    let entry = ConfigEntry::parse("file.txt").unwrap();
    assert!(entry.pattern.matches("file.txt"));
    assert!(!entry.pattern.matches("other.txt"));
    assert!(entry.ranges.is_none());
}

#[test]
fn test_config_entry_parse_with_range() {
    let entry = ConfigEntry::parse("file.txt#10-20").unwrap();
    assert!(entry.pattern.matches("file.txt"));
    assert!(entry.ranges.is_some());
    let ranges = entry.ranges.unwrap();
    assert_eq!(ranges.len(), 1);
}

#[test]
fn test_config_entry_parse_with_multiple_ranges() {
    let entry = ConfigEntry::parse("file.txt#10-20,30-40").unwrap();
    assert!(entry.pattern.matches("file.txt"));
    let ranges = entry.ranges.unwrap();
    assert_eq!(ranges.len(), 2);
}

#[test]
fn test_pattern_from_regex_valid() {
    let pattern = Pattern::from_regex(r".*\.txt$");
    assert!(pattern.is_ok());
    let p = pattern.unwrap();
    assert!(p.matches("test.txt"));
    assert!(p.matches("other.txt"));
    assert!(!p.matches("test.rs"));
}

#[test]
fn test_pattern_from_regex_invalid() {
    let pattern = Pattern::from_regex(r"[invalid");
    assert!(pattern.is_err());
}

#[test]
fn test_veil_file_creates_marker() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "content\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("..."));
}

#[test]
fn test_unveil_restores_exact_content() {
    let temp = TempDir::new().unwrap();

    let original = "line1\nline2\nline3\nline4\nline5\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "test.txt", None).unwrap();
    funveil::unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, original);
}

#[test]
fn test_config_object_registration() {
    let mut config = Config::new(Mode::Blacklist);
    let hash = ContentHash::from_content(b"test");

    config.register_object(
        "test.txt".to_string(),
        funveil::config::ObjectMeta::new(hash.clone(), 0o644),
    );

    let obj = config.get_object("test.txt");
    assert!(obj.is_some());
    assert_eq!(obj.unwrap().hash, hash.full());
}

#[test]
fn test_config_object_removal() {
    let mut config = Config::new(Mode::Blacklist);
    let hash = ContentHash::from_content(b"test");

    config.register_object(
        "test.txt".to_string(),
        funveil::config::ObjectMeta::new(hash, 0o644),
    );
    assert!(config.get_object("test.txt").is_some());

    config.unregister_object("test.txt");
    assert!(config.get_object("test.txt").is_none());
}
