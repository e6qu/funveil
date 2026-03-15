use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::Write;
use tracing::info_span;

use crate::{
    apply_level, check_integrity, command_category, delete_checkpoint, garbage_collect,
    generate_trace_id, get_latest_checkpoint, has_veils, is_supported_source, list_checkpoints,
    normalize_path, restore_checkpoint, save_checkpoint, show_checkpoint, snapshot_config,
    snapshot_files, unveil_all, unveil_file, veil_file, walk_files, ActionHistory, ActionRecord,
    ActionState, ActionSummary, CallGraphBuilder, CommandResult, Config, ContentHash, ContentStore,
    EntrypointDetector, FileDiff, FileStatus, HeaderStrategy, HistoryTracker, LevelResult,
    LineRange, Mode, ObjectMeta, Output, TraceDirection, TreeSitterParser, CONFIG_FILE,
};
#[cfg(not(target_family = "wasm"))]
use crate::{init_tracing, resolve_log_level};

#[derive(Parser)]
#[command(name = "fv")]
#[command(about = "Funveil - Control file visibility in AI agent workspaces")]
#[command(version = env!("FV_VERSION"))]
pub struct Cli {
    /// Suppress output
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Log level (trace, debug, info, warn, error, off)
    #[arg(long, global = true)]
    pub log_level: Option<String>,

    /// Output as JSON (for machine consumption)
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub fn version_long() -> String {
    format!(
        concat!("fv {}\n", "commit: {}\n", "target: {}\n", "profile: {}",),
        env!("FV_VERSION"),
        env!("FV_GIT_SHA"),
        env!("FV_BUILD_TARGET"),
        env!("FV_BUILD_PROFILE"),
    )
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum VeilMode {
    /// Veil entire files
    Full,
    /// Show only headers (signatures), hide implementations
    Headers,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum ParseFormat {
    /// Summary of symbols found
    Summary,
    /// Detailed symbol list
    Detailed,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum TraceFormat {
    /// Tree view of call hierarchy
    Tree,
    /// Flat list view
    List,
    /// DOT format for graph visualization
    Dot,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum EntrypointTypeArg {
    /// Main entry points
    Main,
    /// Test functions
    Test,
    /// CLI handlers
    Cli,
    /// Web/API handlers
    Handler,
    /// Library exports
    Export,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum LanguageArg {
    /// Rust
    Rust,
    /// Go
    Go,
    /// TypeScript
    TypeScript,
    /// Python
    Python,
    /// Bash/Shell
    Bash,
    /// Terraform/HCL
    Terraform,
    /// Helm/YAML
    Helm,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize funveil in current directory
    Init {
        /// Mode to use (whitelist or blacklist)
        #[arg(long, default_value = "whitelist")]
        mode: Mode,
    },

    /// Show or change mode
    Mode {
        /// New mode (if not specified, shows current)
        mode: Option<Mode>,
    },

    /// Show current veil state
    Status {
        /// Show per-file details
        #[arg(long)]
        files: bool,
    },

    /// Add file/directory to whitelist or unveil all
    Unveil {
        /// Pattern to whitelist (file, directory, or pattern with line ranges)
        pattern: Option<String>,
        /// Unveil all veiled files
        #[arg(long, conflicts_with = "pattern")]
        all: bool,
        /// Preview what would be unveiled without making changes
        #[arg(long)]
        dry_run: bool,
        /// Unveil the file containing this symbol
        #[arg(long)]
        symbol: Option<String>,
        /// Unveil files containing callers of this function
        #[arg(long)]
        callers_of: Option<String>,
        /// Unveil files containing callees of this function
        #[arg(long)]
        callees_of: Option<String>,
        /// Disclosure level (0=remove, 1=headers, 2=headers+called bodies, 3=full)
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=3))]
        level: Option<u8>,
    },

    /// Add file/directory to blacklist
    Veil {
        /// Pattern to blacklist (file, directory, or pattern with optional line ranges)
        pattern: String,
        /// Veiling mode (headers or full)
        #[arg(long, value_enum, default_value = "full")]
        mode: VeilMode,
        /// Preview what would be veiled without making changes
        #[arg(long)]
        dry_run: bool,
        /// Veil the lines containing this symbol (partial veil)
        #[arg(long)]
        symbol: Option<String>,
        /// Veil all files not reachable from this function
        #[arg(long)]
        unreachable_from: Option<String>,
        /// Disclosure level (0=remove, 1=headers, 2=headers+called bodies, 3=full)
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=3))]
        level: Option<u8>,
    },

    /// Parse and display symbols from a file (for debugging)
    Parse {
        /// File to parse
        file: String,
        /// Output format
        #[arg(long, value_enum, default_value = "summary")]
        format: ParseFormat,
    },

    /// Trace function calls in the codebase
    Trace {
        /// Function name to start tracing from (use with --from)
        function: Option<String>,
        /// Function to trace from (shows what this function calls)
        #[arg(long, group = "direction")]
        from: Option<String>,
        /// Function to trace to (shows what calls this function)
        #[arg(long, group = "direction")]
        to: Option<String>,
        /// Trace from all detected entrypoints
        #[arg(long, group = "direction")]
        from_entrypoint: bool,
        /// Maximum depth to trace
        #[arg(long, default_value = "3")]
        depth: usize,
        /// Output format
        #[arg(long, value_enum, default_value = "tree")]
        format: TraceFormat,
        /// Filter out standard library functions
        #[arg(long)]
        no_std: bool,
    },

    /// List entrypoints in the codebase
    Entrypoints {
        /// Filter by entrypoint type
        #[arg(long, value_enum)]
        entry_type: Option<EntrypointTypeArg>,
        /// Filter by language
        #[arg(long, value_enum)]
        language: Option<LanguageArg>,
    },

    /// Cache operations
    Cache {
        #[command(subcommand)]
        cmd: CacheCmd,
    },

    /// Re-apply veils to all files
    Apply {
        /// Preview what would be re-applied without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Restore previous veil state
    Restore,

    /// Display file with veil annotations
    Show {
        /// File to show
        file: String,
    },

    /// Checkpoint operations
    Checkpoint {
        #[command(subcommand)]
        cmd: CheckpointCmd,
    },

    /// Check veil integrity
    Doctor,

    /// Garbage collect unused objects
    Gc,

    /// Remove all funveil data
    Clean,

    /// Show detailed version information
    Version,

    /// Show context around a function
    Context {
        /// Function name to show context for
        function: String,
        /// Maximum depth to trace
        #[arg(long, default_value = "2")]
        depth: usize,
    },

    /// Undo last action
    Undo {
        /// Force undo of non-undoable actions
        #[arg(long)]
        force: bool,
    },

    /// Redo previously undone action
    Redo,

    /// Show action history
    History {
        /// Maximum number of entries to show
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Show detailed view of a specific action
        #[arg(long)]
        show: Option<u64>,
    },

    /// Disclose code within a token budget
    Disclose {
        /// Maximum token budget
        #[arg(long)]
        budget: usize,
        /// Focus file or function
        #[arg(long)]
        focus: String,
    },
}

impl Commands {
    pub fn name(&self) -> &'static str {
        match self {
            Commands::Init { .. } => "init",
            Commands::Mode { .. } => "mode",
            Commands::Status { .. } => "status",
            Commands::Unveil { .. } => "unveil",
            Commands::Veil { .. } => "veil",
            Commands::Parse { .. } => "parse",
            Commands::Trace { .. } => "trace",
            Commands::Entrypoints { .. } => "entrypoints",
            Commands::Cache { .. } => "cache",
            Commands::Apply { .. } => "apply",
            Commands::Restore => "restore",
            Commands::Show { .. } => "show",
            Commands::Checkpoint { .. } => "checkpoint",
            Commands::Doctor => "doctor",
            Commands::Gc => "gc",
            Commands::Clean => "clean",
            Commands::Version => "version",
            Commands::Context { .. } => "context",
            Commands::Undo { .. } => "undo",
            Commands::Redo => "redo",
            Commands::History { .. } => "history",
            Commands::Disclose { .. } => "disclose",
        }
    }
}

#[derive(Subcommand)]
pub enum CacheCmd {
    /// Show cache statistics
    Status,
    /// Clear the cache
    Clear,
    /// Invalidate stale entries
    Invalidate,
}

#[derive(Subcommand)]
pub enum CheckpointCmd {
    /// Save current state
    Save { name: String },
    /// Restore saved state
    Restore { name: String },
    /// List all checkpoints
    List,
    /// Show checkpoint details
    Show { name: String },
    /// Delete a checkpoint
    Delete { name: String },
}

pub fn update_metadata(root: &std::path::Path, config: &Config) {
    let _ = crate::rebuild_index(root, config).and_then(|i| crate::save_index(root, &i));
    let _ = crate::generate_manifest(root, config).and_then(|m| crate::save_manifest(root, &m));
}

fn save_and_update(root: &std::path::Path, config: &Config) -> Result<()> {
    config.save(root)?;
    update_metadata(root, config);
    Ok(())
}

pub use crate::history::restore_action_state;

pub fn handle_level_veil(
    root: &std::path::Path,
    pattern: &str,
    level: u8,
    output: &mut Output,
) -> Result<CommandResult> {
    use std::fs;

    if level == 3 {
        let mut config = Config::load(root)?;
        if config.get_object(pattern).is_some() || has_veils(&config, pattern) {
            unveil_file(root, &mut config, pattern, None, output)?;
            save_and_update(root, &config)?;
            let _ = writeln!(output.out, "Level 3: unveiled {pattern} (full source)");
        } else {
            let _ = writeln!(output.out, "Level 3: {pattern} already at full source");
        }
        return Ok(CommandResult::Veil {
            files: vec![pattern.to_string()],
            dry_run: false,
        });
    }

    if level == 0 {
        let mut config = Config::load(root)?;
        let tracker = HistoryTracker::begin(
            &config,
            "veil",
            vec![pattern.to_string(), "--level".to_string(), "0".to_string()],
            &[pattern.to_string()],
            root,
            true,
        );
        veil_file(root, &mut config, pattern, None, output)?;
        config.add_to_blacklist(pattern);
        save_and_update(root, &config)?;
        let _ = writeln!(output.out, "Veiled (level 0): {pattern}");
        tracker.commit(root, &config, format!("Veiled {pattern} (level 0)"))?;
        return Ok(CommandResult::Veil {
            files: vec![pattern.to_string()],
            dry_run: false,
        });
    }

    let path = root.join(pattern);
    if !path.exists() {
        return Err(anyhow::anyhow!("File not found: {pattern}"));
    }
    crate::validate_path_within_root(&path, root)?;

    let mut config = Config::load(root)?;
    let tracker = HistoryTracker::begin(
        &config,
        "veil",
        vec![
            pattern.to_string(),
            "--level".to_string(),
            level.to_string(),
        ],
        &[pattern.to_string()],
        root,
        true,
    );

    let content = fs::read_to_string(&path)?;
    let parser = TreeSitterParser::new()?;
    let parsed = parser.parse_file(&path, &content)?;
    let result = apply_level(level, &content, &parsed)?;

    let veiled = match result {
        LevelResult::Headers(v) => v,
        LevelResult::HeadersAndCalled(v) => v,
        _ => unreachable!("level 0 and 3 handled above"),
    };

    let store = ContentStore::new(root);
    let hash = store.store(content.as_bytes())?;
    let permissions = crate::perms::file_mode(&fs::metadata(&path)?);
    fs::write(&path, &veiled)?;

    config.register_object(pattern.to_string(), ObjectMeta::new(hash, permissions));
    config.add_to_blacklist(pattern);
    save_and_update(root, &config)?;

    let label = match level {
        1 => "headers",
        2 => "headers+called bodies",
        _ => unreachable!(),
    };
    let _ = writeln!(output.out, "Veiled (level {level}, {label}): {pattern}");
    tracker.commit(
        root,
        &config,
        format!("Veiled {pattern} (level {level}, {label})"),
    )?;

    Ok(CommandResult::Veil {
        files: vec![pattern.to_string()],
        dry_run: false,
    })
}

pub fn collect_affected_files_for_pattern(root: &std::path::Path, pattern: &str) -> Vec<String> {
    let mut files = if pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() > 2 {
        let regex_str = &pattern[1..pattern.len() - 1];
        if let Ok(regex) = regex::Regex::new(regex_str) {
            let mut result: Vec<String> = walk_files(root)
                .max_depth(None)
                .build()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .filter_map(|e| {
                    let p = e.path().strip_prefix(root).unwrap_or(e.path());
                    let ps = p.to_string_lossy().to_string();
                    if regex.is_match(&ps) {
                        Some(ps)
                    } else {
                        None
                    }
                })
                .collect();
            // Also check config for veiled files not on disk
            if let Ok(config) = Config::load(root) {
                for file in config.iter_unique_files() {
                    if !root.join(&file).exists() && regex.is_match(&file) {
                        result.push(file);
                    }
                }
            }
            result
        } else {
            vec![]
        }
    } else if pattern.contains('#') {
        if let Ok((file, _)) = parse_pattern(pattern) {
            vec![file.to_string()]
        } else {
            vec![pattern.to_string()]
        }
    } else {
        let p = root.join(pattern);
        if p.is_dir() {
            let mut result: Vec<String> = walk_files(root)
                .max_depth(None)
                .build()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file() && e.path().starts_with(&p))
                .map(|e| {
                    let rel = e.path().strip_prefix(root).unwrap_or(e.path());
                    rel.to_string_lossy().to_string()
                })
                .collect();
            // Also check config for veiled files not on disk under this dir
            if let Ok(config) = Config::load(root) {
                for file in config.iter_unique_files() {
                    if !root.join(&file).exists() && file.starts_with(pattern) {
                        result.push(file);
                    }
                }
            }
            result
        } else {
            // Single file pattern — also check if it's a veiled file not on disk
            let mut result = vec![pattern.to_string()];
            if !p.exists() {
                if let Ok(config) = Config::load(root) {
                    for file in config.iter_unique_files() {
                        if !root.join(&file).exists()
                            && file != pattern
                            && file.starts_with(pattern)
                        {
                            result.push(file);
                        }
                    }
                }
            }
            result
        }
    };
    files.dedup();
    files
}

/// Parse a pattern like "file.txt" or "file.txt#1-5" into (file, optional_ranges)
pub use crate::types::parse_pattern;

pub fn run_command(cli: Cli, root: &std::path::Path, output: &mut Output) -> Result<CommandResult> {
    let root = root.to_path_buf();

    // Initialize structured logging (skipped on WASM — no threads for async appender)
    #[cfg(not(target_family = "wasm"))]
    let _guard = {
        let config_log_level = Config::load(&root).ok().and_then(|c| c.log_level);
        let level = resolve_log_level(cli.log_level.as_deref(), config_log_level.as_deref());
        init_tracing(&root, level).ok()
    };

    let cmd_name = cli.command.name();
    let category = command_category(cmd_name);
    let trace_id = generate_trace_id();
    let _root_span =
        info_span!("command", trace_id = %trace_id, name = cmd_name, category = category).entered();

    let cmd_result = match cli.command {
        Commands::Init { mode } => {
            if Config::exists(&root) {
                let _ = writeln!(
                    output.out,
                    "Funveil is already initialized in this directory."
                );
                return Ok(CommandResult::Init {
                    mode: mode.to_string(),
                });
            }

            let config = Config::new(mode);
            crate::config::ensure_data_dir(&root)?;
            crate::config::ensure_gitignore(&root)?;
            config.save(&root)?;

            // Record in history (not undoable)
            let mut history = ActionHistory::load(&root)?;
            let post_config = snapshot_config(&config);
            history.push(ActionRecord {
                id: history.next_id(),
                timestamp: chrono::Utc::now(),
                command: "init".to_string(),
                args: vec!["--mode".to_string(), mode.to_string()],
                summary: format!("Initialized funveil with {mode} mode"),
                affected_files: vec![],
                undoable: false,
                pre_state: ActionState {
                    config_yaml: None,
                    file_snapshots: vec![],
                },
                post_state: ActionState {
                    config_yaml: post_config,
                    file_snapshots: vec![],
                },
            });
            history.save(&root)?;

            let _ = writeln!(output.out, "Initialized funveil with {mode} mode.");
            let _ = writeln!(
                output.out,
                "Configuration: {}",
                root.join(CONFIG_FILE).display()
            );

            CommandResult::Init {
                mode: mode.to_string(),
            }
        }

        Commands::Mode { mode } => {
            let mut config = Config::load(&root)?;

            if let Some(new_mode) = mode {
                let tracker = HistoryTracker::begin(
                    &config,
                    "mode",
                    vec![new_mode.to_string()],
                    &[],
                    &root,
                    true,
                );
                config.set_mode(new_mode);
                config.save(&root)?;
                tracker.commit(&root, &config, format!("Changed mode to {new_mode}"))?;

                let _ = writeln!(output.out, "Mode changed to: {new_mode}");
                CommandResult::ModeResult {
                    mode: new_mode.to_string(),
                    changed: true,
                }
            } else {
                let _ = writeln!(output.out, "Current mode: {}", config.mode());
                CommandResult::ModeResult {
                    mode: config.mode().to_string(),
                    changed: false,
                }
            }
        }

        Commands::Status { files } => {
            let config = Config::load(&root)?;
            let _ = writeln!(output.out, "Mode: {}", config.mode());

            if !config.blacklist.is_empty() {
                let _ = writeln!(output.out, "\nBlacklisted:");
                for entry in &config.blacklist {
                    let _ = writeln!(output.out, "  - {entry}");
                }
            }

            if !config.whitelist.is_empty() {
                let _ = writeln!(output.out, "\nWhitelisted:");
                for entry in &config.whitelist {
                    let _ = writeln!(output.out, "  - {entry}");
                }
            }

            if !config.objects.is_empty() {
                let _ = writeln!(output.out, "\nVeiled objects: {}", config.objects.len());
            }

            let mut unveiled_count = 0usize;
            let mut file_statuses: Option<Vec<FileStatus>> =
                if files { Some(vec![]) } else { None };

            // Count unique veiled files
            let veiled_files: std::collections::HashSet<String> =
                config.iter_unique_files().map(|f| f.to_string()).collect();
            let veiled_count = veiled_files.len();

            if files {
                let mut seen_files = std::collections::HashSet::new();
                // Walk project to enumerate all files on disk
                for entry in walk_files(&root)
                    .max_depth(None)
                    .build()
                    .filter_map(|e| e.ok())
                {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let rel = normalize_path(path, &root);
                    if rel.starts_with(".funveil") || rel == CONFIG_FILE {
                        continue;
                    }

                    seen_files.insert(rel.clone());

                    if config.get_object(&rel).is_some() {
                        let ranges = config.veiled_ranges(&rel).unwrap_or_default();
                        if ranges.is_empty() {
                            file_statuses.as_mut().unwrap().push(FileStatus {
                                path: rel,
                                state: "veiled".to_string(),
                                veil_type: Some("full".to_string()),
                                ranges: None,
                                on_disk: Some(true),
                            });
                        } else {
                            let range_strs: Vec<String> = ranges
                                .iter()
                                .map(|r| format!("{}-{}", r.start(), r.end()))
                                .collect();
                            file_statuses.as_mut().unwrap().push(FileStatus {
                                path: rel,
                                state: "veiled".to_string(),
                                veil_type: Some("partial".to_string()),
                                ranges: Some(range_strs),
                                on_disk: Some(true),
                            });
                        }
                    } else if has_veils(&config, &rel) {
                        let ranges = config.veiled_ranges(&rel).unwrap_or_default();
                        let range_strs: Vec<String> = ranges
                            .iter()
                            .map(|r| format!("{}-{}", r.start(), r.end()))
                            .collect();
                        file_statuses.as_mut().unwrap().push(FileStatus {
                            path: rel,
                            state: "veiled".to_string(),
                            veil_type: Some("partial".to_string()),
                            ranges: Some(range_strs),
                            on_disk: Some(true),
                        });
                    } else {
                        unveiled_count += 1;
                        file_statuses.as_mut().unwrap().push(FileStatus {
                            path: rel,
                            state: "unveiled".to_string(),
                            veil_type: None,
                            ranges: None,
                            on_disk: Some(true),
                        });
                    }
                }

                // Add fully-veiled files that are not on disk (physically removed)
                for file in config.iter_unique_files() {
                    if !seen_files.contains(&file) && config.get_object(&file).is_some() {
                        file_statuses.as_mut().unwrap().push(FileStatus {
                            path: file,
                            state: "veiled".to_string(),
                            veil_type: Some("full".to_string()),
                            ranges: None,
                            on_disk: Some(false),
                        });
                    }
                }

                if let Some(ref statuses) = file_statuses {
                    let _ = writeln!(output.out, "\nFiles:");
                    for fs in statuses {
                        let extra = if let Some(ref vt) = fs.veil_type {
                            if fs.on_disk == Some(false) {
                                format!(" ({vt}, not on disk)")
                            } else {
                                format!(" ({vt})")
                            }
                        } else {
                            String::new()
                        };
                        let _ = writeln!(output.out, "  {} [{}]{}", fs.path, fs.state, extra);
                    }
                }
            } else {
                // Without --files, just report counts
                unveiled_count = walk_files(&root)
                    .max_depth(None)
                    .build()
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let p = e.path();
                        if !p.is_file() {
                            return false;
                        }
                        let rel = normalize_path(p, &root);
                        !rel.starts_with(".funveil")
                            && rel != CONFIG_FILE
                            && !veiled_files.contains(&rel)
                    })
                    .count();
            }

            CommandResult::Status {
                mode: config.mode().to_string(),
                veiled_count,
                unveiled_count,
                files: file_statuses,
            }
        }

        Commands::Veil {
            pattern,
            mode,
            dry_run,
            symbol,
            unreachable_from,
            level,
        } => {
            if let Some(sym_name) = symbol {
                let index = crate::load_index(&root)?;
                let entries = index
                    .symbols
                    .get(&sym_name)
                    .ok_or_else(|| anyhow::anyhow!("Symbol not found in index: {sym_name}"))?;
                if entries.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Symbol '{sym_name}' found in index but has no entries"
                    ));
                }
                let entry = &entries[0];
                let file_path = &entry.file;
                let range = LineRange::new(entry.line_start, entry.line_end)?;

                if dry_run {
                    let _ = writeln!(
                        output.out,
                        "Would veil symbol {sym_name} in: {file_path} (lines {}-{})",
                        entry.line_start, entry.line_end
                    );
                    return Ok(CommandResult::Veil {
                        files: vec![file_path.clone()],
                        dry_run: true,
                    });
                }

                let mut config = Config::load(&root)?;
                let tracker = HistoryTracker::begin(
                    &config,
                    "veil",
                    vec![format!("--symbol {sym_name}")],
                    std::slice::from_ref(file_path),
                    &root,
                    true,
                );
                veil_file(&root, &mut config, file_path, Some(&[range]), output)?;
                config.add_to_blacklist(file_path);
                save_and_update(&root, &config)?;
                tracker.commit(
                    &root,
                    &config,
                    format!("Veiled symbol {sym_name} in: {file_path}"),
                )?;
                let _ = writeln!(output.out, "Veiled symbol {sym_name} in: {file_path}");
                return Ok(CommandResult::Veil {
                    files: vec![file_path.clone()],
                    dry_run: false,
                });
            }

            if let Some(start_fn) = unreachable_from {
                let config_loaded = Config::load(&root)?;
                let graph = crate::build_call_graph_from_metadata(&root, &config_loaded)?;
                let trace = graph.trace(&start_fn, TraceDirection::Forward, 100);

                let reachable_files: std::collections::HashSet<String> = match &trace {
                    Some(result) => result
                        .all_functions()
                        .iter()
                        .filter_map(|f| f.file.as_ref())
                        .map(|p| p.to_string_lossy().to_string())
                        .collect(),
                    None => std::collections::HashSet::new(),
                };

                let index = crate::load_index(&root)?;
                let all_files: std::collections::HashSet<String> =
                    index.files.keys().cloned().collect();

                let unreachable: Vec<String> =
                    all_files.difference(&reachable_files).cloned().collect();

                if dry_run {
                    for f in &unreachable {
                        let _ = writeln!(output.out, "Would veil (unreachable): {f}");
                    }
                    let _ = writeln!(output.out, "{} files would be affected", unreachable.len());
                    return Ok(CommandResult::Veil {
                        files: unreachable,
                        dry_run: true,
                    });
                }

                let mut config = Config::load(&root)?;
                let mut veiled = Vec::new();
                for f in &unreachable {
                    match veil_file(&root, &mut config, f, None, output) {
                        Ok(()) => {
                            config.add_to_blacklist(f);
                            veiled.push(f.clone());
                        }
                        Err(e) => {
                            let _ = writeln!(output.err, "Warning: failed to veil {f}: {e}");
                        }
                    }
                }
                save_and_update(&root, &config)?;
                let _ = writeln!(output.out, "Veiled {} unreachable files", veiled.len());
                return Ok(CommandResult::Veil {
                    files: veiled,
                    dry_run: false,
                });
            }

            if let Some(lvl) = level {
                return handle_level_veil(&root, &pattern, lvl, output);
            }

            if dry_run {
                let affected = collect_affected_files_for_pattern(&root, &pattern);
                for f in &affected {
                    let path = root.join(f);
                    if path.exists() {
                        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        let _ = writeln!(output.out, "Would veil: {f} ({size} bytes)");
                    } else {
                        let _ = writeln!(output.out, "Would veil: {f}");
                    }
                }
                let _ = writeln!(output.out, "{} files would be affected", affected.len());
                return Ok(CommandResult::Veil {
                    files: affected,
                    dry_run: true,
                });
            }

            match mode {
                VeilMode::Full => {
                    let mut config = Config::load(&root)?;
                    let affected = collect_affected_files_for_pattern(&root, &pattern);
                    let tracker = HistoryTracker::begin(
                        &config,
                        "veil",
                        vec![pattern.clone()],
                        &affected,
                        &root,
                        true,
                    );

                    let mut veiled_files = Vec::new();
                    let mut veiled_any = false;
                    if pattern.contains('#') {
                        let (file, ranges) = parse_pattern(&pattern)?;
                        veil_file(&root, &mut config, file, ranges.as_deref(), output)?;
                        config.add_to_blacklist(file);
                        veiled_files.push(file.to_string());
                        veiled_any = true;
                    } else if pattern.starts_with('/')
                        && pattern.ends_with('/')
                        && pattern.len() > 2
                    {
                        use regex::Regex;
                        let regex_str = &pattern[1..pattern.len() - 1];
                        let regex = Regex::new(regex_str)?;

                        let mut file_errors = 0usize;
                        let mut matched = false;
                        let mut seen_files = std::collections::HashSet::new();
                        for entry in walk_files(&root)
                            .max_depth(None)
                            .build()
                            .filter_map(|e| e.ok())
                        {
                            let path = entry.path();
                            if path.is_file() {
                                let relative_path = path.strip_prefix(&root).unwrap_or(path);
                                let path_str = relative_path.to_string_lossy();
                                if regex.is_match(&path_str) {
                                    seen_files.insert(path_str.to_string());
                                    match veil_file(&root, &mut config, &path_str, None, output) {
                                        Ok(()) => {
                                            config.add_to_blacklist(&path_str);
                                            veiled_files.push(path_str.to_string());
                                            veiled_any = true;
                                        }
                                        Err(e) => {
                                            let _ = writeln!(
                                                output.err,
                                                "Warning: failed to veil {path_str}: {e}"
                                            );
                                            file_errors += 1;
                                        }
                                    }
                                    matched = true;
                                }
                            }
                        }

                        // Also check config for veiled files not on disk (already veiled)
                        let already_veiled: Vec<String> = config
                            .iter_unique_files()
                            .filter(|file| {
                                !seen_files.contains(file)
                                    && !root.join(file).exists()
                                    && regex.is_match(file)
                            })
                            .collect();
                        for file in already_veiled {
                            matched = true;
                            let _ = writeln!(
                                output.err,
                                "Warning: failed to veil {file}: already veiled (not on disk)"
                            );
                            file_errors += 1;
                        }

                        if !matched {
                            let _ = writeln!(output.out, "No files matched pattern: {pattern}");
                        } else if !veiled_any {
                            let _ = writeln!(
                                output.out,
                                "No files could be veiled for pattern: {pattern}"
                            );
                        }
                        if file_errors > 0 {
                            let _ = writeln!(
                                output.err,
                                "Warning: {file_errors} files could not be veiled."
                            );
                        }
                    } else {
                        veil_file(&root, &mut config, &pattern, None, output)?;
                        config.add_to_blacklist(&pattern);
                        veiled_files.push(pattern.clone());
                        veiled_any = true;
                    }

                    save_and_update(&root, &config)?;

                    if veiled_any {
                        let _ = writeln!(output.out, "Veiling: {pattern}");
                        let total_bytes: u64 = veiled_files
                            .iter()
                            .filter_map(|f| std::fs::metadata(root.join(f)).ok())
                            .map(|m| m.len())
                            .sum();
                        tracker.commit(
                            &root,
                            &config,
                            format!(
                                "Veiled {} file(s) ({total_bytes} bytes)",
                                veiled_files.len()
                            ),
                        )?;
                    }

                    CommandResult::Veil {
                        files: veiled_files,
                        dry_run: false,
                    }
                }
                VeilMode::Headers => {
                    use crate::VeilStrategy;
                    use std::fs;

                    let path = root.join(&pattern);
                    if !path.exists() {
                        return Err(anyhow::anyhow!("File not found: {pattern}"));
                    }
                    crate::validate_path_within_root(&path, &root)?;

                    let mut config = Config::load(&root)?;
                    let tracker = HistoryTracker::begin(
                        &config,
                        "veil",
                        vec![pattern.clone(), "--mode".to_string(), "headers".to_string()],
                        std::slice::from_ref(&pattern),
                        &root,
                        true,
                    );

                    let content = fs::read_to_string(&path)?;
                    let parser = TreeSitterParser::new()?;
                    let parsed = parser.parse_file(&path, &content)?;
                    let strategy = HeaderStrategy::new();
                    let veiled = strategy.veil_file(&content, &parsed)?;

                    let store = ContentStore::new(&root);
                    let hash = store.store(content.as_bytes())?;

                    let permissions = crate::perms::file_mode(&fs::metadata(&path)?);
                    fs::write(&path, veiled)?;

                    config.register_object(pattern.clone(), ObjectMeta::new(hash, permissions));
                    config.add_to_blacklist(&pattern);
                    save_and_update(&root, &config)?;
                    tracker.commit(&root, &config, format!("Veiled {pattern} (headers mode)"))?;

                    let _ = writeln!(output.out, "Veiled (headers mode): {pattern}");

                    CommandResult::Veil {
                        files: vec![pattern],
                        dry_run: false,
                    }
                }
            }
        }

        Commands::Parse { file, format } => {
            use crate::TreeSitterParser;
            use std::fs;

            let path = root.join(&file);
            if !path.exists() {
                return Err(anyhow::anyhow!("File not found: {file}"));
            }

            let content = fs::read_to_string(&path)?;
            let parser = TreeSitterParser::new()?;
            let parsed = parser.parse_file(&path, &content)?;

            match format {
                ParseFormat::Summary => {
                    let _ = writeln!(output.out, "File: {}", path.display());
                    let _ = writeln!(output.out, "Language: {}", parsed.language);
                    let _ = writeln!(output.out, "Functions: {}", parsed.functions().count());
                    let _ = writeln!(output.out, "Classes: {}", parsed.classes().count());
                    let _ = writeln!(output.out, "Imports: {}", parsed.imports.len());
                    let _ = writeln!(output.out, "Calls: {}", parsed.calls.len());
                }
                ParseFormat::Detailed => {
                    let _ = writeln!(output.out, "File: {}", path.display());
                    let _ = writeln!(output.out, "Language: {}\n", parsed.language);

                    if !parsed.symbols.is_empty() {
                        let _ = writeln!(output.out, "Symbols:");
                        for symbol in &parsed.symbols {
                            let _ = writeln!(
                                output.out,
                                "  - {} (lines {}-{})",
                                symbol.name(),
                                symbol.line_range().start(),
                                symbol.line_range().end()
                            );
                            if let crate::parser::Symbol::Function { .. } = symbol {
                                let _ =
                                    writeln!(output.out, "    Signature: {}", symbol.signature());
                            }
                        }
                    }

                    if !parsed.imports.is_empty() {
                        let _ = writeln!(output.out, "\nImports:");
                        for import in &parsed.imports {
                            let _ = writeln!(output.out, "  - {}", import.path);
                        }
                    }

                    if !parsed.calls.is_empty() {
                        let _ = writeln!(output.out, "\nCalls:");
                        for call in &parsed.calls {
                            if let Some(ref caller) = call.caller {
                                let _ = writeln!(
                                    output.out,
                                    "  - {} -> {} (line {})",
                                    caller, call.callee, call.line
                                );
                            } else {
                                let _ = writeln!(
                                    output.out,
                                    "  - {} (line {})",
                                    call.callee, call.line
                                );
                            }
                        }
                    }
                }
            }

            CommandResult::Other {
                message: format!("Parsed {file}"),
            }
        }

        Commands::Trace {
            function,
            from,
            to,
            from_entrypoint,
            depth,
            format,
            no_std,
        } => {
            let mut parsed_files = Vec::new();
            let parser = TreeSitterParser::new()?;

            for entry in walk_files(&root)
                .build()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
            {
                let path = entry.path();

                if is_supported_source(path) {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Ok(parsed) = parser.parse_file(path, &content) {
                            parsed_files.push(parsed);
                        }
                    }
                }
            }

            let mut graph = CallGraphBuilder::from_files(&parsed_files);

            if from_entrypoint {
                let entrypoints = EntrypointDetector::detect_all(&parsed_files);

                if entrypoints.is_empty() {
                    let _ = writeln!(output.err, "No entrypoints detected in the codebase");
                    return Ok(CommandResult::Other {
                        message: "No entrypoints detected".to_string(),
                    });
                }

                let _ = writeln!(
                    output.err,
                    "Tracing from {} detected entrypoints (max depth: {})...",
                    entrypoints.len(),
                    depth
                );

                let mut all_functions = std::collections::HashSet::new();

                for ep in &entrypoints {
                    if let Some(result) = graph.trace(&ep.name, TraceDirection::Forward, depth) {
                        for func in result.all_functions() {
                            all_functions.insert(func.name.clone());
                        }
                    }
                }

                let _ = writeln!(output.out, "\nEntrypoints found: {}", entrypoints.len());
                let _ = writeln!(
                    output.out,
                    "Functions reachable from entrypoints: {}",
                    all_functions.len()
                );
                let _ = writeln!(output.out, "\nReachable functions:");
                for func in &all_functions {
                    let _ = writeln!(output.out, "  - {func}");
                }
            } else {
                let (target, direction) = match (from.clone(), to.clone(), function) {
                    (Some(f), None, _) | (None, None, Some(f)) => (f, TraceDirection::Forward),
                    (None, Some(t), _) => (t, TraceDirection::Backward),
                    (Some(_), Some(_), _) => {
                        return Err(anyhow::anyhow!(
                            "Cannot use both --from and --to at the same time"
                        ));
                    }
                    (None, None, None) => {
                        return Err(anyhow::anyhow!(
                            "Must specify a function name or use --from/--to option"
                        ));
                    }
                };

                let _ = writeln!(
                    output.err,
                    "Tracing {direction} from '{target}' (max depth: {depth})..."
                );

                if !graph.contains(&target) {
                    let _ = writeln!(
                        output.err,
                        "Warning: Function '{target}' not found in call graph"
                    );
                    let _ = writeln!(
                        output.err,
                        "Available functions: {}",
                        graph.function_count()
                    );
                }

                match format {
                    TraceFormat::Dot => {
                        if no_std {
                            graph.filter_std_functions();
                        }
                        let _ = writeln!(output.out, "{}", graph.to_dot());
                    }
                    TraceFormat::Tree | TraceFormat::List => {
                        if let Some(mut result) = graph.trace(&target, direction, depth) {
                            if no_std {
                                result.filter_std();
                            }

                            let trace_output = match format {
                                TraceFormat::Tree => result.format_tree(),
                                TraceFormat::List => result.format_list(),
                                _ => unreachable!(),
                            };
                            let _ = writeln!(output.out, "{trace_output}");

                            if result.cycle_detected {
                                let _ =
                                    writeln!(output.err, "\nNote: Cycle detected in call graph");
                            }
                        } else {
                            let _ = writeln!(
                                output.err,
                                "Function '{target}' not found in the codebase"
                            );
                        }
                    }
                }
            }

            CommandResult::Other {
                message: "Trace complete".to_string(),
            }
        }

        Commands::Entrypoints {
            entry_type,
            language,
        } => {
            let mut parsed_files = Vec::new();
            let parser = TreeSitterParser::new()?;

            for entry in walk_files(&root)
                .build()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
            {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());

                let should_parse = matches!(
                    (language.as_ref(), ext),
                    (Some(LanguageArg::Rust), Some("rs"))
                        | (Some(LanguageArg::Go), Some("go"))
                        | (Some(LanguageArg::TypeScript), Some("ts") | Some("tsx"))
                        | (Some(LanguageArg::Python), Some("py"))
                        | (Some(LanguageArg::Bash), Some("sh") | Some("bash"))
                        | (
                            Some(LanguageArg::Terraform),
                            Some("tf") | Some("tfvars") | Some("hcl")
                        )
                        | (Some(LanguageArg::Helm), Some("yaml") | Some("yml"))
                        | (None, _)
                ) && (language.is_some() || is_supported_source(path));

                if should_parse {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Ok(parsed) = parser.parse_file(path, &content) {
                            parsed_files.push(parsed);
                        }
                    }
                }
            }

            let entrypoints = EntrypointDetector::detect_all(&parsed_files);

            if entrypoints.is_empty() {
                let _ = writeln!(output.out, "No entrypoints detected");
                return Ok(CommandResult::Other {
                    message: "No entrypoints detected".to_string(),
                });
            }

            let filtered: Vec<_> = entrypoints
                .iter()
                .filter(|ep| {
                    if let Some(ref filter_type) = entry_type {
                        match filter_type {
                            EntrypointTypeArg::Main => ep.entry_type == crate::EntrypointType::Main,
                            EntrypointTypeArg::Test => ep.entry_type == crate::EntrypointType::Test,
                            EntrypointTypeArg::Cli => ep.entry_type == crate::EntrypointType::Cli,
                            EntrypointTypeArg::Handler => {
                                ep.entry_type == crate::EntrypointType::Handler
                            }
                            EntrypointTypeArg::Export => {
                                ep.entry_type == crate::EntrypointType::Export
                            }
                        }
                    } else {
                        true
                    }
                })
                .collect();

            let grouped = EntrypointDetector::group_refs_by_language(&filtered);

            for (lang, eps) in grouped {
                let _ = writeln!(output.out, "\n[{lang}]");
                for ep in eps {
                    let desc = ep
                        .description
                        .as_ref()
                        .map(|d| format!(" - {d}"))
                        .unwrap_or_default();
                    let _ = writeln!(
                        output.out,
                        "  {} ({}){} - {}:{}",
                        ep.name,
                        ep.entry_type,
                        desc,
                        ep.file.display(),
                        ep.line
                    );
                }
            }

            let _ = writeln!(output.out, "\nTotal: {} entrypoints", filtered.len());

            CommandResult::Other {
                message: format!("{} entrypoints found", filtered.len()),
            }
        }

        Commands::Cache { cmd } => {
            use crate::AnalysisCache;

            match cmd {
                CacheCmd::Status => {
                    let cache = AnalysisCache::load(&root)?;
                    let stats = cache.stats();
                    let _ = writeln!(output.out, "{stats}");
                }
                CacheCmd::Clear => {
                    let mut cache = AnalysisCache::load(&root)?;
                    cache.clear();
                    cache.save(&root)?;
                    let _ = writeln!(output.out, "Cache cleared");
                }
                CacheCmd::Invalidate => {
                    let mut cache = AnalysisCache::load(&root)?;
                    cache.invalidate_stale();
                    cache.save(&root)?;
                    let _ = writeln!(output.out, "Stale cache entries invalidated");
                }
            }

            CommandResult::Other {
                message: "Cache operation complete".to_string(),
            }
        }

        Commands::Unveil {
            pattern,
            all,
            dry_run,
            symbol,
            callers_of,
            callees_of,
            level: _,
        } => {
            if let Some(sym_name) = symbol {
                let index = crate::load_index(&root)?;
                let entries = index
                    .symbols
                    .get(&sym_name)
                    .ok_or_else(|| anyhow::anyhow!("Symbol not found in index: {sym_name}"))?;
                if entries.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Symbol '{sym_name}' found in index but has no entries"
                    ));
                }
                let entry = &entries[0];
                let file_path = entry.file.clone();

                if dry_run {
                    let _ = writeln!(output.out, "Would unveil (symbol {sym_name}): {file_path}");
                    return Ok(CommandResult::Unveil {
                        files: vec![file_path],
                        dry_run: true,
                    });
                }

                let mut config = Config::load(&root)?;
                let tracker = HistoryTracker::begin(
                    &config,
                    "unveil",
                    vec![format!("--symbol {sym_name}")],
                    std::slice::from_ref(&file_path),
                    &root,
                    true,
                );
                unveil_file(&root, &mut config, &file_path, None, output)?;
                config.add_to_whitelist(&file_path);
                save_and_update(&root, &config)?;
                tracker.commit(
                    &root,
                    &config,
                    format!("Unveiled symbol {sym_name}: {file_path}"),
                )?;
                let _ = writeln!(output.out, "Unveiled (symbol {sym_name}): {file_path}");
                return Ok(CommandResult::Unveil {
                    files: vec![file_path],
                    dry_run: false,
                });
            }

            let trace_query = callers_of
                .as_ref()
                .map(|n| (n, TraceDirection::Backward, "caller"))
                .or_else(|| {
                    callees_of
                        .as_ref()
                        .map(|n| (n, TraceDirection::Forward, "callee"))
                });

            if let Some((fn_name, direction, label)) = trace_query {
                let config_loaded = Config::load(&root)?;
                let graph = crate::build_call_graph_from_metadata(&root, &config_loaded)?;
                let trace = graph.trace(fn_name, direction, 10);

                let files: Vec<String> = match &trace {
                    Some(result) => {
                        let mut seen = std::collections::HashSet::new();
                        result
                            .all_functions()
                            .iter()
                            .filter_map(|f| f.file.as_ref())
                            .filter(|p| seen.insert(p.to_string_lossy().to_string()))
                            .map(|p| p.to_string_lossy().to_string())
                            .collect()
                    }
                    None => {
                        let _ = writeln!(output.out, "No {label}s found for: {fn_name}");
                        return Ok(CommandResult::Unveil {
                            files: vec![],
                            dry_run: false,
                        });
                    }
                };

                if dry_run {
                    for f in &files {
                        let _ = writeln!(output.out, "Would unveil ({label} of {fn_name}): {f}");
                    }
                    let _ = writeln!(output.out, "{} files would be affected", files.len());
                    return Ok(CommandResult::Unveil {
                        files,
                        dry_run: true,
                    });
                }

                let mut config = Config::load(&root)?;
                let mut unveiled = Vec::new();
                for f in &files {
                    match unveil_file(&root, &mut config, f, None, output) {
                        Ok(()) => {
                            config.add_to_whitelist(f);
                            unveiled.push(f.clone());
                        }
                        Err(e) => {
                            let _ = writeln!(output.err, "Warning: failed to unveil {f}: {e}");
                        }
                    }
                }
                save_and_update(&root, &config)?;
                let _ = writeln!(output.out, "Unveiled {} {label} files", unveiled.len());
                return Ok(CommandResult::Unveil {
                    files: unveiled,
                    dry_run: false,
                });
            }

            let mut config = Config::load(&root)?;

            if dry_run {
                if all {
                    let files: Vec<String> =
                        config.iter_unique_files().map(|f| f.to_string()).collect();
                    for f in &files {
                        let _ = writeln!(output.out, "Would unveil: {f}");
                    }
                    let _ = writeln!(output.out, "{} files would be affected", files.len());
                    return Ok(CommandResult::Unveil {
                        files,
                        dry_run: true,
                    });
                } else if let Some(ref pattern) = pattern {
                    let affected = collect_affected_files_for_pattern(&root, pattern);
                    for f in &affected {
                        let _ = writeln!(output.out, "Would unveil: {f}");
                    }
                    let _ = writeln!(output.out, "{} files would be affected", affected.len());
                    return Ok(CommandResult::Unveil {
                        files: affected,
                        dry_run: true,
                    });
                }
            }

            let mut unveiled_files = Vec::new();

            if all {
                let files_before: Vec<String> =
                    config.iter_unique_files().map(|f| f.to_string()).collect();
                let tracker = HistoryTracker::begin(
                    &config,
                    "unveil",
                    vec!["--all".to_string()],
                    &files_before,
                    &root,
                    true,
                );

                unveil_all(&root, &mut config, output)?;
                save_and_update(&root, &config)?;

                tracker.commit(
                    &root,
                    &config,
                    format!("Unveiled all files ({} files)", files_before.len()),
                )?;

                unveiled_files = files_before;
                let _ = writeln!(output.out, "Unveiled all files");
            } else if let Some(pattern) = pattern {
                let affected = collect_affected_files_for_pattern(&root, &pattern);
                let tracker = HistoryTracker::begin(
                    &config,
                    "unveil",
                    vec![pattern.clone()],
                    &affected,
                    &root,
                    true,
                );

                if pattern.contains('#') {
                    let (file, ranges) = parse_pattern(&pattern)?;
                    unveil_file(&root, &mut config, file, ranges.as_deref(), output)?;
                    config.add_to_whitelist(file);
                    save_and_update(&root, &config)?;
                    unveiled_files.push(file.to_string());
                    let _ = writeln!(output.out, "Unveiled: {pattern}");
                } else if pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() > 2 {
                    use regex::Regex;
                    let regex_str = &pattern[1..pattern.len() - 1];
                    let regex = Regex::new(regex_str)?;

                    let mut matched = false;
                    let mut unveiled_any = false;
                    let mut file_errors = 0usize;
                    let mut seen_files = std::collections::HashSet::new();
                    for entry in walk_files(&root)
                        .max_depth(None)
                        .build()
                        .filter_map(|e| e.ok())
                    {
                        let path = entry.path();
                        if path.is_file() {
                            let relative_path = path.strip_prefix(&root).unwrap_or(path);
                            let path_str = relative_path.to_string_lossy();
                            if regex.is_match(&path_str) {
                                seen_files.insert(path_str.to_string());
                                if has_veils(&config, &path_str) {
                                    match unveil_file(&root, &mut config, &path_str, None, output) {
                                        Ok(()) => {
                                            config.add_to_whitelist(&path_str);
                                            unveiled_files.push(path_str.to_string());
                                            unveiled_any = true;
                                        }
                                        Err(e) => {
                                            let _ = writeln!(
                                                output.err,
                                                "Warning: failed to unveil {path_str}: {e}"
                                            );
                                            file_errors += 1;
                                        }
                                    }
                                } else {
                                    config.add_to_whitelist(&path_str);
                                }
                                matched = true;
                            }
                        }
                    }

                    // Also check config for veiled files not on disk
                    let not_on_disk: Vec<String> = config
                        .iter_unique_files()
                        .filter(|file| {
                            !seen_files.contains(file)
                                && !root.join(file).exists()
                                && regex.is_match(file)
                        })
                        .collect();
                    for file in not_on_disk {
                        matched = true;
                        if has_veils(&config, &file) {
                            match unveil_file(&root, &mut config, &file, None, output) {
                                Ok(()) => {
                                    config.add_to_whitelist(&file);
                                    unveiled_files.push(file.to_string());
                                    unveiled_any = true;
                                }
                                Err(e) => {
                                    let _ = writeln!(
                                        output.err,
                                        "Warning: failed to unveil {file}: {e}"
                                    );
                                    file_errors += 1;
                                }
                            }
                        }
                    }

                    save_and_update(&root, &config)?;
                    if !matched {
                        let _ = writeln!(output.out, "No files matched pattern: {pattern}");
                    } else if unveiled_any {
                        let _ = writeln!(output.out, "Unveiled: {pattern}");
                    } else if matched && !unveiled_any {
                        let _ = writeln!(output.out, "No veiled files matched pattern: {pattern}");
                    }
                    if file_errors > 0 {
                        let _ = writeln!(
                            output.err,
                            "Warning: {file_errors} files could not be unveiled."
                        );
                    }
                } else {
                    if has_veils(&config, &pattern) {
                        unveil_file(&root, &mut config, &pattern, None, output)?;
                        unveiled_files.push(pattern.clone());
                    }
                    config.add_to_whitelist(&pattern);
                    save_and_update(&root, &config)?;
                    let _ = writeln!(output.out, "Unveiled: {pattern}");
                }

                tracker.commit(
                    &root,
                    &config,
                    format!("Unveiled {} file(s)", unveiled_files.len()),
                )?;
            } else {
                return Err(anyhow::anyhow!(
                    "Must specify a pattern or --all to unveil files."
                ));
            }

            CommandResult::Unveil {
                files: unveiled_files,
                dry_run: false,
            }
        }

        Commands::Apply { dry_run } => {
            let mut config = Config::load(&root)?;
            let store = ContentStore::new(&root);

            if dry_run {
                let mut would_apply = Vec::new();
                let entries: Vec<_> = config
                    .objects
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (key, meta) in &entries {
                    let parsed_key = crate::ConfigKey::parse(key);
                    let file_path = parsed_key.file();
                    let path = root.join(file_path);
                    if !path.exists() {
                        continue;
                    }
                    let current_content = std::fs::read(&path)?;
                    let current_hash = ContentHash::from_content(&current_content);
                    if current_hash.full() == meta.hash {
                        would_apply.push(file_path.to_string());
                        let _ = writeln!(output.out, "Would re-veil: {file_path}");
                    }
                }
                let _ = writeln!(
                    output.out,
                    "{} files would be re-applied",
                    would_apply.len()
                );
                return Ok(CommandResult::Apply {
                    applied: would_apply.len(),
                    skipped: 0,
                    dry_run: true,
                });
            }

            let pre_config = snapshot_config(&config);

            let _ = writeln!(output.out, "Re-applying veils...");

            let mut applied = 0;
            let mut skipped = 0;
            let mut applied_files = Vec::new();
            let mut pre_file_snapshots = Vec::new();

            let entries: Vec<_> = config
                .objects
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            for (key, meta) in &entries {
                let parsed_key = crate::ConfigKey::parse(key);
                let file_str = parsed_key.file();

                let path = root.join(file_str);

                // For full-veil entries: file should not be on disk
                if let crate::ConfigKey::FullVeil { .. } = &parsed_key {
                    if !path.exists() {
                        // Correct state: file is removed
                        let _ = writeln!(output.out, "  \u{2713} {file_str} (veiled, not on disk)");
                        continue;
                    }
                    // Check for legacy marker — migrate by deleting the file
                    if crate::is_legacy_marker(&path) {
                        let snap = snapshot_files(&root, &[file_str.to_string()]);
                        let _ = crate::perms::save_and_make_writable(&path);
                        std::fs::remove_file(&path)?;
                        applied += 1;
                        applied_files.push(file_str.to_string());
                        pre_file_snapshots.extend(snap);
                        let _ =
                            writeln!(output.out, "  \u{2713} {file_str} (migrated legacy marker)");
                        continue;
                    }
                }

                if !path.exists() {
                    let _ = writeln!(output.err, "  Skipping {file_str} (file not found)");
                    skipped += 1;
                    continue;
                }

                let current_content = std::fs::read(&path)?;
                let current_hash = ContentHash::from_content(&current_content);

                if current_hash.full() != meta.hash {
                    let _ = writeln!(output.out, "  \u{2713} {file_str} (already veiled)");
                } else {
                    let original_hash = match ContentHash::from_string(meta.hash.clone()) {
                        Ok(h) => h,
                        Err(e) => {
                            let _ =
                                writeln!(output.err, "  \u{2717} {file_str} (invalid hash: {e})");
                            config.objects.remove(key);
                            skipped += 1;
                            continue;
                        }
                    };
                    if store.exists(&original_hash) {
                        let snap = snapshot_files(&root, &[file_str.to_string()]);
                        let removed_meta = config.objects.remove(key);
                        if let Err(e) = veil_file(&root, &mut config, file_str, None, output) {
                            let _ =
                                writeln!(output.err, "  \u{2717} {file_str} (re-veil failed: {e})");
                            if let Some(meta) = removed_meta {
                                config.objects.insert(key.clone(), meta);
                            }
                            skipped += 1;
                        } else {
                            applied += 1;
                            applied_files.push(file_str.to_string());
                            pre_file_snapshots.extend(snap);
                            let _ = writeln!(output.out, "  \u{2713} {file_str} (re-veiled)");
                        }
                    } else {
                        let _ = writeln!(
                            output.err,
                            "  \u{2717} {file_str} (original content missing from CAS, skipping)"
                        );
                        skipped += 1;
                    }
                }
            }

            config.save(&root)?;

            // Rebuild metadata index and manifest
            let _ = crate::rebuild_index(&root, &config).and_then(|i| crate::save_index(&root, &i));
            let _ = crate::generate_manifest(&root, &config)
                .and_then(|m| crate::save_manifest(&root, &m));

            if applied > 0 {
                let post_config = snapshot_config(&config);
                let post_files = snapshot_files(&root, &applied_files);
                let mut history = ActionHistory::load(&root)?;
                history.push(ActionRecord {
                    id: history.next_id(),
                    timestamp: chrono::Utc::now(),
                    command: "apply".to_string(),
                    args: vec![],
                    summary: format!("Re-applied veils: {applied} applied, {skipped} skipped"),
                    affected_files: applied_files,
                    undoable: true,
                    pre_state: ActionState {
                        config_yaml: pre_config,
                        file_snapshots: pre_file_snapshots,
                    },
                    post_state: ActionState {
                        config_yaml: post_config,
                        file_snapshots: post_files,
                    },
                });
                history.save(&root)?;
            }

            let _ = writeln!(output.out, "\nApplied: {applied}, Skipped: {skipped}");

            CommandResult::Apply {
                applied,
                skipped,
                dry_run: false,
            }
        }

        Commands::Restore => match get_latest_checkpoint(&root)? {
            Some(name) => {
                let config = Config::load(&root)?;
                let checkpoint_dir = root.join(crate::config::CHECKPOINTS_DIR).join(&name);
                let manifest_path = checkpoint_dir.join("manifest.yaml");
                let affected: Vec<String> = if manifest_path.exists() {
                    let manifest_content = std::fs::read_to_string(&manifest_path)?;
                    let manifest: serde_yaml::Value = serde_yaml::from_str(&manifest_content)?;
                    manifest
                        .get("files")
                        .and_then(|f| f.as_mapping())
                        .map(|m| {
                            m.keys()
                                .filter_map(|k| k.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    vec![]
                };
                let pre_state = ActionState::capture(&root, &config, &affected);

                let _ = writeln!(output.out, "Restoring from latest checkpoint: {name}");
                restore_checkpoint(&root, &name, output)?;

                let post_config_loaded = Config::load(&root).ok();
                let post_state = ActionState {
                    config_yaml: post_config_loaded.as_ref().and_then(snapshot_config),
                    file_snapshots: snapshot_files(&root, &affected),
                };

                let mut history = ActionHistory::load(&root)?;
                history.push(ActionRecord {
                    id: history.next_id(),
                    timestamp: chrono::Utc::now(),
                    command: "restore".to_string(),
                    args: vec![name.clone()],
                    summary: format!("Restored from checkpoint '{name}'"),
                    affected_files: affected,
                    undoable: true,
                    pre_state,
                    post_state,
                });
                history.save(&root)?;

                CommandResult::Restore { checkpoint: name }
            }
            None => {
                return Err(anyhow::anyhow!(
                    "No checkpoints found. Use 'fv checkpoint save <name>' to create one."
                ));
            }
        },

        Commands::Show { file } => {
            let config = Config::load(&root)?;
            let file_path = root.join(&file);

            let is_full_veiled = config.get_object(&file).is_some();

            if !file_path.exists() {
                if is_full_veiled {
                    // File is veiled and removed from disk — retrieve from CAS
                    let store = ContentStore::new(&root);
                    let meta = config.get_object(&file).unwrap();
                    let hash = ContentHash::from_string(meta.hash.clone())?;
                    let content = store.retrieve(&hash)?;
                    let content_str = String::from_utf8_lossy(&content);
                    let _ = writeln!(output.out, "File: {file} [VEILED - not on disk]");
                    for (i, line) in content_str.lines().enumerate() {
                        let _ = writeln!(output.out, "{:4} | {}", i + 1, line);
                    }
                    return Ok(CommandResult::Other {
                        message: format!("Showed {file} (veiled, from CAS)"),
                    });
                }
                return Err(anyhow::anyhow!("file not found: {file}"));
            }
            crate::validate_path_within_root(&file_path, &root)?;

            let partial_ranges = config.veiled_ranges(&file)?;

            if is_full_veiled {
                let _ = writeln!(output.out, "File: {file} [FULLY VEILED]");
                let _ = writeln!(
                    output.out,
                    "Content is veiled. Use 'fv unveil {file}' to view."
                );
            } else if !partial_ranges.is_empty() {
                let content = std::fs::read_to_string(&file_path)?;
                let lines: Vec<&str> = content.lines().collect();

                let _ = writeln!(output.out, "File: {file}");
                let marker_re = regex::Regex::new(r"^\.\.\.\[[a-f0-9]{7}\]").unwrap();
                for (i, line) in lines.iter().enumerate() {
                    let line_num = i + 1;
                    let mut is_veiled = false;
                    if let Ok(veiled) = config.is_veiled(&file, line_num) {
                        is_veiled = veiled;
                    }

                    if marker_re.is_match(line) {
                        let _ = writeln!(output.out, "{line_num:4} | [veiled] {line}");
                    } else if is_veiled {
                        let _ = writeln!(output.out, "{line_num:4} | [veiled] ...");
                    } else {
                        let _ = writeln!(output.out, "{line_num:4} | {line}");
                    }
                }
            } else {
                let content = std::fs::read_to_string(&file_path)?;
                let _ = writeln!(output.out, "File: {file}");
                for (i, line) in content.lines().enumerate() {
                    let _ = writeln!(output.out, "{:4} | {}", i + 1, line);
                }
            }

            CommandResult::Other {
                message: format!("Showed {file}"),
            }
        }

        Commands::Checkpoint { cmd } => {
            let cmd_action;
            let cmd_name;
            match cmd {
                CheckpointCmd::Save { ref name } => {
                    let config = Config::load(&root)?;
                    let tracker = HistoryTracker::begin(
                        &config,
                        "checkpoint",
                        vec!["save".to_string(), name.clone()],
                        &[],
                        &root,
                        true,
                    );

                    save_checkpoint(&root, &config, name, output)?;

                    tracker.commit(&root, &config, format!("Saved checkpoint '{name}'"))?;

                    cmd_action = "save".to_string();
                    cmd_name = name.clone();
                }
                CheckpointCmd::Restore { ref name } => {
                    let config = Config::load(&root)?;
                    let checkpoint_dir = root.join(crate::config::CHECKPOINTS_DIR).join(name);
                    let manifest_path = checkpoint_dir.join("manifest.yaml");
                    let affected: Vec<String> = if manifest_path.exists() {
                        let mc = std::fs::read_to_string(&manifest_path)?;
                        let mv: serde_yaml::Value = serde_yaml::from_str(&mc)?;
                        mv.get("files")
                            .and_then(|f| f.as_mapping())
                            .map(|m| {
                                m.keys()
                                    .filter_map(|k| k.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default()
                    } else {
                        vec![]
                    };
                    let pre_state = ActionState::capture(&root, &config, &affected);

                    restore_checkpoint(&root, name, output)?;

                    let post_cfg = Config::load(&root).ok();
                    let post_state = ActionState {
                        config_yaml: post_cfg.as_ref().and_then(snapshot_config),
                        file_snapshots: snapshot_files(&root, &affected),
                    };

                    let mut history = ActionHistory::load(&root)?;
                    history.push(ActionRecord {
                        id: history.next_id(),
                        timestamp: chrono::Utc::now(),
                        command: "checkpoint".to_string(),
                        args: vec!["restore".to_string(), name.clone()],
                        summary: format!("Restored checkpoint '{name}'"),
                        affected_files: affected,
                        undoable: true,
                        pre_state,
                        post_state,
                    });
                    history.save(&root)?;

                    cmd_action = "restore".to_string();
                    cmd_name = name.clone();
                }
                CheckpointCmd::List => {
                    let checkpoints = list_checkpoints(&root)?;
                    if checkpoints.is_empty() {
                        let _ = writeln!(output.out, "No checkpoints found.");
                    } else {
                        let _ = writeln!(output.out, "Checkpoints:");
                        for cp in checkpoints {
                            let _ = writeln!(output.out, "  - {cp}");
                        }
                    }
                    cmd_action = "list".to_string();
                    cmd_name = String::new();
                }
                CheckpointCmd::Show { ref name } => {
                    show_checkpoint(&root, name, output)?;
                    cmd_action = "show".to_string();
                    cmd_name = name.clone();
                }
                CheckpointCmd::Delete { ref name } => {
                    let config = Config::load(&root)?;
                    let tracker = HistoryTracker::begin(
                        &config,
                        "checkpoint",
                        vec!["delete".to_string(), name.clone()],
                        &[],
                        &root,
                        true,
                    );

                    delete_checkpoint(&root, name, output)?;

                    tracker.commit(&root, &config, format!("Deleted checkpoint '{name}'"))?;

                    cmd_action = "delete".to_string();
                    cmd_name = name.clone();
                }
            }

            CommandResult::Checkpoint {
                action: cmd_action,
                name: cmd_name,
            }
        }

        Commands::Doctor => {
            let _ = writeln!(output.out, "Running integrity checks...");

            let config = Config::load(&root)?;
            let store = ContentStore::new(&root);
            let metadata_store = crate::MetadataStore::new(&root);

            let report = check_integrity(
                &config,
                |hash| store.retrieve(hash).is_ok(),
                |file| root.join(file).exists(),
                |file| crate::is_legacy_marker(&root.join(file)),
                |hash| metadata_store.exists(hash),
            );

            if report.issues.is_empty() {
                let _ = writeln!(output.out, "\u{2713} All checks passed. No issues found.");
            } else {
                let _ = writeln!(
                    output.out,
                    "\u{2717} Found {} issue(s):",
                    report.issues.len()
                );
                for issue in &report.issues {
                    let _ = writeln!(output.out, "  - {issue}");
                }
            }

            CommandResult::Doctor {
                issues: report.issues,
            }
        }

        Commands::Gc => {
            let config = Config::load(&root)?;

            let _ = writeln!(output.out, "Running garbage collection...");

            let mut referenced: Vec<ContentHash> = Vec::new();
            for (key, meta) in &config.objects {
                match ContentHash::from_string(meta.hash.clone()) {
                    Ok(h) => referenced.push(h),
                    Err(e) => {
                        let _ =
                            writeln!(output.err, "Warning: skipping invalid hash for {key}: {e}");
                    }
                }
            }

            let tracker = HistoryTracker::begin(&config, "gc", vec![], &[], &root, false);

            let (deleted, freed) = garbage_collect(&root, &referenced, output)?;

            tracker.commit(
                &root,
                &config,
                format!("Garbage collected {deleted} object(s), freed {freed} bytes"),
            )?;

            let _ = writeln!(output.out, "Garbage collected {deleted} object(s)");
            let _ = writeln!(output.out, "Freed {freed} bytes");

            CommandResult::Gc {
                deleted,
                freed_bytes: freed,
            }
        }

        Commands::Clean => {
            let _ = writeln!(output.out, "Removing all funveil data...");

            // Clean is not undoable (all data destroyed)
            let data_dir = root.join(".funveil");
            let config_file = root.join(CONFIG_FILE);

            if data_dir.exists() {
                std::fs::remove_dir_all(&data_dir)?;
            }

            if config_file.exists() {
                std::fs::remove_file(&config_file)?;
            }

            let _ = writeln!(output.out, "\u{2713} Removed all funveil data");

            CommandResult::Clean { success: true }
        }

        Commands::Version => {
            let _ = writeln!(output.out, "{}", version_long());
            CommandResult::VersionResult {
                version: env!("FV_VERSION").to_string(),
            }
        }

        Commands::Undo { force } => {
            let mut history = ActionHistory::load(&root)?;
            let entry = history.undo()?;
            let entry_clone = entry.clone();

            if !entry_clone.undoable && !force {
                // Move cursor back (undo already moved it)
                history.cursor += 1;
                let _ = writeln!(
                    output.err,
                    "Action #{} ({}) is not undoable. Use --force to override.",
                    entry_clone.id, entry_clone.command
                );
                return Err(crate::FunveilError::ActionNotUndoable(entry_clone.id).into());
            }

            // Restore pre_state
            restore_action_state(&root, &entry_clone.pre_state)?;
            history.save(&root)?;

            let _ = writeln!(
                output.out,
                "Undone: #{} {} - {}",
                entry_clone.id, entry_clone.command, entry_clone.summary
            );

            CommandResult::Undo {
                undone: ActionSummary::from_record(&entry_clone),
            }
        }

        Commands::Redo => {
            let mut history = ActionHistory::load(&root)?;
            let entry = history.redo()?;
            let entry_clone = entry.clone();

            // Restore post_state
            restore_action_state(&root, &entry_clone.post_state)?;
            history.save(&root)?;

            let _ = writeln!(
                output.out,
                "Redone: #{} {} - {}",
                entry_clone.id, entry_clone.command, entry_clone.summary
            );

            CommandResult::Redo {
                redone: ActionSummary::from_record(&entry_clone),
            }
        }

        Commands::History { limit, show } => {
            let history = ActionHistory::load(&root)?;

            if let Some(id) = show {
                // Show detailed view of a specific action
                let record = history
                    .get(id)
                    .ok_or_else(|| anyhow::anyhow!("Action #{id} not found in history"))?;

                let _ = writeln!(
                    output.out,
                    "Action #{}: {} {}",
                    record.id,
                    record.command,
                    record.args.join(" ")
                );
                let _ = writeln!(
                    output.out,
                    "Timestamp: {}",
                    record.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
                );
                let _ = writeln!(
                    output.out,
                    "Undoable: {}",
                    if record.undoable { "yes" } else { "no" }
                );
                let _ = writeln!(output.out, "Summary: {}", record.summary);

                // Compute config diff
                let mut config_diff = Vec::new();
                match (
                    &record.pre_state.config_yaml,
                    &record.post_state.config_yaml,
                ) {
                    (Some(pre), Some(post)) if pre != post => {
                        let pre_val: serde_yaml::Value =
                            serde_yaml::from_str(pre).unwrap_or_default();
                        let post_val: serde_yaml::Value =
                            serde_yaml::from_str(post).unwrap_or_default();

                        // Diff objects maps
                        if let (Some(pre_obj), Some(post_obj)) = (
                            pre_val.get("objects").and_then(|o| o.as_mapping()),
                            post_val.get("objects").and_then(|o| o.as_mapping()),
                        ) {
                            for (k, v) in post_obj {
                                if !pre_obj.contains_key(k) {
                                    let key_str = k.as_str().unwrap_or("?");
                                    config_diff.push(format!("+ objects[\"{key_str}\"]: {v:?}"));
                                }
                            }
                            for (k, _) in pre_obj {
                                if !post_obj.contains_key(k) {
                                    let key_str = k.as_str().unwrap_or("?");
                                    config_diff.push(format!("- objects[\"{key_str}\"]"));
                                }
                            }
                        }

                        // Diff mode
                        let pre_mode = pre_val.get("mode").and_then(|m| m.as_str());
                        let post_mode = post_val.get("mode").and_then(|m| m.as_str());
                        if pre_mode != post_mode {
                            config_diff.push(format!(
                                "mode: {:?} -> {:?}",
                                pre_mode.unwrap_or("?"),
                                post_mode.unwrap_or("?")
                            ));
                        }
                    }
                    (None, Some(_)) => {
                        config_diff.push("+ config created".to_string());
                    }
                    (Some(_), None) => {
                        config_diff.push("- config removed".to_string());
                    }
                    _ => {}
                }

                if !config_diff.is_empty() {
                    let _ = writeln!(output.out, "\nConfig changes:");
                    for diff in &config_diff {
                        let _ = writeln!(output.out, "  {diff}");
                    }
                }

                // File diffs (size changes)
                let mut file_diffs = Vec::new();
                let store = ContentStore::new(&root);
                for (pre_snap, post_snap) in record
                    .pre_state
                    .file_snapshots
                    .iter()
                    .zip(record.post_state.file_snapshots.iter())
                {
                    let pre_size = pre_snap
                        .cas_hash
                        .as_ref()
                        .and_then(|h| ContentHash::from_string(h.clone()).ok())
                        .and_then(|h| store.retrieve(&h).ok())
                        .map(|c| c.len())
                        .unwrap_or(0);
                    let post_size = post_snap
                        .cas_hash
                        .as_ref()
                        .and_then(|h| ContentHash::from_string(h.clone()).ok())
                        .and_then(|h| store.retrieve(&h).ok())
                        .map(|c| c.len())
                        .unwrap_or(0);

                    let _ = writeln!(
                        output.out,
                        "\n  {}: {} bytes -> {} bytes",
                        pre_snap.path, pre_size, post_size
                    );
                    file_diffs.push(FileDiff {
                        path: pre_snap.path.clone(),
                        before: format!("{pre_size} bytes"),
                        after: format!("{post_size} bytes"),
                    });
                }

                CommandResult::HistoryShow {
                    action: ActionSummary::from_record(record),
                    config_diff,
                    file_diffs,
                }
            } else {
                // List history
                let past = history.past();
                let future = history.future();

                let _ = writeln!(output.out, "Past (most recent first):");
                let past_to_show: Vec<_> = past.iter().rev().take(limit).collect();
                for entry in &past_to_show {
                    let files_str = if entry.affected_files.is_empty() {
                        "-".to_string()
                    } else {
                        entry.affected_files.join(", ")
                    };
                    let _ = writeln!(
                        output.out,
                        "  #{:<3} {}  {:<12} {:<20} \"{}\"",
                        entry.id,
                        entry.timestamp.format("%Y-%m-%d %H:%M"),
                        entry.command,
                        files_str,
                        entry.summary
                    );
                }

                if !future.is_empty() {
                    let _ = writeln!(output.out, "\u{2500}\u{2500}\u{2500}\u{2500} current state \u{2500}\u{2500}\u{2500}\u{2500}");
                    let _ = writeln!(output.out, "Future:");
                    for entry in future.iter().take(limit) {
                        let files_str = if entry.affected_files.is_empty() {
                            "-".to_string()
                        } else {
                            entry.affected_files.join(", ")
                        };
                        let _ = writeln!(
                            output.out,
                            "  #{:<3} {}  {:<12} {:<20} \"{}\"",
                            entry.id,
                            entry.timestamp.format("%Y-%m-%d %H:%M"),
                            entry.command,
                            files_str,
                            entry.summary
                        );
                    }
                }

                let cursor_id = history.past().last().map(|e| e.id);

                CommandResult::History {
                    past: past_to_show
                        .iter()
                        .map(|e| ActionSummary::from_record(e))
                        .collect(),
                    future: future
                        .iter()
                        .take(limit)
                        .map(ActionSummary::from_record)
                        .collect(),
                    cursor_id,
                }
            }
        }
        Commands::Disclose { budget, focus } => {
            let config = Config::load(&root)?;
            let index = crate::load_index(&root)?;
            let graph = crate::build_call_graph_from_metadata(&root, &config).ok();

            let plan = crate::compute_disclosure_plan(
                &root,
                &config,
                budget,
                &focus,
                graph.as_ref(),
                Some(&index),
            )?;

            let _ = writeln!(
                output.out,
                "Disclosure plan (budget: {} tokens):",
                plan.budget
            );
            for entry in &plan.entries {
                let _ = writeln!(
                    output.out,
                    "  Level {}: {} (~{} tokens)",
                    entry.level, entry.file, entry.estimated_tokens
                );
            }
            let _ = writeln!(
                output.out,
                "Total: {}/{} tokens used",
                plan.used_tokens, plan.budget
            );

            CommandResult::Disclose {
                budget: plan.budget,
                used_tokens: plan.used_tokens,
                entries: plan.entries,
            }
        }

        Commands::Context { function, depth } => {
            let mut config = Config::load(&root)?;
            let index = crate::load_index(&root)?;

            let entries = index
                .symbols
                .get(&function)
                .ok_or_else(|| anyhow::anyhow!("Symbol not found in index: {function}"))?;

            let graph = crate::build_call_graph_from_metadata(&root, &config)?;

            let mut unveiled_files = Vec::new();

            if let Some(trace) = graph.trace(&function, TraceDirection::Forward, depth) {
                for func_node in trace.all_functions() {
                    if let Some(ref file) = func_node.file {
                        let file_str = file.to_string_lossy().to_string();
                        if config.get_object(&file_str).is_some() || has_veils(&config, &file_str) {
                            if let Ok(()) = unveil_file(&root, &mut config, &file_str, None, output)
                            {
                                unveiled_files.push(file_str);
                            }
                        }
                    }
                }
            }

            for entry in entries {
                if config.get_object(&entry.file).is_some() || has_veils(&config, &entry.file) {
                    if let Ok(()) = unveil_file(&root, &mut config, &entry.file, None, output) {
                        if !unveiled_files.contains(&entry.file) {
                            unveiled_files.push(entry.file.clone());
                        }
                    }
                }
            }

            if !unveiled_files.is_empty() {
                save_and_update(&root, &config)?;
            }

            let _ = writeln!(
                output.out,
                "Context for {function} (depth {depth}): unveiled {} files",
                unveiled_files.len()
            );

            CommandResult::Context {
                function,
                unveiled_files,
            }
        }
    };

    Ok(cmd_result)
}
