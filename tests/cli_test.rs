use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::CommandCargoExt;
use predicates::prelude::*;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
#[allow(deprecated)]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Funveil"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("veil"))
        .stdout(predicate::str::contains("unveil"));
}

#[test]
#[allow(deprecated)]
fn test_cli_init() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    // Check config was created
    assert!(temp.path().join(".funveil_config").exists());
    assert!(temp.path().join(".funveil").exists());
}

#[test]
#[allow(deprecated)]
fn test_cli_status_no_config() {
    let temp = TempDir::new().unwrap();

    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("whitelist"));
}

#[test]
#[allow(deprecated)]
fn test_cli_mode_show() {
    let temp = TempDir::new().unwrap();

    // Initialize first
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Check mode
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("mode");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("whitelist"));
}

#[test]
#[allow(deprecated)]
fn test_cli_mode_change() {
    let temp = TempDir::new().unwrap();

    // Initialize
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Change mode
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("mode");
    cmd.arg("blacklist");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("blacklist"));

    // Verify config was updated
    let config_content = fs::read_to_string(temp.path().join(".funveil_config")).unwrap();
    assert!(config_content.contains("blacklist"));
}

#[test]
#[allow(deprecated)]
fn test_cli_init_twice() {
    let temp = TempDir::new().unwrap();

    // Initialize first time
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Try to initialize again
    let mut cmd = Command::cargo_bin("fv").unwrap();
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("already initialized"));
}
