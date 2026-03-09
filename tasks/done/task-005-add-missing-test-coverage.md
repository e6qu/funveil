# Task 005: Add Missing Test Coverage for Core Features

**Priority**: P1 - HIGH  
**Severity**: MEDIUM  
**Type**: Insufficient Testing  
**Estimated Time**: 2-3 hours  
**Dependencies**: None

## Problem Statement

E2E tests don't verify core functionality like veil/unveil round-trips, content integrity, or permission preservation.

## Current State

### Existing E2E Tests (15 total)

```rust
// tests/e2e_smoke_test.rs

test_init_creates_config_and_data_dir
test_default_mode_is_whitelist  
test_mode_can_change_to_blacklist
test_veil_full_file_blacklist_mode
test_unveil_restores_file_content  // Only one that checks content!
test_protected_config_cannot_be_veiled
test_protected_data_dir_cannot_be_veiled
test_doctor_runs_successfully
test_gc_runs_successfully
test_status_shows_whitelisted_files
test_parse_rust_file
test_parse_python_file
test_parse_go_file
test_entrypoints_command
test_trace_command
```

### Issues

1. `test_unveil_restores_file_content` is the ONLY content integrity test
2. No tests for:
   - Round-trip integrity (veil → unveil → verify)
   - Permission preservation
   - Hash verification
   - Partial veil/unveil
   - Whitelist mode workflows
   - Multi-file operations

3. `test_gc_runs_successfully` and `test_doctor_runs_successfully` only check exit code

```rust
#[test]
fn test_doctor_runs_successfully() {
    // ...
    cmd.arg("doctor");
    cmd.assert().success();  // ❌ Doesn't verify actual checks!
}
```

## Missing Test Categories

### 1. Round-Trip Integrity Tests

```rust
#[test]
fn test_veil_unveil_roundtrip_preserves_content() {
    let temp = TempDir::new().unwrap();
    
    // Create test file
    let original = "line1\nline2\nline3\nline4\nline5\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();
    
    // Initialize
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    // Veil
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();
    
    // Verify veiled
    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("..."));
    
    // Unveil
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();
    
    // Verify content restored
    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, original);
}
```

### 2. Partial Veil Round-Trip Tests

```rust
#[test]
fn test_partial_veil_unveil_roundtrip() {
    let temp = TempDir::new().unwrap();
    
    let original = "line1\nline2\nline3\nline4\nline5\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    // Veil lines 2-4
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#2-4", "-q"]);
    cmd.assert().success();
    
    // Verify partial veil
    let veiled = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert!(veiled.contains("line1"));  // Visible
    assert!(veiled.contains("...["));    // Veiled marker
    assert!(veiled.contains("line5"));  // Visible
    
    // Unveil all
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();
    
    // Verify full content restored
    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, original);
}

#[test]
fn test_multiple_partial_veils_then_unveil() {
    let temp = TempDir::new().unwrap();
    
    let original = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
    fs::write(temp.path().join("test.txt"), original).unwrap();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    // Veil non-contiguous ranges
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#2-3", "-q"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#7-8", "-q"]);
    cmd.assert().success();
    
    // Unveil
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();
    
    // Verify content
    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, original);
}
```

### 3. Permission Preservation Tests

```rust
#[test]
#[cfg(unix)]
fn test_veil_unveil_preserves_executable_permission() {
    use std::os::unix::fs::PermissionsExt;
    
    let temp = TempDir::new().unwrap();
    
    // Create executable script
    let path = temp.path().join("script.sh");
    fs::write(&path, "#!/bin/bash\necho hello").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    
    // Initialize and veil
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "script.sh", "-q"]);
    cmd.assert().success();
    
    // Unveil
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "script.sh", "-q"]);
    cmd.assert().success();
    
    // Verify permission preserved
    let metadata = fs::metadata(&path).unwrap();
    let mode = metadata.permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
}

#[test]
#[cfg(unix)]
fn test_veil_unveil_preserves_readonly_permission() {
    use std::os::unix::fs::PermissionsExt;
    
    let temp = TempDir::new().unwrap();
    
    // Create readonly file
    let path = temp.path().join("readonly.txt");
    fs::write(&path, "content").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    
    // Initialize and veil
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "readonly.txt", "-q"]);
    cmd.assert().success();
    
    // Unveil
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "readonly.txt", "-q"]);
    cmd.assert().success();
    
    // Verify permission preserved
    let metadata = fs::metadata(&path).unwrap();
    let mode = metadata.permissions().mode();
    assert_eq!(mode & 0o777, 0o444);
}
```

### 4. Hash Verification Tests

```rust
#[test]
fn test_cas_hash_verification() {
    let temp = TempDir::new().unwrap();
    
    let content = "unique content for hash test";
    fs::write(temp.path().join("test.txt"), content).unwrap();
    
    // Initialize and veil
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();
    
    // Check that object exists in CAS
    let objects_dir = temp.path().join(".funveil/objects");
    assert!(objects_dir.exists());
    
    // Verify object content
    let config_content = fs::read_to_string(temp.path().join(".funveil_config")).unwrap();
    assert!(config_content.contains("hash:"));
    
    // Unveil and verify content matches
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();
    
    let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(restored, content);
}
```

### 5. Whitelist Mode Workflow Tests

```rust
#[test]
fn test_whitelist_mode_hides_by_default() {
    let temp = TempDir::new().unwrap();
    
    fs::write(temp.path().join("public.txt"), "public").unwrap();
    fs::write(temp.path().join("secret.txt"), "secret").unwrap();
    
    // Initialize in whitelist mode
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "whitelist"]);
    cmd.assert().success();
    
    // Unveil only public.txt
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "public.txt", "-q"]);
    cmd.assert().success();
    
    // Check status
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["status"]);
    let output = cmd.assert().success().get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    assert!(stdout.contains("public.txt"));
    // secret.txt should not be in whitelist
}

#[test]
fn test_whitelist_mode_partial_unveil() {
    let temp = TempDir::new().unwrap();
    
    let content = "line1\nline2\nline3\nline4\nline5\n";
    fs::write(temp.path().join("partial.txt"), content).unwrap();
    
    // Initialize in whitelist mode
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "whitelist"]);
    cmd.assert().success();
    
    // Unveil only lines 1-2
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "partial.txt#1-2", "-q"]);
    cmd.assert().success();
    
    // Verify status shows partial unveil
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["status"]);
    let output = cmd.assert().success().get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    assert!(stdout.contains("partial.txt"));
}
```

### 6. Multi-File Operations Tests

```rust
#[test]
fn test_veil_multiple_files() {
    let temp = TempDir::new().unwrap();
    
    fs::write(temp.path().join("a.txt"), "a").unwrap();
    fs::write(temp.path().join("b.txt"), "b").unwrap();
    fs::write(temp.path().join("c.txt"), "c").unwrap();
    
    // Initialize
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    // Veil all three
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "a.txt", "-q"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "b.txt", "-q"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "c.txt", "-q"]);
    cmd.assert().success();
    
    // Unveil all
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "--all", "-q"]);
    cmd.assert().success();
    
    // Verify all restored
    assert_eq!(fs::read_to_string(temp.path().join("a.txt")).unwrap(), "a");
    assert_eq!(fs::read_to_string(temp.path().join("b.txt")).unwrap(), "b");
    assert_eq!(fs::read_to_string(temp.path().join("c.txt")).unwrap(), "c");
}

#[test]
fn test_unveil_all_preserves_order() {
    let temp = TempDir::new().unwrap();
    
    // Create files with different content
    for i in 1..=5 {
        fs::write(temp.path().join(format!("file{}.txt", i)), 
                  format!("content {}", i)).unwrap();
    }
    
    // Initialize and veil all
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    for i in 1..=5 {
        let mut cmd = Command::cargo_bin("fv").unwrap();
        cmd.current_dir(&temp);
        cmd.args(["veil", &format!("file{}.txt", i), "-q"]);
        cmd.assert().success();
    }
    
    // Unveil all at once
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "--all", "-q"]);
    cmd.assert().success();
    
    // Verify all restored correctly
    for i in 1..=5 {
        let content = fs::read_to_string(temp.path().join(format!("file{}.txt", i))).unwrap();
        assert_eq!(content, format!("content {}", i));
    }
}
```

### 7. Doctor Verification Tests

```rust
#[test]
fn test_doctor_detects_missing_object() {
    let temp = TempDir::new().unwrap();
    
    // Create and veil file
    fs::write(temp.path().join("test.txt"), "content").unwrap();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();
    
    // Delete object from CAS
    let objects_dir = temp.path().join(".funveil/objects");
    // Find and delete object file
    for entry in fs::read_dir(&objects_dir).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_dir() {
            for subentry in fs::read_dir(entry.path()).unwrap() {
                let subentry = subentry.unwrap();
                if subentry.path().is_dir() {
                    for file in fs::read_dir(subentry.path()).unwrap() {
                        fs::remove_file(file.unwrap().path()).unwrap();
                    }
                }
            }
        }
    }
    
    // Doctor should detect missing object
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["doctor"]);
    let output = cmd.assert().success().get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    assert!(stdout.contains("issue") || stdout.contains("Missing object"));
}

#[test]
fn test_doctor_passes_when_valid() {
    let temp = TempDir::new().unwrap();
    
    // Create and veil file properly
    fs::write(temp.path().join("test.txt"), "content").unwrap();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();
    
    // Doctor should pass
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["doctor"]);
    let output = cmd.assert().success().get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    assert!(stdout.contains("No issues") || stdout.contains("passed"));
}
```

### 8. GC Verification Tests

```rust
#[test]
fn test_gc_removes_unreferenced_objects() {
    let temp = TempDir::new().unwrap();
    
    // Create and veil file
    fs::write(temp.path().join("test.txt"), "content").unwrap();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();
    
    // Unveil file (object becomes unreferenced)
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();
    
    // Count objects before GC
    let count_before = count_objects(temp.path());
    
    // Run GC
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["gc"]);
    cmd.assert().success();
    
    // Count objects after GC
    let count_after = count_objects(temp.path());
    
    // Should have removed unreferenced objects
    assert!(count_after < count_before);
}

fn count_objects(root: &Path) -> usize {
    let objects_dir = root.join(".funveil/objects");
    let mut count = 0;
    
    if objects_dir.exists() {
        for entry in fs::read_dir(&objects_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                for subentry in fs::read_dir(entry.path()).unwrap() {
                    let subentry = subentry.unwrap();
                    if subentry.path().is_dir() {
                        count += fs::read_dir(subentry.path()).unwrap().count();
                    }
                }
            }
        }
    }
    
    count
}
```

## Implementation Steps

### Step 1: Add Round-Trip Tests (30 min)

Create `tests/integrity_test.rs`:
- `test_veil_unveil_roundtrip_preserves_content`
- `test_partial_veil_unveil_roundtrip`
- `test_multiple_partial_veils_then_unveil`

### Step 2: Add Permission Tests (20 min)

Create `tests/permission_test.rs`:
- `test_veil_unveil_preserves_executable_permission`
- `test_veil_unveil_preserves_readonly_permission`

### Step 3: Add Hash Tests (20 min)

Create `tests/hash_test.rs`:
- `test_cas_hash_verification`

### Step 4: Add Whitelist Tests (20 min)

Create `tests/whitelist_test.rs`:
- `test_whitelist_mode_hides_by_default`
- `test_whitelist_mode_partial_unveil`

### Step 5: Add Multi-File Tests (20 min)

Create `tests/multifile_test.rs`:
- `test_veil_multiple_files`
- `test_unveil_all_preserves_order`

### Step 6: Enhance Doctor/GC Tests (20 min)

Update `tests/e2e_smoke_test.rs`:
- `test_doctor_detects_missing_object`
- `test_doctor_passes_when_valid`
- `test_gc_removes_unreferenced_objects`

## Acceptance Criteria

- [ ] Round-trip integrity tests (3 tests)
- [ ] Permission preservation tests (2 tests)
- [ ] Hash verification test (1 test)
- [ ] Whitelist mode tests (2 tests)
- [ ] Multi-file operation tests (2 tests)
- [ ] Doctor verification tests (2 tests)
- [ ] GC verification test (1 test)
- [ ] All tests pass
- [ ] Tests have descriptive names
- [ ] Tests verify actual functionality, not just exit codes

## Notes

- Tests should be in separate files by category
- Use helper functions for common setup
- Skip platform-specific tests appropriately
- Document any intentional limitations
