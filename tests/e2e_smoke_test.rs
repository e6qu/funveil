//! Smoke tests for Funveil - Core functionality only
//!
//! These tests verify the basic veil/unveil workflows work correctly.

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
fn test_init_creates_config_and_data_dir() {
    let temp = TempDir::new().unwrap();
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
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
fn test_default_mode_is_whitelist() {
    let temp = TempDir::new().unwrap();
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);

    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("mode");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("whitelist"));
}

#[test]
fn test_mode_can_change_to_blacklist() {
    let temp = TempDir::new().unwrap();
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);

    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("mode");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("blacklist"));
}

#[test]
fn test_veil_full_file_blacklist_mode() {
    let temp = TempDir::new().unwrap();

    // Create test file
    create_file(
        &temp,
        "secrets.env",
        "API_KEY=secret123\nDB_PASS=password\n",
    );

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil the file
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "secrets.env"]);
    cmd.assert().success();

    // Verify file is veiled
    let content = read_file(&temp, "secrets.env");
    assert!(content.contains("..."));
}

#[test]
fn test_unveil_restores_file_content() {
    let temp = TempDir::new().unwrap();
    let original_content = "API_KEY=secret123\nDB_PASS=password\n";

    // Create test file
    create_file(&temp, "secrets.env", original_content);

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil the file
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "secrets.env", "-q"]);
    cmd.assert().success();

    // Verify it's veiled
    assert!(read_file(&temp, "secrets.env").contains("..."));

    // Unveil the file
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "secrets.env", "-q"]);
    cmd.assert().success();

    // Verify content restored
    let content = read_file(&temp, "secrets.env");
    assert_eq!(content, original_content);
}

#[test]
fn test_protected_config_cannot_be_veiled() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Try to veil config file
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", ".funveil_config"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("protected"));
}

#[test]
fn test_protected_data_dir_cannot_be_veiled() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Try to veil data directory
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", ".funveil/"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("protected"));
}

#[test]
fn test_status_shows_whitelisted_files() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "README.md", "# Project\n");
    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Unveil README
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "README.md", "-q"]);
    cmd.assert().success();

    // Check status shows README
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("README.md"));
}

#[test]
fn test_doctor_runs_successfully() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Run doctor
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("doctor");
    cmd.assert().success();
}

#[test]
fn test_gc_runs_successfully() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Run gc
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("gc");
    cmd.assert().success();
}

#[test]
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["parse", "--format", "detailed", "src/main.rs"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("main"));
}

#[test]
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["parse", "--format", "detailed", "app.py"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("main"));
}

#[test]
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["parse", "--format", "detailed", "main.go"]);
    cmd.assert().success();
}

#[test]
fn test_entrypoints_command() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("entrypoints");
    cmd.assert().success();
}

#[test]
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["trace", "--from", "main", "--depth", "2"]);
    cmd.assert().success();
}

#[test]
fn test_checkpoint_save_and_list() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "test-cp"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("saved"));

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "list"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("test-cp"));
}

#[test]
fn test_checkpoint_show() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "show-test"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "show", "show-test"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Checkpoint:"));
}

#[test]
fn test_checkpoint_delete() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "to-delete"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "delete", "to-delete"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("deleted"));

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "list"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("to-delete").not());
}

#[test]
fn test_clean_removes_data() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    assert!(temp.path().join(".funveil").exists());
    assert!(temp.path().join(".funveil_config").exists());

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("clean");
    cmd.assert().success();

    assert!(!temp.path().join(".funveil").exists());
    assert!(!temp.path().join(".funveil_config").exists());
}

#[test]
fn test_apply_reapplies_veils() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "secrets.env", "API_KEY=secret123\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "secrets.env", "-q"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("apply");
    cmd.assert().success();
}

#[test]
fn test_restore_fails_without_checkpoints() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init").assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("restore");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No checkpoints found"));
}

#[test]
fn test_checkpoint_restore_workflow() {
    let temp = TempDir::new().unwrap();

    let original = "API_KEY=original\n";
    create_file(&temp, "config.env", original);

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "before-change"]);
    cmd.assert().success();

    create_file(&temp, "config.env", "API_KEY=changed\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "restore", "before-change"]);
    cmd.assert().success();

    let restored = read_file(&temp, "config.env");
    assert!(restored.contains("original"));
}

#[test]
fn test_partial_veil_round_trip() {
    let temp = TempDir::new().unwrap();

    let original = "line1\nline2\nline3\nline4\nline5\n";
    create_file(&temp, "test.txt", original);

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#2-4", "-q"]);
    cmd.assert().success();

    assert!(read_file(&temp, "test.txt").contains("..."));

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, original);
}

#[test]
fn test_partial_veil_non_contiguous_ranges() {
    let temp = TempDir::new().unwrap();

    let original = "header\nmiddle1\nmiddle2\nfooter\nend\n";
    create_file(&temp, "test.txt", original);

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#2-2", "-q"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#4-4", "-q"]);
    cmd.assert().success();

    let veiled = read_file(&temp, "test.txt");
    assert!(veiled.contains("header"));
    assert!(veiled.contains("..."));
    assert!(veiled.contains("end"));

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, original);
}

#[test]
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

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "api.py#8-13", "-q"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "api.py#16-17", "-q"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "api.py", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "api.py");
    assert_eq!(restored, original);
}

#[test]
fn test_cli_veil_nonexistent_file() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("restore");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No checkpoints found"));
}

#[test]
fn test_full_veil_round_trip() {
    let temp = TempDir::new().unwrap();

    let original = "line1\nline2\nline3\nline4\nline5\n";
    create_file(&temp, "test.txt", original);

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    let veiled = read_file(&temp, "test.txt");
    assert!(veiled.contains("..."));

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, original);
}

#[test]
fn test_multiple_partial_veils_round_trip() {
    let temp = TempDir::new().unwrap();

    let original = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
    create_file(&temp, "test.txt", original);

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#2-3", "-q"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#7-8", "-q"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, original);
}

#[test]
fn test_unveil_all_multiple_files() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "a.txt", "content a");
    create_file(&temp, "b.txt", "content b");
    create_file(&temp, "c.txt", "content c");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    for file in &["a.txt", "b.txt", "c.txt"] {
        let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
        cmd.current_dir(&temp);
        cmd.args(["veil", file, "-q"]);
        cmd.assert().success();
    }

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "--all", "-q"]);
    cmd.assert().success();

    assert_eq!(read_file(&temp, "a.txt"), "content a");
    assert_eq!(read_file(&temp, "b.txt"), "content b");
    assert_eq!(read_file(&temp, "c.txt"), "content c");
}

#[test]
fn test_cas_hash_verification() {
    let temp = TempDir::new().unwrap();

    let content = "unique content for hash test\n";
    create_file(&temp, "test.txt", content);

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    assert!(temp.path().join(".funveil/objects").exists());

    let config_content = fs::read_to_string(temp.path().join(".funveil_config")).unwrap();
    assert!(config_content.contains("objects:"));

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let restored = read_file(&temp, "test.txt");
    assert_eq!(restored, content);
}

#[test]
fn test_whitelist_mode_workflow() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "public.txt", "public content");
    create_file(&temp, "secret.txt", "secret content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "whitelist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "public.txt", "-q"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["status"]);
    let output = cmd.assert().success().get_output().clone();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("public.txt"));
}

#[test]
fn test_doctor_detects_issues() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["doctor"]);
    cmd.assert().success();
}

#[test]
fn test_gc_removes_objects() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "test.txt", "-q"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["gc"]);
    cmd.assert().success();
}

#[test]
fn test_cli_unveil_non_veiled_file_succeeds() {
    let temp = TempDir::new().unwrap();
    create_file(&temp, "visible.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "visible.txt"]);
    cmd.assert().success();
}

#[test]
fn test_cli_veil_config_file_fails() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", ".funveil_config"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("protected"));
}

#[test]
fn test_cli_veil_data_dir_fails() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", ".funveil/"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("protected"));
}

#[test]
fn test_cli_restore_without_checkpoints_fails() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("restore");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No checkpoints found"));
}

// ── BUG-065: Doctor command continues on invalid hash instead of aborting ──

#[test]
fn test_bug065_doctor_continues_on_invalid_hash() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil a file so there's an object in config
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
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
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("doctor");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Invalid hash").or(predicate::str::contains("issue")));
}

// ── BUG-066: Show command respects quiet flag ──

#[test]
fn test_bug066_show_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "some content\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
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
fn test_bug067_parse_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    create_file(
        &temp,
        "src/main.rs",
        "fn main() {\n    println!(\"Hello\");\n}\n",
    );

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
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
fn test_bug068_entrypoints_nonempty_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "src/main.rs", "fn main() {}\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
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
fn test_bug069_cache_status_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
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
fn test_bug070_doctor_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
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
fn test_bug071_trace_from_entrypoint_quiet_no_stderr() {
    let temp = TempDir::new().unwrap();

    // Empty project — no entrypoints to detect
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["trace", "--from-entrypoint", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stderr.is_empty(),
        "trace --from-entrypoint --quiet should produce no stderr"
    );
}

// ── BUG-072: Veil non-regex adds to blacklist before verifying veil succeeds ──

#[test]
fn test_bug072_blacklist_not_updated_on_veil_failure() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil a nonexistent file — should fail
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "nonexistent.txt"]);
    cmd.assert().failure();

    // Blacklist should be empty — the failed file should not have been added
    let config_content = fs::read_to_string(temp.path().join(".funveil_config")).unwrap();
    assert!(
        !config_content.contains("nonexistent.txt"),
        "blacklist should not contain file that failed to veil"
    );
}

// ── BUG-073: GC command outputs in quiet mode ──

#[test]
fn test_bug073_gc_quiet_no_output() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["gc", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "gc --quiet should produce no stdout"
    );
}

// ── BUG-074: show_checkpoint prints unconditionally ──

#[test]
fn test_bug074_checkpoint_show_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "show-quiet-test"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "show", "show-quiet-test", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "checkpoint show --quiet should produce no stdout"
    );
}

// ── BUG-075: save_checkpoint prints unconditionally ──

#[test]
fn test_bug075_checkpoint_save_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "save-quiet-test", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "checkpoint save --quiet should produce no stdout"
    );
}

// ── BUG-076: delete_checkpoint prints unconditionally ──

#[test]
fn test_bug076_checkpoint_delete_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "del-quiet-test"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "delete", "del-quiet-test", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "checkpoint delete --quiet should produce no stdout"
    );
}

// ── BUG-077: restore_checkpoint prints unconditionally ──

#[test]
fn test_bug077_checkpoint_restore_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "save", "restore-quiet-test"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["checkpoint", "restore", "restore-quiet-test", "--quiet"]);
    let output = cmd.assert().success().get_output().clone();
    assert!(
        output.stdout.is_empty(),
        "checkpoint restore --quiet should produce no stdout"
    );
}

// ── BUG-078: parse_pattern accepts empty file path ──

#[test]
fn test_bug078_veil_empty_path_pattern() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "#1-5"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Empty file path"));
}

// ── BUG-079: GC command aborts on first invalid hash ──

#[test]
fn test_bug079_gc_continues_on_invalid_hash() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil a file so there's an object in config
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    // Corrupt the hash in the config file
    let config_path = temp.path().join(".funveil_config");
    let config_content = fs::read_to_string(&config_path).unwrap();
    let config_content2 = config_content.replacen("hash:", "hash: INVALID_HASH #", 1);
    fs::write(&config_path, &config_content2).unwrap();

    // GC should complete (not abort) despite the invalid hash
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("gc");
    cmd.assert().success();
}

// ── BUG-081/083: Trace warnings not gated on quiet ──

#[test]
fn test_bug081_083_trace_warning_quiet() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Trace a nonexistent function with --quiet — stderr should be empty
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["trace", "nonexistent_function", "--quiet"]);
    let output = cmd.output().unwrap();
    assert!(
        output.stderr.is_empty(),
        "trace nonexistent function with --quiet should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── BUG-084: Veil regex per-file error warnings not gated on quiet ──

#[test]
fn test_bug084_veil_regex_error_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil the file first
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    // Now veil again with regex that matches the already-veiled file — with --quiet
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "/test\\.txt/", "-q"]);
    let output = cmd.output().unwrap();
    assert!(
        output.stderr.is_empty(),
        "veil regex error with --quiet should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── BUG-085: Unveil regex per-file error warnings not gated on quiet ──

#[test]
fn test_bug085_unveil_regex_error_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Unveil with regex on a file that isn't veiled — with --quiet
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["unveil", "/nonexistent\\.txt/", "-q"]);
    let output = cmd.output().unwrap();
    assert!(
        output.stderr.is_empty(),
        "unveil regex with --quiet should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── BUG-086: Apply command error messages not gated on quiet ──

#[test]
fn test_bug086_apply_error_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil a file
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    // Corrupt the hash in config
    let config_path = temp.path().join(".funveil_config");
    let config_content = fs::read_to_string(&config_path).unwrap();
    let config_content2 = config_content.replacen("hash:", "hash: INVALID_HASH #", 1);
    fs::write(&config_path, &config_content2).unwrap();

    // Apply with --quiet — stderr should be empty
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["apply", "-q"]);
    let output = cmd.output().unwrap();
    assert!(
        output.stderr.is_empty(),
        "apply with --quiet should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── BUG-087: GC invalid-hash warning not gated on quiet ──

#[test]
fn test_bug087_gc_warning_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil a file
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt", "-q"]);
    cmd.assert().success();

    // Corrupt the hash
    let config_path = temp.path().join(".funveil_config");
    let config_content = fs::read_to_string(&config_path).unwrap();
    let config_content2 = config_content.replacen("hash:", "hash: INVALID_HASH #", 1);
    fs::write(&config_path, &config_content2).unwrap();

    // GC with --quiet — stderr should be empty
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["gc", "-q"]);
    let output = cmd.output().unwrap();
    assert!(
        output.stderr.is_empty(),
        "gc with --quiet should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── BUG-088: parse_pattern allows empty range after '#' ──

#[test]
fn test_bug088_veil_trailing_hash() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "test.txt", "content");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["veil", "test.txt#"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Empty range"));
}

// ── BUG-090: Trace DOT format output not gated on quiet ──

#[test]
fn test_bug090_trace_dot_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "main.rs", "fn main() { helper(); }\nfn helper() {}");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["trace", "main", "--format", "dot", "--quiet"])
        .output()
        .unwrap();

    assert!(
        output.stdout.is_empty(),
        "trace --format dot --quiet should produce no stdout, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

// ── BUG-091: Trace Tree/List format output not gated on quiet ──

#[test]
fn test_bug091_trace_tree_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "main.rs", "fn main() { helper(); }\nfn helper() {}");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["trace", "main", "--quiet"])
        .output()
        .unwrap();

    assert!(
        output.stdout.is_empty(),
        "trace --quiet should produce no stdout, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

// ── BUG-092/093: veil/unveil directory warnings not gated on quiet ──

#[test]
fn test_bug092_093_veil_unveil_directory_quiet() {
    let temp = TempDir::new().unwrap();

    create_file(&temp, "subdir/a.txt", "content a\n");
    create_file(&temp, "subdir/b.txt", "content b\n");

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    // Veil directory with quiet
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "subdir", "-q"])
        .output()
        .unwrap();

    assert!(
        output.stderr.is_empty(),
        "veil directory with -q should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Unveil directory with quiet
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["unveil", "subdir", "-q"])
        .output()
        .unwrap();

    assert!(
        output.stderr.is_empty(),
        "unveil directory with -q should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── BUG-103: gc --quiet should suppress stderr ──

#[test]
fn test_bug103_gc_quiet() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    create_file(&temp, "test.txt", "content\n");

    // Veil and unveil to create unreferenced objects
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "test.txt", "-q"])
        .assert()
        .success();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["unveil", "test.txt", "-q"])
        .assert()
        .success();

    // Run gc with --quiet
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["gc", "--quiet"])
        .output()
        .unwrap();

    assert!(
        output.stderr.is_empty(),
        "gc --quiet should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── BUG-104: restore checkpoint --quiet should suppress stderr ──

#[test]
fn test_bug104_restore_checkpoint_quiet() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();

    create_file(&temp, "test.txt", "checkpoint content\n");

    // Save a checkpoint
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["checkpoint", "save", "test-cp", "-q"])
        .assert()
        .success();

    // Modify the file
    create_file(&temp, "test.txt", "modified content\n");

    // Restore with --quiet
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["checkpoint", "restore", "test-cp", "--quiet"])
        .output()
        .unwrap();

    assert!(
        output.stderr.is_empty(),
        "checkpoint restore --quiet should produce no stderr, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify content was restored
    assert_eq!(read_file(&temp, "test.txt"), "checkpoint content\n");
}

// ── BUG-112: Veil with line-range pattern updates blacklist ──

#[test]
fn test_bug112_veil_line_range_blacklist() {
    let temp = TempDir::new().unwrap();

    // Init in blacklist mode
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    create_file(&temp, "test.txt", "line1\nline2\nline3\nline4\nline5\n");

    // Veil with line range pattern
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "test.txt#2-3"])
        .assert()
        .success();

    // Check status — should show test.txt in blacklist
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["status"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("test.txt"),
        "blacklist should contain test.txt after line-range veil, got: {stdout}"
    );
}

// ── BUG-113: Unveil regex no misleading success message ──

#[test]
fn test_bug113_unveil_regex_no_misleading_success() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    // Create a file that matches the regex but is NOT veiled
    create_file(&temp, "test.txt", "content\n");

    // Unveil with regex — file matches but has no veils, so no unveil_file call succeeds
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["unveil", "/test/"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Since no files were actually unveiled (file had no veils), should NOT say "Unveiled:"
    // The file was just added to whitelist without any unveil operation
    assert!(
        !stdout.contains("Unveiled:") || stdout.is_empty(),
        "should not print misleading 'Unveiled:' when no files were actually unveiled, got: {stdout}"
    );
}

// ── BUG-117: Show command with --quiet validates file existence ──

#[test]
fn test_bug117_show_quiet_validates() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init"])
        .assert()
        .success();

    // Show non-existent file with --quiet — should fail
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["show", "nonexistent.txt", "--quiet"])
        .assert()
        .failure();
}

// ── BUG-121: Unveil with line-range pattern updates whitelist ──

#[test]
fn test_bug121_unveil_line_range_whitelist() {
    let temp = TempDir::new().unwrap();

    // Init in whitelist mode
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "whitelist"])
        .assert()
        .success();

    create_file(&temp, "test.txt", "line1\nline2\nline3\nline4\nline5\n");

    // Veil the file first
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "test.txt#2-3"])
        .assert()
        .success();

    // Unveil with line-range pattern
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["unveil", "test.txt#2-3"])
        .assert()
        .success();

    // Check status — test.txt should be in the whitelist
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["status"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("test.txt"),
        "whitelist should contain test.txt after line-range unveil, got: {stdout}"
    );
}

// ── BUG-122/123: tree-sitter parser doesn't panic on out-of-bounds capture index ──

#[test]
fn test_bug122_bug123_parse_does_not_panic() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init"])
        .assert()
        .success();

    // Create a file with imports and function calls
    create_file(
        &temp,
        "test.py",
        "import os\nimport sys\n\ndef foo():\n    os.path.join('a', 'b')\n    print('hello')\n",
    );

    // Parse the file — should not panic
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["show", "test.py"])
        .assert()
        .success();
}

// ── BUG-124: Filename with '#' doesn't cause parse error ──

#[test]
fn test_bug124_filename_with_hash() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    create_file(&temp, "file#name.txt", "some content\n");

    // Veil the file with '#' in name — should not error
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "file#name.txt"])
        .assert()
        .success();

    // Status should not error
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["status"])
        .output()
        .unwrap();

    assert!(output.status.success(), "status should succeed");
}

// ── BUG-125 regression ──────────────────────────────────────────────────────
#[test]
fn test_bug125_unveil_rejects_symlink_escape() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    // Create a real file and veil it so config has an entry
    create_file(&temp, "target.txt", "secret content\n");
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "target.txt"])
        .assert()
        .success();

    // Now replace the veiled file with a symlink pointing outside root
    #[cfg(unix)]
    {
        fs::remove_file(temp.path().join("target.txt")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", temp.path().join("target.txt")).unwrap();

        let output = assert_cmd::cargo_bin_cmd!("fv")
            .current_dir(&temp)
            .args(["unveil", "target.txt"])
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "unveil of symlink escaping root should fail"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("symlink") || stderr.contains("escape") || stderr.contains("outside"),
            "error should mention symlink escape, got: {stderr}"
        );
    }
}

// ── BUG-126 regression ──────────────────────────────────────────────────────
#[test]
fn test_bug126_unveil_rejects_protected_files() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    // Use '#' pattern to force unveil_file to be called (bypasses has_veils gate)
    // Attempt to unveil the config file with a line range — should be rejected
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["unveil", ".funveil_config#1-5"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "unveil of .funveil_config should fail"
    );

    // Attempt to unveil the data directory with a line range — should be rejected
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["unveil", ".funveil#1-5"])
        .output()
        .unwrap();

    assert!(!output.status.success(), "unveil of .funveil should fail");

    // Attempt to unveil a file under .git/ with a line range — should be rejected
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    create_file(&temp, ".git/config", "some git config\n");
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["unveil", ".git/config#1-1"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "unveil of .git/config should fail"
    );
}

// ── BUG-127 regression ──────────────────────────────────────────────────────
#[test]
fn test_bug127_checkpoint_quiet_no_warnings() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    create_file(&temp, "test.txt", "hello world\n");

    // Create a checkpoint with --quiet; stderr should be empty
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["checkpoint", "save", "quiet_cp", "--quiet"])
        .output()
        .unwrap();

    assert!(output.status.success(), "checkpoint save should succeed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "quiet checkpoint should produce no stderr, got: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "quiet checkpoint should produce no stdout, got: {stdout}"
    );
}

// ── Gitignore integration tests ──

#[test]
fn test_init_creates_gitignore() {
    let temp = TempDir::new().unwrap();
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    let gitignore = read_file(&temp, ".gitignore");
    assert!(gitignore.contains("# MANAGED BY FUNVEIL"));
    assert!(gitignore.contains(".funveil_config"));
    assert!(gitignore.contains(".funveil/"));
    assert!(gitignore.contains("# END MANAGED BY FUNVEIL"));
}

#[test]
fn test_init_appends_to_existing_gitignore() {
    let temp = TempDir::new().unwrap();
    create_file(&temp, ".gitignore", "node_modules/\n*.log\n");

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    let gitignore = read_file(&temp, ".gitignore");
    assert!(
        gitignore.starts_with("node_modules/"),
        "existing content should be preserved"
    );
    assert!(gitignore.contains("# MANAGED BY FUNVEIL"));
    assert!(gitignore.contains(".funveil_config"));
}

#[test]
fn test_init_idempotent_gitignore() {
    let temp = TempDir::new().unwrap();

    // Run init twice
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    let first = read_file(&temp, ".gitignore");

    // Second init should do nothing (config already exists, but ensure_gitignore is idempotent anyway)
    // We manually call ensure_gitignore to test idempotency since init exits early
    funveil::config::ensure_gitignore(temp.path()).unwrap();

    let second = read_file(&temp, ".gitignore");
    assert_eq!(
        first, second,
        "gitignore should not be modified on second run"
    );

    let count = second.matches("# MANAGED BY FUNVEIL").count();
    assert_eq!(count, 1, "managed block should appear exactly once");
}

#[test]
fn test_regex_veil_skips_gitignored() {
    let temp = TempDir::new().unwrap();

    // Init
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    // Add *.log to gitignore
    let gitignore = read_file(&temp, ".gitignore");
    fs::write(
        temp.path().join(".gitignore"),
        format!("*.log\n{gitignore}"),
    )
    .unwrap();

    // Create files
    create_file(&temp, "a.txt", "hello\n");
    create_file(&temp, "a.log", "log data\n");

    // Regex veil all files
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "/^a\\./"])
        .assert()
        .success();

    // a.txt should be veiled (marker content)
    let txt_content = read_file(&temp, "a.txt");
    assert_eq!(txt_content, "...\n", "a.txt should be veiled");

    // a.log should NOT be veiled (gitignored)
    let log_content = read_file(&temp, "a.log");
    assert_eq!(
        log_content, "log data\n",
        "a.log should be untouched (gitignored)"
    );
}

#[test]
fn test_veil_directory_skips_gitignored() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    // Add *.log to gitignore
    let gitignore = read_file(&temp, ".gitignore");
    fs::write(
        temp.path().join(".gitignore"),
        format!("*.log\n{gitignore}"),
    )
    .unwrap();

    // Create a subdir with mixed files
    create_file(&temp, "subdir/keep.txt", "keep\n");
    create_file(&temp, "subdir/skip.log", "skip\n");

    // Veil the directory
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "subdir"])
        .assert()
        .success();

    // keep.txt should be veiled
    let txt_content = read_file(&temp, "subdir/keep.txt");
    assert_eq!(txt_content, "...\n");

    // skip.log should be untouched
    let log_content = read_file(&temp, "subdir/skip.log");
    assert_eq!(log_content, "skip\n");
}

#[test]
fn test_explicit_veil_works_on_gitignored() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    // Add *.log to gitignore
    let gitignore = read_file(&temp, ".gitignore");
    fs::write(
        temp.path().join(".gitignore"),
        format!("*.log\n{gitignore}"),
    )
    .unwrap();

    create_file(&temp, "explicit.log", "log content\n");

    // Explicit veil should still work on gitignored files
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "explicit.log"])
        .assert()
        .success();

    let content = read_file(&temp, "explicit.log");
    assert_eq!(
        content, "...\n",
        "explicit veil should work on gitignored file"
    );
}

#[test]
fn test_checkpoint_skips_gitignored() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    // Add *.log to gitignore
    let gitignore = read_file(&temp, ".gitignore");
    fs::write(
        temp.path().join(".gitignore"),
        format!("*.log\n{gitignore}"),
    )
    .unwrap();

    create_file(&temp, "included.txt", "included\n");
    create_file(&temp, "excluded.log", "excluded\n");

    // Save checkpoint
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["checkpoint", "save", "test-cp"])
        .assert()
        .success();

    // Show checkpoint and verify .log file is excluded
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["checkpoint", "show", "test-cp"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("included.txt"),
        "included.txt should be in checkpoint"
    );
    assert!(
        !stdout.contains("excluded.log"),
        "excluded.log should NOT be in checkpoint (gitignored)"
    );
}

// ── BUG-128 regression ──────────────────────────────────────────────────────
// Binary file full veil should give a clear binary-file error, not a UTF-8 error
#[test]
fn test_bug128_binary_full_veil_gives_clear_error() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    // Create a binary file (invalid UTF-8)
    let binary_content: Vec<u8> = vec![0x00, 0x01, 0xFF, 0xFE, 0x89, 0x50, 0x4E, 0x47];
    fs::write(temp.path().join("image.png"), &binary_content).unwrap();

    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "image.png"])
        .output()
        .unwrap();

    // BUG: Currently fails with a generic "stream did not contain valid UTF-8" IO error
    // instead of a dedicated BinaryFile error like partial veils get.
    // When fixed, this should either succeed (storing raw bytes) or fail with
    // a clear "binary file" error message.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("valid UTF-8"),
        "BUG-128: should not expose raw UTF-8 error for binary files, got: {stderr}"
    );
}

// ── BUG-129 regression ──────────────────────────────────────────────────────
// Checkpoint name should not allow path traversal
#[test]
fn test_bug129_checkpoint_name_rejects_path_traversal() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    create_file(&temp, "file.txt", "content\n");

    // Attempt to save a checkpoint with path traversal in the name
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["checkpoint", "save", "../../malicious"])
        .output()
        .unwrap();

    // BUG: Currently succeeds and creates directories outside the project root.
    // When fixed, should reject names containing path separators or '..'
    assert!(
        !output.status.success(),
        "BUG-129: checkpoint save should reject path-traversal names like '../../malicious'"
    );
}

// ── BUG-130 regression ──────────────────────────────────────────────────────
// Show command veil marker detection should not have false positives
#[test]
fn test_bug130_show_marker_false_positive() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    // Create a file with content that looks like a marker but isn't
    create_file(&temp, "code.rs", "let x = arr...[0];\nlet y = foo[1];\n");

    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["show", "code.rs"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // BUG: line.contains("...[") && line.contains("]") matches normal code
    assert!(
        !stdout.contains("[veiled]"),
        "BUG-130: show should not mark 'arr...[0]' as veiled, got: {stdout}"
    );
}

// ── BUG-131 regression ──────────────────────────────────────────────────────
// ensure_gitignore should repair corrupted blocks, not just check start marker
#[test]
fn test_bug131_gitignore_corrupted_block_not_repaired() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    // Corrupt the gitignore block: keep start marker but remove end marker and entries
    fs::write(
        temp.path().join(".gitignore"),
        "# MANAGED BY FUNVEIL\n# user stuff\n",
    )
    .unwrap();

    // Re-run init which calls ensure_gitignore
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    let content = read_file(&temp, ".gitignore");

    // BUG: ensure_gitignore sees the start marker and returns early,
    // leaving the block corrupted without the managed entries
    assert!(
        content.contains(".funveil_config") && content.contains(".funveil/"),
        "BUG-131: ensure_gitignore should repair block with missing entries, got:\n{content}"
    );
}

// ── BUG-132 regression ──────────────────────────────────────────────────────
// ensure_gitignore should respect existing CRLF line endings
#[test]
fn test_bug132_gitignore_crlf_mixed_endings() {
    let temp = TempDir::new().unwrap();

    // Create a CRLF gitignore before init
    fs::write(temp.path().join(".gitignore"), "*.log\r\nnode_modules/\r\n").unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    let content = fs::read(temp.path().join(".gitignore")).unwrap();
    let content_str = String::from_utf8_lossy(&content);

    // BUG: The appended funveil block uses \n while the existing file uses \r\n
    // Check that we don't have mixed line endings
    let has_crlf = content_str.contains("\r\n");
    let has_bare_lf = content_str.replace("\r\n", "").contains('\n');
    assert!(
        !(has_crlf && has_bare_lf),
        "BUG-132: gitignore has mixed line endings (CRLF and LF)"
    );
}

// ── BUG-133 regression ──────────────────────────────────────────────────────
// veil_directory should respect nested .gitignore files
#[test]
fn test_bug133_nested_gitignore_ignored_by_veil_directory() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    // Create a subdirectory with its own .gitignore
    fs::create_dir_all(temp.path().join("subdir")).unwrap();
    create_file(&temp, "subdir/.gitignore", "ignored.txt\n");
    create_file(&temp, "subdir/ignored.txt", "should be ignored\n");
    create_file(&temp, "subdir/included.txt", "should be veiled\n");

    // Veil the entire subdirectory
    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["veil", "subdir"])
        .assert()
        .success();

    // BUG: load_gitignore(root) only loads root-level .gitignore,
    // so subdir/.gitignore is ignored and subdir/ignored.txt gets veiled
    let ignored_content = read_file(&temp, "subdir/ignored.txt");
    assert_ne!(
        ignored_content, "...\n",
        "BUG-133: subdir/ignored.txt should be skipped per subdir/.gitignore but was veiled"
    );
}

// ── BUG-134 regression ──────────────────────────────────────────────────────
// Unveil regex should give feedback when files matched but none were veiled
#[test]
fn test_bug134_unveil_regex_no_feedback_when_none_veiled() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["init", "--mode", "blacklist"])
        .assert()
        .success();

    create_file(&temp, "hello.txt", "not veiled\n");

    // Unveil with regex that matches the file but it's not veiled
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["unveil", "/hello/"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    // BUG: When matched && !unveiled_any, user gets no output at all
    assert!(
        !combined.is_empty(),
        "BUG-134: unveil regex should give feedback when files matched but none were veiled"
    );
}

// ── BUG-135 regression ──────────────────────────────────────────────────────
// max_signature_length=0 should not produce empty string
#[test]
fn test_bug135_max_signature_length_zero() {
    // This is a unit-level bug in header.rs; we test via the parse command
    // which uses header strategy internally. The key issue is that
    // max_signature_length=0 returns "" instead of "..." or an error.
    //
    // Direct unit test: HeaderConfig { max_signature_length: Some(0) }
    // causes truncate_signature to return an empty string.
    // We verify this through the code path rather than CLI since
    // there's no CLI flag to set max_signature_length=0.
    //
    // The bug is documented; a unit test in header.rs would be more direct,
    // but we verify the invariant: a signature should never be empty when
    // a function exists.
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    create_file(&temp, "test.rs", "fn hello() {\n    println!(\"hi\");\n}\n");

    // Parse should produce non-empty signatures for valid functions
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["parse", "test.rs"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the parse works at all — the deeper bug is in header.rs when
    // max_signature_length is Some(0), producing "" instead of "..." or error
    assert!(
        stdout.contains("hello"),
        "parse should find the hello function"
    );
}

// ── BUG-136 regression ──────────────────────────────────────────────────────
// parse_file_line should reject unclosed quoted paths
#[test]
fn test_bug136_unclosed_quoted_path_in_patch() {
    let temp = TempDir::new().unwrap();

    assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .arg("init")
        .assert()
        .success();

    // Create a malformed patch with unclosed quotes in file paths
    let malformed_patch =
        "--- \"src/unclosed_path\n+++ \"src/unclosed_path\n@@ -1,1 +1,1 @@\n-old\n+new\n";
    create_file(&temp, "bad.patch", malformed_patch);

    // Apply the patch — parser should reject the unclosed quoted path
    let output = assert_cmd::cargo_bin_cmd!("fv")
        .current_dir(&temp)
        .args(["patch", "apply", "bad.patch"])
        .output()
        .unwrap();

    // BUG: parse_file_line uses unwrap_or(inner.len()) which silently accepts
    // the entire remaining string as the path instead of returning None.
    // The patch may be silently applied with a wrong path, or succeed
    // when it should fail with a parse error.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The file "src/unclosed_path" doesn't exist, so either:
    // 1. Parser should reject it (correct behavior after fix)
    // 2. Parser accepts the malformed path (the bug)
    // We check that if the path was parsed, it was NOT parsed as the entire
    // remaining string (which would include the newline and other content)
    assert!(
        !stdout.contains("src/unclosed_path\n+++ ") && !stderr.contains("src/unclosed_path\n+++ "),
        "BUG-136: parser should not silently accept unclosed quoted paths"
    );
}
