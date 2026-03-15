#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

use anyhow::Result;
use clap::Parser;
use funveil::{run_command, Cli, Commands, Output, CONFIG_FILE};
use std::env;
use std::path::PathBuf;

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
