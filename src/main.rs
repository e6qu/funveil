use anyhow::Result;
use clap::{Parser, Subcommand};
use funveil::{Config, Mode, CONFIG_FILE};
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "fv")]
#[command(about = "Funveil - Control file visibility in AI agent workspaces")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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

    /// Hide file, directory, or line range
    Veil {
        /// Pattern with optional line ranges
        pattern: String,
    },

    /// Reveal hidden content
    Unveil {
        /// Pattern with optional line ranges, or --all
        pattern: String,
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

    // Find project root (directory containing .git or .funveil_config, or current dir)
    let root = find_project_root()?;

    match cli.command {
        Commands::Init { mode } => {
            if Config::exists(&root) {
                println!("Funveil is already initialized in this directory.");
                return Ok(());
            }

            let config = Config::new(mode);
            config.save(&root)?;
            funveil::config::ensure_data_dir(&root)?;

            println!("Initialized funveil with {mode} mode.");
            println!("Configuration: {}", root.join(CONFIG_FILE).display());
        }

        Commands::Mode { mode } => {
            let mut config = Config::load(&root)?;

            if let Some(new_mode) = mode {
                config.set_mode(new_mode);
                config.save(&root)?;
                println!("Mode changed to: {new_mode}");
            } else {
                println!("Current mode: {}", config.mode());
            }
        }

        Commands::Status => {
            let config = Config::load(&root)?;
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

        Commands::Veil { pattern } => {
            println!("Veiling: {pattern}");
            // TODO: Implement veil logic
        }

        Commands::Unveil { pattern } => {
            if pattern == "--all" {
                println!("Unveiling all files...");
                // TODO: Implement unveil --all
            } else {
                println!("Unveiling: {pattern}");
                // TODO: Implement unveil logic
            }
        }

        Commands::Apply => {
            println!("Re-applying veils...");
            // TODO: Implement apply
        }

        Commands::Restore => {
            println!("Restoring previous state...");
            // TODO: Implement restore
        }

        Commands::Show { file } => {
            println!("Showing: {file}");
            // TODO: Implement show
        }

        Commands::Checkpoint { cmd } => {
            match cmd {
                CheckpointCmd::Save { name } => {
                    println!("Saving checkpoint: {name}");
                    // TODO: Implement checkpoint save
                }
                CheckpointCmd::Restore { name } => {
                    println!("Restoring checkpoint: {name}");
                    // TODO: Implement checkpoint restore
                }
                CheckpointCmd::List => {
                    println!("Checkpoints:");
                    // TODO: Implement checkpoint list
                }
                CheckpointCmd::Show { name } => {
                    println!("Checkpoint: {name}");
                    // TODO: Implement checkpoint show
                }
            }
        }

        Commands::Doctor => {
            println!("Checking veil integrity...");
            // TODO: Implement doctor
        }

        Commands::Gc => {
            println!("Running garbage collection...");
            // TODO: Implement gc
        }

        Commands::Clean => {
            println!("Removing all funveil data...");
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
