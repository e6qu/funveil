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
