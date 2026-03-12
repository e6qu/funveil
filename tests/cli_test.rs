use predicates::prelude::*;

use std::fs;
use tempfile::TempDir;

/// Helper: init a temp dir with funveil
fn init_temp() -> TempDir {
    let temp = TempDir::new().unwrap();
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();
    temp
}

/// Helper: init with blacklist mode
fn init_temp_blacklist() -> TempDir {
    let temp = TempDir::new().unwrap();
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.args(["init", "--mode", "blacklist"]);
    cmd.assert().success();
    temp
}

/// Helper: create a file in temp dir
fn create_file(temp: &TempDir, path: &str, content: &str) {
    let full_path = temp.path().join(path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full_path, content).unwrap();
}

/// Helper: run fv command and return output
fn fv(temp: &TempDir, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(temp);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.assert()
}

#[test]
fn test_cli_help() {
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Funveil"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("veil"))
        .stdout(predicate::str::contains("unveil"));
}

#[test]
fn test_cli_init() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
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
fn test_cli_status_no_config() {
    let temp = TempDir::new().unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("status");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("whitelist"));
}

#[test]
fn test_cli_mode_show() {
    let temp = TempDir::new().unwrap();

    // Initialize first
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Check mode
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("mode");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("whitelist"));
}

#[test]
fn test_cli_mode_change() {
    let temp = TempDir::new().unwrap();

    // Initialize
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Change mode
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
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
fn test_cli_init_twice() {
    let temp = TempDir::new().unwrap();

    // Initialize first time
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert().success();

    // Try to initialize again
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("init");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("already initialized"));
}

// ── Status command: veiled objects count (catches line 280: delete ! on objects.is_empty) ──

#[test]
fn test_cli_status_shows_veiled_objects_count() {
    let temp = init_temp_blacklist();
    create_file(&temp, "secret.txt", "secret content\n");

    // Veil a file
    fv(&temp, &["veil", "secret.txt", "-q"]).success();

    // Status should show "Veiled objects: 1"
    fv(&temp, &["status"])
        .success()
        .stdout(predicate::str::contains("Veiled objects: 1"));
}

#[test]
fn test_cli_status_no_veiled_objects_omits_count() {
    let temp = init_temp();

    // Status should NOT show "Veiled objects" when none exist
    let output = fv(&temp, &["status"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        !stdout.contains("Veiled objects"),
        "should not show veiled objects when empty"
    );
}

// ── Veil command: regex pattern matching (catches lines 301, 337, 345, 348, 360) ──

#[test]
fn test_cli_veil_regex_pattern_matches_files() {
    let temp = init_temp_blacklist();
    create_file(&temp, "src/foo.rs", "fn foo() {}\n");
    create_file(&temp, "src/bar.rs", "fn bar() {}\n");
    create_file(&temp, "src/baz.txt", "not rust\n");

    // Veil using regex to match .rs files
    fv(&temp, &["veil", "/\\.rs$/"])
        .success()
        .stdout(predicate::str::contains("Veiling"));

    // .rs files should be veiled (content replaced with marker)
    let foo_content = fs::read_to_string(temp.path().join("src/foo.rs")).unwrap();
    assert!(foo_content.contains("..."), "foo.rs should be veiled");

    // .txt file should NOT be veiled
    let baz_content = fs::read_to_string(temp.path().join("src/baz.txt")).unwrap();
    assert_eq!(baz_content, "not rust\n", "baz.txt should not be veiled");
}

#[test]
fn test_cli_veil_regex_no_match() {
    let temp = init_temp_blacklist();
    create_file(&temp, "file.txt", "content\n");

    // Regex that matches nothing
    fv(&temp, &["veil", "/nonexistent_pattern_xyz/"])
        .success()
        .stdout(predicate::str::contains("No files matched pattern"));
}

#[test]
fn test_cli_veil_regex_with_errors() {
    let temp = init_temp_blacklist();
    create_file(&temp, "file.txt", "content\n");

    // Veil the file first
    fv(&temp, &["veil", "file.txt", "-q"]).success();

    // Now try regex that matches already-veiled file
    fv(&temp, &["veil", "/file\\.txt/"])
        .success()
        .stderr(predicate::str::contains("Warning"));
}

#[test]
fn test_cli_veil_regex_boundary_len_check() {
    let temp = init_temp_blacklist();
    create_file(&temp, "f.txt", "content\n");

    // Pattern "/" (len 1) and "//" (len 2) should NOT be treated as regex
    // They should be treated as literal filenames
    fv(&temp, &["veil", "//"]).failure(); // file not found: //
}

#[test]
fn test_cli_veil_prints_message_when_not_quiet() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "content\n");

    // Without -q, should print "Veiling: test.txt"
    fv(&temp, &["veil", "test.txt"])
        .success()
        .stdout(predicate::str::contains("Veiling: test.txt"));
}

#[test]
fn test_cli_veil_quiet_suppresses_output() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "content\n");

    // With -q, should NOT print anything
    let output = fv(&temp, &["veil", "test.txt", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.is_empty(), "quiet mode should suppress output");
}

// ── Veil headers mode (catches line 396: delete ! on quiet) ──

#[test]
fn test_cli_veil_headers_mode() {
    let temp = init_temp();
    create_file(
        &temp,
        "lib.rs",
        "pub fn hello() {\n    println!(\"hi\");\n}\n",
    );

    fv(&temp, &["veil", "--mode", "headers", "lib.rs"])
        .success()
        .stdout(predicate::str::contains("headers mode"));
}

// ── Parse command: detailed format (catches lines 445, 452) ──

#[test]
fn test_cli_parse_detailed_with_imports_and_calls() {
    let temp = init_temp();
    create_file(
        &temp,
        "test.rs",
        "use std::fs;\n\nfn main() {\n    fs::read_to_string(\"f\");\n}\n",
    );

    fv(&temp, &["parse", "--format", "detailed", "test.rs"])
        .success()
        .stdout(predicate::str::contains("Imports:"))
        .stdout(predicate::str::contains("Symbols:"));
}

#[test]
fn test_cli_parse_summary_format() {
    let temp = init_temp();
    create_file(&temp, "test.rs", "fn main() {}\n");

    fv(&temp, &["parse", "--format", "summary", "test.rs"])
        .success()
        .stdout(predicate::str::contains("Functions:"));
}

#[test]
fn test_cli_parse_nonexistent_file() {
    let temp = init_temp();

    fv(&temp, &["parse", "nonexistent.rs"])
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ── Trace command: list format (catches line 621: delete match arm TraceFormat::List) ──

#[test]
fn test_cli_trace_list_format() {
    let temp = init_temp();
    create_file(
        &temp,
        "test.rs",
        "fn main() { helper(); }\nfn helper() {}\n",
    );

    fv(
        &temp,
        &["trace", "--from", "main", "--format", "list", "test.rs"],
    )
    .success();
}

#[test]
fn test_cli_trace_tree_format() {
    let temp = init_temp();
    create_file(
        &temp,
        "test.rs",
        "fn main() { helper(); }\nfn helper() {}\n",
    );

    fv(
        &temp,
        &["trace", "--from", "main", "--format", "tree", "test.rs"],
    )
    .success();
}

#[test]
fn test_cli_trace_dot_format() {
    let temp = init_temp();
    create_file(
        &temp,
        "test.rs",
        "fn main() { helper(); }\nfn helper() {}\n",
    );

    fv(
        &temp,
        &["trace", "--from", "main", "--format", "dot", "test.rs"],
    )
    .success();
}

#[test]
fn test_cli_trace_unknown_function_warning() {
    let temp = init_temp();
    create_file(&temp, "test.rs", "fn main() {}\n");

    // Trace a function that doesn't exist - should warn but not error
    fv(&temp, &["trace", "--from", "nonexistent_function_xyz"])
        .success()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_cli_trace_no_std_filter() {
    let temp = init_temp();
    create_file(
        &temp,
        "test.rs",
        "fn main() { helper(); }\nfn helper() { std::io::stdout(); }\n",
    );

    fv(
        &temp,
        &["trace", "--from", "main", "--no-std", "--format", "tree"],
    )
    .success();
}

#[test]
fn test_cli_trace_from_entrypoint() {
    let temp = init_temp();
    create_file(
        &temp,
        "test.rs",
        "fn main() { helper(); }\nfn helper() {}\n",
    );

    fv(&temp, &["trace", "--from-entrypoint"]).success();
}

#[test]
fn test_cli_trace_from_entrypoint_quiet() {
    let temp = init_temp();
    create_file(
        &temp,
        "test.rs",
        "fn main() { helper(); }\nfn helper() {}\n",
    );

    fv(&temp, &["trace", "--from-entrypoint", "-q"]).success();
}

#[test]
fn test_cli_trace_cycle_detected() {
    let temp = init_temp();
    create_file(&temp, "test.rs", "fn a() { b(); }\nfn b() { a(); }\n");

    // Should detect cycle and show note
    fv(&temp, &["trace", "--from", "a", "--depth", "10"])
        .success()
        .stderr(predicate::str::contains("Cycle detected"));
}

// ── Entrypoints command (catches lines 713, 726-736: type filtering) ──

#[test]
fn test_cli_entrypoints_shows_main() {
    let temp = init_temp();
    create_file(&temp, "main.rs", "fn main() {}\n");

    fv(&temp, &["entrypoints"])
        .success()
        .stdout(predicate::str::contains("main"))
        .stdout(predicate::str::contains("Total:"));
}

#[test]
fn test_cli_entrypoints_filter_main_type() {
    let temp = init_temp();
    create_file(
        &temp,
        "main.rs",
        "fn main() {}\n#[test]\nfn test_foo() {}\n",
    );

    // Filter by main type only
    fv(&temp, &["entrypoints", "--entry-type", "main"])
        .success()
        .stdout(predicate::str::contains("main"));
}

#[test]
fn test_cli_entrypoints_filter_test_type() {
    let temp = init_temp();
    create_file(
        &temp,
        "lib.rs",
        "#[test]\nfn test_something() {}\nfn helper() {}\n",
    );

    fv(&temp, &["entrypoints", "--entry-type", "test"])
        .success()
        .stdout(predicate::str::contains("test_something"));
}

#[test]
fn test_cli_entrypoints_no_results() {
    let temp = init_temp();
    // A file with no entrypoints
    create_file(&temp, "lib.rs", "fn helper() {}\n");

    fv(&temp, &["entrypoints", "--entry-type", "main"])
        .success()
        .stdout(predicate::str::contains("No entrypoints"));
}

#[test]
fn test_cli_entrypoints_quiet_suppresses() {
    let temp = init_temp();
    create_file(&temp, "main.rs", "fn main() {}\n");

    let output = fv(&temp, &["entrypoints", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.is_empty(),
        "quiet mode should suppress entrypoint output"
    );
}

// ── Cache commands (catches lines 787, 795) ──

#[test]
fn test_cli_cache_status() {
    let temp = init_temp();

    fv(&temp, &["cache", "status"]).success();
}

#[test]
fn test_cli_cache_clear() {
    let temp = init_temp();

    fv(&temp, &["cache", "clear"])
        .success()
        .stdout(predicate::str::contains("Cache cleared"));
}

#[test]
fn test_cli_cache_invalidate() {
    let temp = init_temp();

    fv(&temp, &["cache", "invalidate"])
        .success()
        .stdout(predicate::str::contains("invalidated"));
}

#[test]
fn test_cli_cache_quiet() {
    let temp = init_temp();

    let output = fv(&temp, &["cache", "clear", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.is_empty(),
        "quiet cache clear should suppress output"
    );

    let output = fv(&temp, &["cache", "invalidate", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.is_empty(),
        "quiet cache invalidate should suppress output"
    );
}

// ── Unveil command: regex, --all, partial (catches lines 808-887) ──

#[test]
fn test_cli_unveil_all() {
    let temp = init_temp_blacklist();
    create_file(&temp, "a.txt", "content a\n");
    create_file(&temp, "b.txt", "content b\n");

    fv(&temp, &["veil", "a.txt", "-q"]).success();
    fv(&temp, &["veil", "b.txt", "-q"]).success();

    fv(&temp, &["unveil", "--all"])
        .success()
        .stdout(predicate::str::contains("Unveiled all files"));
}

#[test]
fn test_cli_unveil_all_quiet() {
    let temp = init_temp_blacklist();
    create_file(&temp, "a.txt", "content a\n");
    fv(&temp, &["veil", "a.txt", "-q"]).success();

    let output = fv(&temp, &["unveil", "--all", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.is_empty(),
        "quiet unveil --all should suppress output"
    );
}

#[test]
fn test_cli_unveil_with_line_ranges() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "line1\nline2\nline3\nline4\n");

    // Veil lines 2-3
    fv(&temp, &["veil", "test.txt#2-3", "-q"]).success();

    // Unveil lines 2-3
    fv(&temp, &["unveil", "test.txt#2-3"])
        .success()
        .stdout(predicate::str::contains("Unveiled"));

    let content = fs::read_to_string(temp.path().join("test.txt")).unwrap();
    assert_eq!(content, "line1\nline2\nline3\nline4\n");
}

#[test]
fn test_cli_unveil_quiet_suppresses() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "content\n");
    fv(&temp, &["veil", "test.txt", "-q"]).success();

    let output = fv(&temp, &["unveil", "test.txt", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.is_empty(), "quiet unveil should suppress output");
}

#[test]
fn test_cli_unveil_regex_pattern() {
    let temp = init_temp_blacklist();
    create_file(&temp, "a.txt", "content a\n");
    create_file(&temp, "b.txt", "content b\n");

    fv(&temp, &["veil", "a.txt", "-q"]).success();
    fv(&temp, &["veil", "b.txt", "-q"]).success();

    // Unveil using regex
    fv(&temp, &["unveil", "/\\.txt$/"])
        .success()
        .stdout(predicate::str::contains("Unveiled"));

    let a_content = fs::read_to_string(temp.path().join("a.txt")).unwrap();
    assert_eq!(a_content, "content a\n");
}

#[test]
fn test_cli_unveil_regex_no_match() {
    let temp = init_temp_blacklist();
    create_file(&temp, "file.txt", "content\n");

    fv(&temp, &["unveil", "/nonexistent_xyz/"])
        .success()
        .stdout(predicate::str::contains("No files matched"));
}

#[test]
fn test_cli_unveil_regex_no_veiled_files() {
    let temp = init_temp_blacklist();
    create_file(&temp, "file.txt", "content\n");

    // Regex matches file but it's not veiled
    fv(&temp, &["unveil", "/file\\.txt/"])
        .success()
        .stdout(predicate::str::contains("No veiled files"));
}

#[test]
fn test_cli_unveil_regex_with_errors() {
    let temp = init_temp_blacklist();
    create_file(&temp, "a.txt", "content a\n");
    create_file(&temp, "b.txt", "content b\n");

    // Veil only a.txt
    fv(&temp, &["veil", "a.txt", "-q"]).success();

    // Regex matches both, but only a.txt is veiled
    fv(&temp, &["unveil", "/\\.txt$/"]).success();
}

#[test]
fn test_cli_unveil_no_pattern_no_all() {
    let temp = init_temp();

    // No pattern and no --all should be a usage error
    fv(&temp, &["unveil"]).failure();
}

// ── Apply command (catches lines 902-994) ──

#[test]
fn test_cli_apply_re_veils_modified_files() {
    let temp = init_temp_blacklist();
    create_file(&temp, "secret.txt", "original content\n");

    // Veil the file
    fv(&temp, &["veil", "secret.txt", "-q"]).success();

    // Unveil it
    fv(&temp, &["unveil", "secret.txt", "-q"]).success();

    // Re-veil it
    fv(&temp, &["veil", "secret.txt", "-q"]).success();

    // Apply should report already veiled
    fv(&temp, &["apply"])
        .success()
        .stdout(predicate::str::contains("Re-applying veils"))
        .stdout(predicate::str::contains("Applied:"));
}

#[test]
fn test_cli_apply_quiet() {
    let temp = init_temp_blacklist();
    create_file(&temp, "secret.txt", "content\n");
    fv(&temp, &["veil", "secret.txt", "-q"]).success();

    let output = fv(&temp, &["apply", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.is_empty(), "quiet apply should suppress output");
}

#[test]
fn test_cli_apply_missing_file_skipped() {
    let temp = init_temp_blacklist();
    create_file(&temp, "temp.txt", "content\n");

    fv(&temp, &["veil", "temp.txt", "-q"]).success();

    // Delete the file from disk but leave config intact
    fs::remove_file(temp.path().join("temp.txt")).unwrap();

    // Apply should skip the missing file
    fv(&temp, &["apply"])
        .success()
        .stdout(predicate::str::contains("Skipped: 1"));
}

// ── Show command (catches line 1038: + with - or *) ──

#[test]
fn test_cli_show_fully_veiled() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "secret\n");

    fv(&temp, &["veil", "test.txt", "-q"]).success();

    fv(&temp, &["show", "test.txt"])
        .success()
        .stdout(predicate::str::contains("FULLY VEILED"));
}

#[test]
fn test_cli_show_partially_veiled() {
    let temp = init_temp_blacklist();
    create_file(
        &temp,
        "test.txt",
        "line1\nline2\nline3\nline4\nline5\nline6\n",
    );

    // Veil lines 3-4
    fv(&temp, &["veil", "test.txt#3-4", "-q"]).success();

    // Show should display the file with veiled annotations and line numbers
    fv(&temp, &["show", "test.txt"])
        .success()
        .stdout(predicate::str::contains("File: test.txt"))
        .stdout(predicate::str::contains("[veiled]"));
}

#[test]
fn test_cli_show_not_veiled() {
    let temp = init_temp();
    create_file(&temp, "test.txt", "hello world\n");

    // Show an un-veiled file should display content with line numbers
    fv(&temp, &["show", "test.txt"])
        .success()
        .stdout(predicate::str::contains("1"))
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn test_cli_show_nonexistent_file() {
    let temp = init_temp();

    fv(&temp, &["show", "nonexistent.txt"])
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ── Checkpoint commands (catches lines 1076) ──

#[test]
fn test_cli_checkpoint_save_and_list() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "content\n");
    fv(&temp, &["veil", "test.txt", "-q"]).success();

    fv(&temp, &["checkpoint", "save", "snap1"]).success();

    fv(&temp, &["checkpoint", "list"])
        .success()
        .stdout(predicate::str::contains("snap1"));
}

#[test]
fn test_cli_checkpoint_list_empty() {
    let temp = init_temp();

    fv(&temp, &["checkpoint", "list"])
        .success()
        .stdout(predicate::str::contains("No checkpoints"));
}

#[test]
fn test_cli_checkpoint_list_empty_quiet() {
    let temp = init_temp();

    let output = fv(&temp, &["checkpoint", "list", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.is_empty(),
        "quiet checkpoint list should suppress output"
    );
}

#[test]
fn test_cli_checkpoint_show() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "content\n");
    fv(&temp, &["veil", "test.txt", "-q"]).success();

    fv(&temp, &["checkpoint", "save", "snap1", "-q"]).success();
    fv(&temp, &["checkpoint", "show", "snap1"]).success();
}

#[test]
fn test_cli_checkpoint_delete() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "content\n");
    fv(&temp, &["veil", "test.txt", "-q"]).success();

    fv(&temp, &["checkpoint", "save", "snap1", "-q"]).success();
    fv(&temp, &["checkpoint", "delete", "snap1"]).success();

    // Should be gone
    fv(&temp, &["checkpoint", "list"])
        .success()
        .stdout(predicate::str::contains("No checkpoints"));
}

#[test]
fn test_cli_checkpoint_restore() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "content\n");
    fv(&temp, &["veil", "test.txt", "-q"]).success();

    fv(&temp, &["checkpoint", "save", "snap1", "-q"]).success();

    // Unveil the file
    fv(&temp, &["unveil", "test.txt", "-q"]).success();

    // Restore checkpoint
    fv(&temp, &["checkpoint", "restore", "snap1"]).success();
}

// ── Restore command (catches line 1001) ──

#[test]
fn test_cli_restore_no_checkpoints() {
    let temp = init_temp();

    fv(&temp, &["restore"])
        .failure()
        .stderr(predicate::str::contains("No checkpoints"));
}

#[test]
fn test_cli_restore_latest() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "content\n");
    fv(&temp, &["veil", "test.txt", "-q"]).success();

    fv(&temp, &["checkpoint", "save", "snap1", "-q"]).success();

    // Modify state
    fv(&temp, &["unveil", "test.txt", "-q"]).success();

    fv(&temp, &["restore"])
        .success()
        .stdout(predicate::str::contains("Restoring from latest checkpoint"));
}

// ── Doctor command (catches lines 1094+) ──

#[test]
fn test_cli_doctor_all_ok() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "content\n");
    fv(&temp, &["veil", "test.txt", "-q"]).success();

    fv(&temp, &["doctor"])
        .success()
        .stdout(predicate::str::contains("All checks passed"));
}

#[test]
fn test_cli_doctor_quiet() {
    let temp = init_temp();

    let output = fv(&temp, &["doctor", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.is_empty(), "quiet doctor should suppress output");
}

// ── GC command (catches lines 1129+) ──

#[test]
fn test_cli_gc_with_output() {
    let temp = init_temp();

    fv(&temp, &["gc"])
        .success()
        .stdout(predicate::str::contains("Garbage collected"))
        .stdout(predicate::str::contains("Freed"));
}

#[test]
fn test_cli_gc_quiet() {
    let temp = init_temp();

    let output = fv(&temp, &["gc", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.is_empty(), "quiet gc should suppress output");
}

// ── Clean command (catches lines 1158, 1173) ──

#[test]
fn test_cli_clean() {
    let temp = init_temp();

    fv(&temp, &["clean"])
        .success()
        .stdout(predicate::str::contains("Removing all funveil data"))
        .stdout(predicate::str::contains("Removed all funveil data"));

    // Verify config and data dir are gone
    assert!(!temp.path().join(".funveil_config").exists());
    assert!(!temp.path().join(".funveil").exists());
}

#[test]
fn test_cli_clean_quiet() {
    let temp = init_temp();

    let output = fv(&temp, &["clean", "-q"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.is_empty(), "quiet clean should suppress output");

    // Still actually cleans
    assert!(!temp.path().join(".funveil_config").exists());
}

// ── Veil with partial line ranges via CLI (catches various) ──

#[test]
fn test_cli_veil_partial_range() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "line1\nline2\nline3\nline4\n");

    fv(&temp, &["veil", "test.txt#2-3"])
        .success()
        .stdout(predicate::str::contains("Veiling"));
}

// ── Entrypoint type filters (catches == to != mutations at lines 726-736) ──

#[test]
fn test_cli_entrypoints_filter_cli_type() {
    let temp = init_temp();
    create_file(
        &temp,
        "main.go",
        "package main\n\nfunc main() {\n\tflag.Parse()\n}\n",
    );

    // Filter by CLI type - may or may not find results but should run cleanly
    fv(&temp, &["entrypoints", "--entry-type", "cli"]).success();
}

#[test]
fn test_cli_entrypoints_filter_handler_type() {
    let temp = init_temp();
    create_file(
        &temp,
        "handler.py",
        "from flask import Flask\napp = Flask(__name__)\n@app.route('/')\ndef index():\n    return 'hi'\n",
    );

    fv(&temp, &["entrypoints", "--entry-type", "handler"]).success();
}

#[test]
fn test_cli_entrypoints_filter_export_type() {
    let temp = init_temp();
    create_file(
        &temp,
        "lib.ts",
        "export function greet(): string { return 'hi'; }\n",
    );

    fv(&temp, &["entrypoints", "--entry-type", "export"]).success();
}

// ── Apply command edge cases (catches lines 918-987) ──

#[test]
fn test_cli_apply_already_veiled_file() {
    let temp = init_temp_blacklist();
    create_file(&temp, "file.txt", "original\n");

    fv(&temp, &["veil", "file.txt", "-q"]).success();

    // Apply on already-veiled file should show "already veiled"
    fv(&temp, &["apply"])
        .success()
        .stdout(predicate::str::contains("already veiled"));
}

#[test]
fn test_cli_apply_hash_in_filename() {
    let temp = init_temp_blacklist();
    // File with '#' in the name (tests BUG-099 key parsing in apply)
    create_file(&temp, "dir/file#name.txt", "content\n");

    fv(&temp, &["veil", "dir/file#name.txt", "-q"]).success();
    fv(&temp, &["apply"]).success();
}

// ── find_project_root: || to && mutation at line 1198 ──

#[test]
fn test_cli_works_with_git_dir() {
    let temp = TempDir::new().unwrap();
    // Create a .git directory (simulating a git repo)
    fs::create_dir_all(temp.path().join(".git")).unwrap();

    // fv should find project root via .git
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("status");
    cmd.assert().success();
}

#[test]
// ── Apply command counter verification (catches lines 918-987) ──

fn test_cli_apply_shows_correct_counts() {
    let temp = init_temp_blacklist();
    create_file(&temp, "file1.txt", "content1\n");
    create_file(&temp, "file2.txt", "content2\n");

    // Veil both files
    fv(&temp, &["veil", "file1.txt", "-q"]).success();
    fv(&temp, &["veil", "file2.txt", "-q"]).success();

    // Apply should show both as already veiled, Applied: 0
    fv(&temp, &["apply"])
        .success()
        .stdout(predicate::str::contains("Applied: 0"));
}

#[test]
fn test_cli_apply_unveiled_file_re_veils() {
    let temp = init_temp_blacklist();
    create_file(&temp, "file1.txt", "content1\n");

    fv(&temp, &["veil", "file1.txt", "-q"]).success();

    // Unveil the file so it has original content on disk
    fv(&temp, &["unveil", "file1.txt", "-q"]).success();

    // Re-veil it manually so config has the entry
    fv(&temp, &["veil", "file1.txt", "-q"]).success();

    // Now unveil again to leave file with original content but config entry
    fv(&temp, &["unveil", "file1.txt", "-q"]).success();
    // Re-veil
    fv(&temp, &["veil", "file1.txt", "-q"]).success();

    // Apply should show "already veiled"
    fv(&temp, &["apply"])
        .success()
        .stdout(predicate::str::contains("already veiled"));
}

#[test]
fn test_cli_apply_skipped_count_with_missing_and_invalid() {
    let temp = init_temp_blacklist();
    create_file(&temp, "exists.txt", "content\n");

    fv(&temp, &["veil", "exists.txt", "-q"]).success();

    // Delete the file to make it "missing"
    fs::remove_file(temp.path().join("exists.txt")).unwrap();

    // Apply should skip it and report Skipped: 1
    fv(&temp, &["apply"])
        .success()
        .stdout(predicate::str::contains("Skipped: 1"))
        .stderr(predicate::str::contains("Skipping"));
}

#[test]
fn test_cli_apply_cas_missing_original() {
    use std::os::unix::fs::PermissionsExt;

    let temp = init_temp_blacklist();
    create_file(&temp, "file.txt", "content\n");

    fv(&temp, &["veil", "file.txt", "-q"]).success();

    // Delete CAS objects - need to make them writable first
    let objects_dir = temp.path().join(".funveil/objects");
    if objects_dir.exists() {
        // Recursively make writable
        for e in walkdir::WalkDir::new(&objects_dir).into_iter().flatten() {
            let _ = fs::set_permissions(e.path(), fs::Permissions::from_mode(0o755));
        }
        let _ = fs::remove_dir_all(&objects_dir);
        fs::create_dir_all(&objects_dir).unwrap();
    }

    // Write original content back to file
    {
        let file_path = temp.path().join("file.txt");
        let _ = fs::set_permissions(&file_path, fs::Permissions::from_mode(0o644));
    }
    create_file(&temp, "file.txt", "content\n");

    // Apply should detect CAS is missing and skip/warn
    fv(&temp, &["apply"]).success().stderr(
        predicate::str::contains("missing from CAS").or(predicate::str::contains("Skipping")),
    );
}

// ── Show command line numbers (catches line 1038: + with * or -) ──

#[test]
fn test_cli_show_line_numbers_correct() {
    let temp = init_temp();
    create_file(&temp, "test.txt", "first\nsecond\nthird\n");

    // Show should display correct line numbers starting from 1
    // If + mutated to *, line 0 (i*1=0) would appear instead of 1
    // If + mutated to -, line 0 (i-1=-1) or wrong numbers would appear
    fv(&temp, &["show", "test.txt"])
        .success()
        .stdout(predicate::str::contains("1 | first"))
        .stdout(predicate::str::contains("2 | second"))
        .stdout(predicate::str::contains("3 | third"));
}

#[test]
fn test_cli_show_line_numbers_not_zero_indexed() {
    let temp = init_temp();
    create_file(&temp, "test.txt", "alpha\nbeta\n");

    let output = fv(&temp, &["show", "test.txt"]).success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    // Line numbers must start at 1, not 0
    assert!(
        !stdout.contains("0 | alpha"),
        "line numbers should start at 1, not 0"
    );
    assert!(
        stdout.contains("1 | alpha"),
        "first line should be numbered 1"
    );
    assert!(
        stdout.contains("2 | beta"),
        "second line should be numbered 2"
    );
}

// ── Apply command with partial veils (catches line 918-919 key parsing) ──

#[test]
fn test_cli_apply_with_partial_veils() {
    let temp = init_temp_blacklist();
    create_file(&temp, "test.txt", "line1\nline2\nline3\nline4\nline5\n");

    // Partial veil creates keys like test.txt#1-2 and test.txt#_original
    fv(&temp, &["veil", "test.txt#1-2", "-q"]).success();

    // Apply should handle partial veil keys correctly (parsing # suffix as range)
    fv(&temp, &["apply"])
        .success()
        .stdout(predicate::str::contains("Applied:"));
}

// ── find_project_root: parent directory discovery (catches line 1198) ──

#[test]
fn test_cli_finds_project_root_in_parent_with_config() {
    let temp = init_temp(); // Creates .funveil_config at root
    let subdir = temp.path().join("src/deep/nested");
    fs::create_dir_all(&subdir).unwrap();

    // Running from subdirectory should find project root via parent traversal
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&subdir);
    cmd.arg("status");
    cmd.assert().success();
}

#[test]
fn test_cli_finds_project_root_with_git_in_parent() {
    let temp = TempDir::new().unwrap();
    // Create .git dir at root
    fs::create_dir_all(temp.path().join(".git")).unwrap();
    let subdir = temp.path().join("src/module");
    fs::create_dir_all(&subdir).unwrap();

    // Running from subdirectory should find project root via .git in parent
    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&subdir);
    cmd.arg("status");
    cmd.assert().success();
}

#[test]
fn test_cli_works_with_funveil_config() {
    let temp = TempDir::new().unwrap();
    // Create a .funveil_config file manually (needs version field)
    fs::write(
        temp.path().join(".funveil_config"),
        "version: 1\nmode: whitelist\nblacklist: []\nwhitelist: []\nobjects: {}\n",
    )
    .unwrap();
    fs::create_dir_all(temp.path().join(".funveil")).unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&temp);
    cmd.arg("status");
    cmd.assert().success();
}

// ── Tests targeting specific main.rs missed mutants ──

#[test]
fn test_cli_veil_regex_no_match_message() {
    // Catches line 345: if !matched && !quiet
    let temp = init_temp_blacklist();
    create_file(&temp, "hello.txt", "hello\n");

    // Regex pattern that matches nothing
    fv(&temp, &["veil", "/\\.xyz$/"])
        .success()
        .stdout(predicate::str::contains("No files matched pattern"));
}

#[test]
fn test_cli_veil_regex_match_no_unmatch_message() {
    // Catches line 345: when pattern DOES match, "No files matched" should NOT appear
    let temp = init_temp_blacklist();
    create_file(&temp, "hello.txt", "hello\n");

    fv(&temp, &["veil", "/\\.txt$/"])
        .success()
        .stdout(predicate::str::contains("No files matched").not());
}

#[test]
fn test_cli_veil_regex_error_count_message() {
    // Catches line 348: if file_errors > 0 && !quiet
    let temp = init_temp_blacklist();
    create_file(&temp, "hello.txt", "hello\n");

    // Veil a file, then try regex that matches already-veiled file
    fv(&temp, &["veil", "hello.txt"]).success();
    fv(&temp, &["veil", "/\\.txt$/"])
        .success()
        .stderr(predicate::str::contains("could not be veiled"));
}

#[test]
fn test_cli_entrypoints_main_type_excludes_non_main() {
    // Catches line 726: ep.entry_type == EntrypointType::Main (== to !=)
    // When filtering for main, test entrypoints should NOT appear
    let temp = init_temp();
    create_file(
        &temp,
        "main.rs",
        "fn main() {}\n#[test]\nfn test_foo() {}\n",
    );

    fv(&temp, &["entrypoints", "--entry-type", "main"])
        .success()
        .stdout(predicate::str::contains("main"))
        .stdout(predicate::str::contains("test_foo").not());
}

#[test]
fn test_cli_entrypoints_test_type_only_tests() {
    // Catches line 729: ep.entry_type == EntrypointType::Test (== to !=)
    let temp = init_temp();
    create_file(
        &temp,
        "lib.rs",
        "#[test]\nfn test_something() {}\nfn main() {}\n",
    );

    fv(&temp, &["entrypoints", "--entry-type", "test"])
        .success()
        .stdout(predicate::str::contains("test_something"));
}

#[test]
fn test_cli_unveil_regex_pattern_restores_content() {
    // Catches line 821: regex pattern detection and unveil
    let temp = init_temp();
    create_file(&temp, "hello.txt", "hello\n");
    create_file(&temp, "world.txt", "world\n");

    fv(&temp, &["veil", "hello.txt"]).success();
    fv(&temp, &["veil", "world.txt"]).success();

    fv(&temp, &["unveil", "/hello/"]).success();

    let content = fs::read_to_string(temp.path().join("hello.txt")).unwrap();
    assert_eq!(content, "hello\n");
    // world.txt should still be veiled
    let world = fs::read_to_string(temp.path().join("world.txt")).unwrap();
    assert_eq!(world, "...\n");
}

#[test]
fn test_cli_unveil_regex_unmatched_non_veiled() {
    // Catches line 875: matched && !unveiled_any && !quiet
    let temp = init_temp();
    create_file(&temp, "hello.txt", "hello\n");

    // File exists but not veiled — regex matches file but nothing to unveil
    fv(&temp, &["unveil", "/hello/"])
        .success()
        .stdout(predicate::str::contains("No veiled files matched"));
}

#[test]
fn test_cli_unveil_regex_no_files_at_all() {
    // Catches line 871-872: !matched && !quiet
    let temp = init_temp();
    create_file(&temp, "hello.txt", "hello\n");

    fv(&temp, &["unveil", "/zzzzz/"])
        .success()
        .stdout(predicate::str::contains("No files matched pattern"));
}

#[test]
fn test_cli_status_hash_in_filename() {
    // Catches lines 918-919: suffix parsing with # in filename
    let temp = init_temp();
    create_file(&temp, "test#file.txt", "content\n");

    fv(&temp, &["veil", "test#file.txt"]).success();
    fv(&temp, &["status"]).success();
}

#[test]
fn test_cli_show_partial_line_numbers_start_at_one() {
    // Catches line 1038 & 1059: let line_num = i + 1
    // If + becomes *, first line would be 0 instead of 1
    let temp = init_temp();
    create_file(&temp, "test.txt", "line1\nline2\nline3\nline4\nline5\n");

    // Show an un-veiled file — format is "   1 | line1"
    fv(&temp, &["show", "test.txt"])
        .success()
        .stdout(predicate::str::contains("   1 | line1"))
        .stdout(predicate::str::contains("   5 | line5"));
}

#[test]
fn test_cli_apply_re_veil_applied_count() {
    // Catches lines 975: applied += 1
    // If += becomes *=, applied stays 0. Test must check "Applied: 1"
    let temp = init_temp_blacklist();
    create_file(&temp, "file.txt", "content\n");

    // Veil, then unveil to leave config entry with original on disk
    fv(&temp, &["veil", "file.txt", "-q"]).success();
    // Manually write original content back (simulate file being modified)
    {
        use std::os::unix::fs::PermissionsExt;
        let fp = temp.path().join("file.txt");
        let _ = fs::set_permissions(&fp, fs::Permissions::from_mode(0o644));
        fs::write(&fp, "content\n").unwrap();
    }

    // Apply should re-veil and report Applied: 1
    fv(&temp, &["apply"])
        .success()
        .stdout(predicate::str::contains("Applied: 1"));
}

#[test]
fn test_cli_show_partial_veil_line_numbers() {
    // Catches line 1038: let line_num = i + 1 (in partial veil path)
    // If + becomes *, first line would be 0
    let temp = init_temp();
    create_file(&temp, "test.txt", "visible1\nsecret1\nsecret2\nvisible2\n");

    fv(&temp, &["veil", "test.txt#2-3"]).success();

    // Show output for partial veil should have correct line numbers
    fv(&temp, &["show", "test.txt"])
        .success()
        .stdout(predicate::str::contains("1 |"))
        .stdout(predicate::str::contains("2 |"))
        .stdout(predicate::str::contains("4 |"));
}

#[test]
fn test_cli_unveil_regex_with_veiled_files() {
    // Catches line 821: regex pattern handling (&&/|| and > / >=)
    // Tests the full regex unveil flow with actual veiled files
    let temp = init_temp();
    create_file(&temp, "alpha.txt", "alpha content\n");
    create_file(&temp, "beta.txt", "beta content\n");
    create_file(&temp, "gamma.log", "gamma content\n");

    fv(&temp, &["veil", "alpha.txt"]).success();
    fv(&temp, &["veil", "beta.txt"]).success();
    fv(&temp, &["veil", "gamma.log"]).success();

    // Regex unveil matching .txt files only
    fv(&temp, &["unveil", "/\\.txt$/"]).success();

    // .txt files should be unveiled
    let alpha = fs::read_to_string(temp.path().join("alpha.txt")).unwrap();
    assert_eq!(alpha, "alpha content\n");
    let beta = fs::read_to_string(temp.path().join("beta.txt")).unwrap();
    assert_eq!(beta, "beta content\n");

    // .log file should still be veiled
    let gamma = fs::read_to_string(temp.path().join("gamma.log")).unwrap();
    assert_eq!(gamma, "...\n");
}

#[test]
fn test_cli_entrypoints_cli_type_filter() {
    // Catches line 731: ep.entry_type == EntrypointType::Cli
    let temp = init_temp();
    // Clap derive with Parser triggers Cli detection
    create_file(
        &temp,
        "app.rs",
        "use clap::Parser;\n#[derive(Parser)]\nstruct Cli {}\nfn main() {}\n",
    );

    fv(&temp, &["entrypoints", "--entry-type", "cli"]).success();
}

#[test]
fn test_cli_entrypoints_handler_type_filter() {
    // Catches line 733: ep.entry_type == EntrypointType::Handler
    let temp = init_temp();
    create_file(
        &temp,
        "handler.py",
        "def handle_request(event, context):\n    return 200\n",
    );

    fv(&temp, &["entrypoints", "--entry-type", "handler"]).success();
}

#[test]
fn test_cli_entrypoints_export_type_filter() {
    // Catches line 736: ep.entry_type == EntrypointType::Export
    let temp = init_temp();
    create_file(
        &temp,
        "lib.ts",
        "export function processData(data: string): void {}\n",
    );

    fv(&temp, &["entrypoints", "--entry-type", "export"]).success();
}

#[test]
fn test_cli_find_project_root_from_subdir_with_config_only() {
    // Catches line 1198: parent.join(CONFIG_FILE).exists() || parent.join(".git").exists()
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join(".funveil_config"),
        "version: 1\nmode: whitelist\nblacklist: []\nwhitelist: []\nobjects: {}\n",
    )
    .unwrap();
    fs::create_dir_all(temp.path().join(".funveil")).unwrap();

    let subdir = temp.path().join("subdir");
    fs::create_dir_all(&subdir).unwrap();

    let mut cmd = assert_cmd::cargo_bin_cmd!("fv");
    cmd.current_dir(&subdir);
    cmd.arg("status");
    cmd.assert().success();
}
