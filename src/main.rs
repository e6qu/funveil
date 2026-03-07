use anyhow::Result;
use clap::{Parser, Subcommand};
use funveil::{
    is_veiled, unveil_all, unveil_file, veil_file, Config, ContentHash, ContentStore,
    HeaderStrategy, LineRange, Mode, CONFIG_FILE,
};
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
enum CheckpointCmd {
    /// Save current state
    Save { name: String },
    /// Restore saved state
    Restore { name: String },
    /// List all checkpoints
    List,
    /// Show checkpoint details
    Show { name: String },
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
                    if pattern.contains('#') {
                        let (file, ranges) = parse_pattern(&pattern)?;
                        veil_file(&root, &mut config, file, ranges.as_deref())?;
                    } else {
                        // Add to blacklist
                        config.add_to_blacklist(&pattern);
                        // Also immediately veil the file
                        veil_file(&root, &mut config, &pattern, None)?;
                    }

                    config.save(&root)?;

                    if !quiet {
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

                    // Write veiled content back
                    fs::write(&path, veiled)?;

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
                                println!("  - {} -> {} (line {})", caller, call.callee, call.line);
                            } else {
                                println!("  - {} (line {})", call.callee, call.line);
                            }
                        }
                    }
                }
            }
        }

        Commands::Unveil { pattern, all } => {
            let mut config = Config::load(&root)?;

            if all {
                unveil_all(&root, &mut config)?;
                config.save(&root)?;
                if !quiet {
                    println!("Unveiled all files");
                }
            } else if let Some(pattern) = pattern {
                if pattern.contains('#') {
                    // Partial unveil with line ranges
                    let (file, ranges) = parse_pattern(&pattern)?;
                    unveil_file(&root, &mut config, file, ranges.as_deref())?;
                    config.save(&root)?;
                    if !quiet {
                        println!("Unveiled: {pattern}");
                    }
                } else {
                    // Add to whitelist
                    config.add_to_whitelist(&pattern);
                    // Also immediately unveil the file if it was veiled
                    if is_veiled(&config, &pattern) {
                        unveil_file(&root, &mut config, &pattern, None)?;
                    }
                    config.save(&root)?;
                    if !quiet {
                        println!("Unveiled: {pattern}");
                    }
                }
            }
        }

        Commands::Apply => {
            if !quiet {
                println!("Re-applying veils...");
            }
            // TODO: Implement apply
        }

        Commands::Restore => {
            if !quiet {
                println!("Restoring previous state...");
            }
            // TODO: Implement restore
        }

        Commands::Show { file } => {
            let config = Config::load(&root)?;
            let file_path = root.join(&file);

            // Check if file is veiled
            let is_full_veiled = config.get_object(&file).is_some();
            let partial_ranges = config.veiled_ranges(&file)?;

            if is_full_veiled {
                println!("File: {file} [FULLY VEILED]");
                println!("Content is veiled. Use 'fv unveil {file}' to view.");
            } else if !partial_ranges.is_empty() {
                // Read and display with annotations
                let content = std::fs::read_to_string(&file_path)?;
                let lines: Vec<&str> = content.lines().collect();

                println!("File: {file}");
                for (i, line) in lines.iter().enumerate() {
                    let line_num = i + 1;
                    // Check if this line is veiled
                    let mut is_veiled = false;
                    if let Ok(veiled) = config.is_veiled(&file, line_num) {
                        is_veiled = veiled;
                    }

                    if line.contains("...[") && line.contains("]") {
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

        Commands::Checkpoint { cmd } => {
            match cmd {
                CheckpointCmd::Save { name } => {
                    if !quiet {
                        println!("Saving checkpoint: {name}");
                    }
                    // TODO: Implement checkpoint save
                }
                CheckpointCmd::Restore { name } => {
                    if !quiet {
                        println!("Restoring checkpoint: {name}");
                    }
                    // TODO: Implement checkpoint restore
                }
                CheckpointCmd::List => {
                    if !quiet {
                        println!("Checkpoints:");
                    }
                    // TODO: Implement checkpoint list
                }
                CheckpointCmd::Show { name } => {
                    if !quiet {
                        println!("Checkpoint: {name}");
                    }
                    // TODO: Implement checkpoint show
                }
            }
        }

        Commands::Doctor => {
            if !quiet {
                println!("Running integrity checks...");
            }

            let config = Config::load(&root)?;
            let store = ContentStore::new(&root);
            let mut issues = Vec::new();

            // Check all objects exist
            for (key, meta) in &config.objects {
                let hash = ContentHash::from_string(meta.hash.clone());
                if store.retrieve(&hash).is_err() {
                    issues.push(format!("Missing object: {key}"));
                }
            }

            if issues.is_empty() {
                println!("✓ All checks passed. No issues found.");
            } else {
                println!("✗ Found {} issue(s):", issues.len());
                for issue in &issues {
                    println!("  - {issue}");
                }
            }
        }

        Commands::Gc => {
            if !quiet {
                println!("Running garbage collection...");
            }
            // TODO: Implement gc
        }

        Commands::Clean => {
            if !quiet {
                println!("Removing all funveil data...");
            }
            // TODO: Implement clean
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
    if let Some(pos) = pattern.find('#') {
        let file = &pattern[..pos];
        let range_str = &pattern[pos + 1..];

        // Parse range like "1-5"
        let parts: Vec<&str> = range_str.split('-').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!("Invalid range format: expected start-end"));
        }
        let start = parts[0].parse::<usize>()?;
        let end = parts[1].parse::<usize>()?;
        let range = LineRange::new(start, end)?;
        Ok((file, Some(vec![range])))
    } else {
        Ok((pattern, None))
    }
}
