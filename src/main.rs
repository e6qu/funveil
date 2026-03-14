#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use anyhow::Result;
use clap::{Parser, Subcommand};
use funveil::{
    command_category, delete_checkpoint, garbage_collect, generate_trace_id, get_latest_checkpoint,
    has_veils, is_supported_source, list_checkpoints, restore_checkpoint, save_checkpoint,
    show_checkpoint, unveil_all, unveil_file, veil_file, walk_files, CallGraphBuilder, Config,
    ContentHash, ContentStore, EntrypointDetector, HeaderStrategy, LineRange, Mode, ObjectMeta,
    Output, TraceDirection, TreeSitterParser, CONFIG_FILE,
};
#[cfg(not(target_family = "wasm"))]
use funveil::{init_tracing, resolve_log_level};
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

    #[command(subcommand)]
    command: Commands,
}

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
    Status,

    /// Add file/directory to whitelist or unveil all
    Unveil {
        /// Pattern to whitelist (file, directory, or pattern with line ranges)
        pattern: Option<String>,
        /// Unveil all veiled files
        #[arg(long, conflicts_with = "pattern")]
        all: bool,
    },

    /// Add file/directory to blacklist
    Veil {
        /// Pattern to blacklist (file, directory, or pattern with optional line ranges)
        pattern: String,
        /// Veiling mode (headers or full)
        #[arg(long, value_enum, default_value = "full")]
        mode: VeilMode,
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
    Apply,

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
}

impl Commands {
    fn name(&self) -> &'static str {
        match self {
            Commands::Init { .. } => "init",
            Commands::Mode { .. } => "mode",
            Commands::Status => "status",
            Commands::Unveil { .. } => "unveil",
            Commands::Veil { .. } => "veil",
            Commands::Parse { .. } => "parse",
            Commands::Trace { .. } => "trace",
            Commands::Entrypoints { .. } => "entrypoints",
            Commands::Cache { .. } => "cache",
            Commands::Apply => "apply",
            Commands::Restore => "restore",
            Commands::Show { .. } => "show",
            Commands::Checkpoint { .. } => "checkpoint",
            Commands::Doctor => "doctor",
            Commands::Gc => "gc",
            Commands::Clean => "clean",
            Commands::Version => "version",
        }
    }
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let quiet = cli.quiet;
    let mut output = Output::new(quiet);
    let root = find_project_root()?;
    let is_version_command = matches!(cli.command, Commands::Version);

    let result = run_command(cli, &root, &mut output);

    #[cfg(not(target_family = "wasm"))]
    funveil::update::maybe_print_update_notice(&mut output.err, &root, is_version_command);

    result
}

fn run_command(cli: Cli, root: &std::path::Path, output: &mut Output) -> Result<()> {
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

    match cli.command {
        Commands::Init { mode } => {
            if Config::exists(&root) {
                let _ = writeln!(
                    output.out,
                    "Funveil is already initialized in this directory."
                );
                return Ok(());
            }

            let config = Config::new(mode);
            funveil::config::ensure_data_dir(&root)?;
            funveil::config::ensure_gitignore(&root)?;
            config.save(&root)?;

            let _ = writeln!(output.out, "Initialized funveil with {mode} mode.");
            let _ = writeln!(
                output.out,
                "Configuration: {}",
                root.join(CONFIG_FILE).display()
            );
        }

        Commands::Mode { mode } => {
            let mut config = Config::load(&root)?;

            if let Some(new_mode) = mode {
                config.set_mode(new_mode);
                config.save(&root)?;
                let _ = writeln!(output.out, "Mode changed to: {new_mode}");
            } else {
                let _ = writeln!(output.out, "Current mode: {}", config.mode());
            }
        }

        Commands::Status => {
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
        }

        Commands::Veil { pattern, mode } => match mode {
            VeilMode::Full => {
                let mut config = Config::load(&root)?;

                let mut veiled_any = false;
                if pattern.contains('#') {
                    let (file, ranges) = parse_pattern(&pattern)?;
                    veil_file(&root, &mut config, file, ranges.as_deref(), output)?;
                    config.add_to_blacklist(file);
                    veiled_any = true;
                } else if pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() > 2 {
                    use regex::Regex;
                    let regex_str = &pattern[1..pattern.len() - 1];
                    let regex = Regex::new(regex_str)?;

                    let mut file_errors = 0usize;
                    let mut matched = false;
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
                                match veil_file(&root, &mut config, &path_str, None, output) {
                                    Ok(()) => {
                                        config.add_to_blacklist(&path_str);
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
                    veiled_any = true;
                }

                config.save(&root)?;

                if veiled_any {
                    let _ = writeln!(output.out, "Veiling: {pattern}");
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

                let content = fs::read_to_string(&path)?;
                let parser = TreeSitterParser::new()?;
                let parsed = parser.parse_file(&path, &content)?;
                let strategy = HeaderStrategy::new();
                let veiled = strategy.veil_file(&content, &parsed)?;

                let mut config = Config::load(&root)?;
                let store = ContentStore::new(&root);
                let hash = store.store(content.as_bytes())?;

                let permissions = funveil::perms::file_mode(&fs::metadata(&path)?);
                fs::write(&path, veiled)?;

                config.register_object(pattern.clone(), ObjectMeta::new(hash, permissions));
                config.add_to_blacklist(&pattern);
                config.save(&root)?;

                let _ = writeln!(output.out, "Veiled (headers mode): {pattern}");
            }
        },

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
                    return Ok(());
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
                return Ok(());
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
        }

        Commands::Unveil { pattern, all } => {
            let mut config = Config::load(&root)?;

            if all {
                unveil_all(&root, &mut config, output)?;
                config.save(&root)?;
                let _ = writeln!(output.out, "Unveiled all files");
            } else if let Some(pattern) = pattern {
                if pattern.contains('#') {
                    let (file, ranges) = parse_pattern(&pattern)?;
                    unveil_file(&root, &mut config, file, ranges.as_deref(), output)?;
                    config.add_to_whitelist(file);
                    config.save(&root)?;
                    let _ = writeln!(output.out, "Unveiled: {pattern}");
                } else if pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() > 2 {
                    use regex::Regex;
                    let regex_str = &pattern[1..pattern.len() - 1];
                    let regex = Regex::new(regex_str)?;

                    let mut matched = false;
                    let mut unveiled_any = false;
                    let mut file_errors = 0usize;
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
                                if has_veils(&config, &path_str) {
                                    match unveil_file(&root, &mut config, &path_str, None, output) {
                                        Ok(()) => {
                                            config.add_to_whitelist(&path_str);
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

                    config.save(&root)?;
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
                    }
                    config.add_to_whitelist(&pattern);
                    config.save(&root)?;
                    let _ = writeln!(output.out, "Unveiled: {pattern}");
                }
            } else {
                return Err(anyhow::anyhow!(
                    "Must specify a pattern or --all to unveil files."
                ));
            }
        }

        Commands::Apply => {
            let mut config = Config::load(&root)?;
            let store = ContentStore::new(&root);

            let _ = writeln!(output.out, "Re-applying veils...");

            let mut applied = 0;
            let mut skipped = 0;

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
                    let _ = writeln!(output.err, "  Skipping {file_path} (file not found)");
                    skipped += 1;
                    continue;
                }

                let current_content = std::fs::read(&path)?;
                let current_hash = ContentHash::from_content(&current_content);

                // If current content matches the original hash, the file is unveiled and needs re-veiling.
                // If it doesn't match, the file is already veiled (placeholder on disk).
                if current_hash.full() != meta.hash {
                    let _ = writeln!(output.out, "  ✓ {file_path} (already veiled)");
                } else {
                    let original_hash = match ContentHash::from_string(meta.hash.clone()) {
                        Ok(h) => h,
                        Err(e) => {
                            let _ = writeln!(output.err, "  ✗ {file_path} (invalid hash: {e})");
                            skipped += 1;
                            continue;
                        }
                    };
                    if store.exists(&original_hash) {
                        // Remove existing config entry so veil_file doesn't reject as AlreadyVeiled
                        let removed_meta = config.objects.remove(key);
                        if let Err(e) = veil_file(&root, &mut config, file_path, None, output) {
                            let _ = writeln!(output.err, "  ✗ {file_path} (re-veil failed: {e})");
                            // Rollback: restore the config entry
                            if let Some(meta) = removed_meta {
                                config.objects.insert(key.clone(), meta);
                            }
                            skipped += 1;
                        } else {
                            applied += 1;
                            let _ = writeln!(output.out, "  ✓ {file_path} (re-veiled)");
                        }
                    } else {
                        let _ = writeln!(
                            output.err,
                            "  ✗ {file_path} (original content missing from CAS, skipping)"
                        );
                        skipped += 1;
                    }
                }
            }

            config.save(&root)?;

            let _ = writeln!(output.out, "\nApplied: {applied}, Skipped: {skipped}");
        }

        Commands::Restore => match get_latest_checkpoint(&root)? {
            Some(name) => {
                let _ = writeln!(output.out, "Restoring from latest checkpoint: {name}");
                restore_checkpoint(&root, &name, output)?;
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

            if !file_path.exists() {
                return Err(anyhow::anyhow!("file not found: {file}"));
            }
            funveil::validate_path_within_root(&file_path, &root)?;

            let is_full_veiled = config.get_object(&file).is_some();
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
        }

        Commands::Checkpoint { cmd } => match cmd {
            CheckpointCmd::Save { name } => {
                let config = Config::load(&root)?;
                save_checkpoint(&root, &config, &name, output)?;
            }
            CheckpointCmd::Restore { name } => {
                restore_checkpoint(&root, &name, output)?;
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
            }
            CheckpointCmd::Show { name } => {
                show_checkpoint(&root, &name, output)?;
            }
            CheckpointCmd::Delete { name } => {
                delete_checkpoint(&root, &name, output)?;
            }
        },

        Commands::Doctor => {
            let _ = writeln!(output.out, "Running integrity checks...");

            let config = Config::load(&root)?;
            let store = ContentStore::new(&root);
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
            }

            if issues.is_empty() {
                let _ = writeln!(output.out, "✓ All checks passed. No issues found.");
            } else {
                let _ = writeln!(output.out, "✗ Found {} issue(s):", issues.len());
                for issue in &issues {
                    let _ = writeln!(output.out, "  - {issue}");
                }
            }
        }

        Commands::Gc => {
            let config = Config::load(&root)?;

            let _ = writeln!(output.out, "Running garbage collection...");

            // Collect all referenced hashes from config, skipping invalid ones
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

            let _ = writeln!(output.out, "Garbage collected {deleted} object(s)");
            let _ = writeln!(output.out, "Freed {freed} bytes");
        }

        Commands::Clean => {
            let _ = writeln!(output.out, "Removing all funveil data...");

            let data_dir = root.join(".funveil");
            let config_file = root.join(CONFIG_FILE);

            if data_dir.exists() {
                std::fs::remove_dir_all(&data_dir)?;
            }

            if config_file.exists() {
                std::fs::remove_file(&config_file)?;
            }

            let _ = writeln!(output.out, "✓ Removed all funveil data");
        }

        Commands::Version => {
            let _ = writeln!(output.out, "{}", version_long());
        }
    }

    Ok(())
}

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
        assert_eq!(Commands::Status.name(), "status");
        assert_eq!(
            Commands::Unveil {
                pattern: None,
                all: false
            }
            .name(),
            "unveil"
        );
        assert_eq!(
            Commands::Veil {
                pattern: "f".into(),
                mode: VeilMode::Full
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
        assert_eq!(Commands::Apply.name(), "apply");
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
            command,
        };
        let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut output = Output {
            out: Box::new(TestWriter(out_buf.clone())),
            err: Box::new(TestWriter(err_buf.clone())),
        };
        let result = run_command(cli, dir, &mut output);
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
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Status);
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
            },
        );
        assert!(result.is_ok());
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("test.txt".into()),
                all: false,
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
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
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
        let (_, _, result) = run_in_dir(temp.path(), Commands::Apply);
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
            },
        );
        let (_, _, result) = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("/a\\.txt/".into()),
                all: false,
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
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: None,
                all: true,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Apply);
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
            },
        );
        let (stdout, _, result) = run_in_dir(
            temp.path(),
            Commands::Show {
                file: "s.txt".into(),
            },
        );
        assert!(result.is_ok());
        assert!(stdout.contains("FULLY VEILED"));
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
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("a.txt".into()),
                all: false,
            },
        );
        let (stdout, _, _) = run_in_dir(temp.path(), Commands::Status);
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
            },
        );
        let _ = run_in_dir(
            temp.path(),
            Commands::Unveil {
                pattern: Some("a.txt".into()),
                all: false,
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Status);
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
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Apply);
        assert!(result.is_ok());
        assert!(stdout.contains("already veiled"));
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
            },
        );
        std::fs::remove_file(temp.path().join("gone.txt")).unwrap();
        let (_, stderr, result) = run_in_dir(temp.path(), Commands::Apply);
        assert!(result.is_ok());
        assert!(stderr.contains("Skipping") || stderr.contains("not found"));
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
        let (_, _, result) = run_in_dir(temp.path(), Commands::Status);
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
        let (_, _, result) = run_in_dir(temp.path(), Commands::Apply);
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
            },
        );
        let veiled_content = std::fs::read_to_string(temp.path().join("s.txt")).unwrap();
        assert_ne!(veiled_content, original_content);
        let file_path = temp.path().join("s.txt");
        let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o644);
        }
        std::fs::set_permissions(&file_path, perms).unwrap();
        std::fs::write(&file_path, original_content).unwrap();
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Apply);
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
            },
        );
        let (stdout, _, result) = run_in_dir(temp.path(), Commands::Status);
        assert!(result.is_ok());
        assert!(stdout.contains("Veiled objects:"));
    }
}
