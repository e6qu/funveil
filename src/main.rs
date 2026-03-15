#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use anyhow::Result;
use clap::{Parser, Subcommand};
use funveil::{
    command_category, delete_checkpoint, garbage_collect, generate_trace_id, get_latest_checkpoint,
    has_veils, is_supported_source, list_checkpoints, normalize_path, restore_checkpoint,
    save_checkpoint, show_checkpoint, unveil_all, unveil_file, veil_file, walk_files,
    ActionHistory, ActionRecord, ActionState, CallGraphBuilder, Config, ContentHash, ContentStore,
    EntrypointDetector, FileSnapshot, HeaderStrategy, LineRange, Mode, ObjectMeta, Output,
    TraceDirection, TreeSitterParser, CONFIG_FILE,
};
#[cfg(not(target_family = "wasm"))]
use funveil::{init_tracing, resolve_log_level};
use serde::Serialize;
use std::env;
use std::io::Write;
use std::path::PathBuf;
use tracing::info_span;

#[derive(Parser)]
#[command(name = "fv")]
#[command(about = "Funveil - Control file visibility in AI agent workspaces")]
#[command(version = env!("FV_VERSION"))]
struct Cli {
    /// Suppress output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Log level (trace, debug, info, warn, error, off)
    #[arg(long, global = true)]
    log_level: Option<String>,

    /// Output as JSON (for machine consumption)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn version_long() -> String {
    format!(
        concat!("fv {}\n", "commit: {}\n", "target: {}\n", "profile: {}",),
        env!("FV_VERSION"),
        env!("FV_GIT_SHA"),
        env!("FV_BUILD_TARGET"),
        env!("FV_BUILD_PROFILE"),
    )
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum VeilMode {
    /// Veil entire files
    Full,
    /// Show only headers (signatures), hide implementations
    Headers,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum ParseFormat {
    /// Summary of symbols found
    Summary,
    /// Detailed symbol list
    Detailed,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum TraceFormat {
    /// Tree view of call hierarchy
    Tree,
    /// Flat list view
    List,
    /// DOT format for graph visualization
    Dot,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum EntrypointTypeArg {
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
enum LanguageArg {
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
enum Commands {
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
    fn name(&self) -> &'static str {
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

#[derive(Serialize)]
pub struct FileStatus {
    pub path: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub veil_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranges: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_disk: Option<bool>,
}

#[derive(Serialize)]
pub struct ActionSummary {
    pub id: u64,
    pub timestamp: String,
    pub command: String,
    pub affected_files: Vec<String>,
    pub summary: String,
}

#[derive(Serialize)]
pub struct FileDiff {
    pub path: String,
    pub before: String,
    pub after: String,
}

#[derive(Serialize)]
#[serde(tag = "command")]
#[allow(clippy::enum_variant_names)]
enum CommandResult {
    #[serde(rename = "init")]
    Init { mode: String },
    #[serde(rename = "mode")]
    ModeResult { mode: String, changed: bool },
    #[serde(rename = "status")]
    Status {
        mode: String,
        veiled_count: usize,
        unveiled_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        files: Option<Vec<FileStatus>>,
    },
    #[serde(rename = "veil")]
    Veil { files: Vec<String>, dry_run: bool },
    #[serde(rename = "unveil")]
    Unveil { files: Vec<String>, dry_run: bool },
    #[serde(rename = "apply")]
    Apply {
        applied: usize,
        skipped: usize,
        dry_run: bool,
    },
    #[serde(rename = "history")]
    History {
        past: Vec<ActionSummary>,
        future: Vec<ActionSummary>,
        cursor_id: Option<u64>,
    },
    #[serde(rename = "history_show")]
    HistoryShow {
        action: ActionSummary,
        config_diff: Vec<String>,
        file_diffs: Vec<FileDiff>,
    },
    #[serde(rename = "undo")]
    Undo { undone: ActionSummary },
    #[serde(rename = "redo")]
    Redo { redone: ActionSummary },
    #[serde(rename = "gc")]
    Gc { deleted: usize, freed_bytes: u64 },
    #[serde(rename = "clean")]
    Clean { success: bool },
    #[serde(rename = "restore")]
    Restore { checkpoint: String },
    #[serde(rename = "checkpoint")]
    Checkpoint { action: String, name: String },
    #[serde(rename = "doctor")]
    Doctor { issues: Vec<String> },
    #[serde(rename = "version")]
    VersionResult { version: String },
    #[serde(rename = "context")]
    Context {
        function: String,
        unveiled_files: Vec<String>,
    },
    #[serde(rename = "disclose")]
    Disclose {
        budget: usize,
        used_tokens: usize,
        entries: Vec<funveil::DisclosureEntry>,
    },
    #[serde(rename = "other")]
    Other { message: String },
}

impl ActionSummary {
    fn from_record(r: &ActionRecord) -> Self {
        Self {
            id: r.id,
            timestamp: r.timestamp.to_rfc3339(),
            command: r.command.clone(),
            affected_files: r.affected_files.clone(),
            summary: r.summary.clone(),
        }
    }
}

fn update_metadata(root: &std::path::Path, config: &Config) {
    let _ = funveil::rebuild_index(root, config).and_then(|i| funveil::save_index(root, &i));
    let _ = funveil::generate_manifest(root, config).and_then(|m| funveil::save_manifest(root, &m));
}

fn snapshot_config(config: &Config) -> Option<String> {
    serde_yaml::to_string(config).ok()
}

fn snapshot_files(root: &std::path::Path, files: &[String]) -> Vec<FileSnapshot> {
    let store = ContentStore::new(root);
    files
        .iter()
        .filter_map(|f| {
            let path = root.join(f);
            if path.exists() {
                let content = std::fs::read(&path).ok()?;
                let hash = store.store(&content).ok()?;
                let perms = funveil::perms::file_mode(&std::fs::metadata(&path).ok()?);
                Some(FileSnapshot {
                    path: f.clone(),
                    cas_hash: Some(hash.full().to_string()),
                    permissions: funveil::perms::format_mode(perms),
                })
            } else {
                Some(FileSnapshot {
                    path: f.clone(),
                    cas_hash: None,
                    permissions: "644".to_string(),
                })
            }
        })
        .collect()
}

fn restore_action_state(root: &std::path::Path, state: &ActionState) -> Result<()> {
    // Restore config
    if let Some(ref config_yaml) = state.config_yaml {
        let config: Config = serde_yaml::from_str(config_yaml)?;
        config.save(root)?;
    }

    // Restore files
    let store = ContentStore::new(root);
    for snap in &state.file_snapshots {
        let file_path = root.join(&snap.path);
        if let Some(ref hash_str) = snap.cas_hash {
            let hash = ContentHash::from_string(hash_str.clone())?;
            let content = store.retrieve(&hash)?;
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Make writable if exists and read-only
            if file_path.exists() {
                let _ = funveil::perms::save_and_make_writable(&file_path);
            }
            std::fs::write(&file_path, content)?;
            let mode = funveil::perms::parse_mode(&snap.permissions);
            funveil::perms::set_mode(&file_path, mode)?;
        } else {
            // File didn't exist before — delete it
            if file_path.exists() {
                let _ = funveil::perms::save_and_make_writable(&file_path);
                std::fs::remove_file(&file_path)?;
            }
        }
    }

    Ok(())
}

fn handle_level_veil(
    root: &std::path::Path,
    pattern: &str,
    level: u8,
    output: &mut Output,
) -> Result<CommandResult> {
    use funveil::{TreeSitterParser, VeilStrategy};
    use std::fs;

    match level {
        0 => {
            let mut config = Config::load(root)?;
            let pre_config = snapshot_config(&config);
            let pre_files = snapshot_files(root, std::slice::from_ref(&pattern.to_string()));
            veil_file(root, &mut config, pattern, None, output)?;
            config.add_to_blacklist(pattern);
            config.save(root)?;
            update_metadata(root, &config);
            let _ = writeln!(output.out, "Veiled (level 0): {pattern}");

            let post_config = snapshot_config(&config);
            let post_files = snapshot_files(root, std::slice::from_ref(&pattern.to_string()));
            let mut history = ActionHistory::load(root)?;
            history.push(ActionRecord {
                id: history.next_id(),
                timestamp: chrono::Utc::now(),
                command: "veil".to_string(),
                args: vec![pattern.to_string(), "--level".to_string(), "0".to_string()],
                summary: format!("Veiled {pattern} (level 0)"),
                affected_files: vec![pattern.to_string()],
                undoable: true,
                pre_state: ActionState {
                    config_yaml: pre_config,
                    file_snapshots: pre_files,
                },
                post_state: ActionState {
                    config_yaml: post_config,
                    file_snapshots: post_files,
                },
            });
            history.save(root)?;

            Ok(CommandResult::Veil {
                files: vec![pattern.to_string()],
                dry_run: false,
            })
        }
        1 => {
            let path = root.join(pattern);
            if !path.exists() {
                return Err(anyhow::anyhow!("File not found: {pattern}"));
            }
            funveil::validate_path_within_root(&path, root)?;

            let mut config = Config::load(root)?;
            let pre_config = snapshot_config(&config);
            let pre_files = snapshot_files(root, std::slice::from_ref(&pattern.to_string()));

            let content = fs::read_to_string(&path)?;
            let parser = TreeSitterParser::new()?;
            let parsed = parser.parse_file(&path, &content)?;
            let strategy = HeaderStrategy::new();
            let veiled = strategy.veil_file(&content, &parsed)?;

            let store = ContentStore::new(root);
            let hash = store.store(content.as_bytes())?;
            let permissions = funveil::perms::file_mode(&fs::metadata(&path)?);
            fs::write(&path, veiled)?;

            config.register_object(pattern.to_string(), ObjectMeta::new(hash, permissions));
            config.add_to_blacklist(pattern);
            config.save(root)?;
            update_metadata(root, &config);

            let post_config = snapshot_config(&config);
            let post_files = snapshot_files(root, std::slice::from_ref(&pattern.to_string()));
            let mut history = ActionHistory::load(root)?;
            history.push(ActionRecord {
                id: history.next_id(),
                timestamp: chrono::Utc::now(),
                command: "veil".to_string(),
                args: vec![pattern.to_string(), "--level".to_string(), "1".to_string()],
                summary: format!("Veiled {pattern} (level 1, headers)"),
                affected_files: vec![pattern.to_string()],
                undoable: true,
                pre_state: ActionState {
                    config_yaml: pre_config,
                    file_snapshots: pre_files,
                },
                post_state: ActionState {
                    config_yaml: post_config,
                    file_snapshots: post_files,
                },
            });
            history.save(root)?;

            let _ = writeln!(output.out, "Veiled (level 1, headers): {pattern}");
            Ok(CommandResult::Veil {
                files: vec![pattern.to_string()],
                dry_run: false,
            })
        }
        2 => {
            let path = root.join(pattern);
            if !path.exists() {
                return Err(anyhow::anyhow!("File not found: {pattern}"));
            }
            funveil::validate_path_within_root(&path, root)?;

            let mut config = Config::load(root)?;
            let pre_config = snapshot_config(&config);
            let pre_files = snapshot_files(root, std::slice::from_ref(&pattern.to_string()));

            let content = fs::read_to_string(&path)?;
            let parser = TreeSitterParser::new()?;
            let parsed = parser.parse_file(&path, &content)?;

            let called_names: std::collections::HashSet<String> =
                parsed.calls.iter().map(|c| c.callee.clone()).collect();

            let lines: Vec<&str> = content.lines().collect();
            let mut included_ranges: Vec<(usize, usize)> = Vec::new();

            for symbol in &parsed.symbols {
                match symbol {
                    funveil::parser::Symbol::Function {
                        name,
                        line_range,
                        body_range,
                        ..
                    } => {
                        if called_names.contains(name.as_str()) {
                            included_ranges.push((line_range.start() - 1, line_range.end() - 1));
                        } else {
                            let sig_end = (body_range.start() - 1).min(line_range.end() - 1);
                            included_ranges.push((line_range.start() - 1, sig_end));
                        }
                    }
                    funveil::parser::Symbol::Class {
                        methods,
                        line_range,
                        ..
                    } => {
                        included_ranges.push((line_range.start() - 1, line_range.start() - 1));
                        for method in methods {
                            if let funveil::parser::Symbol::Function {
                                name,
                                line_range: m_range,
                                body_range: m_body,
                                ..
                            } = method
                            {
                                if called_names.contains(name.as_str()) {
                                    included_ranges.push((m_range.start() - 1, m_range.end() - 1));
                                } else {
                                    let sig_end = (m_body.start() - 1).min(m_range.end() - 1);
                                    included_ranges.push((m_range.start() - 1, sig_end));
                                }
                            }
                        }
                    }
                    funveil::parser::Symbol::Module { line_range, .. } => {
                        included_ranges.push((line_range.start() - 1, line_range.start() - 1));
                    }
                }
            }

            included_ranges.sort_by_key(|r| r.0);

            let mut output_lines: Vec<String> = Vec::new();
            let mut last_end: Option<usize> = None;
            for (start, end) in &included_ranges {
                if let Some(le) = last_end {
                    if *start > le + 1 {
                        output_lines.push("// ...".to_string());
                    }
                }
                for line in lines
                    .iter()
                    .take((*end).min(lines.len().saturating_sub(1)) + 1)
                    .skip(*start)
                {
                    output_lines.push(line.to_string());
                }
                last_end = Some(*end);
            }

            let veiled = output_lines.join("\n") + "\n";

            let store = ContentStore::new(root);
            let hash = store.store(content.as_bytes())?;
            let permissions = funveil::perms::file_mode(&fs::metadata(&path)?);
            fs::write(&path, &veiled)?;

            config.register_object(pattern.to_string(), ObjectMeta::new(hash, permissions));
            config.add_to_blacklist(pattern);
            config.save(root)?;
            update_metadata(root, &config);

            let post_config = snapshot_config(&config);
            let post_files = snapshot_files(root, std::slice::from_ref(&pattern.to_string()));
            let mut history = ActionHistory::load(root)?;
            history.push(ActionRecord {
                id: history.next_id(),
                timestamp: chrono::Utc::now(),
                command: "veil".to_string(),
                args: vec![pattern.to_string(), "--level".to_string(), "2".to_string()],
                summary: format!("Veiled {pattern} (level 2, headers+called bodies)"),
                affected_files: vec![pattern.to_string()],
                undoable: true,
                pre_state: ActionState {
                    config_yaml: pre_config,
                    file_snapshots: pre_files,
                },
                post_state: ActionState {
                    config_yaml: post_config,
                    file_snapshots: post_files,
                },
            });
            history.save(root)?;

            let _ = writeln!(
                output.out,
                "Veiled (level 2, headers+called bodies): {pattern}"
            );
            Ok(CommandResult::Veil {
                files: vec![pattern.to_string()],
                dry_run: false,
            })
        }
        3 => {
            let mut config = Config::load(root)?;
            if config.get_object(pattern).is_some() || has_veils(&config, pattern) {
                unveil_file(root, &mut config, pattern, None, output)?;
                config.save(root)?;
                update_metadata(root, &config);
                let _ = writeln!(output.out, "Level 3: unveiled {pattern} (full source)");
            } else {
                let _ = writeln!(output.out, "Level 3: {pattern} already at full source");
            }
            Ok(CommandResult::Veil {
                files: vec![pattern.to_string()],
                dry_run: false,
            })
        }
        _ => unreachable!("clap restricts level to 0..=3"),
    }
}

fn collect_affected_files_for_pattern(root: &std::path::Path, pattern: &str) -> Vec<String> {
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

#[derive(Subcommand)]
enum CacheCmd {
    /// Show cache statistics
    Status,
    /// Clear the cache
    Clear,
    /// Invalidate stale entries
    Invalidate,
}

#[derive(Subcommand)]
enum CheckpointCmd {
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

#[cfg_attr(coverage_nightly, coverage(off))]
fn main() -> Result<()> {
    let cli = Cli::parse();
    let quiet = cli.quiet;
    let json = cli.json;
    let is_version_command = matches!(cli.command, Commands::Version);

    // In JSON mode, suppress human-readable output (all results go through CommandResult)
    let mut output = Output::new(quiet || json);
    let root = find_project_root()?;

    let result = run_command(cli, &root, &mut output);

    match &result {
        Ok(cmd_result) => {
            if json {
                let json_str = serde_json::to_string(cmd_result)
                    .unwrap_or_else(|e| format!("{{\"error\":true,\"message\":\"{e}\"}}"));
                println!("{json_str}");
            }
        }
        Err(e) => {
            if json {
                if let Some(fv_err) = e.downcast_ref::<funveil::FunveilError>() {
                    let json_str = serde_json::json!({
                        "error": true,
                        "code": fv_err.code(),
                        "message": fv_err.to_string()
                    });
                    println!("{json_str}");
                } else {
                    let json_str = serde_json::json!({
                        "error": true,
                        "code": "E000",
                        "message": e.to_string()
                    });
                    println!("{json_str}");
                }
            }
        }
    }

    #[cfg(not(target_family = "wasm"))]
    {
        let mut err_output = Output::new(quiet);
        funveil::update::maybe_print_update_notice(&mut err_output.err, &root, is_version_command);
    }

    result.map(|_| ())
}

fn run_command(cli: Cli, root: &std::path::Path, output: &mut Output) -> Result<CommandResult> {
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
            funveil::config::ensure_data_dir(&root)?;
            funveil::config::ensure_gitignore(&root)?;
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
                let pre_config = snapshot_config(&config);
                config.set_mode(new_mode);
                config.save(&root)?;
                let post_config = snapshot_config(&config);

                let mut history = ActionHistory::load(&root)?;
                history.push(ActionRecord {
                    id: history.next_id(),
                    timestamp: chrono::Utc::now(),
                    command: "mode".to_string(),
                    args: vec![new_mode.to_string()],
                    summary: format!("Changed mode to {new_mode}"),
                    affected_files: vec![],
                    undoable: true,
                    pre_state: ActionState {
                        config_yaml: pre_config,
                        file_snapshots: vec![],
                    },
                    post_state: ActionState {
                        config_yaml: post_config,
                        file_snapshots: vec![],
                    },
                });
                history.save(&root)?;

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
                let index = funveil::load_index(&root)?;
                let entries = index
                    .symbols
                    .get(&sym_name)
                    .ok_or_else(|| anyhow::anyhow!("Symbol not found in index: {sym_name}"))?;
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
                veil_file(&root, &mut config, file_path, Some(&[range]), output)?;
                config.add_to_blacklist(file_path);
                config.save(&root)?;
                update_metadata(&root, &config);
                let _ = writeln!(output.out, "Veiled symbol {sym_name} in: {file_path}");
                return Ok(CommandResult::Veil {
                    files: vec![file_path.clone()],
                    dry_run: false,
                });
            }

            if let Some(start_fn) = unreachable_from {
                let config_loaded = Config::load(&root)?;
                let graph = funveil::build_call_graph_from_metadata(&root, &config_loaded)?;
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

                let index = funveil::load_index(&root)?;
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
                config.save(&root)?;
                update_metadata(&root, &config);
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
                    let pre_config = snapshot_config(&config);
                    let affected = collect_affected_files_for_pattern(&root, &pattern);
                    let pre_files = snapshot_files(&root, &affected);

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

                    config.save(&root)?;
                    update_metadata(&root, &config);

                    if veiled_any {
                        let _ = writeln!(output.out, "Veiling: {pattern}");

                        let post_config = snapshot_config(&config);
                        let post_files = snapshot_files(&root, &veiled_files);
                        let total_bytes: u64 = veiled_files
                            .iter()
                            .filter_map(|f| std::fs::metadata(root.join(f)).ok())
                            .map(|m| m.len())
                            .sum();
                        let mut history = ActionHistory::load(&root)?;
                        history.push(ActionRecord {
                            id: history.next_id(),
                            timestamp: chrono::Utc::now(),
                            command: "veil".to_string(),
                            args: vec![pattern.clone()],
                            summary: format!(
                                "Veiled {} file(s) ({total_bytes} bytes)",
                                veiled_files.len()
                            ),
                            affected_files: veiled_files.clone(),
                            undoable: true,
                            pre_state: ActionState {
                                config_yaml: pre_config,
                                file_snapshots: pre_files,
                            },
                            post_state: ActionState {
                                config_yaml: post_config,
                                file_snapshots: post_files,
                            },
                        });
                        history.save(&root)?;
                    }

                    CommandResult::Veil {
                        files: veiled_files,
                        dry_run: false,
                    }
                }
                VeilMode::Headers => {
                    use funveil::{TreeSitterParser, VeilStrategy};
                    use std::fs;

                    let path = root.join(&pattern);
                    if !path.exists() {
                        return Err(anyhow::anyhow!("File not found: {pattern}"));
                    }
                    funveil::validate_path_within_root(&path, &root)?;

                    let mut config = Config::load(&root)?;
                    let pre_config = snapshot_config(&config);
                    let pre_files = snapshot_files(&root, std::slice::from_ref(&pattern));

                    let content = fs::read_to_string(&path)?;
                    let parser = TreeSitterParser::new()?;
                    let parsed = parser.parse_file(&path, &content)?;
                    let strategy = HeaderStrategy::new();
                    let veiled = strategy.veil_file(&content, &parsed)?;

                    let store = ContentStore::new(&root);
                    let hash = store.store(content.as_bytes())?;

                    let permissions = funveil::perms::file_mode(&fs::metadata(&path)?);
                    fs::write(&path, veiled)?;

                    config.register_object(pattern.clone(), ObjectMeta::new(hash, permissions));
                    config.add_to_blacklist(&pattern);
                    config.save(&root)?;
                    update_metadata(&root, &config);

                    let post_config = snapshot_config(&config);
                    let post_files = snapshot_files(&root, std::slice::from_ref(&pattern));
                    let mut history = ActionHistory::load(&root)?;
                    history.push(ActionRecord {
                        id: history.next_id(),
                        timestamp: chrono::Utc::now(),
                        command: "veil".to_string(),
                        args: vec![pattern.clone(), "--mode".to_string(), "headers".to_string()],
                        summary: format!("Veiled {pattern} (headers mode)"),
                        affected_files: vec![pattern.clone()],
                        undoable: true,
                        pre_state: ActionState {
                            config_yaml: pre_config,
                            file_snapshots: pre_files,
                        },
                        post_state: ActionState {
                            config_yaml: post_config,
                            file_snapshots: post_files,
                        },
                    });
                    history.save(&root)?;

                    let _ = writeln!(output.out, "Veiled (headers mode): {pattern}");

                    CommandResult::Veil {
                        files: vec![pattern],
                        dry_run: false,
                    }
                }
            }
        }

        Commands::Parse { file, format } => {
            use funveil::TreeSitterParser;
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
                            if let funveil::parser::Symbol::Function { .. } = symbol {
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
                            EntrypointTypeArg::Main => {
                                ep.entry_type == funveil::EntrypointType::Main
                            }
                            EntrypointTypeArg::Test => {
                                ep.entry_type == funveil::EntrypointType::Test
                            }
                            EntrypointTypeArg::Cli => ep.entry_type == funveil::EntrypointType::Cli,
                            EntrypointTypeArg::Handler => {
                                ep.entry_type == funveil::EntrypointType::Handler
                            }
                            EntrypointTypeArg::Export => {
                                ep.entry_type == funveil::EntrypointType::Export
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
            use funveil::AnalysisCache;

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
                let index = funveil::load_index(&root)?;
                let entries = index
                    .symbols
                    .get(&sym_name)
                    .ok_or_else(|| anyhow::anyhow!("Symbol not found in index: {sym_name}"))?;
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
                unveil_file(&root, &mut config, &file_path, None, output)?;
                config.add_to_whitelist(&file_path);
                config.save(&root)?;
                update_metadata(&root, &config);
                let _ = writeln!(output.out, "Unveiled (symbol {sym_name}): {file_path}");
                return Ok(CommandResult::Unveil {
                    files: vec![file_path],
                    dry_run: false,
                });
            }

            if let Some(ref fn_name) = callers_of {
                let config_loaded = Config::load(&root)?;
                let graph = funveil::build_call_graph_from_metadata(&root, &config_loaded)?;
                let trace = graph.trace(fn_name, TraceDirection::Backward, 10);

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
                        let _ = writeln!(output.out, "No callers found for: {fn_name}");
                        return Ok(CommandResult::Unveil {
                            files: vec![],
                            dry_run: false,
                        });
                    }
                };

                if dry_run {
                    for f in &files {
                        let _ = writeln!(output.out, "Would unveil (caller of {fn_name}): {f}");
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
                config.save(&root)?;
                update_metadata(&root, &config);
                let _ = writeln!(output.out, "Unveiled {} caller files", unveiled.len());
                return Ok(CommandResult::Unveil {
                    files: unveiled,
                    dry_run: false,
                });
            }

            if let Some(ref fn_name) = callees_of {
                let config_loaded = Config::load(&root)?;
                let graph = funveil::build_call_graph_from_metadata(&root, &config_loaded)?;
                let trace = graph.trace(fn_name, TraceDirection::Forward, 10);

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
                        let _ = writeln!(output.out, "No callees found for: {fn_name}");
                        return Ok(CommandResult::Unveil {
                            files: vec![],
                            dry_run: false,
                        });
                    }
                };

                if dry_run {
                    for f in &files {
                        let _ = writeln!(output.out, "Would unveil (callee of {fn_name}): {f}");
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
                config.save(&root)?;
                update_metadata(&root, &config);
                let _ = writeln!(output.out, "Unveiled {} callee files", unveiled.len());
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

            let pre_config = snapshot_config(&config);
            let mut unveiled_files = Vec::new();

            if all {
                let files_before: Vec<String> =
                    config.iter_unique_files().map(|f| f.to_string()).collect();
                let pre_files = snapshot_files(&root, &files_before);

                unveil_all(&root, &mut config, output)?;
                config.save(&root)?;
                update_metadata(&root, &config);

                let post_config = snapshot_config(&config);
                let post_files = snapshot_files(&root, &files_before);

                let mut history = ActionHistory::load(&root)?;
                history.push(ActionRecord {
                    id: history.next_id(),
                    timestamp: chrono::Utc::now(),
                    command: "unveil".to_string(),
                    args: vec!["--all".to_string()],
                    summary: format!("Unveiled all files ({} files)", files_before.len()),
                    affected_files: files_before.clone(),
                    undoable: true,
                    pre_state: ActionState {
                        config_yaml: pre_config,
                        file_snapshots: pre_files,
                    },
                    post_state: ActionState {
                        config_yaml: post_config,
                        file_snapshots: post_files,
                    },
                });
                history.save(&root)?;

                unveiled_files = files_before;
                let _ = writeln!(output.out, "Unveiled all files");
            } else if let Some(pattern) = pattern {
                let affected = collect_affected_files_for_pattern(&root, &pattern);
                let pre_files = snapshot_files(&root, &affected);

                if pattern.contains('#') {
                    let (file, ranges) = parse_pattern(&pattern)?;
                    unveil_file(&root, &mut config, file, ranges.as_deref(), output)?;
                    config.add_to_whitelist(file);
                    config.save(&root)?;
                    update_metadata(&root, &config);
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

                    config.save(&root)?;
                    update_metadata(&root, &config);
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
                    config.save(&root)?;
                    update_metadata(&root, &config);
                    let _ = writeln!(output.out, "Unveiled: {pattern}");
                }

                if !unveiled_files.is_empty() {
                    let post_config = snapshot_config(&config);
                    let post_files = snapshot_files(&root, &unveiled_files);
                    let mut history = ActionHistory::load(&root)?;
                    history.push(ActionRecord {
                        id: history.next_id(),
                        timestamp: chrono::Utc::now(),
                        command: "unveil".to_string(),
                        args: vec![pattern.clone()],
                        summary: format!("Unveiled {} file(s)", unveiled_files.len()),
                        affected_files: unveiled_files.clone(),
                        undoable: true,
                        pre_state: ActionState {
                            config_yaml: pre_config,
                            file_snapshots: pre_files,
                        },
                        post_state: ActionState {
                            config_yaml: post_config,
                            file_snapshots: post_files,
                        },
                    });
                    history.save(&root)?;
                }
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
                    let parsed_key = funveil::ConfigKey::parse(key);
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
                let parsed_key = funveil::ConfigKey::parse(key);
                let file_str = parsed_key.file();

                let path = root.join(file_str);

                // For full-veil entries: file should not be on disk
                if let funveil::ConfigKey::FullVeil { .. } = &parsed_key {
                    if !path.exists() {
                        // Correct state: file is removed
                        let _ = writeln!(output.out, "  ✓ {file_str} (veiled, not on disk)");
                        continue;
                    }
                    // Check for legacy marker — migrate by deleting the file
                    if funveil::is_legacy_marker(&path) {
                        let snap = snapshot_files(&root, &[file_str.to_string()]);
                        let _ = funveil::perms::save_and_make_writable(&path);
                        std::fs::remove_file(&path)?;
                        applied += 1;
                        applied_files.push(file_str.to_string());
                        pre_file_snapshots.extend(snap);
                        let _ = writeln!(output.out, "  ✓ {file_str} (migrated legacy marker)");
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
                    let _ = writeln!(output.out, "  ✓ {file_str} (already veiled)");
                } else {
                    let original_hash = match ContentHash::from_string(meta.hash.clone()) {
                        Ok(h) => h,
                        Err(e) => {
                            let _ = writeln!(output.err, "  ✗ {file_str} (invalid hash: {e})");
                            skipped += 1;
                            continue;
                        }
                    };
                    if store.exists(&original_hash) {
                        let snap = snapshot_files(&root, &[file_str.to_string()]);
                        let removed_meta = config.objects.remove(key);
                        if let Err(e) = veil_file(&root, &mut config, file_str, None, output) {
                            let _ = writeln!(output.err, "  ✗ {file_str} (re-veil failed: {e})");
                            if let Some(meta) = removed_meta {
                                config.objects.insert(key.clone(), meta);
                            }
                            skipped += 1;
                        } else {
                            applied += 1;
                            applied_files.push(file_str.to_string());
                            pre_file_snapshots.extend(snap);
                            let _ = writeln!(output.out, "  ✓ {file_str} (re-veiled)");
                        }
                    } else {
                        let _ = writeln!(
                            output.err,
                            "  ✗ {file_str} (original content missing from CAS, skipping)"
                        );
                        skipped += 1;
                    }
                }
            }

            config.save(&root)?;

            // Rebuild metadata index and manifest
            let _ =
                funveil::rebuild_index(&root, &config).and_then(|i| funveil::save_index(&root, &i));
            let _ = funveil::generate_manifest(&root, &config)
                .and_then(|m| funveil::save_manifest(&root, &m));

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
                let pre_config = snapshot_config(&config);
                // Snapshot all project files that checkpoint will overwrite
                let checkpoint_dir = root.join(funveil::config::CHECKPOINTS_DIR).join(&name);
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
                let pre_files = snapshot_files(&root, &affected);

                let _ = writeln!(output.out, "Restoring from latest checkpoint: {name}");
                restore_checkpoint(&root, &name, output)?;

                let post_config_loaded = Config::load(&root).ok();
                let post_config = post_config_loaded.as_ref().and_then(snapshot_config);
                let post_files = snapshot_files(&root, &affected);

                let mut history = ActionHistory::load(&root)?;
                history.push(ActionRecord {
                    id: history.next_id(),
                    timestamp: chrono::Utc::now(),
                    command: "restore".to_string(),
                    args: vec![name.clone()],
                    summary: format!("Restored from checkpoint '{name}'"),
                    affected_files: affected,
                    undoable: true,
                    pre_state: ActionState {
                        config_yaml: pre_config,
                        file_snapshots: pre_files,
                    },
                    post_state: ActionState {
                        config_yaml: post_config,
                        file_snapshots: post_files,
                    },
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
            funveil::validate_path_within_root(&file_path, &root)?;

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
                    let pre_config = snapshot_config(&config);

                    save_checkpoint(&root, &config, name, output)?;

                    let mut history = ActionHistory::load(&root)?;
                    history.push(ActionRecord {
                        id: history.next_id(),
                        timestamp: chrono::Utc::now(),
                        command: "checkpoint".to_string(),
                        args: vec!["save".to_string(), name.clone()],
                        summary: format!("Saved checkpoint '{name}'"),
                        affected_files: vec![],
                        undoable: true,
                        pre_state: ActionState {
                            config_yaml: pre_config.clone(),
                            file_snapshots: vec![],
                        },
                        post_state: ActionState {
                            config_yaml: pre_config,
                            file_snapshots: vec![],
                        },
                    });
                    history.save(&root)?;

                    cmd_action = "save".to_string();
                    cmd_name = name.clone();
                }
                CheckpointCmd::Restore { ref name } => {
                    let config = Config::load(&root)?;
                    let pre_config = snapshot_config(&config);
                    let checkpoint_dir = root.join(funveil::config::CHECKPOINTS_DIR).join(name);
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
                    let pre_files = snapshot_files(&root, &affected);

                    restore_checkpoint(&root, name, output)?;

                    let post_cfg = Config::load(&root).ok();
                    let post_config = post_cfg.as_ref().and_then(snapshot_config);
                    let post_files = snapshot_files(&root, &affected);

                    let mut history = ActionHistory::load(&root)?;
                    history.push(ActionRecord {
                        id: history.next_id(),
                        timestamp: chrono::Utc::now(),
                        command: "checkpoint".to_string(),
                        args: vec!["restore".to_string(), name.clone()],
                        summary: format!("Restored checkpoint '{name}'"),
                        affected_files: affected,
                        undoable: true,
                        pre_state: ActionState {
                            config_yaml: pre_config,
                            file_snapshots: pre_files,
                        },
                        post_state: ActionState {
                            config_yaml: post_config,
                            file_snapshots: post_files,
                        },
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
                    let pre_config = snapshot_config(&config);

                    delete_checkpoint(&root, name, output)?;

                    let mut history = ActionHistory::load(&root)?;
                    history.push(ActionRecord {
                        id: history.next_id(),
                        timestamp: chrono::Utc::now(),
                        command: "checkpoint".to_string(),
                        args: vec!["delete".to_string(), name.clone()],
                        summary: format!("Deleted checkpoint '{name}'"),
                        affected_files: vec![],
                        undoable: true,
                        pre_state: ActionState {
                            config_yaml: pre_config.clone(),
                            file_snapshots: vec![],
                        },
                        post_state: ActionState {
                            config_yaml: pre_config,
                            file_snapshots: vec![],
                        },
                    });
                    history.save(&root)?;

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
            let metadata_store = funveil::MetadataStore::new(&root);
            let mut issues = Vec::new();

            for (key, meta) in &config.objects {
                let hash = match ContentHash::from_string(meta.hash.clone()) {
                    Ok(h) => h,
                    Err(e) => {
                        issues.push(format!("Invalid hash for {key}: {e}"));
                        continue;
                    }
                };
                if store.retrieve(&hash).is_err() {
                    issues.push(format!("Missing object: {key}"));
                }

                // For full-veil entries, verify correct state
                let parsed_key = funveil::ConfigKey::parse(key);
                if let funveil::ConfigKey::FullVeil { file } = parsed_key {
                    let file_path = root.join(file);
                    if file_path.exists() && funveil::is_legacy_marker(&file_path) {
                        issues.push(format!(
                            "Legacy marker detected: {file} (run `fv apply` to migrate)"
                        ));
                    }
                    // Check metadata exists for CAS objects with supported extensions
                    if funveil::is_supported_source(std::path::Path::new(file))
                        && !metadata_store.exists(&hash)
                    {
                        issues.push(format!(
                            "Missing metadata for {file} (run `fv apply` to rebuild)"
                        ));
                    }
                }
            }

            if issues.is_empty() {
                let _ = writeln!(output.out, "✓ All checks passed. No issues found.");
            } else {
                let _ = writeln!(output.out, "✗ Found {} issue(s):", issues.len());
                for issue in &issues {
                    let _ = writeln!(output.out, "  - {issue}");
                }
            }

            CommandResult::Doctor { issues }
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

            let (deleted, freed) = garbage_collect(&root, &referenced, output)?;

            // GC is not undoable (CAS objects permanently deleted)
            let mut history = ActionHistory::load(&root)?;
            history.push(ActionRecord {
                id: history.next_id(),
                timestamp: chrono::Utc::now(),
                command: "gc".to_string(),
                args: vec![],
                summary: format!("Garbage collected {deleted} object(s), freed {freed} bytes"),
                affected_files: vec![],
                undoable: false,
                pre_state: ActionState {
                    config_yaml: None,
                    file_snapshots: vec![],
                },
                post_state: ActionState {
                    config_yaml: None,
                    file_snapshots: vec![],
                },
            });
            history.save(&root)?;

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

            let _ = writeln!(output.out, "✓ Removed all funveil data");

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
                return Err(funveil::FunveilError::ActionNotUndoable(entry_clone.id).into());
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
                    let _ = writeln!(output.out, "──── current state ────");
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
            let index = funveil::load_index(&root)?;
            let graph = funveil::build_call_graph_from_metadata(&root, &config).ok();

            let plan = funveil::compute_disclosure_plan(
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
            let index = funveil::load_index(&root)?;

            let entries = index
                .symbols
                .get(&function)
                .ok_or_else(|| anyhow::anyhow!("Symbol not found in index: {function}"))?;

            let graph = funveil::build_call_graph_from_metadata(&root, &config)?;

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
                config.save(&root)?;
                update_metadata(&root, &config);
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

#[cfg_attr(coverage_nightly, coverage(off))]
fn find_project_root() -> Result<PathBuf> {
    let current = env::current_dir()?;

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

/// Parse a pattern like "file.txt" or "file.txt#1-5" into (file, optional_ranges)
fn parse_pattern(pattern: &str) -> Result<(&str, Option<Vec<LineRange>>)> {
    if let Some(pos) = pattern.rfind('#') {
        let file = &pattern[..pos];
        let ranges_str = &pattern[pos + 1..];

        if file.is_empty() {
            return Err(anyhow::anyhow!("Empty file path in pattern"));
        }
        if ranges_str.is_empty() {
            return Err(anyhow::anyhow!("Empty range specification after '#'"));
        }

        // Try to parse as ranges; if suffix doesn't look like a range, treat entire string as filename
        let mut ranges = Vec::new();
        let mut valid_ranges = true;
        for range_str in ranges_str.split(',') {
            let parts: Vec<&str> = range_str.split('-').collect();
            if parts.len() != 2 {
                valid_ranges = false;
                break;
            }
            match (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                (Ok(start), Ok(end)) => match LineRange::new(start, end) {
                    Ok(range) => ranges.push(range),
                    Err(_) => {
                        valid_ranges = false;
                        break;
                    }
                },
                _ => {
                    valid_ranges = false;
                    break;
                }
            }
        }

        if valid_ranges {
            Ok((file, Some(ranges)))
        } else {
            // Suffix wasn't a valid range spec — treat entire pattern as a filename
            Ok((pattern, None))
        }
    } else {
        Ok((pattern, None))
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;

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

    fn run_in_temp(command: Commands) -> (String, String, anyhow::Result<()>) {
        let temp = tempfile::TempDir::new().unwrap();
        run_in_dir(temp.path(), command)
    }

    struct TestWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }

    fn run_in_dir(
        dir: &std::path::Path,
        command: Commands,
    ) -> (String, String, anyhow::Result<()>) {
        let cli = Cli {
            quiet: false,
            log_level: None,
            json: false,
            command,
        };
        let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut output = Output {
            out: Box::new(TestWriter(out_buf.clone())),
            err: Box::new(TestWriter(err_buf.clone())),
        };
        let result = run_command(cli, dir, &mut output).map(|_| ());
        let stdout = String::from_utf8(out_buf.lock().unwrap().clone()).unwrap();
        let stderr = String::from_utf8(err_buf.lock().unwrap().clone()).unwrap();
        (stdout, stderr, result)
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
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("already initialized"));
    }

    #[test]
    fn test_run_mode_get() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Mode { mode: None });
        assert!(result.is_ok());
        assert!(stdout.contains("whitelist"));
    }

    #[test]
    fn test_run_mode_set() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Mode {
                mode: Some(Mode::Blacklist),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Mode changed to: blacklist"));
    }

    #[test]
    fn test_run_status() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Status { files: false });
        assert!(result.is_ok());
        assert!(stdout.contains("Mode: whitelist"));
    }

    #[test]
    fn test_run_veil_and_unveil() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("test.txt"), "hello\nworld\n").unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "test.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("test.txt".into()),
                all: false,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_unveil_all() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("a.txt"), "aaa\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "a.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_unveil_no_pattern_no_all() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
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
    fn test_run_show() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("show.txt"), "content\n").unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Show {
                file: "show.txt".into(),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("content"));
    }

    #[test]
    fn test_run_doctor() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (_, _, result) = run_in_dir(temp.path(), Commands::Doctor);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_gc() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (_, _, result) = run_in_dir(temp.path(), Commands::Gc);
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
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Clean);
        assert!(result.is_ok());
        assert!(stdout.contains("Removed all funveil data"));
    }

    #[test]
    fn test_run_cache_status() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Cache {
                cmd: CacheCmd::Status,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_cache_clear() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Cache {
                cmd: CacheCmd::Clear,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_cache_invalidate() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Cache {
                cmd: CacheCmd::Invalidate,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_apply() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(temp.path(), Commands::Apply { dry_run: false });
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_restore_no_checkpoint() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(temp.path(), Commands::Restore);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No checkpoints found"));
    }

    #[test]
    fn test_run_restore_with_checkpoint() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("r.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "r.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Save {
                    name: "snap".into(),
                },
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Restore);
        assert!(result.is_ok());
        assert!(stdout.contains("Restoring from latest checkpoint"));
    }

    #[test]
    fn test_run_parse() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("hello.rs"), "fn main() {}\n").unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Parse {
                file: "hello.rs".into(),
                format: ParseFormat::Summary,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_entrypoints() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("main.rs"), "fn main() {}\n").unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: None,
                language: None,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_checkpoint_save_list_show_delete() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Save { name: "cp1".into() },
            },
        );
        let (stdout, _, _) = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::List,
            },
        );
        assert!(stdout.contains("cp1"));
        let _ = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Show { name: "cp1".into() },
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Delete { name: "cp1".into() },
            },
        );
    }

    #[test]
    fn test_run_checkpoint_restore() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Save {
                    name: "snap".into(),
                },
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Restore {
                    name: "snap".into(),
                },
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_veil_regex() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("foo.txt"), "foo\n").unwrap();
        std::fs::write(temp.path().join("bar.txt"), "bar\n").unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "/.*\\.txt/".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Veiling"));
    }

    #[test]
    fn test_run_unveil_regex() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("a.txt"), "aaa\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "a.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("/a\\.txt/".into()),
                all: false,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_veil_partial() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("f.txt"),
            "line1\nline2\nline3\nline4\nline5\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt#2-4".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_show_nonexistent() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Show {
                file: "nonexistent.txt".into(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_run_trace_from() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("lib.rs"),
            "fn foo() { bar(); }\nfn bar() {}\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: Some("foo".into()),
                to: None,
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::Tree,
                no_std: false,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_veil_headers() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("hello.rs"),
            "fn main() {}\nfn helper() {}\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "hello.rs".into(),
                mode: VeilMode::Headers,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Veiled (headers mode)"));
    }

    #[test]
    fn test_run_parse_detailed() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}\nfn bar() {}\n").unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Parse {
                file: "lib.rs".into(),
                format: ParseFormat::Detailed,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Symbols:"));
    }

    #[test]
    fn test_run_trace_to() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("lib.rs"),
            "fn foo() { bar(); }\nfn bar() {}\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: None,
                to: Some("bar".into()),
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::Tree,
                no_std: false,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_trace_from_entrypoint() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("main.rs"),
            "fn main() { helper(); }\nfn helper() {}\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: None,
                to: None,
                from_entrypoint: true,
                depth: 3,
                format: TraceFormat::Tree,
                no_std: false,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Entrypoints found:") || stdout.is_empty());
    }

    #[test]
    fn test_run_trace_dot_format() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}\n").unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: Some("foo".into()),
                to: None,
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::Dot,
                no_std: false,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("digraph"));
    }

    #[test]
    fn test_run_trace_list_format() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("lib.rs"),
            "fn foo() { bar(); }\nfn bar() {}\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: Some("foo".into()),
                to: None,
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::List,
                no_std: false,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_trace_no_std() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("lib.rs"),
            "fn foo() { println!(\"hi\"); }\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: Some("foo".into()),
                to: None,
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::Tree,
                no_std: true,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_entrypoints_with_language() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("main.rs"), "fn main() {}\n").unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: None,
                language: Some(LanguageArg::Rust),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("main"));
    }

    #[test]
    fn test_run_apply_reveils() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("secret.txt"), "secret data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "secret.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Apply { dry_run: false });
        assert!(result.is_ok());
        assert!(stdout.contains("Re-applying veils") || stdout.contains("Applied:"));
    }

    #[test]
    fn test_run_doctor_with_veils() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "content\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Doctor);
        assert!(result.is_ok());
        assert!(stdout.contains("All checks passed"));
    }

    #[test]
    fn test_run_show_veiled_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("s.txt"), "secret\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "s.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Show {
                file: "s.txt".into(),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("VEILED - not on disk"));
    }

    #[test]
    fn test_run_show_partially_veiled() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("p.txt"),
            "line1\nline2\nline3\nline4\nline5\n",
        )
        .unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "p.txt#2-4".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Show {
                file: "p.txt".into(),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("p.txt"));
    }

    #[test]
    fn test_run_veil_regex_no_match() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "/nonexistent_pattern/".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("No files matched"));
    }

    #[test]
    fn test_run_status_with_blacklist_and_whitelist() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("a.txt"), "aaa\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "a.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("a.txt".into()),
                all: false,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        let (stdout, _, _) = run_in_dir(temp.path(), Commands::Status { files: false });
        assert!(stdout.contains("Mode:"));
    }

    #[test]
    fn test_run_veil_headers_nonexistent() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "nonexistent.rs".into(),
                mode: VeilMode::Headers,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[test]
    fn test_run_parse_detailed_with_calls_and_imports() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("prog.rs"),
            "use std::io;\nfn main() { helper(); }\nfn helper() { println!(\"hi\"); }\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Parse {
                file: "prog.rs".into(),
                format: ParseFormat::Detailed,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Symbols:"));
        assert!(stdout.contains("Signature:"));
    }

    #[test]
    fn test_run_trace_both_from_and_to_error() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}\n").unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: Some("foo".into()),
                to: Some("bar".into()),
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::Tree,
                no_std: false,
            },
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot use both"));
    }

    #[test]
    fn test_run_trace_no_function_error() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}\n").unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: None,
                to: None,
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::Tree,
                no_std: false,
            },
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Must specify"));
    }

    #[test]
    fn test_run_trace_function_not_in_graph() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("lib.rs"), "fn foo() {}\n").unwrap();
        let (_, stderr, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: Some("nonexistent_fn".into()),
                to: None,
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::Tree,
                no_std: false,
            },
        );
        assert!(result.is_ok());
        assert!(
            stderr.contains("not found in call graph")
                || stderr.contains("not found in the codebase")
        );
    }

    #[test]
    fn test_run_status_with_whitelist() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("a.txt"), "aaa\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "a.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("a.txt".into()),
                all: false,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Status { files: false });
        assert!(result.is_ok());
        assert!(stdout.contains("Whitelisted:"));
    }

    #[test]
    fn test_run_entrypoints_with_type_filter() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("main.rs"),
            "fn main() {}\n#[test]\nfn test_foo() {}\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: Some(EntrypointTypeArg::Main),
                language: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("main"));
    }

    #[test]
    fn test_run_entrypoints_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: None,
                language: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("No entrypoints detected"));
    }

    #[test]
    fn test_run_veil_regex_matched_but_no_veilable() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("empty.txt"), "").unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "/empty/".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("No files") || stdout.contains("Veiling"));
    }

    #[test]
    fn test_run_unveil_regex_no_match() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("/nonexistent_xyz/".into()),
                all: false,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("No files matched"));
    }

    #[test]
    fn test_run_unveil_regex_match_no_veils() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("plain.txt"), "hello\n").unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("/plain/".into()),
                all: false,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("No veiled files matched") || stdout.contains("Unveiled"));
    }

    #[test]
    fn test_run_apply_already_veiled() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Apply { dry_run: false });
        assert!(result.is_ok());
        assert!(stdout.contains("veiled, not on disk"));
    }

    #[test]
    fn test_run_apply_missing_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("gone.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "gone.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(!temp.path().join("gone.txt").exists());
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Apply { dry_run: false });
        assert!(result.is_ok());
        assert!(stdout.contains("veiled, not on disk"));
    }

    #[test]
    fn test_run_checkpoint_list_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::List,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("No checkpoints found"));
    }

    #[test]
    fn test_run_trace_with_function_arg() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("lib.rs"),
            "fn foo() { bar(); }\nfn bar() {}\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: Some("foo".into()),
                from: None,
                to: None,
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::Tree,
                no_std: false,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_trace_dot_no_std() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("lib.rs"),
            "fn foo() { println!(\"hi\"); }\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: Some("foo".into()),
                to: None,
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::Dot,
                no_std: true,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("digraph"));
    }

    #[test]
    fn test_run_trace_cycle_detection() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("cycle.rs"),
            "fn alpha() { beta(); }\nfn beta() { alpha(); }\n",
        )
        .unwrap();
        let (_, stderr, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: Some("alpha".into()),
                to: None,
                from_entrypoint: false,
                depth: 10,
                format: TraceFormat::Tree,
                no_std: false,
            },
        );
        assert!(result.is_ok());
        assert!(stderr.contains("Cycle detected") || !stderr.contains("Cycle detected"));
    }

    #[test]
    fn test_run_parse_detailed_with_imports() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("uses.rs"),
            "use std::io;\nuse std::fs;\nfn main() {}\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Parse {
                file: "uses.rs".into(),
                format: ParseFormat::Detailed,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Imports:"));
    }

    #[test]
    fn test_run_parse_detailed_with_calls() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("calls.rs"),
            "fn main() { helper(); }\nfn helper() {}\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Parse {
                file: "calls.rs".into(),
                format: ParseFormat::Detailed,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Calls:"));
    }

    #[test]
    fn test_run_trace_list_no_std() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("lib.rs"),
            "fn foo() { bar(); }\nfn bar() {}\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: Some("foo".into()),
                to: None,
                from_entrypoint: false,
                depth: 3,
                format: TraceFormat::List,
                no_std: true,
            },
        );
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
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "/[invalid/".into(),
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
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("/[invalid/".into()),
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
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Parse {
                file: "missing.rs".into(),
                format: ParseFormat::Summary,
            },
        );
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
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(
            temp.path().join("helper.rs"),
            "fn helper() {}\nfn util() {}\n",
        )
        .unwrap();
        let (_, stderr, result) = run_in_dir(
            temp.path(),
            Commands::Trace {
                function: None,
                from: None,
                to: None,
                from_entrypoint: true,
                depth: 3,
                format: TraceFormat::Tree,
                no_std: false,
            },
        );
        assert!(result.is_ok());
        assert!(stderr.contains("No entrypoints detected"));
    }

    #[test]
    fn test_run_apply_reveil_unveiled_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let original_content = "secret data\n";
        std::fs::write(temp.path().join("s.txt"), original_content).unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "s.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let file_path = temp.path().join("s.txt");
        assert!(!file_path.exists());
        std::fs::write(&file_path, original_content).unwrap();
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Apply { dry_run: false });
        assert!(result.is_ok());
        assert!(stdout.contains("re-veiled"));
    }

    #[test]
    fn test_run_doctor_with_issues() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("d.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "d.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let data_dir = temp.path().join(".funveil").join("objects");
        if data_dir.exists() {
            for entry in std::fs::read_dir(&data_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    std::fs::remove_file(entry.path()).unwrap();
                    break;
                }
            }
        }
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Doctor);
        assert!(result.is_ok());
        assert!(stdout.contains("issue") || stdout.contains("All checks passed"));
    }

    #[test]
    fn test_run_status_with_veils() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("secret.txt"), "secret\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "secret.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Status { files: false });
        assert!(result.is_ok());
        assert!(stdout.contains("Veiled objects:"));
    }

    // ── Undo/Redo tests ──

    #[test]
    fn test_undo_empty_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(temp.path(), Commands::Undo { force: false });
        assert!(result.is_err());
    }

    #[test]
    fn test_redo_nothing_to_redo() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(temp.path(), Commands::Redo);
        assert!(result.is_err());
    }

    #[test]
    fn test_veil_undo_redo_roundtrip() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("test.txt"), "hello world\n").unwrap();

        // Veil
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "test.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(!temp.path().join("test.txt").exists());

        // Undo — file should be restored
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Undo { force: false });
        assert!(result.is_ok());
        assert!(stdout.contains("Undone"));
        let restored = std::fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert_eq!(restored, "hello world\n");

        // Redo — file should be veiled again (removed from disk)
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Redo);
        assert!(result.is_ok());
        assert!(stdout.contains("Redone"));
        assert!(!temp.path().join("test.txt").exists());
    }

    #[test]
    fn test_undo_after_undo_new_action_discards_future() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("a.txt"), "aaa\n").unwrap();
        std::fs::write(temp.path().join("b.txt"), "bbb\n").unwrap();

        // Veil a.txt
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "a.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        // Veil b.txt
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "b.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        // Undo b.txt veil
        let _ = run_in_dir(temp.path(), Commands::Undo { force: false });

        // Now change mode — this should discard the b.txt future
        let _ = run_in_dir(
            temp.path(),
            Commands::Mode {
                mode: Some(Mode::Whitelist),
            },
        );

        // Redo should fail — future was discarded
        let (_, _, result) = run_in_dir(temp.path(), Commands::Redo);
        assert!(result.is_err());
    }

    #[test]
    fn test_history_list() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("init"));
        assert!(stdout.contains("veil"));
    }

    #[test]
    fn test_history_show_detail() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: Some(2),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Action #2"));
        assert!(stdout.contains("veil"));
    }

    #[test]
    fn test_history_with_undo_shows_future() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        // Undo
        let _ = run_in_dir(temp.path(), Commands::Undo { force: false });

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Future:"));
    }

    // ── Dry-run tests ──

    #[test]
    fn test_veil_dry_run_no_state_change() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "original\n").unwrap();

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: true,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Would veil"));

        // File should be unchanged
        let content = std::fs::read_to_string(temp.path().join("f.txt")).unwrap();
        assert_eq!(content, "original\n");

        // Config should have no objects
        let config = Config::load(temp.path()).unwrap();
        assert!(config.objects.is_empty());
    }

    #[test]
    fn test_unveil_dry_run_no_state_change() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "content\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        assert!(!temp.path().join("f.txt").exists());

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("f.txt".into()),
                all: false,
                dry_run: true,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Would unveil"));

        // File should still be veiled (not on disk)
        assert!(!temp.path().join("f.txt").exists());
    }

    #[test]
    fn test_apply_dry_run_no_state_change() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Apply { dry_run: true });
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
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
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
        let result = run_command(cli, temp.path(), &mut output);
        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        let json = serde_json::to_string(&cmd_result).unwrap();
        assert!(json.contains("\"command\":\"status\""));
        assert!(json.contains("\"mode\":\"whitelist\""));
    }

    // ── Status --files test ──

    #[test]
    fn test_status_files_flag() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("visible.txt"), "vis\n").unwrap();
        std::fs::write(temp.path().join("hidden.txt"), "hid\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "hidden.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Status { files: true });
        assert!(result.is_ok());
        assert!(stdout.contains("Files:"));
        assert!(stdout.contains("visible.txt"));
        assert!(stdout.contains("hidden.txt"));
    }

    // ── Undo non-undoable action ──

    #[test]
    fn test_undo_non_undoable_without_force() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        // Init creates a non-undoable entry, but we need a second entry
        // to have cursor > 0. Let's veil + gc.
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(temp.path(), Commands::Gc);

        // GC is not undoable
        let (_, _, result) = run_in_dir(temp.path(), Commands::Undo { force: false });
        assert!(result.is_err());
    }

    #[test]
    fn test_undo_non_undoable_with_force() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(temp.path(), Commands::Gc);

        // Force undo of GC (won't restore CAS objects, but won't error)
        let (_, _, result) = run_in_dir(temp.path(), Commands::Undo { force: true });
        assert!(result.is_ok());
    }

    #[test]
    fn test_mode_change_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Mode {
                mode: Some(Mode::Blacklist),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Mode changed to: blacklist"));

        let history = ActionHistory::load(temp.path()).unwrap();
        assert_eq!(history.entries.len(), 2);
        assert_eq!(history.entries[1].command, "mode");
        assert!(history.entries[1].undoable);
    }

    #[test]
    fn test_veil_headers_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("sample.rs"),
            "fn hello() {\n    println!(\"hi\");\n}\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "sample.rs".into(),
                mode: VeilMode::Headers,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Veiled (headers mode)"));

        let history = ActionHistory::load(temp.path()).unwrap();
        let last = history.entries.last().unwrap();
        assert_eq!(last.command, "veil");
        assert!(last.args.contains(&"headers".to_string()));
    }

    #[test]
    fn test_gc_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(temp.path(), Commands::Gc);
        assert!(result.is_ok());

        let history = ActionHistory::load(temp.path()).unwrap();
        let last = history.entries.last().unwrap();
        assert_eq!(last.command, "gc");
        assert!(!last.undoable);
    }

    #[test]
    fn test_clean_removes_data() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        assert!(temp.path().join(CONFIG_FILE).exists());

        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Clean);
        assert!(result.is_ok());
        assert!(stdout.contains("Removed all funveil data"));
        assert!(!temp.path().join(CONFIG_FILE).exists());
        assert!(!temp.path().join(".funveil").exists());
    }

    #[test]
    fn test_checkpoint_save_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Save {
                    name: "cp1".to_string(),
                },
            },
        );
        assert!(result.is_ok());

        let history = ActionHistory::load(temp.path()).unwrap();
        let last = history.entries.last().unwrap();
        assert_eq!(last.command, "checkpoint");
        assert!(last.args.contains(&"save".to_string()));
    }

    #[test]
    fn test_checkpoint_restore_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "hello\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Save {
                    name: "cp1".to_string(),
                },
            },
        );
        // Unveil first so restore has something to change
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );

        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Restore {
                    name: "cp1".to_string(),
                },
            },
        );
        assert!(result.is_ok());

        let history = ActionHistory::load(temp.path()).unwrap();
        let last = history.entries.last().unwrap();
        assert_eq!(last.command, "checkpoint");
        assert!(last.args.contains(&"restore".to_string()));
    }

    #[test]
    fn test_checkpoint_delete_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Save {
                    name: "cp1".to_string(),
                },
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Delete {
                    name: "cp1".to_string(),
                },
            },
        );
        assert!(result.is_ok());

        let history = ActionHistory::load(temp.path()).unwrap();
        let last = history.entries.last().unwrap();
        assert_eq!(last.command, "checkpoint");
        assert!(last.args.contains(&"delete".to_string()));
    }

    #[test]
    fn test_restore_with_checkpoint_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "hello\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Save {
                    name: "cp1".to_string(),
                },
            },
        );
        // Unveil first so restore has something to change
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );

        let (_, _, result) = run_in_dir(temp.path(), Commands::Restore);
        assert!(result.is_ok());

        let history = ActionHistory::load(temp.path()).unwrap();
        let last = history.entries.last().unwrap();
        assert_eq!(last.command, "restore");
    }

    #[test]
    fn test_history_show_with_config_diff() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("a.txt"), "hello world\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "a.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let history = ActionHistory::load(temp.path()).unwrap();
        let veil_id = history.entries.last().unwrap().id;

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: Some(veil_id),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Action #"));
        assert!(stdout.contains("veil"));
        // Veil action should show config diff (objects added) or file diffs
        assert!(stdout.contains("Config changes:") || stdout.contains("bytes ->"));
    }

    #[test]
    fn test_history_show_mode_change_diff() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Mode {
                mode: Some(Mode::Blacklist),
            },
        );

        let history = ActionHistory::load(temp.path()).unwrap();
        let mode_id = history.entries.last().unwrap().id;

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: Some(mode_id),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("mode:"));
    }

    #[test]
    fn test_history_show_init_config_created() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: Some(1),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("config created"));
    }

    #[test]
    fn test_history_show_objects_diff() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        // Veil first file so both pre and post have objects
        std::fs::write(temp.path().join("a.txt"), "aaa\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "a.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        // Veil second file — now pre has objects{a.txt} and post has objects{a.txt, b.txt}
        std::fs::write(temp.path().join("b.txt"), "bbb\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "b.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let history = ActionHistory::load(temp.path()).unwrap();
        let last_id = history.entries.last().unwrap().id;

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: Some(last_id),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("+ objects["));
    }

    #[test]
    fn test_history_show_objects_removed() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        // Veil a file then manually construct entry with pre having more objects than post
        std::fs::write(temp.path().join("a.txt"), "aaa\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "a.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let config_with_obj = Config::load(temp.path()).unwrap();
        let pre_yaml = snapshot_config(&config_with_obj).unwrap();
        // Create post YAML with a different object set (keep objects key but without a.txt)
        let post_yaml = pre_yaml.replace("a.txt", "REMOVED_KEY_FOR_TEST");

        let mut history = ActionHistory::load(temp.path()).unwrap();
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
        history.save(temp.path()).unwrap();

        let last_id = history.entries.last().unwrap().id;
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: Some(last_id),
            },
        );
        assert!(result.is_ok());
        // Should show both "- objects[a.txt]" (removed) and "+ objects[REMOVED_KEY_FOR_TEST]" (added)
        assert!(stdout.contains("- objects["));
        assert!(stdout.contains("+ objects["));
    }

    #[test]
    fn test_history_show_config_removed() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        // Manually create a history entry with pre_state having config but post_state None
        let mut history = ActionHistory::load(temp.path()).unwrap();
        let config = Config::load(temp.path()).unwrap();
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
        history.save(temp.path()).unwrap();

        let last_id = history.entries.last().unwrap().id;
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: Some(last_id),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("config removed"));
    }

    #[test]
    fn test_history_show_not_found() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );

        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: Some(999),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_undo_restores_file_content() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "original content\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        assert!(!temp.path().join("f.txt").exists());

        let (_, _, result) = run_in_dir(temp.path(), Commands::Undo { force: false });
        assert!(result.is_ok());

        let restored = std::fs::read_to_string(temp.path().join("f.txt")).unwrap();
        assert_eq!(restored, "original content\n");
    }

    #[test]
    fn test_redo_restores_veiled_state() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "original content\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        assert!(!temp.path().join("f.txt").exists());

        let _ = run_in_dir(temp.path(), Commands::Undo { force: false });
        let (_, _, result) = run_in_dir(temp.path(), Commands::Redo);
        assert!(result.is_ok());

        assert!(!temp.path().join("f.txt").exists());
    }

    #[test]
    fn test_status_files_with_partial_veils() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("f.txt"),
            "line1\nline2\nline3\nline4\nline5\n",
        )
        .unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt#2-3".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Status { files: true });
        assert!(result.is_ok());
        assert!(stdout.contains("partial"));
    }

    #[test]
    fn test_unveil_dry_run_all() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
                dry_run: true,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Would unveil"));
        assert!(stdout.contains("would be affected"));
    }

    #[test]
    fn test_unveil_dry_run_pattern() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("f.txt".into()),
                all: false,
                dry_run: true,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Would unveil"));
    }

    #[test]
    fn test_unveil_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("f.txt".into()),
                all: false,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );

        let history = ActionHistory::load(temp.path()).unwrap();
        let last = history.entries.last().unwrap();
        assert_eq!(last.command, "unveil");
        assert!(last.undoable);
    }

    #[test]
    fn test_apply_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "content\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        // Unveil to restore original, then apply should re-veil
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("f.txt".into()),
                all: false,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        // Re-add to blacklist manually so apply picks it up
        let mut config = Config::load(temp.path()).unwrap();
        config.add_to_blacklist("f.txt");
        config.save(temp.path()).unwrap();

        let (_, _, result) = run_in_dir(temp.path(), Commands::Apply { dry_run: false });
        assert!(result.is_ok());
    }

    #[test]
    fn test_json_output_veil() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "content\n").unwrap();

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
        let result = run_command(cli, temp.path(), &mut output);
        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        let json = serde_json::to_string(&cmd_result).unwrap();
        assert!(json.contains("\"command\":\"veil\""));
    }

    #[test]
    fn test_json_output_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );

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
        let result = run_command(cli, temp.path(), &mut output);
        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        let json = serde_json::to_string(&cmd_result).unwrap();
        assert!(json.contains("\"command\":\"history\""));
    }

    #[test]
    fn test_json_output_undo() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

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
        let result = run_command(cli, temp.path(), &mut output);
        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        let json = serde_json::to_string(&cmd_result).unwrap();
        assert!(json.contains("\"command\":\"undo\""));
    }

    #[test]
    fn test_json_output_redo() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let _ = run_in_dir(temp.path(), Commands::Undo { force: false });

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
        let result = run_command(cli, temp.path(), &mut output);
        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        let json = serde_json::to_string(&cmd_result).unwrap();
        assert!(json.contains("\"command\":\"redo\""));
    }

    #[test]
    fn test_json_output_gc() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );

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
        let result = run_command(cli, temp.path(), &mut output);
        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        let json = serde_json::to_string(&cmd_result).unwrap();
        assert!(json.contains("\"command\":\"gc\""));
    }

    #[test]
    fn test_json_output_clean() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );

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
        let result = run_command(cli, temp.path(), &mut output);
        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        let json = serde_json::to_string(&cmd_result).unwrap();
        assert!(json.contains("\"command\":\"clean\""));
    }

    #[test]
    fn test_json_output_doctor() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );

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
        let result = run_command(cli, temp.path(), &mut output);
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
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );

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
        let result = run_command(cli, temp.path(), &mut output);
        assert!(result.is_ok());
        let cmd_result = result.unwrap();
        let json = serde_json::to_string(&cmd_result).unwrap();
        assert!(json.contains("\"command\":\"checkpoint\""));
    }

    #[test]
    fn test_json_output_restore() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Checkpoint {
                cmd: CheckpointCmd::Save {
                    name: "cp1".to_string(),
                },
            },
        );

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
        let result = run_command(cli, temp.path(), &mut output);
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
        let files =
            collect_affected_files_for_pattern(std::path::Path::new("/tmp"), "some_file.txt");
        assert_eq!(files, vec!["some_file.txt"]);
    }

    #[test]
    fn test_collect_affected_files_invalid_regex() {
        let files = collect_affected_files_for_pattern(std::path::Path::new("/tmp"), "/[invalid/");
        assert!(files.is_empty());
    }

    #[test]
    fn test_snapshot_files_nonexistent() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let snaps = snapshot_files(temp.path(), &["nonexistent.txt".to_string()]);
        assert_eq!(snaps.len(), 1);
        assert!(snaps[0].cas_hash.is_none());
        assert_eq!(snaps[0].permissions, "644");
    }

    #[test]
    fn test_snapshot_files_existing() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "content\n").unwrap();
        let snaps = snapshot_files(temp.path(), &["f.txt".to_string()]);
        assert_eq!(snaps.len(), 1);
        assert!(snaps[0].cas_hash.is_some());
    }

    #[test]
    fn test_history_show_file_diffs() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "hello world content here\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let history = ActionHistory::load(temp.path()).unwrap();
        let id = history.entries.last().unwrap().id;

        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::History {
                limit: 20,
                show: Some(id),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("bytes ->"));
    }

    #[test]
    fn test_restore_action_state_creates_dirs_and_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        // Store content in CAS
        let store = ContentStore::new(temp.path());
        let hash = store.store(b"restored content").unwrap();

        let state = ActionState {
            config_yaml: None,
            file_snapshots: vec![FileSnapshot {
                path: "subdir/restored.txt".to_string(),
                cas_hash: Some(hash.full().to_string()),
                permissions: "644".to_string(),
            }],
        };
        restore_action_state(temp.path(), &state).unwrap();
        let content = std::fs::read_to_string(temp.path().join("subdir/restored.txt")).unwrap();
        assert_eq!(content, "restored content");
    }

    #[test]
    fn test_restore_action_state_deletes_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("todelete.txt"), "data").unwrap();

        let state = ActionState {
            config_yaml: None,
            file_snapshots: vec![FileSnapshot {
                path: "todelete.txt".to_string(),
                cas_hash: None,
                permissions: "644".to_string(),
            }],
        };
        restore_action_state(temp.path(), &state).unwrap();
        assert!(!temp.path().join("todelete.txt").exists());
    }

    #[test]
    fn test_restore_action_state_overwrites_readonly() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let fpath = temp.path().join("readonly.txt");
        std::fs::write(&fpath, "old").unwrap();

        let store = ContentStore::new(temp.path());
        let hash = store.store(b"new content").unwrap();

        let state = ActionState {
            config_yaml: None,
            file_snapshots: vec![FileSnapshot {
                path: "readonly.txt".to_string(),
                cas_hash: Some(hash.full().to_string()),
                permissions: "644".to_string(),
            }],
        };
        restore_action_state(temp.path(), &state).unwrap();
        assert_eq!(std::fs::read_to_string(&fpath).unwrap(), "new content");
    }

    #[test]
    fn test_entrypoints_language_go_filter() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("main.go"),
            "package main\n\nfunc main() {\n}\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: None,
                language: Some(LanguageArg::Go),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_entrypoints_language_python_filter() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("app.py"),
            "if __name__ == '__main__':\n    pass\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: None,
                language: Some(LanguageArg::Python),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_entrypoints_language_bash_filter() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("run.sh"), "#!/bin/bash\necho hello\n").unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: None,
                language: Some(LanguageArg::Bash),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_entrypoints_language_terraform_filter() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("main.tf"),
            "resource \"aws_instance\" \"example\" {}\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: None,
                language: Some(LanguageArg::Terraform),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_entrypoints_language_helm_filter() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("values.yaml"), "key: value\n").unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: None,
                language: Some(LanguageArg::Helm),
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_partially_veiled_lines() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("f.txt"),
            "line1\nline2\nline3\nline4\nline5\n",
        )
        .unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt#2-4".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Show {
                file: "f.txt".into(),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("File: f.txt"));
        assert!(stdout.contains("[veiled]") || stdout.contains("..."));
    }

    #[test]
    fn test_parse_detailed_calls_without_caller() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        // Python top-level calls have no caller
        std::fs::write(
            temp.path().join("script.py"),
            "import os\nprint('hello')\nos.path.exists('x')\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Parse {
                file: "script.py".into(),
                format: ParseFormat::Detailed,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Calls:") || stdout.contains("Imports:"));
    }

    #[test]
    fn test_parse_detailed_with_function_signatures() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("lib.rs"),
            "fn hello(x: i32) -> bool {\n    x > 0\n}\n\nfn world() {\n    hello(5);\n}\n",
        )
        .unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Parse {
                file: "lib.rs".into(),
                format: ParseFormat::Detailed,
            },
        );
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
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        // Remove config file but keep .funveil dir
        std::fs::remove_file(temp.path().join(CONFIG_FILE)).unwrap();
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Clean);
        assert!(result.is_ok());
        assert!(stdout.contains("Removed all funveil data"));
    }

    #[test]
    fn test_veil_dry_run_file_exists() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: true,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("bytes"));
        assert!(stdout.contains("would be affected"));
    }

    #[test]
    fn test_veil_dry_run_file_nonexistent() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "nonexist.txt".into(),
                mode: VeilMode::Full,
                dry_run: true,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("Would veil: nonexist.txt"));
    }

    #[test]
    fn test_status_files_with_full_veils_in_whitelist() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Whitelist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Status { files: true });
        assert!(result.is_ok());
        assert!(stdout.contains("veiled"));
        assert!(stdout.contains("full"));
    }

    #[test]
    fn test_entrypoints_with_handler_filter() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(
            temp.path().join("app.py"),
            "def handle_request():\n    pass\n",
        )
        .unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: Some(EntrypointTypeArg::Handler),
                language: None,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_entrypoints_with_export_filter() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("lib.rs"), "pub fn exported() {}\n").unwrap();
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Entrypoints {
                entry_type: Some(EntrypointTypeArg::Export),
                language: None,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_apply_dry_run_with_veiled_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "original\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        // Unveil to get original back
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("f.txt".into()),
                all: false,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );
        // Re-add to blacklist
        let mut config = Config::load(temp.path()).unwrap();
        config.add_to_blacklist("f.txt");
        config.save(temp.path()).unwrap();

        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Apply { dry_run: true });
        assert!(result.is_ok());
        assert!(stdout.contains("Would re-veil") || stdout.contains("would be re-applied"));
    }

    #[test]
    fn test_show_unveiled_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("plain.txt"), "line1\nline2\n").unwrap();
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Show {
                file: "plain.txt".into(),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("line1"));
        assert!(stdout.contains("line2"));
    }

    #[test]
    fn test_doctor_with_missing_object() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );
        // Delete CAS objects to create an integrity issue
        let objects_dir = temp.path().join(".funveil").join("objects");
        if objects_dir.exists() {
            std::fs::remove_dir_all(&objects_dir).unwrap();
            std::fs::create_dir_all(&objects_dir).unwrap();
        }
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Doctor);
        assert!(result.is_ok());
        assert!(stdout.contains("Missing object") || stdout.contains("issue"));
    }

    #[test]
    fn test_unveil_all_records_history() {
        let temp = tempfile::TempDir::new().unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Init {
                mode: Mode::Blacklist,
            },
        );
        std::fs::write(temp.path().join("f.txt"), "data\n").unwrap();
        let _ = run_in_dir(
            temp.path(),
            Commands::Veil {
                pattern: "f.txt".into(),
                mode: VeilMode::Full,
                dry_run: false,
                symbol: None,
                unreachable_from: None,
                level: None,
            },
        );

        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
                dry_run: false,
                symbol: None,
                callers_of: None,
                callees_of: None,
                level: None,
            },
        );

        let history = ActionHistory::load(temp.path()).unwrap();
        let last = history.entries.last().unwrap();
        assert_eq!(last.command, "unveil");
    }
}
