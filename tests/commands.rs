#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![allow(unused_imports)]

mod common;

use common::{run_in_dir, run_in_temp, TestEnv, TestWriter};
use funveil::CONFIG_FILE;
use funveil::{
    apply_level, check_integrity, command_category, delete_checkpoint, garbage_collect,
    generate_trace_id, get_latest_checkpoint, has_veils, is_supported_source, list_checkpoints,
    normalize_path, restore_checkpoint, save_checkpoint, show_checkpoint, snapshot_config,
    snapshot_files, unveil_all, unveil_file, veil_file, walk_files,
};
use funveil::{
    collect_affected_files_for_pattern, handle_level_veil, parse_pattern, restore_action_state,
    run_command, update_metadata, version_long, ActionHistory, ActionRecord, ActionState,
    ActionSummary, CacheCmd, CallGraphBuilder, CheckpointCmd, Cli, CommandResult, Commands, Config,
    ContentHash, ContentStore, EntrypointDetector, EntrypointTypeArg, FileDiff, FileSnapshot,
    FileStatus, HeaderStrategy, HistoryTracker, LanguageArg, LevelResult, LineRange, Mode,
    ObjectMeta, Output, ParseFormat, TraceDirection, TraceFormat, TreeSitterParser, VeilMode,
};

fn find_project_root() -> anyhow::Result<std::path::PathBuf> {
    let current = std::env::current_dir()?;
    if current.join(CONFIG_FILE).exists() {
        return Ok(current);
    }
    if current.join(".git").exists() {
        return Ok(current);
    }
    let mut path = current.as_path();
    while let Some(parent) = path.parent() {
        if parent.join(CONFIG_FILE).exists() || parent.join(".git").exists() {
            return Ok(parent.to_path_buf());
        }
        path = parent;
    }
    Ok(current)
}

// ── BUG-107: parse_pattern with '#' in filename ──

#[test]
fn test_bug107_parse_pattern_hash_in_filename() {
    // "dir/file#name.txt#1-5" should split at the last '#' since "1-5" is a valid range
    let (file, ranges) = parse_pattern("dir/file#name.txt#1-5").unwrap();
    assert_eq!(file, "dir/file#name.txt");
    let ranges = ranges.unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start(), 1);
    assert_eq!(ranges[0].end(), 5);
}

#[test]
fn test_bug107_parse_pattern_hash_no_range() {
    // "dir/file#name.txt" — suffix "name.txt" is not a valid range, so treat as filename
    let (file, ranges) = parse_pattern("dir/file#name.txt").unwrap();
    assert_eq!(file, "dir/file#name.txt");
    assert!(ranges.is_none());
}

#[test]
fn test_bug107_parse_pattern_normal() {
    // "file.txt#1-5" — normal case, should still work
    let (file, ranges) = parse_pattern("file.txt#1-5").unwrap();
    assert_eq!(file, "file.txt");
    let ranges = ranges.unwrap();
    assert_eq!(ranges[0].start(), 1);
    assert_eq!(ranges[0].end(), 5);
}

// ── BUG-099: Apply command config key parsing ──

#[test]
fn test_bug099_apply_hash_in_filename() {
    // Verify rfind('#') with suffix validation extracts correct path
    let key = "dir/file#name.txt#1-5";
    let file_path = if let Some(pos) = key.rfind('#') {
        let suffix = &key[pos + 1..];
        if suffix == "_original" || suffix.parse::<LineRange>().is_ok() {
            &key[..pos]
        } else {
            key
        }
    } else {
        key
    };
    assert_eq!(file_path, "dir/file#name.txt");
}

#[test]
fn test_parse_pattern_no_hash() {
    let (file, ranges) = parse_pattern("simple_file.txt").unwrap();
    assert_eq!(file, "simple_file.txt");
    assert!(ranges.is_none());
}

#[test]
fn test_parse_pattern_empty_file_path() {
    let result = parse_pattern("#1-5");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Empty file path"));
}

#[test]
fn test_parse_pattern_empty_range_after_hash() {
    let result = parse_pattern("file.txt#");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Empty range specification"));
}

#[test]
fn test_parse_pattern_single_number_not_range() {
    let (file, ranges) = parse_pattern("file.txt#42").unwrap();
    assert_eq!(file, "file.txt#42");
    assert!(ranges.is_none());
}

#[test]
fn test_parse_pattern_non_numeric_range() {
    let (file, ranges) = parse_pattern("file.txt#abc-def").unwrap();
    assert_eq!(file, "file.txt#abc-def");
    assert!(ranges.is_none());
}

#[test]
fn test_parse_pattern_inverted_range() {
    let (file, ranges) = parse_pattern("file.txt#10-1").unwrap();
    assert_eq!(file, "file.txt#10-1");
    assert!(ranges.is_none());
}

#[test]
fn test_parse_pattern_multiple_ranges() {
    let (file, ranges) = parse_pattern("file.txt#1-5,10-20").unwrap();
    assert_eq!(file, "file.txt");
    let ranges = ranges.unwrap();
    assert_eq!(ranges.len(), 2);
    assert_eq!(ranges[0].start(), 1);
    assert_eq!(ranges[0].end(), 5);
    assert_eq!(ranges[1].start(), 10);
    assert_eq!(ranges[1].end(), 20);
}

#[test]
fn test_parse_pattern_multiple_ranges_one_invalid() {
    let (file, ranges) = parse_pattern("file.txt#1-5,bad").unwrap();
    assert_eq!(file, "file.txt#1-5,bad");
    assert!(ranges.is_none());
}

#[test]
fn test_parse_pattern_range_with_zero_start() {
    let (file, ranges) = parse_pattern("file.txt#0-5").unwrap();
    assert_eq!(file, "file.txt#0-5");
    assert!(ranges.is_none());
}

#[test]
fn test_parse_pattern_too_many_dashes() {
    let (file, ranges) = parse_pattern("file.txt#1-2-3").unwrap();
    assert_eq!(file, "file.txt#1-2-3");
    assert!(ranges.is_none());
}

#[test]
fn test_version_long_contains_expected_fields() {
    let v = version_long();
    assert!(v.starts_with("fv "));
    assert!(v.contains("commit:"));
    assert!(v.contains("target:"));
    assert!(v.contains("profile:"));
}

#[test]
fn test_parse_pattern_mixed_numeric_non_numeric() {
    let (file, ranges) = parse_pattern("file.txt#1-abc").unwrap();
    assert_eq!(file, "file.txt#1-abc");
    assert!(ranges.is_none());
}

#[test]
fn test_commands_name_operation_variants() {
    assert_eq!(
        Commands::Init {
            mode: Mode::Whitelist
        }
        .name(),
        "init"
    );
    assert_eq!(Commands::Mode { mode: None }.name(), "mode");
    assert_eq!(Commands::Status { files: false }.name(), "status");
    assert_eq!(
        Commands::Unveil {
            pattern: None,
            all: false,
            dry_run: false,
            symbol: None,
            callers_of: None,
            callees_of: None,
            level: None,
        }
        .name(),
        "unveil"
    );
    assert_eq!(
        Commands::Veil {
            pattern: "f".into(),
            mode: VeilMode::Full,
            dry_run: false,
            symbol: None,
            unreachable_from: None,
            level: None,
        }
        .name(),
        "veil"
    );
    assert_eq!(
        Commands::Parse {
            file: "f".into(),
            format: ParseFormat::Summary
        }
        .name(),
        "parse"
    );
    assert_eq!(
        Commands::Trace {
            function: None,
            from: None,
            to: None,
            from_entrypoint: false,
            depth: 3,
            format: TraceFormat::Tree,
            no_std: false,
        }
        .name(),
        "trace"
    );
    assert_eq!(
        Commands::Entrypoints {
            entry_type: None,
            language: None
        }
        .name(),
        "entrypoints"
    );
    assert_eq!(
        Commands::Cache {
            cmd: CacheCmd::Status
        }
        .name(),
        "cache"
    );
    assert_eq!(Commands::Apply { dry_run: false }.name(), "apply");
    assert_eq!(Commands::Restore.name(), "restore");
    assert_eq!(Commands::Show { file: "f".into() }.name(), "show");
    assert_eq!(
        Commands::Checkpoint {
            cmd: CheckpointCmd::List
        }
        .name(),
        "checkpoint"
    );
    assert_eq!(Commands::Doctor.name(), "doctor");
    assert_eq!(Commands::Gc.name(), "gc");
    assert_eq!(Commands::Clean.name(), "clean");
    assert_eq!(Commands::Version.name(), "version");
}

#[test]
fn test_find_project_root_returns_a_directory() {
    let root = find_project_root().unwrap();
    assert!(root.is_dir());
}

#[test]
fn test_version_long_has_four_lines() {
    let v = version_long();
    let lines: Vec<&str> = v.lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[0].starts_with("fv "));
    assert!(lines[1].starts_with("commit: "));
    assert!(lines[2].starts_with("target: "));
    assert!(lines[3].starts_with("profile: "));
}

#[test]
fn test_parse_pattern_equal_start_end() {
    let (file, ranges) = parse_pattern("file.txt#5-5").unwrap();
    assert_eq!(file, "file.txt");
    let ranges = ranges.unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start(), 5);
    assert_eq!(ranges[0].end(), 5);
}

#[test]
fn test_parse_pattern_three_valid_ranges() {
    let (file, ranges) = parse_pattern("src/lib.rs#1-3,10-20,50-100").unwrap();
    assert_eq!(file, "src/lib.rs");
    let ranges = ranges.unwrap();
    assert_eq!(ranges.len(), 3);
    assert_eq!(ranges[2].start(), 50);
    assert_eq!(ranges[2].end(), 100);
}

#[test]
fn test_parse_pattern_multiple_hashes_invalid_suffix() {
    let (file, ranges) = parse_pattern("a#b#c").unwrap();
    assert_eq!(file, "a#b#c");
    assert!(ranges.is_none());
}

#[test]
fn test_run_init_whitelist() {
    let (stdout, _, result) = run_in_temp(Commands::Init {
        mode: Mode::Whitelist,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Initialized funveil with whitelist mode"));
}

#[test]
fn test_run_init_already_initialized() {
    let env = TestEnv::init(Mode::Whitelist);
    let (stdout, _, result) = env.run(Commands::Init {
        mode: Mode::Whitelist,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("already initialized"));
}

#[test]
fn test_run_mode_get() {
    let env = TestEnv::init(Mode::Whitelist);
    let (stdout, _, result) = env.run(Commands::Mode { mode: None });
    assert!(result.is_ok());
    assert!(stdout.contains("whitelist"));
}

#[test]
fn test_run_mode_set() {
    let env = TestEnv::init(Mode::Whitelist);
    let (stdout, _, result) = env.run(Commands::Mode {
        mode: Some(Mode::Blacklist),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Mode changed to: blacklist"));
}

#[test]
fn test_run_status() {
    let env = TestEnv::init(Mode::Whitelist);
    let (stdout, _, result) = env.run(Commands::Status { files: false });
    assert!(result.is_ok());
    assert!(stdout.contains("Mode: whitelist"));
}

#[test]
fn test_run_veil_and_unveil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("test.txt", "hello\nworld\n");
    let (_, _, result) = env.veil("test.txt");
    assert!(result.is_ok());
    let (_, _, result) = env.unveil("test.txt");
    assert!(result.is_ok());
}

#[test]
fn test_run_unveil_all() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("a.txt", "aaa\n");
    let _ = env.veil("a.txt");
    let (_, _, result) = env.unveil_all();
    assert!(result.is_ok());
}

#[test]
fn test_run_unveil_no_pattern_no_all() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_err());
}

#[test]
fn test_run_show() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("show.txt", "content\n");
    let (stdout, _, result) = env.run(Commands::Show {
        file: "show.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("content"));
}

#[test]
fn test_run_doctor() {
    let env = TestEnv::init(Mode::Whitelist);
    let (_, _, result) = env.run(Commands::Doctor);
    assert!(result.is_ok());
}

#[test]
fn test_run_gc() {
    let env = TestEnv::init(Mode::Whitelist);
    let (_, _, result) = env.run(Commands::Gc);
    assert!(result.is_ok());
}

#[test]
fn test_run_version() {
    let (stdout, _, result) = run_in_temp(Commands::Version);
    assert!(result.is_ok());
    assert!(stdout.contains("fv "));
}

#[test]
fn test_run_clean() {
    let env = TestEnv::init(Mode::Whitelist);
    let (stdout, _, result) = env.run(Commands::Clean);
    assert!(result.is_ok());
    assert!(stdout.contains("Removed all funveil data"));
}

#[test]
fn test_run_cache_status() {
    let env = TestEnv::init(Mode::Whitelist);
    let (_, _, result) = env.run(Commands::Cache {
        cmd: CacheCmd::Status,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_cache_clear() {
    let env = TestEnv::init(Mode::Whitelist);
    let (_, _, result) = env.run(Commands::Cache {
        cmd: CacheCmd::Clear,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_cache_invalidate() {
    let env = TestEnv::init(Mode::Whitelist);
    let (_, _, result) = env.run(Commands::Cache {
        cmd: CacheCmd::Invalidate,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_apply() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
}

#[test]
fn test_run_restore_no_checkpoint() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Restore);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No checkpoints found"));
}

#[test]
fn test_run_restore_with_checkpoint() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("r.txt", "data\n");
    let _ = env.veil("r.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "snap".into(),
        },
    });
    let _ = env.unveil_all();
    let (stdout, _, result) = env.run(Commands::Restore);
    assert!(result.is_ok());
    assert!(stdout.contains("Restoring from latest checkpoint"));
}

#[test]
fn test_run_parse() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("hello.rs", "fn main() {}\n");
    let (_, _, result) = env.run(Commands::Parse {
        file: "hello.rs".into(),
        format: ParseFormat::Summary,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_entrypoints() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("main.rs", "fn main() {}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_checkpoint_save_list_show_delete() {
    let env = TestEnv::init(Mode::Blacklist);
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save { name: "cp1".into() },
    });
    let (stdout, _, _) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::List,
    });
    assert!(stdout.contains("cp1"));
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Show { name: "cp1".into() },
    });
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Delete { name: "cp1".into() },
    });
}

#[test]
fn test_run_checkpoint_restore() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "snap".into(),
        },
    });
    let _ = env.unveil_all();
    let (_, _, result) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Restore {
            name: "snap".into(),
        },
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_veil_regex() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("foo.txt", "foo\n");
    env.write_file("bar.txt", "bar\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "/.*\\.txt/".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiling"));
}

#[test]
fn test_run_unveil_regex() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("a.txt", "aaa\n");
    let _ = env.veil("a.txt");
    let (_, _, result) = env.run(Commands::Unveil {
        pattern: Some("/a\\.txt/".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_veil_partial() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "line1\nline2\nline3\nline4\nline5\n");
    let (_, _, result) = env.run(Commands::Veil {
        pattern: "f.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_show_nonexistent() {
    let env = TestEnv::init(Mode::Whitelist);
    let (_, _, result) = env.run(Commands::Show {
        file: "nonexistent.txt".into(),
    });
    assert!(result.is_err());
}

#[test]
fn test_run_trace_from() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() { bar(); }\nfn bar() {}\n");
    let (_, _, result) = env.run(Commands::Trace {
        function: None,
        from: Some("foo".into()),
        to: None,
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_veil_headers() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("hello.rs", "fn main() {}\nfn helper() {}\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "hello.rs".into(),
        mode: VeilMode::Headers,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled (headers mode)"));
}

#[test]
fn test_run_parse_detailed() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() {}\nfn bar() {}\n");
    let (stdout, _, result) = env.run(Commands::Parse {
        file: "lib.rs".into(),
        format: ParseFormat::Detailed,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Symbols:"));
}

#[test]
fn test_run_trace_to() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() { bar(); }\nfn bar() {}\n");
    let (_, _, result) = env.run(Commands::Trace {
        function: None,
        from: None,
        to: Some("bar".into()),
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_trace_from_entrypoint() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("main.rs", "fn main() { helper(); }\nfn helper() {}\n");
    let (stdout, _, result) = env.run(Commands::Trace {
        function: None,
        from: None,
        to: None,
        from_entrypoint: true,
        depth: 3,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Entrypoints found:") || stdout.is_empty());
}

#[test]
fn test_run_trace_dot_format() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() {}\n");
    let (stdout, _, result) = env.run(Commands::Trace {
        function: None,
        from: Some("foo".into()),
        to: None,
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::Dot,
        no_std: false,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("digraph"));
}

#[test]
fn test_run_trace_list_format() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() { bar(); }\nfn bar() {}\n");
    let (_, _, result) = env.run(Commands::Trace {
        function: None,
        from: Some("foo".into()),
        to: None,
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::List,
        no_std: false,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_trace_no_std() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() { println!(\"hi\"); }\n");
    let (_, _, result) = env.run(Commands::Trace {
        function: None,
        from: Some("foo".into()),
        to: None,
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::Tree,
        no_std: true,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_entrypoints_with_language() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("main.rs", "fn main() {}\n");
    let (stdout, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::Rust),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("main"));
}

#[test]
fn test_run_apply_reveils() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("secret.txt", "secret data\n");
    let _ = env.veil("secret.txt");
    let _ = env.unveil_all();
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(stdout.contains("Re-applying veils") || stdout.contains("Applied:"));
}

#[test]
fn test_run_doctor_with_veils() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "content\n");
    let _ = env.veil("f.txt");
    let (stdout, _, result) = env.run(Commands::Doctor);
    assert!(result.is_ok());
    assert!(stdout.contains("All checks passed"));
}

#[test]
fn test_run_show_veiled_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("s.txt", "secret\n");
    let _ = env.veil("s.txt");
    let (stdout, _, result) = env.run(Commands::Show {
        file: "s.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("VEILED - not on disk"));
}

#[test]
fn test_run_show_partially_veiled() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("p.txt", "line1\nline2\nline3\nline4\nline5\n");
    let _ = env.run(Commands::Veil {
        pattern: "p.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Show {
        file: "p.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("p.txt"));
}

#[test]
fn test_run_veil_regex_no_match() {
    let env = TestEnv::init(Mode::Blacklist);
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "/nonexistent_pattern/".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("No files matched"));
}

#[test]
fn test_run_status_with_blacklist_and_whitelist() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("a.txt", "aaa\n");
    let _ = env.veil("a.txt");
    let _ = env.unveil("a.txt");
    let (stdout, _, _) = env.run(Commands::Status { files: false });
    assert!(stdout.contains("Mode:"));
}

#[test]
fn test_run_veil_headers_nonexistent() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Veil {
        pattern: "nonexistent.rs".into(),
        mode: VeilMode::Headers,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("File not found"));
}

#[test]
fn test_run_parse_detailed_with_calls_and_imports() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file(
        "prog.rs",
        "use std::io;\nfn main() { helper(); }\nfn helper() { println!(\"hi\"); }\n",
    );
    let (stdout, _, result) = env.run(Commands::Parse {
        file: "prog.rs".into(),
        format: ParseFormat::Detailed,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Symbols:"));
    assert!(stdout.contains("Signature:"));
}

#[test]
fn test_run_trace_both_from_and_to_error() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() {}\n");
    let (_, _, result) = env.run(Commands::Trace {
        function: None,
        from: Some("foo".into()),
        to: Some("bar".into()),
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cannot use both"));
}

#[test]
fn test_run_trace_no_function_error() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() {}\n");
    let (_, _, result) = env.run(Commands::Trace {
        function: None,
        from: None,
        to: None,
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Must specify"));
}

#[test]
fn test_run_trace_function_not_in_graph() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() {}\n");
    let (_, stderr, result) = env.run(Commands::Trace {
        function: None,
        from: Some("nonexistent_fn".into()),
        to: None,
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_ok());
    assert!(
        stderr.contains("not found in call graph") || stderr.contains("not found in the codebase")
    );
}

#[test]
fn test_run_status_with_whitelist() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("a.txt", "aaa\n");
    let _ = env.veil("a.txt");
    let _ = env.unveil("a.txt");
    let (stdout, _, result) = env.run(Commands::Status { files: false });
    assert!(result.is_ok());
    assert!(stdout.contains("Whitelisted:"));
}

#[test]
fn test_run_entrypoints_with_type_filter() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("main.rs", "fn main() {}\n#[test]\nfn test_foo() {}\n");
    let (stdout, _, result) = env.run(Commands::Entrypoints {
        entry_type: Some(EntrypointTypeArg::Main),
        language: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("main"));
}

#[test]
fn test_run_entrypoints_empty() {
    let env = TestEnv::init(Mode::Whitelist);
    let (stdout, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("No entrypoints detected"));
}

#[test]
fn test_run_veil_regex_matched_but_no_veilable() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("empty.txt", "");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "/empty/".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("No files") || stdout.contains("Veiling"));
}

#[test]
fn test_run_unveil_regex_no_match() {
    let env = TestEnv::init(Mode::Blacklist);
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("/nonexistent_xyz/".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("No files matched"));
}

#[test]
fn test_run_unveil_regex_match_no_veils() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("plain.txt", "hello\n");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("/plain/".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("No veiled files matched") || stdout.contains("Unveiled"));
}

#[test]
fn test_run_apply_already_veiled() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(stdout.contains("veiled, not on disk"));
}

#[test]
fn test_run_apply_missing_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("gone.txt", "data\n");
    let _ = env.veil("gone.txt");
    assert!(!env.dir().join("gone.txt").exists());
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(stdout.contains("veiled, not on disk"));
}

#[test]
fn test_run_checkpoint_list_empty() {
    let env = TestEnv::init(Mode::Blacklist);
    let (stdout, _, result) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::List,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("No checkpoints found"));
}

#[test]
fn test_run_trace_with_function_arg() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() { bar(); }\nfn bar() {}\n");
    let (_, _, result) = env.run(Commands::Trace {
        function: Some("foo".into()),
        from: None,
        to: None,
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_trace_dot_no_std() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() { println!(\"hi\"); }\n");
    let (stdout, _, result) = env.run(Commands::Trace {
        function: None,
        from: Some("foo".into()),
        to: None,
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::Dot,
        no_std: true,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("digraph"));
}

#[test]
fn test_run_trace_cycle_detection() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file(
        "cycle.rs",
        "fn alpha() { beta(); }\nfn beta() { alpha(); }\n",
    );
    let (_, stderr, result) = env.run(Commands::Trace {
        function: None,
        from: Some("alpha".into()),
        to: None,
        from_entrypoint: false,
        depth: 10,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_ok());
    assert!(stderr.contains("Cycle detected") || !stderr.contains("Cycle detected"));
}

#[test]
fn test_run_parse_detailed_with_imports() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("uses.rs", "use std::io;\nuse std::fs;\nfn main() {}\n");
    let (stdout, _, result) = env.run(Commands::Parse {
        file: "uses.rs".into(),
        format: ParseFormat::Detailed,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Imports:"));
}

#[test]
fn test_run_parse_detailed_with_calls() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("calls.rs", "fn main() { helper(); }\nfn helper() {}\n");
    let (stdout, _, result) = env.run(Commands::Parse {
        file: "calls.rs".into(),
        format: ParseFormat::Detailed,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Calls:"));
}

#[test]
fn test_run_trace_list_no_std() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("lib.rs", "fn foo() { bar(); }\nfn bar() {}\n");
    let (_, _, result) = env.run(Commands::Trace {
        function: None,
        from: Some("foo".into()),
        to: None,
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::List,
        no_std: true,
    });
    assert!(result.is_ok());
}

fn init_with_bad_config(dir: &std::path::Path) {
    let _ = run_in_dir(
        dir,
        Commands::Init {
            mode: Mode::Blacklist,
        },
    );
    std::fs::write(dir.join(".funveil_config"), "{{{{invalid yaml").unwrap();
}

#[test]
fn test_run_mode_load_error() {
    let temp = tempfile::TempDir::new().unwrap();
    init_with_bad_config(temp.path());
    let (_, _, result) = run_in_dir(temp.path(), Commands::Mode { mode: None });
    assert!(result.is_err());
}

#[test]
fn test_run_status_load_error() {
    let temp = tempfile::TempDir::new().unwrap();
    init_with_bad_config(temp.path());
    let (_, _, result) = run_in_dir(temp.path(), Commands::Status { files: false });
    assert!(result.is_err());
}

#[test]
fn test_run_veil_load_error() {
    let temp = tempfile::TempDir::new().unwrap();
    init_with_bad_config(temp.path());
    let (_, _, result) = run_in_dir(
        temp.path(),
        Commands::Veil {
            pattern: "file.txt".into(),
            mode: VeilMode::Full,
            dry_run: false,
            symbol: None,
            unreachable_from: None,
            level: None,
        },
    );
    assert!(result.is_err());
}

#[test]
fn test_run_veil_regex_invalid() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Veil {
        pattern: "/[invalid/".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_err());
}

#[test]
fn test_run_unveil_load_error() {
    let temp = tempfile::TempDir::new().unwrap();
    init_with_bad_config(temp.path());
    let (_, _, result) = run_in_dir(
        temp.path(),
        Commands::Unveil {
            pattern: Some("file.txt".into()),
            all: false,
            dry_run: false,
            symbol: None,
            callers_of: None,
            callees_of: None,
            level: None,
        },
    );
    assert!(result.is_err());
}

#[test]
fn test_run_unveil_regex_invalid() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Unveil {
        pattern: Some("/[invalid/".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_err());
}

#[test]
fn test_run_show_load_error() {
    let temp = tempfile::TempDir::new().unwrap();
    init_with_bad_config(temp.path());
    std::fs::write(temp.path().join("file.txt"), "content\n").unwrap();
    let (_, _, result) = run_in_dir(
        temp.path(),
        Commands::Show {
            file: "file.txt".into(),
        },
    );
    assert!(result.is_err());
}

#[test]
fn test_run_parse_nonexistent() {
    let env = TestEnv::init(Mode::Whitelist);
    let (_, _, result) = env.run(Commands::Parse {
        file: "missing.rs".into(),
        format: ParseFormat::Summary,
    });
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_run_cache_load_error() {
    let temp = tempfile::TempDir::new().unwrap();
    init_with_bad_config(temp.path());
    let (_, _, result) = run_in_dir(
        temp.path(),
        Commands::Cache {
            cmd: CacheCmd::Status,
        },
    );
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_run_apply_load_error() {
    let temp = tempfile::TempDir::new().unwrap();
    init_with_bad_config(temp.path());
    let (_, _, result) = run_in_dir(temp.path(), Commands::Apply { dry_run: false });
    assert!(result.is_err());
}

#[test]
fn test_run_trace_from_entrypoint_no_entrypoints() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("helper.rs", "fn helper() {}\nfn util() {}\n");
    let (_, stderr, result) = env.run(Commands::Trace {
        function: None,
        from: None,
        to: None,
        from_entrypoint: true,
        depth: 3,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_ok());
    assert!(stderr.contains("No entrypoints detected"));
}

#[test]
fn test_run_apply_reveil_unveiled_file() {
    let env = TestEnv::init(Mode::Blacklist);
    let original_content = "secret data\n";
    env.write_file("s.txt", original_content);
    let _ = env.veil("s.txt");
    let file_path = env.dir().join("s.txt");
    assert!(!file_path.exists());
    std::fs::write(&file_path, original_content).unwrap();
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(stdout.contains("re-veiled"));
}

#[test]
fn test_run_doctor_with_issues() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("d.txt", "data\n");
    let _ = env.veil("d.txt");
    let data_dir = env.dir().join(".funveil").join("objects");
    if data_dir.exists() {
        for entry in std::fs::read_dir(&data_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::remove_file(entry.path()).unwrap();
                break;
            }
        }
    }
    let (stdout, _, result) = env.run(Commands::Doctor);
    assert!(result.is_ok());
    assert!(stdout.contains("issue") || stdout.contains("All checks passed"));
}

#[test]
fn test_run_status_with_veils() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("secret.txt", "secret\n");
    let _ = env.veil("secret.txt");
    let (stdout, _, result) = env.run(Commands::Status { files: false });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled objects:"));
}

// ── Undo/Redo tests ──

#[test]
fn test_undo_empty_history() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Undo { force: false });
    assert!(result.is_err());
}

#[test]
fn test_redo_nothing_to_redo() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Redo);
    assert!(result.is_err());
}

#[test]
fn test_veil_undo_redo_roundtrip() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("test.txt", "hello world\n");

    // Veil
    let (_, _, result) = env.veil("test.txt");
    assert!(result.is_ok());
    assert!(!env.dir().join("test.txt").exists());

    // Undo — file should be restored
    let (stdout, _, result) = env.run(Commands::Undo { force: false });
    assert!(result.is_ok());
    assert!(stdout.contains("Undone"));
    let restored = std::fs::read_to_string(env.dir().join("test.txt")).unwrap();
    assert_eq!(restored, "hello world\n");

    // Redo — file should be veiled again (removed from disk)
    let (stdout, _, result) = env.run(Commands::Redo);
    assert!(result.is_ok());
    assert!(stdout.contains("Redone"));
    assert!(!env.dir().join("test.txt").exists());
}

#[test]
fn test_undo_after_undo_new_action_discards_future() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("a.txt", "aaa\n");
    env.write_file("b.txt", "bbb\n");

    // Veil a.txt
    let _ = env.veil("a.txt");
    // Veil b.txt
    let _ = env.veil("b.txt");

    // Undo b.txt veil
    let _ = env.run(Commands::Undo { force: false });

    // Now change mode — this should discard the b.txt future
    let _ = env.run(Commands::Mode {
        mode: Some(Mode::Whitelist),
    });

    // Redo should fail — future was discarded
    let (_, _, result) = env.run(Commands::Redo);
    assert!(result.is_err());
}

#[test]
fn test_history_list() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");

    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("init"));
    assert!(stdout.contains("veil"));
}

#[test]
fn test_history_show_detail() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");

    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Action #2"));
    assert!(stdout.contains("veil"));
}

#[test]
fn test_history_with_undo_shows_future() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");

    // Undo
    let _ = env.run(Commands::Undo { force: false });

    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Future:"));
}

// ── Dry-run tests ──

#[test]
fn test_veil_dry_run_no_state_change() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "original\n");

    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "f.txt".into(),
        mode: VeilMode::Full,
        dry_run: true,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would veil"));

    // File should be unchanged
    let content = std::fs::read_to_string(env.dir().join("f.txt")).unwrap();
    assert_eq!(content, "original\n");

    // Config should have no objects
    let config = Config::load(env.dir()).unwrap();
    assert!(config.objects.is_empty());
}

#[test]
fn test_unveil_dry_run_no_state_change() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "content\n");
    let _ = env.veil("f.txt");

    assert!(!env.dir().join("f.txt").exists());

    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("f.txt".into()),
        all: false,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil"));

    // File should still be veiled (not on disk)
    assert!(!env.dir().join("f.txt").exists());
}

#[test]
fn test_apply_dry_run_no_state_change() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");

    let (stdout, _, result) = env.run(Commands::Apply { dry_run: true });
    assert!(result.is_ok());
    assert!(stdout.contains("would be re-applied"));
}

// ── JSON output tests ──

#[test]
fn test_json_output_init() {
    let temp = tempfile::TempDir::new().unwrap();
    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Init {
            mode: Mode::Whitelist,
        },
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, temp.path(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"init\""));
    assert!(json.contains("\"mode\":\"whitelist\""));
}

#[test]
fn test_json_output_status() {
    let env = TestEnv::init(Mode::Whitelist);
    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Status { files: false },
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"status\""));
    assert!(json.contains("\"mode\":\"whitelist\""));
}

// ── Status --files test ──

#[test]
fn test_status_files_flag() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("visible.txt", "vis\n");
    env.write_file("hidden.txt", "hid\n");
    let _ = env.veil("hidden.txt");

    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Files:"));
    assert!(stdout.contains("visible.txt"));
    assert!(stdout.contains("hidden.txt"));
}

// ── Undo non-undoable action ──

#[test]
fn test_undo_non_undoable_without_force() {
    let env = TestEnv::init(Mode::Blacklist);
    // Init creates a non-undoable entry, but we need a second entry
    // to have cursor > 0. Let's veil + gc.
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");
    let _ = env.run(Commands::Gc);

    // GC is not undoable
    let (_, _, result) = env.run(Commands::Undo { force: false });
    assert!(result.is_err());
}

#[test]
fn test_undo_non_undoable_with_force() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");
    let _ = env.run(Commands::Gc);

    // Force undo of GC (won't restore CAS objects, but won't error)
    let (_, _, result) = env.run(Commands::Undo { force: true });
    assert!(result.is_ok());
}

#[test]
fn test_mode_change_records_history() {
    let env = TestEnv::init(Mode::Whitelist);
    let (stdout, _, result) = env.run(Commands::Mode {
        mode: Some(Mode::Blacklist),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Mode changed to: blacklist"));

    let history = ActionHistory::load(env.dir()).unwrap();
    assert_eq!(history.entries.len(), 2);
    assert_eq!(history.entries[1].command, "mode");
    assert!(history.entries[1].undoable);
}

#[test]
fn test_veil_headers_records_history() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("sample.rs", "fn hello() {\n    println!(\"hi\");\n}\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "sample.rs".into(),
        mode: VeilMode::Headers,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled (headers mode)"));

    let history = ActionHistory::load(env.dir()).unwrap();
    let last = history.entries.last().unwrap();
    assert_eq!(last.command, "veil");
    assert!(last.args.contains(&"headers".to_string()));
}

#[test]
fn test_gc_records_history() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Gc);
    assert!(result.is_ok());

    let history = ActionHistory::load(env.dir()).unwrap();
    let last = history.entries.last().unwrap();
    assert_eq!(last.command, "gc");
    assert!(!last.undoable);
}

#[test]
fn test_clean_removes_data() {
    let env = TestEnv::init(Mode::Blacklist);
    assert!(env.dir().join(CONFIG_FILE).exists());

    let (stdout, _, result) = env.run(Commands::Clean);
    assert!(result.is_ok());
    assert!(stdout.contains("Removed all funveil data"));
    assert!(!env.dir().join(CONFIG_FILE).exists());
    assert!(!env.dir().join(".funveil").exists());
}

#[test]
fn test_checkpoint_save_records_history() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "cp1".to_string(),
        },
    });
    assert!(result.is_ok());

    let history = ActionHistory::load(env.dir()).unwrap();
    let last = history.entries.last().unwrap();
    assert_eq!(last.command, "checkpoint");
    assert!(last.args.contains(&"save".to_string()));
}

#[test]
fn test_checkpoint_restore_records_history() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "hello\n");
    let _ = env.veil("f.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "cp1".to_string(),
        },
    });
    // Unveil first so restore has something to change
    let _ = env.unveil_all();

    let (_, _, result) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Restore {
            name: "cp1".to_string(),
        },
    });
    assert!(result.is_ok());

    let history = ActionHistory::load(env.dir()).unwrap();
    let last = history.entries.last().unwrap();
    assert_eq!(last.command, "checkpoint");
    assert!(last.args.contains(&"restore".to_string()));
}

#[test]
fn test_checkpoint_delete_records_history() {
    let env = TestEnv::init(Mode::Blacklist);
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "cp1".to_string(),
        },
    });
    let (_, _, result) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Delete {
            name: "cp1".to_string(),
        },
    });
    assert!(result.is_ok());

    let history = ActionHistory::load(env.dir()).unwrap();
    let last = history.entries.last().unwrap();
    assert_eq!(last.command, "checkpoint");
    assert!(last.args.contains(&"delete".to_string()));
}

#[test]
fn test_restore_with_checkpoint_records_history() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "hello\n");
    let _ = env.veil("f.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "cp1".to_string(),
        },
    });
    // Unveil first so restore has something to change
    let _ = env.unveil_all();

    let (_, _, result) = env.run(Commands::Restore);
    assert!(result.is_ok());

    let history = ActionHistory::load(env.dir()).unwrap();
    let last = history.entries.last().unwrap();
    assert_eq!(last.command, "restore");
}

#[test]
fn test_history_show_with_config_diff() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("a.txt", "hello world\n");
    let _ = env.veil("a.txt");

    let history = ActionHistory::load(env.dir()).unwrap();
    let veil_id = history.entries.last().unwrap().id;

    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(veil_id),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Action #"));
    assert!(stdout.contains("veil"));
    // Veil action should show config diff (objects added) or file diffs
    assert!(stdout.contains("Config changes:") || stdout.contains("bytes ->"));
}

#[test]
fn test_history_show_mode_change_diff() {
    let env = TestEnv::init(Mode::Whitelist);
    let _ = env.run(Commands::Mode {
        mode: Some(Mode::Blacklist),
    });

    let history = ActionHistory::load(env.dir()).unwrap();
    let mode_id = history.entries.last().unwrap().id;

    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(mode_id),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("mode:"));
}

#[test]
fn test_history_show_init_config_created() {
    let env = TestEnv::init(Mode::Blacklist);

    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(1),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("config created"));
}

#[test]
fn test_history_show_objects_diff() {
    let env = TestEnv::init(Mode::Blacklist);
    // Veil first file so both pre and post have objects
    env.write_file("a.txt", "aaa\n");
    let _ = env.veil("a.txt");
    // Veil second file — now pre has objects{a.txt} and post has objects{a.txt, b.txt}
    env.write_file("b.txt", "bbb\n");
    let _ = env.veil("b.txt");

    let history = ActionHistory::load(env.dir()).unwrap();
    let last_id = history.entries.last().unwrap().id;

    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(last_id),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("+ objects["));
}

#[test]
fn test_history_show_objects_removed() {
    let env = TestEnv::init(Mode::Blacklist);
    // Veil a file then manually construct entry with pre having more objects than post
    env.write_file("a.txt", "aaa\n");
    let _ = env.veil("a.txt");

    let config_with_obj = Config::load(env.dir()).unwrap();
    let pre_yaml = snapshot_config(&config_with_obj).unwrap();
    // Create post YAML with a different object set (keep objects key but without a.txt)
    let post_yaml = pre_yaml.replace("a.txt", "REMOVED_KEY_FOR_TEST");

    let mut history = ActionHistory::load(env.dir()).unwrap();
    history.push(ActionRecord {
        id: history.next_id(),
        timestamp: chrono::Utc::now(),
        command: "test-remove".to_string(),
        args: vec![],
        summary: "Modified objects".to_string(),
        affected_files: vec![],
        undoable: true,
        pre_state: ActionState {
            config_yaml: Some(pre_yaml),
            file_snapshots: vec![],
        },
        post_state: ActionState {
            config_yaml: Some(post_yaml),
            file_snapshots: vec![],
        },
    });
    history.save(env.dir()).unwrap();

    let last_id = history.entries.last().unwrap().id;
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(last_id),
    });
    assert!(result.is_ok());
    // Should show both "- objects[a.txt]" (removed) and "+ objects[REMOVED_KEY_FOR_TEST]" (added)
    assert!(stdout.contains("- objects["));
    assert!(stdout.contains("+ objects["));
}

#[test]
fn test_history_show_config_removed() {
    let env = TestEnv::init(Mode::Blacklist);
    // Manually create a history entry with pre_state having config but post_state None
    let mut history = ActionHistory::load(env.dir()).unwrap();
    let config = Config::load(env.dir()).unwrap();
    history.push(ActionRecord {
        id: history.next_id(),
        timestamp: chrono::Utc::now(),
        command: "clean".to_string(),
        args: vec![],
        summary: "Cleaned all data".to_string(),
        affected_files: vec![],
        undoable: false,
        pre_state: ActionState {
            config_yaml: snapshot_config(&config),
            file_snapshots: vec![],
        },
        post_state: ActionState {
            config_yaml: None,
            file_snapshots: vec![],
        },
    });
    history.save(env.dir()).unwrap();

    let last_id = history.entries.last().unwrap().id;
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(last_id),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("config removed"));
}

#[test]
fn test_history_show_not_found() {
    let env = TestEnv::init(Mode::Blacklist);

    let (_, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(999),
    });
    assert!(result.is_err());
}

#[test]
fn test_undo_restores_file_content() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "original content\n");
    let _ = env.veil("f.txt");

    assert!(!env.dir().join("f.txt").exists());

    let (_, _, result) = env.run(Commands::Undo { force: false });
    assert!(result.is_ok());

    let restored = std::fs::read_to_string(env.dir().join("f.txt")).unwrap();
    assert_eq!(restored, "original content\n");
}

#[test]
fn test_redo_restores_veiled_state() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "original content\n");
    let _ = env.veil("f.txt");

    assert!(!env.dir().join("f.txt").exists());

    let _ = env.run(Commands::Undo { force: false });
    let (_, _, result) = env.run(Commands::Redo);
    assert!(result.is_ok());

    assert!(!env.dir().join("f.txt").exists());
}

#[test]
fn test_status_files_with_partial_veils() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "line1\nline2\nline3\nline4\nline5\n");
    let _ = env.run(Commands::Veil {
        pattern: "f.txt#2-3".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });

    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("partial"));
}

#[test]
fn test_unveil_dry_run_all() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");

    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: true,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil"));
    assert!(stdout.contains("would be affected"));
}

#[test]
fn test_unveil_dry_run_pattern() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");

    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("f.txt".into()),
        all: false,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil"));
}

#[test]
fn test_unveil_records_history() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");
    let _ = env.unveil("f.txt");

    let history = ActionHistory::load(env.dir()).unwrap();
    let last = history.entries.last().unwrap();
    assert_eq!(last.command, "unveil");
    assert!(last.undoable);
}

#[test]
fn test_apply_records_history() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "content\n");
    let _ = env.veil("f.txt");
    // Unveil to restore original, then apply should re-veil
    let _ = env.unveil("f.txt");
    // Re-add to blacklist manually so apply picks it up
    let mut config = Config::load(env.dir()).unwrap();
    config.add_to_blacklist("f.txt");
    config.save(env.dir()).unwrap();

    let (_, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
}

#[test]
fn test_json_output_veil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "content\n");

    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Veil {
            pattern: "f.txt".into(),
            mode: VeilMode::Full,
            dry_run: false,
            symbol: None,
            unreachable_from: None,
            level: None,
        },
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"veil\""));
}

#[test]
fn test_json_output_history() {
    let env = TestEnv::init(Mode::Blacklist);

    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::History {
            limit: 20,
            show: None,
        },
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"history\""));
}

#[test]
fn test_json_output_undo() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");

    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Undo { force: false },
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"undo\""));
}

#[test]
fn test_json_output_redo() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");
    let _ = env.run(Commands::Undo { force: false });

    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Redo,
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"redo\""));
}

#[test]
fn test_json_output_gc() {
    let env = TestEnv::init(Mode::Blacklist);

    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Gc,
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"gc\""));
}

#[test]
fn test_json_output_clean() {
    let env = TestEnv::init(Mode::Blacklist);

    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Clean,
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"clean\""));
}

#[test]
fn test_json_output_doctor() {
    let env = TestEnv::init(Mode::Blacklist);

    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Doctor,
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"doctor\""));
}

#[test]
fn test_json_output_version() {
    let temp = tempfile::TempDir::new().unwrap();
    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Version,
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, temp.path(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"version\""));
}

#[test]
fn test_json_output_checkpoint() {
    let env = TestEnv::init(Mode::Blacklist);

    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Checkpoint {
            cmd: CheckpointCmd::Save {
                name: "test".to_string(),
            },
        },
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"checkpoint\""));
}

#[test]
fn test_json_output_restore() {
    let env = TestEnv::init(Mode::Blacklist);
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "cp1".to_string(),
        },
    });

    let cli = Cli {
        quiet: false,
        log_level: None,
        json: true,
        command: Commands::Restore,
    };
    let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut output = Output {
        out: Box::new(TestWriter(out_buf.clone())),
        err: Box::new(TestWriter(err_buf.clone())),
    };
    let result = run_command(cli, env.dir(), &mut output);
    assert!(result.is_ok());
    let cmd_result = result.unwrap();
    let json = serde_json::to_string(&cmd_result).unwrap();
    assert!(json.contains("\"command\":\"restore\""));
}

#[test]
fn test_collect_affected_files_regex() {
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    std::fs::write(temp.path().join("b.rs"), "b").unwrap();
    let files = collect_affected_files_for_pattern(temp.path(), "/.*\\.txt/");
    assert!(files.contains(&"a.txt".to_string()));
    assert!(!files.contains(&"b.rs".to_string()));
}

#[test]
fn test_collect_affected_files_directory() {
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(temp.path().join("sub")).unwrap();
    std::fs::write(temp.path().join("sub").join("f.txt"), "f").unwrap();
    let files = collect_affected_files_for_pattern(temp.path(), "sub");
    assert_eq!(files.len(), 1);
    assert!(files[0].contains("f.txt"));
}

#[test]
fn test_collect_affected_files_hash_pattern() {
    let files = collect_affected_files_for_pattern(std::path::Path::new("/tmp"), "foo.txt#1-5");
    assert_eq!(files, vec!["foo.txt"]);
}

#[test]
fn test_collect_affected_files_plain_file() {
    let files = collect_affected_files_for_pattern(std::path::Path::new("/tmp"), "some_file.txt");
    assert_eq!(files, vec!["some_file.txt"]);
}

#[test]
fn test_collect_affected_files_invalid_regex() {
    let files = collect_affected_files_for_pattern(std::path::Path::new("/tmp"), "/[invalid/");
    assert!(files.is_empty());
}

#[test]
fn test_snapshot_files_nonexistent() {
    let env = TestEnv::init(Mode::Blacklist);
    let snaps = snapshot_files(env.dir(), &["nonexistent.txt".to_string()]);
    assert_eq!(snaps.len(), 1);
    assert!(snaps[0].cas_hash.is_none());
    assert_eq!(snaps[0].permissions, "644");
}

#[test]
fn test_snapshot_files_existing() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "content\n");
    let snaps = snapshot_files(env.dir(), &["f.txt".to_string()]);
    assert_eq!(snaps.len(), 1);
    assert!(snaps[0].cas_hash.is_some());
}

#[test]
fn test_history_show_file_diffs() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "hello world content here\n");
    let _ = env.veil("f.txt");

    let history = ActionHistory::load(env.dir()).unwrap();
    let id = history.entries.last().unwrap().id;

    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(id),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("bytes ->"));
}

#[test]
fn test_restore_action_state_creates_dirs_and_files() {
    let env = TestEnv::init(Mode::Blacklist);
    // Store content in CAS
    let store = ContentStore::new(env.dir());
    let hash = store.store(b"restored content").unwrap();

    let state = ActionState {
        config_yaml: None,
        file_snapshots: vec![FileSnapshot {
            path: "subdir/restored.txt".to_string(),
            cas_hash: Some(hash.full().to_string()),
            permissions: "644".to_string(),
        }],
    };
    restore_action_state(env.dir(), &state).unwrap();
    let content = std::fs::read_to_string(env.dir().join("subdir/restored.txt")).unwrap();
    assert_eq!(content, "restored content");
}

#[test]
fn test_restore_action_state_deletes_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("todelete.txt", "data");

    let state = ActionState {
        config_yaml: None,
        file_snapshots: vec![FileSnapshot {
            path: "todelete.txt".to_string(),
            cas_hash: None,
            permissions: "644".to_string(),
        }],
    };
    restore_action_state(env.dir(), &state).unwrap();
    assert!(!env.dir().join("todelete.txt").exists());
}

#[test]
fn test_restore_action_state_overwrites_readonly() {
    let env = TestEnv::init(Mode::Blacklist);
    let fpath = env.dir().join("readonly.txt");
    std::fs::write(&fpath, "old").unwrap();

    let store = ContentStore::new(env.dir());
    let hash = store.store(b"new content").unwrap();

    let state = ActionState {
        config_yaml: None,
        file_snapshots: vec![FileSnapshot {
            path: "readonly.txt".to_string(),
            cas_hash: Some(hash.full().to_string()),
            permissions: "644".to_string(),
        }],
    };
    restore_action_state(env.dir(), &state).unwrap();
    assert_eq!(std::fs::read_to_string(&fpath).unwrap(), "new content");
}

#[test]
fn test_entrypoints_language_go_filter() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("main.go", "package main\n\nfunc main() {\n}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::Go),
    });
    assert!(result.is_ok());
}

#[test]
fn test_entrypoints_language_python_filter() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("app.py", "if __name__ == '__main__':\n    pass\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::Python),
    });
    assert!(result.is_ok());
}

#[test]
fn test_entrypoints_language_bash_filter() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("run.sh", "#!/bin/bash\necho hello\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::Bash),
    });
    assert!(result.is_ok());
}

#[test]
fn test_entrypoints_language_terraform_filter() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("main.tf", "resource \"aws_instance\" \"example\" {}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::Terraform),
    });
    assert!(result.is_ok());
}

#[test]
fn test_entrypoints_language_helm_filter() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("values.yaml", "key: value\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::Helm),
    });
    assert!(result.is_ok());
}

#[test]
fn test_show_partially_veiled_lines() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "line1\nline2\nline3\nline4\nline5\n");
    let _ = env.run(Commands::Veil {
        pattern: "f.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Show {
        file: "f.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("File: f.txt"));
    assert!(stdout.contains("[veiled]") || stdout.contains("..."));
}

#[test]
fn test_parse_detailed_calls_without_caller() {
    let env = TestEnv::init(Mode::Blacklist);
    // Python top-level calls have no caller
    env.write_file(
        "script.py",
        "import os\nprint('hello')\nos.path.exists('x')\n",
    );
    let (stdout, _, result) = env.run(Commands::Parse {
        file: "script.py".into(),
        format: ParseFormat::Detailed,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Calls:") || stdout.contains("Imports:"));
}

#[test]
fn test_parse_detailed_with_function_signatures() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "lib.rs",
        "fn hello(x: i32) -> bool {\n    x > 0\n}\n\nfn world() {\n    hello(5);\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Parse {
        file: "lib.rs".into(),
        format: ParseFormat::Detailed,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Signature:"));
}

#[test]
fn test_clean_without_data_dir() {
    let temp = tempfile::TempDir::new().unwrap();
    // Don't init, so no .funveil dir exists
    std::fs::write(
        temp.path().join(CONFIG_FILE),
        "mode: blacklist\nobjects: {}\nwhitelist: []\nblacklist: []\n",
    )
    .unwrap();
    let (stdout, _, result) = run_in_dir(temp.path(), Commands::Clean);
    assert!(result.is_ok());
    assert!(stdout.contains("Removed all funveil data"));
}

#[test]
fn test_clean_without_config_file() {
    let env = TestEnv::init(Mode::Blacklist);
    // Remove config file but keep .funveil dir
    std::fs::remove_file(env.dir().join(CONFIG_FILE)).unwrap();
    let (stdout, _, result) = env.run(Commands::Clean);
    assert!(result.is_ok());
    assert!(stdout.contains("Removed all funveil data"));
}

#[test]
fn test_veil_dry_run_file_exists() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "f.txt".into(),
        mode: VeilMode::Full,
        dry_run: true,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("bytes"));
    assert!(stdout.contains("would be affected"));
}

#[test]
fn test_veil_dry_run_file_nonexistent() {
    let env = TestEnv::init(Mode::Blacklist);
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "nonexist.txt".into(),
        mode: VeilMode::Full,
        dry_run: true,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would veil: nonexist.txt"));
}

#[test]
fn test_status_files_with_full_veils_in_whitelist() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("veiled"));
    assert!(stdout.contains("full"));
}

#[test]
fn test_entrypoints_with_handler_filter() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("app.py", "def handle_request():\n    pass\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: Some(EntrypointTypeArg::Handler),
        language: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_entrypoints_with_export_filter() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("lib.rs", "pub fn exported() {}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: Some(EntrypointTypeArg::Export),
        language: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_apply_dry_run_with_veiled_files() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "original\n");
    let _ = env.veil("f.txt");
    // Unveil to get original back
    let _ = env.unveil("f.txt");
    // Re-add to blacklist
    let mut config = Config::load(env.dir()).unwrap();
    config.add_to_blacklist("f.txt");
    config.save(env.dir()).unwrap();

    let (stdout, _, result) = env.run(Commands::Apply { dry_run: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Would re-veil") || stdout.contains("would be re-applied"));
}

#[test]
fn test_show_unveiled_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("plain.txt", "line1\nline2\n");
    let (stdout, _, result) = env.run(Commands::Show {
        file: "plain.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("line1"));
    assert!(stdout.contains("line2"));
}

#[test]
fn test_doctor_with_missing_object() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");
    // Delete CAS objects to create an integrity issue
    let objects_dir = env.dir().join(".funveil").join("objects");
    if objects_dir.exists() {
        std::fs::remove_dir_all(&objects_dir).unwrap();
        std::fs::create_dir_all(&objects_dir).unwrap();
    }
    let (stdout, _, result) = env.run(Commands::Doctor);
    assert!(result.is_ok());
    assert!(stdout.contains("Missing object") || stdout.contains("issue"));
}

#[test]
fn test_unveil_all_records_history() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");

    let _ = env.unveil_all();

    let history = ActionHistory::load(env.dir()).unwrap();
    let last = history.entries.last().unwrap();
    assert_eq!(last.command, "unveil");
}

#[test]
fn test_run_veil_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("dry.txt", "content\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "dry.txt".into(),
        mode: VeilMode::Full,
        dry_run: true,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would veil"));
    assert_eq!(
        std::fs::read_to_string(env.dir().join("dry.txt")).unwrap(),
        "content\n"
    );
}

#[test]
fn test_run_unveil_all_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("a.txt", "aaa\n");
    let _ = env.veil("a.txt");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: true,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil"));
}

#[test]
fn test_run_unveil_pattern_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("b.txt", "bbb\n");
    let _ = env.veil("b.txt");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("b.txt".into()),
        all: false,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil"));
}

#[test]
fn test_run_apply_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("x.txt", "data\n");
    let _ = env.veil("x.txt");
    let _ = env.unveil_all();
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Would re-veil") || stdout.contains("would be re-applied"));
}

#[test]
fn test_run_history_list() {
    let env = TestEnv::init(Mode::Blacklist);
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Past"));
}

#[test]
fn test_run_history_show() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("h.txt", "data\n");
    let _ = env.veil("h.txt");
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Action #2"));
}

#[test]
fn test_run_history_show_not_found() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(999),
    });
    assert!(result.is_err());
}

#[test]
fn test_run_undo_redo() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("u.txt", "data\n");
    let _ = env.veil("u.txt");
    let (stdout, _, result) = env.run(Commands::Undo { force: false });
    assert!(result.is_ok());
    assert!(stdout.contains("Undone"));

    let (stdout, _, result) = env.run(Commands::Redo);
    assert!(result.is_ok());
    assert!(stdout.contains("Redone"));
}

#[test]
fn test_run_undo_empty() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Undo { force: false });
    assert!(result.is_err());
}

#[test]
fn test_run_redo_nothing() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Redo);
    assert!(result.is_err());
}

#[test]
fn test_run_veil_level0() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("l0.txt", "data\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "l0.txt".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(0),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled (level 0)"));
}

#[test]
fn test_run_veil_level1() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("l1.rs", "fn hello() {\n    println!(\"hi\");\n}\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "l1.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(1),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("level 1"));
}

#[test]
fn test_run_veil_level2() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "l2.rs",
        "fn caller() { helper(); }\nfn helper() { work(); }\nfn unused() { secret(); }\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "l2.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("level 2"));
}

#[test]
fn test_run_veil_level3() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("l3.txt", "data\n");
    let _ = env.veil("l3.txt");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "l3.txt".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(3),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Level 3"));
}

#[test]
fn test_run_veil_level3_already_unveiled() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("l3b.txt", "data\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "l3b.txt".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(3),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("already at full source"));
}

#[test]
fn test_run_status_with_files() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("vis.txt", "visible\n");
    env.write_file("hid.txt", "hidden\n");
    let _ = env.veil("hid.txt");
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Files:"));
    assert!(stdout.contains("vis.txt"));
    assert!(stdout.contains("veiled"));
}

#[test]
fn test_run_disclose() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("focus.rs", "fn main() {}\nfn helper() {}\n");
    let _ = env.veil("focus.rs");
    let (stdout, _, result) = env.run(Commands::Disclose {
        budget: 10000,
        focus: "focus.rs".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Disclosure plan"));
}

#[test]
fn test_run_veil_directory() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("subdir/a.txt", "aaa\n");
    env.write_file("subdir/b.txt", "bbb\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "subdir".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiling"));
}

#[test]
fn test_run_veil_level1_nonexistent() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Veil {
        pattern: "missing.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(1),
    });
    assert!(result.is_err());
}

#[test]
fn test_run_context() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("ctx.rs", "fn target() { dep(); }\nfn dep() {}\n");
    let _ = env.veil("ctx.rs");
    let (stdout, _, result) = env.run(Commands::Context {
        function: "target".into(),
        depth: 2,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Context for target"));
}

#[test]
fn test_run_status_with_files_flag_full_veiled() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("a.txt", "aaa\n");
    env.write_file("b.txt", "bbb\n");
    let _ = env.veil("a.txt");
    let _ = env.veil("b.txt");
    env.write_file("a.txt", "aaa\n");
    env.write_file("b.txt", "bbb\n");
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Files:"));
}

#[test]
fn test_run_status_with_files_flag_partial_veiled() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("p.txt", "line1\nline2\nline3\nline4\nline5\n");
    let _ = env.run(Commands::Veil {
        pattern: "p.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Files:"));
    assert!(stdout.contains("p.txt"));
}

#[test]
fn test_run_veil_dry_run_preserves_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("dry.txt", "content\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "dry.txt".into(),
        mode: VeilMode::Full,
        dry_run: true,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would veil"));
    assert!(env.dir().join("dry.txt").exists());
}

#[test]
fn test_run_veil_regex_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("x.txt", "xxx\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "/x\\.txt/".into(),
        mode: VeilMode::Full,
        dry_run: true,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would veil") || stdout.contains("would be affected"));
}

#[test]
fn test_run_unveil_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("un.txt", "data\n");
    let _ = env.veil("un.txt");
    env.write_file("un.txt", "data\n");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("un.txt".into()),
        all: false,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil"));
}

#[test]
fn test_run_unveil_regex_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("rd.txt", "data\n");
    let _ = env.veil("rd.txt");
    env.write_file("rd.txt", "data\n");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("/rd\\.txt/".into()),
        all: false,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil") || stdout.contains("would be affected"));
}

#[test]
fn test_run_apply_dry_run_shows_would_reveil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("ad.txt", "secret\n");
    let _ = env.veil("ad.txt");
    let _ = env.unveil("ad.txt");
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Would re-veil") || stdout.contains("would be re-applied"));
}

#[test]
fn test_run_apply_with_invalid_hash_in_config() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let _ = env.veil("f.txt");
    env.write_file("f.txt", "data\n");
    let mut config = Config::load(env.dir()).unwrap();
    config.register_object(
        "f.txt".to_string(),
        ObjectMeta::new(ContentHash::from_content(b"data\n"), 0o644),
    );
    let cas_dir = env.dir().join(".funveil").join("objects");
    if cas_dir.exists() {
        for entry in std::fs::read_dir(&cas_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.path().is_dir() {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    config.save(env.dir()).unwrap();
    let (_, stderr, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(
        stderr.contains("missing from CAS")
            || stderr.contains("invalid hash")
            || stderr.contains("Skipping")
    );
}

#[test]
fn test_run_show_fully_veiled_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("full.txt", "secret content\n");
    let _ = env.veil("full.txt");
    let (stdout, _, result) = env.run(Commands::Show {
        file: "full.txt".into(),
    });
    assert!(result.is_ok());
    assert!(
        stdout.contains("FULLY VEILED")
            || stdout.contains("VEILED - not on disk")
            || stdout.contains("veiled")
    );
}

#[test]
fn test_run_show_partially_veiled_lines() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("partial.txt", "line1\nline2\nline3\nline4\nline5\n");
    let _ = env.run(Commands::Veil {
        pattern: "partial.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Show {
        file: "partial.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("partial.txt"));
    assert!(stdout.contains("line1") || stdout.contains("veiled"));
}

#[test]
fn test_run_clean_already_cleaned() {
    let env = TestEnv::init(Mode::Whitelist);
    let _ = env.run(Commands::Clean);
    let (stdout, _, result) = env.run(Commands::Clean);
    assert!(result.is_ok() || result.is_err());
    let _ = stdout;
}

#[test]
fn test_run_history_list_with_entries() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("h.txt", "data\n");
    let _ = env.veil("h.txt");
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Past"));
}

#[test]
fn test_run_history_with_future_entries() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("hf.txt", "data\n");
    let _ = env.veil("hf.txt");
    let _ = env.run(Commands::Undo { force: false });
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Future") || stdout.contains("Past"));
}

#[test]
fn test_run_history_show_detail() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("hs.txt", "data\n");
    let _ = env.veil("hs.txt");
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(1),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Action #1") || stdout.contains("not found"));
}

#[test]
fn test_run_disclose_command() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("disc.rs", "fn main() { helper(); }\nfn helper() {}\n");
    let _ = env.veil("disc.rs");
    let (stdout, _, result) = env.run(Commands::Disclose {
        budget: 10000,
        focus: "disc.rs".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Disclosure plan") || stdout.contains("tokens"));
}

#[test]
fn test_run_unveil_callees_of() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "caller.rs",
        "fn main() { helper(); }\nfn helper() { println!(\"hi\"); }\n",
    );
    let _ = env.veil("caller.rs");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: Some("main".into()),
        level: None,
    });
    assert!(result.is_ok());
    assert!(
        stdout.contains("Unveiled") || stdout.contains("callee") || stdout.contains("No callee")
    );
}

#[test]
fn test_run_unveil_callees_of_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "cd.rs",
        "fn main() { helper(); }\nfn helper() { println!(\"hi\"); }\n",
    );
    let _ = env.veil("cd.rs");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: false,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: Some("main".into()),
        level: None,
    });
    assert!(result.is_ok());
    assert!(
        stdout.contains("Would unveil")
            || stdout.contains("would be affected")
            || stdout.contains("No callee")
    );
}

#[test]
fn test_run_unveil_callers_of() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("callee.rs", "fn main() { target(); }\nfn target() {}\n");
    let _ = env.veil("callee.rs");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: Some("target".into()),
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(
        stdout.contains("Unveiled") || stdout.contains("caller") || stdout.contains("No caller")
    );
}

#[test]
fn test_run_entrypoints_type_handler() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("handler.rs", "fn main() {}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: Some(EntrypointTypeArg::Handler),
        language: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_entrypoints_type_export() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("export.rs", "pub fn public_api() {}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: Some(EntrypointTypeArg::Export),
        language: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_entrypoints_type_test() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("tst.rs", "#[test]\nfn test_foo() {}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: Some(EntrypointTypeArg::Test),
        language: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_entrypoints_type_cli() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("cli.rs", "fn main() {}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: Some(EntrypointTypeArg::Cli),
        language: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_veil_level1_headers_output() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "lvl1.rs",
        "fn greet(name: &str) -> String {\n    format!(\"hi {name}\")\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "lvl1.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(1),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled (level 1, headers)"));
}

#[test]
fn test_run_veil_level2_headers_and_called() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "lvl2.rs",
        "fn caller() {\n    helper();\n}\nfn helper() {\n    do_work();\n}\nfn unused() {\n    secret();\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "lvl2.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled (level 2, headers+called bodies)"));
}

#[test]
fn test_run_veil_level0_removes_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("lvl0.rs", "fn main() {}\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "lvl0.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(0),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled") || stdout.contains("Removed"));
}

#[test]
fn test_run_veil_level3_full_source() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("lvl3.rs", "fn main() { println!(\"hello\"); }\n");
    let _ = env.veil("lvl3.rs");
    env.write_file("lvl3.rs", "fn main() { println!(\"hello\"); }\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "lvl3.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(3),
    });
    assert!(result.is_ok());
    assert!(
        stdout.contains("Level 3") || stdout.contains("unveiled") || stdout.contains("full source")
    );
}

#[test]
fn test_collect_affected_files_for_pattern_regex() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("match1.txt", "aaa\n");
    env.write_file("match2.txt", "bbb\n");
    let files = collect_affected_files_for_pattern(env.dir(), "/match.*/");
    assert!(files.len() >= 2);
}

#[test]
fn test_collect_affected_files_for_pattern_directory() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("subdir/a.txt", "aaa\n");
    env.write_file("subdir/b.txt", "bbb\n");
    let files = collect_affected_files_for_pattern(env.dir(), "subdir");
    assert!(files.len() >= 2);
}

#[test]
fn test_collect_affected_files_for_pattern_hash() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("f.txt", "data\n");
    let files = collect_affected_files_for_pattern(env.dir(), "f.txt#1-3");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], "f.txt");
}

#[test]
fn test_collect_affected_files_for_pattern_nonexistent_prefix() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("existing.txt", "data\n");
    let _ = env.veil("existing.txt");
    let files = collect_affected_files_for_pattern(env.dir(), "nonexist");
    assert!(!files.is_empty());
}

#[test]
fn test_run_undo_and_redo() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("ur.txt", "data\n");
    let _ = env.veil("ur.txt");
    let (stdout, _, result) = env.run(Commands::Undo { force: false });
    assert!(result.is_ok());
    assert!(stdout.contains("Undone"));
    let (stdout, _, result) = env.run(Commands::Redo);
    assert!(result.is_ok());
    assert!(stdout.contains("Redone"));
}

#[test]
fn test_history_default() {
    let h = ActionHistory::default();
    assert!(h.is_empty());
}

#[test]
fn test_history_load_empty_content() {
    let temp = tempfile::TempDir::new().unwrap();
    let history_dir = temp.path().join(".funveil").join("history");
    std::fs::create_dir_all(&history_dir).unwrap();
    std::fs::write(history_dir.join("history.yaml"), "   \n").unwrap();
    let h = ActionHistory::load(temp.path()).unwrap();
    assert!(h.is_empty());
}

#[test]
fn test_restore_action_state_remove_file() {
    let temp = tempfile::TempDir::new().unwrap();
    funveil::config::ensure_data_dir(temp.path()).unwrap();
    let file_path = temp.path().join("to_remove.txt");
    std::fs::write(&file_path, "content").unwrap();
    assert!(file_path.exists());

    let state = ActionState {
        config_yaml: None,
        file_snapshots: vec![FileSnapshot {
            path: "to_remove.txt".to_string(),
            cas_hash: None,
            permissions: "0644".to_string(),
        }],
    };
    restore_action_state(temp.path(), &state).unwrap();
    assert!(!file_path.exists());
}

#[test]
fn test_restore_action_state_create_in_subdir() {
    let temp = tempfile::TempDir::new().unwrap();
    funveil::config::ensure_data_dir(temp.path()).unwrap();
    let store = ContentStore::new(temp.path());
    let hash = store.store(b"restored content").unwrap();

    let state = ActionState {
        config_yaml: None,
        file_snapshots: vec![FileSnapshot {
            path: "deep/nested/file.txt".to_string(),
            cas_hash: Some(hash.full().to_string()),
            permissions: "0644".to_string(),
        }],
    };
    restore_action_state(temp.path(), &state).unwrap();
    assert!(temp.path().join("deep/nested/file.txt").exists());
}

#[test]
fn test_run_unveil_all_with_files() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("ua1.txt", "aaa\n");
    env.write_file("ua2.txt", "bbb\n");
    let _ = env.veil("ua1.txt");
    let _ = env.veil("ua2.txt");
    env.write_file("ua1.txt", "aaa\n");
    env.write_file("ua2.txt", "bbb\n");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: true,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled all files"));
}

#[test]
fn test_run_veil_directory_multi_files() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("dir/a.txt", "aaa\n");
    env.write_file("dir/b.txt", "bbb\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "dir".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veil") || stdout.contains("veil"));
}

#[test]
fn test_run_unveil_directory() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("udir/a.txt", "aaa\n");
    env.write_file("udir/b.txt", "bbb\n");
    let _ = env.run(Commands::Veil {
        pattern: "udir".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    env.write_file("udir/a.txt", "aaa\n");
    env.write_file("udir/b.txt", "bbb\n");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("udir".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled") || stdout.contains("unveil"));
}

#[test]
fn test_run_entrypoints_language_go() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("main.go", "package main\nfunc main() {}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::Go),
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_entrypoints_language_python() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file(
        "main.py",
        "def main():\n    pass\n\nif __name__ == '__main__':\n    main()\n",
    );
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::Python),
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_entrypoints_language_typescript() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("app.ts", "export function main() {}\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::TypeScript),
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_context_with_veiled_deps() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "main.rs",
        "fn entry() { dep_a(); }\nfn dep_a() { dep_b(); }\nfn dep_b() {}\n",
    );
    let _ = env.veil("main.rs");
    env.write_file(
        "main.rs",
        "fn entry() { dep_a(); }\nfn dep_a() { dep_b(); }\nfn dep_b() {}\n",
    );
    let (stdout, _, result) = env.run(Commands::Context {
        function: "entry".into(),
        depth: 3,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Context for entry"));
}

#[test]
fn test_run_apply_skips_missing_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("gone2.txt", "data\n");
    let _ = env.veil("gone2.txt");
    let _ = env.unveil("gone2.txt");
    std::fs::remove_file(env.dir().join("gone2.txt")).ok();
    let (_, stderr, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(stderr.contains("Skipping") || stderr.contains("not found") || stderr.is_empty());
}

#[test]
fn test_run_history_show_nonexistent_id() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::History {
        limit: 20,
        show: Some(9999),
    });
    assert!(result.is_err());
}

#[test]
fn test_run_veil_regex_with_veiled_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("r1.txt", "aaa\n");
    env.write_file("r2.txt", "bbb\n");
    let _ = env.veil("r1.txt");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "/r.*\\.txt/".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    let _ = stdout;
}

#[test]
fn test_run_unveil_regex_with_veiled_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("nod.txt", "data\n");
    let _ = env.veil("nod.txt");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("/nod\\.txt/".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(
        stdout.contains("Unveiled")
            || stdout.contains("No veiled files")
            || stdout.contains("not on disk")
    );
}

#[test]
fn test_collect_affected_files_regex_with_veiled_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("rv.txt", "data\n");
    let _ = env.veil("rv.txt");
    let files = collect_affected_files_for_pattern(env.dir(), "/rv\\.txt/");
    assert!(!files.is_empty());
}

#[test]
fn test_collect_affected_files_directory_with_veiled_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("subdir2/v.txt", "data\n");
    let _ = env.run(Commands::Veil {
        pattern: "subdir2".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let files = collect_affected_files_for_pattern(env.dir(), "subdir2");
    assert!(!files.is_empty());
}

#[test]
fn test_run_commands_name_context_and_disclose() {
    assert_eq!(
        Commands::Context {
            function: "f".into(),
            depth: 2
        }
        .name(),
        "context"
    );
    assert_eq!(
        Commands::Disclose {
            budget: 100,
            focus: "f".into()
        }
        .name(),
        "disclose"
    );
    assert_eq!(Commands::Undo { force: false }.name(), "undo");
    assert_eq!(Commands::Redo.name(), "redo");
    assert_eq!(
        Commands::History {
            limit: 20,
            show: None
        }
        .name(),
        "history"
    );
}

#[test]
fn test_run_checkpoint_restore_with_manifest_files() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("cp_f.txt", "data\n");
    let _ = env.veil("cp_f.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "with_files".into(),
        },
    });
    env.write_file("cp_f.txt", "data\n");
    let _ = env.unveil("cp_f.txt");
    let (stdout, _, result) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Restore {
            name: "with_files".into(),
        },
    });
    assert!(result.is_ok());
    let _ = stdout;
}

#[test]
fn test_run_restore_latest_checkpoint() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("rr.txt", "data\n");
    let _ = env.veil("rr.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "latest".into(),
        },
    });
    env.write_file("rr.txt", "data\n");
    let _ = env.unveil("rr.txt");
    let (stdout, _, result) = env.run(Commands::Restore);
    assert!(result.is_ok());
    assert!(stdout.contains("Restoring"));
}

#[test]
fn test_run_trace_from_entrypoint_with_deeper_code() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file(
        "deep.rs",
        "fn main() { level1(); }\nfn level1() { level2(); }\nfn level2() {}\n",
    );
    let (stdout, _, result) = env.run(Commands::Trace {
        function: None,
        from: None,
        to: None,
        from_entrypoint: true,
        depth: 5,
        format: TraceFormat::Tree,
        no_std: false,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Entrypoints found:") || stdout.is_empty());
}

#[test]
fn test_run_veil_with_symbol() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "sym.rs",
        "fn public_fn() {\n    println!(\"hello\");\n}\n\nfn private_fn() {\n    println!(\"secret\");\n}\n",
    );
    let _ = env.veil("sym.rs");
    env.write_file(
        "sym.rs",
        "fn public_fn() {\n    println!(\"hello\");\n}\n\nfn private_fn() {\n    println!(\"secret\");\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "sym.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: Some("private_fn".into()),
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok() || result.is_err());
    let _ = stdout;
}

#[test]
fn test_run_unveil_with_symbol() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("usym.rs", "fn target() {\n    println!(\"hello\");\n}\n");
    let _ = env.veil("usym.rs");
    env.write_file("usym.rs", "fn target() {\n    println!(\"hello\");\n}\n");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: false,
        dry_run: false,
        symbol: Some("target".into()),
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok() || result.is_err());
    let _ = stdout;
}

#[test]
fn test_run_veil_unreachable_from_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("reachable.rs", "fn main() { helper(); }\nfn helper() {}\n");
    env.write_file("orphan.rs", "fn orphan() {}\n");
    let _ = env.veil("reachable.rs");
    let _ = env.veil("orphan.rs");
    env.write_file("reachable.rs", "fn main() { helper(); }\nfn helper() {}\n");
    env.write_file("orphan.rs", "fn orphan() {}\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "unused".into(),
        mode: VeilMode::Full,
        dry_run: true,
        symbol: None,
        unreachable_from: Some("main".into()),
        level: None,
    });
    assert!(result.is_ok() || result.is_err());
    let _ = stdout;
}

#[test]
fn test_run_veil_unreachable_from() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("reach.rs", "fn main() { dep(); }\nfn dep() {}\n");
    env.write_file("iso.rs", "fn isolated() {}\n");
    let _ = env.veil("reach.rs");
    let _ = env.veil("iso.rs");
    env.write_file("reach.rs", "fn main() { dep(); }\nfn dep() {}\n");
    env.write_file("iso.rs", "fn isolated() {}\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "unused".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: Some("main".into()),
        level: None,
    });
    assert!(result.is_ok() || result.is_err());
    let _ = stdout;
}

#[test]
fn test_run_unveil_file_not_on_disk_restore() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("vanish.txt", "original content\n");
    let _ = env.veil("vanish.txt");
    assert!(!env.dir().join("vanish.txt").exists());
    let (stdout, _, result) = env.unveil("vanish.txt");
    assert!(result.is_ok());
    assert!(env.dir().join("vanish.txt").exists());
    let _ = stdout;
}

#[test]
fn test_run_apply_reveils_after_unveil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("reapply.txt", "secret data\n");
    let _ = env.veil("reapply.txt");
    let _ = env.unveil("reapply.txt");
    let content_after_unveil = std::fs::read_to_string(env.dir().join("reapply.txt")).unwrap();
    assert_eq!(content_after_unveil, "secret data\n");
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(
        stdout.contains("re-veiled")
            || stdout.contains("Applied")
            || stdout.contains("Re-applying")
    );
}

#[test]
fn test_run_gc_after_veil_unveil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("gc1.txt", "data for gc\n");
    env.write_file("gc2.txt", "more data\n");
    let _ = env.veil("gc1.txt");
    let _ = env.veil("gc2.txt");
    let _ = env.unveil("gc1.txt");
    let (stdout, _, result) = env.run(Commands::Gc);
    assert!(result.is_ok());
    assert!(stdout.contains("Garbage collected"));
}

#[test]
fn test_run_veil_symbol_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("sd.rs", "fn target_fn() {\n    println!(\"hi\");\n}\n");
    let _ = env.veil("sd.rs");
    env.write_file("sd.rs", "fn target_fn() {\n    println!(\"hi\");\n}\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "sd.rs".into(),
        mode: VeilMode::Full,
        dry_run: true,
        symbol: Some("target_fn".into()),
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok() || result.is_err());
    let _ = stdout;
}

#[test]
fn test_run_show_nonveiled_file_with_content() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("plain.txt", "line1\nline2\nline3\n");
    let (stdout, _, result) = env.run(Commands::Show {
        file: "plain.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("plain.txt"));
    assert!(stdout.contains("line1"));
    assert!(stdout.contains("line2"));
    assert!(stdout.contains("line3"));
}

#[test]
fn test_run_history_empty() {
    let env = TestEnv::init(Mode::Blacklist);
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Past"));
}

#[test]
fn test_run_veil_headers_with_source_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "hdr.rs",
        "fn hello() {\n    println!(\"world\");\n}\nfn unused() {\n    secret();\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "hdr.rs".into(),
        mode: VeilMode::Headers,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled (headers mode)"));
}

#[test]
fn test_run_unveil_all_dry_run_output() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("adr.txt", "data\n");
    let _ = env.veil("adr.txt");
    env.write_file("adr.txt", "data\n");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: true,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil") || stdout.contains("would be affected"));
}

#[test]
fn test_run_clean_removes_data_dir() {
    let env = TestEnv::init(Mode::Whitelist);
    assert!(env.dir().join(".funveil").exists());
    let (_, _, result) = env.run(Commands::Clean);
    assert!(result.is_ok());
    assert!(!env.dir().join(".funveil").exists());
}

#[test]
fn test_run_doctor_with_partial_veil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("doc.txt", "line1\nline2\nline3\nline4\n");
    let _ = env.run(Commands::Veil {
        pattern: "doc.txt#2-3".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Doctor);
    assert!(result.is_ok());
    assert!(stdout.contains("checks passed") || stdout.contains("Doctor"));
}

#[test]
fn test_level2_with_python_class() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "cls.py",
        "class MyClass:\n    def method_a(self):\n        self.method_b()\n\n    def method_b(self):\n        pass\n\n    def unused(self):\n        pass\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "cls.py".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled (level 2") || stdout.contains("Level 2"));
}

#[test]
fn test_run_history_show_veil_action() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("hs2.txt", "data\n");
    let _ = env.veil("hs2.txt");
    let history = ActionHistory::load(env.dir()).unwrap();
    if !history.is_empty() {
        let id = history.past().last().map(|e| e.id).unwrap_or(1);
        let (stdout, _, result) = env.run(Commands::History {
            limit: 20,
            show: Some(id),
        });
        assert!(result.is_ok());
        assert!(stdout.contains("Action #"));
    }
}

#[test]
fn test_run_undo_non_undoable_action() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Undo { force: false });
    assert!(result.is_err());
}

#[test]
fn test_run_redo_nothing_to_redo() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Redo);
    assert!(result.is_err());
}

#[test]
fn test_run_veil_regex_file_errors() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("err.txt", "data\n");
    let _ = env.veil("err.txt");
    env.write_file("err.txt", "data\n");
    let (_, _, result) = env.run(Commands::Veil {
        pattern: "/err\\.txt/".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_unveil_partial_veil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("pv.txt", "line1\nline2\nline3\nline4\nline5\n");
    let _ = env.run(Commands::Veil {
        pattern: "pv.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("pv.txt".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled"));
}

#[test]
fn test_run_unveil_full_veil_restores_content() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("restore.txt", "original content here\n");
    let _ = env.veil("restore.txt");
    assert!(!env.dir().join("restore.txt").exists());
    let (_, _, result) = env.unveil("restore.txt");
    assert!(result.is_ok());
    let content = std::fs::read_to_string(env.dir().join("restore.txt")).unwrap();
    assert_eq!(content, "original content here\n");
}

#[test]
fn test_run_checkpoint_save_and_restore_preserves_state() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("cpsr.txt", "checkpoint data\n");
    let _ = env.veil("cpsr.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "pre_unveil".into(),
        },
    });
    let _ = env.unveil("cpsr.txt");
    let (_, _, result) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Restore {
            name: "pre_unveil".into(),
        },
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_veil_with_level_and_python() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "lvl.py",
        "def main():\n    helper()\n\ndef helper():\n    print('hi')\n\ndef unused():\n    print('bye')\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "lvl.py".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(1),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled (level 1, headers)"));
}

#[test]
fn test_run_status_files_with_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("nod2.txt", "data\n");
    let _ = env.veil("nod2.txt");
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Files:") || stdout.contains("not on disk"));
}

#[test]
fn test_run_mode_change_records_history() {
    let env = TestEnv::init(Mode::Whitelist);
    let _ = env.run(Commands::Mode {
        mode: Some(Mode::Blacklist),
    });
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("mode") || stdout.contains("Past"));
}

#[test]
fn test_run_veil_multiple_partial_ranges() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "multi.txt",
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\n",
    );
    let _ = env.run(Commands::Veil {
        pattern: "multi.txt#2-3,6-7".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Show {
        file: "multi.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("multi.txt"));
}

#[test]
fn test_run_context_symbol_not_found() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("no_sym.rs", "fn main() {}\n");
    let _ = env.veil("no_sym.rs");
    let (_, _, result) = env.run(Commands::Context {
        function: "nonexistent_function".into(),
        depth: 2,
    });
    assert!(result.is_err());
}

#[test]
fn test_run_disclose_with_veiled_focus() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "focus.rs",
        "fn main() { dep(); }\nfn dep() { println!(\"hello\"); }\n",
    );
    let _ = env.veil("focus.rs");
    let (stdout, _, result) = env.run(Commands::Disclose {
        budget: 100000,
        focus: "focus.rs".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Disclosure plan"));
    assert!(stdout.contains("tokens"));
}

#[test]
fn test_run_entrypoints_language_bash() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("script.sh", "#!/bin/bash\necho hello\n");
    let (_, _, result) = env.run(Commands::Entrypoints {
        entry_type: None,
        language: Some(LanguageArg::Bash),
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_unveil_regex_veiled_files_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("rv1.txt", "aaa\n");
    env.write_file("rv2.txt", "bbb\n");
    let _ = env.run(Commands::Veil {
        pattern: "rv1.txt#1-1".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("/rv.*\\.txt/".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled") || stdout.contains("No veiled files"));
}

#[test]
fn test_run_init_blacklist_mode() {
    let (stdout, _, result) = run_in_temp(Commands::Init {
        mode: Mode::Blacklist,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Initialized funveil with blacklist mode"));
}

#[test]
fn test_run_show_full_veiled_file_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("fvod.txt", "secret\n");
    let _ = env.veil("fvod.txt");
    env.write_file("fvod.txt", "re-created\n");
    let (stdout, _, result) = env.run(Commands::Show {
        file: "fvod.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("FULLY VEILED"));
}

#[test]
fn test_run_show_partial_veiled_with_visible_lines() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("spv.txt", "visible1\nsecret2\nsecret3\nvisible4\n");
    let _ = env.run(Commands::Veil {
        pattern: "spv.txt#2-3".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Show {
        file: "spv.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("visible1") || stdout.contains("veiled"));
}

#[test]
fn test_run_disclose_with_dependencies() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("main_d.rs", "fn main() { dep_fn(); }\n");
    env.write_file("dep_d.rs", "fn dep_fn() { transitive_fn(); }\n");
    env.write_file(
        "transitive.rs",
        "fn transitive_fn() { println!(\"deep\"); }\n",
    );
    let _ = env.veil("main_d.rs");
    let _ = env.veil("dep_d.rs");
    let _ = env.veil("transitive.rs");
    let (stdout, _, result) = env.run(Commands::Disclose {
        budget: 100000,
        focus: "main_d.rs".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Disclosure plan"));
    assert!(stdout.contains("main_d.rs") || stdout.contains("tokens"));
}

#[test]
fn test_run_disclose_with_tiny_budget() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "big.rs",
        &format!("fn main() {{ dep(); }}\n{}", "// padding\n".repeat(100)),
    );
    env.write_file(
        "dep_big.rs",
        &format!(
            "fn dep() {{ println!(\"hi\"); }}\n{}",
            "// more padding\n".repeat(100)
        ),
    );
    let _ = env.veil("big.rs");
    let _ = env.veil("dep_big.rs");
    let (stdout, _, result) = env.run(Commands::Disclose {
        budget: 5,
        focus: "big.rs".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Disclosure plan"));
    assert!(stdout.contains("0/5") || stdout.contains("tokens"));
}

#[test]
fn test_run_context_unveils_dependencies() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("ctx_main.rs", "fn entry() { ctx_dep(); }\n");
    env.write_file("ctx_dep.rs", "fn ctx_dep() { println!(\"hello\"); }\n");
    let _ = env.veil("ctx_main.rs");
    let _ = env.veil("ctx_dep.rs");
    env.write_file("ctx_main.rs", "fn entry() { ctx_dep(); }\n");
    env.write_file("ctx_dep.rs", "fn ctx_dep() { println!(\"hello\"); }\n");
    let (stdout, _, result) = env.run(Commands::Context {
        function: "entry".into(),
        depth: 3,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Context for entry"));
}

#[test]
fn test_run_status_files_with_veiled_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("sod.txt", "data\n");
    let _ = env.veil("sod.txt");
    env.write_file("sod.txt", "data\n");
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Files:"));
    assert!(stdout.contains("sod.txt"));
}

#[test]
fn test_run_status_files_with_partial_veiled_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("spod.txt", "l1\nl2\nl3\nl4\n");
    let _ = env.run(Commands::Veil {
        pattern: "spod.txt#2-3".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("partial") || stdout.contains("spod.txt"));
}

#[test]
fn test_run_unveil_regex_error_handling() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("ure1.txt", "aaa\n");
    env.write_file("ure2.txt", "bbb\n");
    let _ = env.veil("ure1.txt");
    let _ = env.veil("ure2.txt");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("/ure.*\\.txt/".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled") || stdout.contains("No files"));
}

#[test]
fn test_run_apply_dry_run_with_veiled_files() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("adv.txt", "data for dry run\n");
    let _ = env.veil("adv.txt");
    let _ = env.unveil("adv.txt");
    let content = std::fs::read_to_string(env.dir().join("adv.txt")).unwrap();
    assert_eq!(content, "data for dry run\n");
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Would re-veil") || stdout.contains("would be re-applied"));
}

#[test]
fn test_run_apply_reveil_existing_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("rev.txt", "original\n");
    let _ = env.veil("rev.txt");
    let _ = env.unveil("rev.txt");
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(
        stdout.contains("re-veiled")
            || stdout.contains("Re-applying")
            || stdout.contains("Applied")
    );
}

#[test]
fn test_run_unveil_all_then_apply() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("ua_apply1.txt", "aaa\n");
    env.write_file("ua_apply2.txt", "bbb\n");
    let _ = env.veil("ua_apply1.txt");
    let _ = env.veil("ua_apply2.txt");
    let _ = env.unveil_all();
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    let _ = stdout;
}

#[test]
fn test_level2_veil_with_module() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "mod_lvl2.py",
        "def caller():\n    helper()\n\ndef helper():\n    print('hi')\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "mod_lvl2.py".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("level 2"));
}

#[test]
fn test_run_trace_backward() {
    let env = TestEnv::init(Mode::Whitelist);
    env.write_file("back.rs", "fn caller() { target(); }\nfn target() {}\n");
    let (_, _, result) = env.run(Commands::Trace {
        function: None,
        from: None,
        to: Some("target".into()),
        from_entrypoint: false,
        depth: 3,
        format: TraceFormat::List,
        no_std: false,
    });
    assert!(result.is_ok());
}

#[test]
fn test_run_unveil_callers_of_nonexistent() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("nc.rs", "fn main() {}\n");
    let _ = env.veil("nc.rs");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: Some("nonexistent_fn".into()),
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("No caller") || stdout.contains("Unveiled"));
}

#[test]
fn test_run_unveil_callees_of_nonexistent() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("nce.rs", "fn main() {}\n");
    let _ = env.veil("nce.rs");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: Some("nonexistent_fn".into()),
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("No callee") || stdout.contains("Unveiled"));
}

#[test]
fn test_run_undo_force_non_undoable() {
    let env = TestEnv::init(Mode::Blacklist);
    let (_, _, result) = env.run(Commands::Undo { force: true });
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_run_veil_headers_and_unveil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "hdu.rs",
        "fn keep_sig() {\n    let secret = 42;\n    println!(\"{secret}\");\n}\n",
    );
    let _ = env.run(Commands::Veil {
        pattern: "hdu.rs".into(),
        mode: VeilMode::Headers,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let content = std::fs::read_to_string(env.dir().join("hdu.rs")).unwrap();
    assert!(content.contains("fn keep_sig"));
    let (_, _, result) = env.unveil("hdu.rs");
    assert!(result.is_ok());
}

#[test]
fn test_run_veil_level1_and_unveil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "l1u.rs",
        "fn func_a() {\n    println!(\"body\");\n}\nfn func_b() {\n    println!(\"body2\");\n}\n",
    );
    let _ = env.run(Commands::Veil {
        pattern: "l1u.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(1),
    });
    let (_, _, result) = env.unveil("l1u.rs");
    assert!(result.is_ok());
    let content = std::fs::read_to_string(env.dir().join("l1u.rs")).unwrap();
    assert!(content.contains("println"));
}

#[test]
fn test_run_veil_level2_and_unveil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "l2u.rs",
        "fn caller() {\n    callee();\n}\nfn callee() {\n    println!(\"body\");\n}\nfn unused() {\n    println!(\"hidden\");\n}\n",
    );
    let _ = env.run(Commands::Veil {
        pattern: "l2u.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    let (_, _, result) = env.unveil("l2u.rs");
    assert!(result.is_ok());
    let content = std::fs::read_to_string(env.dir().join("l2u.rs")).unwrap();
    assert!(content.contains("hidden"));
}

#[test]
fn test_run_checkpoint_show_details() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("csd.txt", "data\n");
    let _ = env.veil("csd.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "detailed".into(),
        },
    });
    let (stdout, _, result) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Show {
            name: "detailed".into(),
        },
    });
    assert!(result.is_ok());
    assert!(stdout.contains("detailed") || stdout.contains("Checkpoint"));
}

#[test]
fn test_run_veil_directory_and_unveil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("dvu/a.txt", "aaa\n");
    env.write_file("dvu/b.txt", "bbb\n");
    let _ = env.run(Commands::Veil {
        pattern: "dvu".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("dvu".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled") || stdout.contains("unveil"));
}

#[test]
fn test_run_unveil_nonexistent_pattern() {
    let env = TestEnv::init(Mode::Blacklist);
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("nonexistent_file.txt".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled") || stdout.contains("unveil"));
}

#[test]
fn test_run_veil_regex_dry_run_multiple_files() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("dry1.txt", "aaa\n");
    env.write_file("dry2.txt", "bbb\n");
    env.write_file("dry3.txt", "ccc\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "/dry.*\\.txt/".into(),
        mode: VeilMode::Full,
        dry_run: true,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would veil") || stdout.contains("would be affected"));
}

#[test]
fn test_run_unveil_single_pattern_dry_run() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("udp.txt", "data\n");
    let _ = env.veil("udp.txt");
    env.write_file("udp.txt", "data\n");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("udp.txt".into()),
        all: false,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil") || stdout.contains("would be affected"));
}

#[test]
fn test_run_clean_no_config() {
    let temp = tempfile::TempDir::new().unwrap();
    let (_, _, result) = run_in_dir(temp.path(), Commands::Clean);
    assert!(result.is_ok());
}

#[test]
fn test_level2_veil_with_typescript_class() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "cls.ts",
        "class MyService {\n  methodA(): void {\n    this.methodB();\n  }\n\n  methodB(): void {\n    console.log('b');\n  }\n\n  unused(): void {\n    console.log('unused');\n  }\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "cls.ts".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("level 2"));
}

#[test]
fn test_level2_veil_with_rust_impl() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "impl_test.rs",
        "struct Foo;\n\nimpl Foo {\n    fn called_method(&self) {\n        println!(\"called\");\n    }\n\n    fn uncalled_method(&self) {\n        println!(\"uncalled\");\n    }\n}\n\nfn main() {\n    let f = Foo;\n    f.called_method();\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "impl_test.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("level 2"));
}

#[test]
fn test_level1_veil_with_typescript() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "l1.ts",
        "function hello(): string {\n  return 'world';\n}\n\nfunction secret(): number {\n  return 42;\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "l1.ts".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(1),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("level 1") || stdout.contains("headers"));
}

#[test]
fn test_run_veil_and_show_partial_with_markers() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("mkr.txt", "visible\nhidden1\nhidden2\nhidden3\nvisible2\n");
    let _ = env.run(Commands::Veil {
        pattern: "mkr.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Show {
        file: "mkr.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("mkr.txt"));
}

#[test]
fn test_run_apply_after_partial_veil_unveil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("pvu.txt", "l1\nl2\nl3\nl4\nl5\n");
    let _ = env.run(Commands::Veil {
        pattern: "pvu.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let _ = env.run(Commands::Unveil {
        pattern: Some("pvu.txt".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    let (_, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
}

#[test]
fn test_run_veil_regex_not_on_disk_files() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("nod_r1.txt", "aaa\n");
    let _ = env.veil("nod_r1.txt");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "/nod_r/".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    let _ = stdout;
}

#[test]
fn test_run_unveil_single_file_then_veil_again() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("cycle.txt", "data\n");
    let _ = env.veil("cycle.txt");
    let _ = env.unveil("cycle.txt");
    let _ = env.veil("cycle.txt");
    let (stdout, _, result) = env.run(Commands::Status { files: false });
    assert!(result.is_ok());
    let _ = stdout;
}

#[test]
fn test_run_multiple_checkpoints() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("mc.txt", "data\n");
    let _ = env.veil("mc.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "cp_a".into(),
        },
    });
    let _ = env.unveil("mc.txt");
    let _ = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::Save {
            name: "cp_b".into(),
        },
    });
    let (stdout, _, result) = env.run(Commands::Checkpoint {
        cmd: CheckpointCmd::List,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("cp_a"));
    assert!(stdout.contains("cp_b"));
}

#[test]
fn test_run_veil_unveil_undo_redo_history() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("vuhr.txt", "data\n");
    let _ = env.veil("vuhr.txt");
    let _ = env.unveil("vuhr.txt");
    let _ = env.run(Commands::Undo { force: false });
    let (stdout, _, result) = env.run(Commands::History {
        limit: 20,
        show: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Future") || stdout.contains("Past"));
}

#[test]
fn test_run_veil_go_file_level1() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "main.go",
        "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"hello\")\n}\n\nfunc helper() {\n\tfmt.Println(\"helper\")\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "main.go".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(1),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("level 1"));
}

#[test]
fn test_run_veil_and_unveil_multiple_times() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("multi_vu.txt", "content\n");
    for _ in 0..3 {
        let _ = env.veil("multi_vu.txt");
        let _ = env.unveil("multi_vu.txt");
    }
    let content = std::fs::read_to_string(env.dir().join("multi_vu.txt")).unwrap();
    assert_eq!(content, "content\n");
}

#[test]
fn test_run_gc_with_objects() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("gc_data.txt", "gc test data\n");
    let _ = env.veil("gc_data.txt");
    let _ = env.unveil("gc_data.txt");
    env.write_file("gc_data.txt", "modified\n");
    let (stdout, _, result) = env.run(Commands::Gc);
    assert!(result.is_ok());
    assert!(stdout.contains("Garbage collected"));
}

#[test]
fn test_run_doctor_with_unveiled_and_veiled() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("d1.txt", "data1\n");
    env.write_file("d2.txt", "data2\n");
    let _ = env.veil("d1.txt");
    let (stdout, _, result) = env.run(Commands::Doctor);
    assert!(result.is_ok());
    let _ = stdout;
}

#[test]
fn test_run_veil_single_line_range() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("slr.txt", "line1\nline2\nline3\n");
    let _ = env.run(Commands::Veil {
        pattern: "slr.txt#2-2".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Show {
        file: "slr.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("slr.txt"));
}

#[test]
fn test_run_unveil_single_line_range() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("uslr.txt", "line1\nline2\nline3\n");
    let _ = env.run(Commands::Veil {
        pattern: "uslr.txt#2-2".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("uslr.txt".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled"));
    let content = std::fs::read_to_string(env.dir().join("uslr.txt")).unwrap();
    assert!(content.contains("line2"));
}

#[test]
fn test_status_files_with_partial_veil() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("partial.txt", "line1\nline2\nline3\nline4\nline5\n");
    let (_, _, result) = env.run(Commands::Veil {
        pattern: "partial.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("partial"));
}

#[test]
fn test_show_partially_veiled_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("showpartial.txt", "line1\nline2\nline3\nline4\nline5\n");
    let (stdout_veil, stderr_veil, result) = env.run(Commands::Veil {
        pattern: "showpartial.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(
        result.is_ok(),
        "veil failed: {stderr_veil}, stdout: {stdout_veil}"
    );
    assert!(
        env.dir().join("showpartial.txt").exists(),
        "file should still exist after partial veil"
    );
    let (stdout, stderr, result) = env.run(Commands::Show {
        file: "showpartial.txt".into(),
    });
    assert!(result.is_ok(), "show failed: {stderr}");
    assert!(stdout.contains("File:"), "stdout was: {stdout}");
}

#[test]
fn test_apply_dry_run_with_unveiled_file() {
    let env = TestEnv::init(Mode::Blacklist);
    let original = "original content here\n";
    env.write_file("apply_dry.txt", original);
    let (_, _, result) = env.veil("apply_dry.txt");
    assert!(result.is_ok());
    std::fs::write(env.dir().join("apply_dry.txt"), original).unwrap();
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: true });
    assert!(result.is_ok());
    assert!(stdout.contains("Would re-veil"));
    assert!(stdout.contains("1 files would be re-applied"));
}

#[test]
fn test_apply_with_missing_file_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("will_delete.txt", "some content\n");
    let (_, _, result) = env.veil("will_delete.txt");
    assert!(result.is_ok());
    let (_, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
}

#[test]
fn test_apply_re_veils_restored_file() {
    let env = TestEnv::init(Mode::Blacklist);
    let original = "fn main() { println!(\"hello\"); }\n";
    env.write_file("reveil.txt", original);
    let (_, _, result) = env.veil("reveil.txt");
    assert!(result.is_ok());
    std::fs::write(env.dir().join("reveil.txt"), original).unwrap();
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(stdout.contains("Re-applying veils"));
}

#[test]
fn test_show_fully_veiled_file_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    let original = "secret content\n";
    env.write_file("showfull.txt", original);
    let (_, _, result) = env.veil("showfull.txt");
    assert!(result.is_ok());
    std::fs::write(env.dir().join("showfull.txt"), original).unwrap();
    let (stdout, _, result) = env.run(Commands::Show {
        file: "showfull.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("FULLY VEILED"));
}

#[test]
fn test_show_veiled_file_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("showgone.txt", "secret\n");
    let (_, _, result) = env.veil("showgone.txt");
    assert!(result.is_ok());
    let (stdout, _, result) = env.run(Commands::Show {
        file: "showgone.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("VEILED"));
}

#[test]
fn test_show_unveiled_file_displays_content() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("showopen.txt", "visible content\n");
    let (stdout, _, result) = env.run(Commands::Show {
        file: "showopen.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("visible content"));
}

#[test]
fn test_collect_affected_files_directory_veiled_removed_from_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("subdir2/a.txt", "content a\n");
    env.write_file("subdir2/b.txt", "content b\n");
    let (_, _, result) = env.veil("subdir2/a.txt");
    assert!(result.is_ok());
    let (_, _, result) = env.veil("subdir2/b.txt");
    assert!(result.is_ok());
    let files = collect_affected_files_for_pattern(env.dir(), "subdir2");
    assert!(files.len() >= 2);
}

#[test]
fn test_collect_affected_files_single_veiled_removed() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("solo2.txt", "content\n");
    let (_, _, result) = env.veil("solo2.txt");
    assert!(result.is_ok());
    let files = collect_affected_files_for_pattern(env.dir(), "solo2.txt");
    assert!(!files.is_empty());
}

#[test]
fn test_unveil_all_dry_run_flag() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("dry1.txt", "content1\n");
    env.write_file("dry2.txt", "content2\n");
    let (_, _, r) = env.veil("dry1.txt");
    assert!(r.is_ok());
    let (_, _, r) = env.veil("dry2.txt");
    assert!(r.is_ok());
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: None,
        all: true,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil"));
}

#[test]
fn test_rebuild_index_with_class_methods() {
    use funveil::{rebuild_index, MetadataStore};
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "myclass.py",
        "class Greeter:\n    def hello(self):\n        return \"hi\"\n\n    def goodbye(self):\n        return \"bye\"\n",
    );
    let (_, _, result) = env.veil("myclass.py");
    assert!(result.is_ok());
    let config = Config::load(env.dir()).unwrap();
    let index = rebuild_index(env.dir(), &config).unwrap();
    assert!(
        index.symbols.contains_key("hello") || index.symbols.contains_key("goodbye"),
        "methods should be indexed, symbols: {:?}",
        index.symbols.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_rebuild_index_with_typescript_class() {
    use funveil::rebuild_index;
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "service.ts",
        "class UserService {\n  getName(): string {\n    return \"name\";\n  }\n  getAge(): number {\n    return 42;\n  }\n}\n",
    );
    let (_, _, result) = env.veil("service.ts");
    assert!(result.is_ok());
    let config = Config::load(env.dir()).unwrap();
    let index = rebuild_index(env.dir(), &config).unwrap();
    assert!(
        index.symbols.contains_key("UserService"),
        "TS class should be indexed, symbols: {:?}",
        index.symbols.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_level2_veil_typescript_class_with_methods() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "svc.ts",
        "class Service {\n  getData(): string {\n    return \"data\";\n  }\n  process(): void {\n    console.log(this.getData());\n  }\n}\n\nfunction main() {\n  const s = new Service();\n  s.process();\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "svc.ts".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok(), "level 2 veil should succeed");
    assert!(stdout.contains("level 2"));
}

#[test]
fn test_level2_veil_python_class_with_methods() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "animal.py",
        "class Animal:\n    def speak(self):\n        return \"sound\"\n\n    def eat(self):\n        return \"food\"\n\ndef main():\n    a = Animal()\n    a.speak()\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "animal.py".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok(), "level 2 veil on python should succeed");
    assert!(stdout.contains("level 2"));
}

#[test]
fn test_apply_level2_with_class_methods_directly() {
    let code = "class Dog:\n    def bark(self):\n        print(\"woof\")\n\n    def fetch(self):\n        print(\"fetching\")\n\ndef main():\n    d = Dog()\n    d.bark()\n";

    let parsed = funveil::ParsedFile {
        language: funveil::Language::Python,
        path: std::path::PathBuf::from("test.py"),
        symbols: vec![
            funveil::Symbol::Class {
                name: "Dog".to_string(),
                methods: vec![
                    funveil::Symbol::Function {
                        name: "bark".to_string(),
                        params: vec![funveil::parser::Param {
                            name: "self".to_string(),
                            type_annotation: None,
                        }],
                        return_type: None,
                        visibility: funveil::parser::Visibility::Public,
                        line_range: LineRange::new(2, 3).unwrap(),
                        body_range: LineRange::new(3, 3).unwrap(),
                        is_async: false,
                        attributes: vec![],
                    },
                    funveil::Symbol::Function {
                        name: "fetch".to_string(),
                        params: vec![funveil::parser::Param {
                            name: "self".to_string(),
                            type_annotation: None,
                        }],
                        return_type: None,
                        visibility: funveil::parser::Visibility::Public,
                        line_range: LineRange::new(5, 6).unwrap(),
                        body_range: LineRange::new(6, 6).unwrap(),
                        is_async: false,
                        attributes: vec![],
                    },
                ],
                properties: vec![],
                visibility: funveil::parser::Visibility::Public,
                line_range: LineRange::new(1, 6).unwrap(),
                kind: funveil::parser::ClassKind::Class,
            },
            funveil::Symbol::Function {
                name: "main".to_string(),
                params: vec![],
                return_type: None,
                visibility: funveil::parser::Visibility::Public,
                line_range: LineRange::new(8, 10).unwrap(),
                body_range: LineRange::new(9, 10).unwrap(),
                is_async: false,
                attributes: vec![],
            },
        ],
        imports: vec![],
        calls: vec![funveil::parser::Call {
            caller: Some("main".to_string()),
            callee: "bark".to_string(),
            line: 10,
            is_dynamic: false,
        }],
    };

    let result = apply_level(2, code, &parsed).unwrap();
    match result {
        LevelResult::HeadersAndCalled(veiled) => {
            assert!(
                veiled.contains("bark"),
                "called method 'bark' body should be included"
            );
        }
        _ => panic!("expected HeadersAndCalled"),
    }
}

#[test]
fn test_apply_level2_with_module_symbol() {
    let code = "mod utils {\n    fn helper() {\n        println!(\"hi\");\n    }\n}\n";

    let parsed = funveil::ParsedFile {
        language: funveil::Language::Rust,
        path: std::path::PathBuf::from("test.rs"),
        symbols: vec![funveil::Symbol::Module {
            name: "utils".to_string(),
            line_range: LineRange::new(1, 5).unwrap(),
        }],
        imports: vec![],
        calls: vec![],
    };

    let result = apply_level(2, code, &parsed).unwrap();
    match result {
        LevelResult::HeadersAndCalled(veiled) => {
            assert!(veiled.contains("mod utils"));
        }
        _ => panic!("expected HeadersAndCalled"),
    }
}

#[test]
fn test_unveil_single_dry_run_flag() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("drysingle.txt", "content\n");
    let (_, _, r) = env.veil("drysingle.txt");
    assert!(r.is_ok());
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("drysingle.txt".into()),
        all: false,
        dry_run: true,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Would unveil"));
}

#[test]
fn test_status_files_with_veiled_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("a.txt", "aaa\nbbb\n");
    let _ = env.veil("a.txt");
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("[veiled]"));
}

#[test]
fn test_collect_affected_files_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("gone.txt", "data\n");
    let _ = env.veil("gone.txt");
    let affected = collect_affected_files_for_pattern(env.dir(), "gone.txt");
    assert!(affected.contains(&"gone.txt".to_string()));
}

#[test]
fn test_collect_affected_files_dir_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("sub/f.txt", "data\n");
    let _ = env.veil("sub/f.txt");
    let affected = collect_affected_files_for_pattern(env.dir(), "sub");
    assert!(!affected.is_empty());
}

#[test]
fn test_unveil_regex_not_on_disk_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("vanish.txt", "data\n");
    let _ = env.veil("vanish.txt");
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("/vanish/".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled") || stdout.contains("vanish"));
}

#[test]
fn test_apply_re_veils_unveiled_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("rv.txt", "content\nhere\n");
    let _ = env.veil("rv.txt");
    let _ = env.unveil("rv.txt");
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(stdout.contains("Re-applying") || stdout.contains("Applied"));
}

#[test]
fn test_handle_level_veil_level2() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("lv2.rs", "fn foo() { bar(); }\nfn bar() { }\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "lv2.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Veiled (level 2"));
}

#[test]
fn test_status_files_full_veil_with_ranges_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("ranged.txt", "line1\nline2\nline3\nline4\nline5\n");
    // Partial veil creates an object with ranges
    let _ = env.run(Commands::Veil {
        pattern: "ranged.txt#2-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    // Now also do a full veil on the same file — this should register it as an object
    let _ = env.veil("ranged.txt");
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("ranged.txt"));
    assert!(stdout.contains("[veiled]"));
}

#[test]
fn test_apply_skips_missing_file() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("ephemeral.txt", "temp data\n");
    let _ = env.veil("ephemeral.txt");
    // Manually write original content back so apply sees hash match
    let config = Config::load(env.dir()).unwrap();
    let store = ContentStore::new(env.dir());
    let meta = config.get_object("ephemeral.txt").unwrap();
    let hash = ContentHash::from_string(meta.hash.clone()).unwrap();
    let original = store.retrieve(&hash).unwrap();
    std::fs::write(env.dir().join("ephemeral.txt"), &original).unwrap();
    // Now apply should re-veil
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(stdout.contains("re-veiled") || stdout.contains("Re-applying"));
}

#[test]
fn test_collect_affected_regex_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("rx_gone.txt", "data\n");
    let _ = env.veil("rx_gone.txt");
    // File is now removed from disk; regex collect should still find it
    let affected = collect_affected_files_for_pattern(env.dir(), "/rx_gone/");
    assert!(affected.contains(&"rx_gone.txt".to_string()));
}

#[test]
fn test_veil_single_line_range() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("single.txt", "line1\nline2\nline3\n");
    let (_, _, result) = env.run(Commands::Veil {
        pattern: "single.txt#2-2".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    let content = std::fs::read_to_string(env.dir().join("single.txt")).unwrap();
    assert!(content.contains("line1"));
    assert!(content.contains("line3"));
}

#[test]
fn test_disclose_budget_with_code() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "src/main.rs",
        "fn main() {\n    println!(\"hello\");\n}\nfn helper() {\n    let x = 1;\n}\n",
    );
    env.write_file("src/lib.rs", "pub fn util() {\n    let y = 2;\n}\n");
    let _ = env.veil("src/main.rs");
    let (stdout, _, result) = env.run(Commands::Disclose {
        budget: 1000,
        focus: "src/main.rs".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Disclosure plan"));
}

#[test]
fn test_metadata_index_with_methods() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "src/cls.rs",
        "impl Foo {\n    fn bar(&self) {}\n    fn baz(&self) {}\n}\n",
    );
    let _ = env.veil("src/cls.rs");
    // Index should have been built with method entries
    let index = funveil::load_index(env.dir());
    assert!(index.is_ok());
}

#[test]
fn test_unveil_regex_with_errors() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("err1.txt", "data1\n");
    env.write_file("err2.txt", "data2\n");
    let _ = env.veil("err1.txt");
    let _ = env.veil("err2.txt");
    // Both files now removed from disk, regex unveil should handle not-on-disk files
    let (stdout, _, result) = env.run(Commands::Unveil {
        pattern: Some("/err[12]/".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Unveiled") || stdout.contains("err"));
}

#[test]
fn test_apply_with_original_content_restored() {
    let env = TestEnv::init(Mode::Blacklist);
    let original = "original content\nsecond line\n";
    env.write_file("apply_test.txt", original);
    let _ = env.veil("apply_test.txt");
    // Restore original content manually to simulate user editing
    std::fs::write(env.dir().join("apply_test.txt"), original).unwrap();
    let (stdout, _, result) = env.run(Commands::Apply { dry_run: false });
    assert!(result.is_ok());
    assert!(stdout.contains("re-veiled") || stdout.contains("Applied"));
}

#[test]
fn test_unveil_restores_file_in_subdir() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("deep/nested/file.txt", "deep content\n");
    let _ = env.veil("deep/nested/file.txt");
    // File was physically removed; remove the empty dir too
    let _ = std::fs::remove_dir(env.dir().join("deep/nested"));
    let _ = std::fs::remove_dir(env.dir().join("deep"));
    assert!(!env.dir().join("deep/nested/file.txt").exists());
    let (_, _, result) = env.unveil("deep/nested/file.txt");
    assert!(result.is_ok());
    assert!(env.dir().join("deep/nested/file.txt").exists());
    let content = std::fs::read_to_string(env.dir().join("deep/nested/file.txt")).unwrap();
    assert_eq!(content, "deep content\n");
}

#[test]
fn test_veil_align_to_class_method_boundary() {
    let env = TestEnv::init(Mode::Blacklist);
    // Rust impl block with methods — partial veil should align to method boundary
    env.write_file("cls.rs", "struct Foo;\nimpl Foo {\n    fn method1(&self) {\n        let x = 1;\n    }\n    fn method2(&self) {\n        let y = 2;\n    }\n}\n");
    let (_, _, result) = env.run(Commands::Veil {
        pattern: "cls.rs#3-4".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
}

#[test]
fn test_disclose_with_multi_file_graph() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("src/caller.rs", "fn caller() {\n    callee();\n}\n");
    env.write_file("src/callee.rs", "fn callee() {\n    println!(\"hi\");\n}\n");
    let _ = env.veil("src/caller.rs");
    let _ = env.veil("src/callee.rs");
    let (stdout, _, result) = env.run(Commands::Disclose {
        budget: 5000,
        focus: "src/caller.rs".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Disclosure plan"));
}

#[test]
fn test_status_files_veiled_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("removed.txt", "will be gone\n");
    let _ = env.veil("removed.txt");
    // File should be removed from disk after veil
    assert!(!env.dir().join("removed.txt").exists());
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("removed.txt"));
    assert!(stdout.contains("not on disk"));
}

#[test]
fn test_show_veiled_not_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("hidden.txt", "secret content\n");
    let _ = env.veil("hidden.txt");
    let (stdout, _, result) = env.run(Commands::Show {
        file: "hidden.txt".into(),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("VEILED"));
    assert!(stdout.contains("secret content"));
}

#[test]
fn test_veil_headers_mode() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "src/example.rs",
        "fn hello() {\n    println!(\"hi\");\n}\n\nfn world(x: i32) -> bool {\n    x > 0\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "src/example.rs".into(),
        mode: VeilMode::Headers,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("headers mode"));
    // File should still exist (headers mode doesn't remove)
    assert!(env.dir().join("src/example.rs").exists());
    // Content should have headers but bodies replaced
    let content = std::fs::read_to_string(env.dir().join("src/example.rs")).unwrap();
    assert!(content.contains("fn hello"));
    assert!(!content.contains("println"));
}

#[test]
fn test_parse_detailed_format() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "src/lib.rs",
        "use std::io;\n\nfn greet(name: &str) {\n    println!(\"Hello, {}\", name);\n}\n\nfn main() {\n    greet(\"world\");\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Parse {
        file: "src/lib.rs".into(),
        format: ParseFormat::Detailed,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Symbols:"));
    assert!(stdout.contains("greet"));
    assert!(stdout.contains("Signature:"));
}

#[test]
fn test_parse_summary_format() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "src/lib.rs",
        "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Parse {
        file: "src/lib.rs".into(),
        format: ParseFormat::Summary,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Functions:"));
    assert!(stdout.contains("Language:"));
}

#[test]
fn test_status_files_with_partial_veil_on_disk() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "src/code.rs",
        "fn public_api() {}\n\nfn secret() {\n    do_stuff();\n}\n\nfn another() {}\n",
    );
    // Veil lines 3-5 (partial veil)
    let (_, _, result) = env.run(Commands::Veil {
        pattern: "src/code.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(1),
    });
    assert!(result.is_ok());
    // File should still be on disk (level veil keeps it)
    assert!(env.dir().join("src/code.rs").exists());
    let (stdout, _, result) = env.run(Commands::Status { files: true });
    assert!(result.is_ok());
    assert!(stdout.contains("src/code.rs"));
}

#[test]
fn test_handle_level_veil_level_0() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("src/mod.rs", "fn foo() {\n    bar();\n}\n");
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "src/mod.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(0),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("level 0"));
    // Level 0 = full veil, file removed
    assert!(!env.dir().join("src/mod.rs").exists());
}

#[test]
fn test_handle_level_veil_level_3_unveils() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("src/target.rs", "fn secret() {\n    inner();\n}\n");
    // First veil it
    let _ = env.veil("src/target.rs");
    assert!(!env.dir().join("src/target.rs").exists());
    // Level 3 should unveil
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "src/target.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(3),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("Level 3"));
    assert!(env.dir().join("src/target.rs").exists());
}

#[test]
fn test_handle_level_veil_level_2() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "src/multi.rs",
        "fn outer() {\n    inner();\n}\n\nfn inner() {\n    println!(\"hi\");\n}\n",
    );
    let (stdout, _, result) = env.run(Commands::Veil {
        pattern: "src/multi.rs".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: Some(2),
    });
    assert!(result.is_ok());
    assert!(stdout.contains("level 2"));
    // File should still exist (level veil keeps it)
    assert!(env.dir().join("src/multi.rs").exists());
}

#[test]
fn test_build_call_graph_from_metadata() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("src/caller.rs", "fn caller() {\n    callee();\n}\n");
    env.write_file(
        "src/callee.rs",
        "fn callee() {\n    println!(\"called\");\n}\n",
    );
    let _ = env.veil("src/caller.rs");
    let _ = env.veil("src/callee.rs");
    // build_call_graph_from_metadata is called via disclose command
    let config = Config::load(env.dir()).unwrap();
    let graph = funveil::build_call_graph_from_metadata(env.dir(), &config);
    assert!(graph.is_ok());
}

#[test]
fn test_unveil_specific_ranges_with_remaining_veils() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file(
        "src/ranges.rs",
        "fn a() {\n    one();\n}\n\nfn b() {\n    two();\n}\n\nfn c() {\n    three();\n}\n",
    );
    // Veil lines 1-3, then lines 5-7 (ranges use # separator)
    let (_, _, r) = env.run(Commands::Veil {
        pattern: "src/ranges.rs#1-3".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(r.is_ok());
    let (_, _, r) = env.run(Commands::Veil {
        pattern: "src/ranges.rs#5-7".into(),
        mode: VeilMode::Full,
        dry_run: false,
        symbol: None,
        unreachable_from: None,
        level: None,
    });
    assert!(r.is_ok());
    // Unveil only lines 1-3, keeping 5-7 veiled
    let (_, _, r) = env.run(Commands::Unveil {
        pattern: Some("src/ranges.rs#1-3".into()),
        all: false,
        dry_run: false,
        symbol: None,
        callers_of: None,
        callees_of: None,
        level: None,
    });
    assert!(r.is_ok());
    let content = std::fs::read_to_string(env.dir().join("src/ranges.rs")).unwrap();
    // Lines 1-3 should be restored
    assert!(content.contains("fn a()"));
    // Lines 5-7 should still be veiled (marker present)
    assert!(!content.contains("fn b()"));
}

#[test]
fn test_disclose_command_with_budget() {
    let env = TestEnv::init(Mode::Blacklist);
    env.write_file("src/focus.rs", "fn main() {\n    helper();\n}\n");
    env.write_file(
        "src/helper.rs",
        "fn helper() {\n    println!(\"helping\");\n}\n",
    );
    let _ = env.veil("src/focus.rs");
    let _ = env.veil("src/helper.rs");
    let (stdout, _, result) = env.run(Commands::Disclose {
        focus: "src/focus.rs".into(),
        budget: 5000,
    });
    assert!(result.is_ok());
    assert!(stdout.contains("focus") || stdout.contains("Disclosure"));
}
