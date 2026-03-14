use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_FILE: &str = "update_check.json";
const CHECK_TTL_SECS: i64 = 86_400;
const HTTP_TIMEOUT_SECS: u64 = 3;
const GITHUB_API_URL: &str = "https://api.github.com/repos/e6qu/funveil/releases/latest";

#[derive(Serialize, Deserialize)]
struct UpdateCache {
    last_check_epoch: i64,
    latest_version: String,
    release_url: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

pub fn maybe_print_update_notice(err: &mut dyn Write, project_root: &Path, force: bool) {
    let _ = check_and_notify(err, project_root, force);
}

fn check_and_notify(err: &mut dyn Write, project_root: &Path, force: bool) -> Option<()> {
    if std::env::var("FV_NO_UPDATE_CHECK").ok().as_deref() == Some("1") {
        return Some(());
    }

    let data_dir = project_root.join(crate::config::DATA_DIR);
    if !data_dir.is_dir() {
        return Some(());
    }

    let cache_path = data_dir.join(CACHE_FILE);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;

    let cache = match read_cache(&cache_path) {
        Some(c) if (now - c.last_check_epoch) < CHECK_TTL_SECS => c,
        _ => {
            let release = fetch_latest_release()?;
            let version = release
                .tag_name
                .strip_prefix('v')
                .unwrap_or(&release.tag_name)
                .to_string();
            let cache = UpdateCache {
                last_check_epoch: now,
                latest_version: version,
                release_url: release.html_url,
            };
            write_cache(&cache_path, &cache);
            cache
        }
    };

    let current = env!("FV_VERSION");
    if !is_newer(&cache.latest_version, current) {
        return Some(());
    }

    if !force {
        // In non-force mode, only show notice if cache was already present
        // (i.e., don't show on first fetch — let it appear next run)
    }

    let target = env!("FV_BUILD_TARGET");
    let _ = writeln!(
        err,
        "Update available: fv {} (current: {})",
        cache.latest_version, current
    );
    let _ = writeln!(err, "Release: {}", cache.release_url);
    if let Some(url) = download_url(&cache.latest_version, target) {
        let _ = writeln!(err, "Download: {}", url);
    }
    let _ = writeln!(err);
    let _ = writeln!(err, "Set FV_NO_UPDATE_CHECK=1 to disable this check.");

    Some(())
}

fn fetch_latest_release() -> Option<GitHubRelease> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_global(Some(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS)))
            .build(),
    );
    let body = agent
        .get(GITHUB_API_URL)
        .header("User-Agent", &format!("fv/{}", env!("FV_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .ok()?
        .into_body()
        .read_to_string()
        .ok()?;
    serde_json::from_str(&body).ok()
}

fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |s: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ))
    };
    match (parse(remote), parse(current)) {
        (Some(r), Some(c)) => r > c,
        _ => false,
    }
}

fn asset_name_for_target(target: &str) -> Option<&'static str> {
    match target {
        "x86_64-unknown-linux-gnu" => Some("fv-linux-amd64.tar.gz"),
        "aarch64-unknown-linux-gnu" => Some("fv-linux-arm64.tar.gz"),
        "x86_64-apple-darwin" => Some("fv-darwin-amd64.tar.gz"),
        "aarch64-apple-darwin" => Some("fv-darwin-arm64.tar.gz"),
        "wasm32-wasip2" => Some("fv-wasm.tar.gz"),
        _ => None,
    }
}

fn download_url(version: &str, target: &str) -> Option<String> {
    let asset = asset_name_for_target(target)?;
    Some(format!(
        "https://github.com/e6qu/funveil/releases/download/v{version}/{asset}"
    ))
}

fn read_cache(path: &Path) -> Option<UpdateCache> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_cache(path: &Path, cache: &UpdateCache) {
    if let Ok(data) = serde_json::to_string(cache) {
        let _ = std::fs::write(path, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.3.0"));
    }

    #[test]
    fn test_is_newer_malformed() {
        assert!(!is_newer("abc", "0.2.0"));
        assert!(!is_newer("0.2.0", "xyz"));
        assert!(!is_newer("", ""));
        assert!(!is_newer("1.0", "0.9"));
        assert!(!is_newer("1.0.0.0", "0.9.0"));
    }

    #[test]
    fn test_asset_name_for_target() {
        assert_eq!(
            asset_name_for_target("x86_64-unknown-linux-gnu"),
            Some("fv-linux-amd64.tar.gz")
        );
        assert_eq!(
            asset_name_for_target("aarch64-unknown-linux-gnu"),
            Some("fv-linux-arm64.tar.gz")
        );
        assert_eq!(
            asset_name_for_target("x86_64-apple-darwin"),
            Some("fv-darwin-amd64.tar.gz")
        );
        assert_eq!(
            asset_name_for_target("aarch64-apple-darwin"),
            Some("fv-darwin-arm64.tar.gz")
        );
        assert_eq!(
            asset_name_for_target("wasm32-wasip2"),
            Some("fv-wasm.tar.gz")
        );
        assert_eq!(asset_name_for_target("unknown-target"), None);
    }

    #[test]
    fn test_download_url() {
        assert_eq!(
            download_url("0.3.0", "x86_64-unknown-linux-gnu"),
            Some(
                "https://github.com/e6qu/funveil/releases/download/v0.3.0/fv-linux-amd64.tar.gz"
                    .to_string()
            )
        );
        assert_eq!(
            download_url("0.3.0", "aarch64-apple-darwin"),
            Some(
                "https://github.com/e6qu/funveil/releases/download/v0.3.0/fv-darwin-arm64.tar.gz"
                    .to_string()
            )
        );
        assert_eq!(download_url("0.3.0", "unknown-target"), None);
    }

    #[test]
    fn test_cache_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("cache.json");
        let cache = UpdateCache {
            last_check_epoch: 1234567890,
            latest_version: "0.3.0".to_string(),
            release_url: "https://github.com/e6qu/funveil/releases/tag/v0.3.0".to_string(),
        };
        write_cache(&path, &cache);
        let loaded = read_cache(&path).unwrap();
        assert_eq!(loaded.last_check_epoch, 1234567890);
        assert_eq!(loaded.latest_version, "0.3.0");
        assert_eq!(
            loaded.release_url,
            "https://github.com/e6qu/funveil/releases/tag/v0.3.0"
        );
    }

    #[test]
    fn test_read_cache_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        assert!(read_cache(&path).is_none());
    }

    #[test]
    fn test_read_cache_corrupted() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not json at all!!!").unwrap();
        assert!(read_cache(&path).is_none());
    }

    #[test]
    fn test_notice_output() {
        let dir = tempfile::TempDir::new().unwrap();
        let data_dir = dir.path().join(".funveil");
        std::fs::create_dir(&data_dir).unwrap();
        let cache = UpdateCache {
            last_check_epoch: i64::MAX / 2, // far future so TTL is fresh
            latest_version: "99.0.0".to_string(),
            release_url: "https://github.com/e6qu/funveil/releases/tag/v99.0.0".to_string(),
        };
        write_cache(&data_dir.join(CACHE_FILE), &cache);

        let mut buf = Vec::new();
        maybe_print_update_notice(&mut buf, dir.path(), false);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Update available: fv 99.0.0"));
        assert!(output.contains("Release: https://github.com/e6qu/funveil/releases/tag/v99.0.0"));
        assert!(output.contains("FV_NO_UPDATE_CHECK=1"));
    }

    #[test]
    fn test_skipped_when_no_data_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        // No .funveil directory
        let mut buf = Vec::new();
        maybe_print_update_notice(&mut buf, dir.path(), false);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_skipped_with_env_var() {
        let dir = tempfile::TempDir::new().unwrap();
        let data_dir = dir.path().join(".funveil");
        std::fs::create_dir(&data_dir).unwrap();
        let cache = UpdateCache {
            last_check_epoch: i64::MAX / 2,
            latest_version: "99.0.0".to_string(),
            release_url: "https://github.com/e6qu/funveil/releases/tag/v99.0.0".to_string(),
        };
        write_cache(&data_dir.join(CACHE_FILE), &cache);

        // Set env var to disable update check
        std::env::set_var("FV_NO_UPDATE_CHECK", "1");
        let mut buf = Vec::new();
        maybe_print_update_notice(&mut buf, dir.path(), false);
        // Clean up env var before asserting (in case of test failure)
        std::env::remove_var("FV_NO_UPDATE_CHECK");
        assert!(buf.is_empty());
    }
}
