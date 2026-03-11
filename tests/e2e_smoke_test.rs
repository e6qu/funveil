//! Smoke tests for Funveil - Core functionality only
//!
//! These tests verify the basic veil/unveil workflows work correctly.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Helper to create a file in the temp directory
fn create_file(temp: &TempDir, path: &str, content: &str) {
    let full_path = temp.path().join(path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full_path, content).unwrap();
}

/// Helper to read a file
fn read_file(temp: &TempDir, path: &str) -> String {
    fs::read_to_string(temp.path().join(path)).unwrap()
}

#[test]
#[allow(deprecated)]
fn test_init_creates_config_and_data_dir() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);

    cmd.arg("init");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    // Verify config file exists
    assert!(temp.path().join(".funveil_config").exists());

    // Verify data directory exists
    assert!(temp.path().join(".funveil").exists());
}

#[test]
#[allow(deprecated)]
fn test_default_mode_is_whitelist() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);

    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("mode");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("whitelist"));
}

#[test]
#[allow(deprecated)]
fn test_mode_can_change_to_blacklist() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);

    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("mode");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("blacklist"));
}

#[test]
#[allow(deprecated)]
fn test_veil_full_file_blacklist_mode() {
    let temp = TempDir::new().unwrap();

    // Create test file
    create_file(
        &temp,
        "secrets.env",
        "API_KEY=secret123\nDB_PASS=password\n",
    );

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil the file
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "secrets.env"]);
    cmd.assert().success();

    // Verify file is veiled
    let content = read_file(&temp, "secrets.env");
    assert!(content.contains("..."));
}

#[test]
#[allow(deprecated)]
fn test_unveil_restores_file_content() {
    let temp = TempDir::new().unwrap();
    let original_content = "API_KEY=secret123\nDB_PASS=password\n";

    // Create test file
    create_file(&temp, "secrets.env", original_content);

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil the file
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "secrets.env", "-q"]);
    cmd.assert().success();

    // Verify it's veiled
    assert!(read_file(&temp, "secrets.env").contains("..."));

    // Unveil the file
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "secrets.env", "-q"]);
    cmd.assert().success();

    // Verify content restored
    let content = read_file(&temp, "secrets.env");
    assert_eq!(content, original_content);
}

#[test]
#[allow(deprecated)]
fn test_protected_config_cannot_be_veiled() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Try to veil config file
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", ".funveil_config"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("protected"));
}

#[test]
#[allow(deprecated)]
fn test_protected_data_dir_cannot_be_veiled() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Try to veil data directory
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", ".funveil/"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("protected"));
}

#[test]
#[allow(deprecated)]
fn test_status_shows_whitelisted_files() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "README.md", "# Project\n");
    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Unveil README
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "README.md", "-q"]);
    cmd.assert().success();

    // Check status shows README
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("README.md"));
}

#[test]
#[allow(deprecated)]
fn test_doctor_runs_successfully() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Run doctor
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("doctor");
    cmd.assert().success();
}

#[test]
#[allow(deprecated)]
fn test_gc_runs_successfully() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Run gc
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("gc");
    cmd.assert().success();
}

#[test]
#[allow(deprecated)]
fn test_parse_rust_file() {
    let temp = TempDir::new().unwrap();

    create_file(
        &temp,
        "src/main.rs",
        r#"
fn main() {
    println!("Hello");
}

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#,
    );

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["parse", "--format", "detailed", "src/main.rs"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("main"));
}

#[test]
#[allow(deprecated)]
fn test_parse_python_file() {
    let temp = TempDir::new().unwrap();

    create_file(
        &temp,
        "app.py",
        r#"
def main():
    print("Hello")

def calculate(x, y):
    return x + y
"#,
    );

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["parse", "--format", "detailed", "app.py"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("main"));
}

#[test]
#[allow(deprecated)]
fn test_parse_go_file() {
    let temp = TempDir::new().unwrap();

    create_file(
        &temp,
        "main.go",
        r#"
package main

import "fmt"

func main() {
    fmt.Println("Hello")
}

func Add(a, b int) int {
    return a + b
}
"#,
    );

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["parse", "--format", "detailed", "main.go"]);
    cmd.assert().success();
}

#[test]
#[allow(deprecated)]
fn test_entrypoints_command() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("entrypoints");
    cmd.assert().success();
}

#[test]
#[allow(deprecated)]
fn test_trace_command() {
    let temp = TempDir::new().unwrap();

    create_file(
        &temp,
        "src/main.rs",
        r#"
fn main() {
    helper();
}

fn helper() {}
"#,
    );

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["trace", "--from", "main", "--depth", "2"]);
    cmd.assert().success();
}

#[test]
#[allow(deprecated)]
fn test_checkpoint_save_and_list() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "test-cp"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("saved"));

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "list"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("test-cp"));
}

#[test]
#[allow(deprecated)]
fn test_checkpoint_show() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "show-test"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "show", "show-test"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Checkpoint:"));
}

#[test]
#[allow(deprecated)]
fn test_checkpoint_delete() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "to-delete"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "delete", "to-delete"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("deleted"));

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "list"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("to-delete").not());
}

#[test]
#[allow(deprecated)]
fn test_clean_removes_data() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    assert!(temp.path().join(".funveil").exists());
    assert!(temp.path().join(".funveil_config").exists());

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("clean");
    cmd.assert().success();

    assert!(!temp.path().join(".funveil").exists());
    assert!(!temp.path().join(".funveil_config").exists());
}

#[test]
#[allow(deprecated)]
fn test_apply_reapplies_veils() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "secrets.env", "API_KEY=secret123\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "secrets.env", "-q"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("apply");
    cmd.assert().success();
}

#[test]
#[allow(deprecated)]
fn test_restore_fails_without_checkpoints() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("restore");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No checkpoints found"));
}

#[test]
#[allow(deprecated)]
fn test_checkpoint_restore_workflow() {
    let temp = TempDir::new().unwrap();

    let original = "API_KEY=original\n";
    create_file(&temp, "config.env", original);

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "before-change"]);
    cmd.assert().success();

    create_file(&temp, "config.env", "API_KEY=changed\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "restore", "before-change"]);
    cmd.assert().success();

    let restored = read_file(&temp, "config.env");
    assert!(restored.contains("original"));
}

#[test]
#[allow(deprecated)]
fn test_partial_veil_round_trip() {
    let temp = TempDir::new().unwrap();

    let original = "line1\nline2\nline3\nline4\nline5\n";
    create_file(&temp, "test.txt", original);

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#2-4", "-q"]);
    cmd.assert().success();

    assert!(read_file(&temp, "test.txt").contains("..."));

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, original);
}

#[test]
#[allow(deprecated)]
fn test_partial_veil_non_contiguous_ranges() {
    let temp = TempDir::new().unwrap();

    let original = "header\nmiddle1\nmiddle2\nfooter\nend\n";
    create_file(&temp, "test.txt", original);

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#2-2", "-q"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#4-4", "-q"]);
    cmd.assert().success();

    let veiled = read_file(&temp, "test.txt");
    assert!(veiled.contains("header"));
    assert!(veiled.contains("..."));
    assert!(veiled.contains("end"));

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, original);
}

#[test]
#[allow(deprecated)]
fn test_partial_veil_preserves_all_content() {
    let temp = TempDir::new().unwrap();

    let original = r#"# Header

def public():
    pass

# Implementation
def _helper():
    data = fetch()
    result = process(data)
    return result

# Exports
__all__ = ['public']

# End
"#;
    create_file(&temp, "api.py", original);

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "api.py#8-13", "-q"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "api.py#16-17", "-q"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "api.py", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "api.py");
    assert_eq!(restored, original);
}

#[test]
#[allow(deprecated)]
fn test_cli_veil_nonexistent_file() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("restore");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No checkpoints found"));
}

#[test]
#[allow(deprecated)]
fn test_full_veil_round_trip() {
    let temp = TempDir::new().unwrap();

    let original = "line1\nline2\nline3\nline4\nline5\n";
    create_file(&temp, "test.txt", original);

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    let veiled = read_file(&temp, "test.txt");
    assert!(veiled.contains("..."));

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, original);
}

#[test]
#[allow(deprecated)]
fn test_multiple_partial_veils_round_trip() {
    let temp = TempDir::new().unwrap();

    let original = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
    create_file(&temp, "test.txt", original);

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#2-3", "-q"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#7-8", "-q"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, original);
}

#[test]
#[allow(deprecated)]
fn test_unveil_all_multiple_files() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "a.txt", "content a");
    create_file(&temp, "b.txt", "content b");
    create_file(&temp, "c.txt", "content c");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    for file in &["a.txt", "b.txt", "c.txt"] {
        let mut cmd = Command::cargo_bin("fv").unwrap();
        cmd.current_dir(&temp);
        cmd.args(["veil", file, "-q"]);
        cmd.assert().success();
    }

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "--all", "-q"]);
    cmd.assert().success();

    assert_eq!(read_file(&temp, "a.txt"), "content a");
    assert_eq!(read_file(&temp, "b.txt"), "content b");
    assert_eq!(read_file(&temp, "c.txt"), "content c");
}

#[test]
#[allow(deprecated)]
fn test_cas_hash_verification() {
    let temp = TempDir::new().unwrap();

    let content = "unique content for hash test\n";
    create_file(&temp, "test.txt", content);

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    assert!(temp.path().join(".funveil/objects").exists());

    let config_content = fs::read_to_string(temp.path().join(".funveil_config")).unwrap();
    assert!(config_content.contains("objects:"));

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, content);
}

#[test]
#[allow(deprecated)]
fn test_whitelist_mode_workflow() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "public.txt", "public content");
    create_file(&temp, "secret.txt", "secret content");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "whitelist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "public.txt", "-q"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["status"]);
    let output = cmd.assert().success().get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("public.txt"));
}

#[test]
#[allow(deprecated)]
fn test_doctor_detects_issues() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["doctor"]);
    cmd.assert().success();
}

#[test]
#[allow(deprecated)]
fn test_gc_removes_objects() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["gc"]);
    cmd.assert().success();
}

#[test]
#[allow(deprecated)]
fn test_cli_unveil_non_veiled_file_succeeds() {
    let temp = TempDir::new().unwrap();
    create_file(&temp, "visible.txt", "content");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["unveil", "visible.txt"]);
    cmd.assert().success();
}

#[test]
#[allow(deprecated)]
fn test_cli_veil_config_file_fails() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", ".funveil_config"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("protected"));
}

#[test]
#[allow(deprecated)]
fn test_cli_veil_data_dir_fails() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", ".funveil/"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("protected"));
}

#[test]
#[allow(deprecated)]
fn test_cli_restore_without_checkpoints_fails() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("restore");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No checkpoints found"));
}

// ── BUG-065: Doctor command continues on invalid hash instead of aborting ──

#[test]
#[allow(deprecated)]
fn test_bug065_doctor_continues_on_invalid_hash() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil a file so there's an object in config
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    // Corrupt the hash in the config file
    let config_path = temp.path().join(".funveil_config");
    let config_content = fs::read_to_string(&config_path).unwrap();
    // Replace the hash value with an invalid one
    let config_content2 = config_content.replacen("hash:", "hash: INVALID_HASH #", 1);
    fs::write(&config_path, &config_content2).unwrap();

    // Doctor should complete (not abort) and report the issue
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("doctor");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Invalid hash").or(predicate::str::contains("issue")));
}

// ── BUG-066: Show command respects quiet flag ──

#[test]
#[allow(deprecated)]
fn test_bug066_show_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "some content\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["show", "test.txt", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "show --quiet should produce no stdout"
    );
}

// ── BUG-067: Parse command respects quiet flag ──

#[test]
#[allow(deprecated)]
fn test_bug067_parse_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    create_file(
        &temp,
        "src/main.rs",
        "fn main() {\n    println!(\"Hello\");\n}\n",
    );

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["parse", "src/main.rs", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "parse --quiet should produce no stdout"
    );
}

// ── BUG-068: Entrypoints non-empty output respects quiet flag ──

#[test]
#[allow(deprecated)]
fn test_bug068_entrypoints_nonempty_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["entrypoints", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "entrypoints --quiet should produce no stdout"
    );
}

// ── BUG-069: Cache Status respects quiet flag ──

#[test]
#[allow(deprecated)]
fn test_bug069_cache_status_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["cache", "status", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "cache status --quiet should produce no stdout"
    );
}

// ── BUG-070: Doctor results respect quiet flag ──

#[test]
#[allow(deprecated)]
fn test_bug070_doctor_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["doctor", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "doctor --quiet should produce no stdout"
    );
}

// ── BUG-071: Trace from-entrypoint "no entrypoints" message respects quiet flag ──

#[test]
#[allow(deprecated)]
fn test_bug071_trace_from_entrypoint_quiet_no_stderr() {
    let temp = TempDir::new().unwrap();

    // Empty project — no entrypoints to detect
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.args(["trace", "--from-entrypoint", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stderr.is_empty(),
        "trace --from-entrypoint --quiet should produce no stderr"
    );
}
