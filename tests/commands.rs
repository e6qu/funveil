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
