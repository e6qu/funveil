use anyhow::Result;
use clap::{Parser, Subcommand};
use funveil::{
    delete_checkpoint, garbage_collect, get_latest_checkpoint, has_veils, list_checkpoints,
    restore_checkpoint, save_checkpoint, show_checkpoint, unveil_all, unveil_file, veil_file,
    CallGraphBuilder, Config, ContentHash, ContentStore, EntrypointDetector, HeaderStrategy,
    LineRange, Mode, ObjectMeta, TraceDirection, TreeSitterParser, CONFIG_FILE,
};
use ignore::WalkBuilder;
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fv")]
#[command(about = "Funveil - Control file visibility in AI agent workspaces")]
struct Cli {
    /// Suppress output
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
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

    // Find project root (directory containing .git or .funveil_config, or current dir)
    let root = find_project_root()?;

    match cli.command {
        Commands::Init { mode } => {
            if Config::exists(&root) {
                if !quiet {
                    println!("Funveil is already initialized in this directory.");
                }
                return Ok(());
            }

            let config = Config::new(mode);
            config.save(&root)?;
            funveil::config::ensure_data_dir(&root)?;
            funveil::config::ensure_gitignore(&root)?;

            if !quiet {
                println!("Initialized funveil with {mode} mode.");
                println!("Configuration: {}", root.join(CONFIG_FILE).display());
            }
        }

        Commands::Mode { mode } => {
            let mut config = Config::load(&root)?;

            if let Some(new_mode) = mode {
                config.set_mode(new_mode);
                config.save(&root)?;
                if !quiet {
                    println!("Mode changed to: {new_mode}");
                }
            } else if !quiet {
                println!("Current mode: {}", config.mode());
            }
        }

        Commands::Status => {
            let config = Config::load(&root)?;
            if !quiet {
                println!("Mode: {}", config.mode());

                if !config.blacklist.is_empty() {
                    println!("\nBlacklisted:");
                    for entry in &config.blacklist {
                        println!("  - {entry}");
                    }
                }

                if !config.whitelist.is_empty() {
                    println!("\nWhitelisted:");
                    for entry in &config.whitelist {
                        println!("  - {entry}");
                    }
                }

                if !config.objects.is_empty() {
                    println!("\nVeiled objects: {}", config.objects.len());
                }
            }
        }

        Commands::Veil { pattern, mode } => {
            match mode {
                VeilMode::Full => {
                    let mut config = Config::load(&root)?;

                    // Check if pattern has line ranges
                    let mut veiled_any = false;
                    if pattern.contains('#') {
                        let (file, ranges) = parse_pattern(&pattern)?;
                        veil_file(&root, &mut config, file, ranges.as_deref(), quiet)?;
                        // BUG-112: Add to blacklist after successful veil (same as literal/regex paths)
                        config.add_to_blacklist(file);
                        veiled_any = true;
                    } else if pattern.starts_with('/')
                        && pattern.ends_with('/')
                        && pattern.len() > 2
                    {
                        // Regex pattern: /pattern/
                        use regex::Regex;
                        let regex_str = &pattern[1..pattern.len() - 1];
                        let regex = Regex::new(regex_str)?;

                        // Find all matching files
                        let mut file_errors = 0usize;
                        let mut matched = false;
                        for entry in WalkBuilder::new(&root)
                            .max_depth(Some(10))
                            .hidden(false)
                            .git_ignore(true)
                            .git_global(false)
                            .git_exclude(false)
                            .require_git(false)
                            .build()
                            .filter_map(|e| e.ok())
                        {
                            let path = entry.path();
                            if path.is_file() {
                                let relative_path = path.strip_prefix(&root).unwrap_or(path);
                                let path_str = relative_path.to_string_lossy();
                                if regex.is_match(&path_str) {
                                    match veil_file(&root, &mut config, &path_str, None, quiet) {
                                        Ok(()) => {
                                            config.add_to_blacklist(&path_str);
                                            veiled_any = true;
                                        }
                                        Err(e) => {
                                            if !quiet {
                                                eprintln!(
                                                    "Warning: failed to veil {path_str}: {e}"
                                                );
                                            }
                                            file_errors += 1;
                                        }
                                    }
                                    matched = true;
                                }
                            }
                        }

                        if !matched && !quiet {
                            println!("No files matched pattern: {pattern}");
                        }
                        if file_errors > 0 && !quiet {
                            eprintln!("Warning: {file_errors} files could not be veiled.");
                        }
                    } else {
                        // Veil the file first, then add to blacklist only on success
                        veil_file(&root, &mut config, &pattern, None, quiet)?;
                        config.add_to_blacklist(&pattern);
                        veiled_any = true;
                    }

                    config.save(&root)?;

                    if veiled_any && !quiet {
                        println!("Veiling: {pattern}");
                    }
                }
                VeilMode::Headers => {
                    // Header mode: parse and show only signatures
                    use funveil::{TreeSitterParser, VeilStrategy};
                    use std::fs;

                    let path = root.join(&pattern);
                    if !path.exists() {
                        return Err(anyhow::anyhow!("File not found: {pattern}"));
                    }

                    let content = fs::read_to_string(&path)?;
                    let parser = TreeSitterParser::new()?;
                    let parsed = parser.parse_file(&path, &content)?;
                    let strategy = HeaderStrategy::new();
                    let veiled = strategy.veil_file(&content, &parsed)?;

                    // Store original content in CAS before overwriting
                    let mut config = Config::load(&root)?;
                    let store = ContentStore::new(&root);
                    let hash = store.store(content.as_bytes())?;

                    let permissions = {
                        use std::os::unix::fs::PermissionsExt;
                        fs::metadata(&path)?.permissions().mode()
                    };
                    // BUG-108: Write file before registering config to avoid inconsistency on write failure
                    fs::write(&path, veiled)?;

                    config.register_object(pattern.clone(), ObjectMeta::new(hash, permissions));
                    config.add_to_blacklist(&pattern);
                    config.save(&root)?;

                    if !quiet {
                        println!("Veiled (headers mode): {pattern}");
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

            if !quiet {
                match format {
                    ParseFormat::Summary => {
                        println!("File: {}", path.display());
                        println!("Language: {}", parsed.language);
                        println!("Functions: {}", parsed.functions().count());
                        println!("Classes: {}", parsed.classes().count());
                        println!("Imports: {}", parsed.imports.len());
                        println!("Calls: {}", parsed.calls.len());
                    }
                    ParseFormat::Detailed => {
                        println!("File: {}", path.display());
                        println!("Language: {}\n", parsed.language);

                        if !parsed.symbols.is_empty() {
                            println!("Symbols:");
                            for symbol in &parsed.symbols {
                                println!(
                                    "  - {} (lines {}-{})",
                                    symbol.name(),
                                    symbol.line_range().start(),
                                    symbol.line_range().end()
                                );
                                if let funveil::parser::Symbol::Function { .. } = symbol {
                                    println!("    Signature: {}", symbol.signature());
                                }
                            }
                        }

                        if !parsed.imports.is_empty() {
                            println!("\nImports:");
                            for import in &parsed.imports {
                                println!("  - {}", import.path);
                            }
                        }

                        if !parsed.calls.is_empty() {
                            println!("\nCalls:");
                            for call in &parsed.calls {
                                if let Some(ref caller) = call.caller {
                                    println!(
                                        "  - {} -> {} (line {})",
                                        caller, call.callee, call.line
                                    );
                                } else {
                                    println!("  - {} (line {})", call.callee, call.line);
                                }
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
            // Parse all source files in the project
            let mut parsed_files = Vec::new();
            let parser = TreeSitterParser::new()?;

            for entry in WalkBuilder::new(&root)
                .hidden(false)
                .git_ignore(true)
                .git_global(false)
                .git_exclude(false)
                .require_git(false)
                .build()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
            {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());

                // Only parse supported source files
                // Include: rs, go, ts, tsx, js, jsx, py, sh, bash, tf, hcl, yaml, yml,
                // html, css, xml, md, zig
                if matches!(
                    ext,
                    Some("rs")
                        | Some("go")
                        | Some("ts")
                        | Some("tsx")
                        | Some("js")
                        | Some("jsx")
                        | Some("py")
                        | Some("sh")
                        | Some("bash")
                        | Some("tf")
                        | Some("tfvars")
                        | Some("hcl")
                        | Some("yaml")
                        | Some("yml")
                        | Some("html")
                        | Some("htm")
                        | Some("css")
                        | Some("xml")
                        | Some("md")
                        | Some("zig")
                ) {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Ok(parsed) = parser.parse_file(path, &content) {
                            parsed_files.push(parsed);
                        }
                    }
                }
            }

            // Build the call graph
            let mut graph = CallGraphBuilder::from_files(&parsed_files);

            if from_entrypoint {
                // Trace from all detected entrypoints
                let entrypoints = EntrypointDetector::detect_all(&parsed_files);

                if entrypoints.is_empty() {
                    if !quiet {
                        eprintln!("No entrypoints detected in the codebase");
                    }
                    return Ok(());
                }

                if !quiet {
                    eprintln!(
                        "Tracing from {} detected entrypoints (max depth: {})...",
                        entrypoints.len(),
                        depth
                    );
                }

                let mut all_functions = std::collections::HashSet::new();

                for ep in &entrypoints {
                    if let Some(result) = graph.trace(&ep.name, TraceDirection::Forward, depth) {
                        for func in result.all_functions() {
                            all_functions.insert(func.name.clone());
                        }
                    }
                }

                if !quiet {
                    println!("\nEntrypoints found: {}", entrypoints.len());
                    println!(
                        "Functions reachable from entrypoints: {}",
                        all_functions.len()
                    );
                    println!("\nReachable functions:");
                    for func in &all_functions {
                        println!("  - {func}");
                    }
                }
            } else {
                // Determine the target function and direction
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

                if !quiet {
                    eprintln!("Tracing {direction} from '{target}' (max depth: {depth})...");
                }

                if !graph.contains(&target) && !quiet {
                    eprintln!("Warning: Function '{target}' not found in call graph");
                    eprintln!("Available functions: {}", graph.function_count());
                    // Continue anyway - might be an external function
                }

                match format {
                    TraceFormat::Dot => {
                        // Filter out std functions if requested
                        if no_std {
                            graph.filter_std_functions();
                        }
                        // Output the entire graph in DOT format
                        if !quiet {
                            println!("{}", graph.to_dot());
                        }
                    }
                    TraceFormat::Tree | TraceFormat::List => {
                        // Trace from/to the target function
                        if let Some(mut result) = graph.trace(&target, direction, depth) {
                            // Filter out std functions if requested
                            if no_std {
                                result.filter_std();
                            }

                            let output = match format {
                                TraceFormat::Tree => result.format_tree(),
                                TraceFormat::List => result.format_list(),
                                _ => unreachable!(),
                            };
                            if !quiet {
                                println!("{output}");
                            }

                            if result.cycle_detected && !quiet {
                                eprintln!("\nNote: Cycle detected in call graph");
                            }
                        } else if !quiet {
                            eprintln!("Function '{target}' not found in the codebase");
                        }
                    }
                }
            }
        }

        Commands::Entrypoints {
            entry_type,
            language,
        } => {
            // Parse all source files
            let mut parsed_files = Vec::new();
            let parser = TreeSitterParser::new()?;

            for entry in WalkBuilder::new(&root)
                .hidden(false)
                .git_ignore(true)
                .git_global(false)
                .git_exclude(false)
                .require_git(false)
                .build()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
            {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());

                // Filter by language if specified
                // Supported extensions: rs, go, ts, tsx, py, sh, bash, tf, hcl, yaml, yml,
                // html, css, xml, md, zig, js, jsx
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
                        | (
                            None,
                            Some("rs")
                                | Some("go")
                                | Some("ts")
                                | Some("tsx")
                                | Some("js")
                                | Some("jsx")
                                | Some("py")
                                | Some("sh")
                                | Some("bash")
                                | Some("tf")
                                | Some("tfvars")
                                | Some("hcl")
                                | Some("yaml")
                                | Some("yml")
                                | Some("html")
                                | Some("htm")
                                | Some("css")
                                | Some("xml")
                                | Some("md")
                                | Some("zig")
                        )
                );

                if should_parse {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Ok(parsed) = parser.parse_file(path, &content) {
                            parsed_files.push(parsed);
                        }
                    }
                }
            }

            // Detect entrypoints
            let entrypoints = EntrypointDetector::detect_all(&parsed_files);

            if entrypoints.is_empty() {
                if !quiet {
                    println!("No entrypoints detected");
                }
                return Ok(());
            }

            // Filter by type if specified
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

            // Group by language for display
            let grouped = EntrypointDetector::group_refs_by_language(&filtered);

            if !quiet {
                for (lang, eps) in grouped {
                    println!("\n[{lang}]");
                    for ep in eps {
                        let desc = ep
                            .description
                            .as_ref()
                            .map(|d| format!(" - {d}"))
                            .unwrap_or_default();
                        println!(
                            "  {} ({}){} - {}:{}",
                            ep.name,
                            ep.entry_type,
                            desc,
                            ep.file.display(),
                            ep.line
                        );
                    }
                }

                println!("\nTotal: {} entrypoints", filtered.len());
            }
        }

        Commands::Cache { cmd } => {
            use funveil::AnalysisCache;

            match cmd {
                CacheCmd::Status => {
                    let cache = AnalysisCache::load(&root)?;
                    let stats = cache.stats();
                    if !quiet {
                        println!("{stats}");
                    }
                }
                CacheCmd::Clear => {
                    let mut cache = AnalysisCache::load(&root)?;
                    cache.clear();
                    cache.save(&root)?;
                    if !quiet {
                        println!("Cache cleared");
                    }
                }
                CacheCmd::Invalidate => {
                    let mut cache = AnalysisCache::load(&root)?;
                    cache.invalidate_stale();
                    cache.save(&root)?;
                    if !quiet {
                        println!("Stale cache entries invalidated");
                    }
                }
            }
        }

        Commands::Unveil { pattern, all } => {
            let mut config = Config::load(&root)?;

            if all {
                unveil_all(&root, &mut config, quiet)?;
                config.save(&root)?;
                if !quiet {
                    println!("Unveiled all files");
                }
            } else if let Some(pattern) = pattern {
                if pattern.contains('#') {
                    // Partial unveil with line ranges
                    let (file, ranges) = parse_pattern(&pattern)?;
                    unveil_file(&root, &mut config, file, ranges.as_deref(), quiet)?;
                    config.add_to_whitelist(file);
                    config.save(&root)?;
                    if !quiet {
                        println!("Unveiled: {pattern}");
                    }
                } else if pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() > 2 {
                    // Regex pattern: /pattern/
                    use regex::Regex;
                    let regex_str = &pattern[1..pattern.len() - 1];
                    let regex = Regex::new(regex_str)?;

                    // Find all matching files
                    let mut matched = false;
                    let mut unveiled_any = false;
                    let mut file_errors = 0usize;
                    for entry in WalkBuilder::new(&root)
                        .max_depth(Some(10))
                        .hidden(false)
                        .git_ignore(true)
                        .git_global(false)
                        .git_exclude(false)
                        .require_git(false)
                        .build()
                        .filter_map(|e| e.ok())
                    {
                        let path = entry.path();
                        if path.is_file() {
                            let relative_path = path.strip_prefix(&root).unwrap_or(path);
                            let path_str = relative_path.to_string_lossy();
                            if regex.is_match(&path_str) {
                                if has_veils(&config, &path_str) {
                                    match unveil_file(&root, &mut config, &path_str, None, quiet) {
                                        Ok(()) => {
                                            config.add_to_whitelist(&path_str);
                                            // BUG-113: Only set unveiled_any on actual success
                                            unveiled_any = true;
                                        }
                                        Err(e) => {
                                            if !quiet {
                                                eprintln!(
                                                    "Warning: failed to unveil {path_str}: {e}"
                                                );
                                            }
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
                    if !matched && !quiet {
                        println!("No files matched pattern: {pattern}");
                    } else if unveiled_any && !quiet {
                        println!("Unveiled: {pattern}");
                    } else if matched && !unveiled_any && !quiet {
                        println!("No veiled files matched pattern: {pattern}");
                    }
                    if file_errors > 0 && !quiet {
                        eprintln!("Warning: {file_errors} files could not be unveiled.");
                    }
                } else {
                    if has_veils(&config, &pattern) {
                        unveil_file(&root, &mut config, &pattern, None, quiet)?;
                    }
                    config.add_to_whitelist(&pattern);
                    config.save(&root)?;
                    if !quiet {
                        println!("Unveiled: {pattern}");
                    }
                }
            } else {
                // BUG-151 fix: no pattern and no --all is a usage error, not a match failure
                eprintln!("Must specify a pattern or --all to unveil files.");
                std::process::exit(1);
            }
        }

        Commands::Apply => {
            let mut config = Config::load(&root)?;
            let store = ContentStore::new(&root);

            if !quiet {
                println!("Re-applying veils...");
            }

            let mut applied = 0;
            let mut skipped = 0;

            let entries: Vec<_> = config
                .objects
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            for (key, meta) in &entries {
                // BUG-099: Use rfind('#') with suffix validation for filenames containing '#'
                let file_path = if let Some(pos) = key.rfind('#') {
                    let suffix = &key[pos + 1..];
                    if suffix == "_original" || suffix.parse::<LineRange>().is_ok() {
                        &key[..pos]
                    } else {
                        key.as_str()
                    }
                } else {
                    key.as_str()
                };

                let path = root.join(file_path);

                if !path.exists() {
                    if !quiet {
                        eprintln!("  Skipping {file_path} (file not found)");
                    }
                    skipped += 1;
                    continue;
                }

                // Read current content
                let current_content = std::fs::read(&path)?;
                let current_hash = ContentHash::from_content(&current_content);

                // Check if content matches expected hash
                // If current content matches the original hash, the file is unveiled and needs re-veiling.
                // If it doesn't match, the file is already veiled (placeholder on disk).
                if current_hash.full() != meta.hash {
                    if !quiet {
                        println!("  ✓ {file_path} (already veiled)");
                    }
                } else {
                    // File has been modified — re-veil using the original stored content
                    let original_hash = match ContentHash::from_string(meta.hash.clone()) {
                        Ok(h) => h,
                        Err(e) => {
                            if !quiet {
                                eprintln!("  ✗ {file_path} (invalid hash: {e})");
                            }
                            skipped += 1;
                            continue;
                        }
                    };
                    if store.exists(&original_hash) {
                        // Original is safe in CAS; just re-veil the file
                        // Remove existing config entry so veil_file doesn't reject as AlreadyVeiled
                        let removed_meta = config.objects.remove(key);
                        if let Err(e) = veil_file(&root, &mut config, file_path, None, quiet) {
                            if !quiet {
                                eprintln!("  ✗ {file_path} (re-veil failed: {e})");
                            }
                            // Rollback: restore the config entry
                            if let Some(meta) = removed_meta {
                                config.objects.insert(key.clone(), meta);
                            }
                            skipped += 1;
                        } else {
                            applied += 1;
                            if !quiet {
                                println!("  ✓ {file_path} (re-veiled)");
                            }
                        }
                    } else {
                        // Original not in CAS — cannot verify content authenticity
                        if !quiet {
                            eprintln!(
                                "  ✗ {file_path} (original content missing from CAS, skipping)"
                            );
                        }
                        skipped += 1;
                    }
                }
            }

            config.save(&root)?;

            if !quiet {
                println!("\nApplied: {applied}, Skipped: {skipped}");
            }
        }

        Commands::Restore => match get_latest_checkpoint(&root)? {
            Some(name) => {
                if !quiet {
                    println!("Restoring from latest checkpoint: {name}");
                }
                restore_checkpoint(&root, &name, quiet)?;
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

            // BUG-117: Validate file existence even when quiet
            if !file_path.exists() {
                return Err(anyhow::anyhow!("file not found: {file}"));
            }

            // Check if file is veiled
            let is_full_veiled = config.get_object(&file).is_some();
            let partial_ranges = config.veiled_ranges(&file)?;

            if !quiet {
                if is_full_veiled {
                    println!("File: {file} [FULLY VEILED]");
                    println!("Content is veiled. Use 'fv unveil {file}' to view.");
                } else if !partial_ranges.is_empty() {
                    // Read and display with annotations
                    let content = std::fs::read_to_string(&file_path)?;
                    let lines: Vec<&str> = content.lines().collect();

                    println!("File: {file}");
                    let marker_re = regex::Regex::new(r"^\.\.\.\[[a-f0-9]{7}\]").unwrap();
                    for (i, line) in lines.iter().enumerate() {
                        let line_num = i + 1;
                        // Check if this line is veiled
                        let mut is_veiled = false;
                        if let Ok(veiled) = config.is_veiled(&file, line_num) {
                            is_veiled = veiled;
                        }

                        if marker_re.is_match(line) {
                            // Already veiled marker
                            println!("{line_num:4} | [veiled] {line}");
                        } else if is_veiled {
                            println!("{line_num:4} | [veiled] ...");
                        } else {
                            println!("{line_num:4} | {line}");
                        }
                    }
                } else {
                    // Not veiled, just show
                    let content = std::fs::read_to_string(&file_path)?;
                    println!("File: {file}");
                    for (i, line) in content.lines().enumerate() {
                        println!("{:4} | {}", i + 1, line);
                    }
                }
            }
        }

        Commands::Checkpoint { cmd } => match cmd {
            CheckpointCmd::Save { name } => {
                let config = Config::load(&root)?;
                save_checkpoint(&root, &config, &name, quiet)?;
            }
            CheckpointCmd::Restore { name } => {
                restore_checkpoint(&root, &name, quiet)?;
            }
            CheckpointCmd::List => {
                let checkpoints = list_checkpoints(&root)?;
                if checkpoints.is_empty() {
                    if !quiet {
                        println!("No checkpoints found.");
                    }
                } else if !quiet {
                    println!("Checkpoints:");
                    for cp in checkpoints {
                        println!("  - {cp}");
                    }
                }
            }
            CheckpointCmd::Show { name } => {
                show_checkpoint(&root, &name, quiet)?;
            }
            CheckpointCmd::Delete { name } => {
                delete_checkpoint(&root, &name, quiet)?;
            }
        },

        Commands::Doctor => {
            if !quiet {
                println!("Running integrity checks...");
            }

            let config = Config::load(&root)?;
            let store = ContentStore::new(&root);
            let mut issues = Vec::new();

            // Check all objects exist
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

            if !quiet {
                if issues.is_empty() {
                    println!("✓ All checks passed. No issues found.");
                } else {
                    println!("✗ Found {} issue(s):", issues.len());
                    for issue in &issues {
                        println!("  - {issue}");
                    }
                }
            }
        }

        Commands::Gc => {
            let config = Config::load(&root)?;

            if !quiet {
                println!("Running garbage collection...");
            }

            // Collect all referenced hashes from config, skipping invalid ones
            let mut referenced: Vec<ContentHash> = Vec::new();
            for (key, meta) in &config.objects {
                match ContentHash::from_string(meta.hash.clone()) {
                    Ok(h) => referenced.push(h),
                    Err(e) => {
                        if !quiet {
                            eprintln!("Warning: skipping invalid hash for {key}: {e}");
                        }
                    }
                }
            }

            let (deleted, freed) = garbage_collect(&root, &referenced, quiet)?;

            if !quiet {
                println!("Garbage collected {deleted} object(s)");
                println!("Freed {freed} bytes");
            }
        }

        Commands::Clean => {
            if !quiet {
                println!("Removing all funveil data...");
            }

            let data_dir = root.join(".funveil");
            let config_file = root.join(CONFIG_FILE);

            if data_dir.exists() {
                std::fs::remove_dir_all(&data_dir)?;
            }

            if config_file.exists() {
                std::fs::remove_file(&config_file)?;
            }

            if !quiet {
                println!("✓ Removed all funveil data");
            }
        }
    }

    Ok(())
}

fn find_project_root() -> Result<PathBuf> {
    let current = env::current_dir()?;

    // Check for .funveil_config
    if current.join(CONFIG_FILE).exists() {
        return Ok(current);
    }

    // Check for .git
    if current.join(".git").exists() {
        return Ok(current);
    }

    // Check parent directories
    let mut path = current.as_path();
    while let Some(parent) = path.parent() {
        if parent.join(CONFIG_FILE).exists() || parent.join(".git").exists() {
            return Ok(parent.to_path_buf());
        }
        path = parent;
    }

    // Default to current directory
    Ok(current)
}

/// Parse a pattern like "file.txt" or "file.txt#1-5" into (file, optional_ranges)
fn parse_pattern(pattern: &str) -> Result<(&str, Option<Vec<LineRange>>)> {
    // BUG-107: Use rfind('#') and validate suffix is a parseable range spec
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
}
