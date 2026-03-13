use std::process::Command;

fn main() {
    // Git commit hash
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Git dirty flag
    let git_dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| if o.stdout.is_empty() { "" } else { "-dirty" })
        .unwrap_or("");

    // Rerun if the release tag changes
    println!("cargo:rerun-if-env-changed=FV_RELEASE_TAG");

    // Version: prefer FV_RELEASE_TAG (set by CI from git tag), fall back to Cargo.toml
    let version = std::env::var("FV_RELEASE_TAG")
        .ok()
        .map(|t| t.strip_prefix('v').unwrap_or(&t).to_string())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());

    println!("cargo:rustc-env=FV_VERSION={version}");
    println!("cargo:rustc-env=FV_GIT_SHA={git_hash}{git_dirty}");
    println!(
        "cargo:rustc-env=FV_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    println!(
        "cargo:rustc-env=FV_BUILD_PROFILE={}",
        std::env::var("PROFILE").unwrap_or_default()
    );
}
