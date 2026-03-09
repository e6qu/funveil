# Task 004: Add Error Handling Tests

**Priority**: P1 - HIGH  
**Severity**: MEDIUM  
**Type**: Missing Test Coverage  
**Estimated Time**: 2-3 hours  
**Dependencies**: None

## Problem Statement

No tests exist for error conditions, edge cases, or failure scenarios. Tests only verify happy paths.

## Current State

### Existing Test Coverage

**Unit Tests**: 83 tests  
**CLI Tests**: 6 tests  
**E2E Tests**: 15 tests

All tests verify **successful** execution. None test:
- Error messages
- Recovery from errors
- Edge cases (empty files, binary files, corrupted CAS)
- Concurrent access
- Large files

### Test Analysis

```bash
$ grep -r "assert.*failure\|assert.*error\|assert_err" tests/
# (no results)

$ grep -r "#\[test\]" tests/ | wc -l
21

$ grep -r "should_panic" tests/
# (no results)
```

All tests use `assert().success()` - none check for proper failures.

## Missing Test Categories

### 1. Corrupted CAS Tests

**What happens when CAS objects are corrupted?**

```rust
#[test]
fn test_retrieve_corrupted_object() {
    let temp = TempDir::new().unwrap();
    let store = ContentStore::new(temp.path());
    
    // Store content
    let hash = store.store(b"original content").unwrap();
    
    // Corrupt the object
    let path = store.path_for(&hash);
    fs::write(&path, b"corrupted!").unwrap();
    
    // Retrieve should fail or detect corruption
    let result = store.retrieve(&hash);
    assert!(result.is_err());
    // OR: Should verify hash on retrieval
}

#[test]
fn test_veiled_file_with_missing_cas_object() {
    let temp = TempDir::new().unwrap();
    
    // Create config with reference to non-existent object
    let mut config = Config::new(Mode::Blacklist);
    let fake_hash = ContentHash::from_string("a".repeat(64));
    config.register_object("missing.txt".to_string(), ObjectMeta::new(fake_hash, 0o644));
    
    // Try to unveil - should error with clear message
    let result = unveil_file(temp.path(), &mut config, "missing.txt", None);
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Object not found"));
}
```

### 2. Empty File Tests

```rust
#[test]
fn test_veil_empty_file() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("empty.txt"), "").unwrap();
    
    let mut config = Config::new(Mode::Blacklist);
    let result = veil_file(temp.path(), &mut config, "empty.txt", None);
    
    // Empty files should either be rejected or handled gracefully
    assert!(result.is_ok() || result.is_err());
    
    if let Err(e) = result {
        assert!(e.to_string().contains("empty"));
    }
}

#[test]
fn test_unveil_empty_file() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("empty.txt"), "").unwrap();
    
    let mut config = Config::new(Mode::Blacklist);
    
    // Try to unveil non-veiled empty file
    let result = unveil_file(temp.path(), &mut config, "empty.txt", None);
    assert!(result.is_err());
}
```

### 3. Binary File Tests

```rust
#[test]
fn test_veil_binary_file() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("image.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    
    let mut config = Config::new(Mode::Blacklist);
    
    // Full veil should work
    let result = veil_file(temp.path(), &mut config, "image.png", None);
    assert!(result.is_ok());
    
    // Partial veil should fail
    let ranges = vec![LineRange::new(1, 5).unwrap()];
    let result = veil_file(temp.path(), &mut config, "image.png", Some(&ranges));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("binary"));
}

#[test]
fn test_binary_detection_with_extension() {
    let temp = TempDir::new().unwrap();
    
    // Text content but .exe extension
    fs::write(temp.path().join("fake.exe"), "not really an exe").unwrap();
    
    assert!(is_binary_file(&temp.path().join("fake.exe")));
}

#[test]
fn test_binary_detection_with_null_bytes() {
    let temp = TempDir::new().unwrap();
    
    // Binary content but .txt extension
    fs::write(temp.path().join("data.txt"), b"text\x00binary").unwrap();
    
    assert!(is_binary_file(&temp.path().join("data.txt")));
}
```

### 4. Invalid Config Tests

```rust
#[test]
fn test_config_missing_version() {
    let temp = TempDir::new().unwrap();
    
    let yaml = r#"
mode: whitelist
whitelist:
  - README.md
"#;
    fs::write(temp.path().join(CONFIG_FILE), yaml).unwrap();
    
    // Should load with default version or error
    let result = Config::load(temp.path());
    assert!(result.is_ok()); // Should use default
}

#[test]
fn test_config_invalid_mode() {
    let temp = TempDir::new().unwrap();
    
    let yaml = r#"
version: 1
mode: invalid_mode
"#;
    fs::write(temp.path().join(CONFIG_FILE), yaml).unwrap();
    
    let result = Config::load(temp.path());
    assert!(result.is_err());
}

#[test]
fn test_config_malformed_yaml() {
    let temp = TempDir::new().unwrap();
    
    fs::write(temp.path().join(CONFIG_FILE), "invalid: [yaml").unwrap();
    
    let result = Config::load(temp.path());
    assert!(result.is_err());
    
    let err = result.unwrap_err().to_string();
    assert!(err.contains("YAML") || err.contains("parse"));
}
```

### 5. Invalid Pattern Tests

```rust
#[test]
fn test_config_entry_invalid_regex() {
    let result = ConfigEntry::parse("/[invalid/");
    assert!(result.is_err());
    
    let err = result.unwrap_err().to_string();
    assert!(err.contains("regex") || err.contains("pattern"));
}

#[test]
fn test_config_entry_overlapping_ranges() {
    let result = ConfigEntry::parse("file.txt#10-20,15-25");
    assert!(result.is_err());
    
    let err = result.unwrap_err().to_string();
    assert!(err.contains("overlap"));
}

#[test]
fn test_config_entry_invalid_range() {
    let result = ConfigEntry::parse("file.txt#20-10"); // start > end
    assert!(result.is_err());
    
    let result = ConfigEntry::parse("file.txt#0-5"); // 0 is invalid
    assert!(result.is_err());
}

#[test]
fn test_config_entry_relative_path() {
    let result = ConfigEntry::parse("./file.txt");
    assert!(result.is_err());
    
    let result = ConfigEntry::parse("../file.txt");
    assert!(result.is_err());
}
```

### 6. Permission Tests

```rust
#[test]
#[cfg(unix)]
fn test_veil_preserves_permissions() {
    use std::os::unix::fs::PermissionsExt;
    
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("script.sh");
    fs::write(&path, "#!/bin/bash\necho hello").unwrap();
    
    // Set executable permission
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    
    let mut config = Config::new(Mode::Blacklist);
    veil_file(temp.path(), &mut config, "script.sh", None).unwrap();
    
    unveil_file(temp.path(), &mut config, "script.sh", None).unwrap();
    
    let metadata = fs::metadata(&path).unwrap();
    let mode = metadata.permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
}

#[test]
fn test_veil_readonly_file() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("readonly.txt");
    fs::write(&path, "content").unwrap();
    
    // Make read-only
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&path, perms).unwrap();
    
    let mut config = Config::new(Mode::Blacklist);
    
    // Veiling should still work (sets writable temporarily)
    let result = veil_file(temp.path(), &mut config, "readonly.txt", None);
    assert!(result.is_ok());
}
```

### 7. Protected Path Tests

```rust
#[test]
fn test_veil_config_file_fails() {
    let temp = TempDir::new().unwrap();
    let mut config = Config::new(Mode::Blacklist);
    
    let result = veil_file(temp.path(), &mut config, ".funveil_config", None);
    assert!(result.is_err());
    
    let err = result.unwrap_err().to_string();
    assert!(err.contains("protected"));
}

#[test]
fn test_veil_data_directory_fails() {
    let temp = TempDir::new().unwrap();
    let mut config = Config::new(Mode::Blacklist);
    
    let result = veil_file(temp.path(), &mut config, ".funveil/", None);
    assert!(result.is_err());
    
    let err = result.unwrap_err().to_string();
    assert!(err.contains("protected"));
}

#[test]
fn test_veil_vcs_directory_fails() {
    let temp = TempDir::new().unwrap();
    let mut config = Config::new(Mode::Blacklist);
    
    let result = veil_file(temp.path(), &mut config, ".git/config", None);
    assert!(result.is_err());
    
    let err = result.unwrap_err().to_string();
    assert!(err.contains("VCS") || err.contains("git"));
}
```

### 8. Large File Tests

```rust
#[test]
fn test_veil_large_file() {
    let temp = TempDir::new().unwrap();
    
    // Create 10MB file
    let large_content = "x".repeat(10 * 1024 * 1024);
    fs::write(temp.path().join("large.txt"), &large_content).unwrap();
    
    let mut config = Config::new(Mode::Blacklist);
    
    // Should handle without running out of memory
    let result = veil_file(temp.path(), &mut config, "large.txt", None);
    assert!(result.is_ok());
    
    // Verify can unveil
    let result = unveil_file(temp.path(), &mut config, "large.txt", None);
    assert!(result.is_ok());
    
    // Verify content matches
    let restored = fs::read_to_string(temp.path().join("large.txt")).unwrap();
    assert_eq!(restored.len(), large_content.len());
}
```

### 9. Concurrent Access Tests

```rust
#[test]
fn test_concurrent_veil_unveil() {
    use std::thread;
    
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("file.txt"), "content").unwrap();
    
    let root = temp.path().to_path_buf();
    
    // Spawn threads that veil/unveil simultaneously
    let handles: Vec<_> = (0..10).map(|i| {
        let root = root.clone();
        thread::spawn(move || {
            let mut config = Config::load(&root).unwrap();
            if i % 2 == 0 {
                veil_file(&root, &mut config, "file.txt", None)
            } else {
                unveil_file(&root, &mut config, "file.txt", None)
            }
        })
    }).collect();
    
    // Should handle gracefully (some may fail, but no panics/corruption)
    for handle in handles {
        let _ = handle.join();
    }
    
    // Verify file still exists and is valid
    assert!(root.join("file.txt").exists());
}
```

### 10. Unicode/Encoding Tests

```rust
#[test]
fn test_veil_unicode_filename() {
    let temp = TempDir::new().unwrap();
    
    fs::write(temp.path().join("файл.txt"), "unicode content").unwrap();
    
    let mut config = Config::new(Mode::Blacklist);
    let result = veil_file(temp.path(), &mut config, "файл.txt", None);
    
    assert!(result.is_ok());
}

#[test]
fn test_veil_unicode_content() {
    let temp = TempDir::new().unwrap();
    
    fs::write(temp.path().join("file.txt"), "Hello 世界 🌍").unwrap();
    
    let mut config = Config::new(Mode::Blacklist);
    veil_file(temp.path(), &mut config, "file.txt", None).unwrap();
    unveil_file(temp.path(), &mut config, "file.txt", None).unwrap();
    
    let content = fs::read_to_string(temp.path().join("file.txt")).unwrap();
    assert_eq!(content, "Hello 世界 🌍");
}
```

## Implementation Steps

### Step 1: Create Test Module (30 min)

```bash
# Create new test file
touch tests/error_handling_test.rs
```

```rust
// tests/error_handling_test.rs
use funveil::*;
use std::fs;
use tempfile::TempDir;

mod corrupted_cas_tests {
    use super::*;
    // ... tests from category 1
}

mod empty_file_tests {
    use super::*;
    // ... tests from category 2
}

// ... etc
```

### Step 2: Add Tests by Category (1 hour each)

1. Corrupted CAS (30 min)
2. Empty files (15 min)
3. Binary files (20 min)
4. Invalid configs (30 min)
5. Invalid patterns (20 min)
6. Permissions (20 min)
7. Protected paths (15 min)
8. Large files (20 min)
9. Concurrent access (30 min)
10. Unicode (15 min)

### Step 3: Add CLI Error Tests (30 min)

```rust
// tests/cli_error_test.rs

#[test]
fn test_cli_veil_nonexistent_file() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "nonexistent.txt"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_cli_unveil_non_veiled_file() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("visible.txt"), "content").unwrap();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "visible.txt"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not veiled"));
}
```

### Step 4: Run and Verify (30 min)

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific category
cargo test corrupted_cas

# Verify coverage
cargo tarpaulin --out Html
```

## Testing Requirements

### Test Categories

- [ ] Corrupted CAS (5 tests)
- [ ] Empty files (2 tests)
- [ ] Binary files (3 tests)
- [ ] Invalid configs (3 tests)
- [ ] Invalid patterns (5 tests)
- [ ] Permissions (2 tests)
- [ ] Protected paths (3 tests)
- [ ] Large files (1 test)
- [ ] Concurrent access (1 test)
- [ ] Unicode (2 tests)
- [ ] CLI errors (5 tests)

**Total**: ~32 new tests

## Acceptance Criteria

- [ ] All 32+ error handling tests written
- [ ] All tests pass
- [ ] Tests cover all error paths
- [ ] Tests use descriptive names
- [ ] Each test has clear assertion messages
- [ ] No panics in library code
- [ ] Error messages are user-friendly
- [ ] Code coverage for error paths > 70%

## Notes

- Some tests may reveal bugs (good!)
- Consider mocking for large file tests
- Skip platform-specific tests on non-Unix
- Document any intentional limitations
- Update SPEC.md with error conditions
