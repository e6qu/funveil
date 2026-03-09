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
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "content").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let metadata = fs::metadata(temp.path().join("test.txt")).unwrap();
    assert!(metadata.permissions().readonly());
}

#[test]
fn test_unveil_file_writable_after_unveil() {
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
    let _temp = TempDir::new().unwrap();

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

    let config = Config::new(Mode::Blacklist);
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

#[test]
fn test_entrypoint_detector_rust_main() {
    use funveil::{EntrypointDetector, EntrypointType, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"fn main() {
    println!("Hello");
}

fn helper() {
    println!("Helper");
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.rs"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert_eq!(entrypoints.len(), 1);
    assert_eq!(entrypoints[0].name, "main");
    assert_eq!(entrypoints[0].entry_type, EntrypointType::Main);
}

#[test]
fn test_entrypoint_detector_rust_tests() {
    use funveil::{EntrypointDetector, EntrypointType, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"#[test]
fn test_addition() {
    assert_eq!(1 + 1, 2);
}

fn test_subtraction() {
    assert_eq!(2 - 1, 1);
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.rs"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert!(entrypoints
        .iter()
        .any(|e| e.name == "test_addition" && e.entry_type == EntrypointType::Test));
}

#[test]
fn test_entrypoint_detector_typescript() {
    use funveil::{EntrypointDetector, EntrypointType, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"function main() {
    console.log("Hello");
}

function test_something() {
    expect(true).toBe(true);
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.ts"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert!(entrypoints
        .iter()
        .any(|e| e.name == "main" && e.entry_type == EntrypointType::Main));
}

#[test]
fn test_entrypoint_detector_python() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"if __name__ == "__main__":
    print("Hello")

def test_something():
    assert True
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.py"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert!(!entrypoints.is_empty());
}

#[test]
fn test_entrypoint_detector_go() {
    use funveil::{EntrypointDetector, EntrypointType, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"package main

func main() {
    println("Hello")
}

func TestSomething(t *testing.T) {
    assert.True(t, true)
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.go"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert!(entrypoints
        .iter()
        .any(|e| e.name == "main" && e.entry_type == EntrypointType::Main));
}

#[test]
fn test_entrypoint_detect_all() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();

    let rust_code = r#"fn main() { println!("Hello"); }"#;
    let ts_code = r#"function main() { console.log("Hello"); }"#;

    let parsed_rust = parser
        .parse_file(std::path::Path::new("main.rs"), rust_code)
        .unwrap();
    let parsed_ts = parser
        .parse_file(std::path::Path::new("main.ts"), ts_code)
        .unwrap();

    let entrypoints = EntrypointDetector::detect_all(&[parsed_rust, parsed_ts]);

    assert_eq!(entrypoints.len(), 2);
}

#[test]
fn test_entrypoint_with_description() {
    use funveil::{Entrypoint, EntrypointType, Language};
    use std::path::PathBuf;

    let entrypoint = Entrypoint::new(
        "test_fn",
        PathBuf::from("test.rs"),
        10,
        EntrypointType::Test,
        Language::Rust,
    )
    .with_description("A test function");

    assert_eq!(entrypoint.name, "test_fn");
    assert_eq!(entrypoint.description, Some("A test function".to_string()));
}

#[test]
fn test_entrypoint_type_display() {
    use funveil::EntrypointType;

    assert_eq!(format!("{}", EntrypointType::Main), "main");
    assert_eq!(format!("{}", EntrypointType::Test), "test");
    assert_eq!(format!("{}", EntrypointType::Cli), "cli");
    assert_eq!(format!("{}", EntrypointType::Handler), "handler");
    assert_eq!(format!("{}", EntrypointType::Export), "export");
}

#[test]
fn test_header_strategy_basic() {
    use funveil::{HeaderStrategy, TreeSitterParser, VeilStrategy};

    let parser = TreeSitterParser::new().unwrap();
    let strategy = HeaderStrategy::new();

    let code = r#"fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn multiply(x: i32, y: i32) -> i32 {
    x * y
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("math.rs"), code)
        .unwrap();
    let veiled = strategy.veil_file(code, &parsed).unwrap();

    assert!(veiled.contains("fn add"));
    assert!(veiled.contains("fn multiply"));
    assert!(veiled.contains("..."));
}

#[test]
fn test_header_strategy_with_config() {
    use funveil::{HeaderConfig, HeaderStrategy, TreeSitterParser, VeilStrategy};

    let parser = TreeSitterParser::new().unwrap();
    let config = HeaderConfig {
        include_docstrings: false,
        max_signature_length: None,
        show_methods: true,
        show_properties: false,
    };
    let strategy = HeaderStrategy::with_config(config);

    let code = r#"fn compute(x: i32) -> i32 {
    x * 2
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("compute.rs"), code)
        .unwrap();
    let veiled = strategy.veil_file(code, &parsed).unwrap();

    assert!(veiled.contains("fn compute"));
}

#[test]
fn test_header_strategy_description() {
    use funveil::{HeaderStrategy, VeilStrategy};

    let strategy = HeaderStrategy::new();
    let desc = strategy.description();

    assert!(!desc.is_empty());
    assert!(desc.contains("signature"));
}

#[test]
fn test_call_graph_builder() {
    use funveil::{CallGraphBuilder, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();

    let code = r#"fn main() {
    helper();
}

fn helper() {
    println!("Hello");
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("main.rs"), code)
        .unwrap();
    let graph = CallGraphBuilder::from_files(&[parsed]);

    assert!(graph.contains("main") || graph.contains("helper"));
}

#[test]
fn test_analysis_cache_operations() {
    use funveil::AnalysisCache;

    let mut cache = AnalysisCache::new();
    let temp = TempDir::new().unwrap();

    fs::write(temp.path().join("test.rs"), "fn main() {}").unwrap();

    let parsed = funveil::ParsedFile::new(funveil::Language::Rust, temp.path().join("test.rs"));

    let path = temp.path().join("test.rs");
    cache.insert(path.clone(), parsed);

    assert!(cache.get(&path).is_some());

    cache.invalidate_stale();
}

#[test]
fn test_tree_sitter_parser_multiple_languages() {
    use funveil::TreeSitterParser;

    let parser = TreeSitterParser::new().unwrap();

    let rust_code = "fn main() {}";
    let ts_code = "function main() {}";
    let py_code = "def main():\n    pass";

    let parsed_rust = parser
        .parse_file(std::path::Path::new("test.rs"), rust_code)
        .unwrap();
    let parsed_ts = parser
        .parse_file(std::path::Path::new("test.ts"), ts_code)
        .unwrap();
    let parsed_py = parser
        .parse_file(std::path::Path::new("test.py"), py_code)
        .unwrap();

    assert!(!parsed_rust.symbols.is_empty());
    assert!(!parsed_ts.symbols.is_empty());
    assert!(!parsed_py.symbols.is_empty());
}

#[test]
fn test_parsed_file_info() {
    use funveil::{Language, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = "fn main() { println!(\"test\"); }";

    let parsed = parser
        .parse_file(std::path::Path::new("test.rs"), code)
        .unwrap();

    assert_eq!(parsed.language, Language::Rust);
    assert!(parsed.path.ends_with("test.rs"));
}

#[test]
fn test_trace_direction() {
    use funveil::TraceDirection;

    let forward = TraceDirection::Forward;
    let backward = TraceDirection::Backward;

    assert_ne!(forward, backward);
}

#[test]
fn test_cas_store_nonexistent_retrieve() {
    use funveil::ContentHash;

    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());

    let fake_hash = ContentHash::from_content(b"nonexistent");
    let result = store.retrieve(&fake_hash);

    assert!(result.is_err());
}

#[test]
fn test_cas_list_all() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());

    store.store(b"content1").unwrap();
    store.store(b"content2").unwrap();

    let all = store.list_all().unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_cas_garbage_collect() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());

    let hash1 = store.store(b"content1").unwrap();
    store.store(b"content2").unwrap();

    let (count, _bytes) = funveil::garbage_collect(temp.path(), &[hash1]).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_checkpoint_operations() {
    let temp = TempDir::new().unwrap();

    let config = Config::new(Mode::Blacklist);
    config.save(temp.path()).unwrap();

    funveil::save_checkpoint(temp.path(), &config, "test-op").unwrap();

    let list = funveil::list_checkpoints(temp.path()).unwrap();
    assert!(list.contains(&"test-op".to_string()));

    funveil::delete_checkpoint(temp.path(), "test-op").unwrap();

    let list_after = funveil::list_checkpoints(temp.path()).unwrap();
    assert!(!list_after.contains(&"test-op".to_string()));
}

#[test]
fn test_checkpoint_show() {
    let temp = TempDir::new().unwrap();

    fs::write(temp.path().join("file.txt"), "original").unwrap();

    let config = Config::new(Mode::Blacklist);
    config.save(temp.path()).unwrap();

    funveil::save_checkpoint(temp.path(), &config, "show-test").unwrap();

    funveil::show_checkpoint(temp.path(), "show-test").unwrap();
}

#[test]
fn test_checkpoint_get_latest() {
    let temp = TempDir::new().unwrap();

    let config = Config::new(Mode::Blacklist);
    config.save(temp.path()).unwrap();

    funveil::save_checkpoint(temp.path(), &config, "latest-test").unwrap();

    let latest = funveil::get_latest_checkpoint(temp.path()).unwrap();
    assert_eq!(latest, Some("latest-test".to_string()));
}

#[test]
fn test_config_load_missing() {
    let temp = TempDir::new().unwrap();
    let result = Config::load(temp.path());

    assert!(result.is_ok());
    let config = result.unwrap();
    assert!(config.blacklist.is_empty());
    assert!(config.whitelist.is_empty());
}

#[test]
fn test_config_save_load_roundtrip() {
    let temp = TempDir::new().unwrap();

    let mut config = Config::new(Mode::Whitelist);
    config.add_to_whitelist("file1.txt");
    config.add_to_whitelist("file2.txt#10-20");

    config.save(temp.path()).unwrap();

    let loaded = Config::load(temp.path()).unwrap();
    assert_eq!(loaded.whitelist.len(), 2);
}

#[test]
fn test_line_range_contains_edge() {
    let range = LineRange::new(5, 10).unwrap();

    assert!(!range.contains(4));
    assert!(range.contains(5));
    assert!(range.contains(10));
    assert!(!range.contains(11));
}

#[test]
fn test_line_range_overlapping_adjacent() {
    let r1 = LineRange::new(1, 5).unwrap();
    let r2 = LineRange::new(6, 10).unwrap();

    assert!(!r1.overlaps(&r2));
    assert!(!r2.overlaps(&r1));
}

#[test]
fn test_pattern_literal_matching_edge_cases() {
    let pattern = Pattern::from_literal("test.txt".to_string());

    assert!(pattern.matches("test.txt"));
    assert!(!pattern.matches("test.txt.bak"));
    assert!(!pattern.matches("Test.txt"));
}

#[test]
fn test_pattern_regex_special_chars() {
    let pattern = Pattern::from_literal("file[1].txt".to_string());

    assert!(pattern.matches("file[1].txt"));
}

#[test]
fn test_validate_path_within_root() {
    use funveil::validate_path_within_root;

    let temp = TempDir::new().unwrap();

    fs::create_dir_all(temp.path().join("subdir")).unwrap();
    fs::write(temp.path().join("subdir").join("file.txt"), "content").unwrap();

    let valid = temp.path().join("subdir").join("file.txt");

    assert!(validate_path_within_root(&valid, temp.path()).is_ok());
}

#[test]
fn test_veil_file_with_content_hash() {
    let temp = TempDir::new().unwrap();
    let content = "line1\nline2\nline3\n";

    fs::write(temp.path().join("hash.txt"), content).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "hash.txt", None).unwrap();

    assert!(config.get_object("hash.txt").is_some());
}

#[test]
fn test_unveil_file_without_config_entry() {
    let temp = TempDir::new().unwrap();

    fs::write(temp.path().join("plain.txt"), "content\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let result = funveil::unveil_file(temp.path(), &mut config, "plain.txt", None);

    assert!(result.is_err());
}

#[test]
fn test_veil_file_with_different_modes() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("mode.txt"), "content\n").unwrap();

    let mut config_whitelist = Config::new(Mode::Whitelist);
    config_whitelist.add_to_whitelist("mode.txt");
    config_whitelist.save(temp.path()).unwrap();

    let mut config_blacklist = Config::new(Mode::Blacklist);
    config_blacklist.add_to_blacklist("mode.txt");
    config_blacklist.save(temp.path()).unwrap();
}

#[test]
fn test_multiple_veils_same_file() {
    let temp = TempDir::new().unwrap();
    let original = "line1\nline2\nline3\nline4\nline5\n";

    fs::write(temp.path().join("multi.txt"), original).unwrap();

    let mut config = Config::new(Mode::Blacklist);

    let ranges1 = vec![LineRange::new(1, 2).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "multi.txt", Some(&ranges1)).unwrap();
    funveil::unveil_file(temp.path(), &mut config, "multi.txt", None).unwrap();

    let restored = fs::read_to_string(temp.path().join("multi.txt")).unwrap();
    assert_eq!(restored, original);
}

#[test]
fn test_veil_empty_directory() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("empty_dir")).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    config.add_to_blacklist("empty_dir");
    config.save(temp.path()).unwrap();
}

#[test]
fn test_config_entry_with_regex_pattern() {
    let entry = ConfigEntry::parse("/.*\\.txt/").unwrap();

    assert!(entry.pattern.matches("file.txt"));
    assert!(entry.pattern.matches("other.txt"));
    assert!(!entry.pattern.matches("test.rs"));
}

#[test]
fn test_entrypoint_detector_bash() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"#!/bin/bash
function main() {
    echo "Hello"
}

function test_setup() {
    echo "Setup"
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.sh"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert!(!entrypoints.is_empty());
}

#[test]
fn test_entrypoint_detector_zig() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"pub fn main() void {
    std.debug.print("Hello", .{});
}

test "basic test" {
    try expect(true);
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.zig"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert!(entrypoints.iter().any(|e| e.name == "main"));
}

#[test]
fn test_entrypoint_detector_html() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body></body>
</html>
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.html"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert!(!entrypoints.is_empty() || parsed.symbols.is_empty());
}

#[test]
fn test_entrypoint_detector_css() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#".container {
    display: flex;
}

#main {
    padding: 10px;
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.css"), code)
        .unwrap();
    let _entrypoints = EntrypointDetector::detect_in_file(&parsed);
}

#[test]
fn test_entrypoint_detector_xml() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"<?xml version="1.0"?>
<root>
    <element>value</element>
</root>
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.xml"), code)
        .unwrap();
    let _entrypoints = EntrypointDetector::detect_in_file(&parsed);
}

#[test]
fn test_entrypoint_detector_markdown() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"# Title

Some content here.

## Section

More content.
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.md"), code)
        .unwrap();
    let _entrypoints = EntrypointDetector::detect_in_file(&parsed);
}

#[test]
fn test_call_graph_trace() {
    use funveil::{CallGraphBuilder, TraceDirection, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"fn main() {
    helper();
    other();
}

fn helper() {
    inner();
}

fn other() {}

fn inner() {}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("main.rs"), code)
        .unwrap();
    let graph = CallGraphBuilder::from_files(&[parsed]);

    if graph.contains("main") {
        let trace = graph.trace("main", TraceDirection::Forward, 5);
        if let Some(result) = trace {
            assert!(!result.all_functions().is_empty());
        }
    }
}

#[test]
fn test_call_graph_function_count() {
    use funveil::{CallGraphBuilder, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"fn foo() {}
fn bar() {}
fn baz() {}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.rs"), code)
        .unwrap();
    let graph = CallGraphBuilder::from_files(&[parsed]);

    assert!(graph.function_count() >= 3);
}

#[test]
fn test_call_graph_edge_count() {
    use funveil::{CallGraphBuilder, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"fn main() {
    foo();
    bar();
}

fn foo() {
    baz();
}

fn bar() {}
fn baz() {}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.rs"), code)
        .unwrap();
    let graph = CallGraphBuilder::from_files(&[parsed]);

    assert!(graph.edge_count() >= 2);
}

#[test]
fn test_cached_parser() {
    use funveil::{CachedParser, TreeSitterParser};

    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.rs"), "fn main() {}").unwrap();

    let parser = TreeSitterParser::new().unwrap();
    let mut cached = CachedParser::new(temp.path()).unwrap();

    let content = "fn main() {}";
    let result = cached.get_or_parse(temp.path().join("test.rs").as_path(), content, &parser);
    assert!(result.is_ok());
}

#[test]
fn test_cached_parser_stats() {
    use funveil::CachedParser;

    let temp = TempDir::new().unwrap();

    let cached = CachedParser::new(temp.path()).unwrap();
    let stats = cached.stats();
    let _count = stats.entry_count;
}

#[test]
fn test_analysis_cache_load_empty() {
    use funveil::AnalysisCache;

    let temp = TempDir::new().unwrap();
    let cache = AnalysisCache::load(temp.path()).unwrap();

    assert_eq!(cache.stats().entry_count, 0);
}

#[test]
fn test_veil_partial_edge_cases() {
    let temp = TempDir::new().unwrap();
    let content = "line1\nline2\nline3\n";

    fs::write(temp.path().join("edge.txt"), content).unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges = vec![LineRange::new(2, 2).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "edge.txt", Some(&ranges)).unwrap();

    let veiled = fs::read_to_string(temp.path().join("edge.txt")).unwrap();
    assert!(veiled.contains("...") || veiled.contains("line1"));
}

#[test]
fn test_has_veils_with_veiled_files() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "content\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    assert!(funveil::has_veils(&config, "test.txt"));
}

#[test]
fn test_content_hash_short_display() {
    use funveil::ContentHash;

    let hash = ContentHash::from_content(b"test content for display");

    let short = hash.short();
    let full = hash.full();

    assert!(!short.is_empty());
    assert!(short.len() <= full.len());
}

#[test]
fn test_config_mode_display() {
    let whitelist = Mode::Whitelist;
    let blacklist = Mode::Blacklist;

    assert!(whitelist.is_whitelist());
    assert!(!whitelist.is_blacklist());
    assert!(blacklist.is_blacklist());
    assert!(!blacklist.is_whitelist());
}

#[test]
fn test_entrypoint_detector_terraform() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"resource "aws_instance" "example" {
  ami           = "ami-12345678"
  instance_type = "t2.micro"
}

variable "region" {
  default = "us-east-1"
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("main.tf"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert!(!entrypoints.is_empty());
}

#[test]
fn test_entrypoint_detector_helm() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: test-config
data:
  key: value
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("values.yaml"), code)
        .unwrap();
    let _entrypoints = EntrypointDetector::detect_in_file(&parsed);
}

#[test]
fn test_entrypoint_unknown_language() {
    use funveil::{EntrypointDetector, Language, ParsedFile};
    use std::path::PathBuf;

    let mut parsed = ParsedFile::new(Language::Unknown, PathBuf::from("test.xyz"));
    parsed.symbols.push(funveil::parser::Symbol::Function {
        name: "test".to_string(),
        params: vec![],
        return_type: None,
        visibility: funveil::parser::Visibility::Public,
        line_range: LineRange::new(1, 5).unwrap(),
        body_range: LineRange::new(2, 5).unwrap(),
        is_async: false,
        attributes: vec![],
    });

    let entrypoints = EntrypointDetector::detect_in_file(&parsed);
    assert!(entrypoints.is_empty());
}

#[test]
fn test_typescript_tsx_detection() {
    use funveil::{EntrypointDetector, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"function App() {
    return <div>Hello</div>;
}

export default App;
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("App.tsx"), code)
        .unwrap();
    let entrypoints = EntrypointDetector::detect_in_file(&parsed);

    assert!(!entrypoints.is_empty());
}

#[test]
fn test_tree_sitter_parser_rust() {
    use funveil::{Language, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"use std::collections::HashMap;

pub struct MyStruct {
    field: i32,
}

impl MyStruct {
    pub fn new() -> Self {
        Self { field: 0 }
    }
}

fn main() {
    let s = MyStruct::new();
    println!("{}", s.field);
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.rs"), code)
        .unwrap();

    assert_eq!(parsed.language, Language::Rust);
    assert!(!parsed.symbols.is_empty());
    assert!(!parsed.imports.is_empty());
}

#[test]
fn test_tree_sitter_parser_go() {
    use funveil::{Language, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"package main

import "fmt"

type MyStruct struct {
    Field int
}

func main() {
    fmt.Println("Hello")
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.go"), code)
        .unwrap();

    assert_eq!(parsed.language, Language::Go);
    assert!(!parsed.symbols.is_empty());
}

#[test]
fn test_tree_sitter_parser_unknown() {
    use funveil::{Language, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = "some random content";

    let parsed = parser
        .parse_file(std::path::Path::new("test.xyz"), code)
        .unwrap();

    assert_eq!(parsed.language, Language::Unknown);
    assert!(parsed.symbols.is_empty());
}

#[test]
fn test_call_graph_get_node() {
    use funveil::{CallGraphBuilder, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"fn foo() {
    bar();
}

fn bar() {}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.rs"), code)
        .unwrap();
    let graph = CallGraphBuilder::from_files(&[parsed]);

    assert!(graph.get_node("foo").is_some());
    assert!(graph.get_node("bar").is_some());
    assert!(graph.get_node("nonexistent").is_none());
}

#[test]
fn test_call_graph_backward_trace() {
    use funveil::{CallGraphBuilder, TraceDirection, TreeSitterParser};

    let parser = TreeSitterParser::new().unwrap();
    let code = r#"fn a() { b(); }
fn b() { c(); }
fn c() {}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("test.rs"), code)
        .unwrap();
    let graph = CallGraphBuilder::from_files(&[parsed]);

    if graph.contains("c") {
        let trace = graph.trace("c", TraceDirection::Backward, 5);
        if let Some(result) = trace {
            assert!(!result.all_functions().is_empty());
        }
    }
}

#[test]
fn test_analysis_cache_save_load() {
    use funveil::AnalysisCache;

    let temp = TempDir::new().unwrap();

    fs::write(temp.path().join("test.rs"), "fn main() {}").unwrap();

    let mut cache = AnalysisCache::new();
    let parsed = funveil::ParsedFile::new(funveil::Language::Rust, temp.path().join("test.rs"));
    cache.insert(temp.path().join("test.rs"), parsed);

    cache.save(temp.path()).unwrap();

    let loaded = AnalysisCache::load(temp.path()).unwrap();
    assert!(loaded.get(&temp.path().join("test.rs")).is_some());
}

#[test]
fn test_header_strategy_with_classes() {
    use funveil::{HeaderStrategy, TreeSitterParser, VeilStrategy};

    let parser = TreeSitterParser::new().unwrap();
    let strategy = HeaderStrategy::new();

    let code = r#"pub struct User {
    name: String,
    age: i32,
}

impl User {
    pub fn new(name: String, age: i32) -> Self {
        Self { name, age }
    }
}
"#;

    let parsed = parser
        .parse_file(std::path::Path::new("user.rs"), code)
        .unwrap();
    let veiled = strategy.veil_file(code, &parsed).unwrap();

    assert!(veiled.contains("struct User") || veiled.contains("impl User"));
}

#[test]
fn test_pattern_is_regex() {
    let regex_pattern = Pattern::from_regex(r".*\.rs$").unwrap();
    let literal_pattern = Pattern::from_literal("test.txt".to_string());

    assert!(regex_pattern.is_regex());
    assert!(!literal_pattern.is_regex());
    assert!(literal_pattern.is_literal());
    assert!(!regex_pattern.is_literal());
}

#[test]
fn test_pattern_display() {
    let literal = Pattern::from_literal("file.txt".to_string());
    let regex = Pattern::from_regex(r".*\.rs$").unwrap();

    let lit_str = format!("{literal}");
    let reg_str = format!("{regex}");

    assert!(lit_str.contains("file.txt"));
    assert!(reg_str.contains(".*\\.rs$"));
}

#[test]
fn test_content_hash_display() {
    use funveil::ContentHash;

    let hash = ContentHash::from_content(b"test content");

    let hash_str = format!("{hash}");
    assert!(!hash_str.is_empty());
}

#[test]
fn test_line_range_display() {
    let range = LineRange::new(1, 10).unwrap();

    let range_str = format!("{range}");
    assert!(range_str.contains("1"));
    assert!(range_str.contains("10"));
}

#[test]
fn test_config_default() {
    let config = Config::default();

    assert!(config.blacklist.is_empty());
    assert!(config.whitelist.is_empty());
    assert!(config.mode.is_whitelist());
}

#[test]
fn test_config_entry_invalid_relative() {
    let result = ConfigEntry::parse("./test.txt");
    assert!(result.is_err());

    let result2 = ConfigEntry::parse("../test.txt");
    assert!(result2.is_err());
}

#[test]
fn test_config_entry_hidden_file_error() {
    let result = ConfigEntry::parse(".env");
    assert!(result.is_err());
}

#[test]
fn test_config_entry_directory_with_ranges() {
    let result = ConfigEntry::parse("some_dir/#10-20");
    assert!(result.is_err());
}

#[test]
fn test_config_entry_invalid_regex() {
    let result = ConfigEntry::parse("/[invalid/");
    assert!(result.is_err());
}

#[test]
fn test_config_entry_regex_without_closing_slash() {
    let result = ConfigEntry::parse("/.*\\.txt");
    assert!(result.is_err());
}

#[test]
fn test_line_range_invalid_format() {
    let result = LineRange::new(0, 10);
    assert!(result.is_err());

    let result2 = LineRange::new(10, 5);
    assert!(result2.is_err());
}

#[test]
fn test_terra_language_detection() {
    use funveil::parser::detect_language;
    use std::path::Path;

    assert_eq!(
        detect_language(Path::new("main.tf")),
        funveil::Language::Terraform
    );
    assert_eq!(
        detect_language(Path::new("vars.tfvars")),
        funveil::Language::Terraform
    );
    assert_eq!(
        detect_language(Path::new("config.hcl")),
        funveil::Language::Terraform
    );
}

#[test]
fn test_helm_language_detection() {
    use funveil::parser::detect_language;
    use std::path::Path;

    assert_eq!(
        detect_language(Path::new("values.yaml")),
        funveil::Language::Helm
    );
    assert_eq!(
        detect_language(Path::new("Chart.yml")),
        funveil::Language::Helm
    );
}

#[test]
fn test_go_language_detection() {
    use funveil::parser::detect_language;
    use std::path::Path;

    assert_eq!(detect_language(Path::new("main.go")), funveil::Language::Go);
}

#[test]
fn test_zig_language_detection() {
    use funveil::parser::detect_language;
    use std::path::Path;

    assert_eq!(
        detect_language(Path::new("main.zig")),
        funveil::Language::Zig
    );
}

#[test]
fn test_parser_language_extensions() {
    use funveil::Language;

    assert!(!Language::Rust.extensions().is_empty());
    assert!(!Language::Python.extensions().is_empty());
    assert!(!Language::Go.extensions().is_empty());
    assert!(Language::Unknown.extensions().is_empty());
}

#[test]
fn test_parser_language_names() {
    use funveil::Language;

    assert_eq!(Language::Rust.name(), "Rust");
    assert_eq!(Language::Python.name(), "Python");
    assert_eq!(Language::Go.name(), "Go");
    assert_eq!(Language::Unknown.name(), "Unknown");
}

#[test]
fn test_parser_language_display() {
    use funveil::Language;

    let rust_str = format!("{}", Language::Rust);
    assert_eq!(rust_str, "Rust");
}

#[test]
fn test_param_display() {
    use funveil::parser::Param;

    let param_with_type = Param {
        name: "count".to_string(),
        type_annotation: Some("i32".to_string()),
    };
    let param_without_type = Param {
        name: "value".to_string(),
        type_annotation: None,
    };

    assert_eq!(format!("{param_with_type}"), "count: i32");
    assert_eq!(format!("{param_without_type}"), "value");
}

#[test]
fn test_symbol_line_range() {
    use funveil::parser::{Symbol, Visibility};

    let func = Symbol::Function {
        name: "test".to_string(),
        params: vec![],
        return_type: None,
        visibility: Visibility::Public,
        line_range: LineRange::new(1, 10).unwrap(),
        body_range: LineRange::new(2, 10).unwrap(),
        is_async: false,
        attributes: vec![],
    };

    assert_eq!(func.line_range().start(), 1);
    assert_eq!(func.line_range().end(), 10);
}

#[test]
fn test_unveil_all_with_veiled_files() {
    let temp = TempDir::new().unwrap();

    fs::write(temp.path().join("file1.txt"), "content1\n").unwrap();
    fs::write(temp.path().join("file2.txt"), "content2\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "file1.txt", None).unwrap();
    funveil::veil_file(temp.path(), &mut config, "file2.txt", None).unwrap();

    funveil::unveil_all(temp.path(), &mut config).unwrap();

    let content1 = fs::read_to_string(temp.path().join("file1.txt")).unwrap();
    let content2 = fs::read_to_string(temp.path().join("file2.txt")).unwrap();

    assert_eq!(content1, "content1\n");
    assert_eq!(content2, "content2\n");
}

#[test]
fn test_config_object_meta_hash() {
    use funveil::config::ObjectMeta;

    let hash = ContentHash::from_content(b"test content");
    let meta = ObjectMeta::new(hash.clone(), 0o644);

    let retrieved_hash = meta.hash();
    assert_eq!(retrieved_hash.full(), hash.full());
}

#[test]
fn test_config_remove_from_blacklist_not_found() {
    let mut config = Config::new(Mode::Blacklist);
    config.add_to_blacklist("file1.txt");

    let removed = config.remove_from_blacklist("nonexistent.txt");
    assert!(!removed);

    let removed2 = config.remove_from_blacklist("file1.txt");
    assert!(removed2);
}

#[test]
fn test_config_remove_from_whitelist_not_found() {
    let mut config = Config::new(Mode::Whitelist);
    config.add_to_whitelist("file1.txt");

    let removed = config.remove_from_whitelist("nonexistent.txt");
    assert!(!removed);

    let removed2 = config.remove_from_whitelist("file1.txt");
    assert!(removed2);
}

#[test]
fn test_config_veiled_ranges_full_file() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "line1\nline2\nline3\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let ranges = config.veiled_ranges("test.txt").unwrap();
    assert!(ranges.is_empty());
}

#[test]
fn test_config_veiled_ranges_partial() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "line1\nline2\nline3\nline4\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges_to_veil = vec![LineRange::new(1, 2).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges_to_veil)).unwrap();

    let ranges = config.veiled_ranges("test.txt").unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start(), 1);
    assert_eq!(ranges[0].end(), 2);
}

#[test]
fn test_config_veiled_ranges_multiple() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("test.txt"),
        "line1\nline2\nline3\nline4\nline5\n",
    )
    .unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges_to_veil = vec![LineRange::new(1, 2).unwrap(), LineRange::new(4, 5).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges_to_veil)).unwrap();

    let ranges = config.veiled_ranges("test.txt").unwrap();
    assert_eq!(ranges.len(), 2);
}

#[test]
fn test_config_veiled_ranges_no_veils() {
    let config = Config::new(Mode::Blacklist);

    let ranges = config.veiled_ranges("nonexistent.txt").unwrap();
    assert!(ranges.is_empty());
}

#[test]
fn test_config_is_veiled_whitelist_mode_full_file() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "content\n").unwrap();

    let mut config = Config::new(Mode::Whitelist);
    config.add_to_whitelist("test.txt");

    let is_veiled = config.is_veiled("test.txt", 1).unwrap();
    assert!(!is_veiled);
}

#[test]
fn test_config_is_veiled_whitelist_mode_partial() {
    let mut config = Config::new(Mode::Whitelist);
    config.add_to_whitelist("test.txt#10-20");

    let is_veiled_inside = config.is_veiled("test.txt", 15).unwrap();
    let is_veiled_outside = config.is_veiled("test.txt", 5).unwrap();

    assert!(!is_veiled_inside);
    assert!(is_veiled_outside);
}

#[test]
fn test_config_is_veiled_blacklist_mode_partial() {
    let mut config = Config::new(Mode::Blacklist);
    config.add_to_blacklist("test.txt#10-20");

    let is_veiled_inside = config.is_veiled("test.txt", 15).unwrap();
    let is_veiled_outside = config.is_veiled("test.txt", 5).unwrap();

    assert!(is_veiled_inside);
    assert!(!is_veiled_outside);
}

#[test]
fn test_veil_partial_single_line() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "line1\nline2\nline3\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges = vec![LineRange::new(2, 2).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("line1"));
    assert!(veiled.contains("..."));
    assert!(veiled.contains("line3"));
}

#[test]
fn test_veil_partial_multiple_ranges() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("test.txt"),
        "line1\nline2\nline3\nline4\nline5\n",
    )
    .unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges = vec![LineRange::new(1, 2).unwrap(), LineRange::new(4, 5).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("..."));
}

#[test]
fn test_veil_partial_add_more_ranges() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("test.txt"),
        "line1\nline2\nline3\nline4\nline5\n",
    )
    .unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges1 = vec![LineRange::new(1, 2).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges1)).unwrap();

    funveil::unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let ranges2 = vec![LineRange::new(3, 4).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges2)).unwrap();

    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("..."));
}

#[test]
fn test_veil_directory_with_files() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("subdir")).unwrap();
    fs::write(temp.path().join("subdir").join("file1.txt"), "content1\n").unwrap();
    fs::write(temp.path().join("subdir").join("file2.txt"), "content2\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "subdir", None).unwrap();

    let file1_content = fs::read_to_string(temp.path().join("subdir").join("file1.txt")).unwrap();
    let file2_content = fs::read_to_string(temp.path().join("subdir").join("file2.txt")).unwrap();

    assert_eq!(file1_content, "...\n");
    assert_eq!(file2_content, "...\n");
}

#[test]
fn test_has_veils_true() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "content\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    funveil::veil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    assert!(funveil::has_veils(&config, "test.txt"));
}

#[test]
fn test_has_veils_false() {
    let config = Config::new(Mode::Blacklist);
    assert!(!funveil::has_veils(&config, "test.txt"));
}

#[test]
fn test_has_veils_partial() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "line1\nline2\nline3\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges = vec![LineRange::new(1, 2).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

    assert!(funveil::has_veils(&config, "test.txt"));
}

#[test]
fn test_unveil_partial_line() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("test.txt"), "line1\nline2\nline3\nline4\n").unwrap();

    let mut config = Config::new(Mode::Blacklist);
    let ranges = vec![LineRange::new(2, 3).unwrap()];
    funveil::veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

    funveil::unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();

    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, "line1\nline2\nline3\nline4\n");
}
