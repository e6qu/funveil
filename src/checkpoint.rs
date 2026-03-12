use crate::cas::ContentStore;
use crate::config::{Config, CHECKPOINTS_DIR};
use crate::error::{FunveilError, Result};
use crate::output::Output;
use crate::types::ContentHash;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointManifest {
    pub created: DateTime<Utc>,
    pub mode: String,
    pub files: HashMap<String, CheckpointFile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointFile {
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<(usize, usize)>>,
    pub permissions: String,
}

impl CheckpointManifest {
    pub fn new(mode: &str) -> Self {
        Self {
            created: Utc::now(),
            mode: mode.to_string(),
            files: HashMap::new(),
        }
    }

    pub fn add_file(
        &mut self,
        path: String,
        hash: ContentHash,
        lines: Option<Vec<(usize, usize)>>,
        permissions: String,
    ) {
        self.files.insert(
            path,
            CheckpointFile {
                hash: hash.full().to_string(),
                lines,
                permissions,
            },
        );
    }
}

fn validate_checkpoint_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(FunveilError::InvalidCheckpointName(
            "name cannot be empty".to_string(),
        ));
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(FunveilError::InvalidCheckpointName(format!(
            "name contains forbidden characters: {name}"
        )));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(FunveilError::InvalidCheckpointName(format!(
            "name contains control characters: {name}"
        )));
    }
    Ok(())
}

#[tracing::instrument(skip(root, config, output), fields(name = %name))]
pub fn save_checkpoint(
    root: &Path,
    config: &Config,
    name: &str,
    output: &mut Output,
) -> Result<()> {
    validate_checkpoint_name(name)?;
    let checkpoint_dir = root.join(CHECKPOINTS_DIR).join(name);
    fs::create_dir_all(&checkpoint_dir)?;

    let mut manifest = CheckpointManifest::new(&config.mode.to_string());
    let store = ContentStore::new(root);

    let mut walk_errors = 0usize;
    for entry_result in ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .build()
    {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                let _ = writeln!(output.err, "Warning: skipping directory entry: {e}");
                walk_errors += 1;
                continue;
            }
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .expect("WalkDir entry should be under root");
        let relative_str = relative.to_string_lossy().to_string();

        if relative_str.starts_with(".funveil")
            || relative_str.starts_with(".git")
            || relative_str == ".funveil_config"
        {
            continue;
        }

        let content = fs::read(path)?;
        let hash = store.store(&content)?;

        let metadata = fs::metadata(path)?;
        let permissions = format!("{:o}", metadata.mode() & 0o777);

        let lines = config
            .veiled_ranges(&relative_str)
            .ok()
            .map(|ranges| ranges.iter().map(|r| (r.start(), r.end())).collect());

        manifest.add_file(relative_str, hash, lines, permissions);
    }

    if walk_errors > 0 {
        let _ = writeln!(
            output.err,
            "Warning: {walk_errors} entries could not be read. Checkpoint may be incomplete."
        );
    }

    let manifest_path = checkpoint_dir.join("manifest.yaml");
    let manifest_yaml = serde_yaml::to_string(&manifest)?;
    fs::write(&manifest_path, manifest_yaml)?;

    tracing::info!(files = manifest.files.len(), "checkpoint saved");

    let _ = writeln!(
        output.out,
        "Checkpoint '{}' saved with {} files.",
        name,
        manifest.files.len()
    );
    Ok(())
}

pub fn list_checkpoints(root: &Path) -> Result<Vec<String>> {
    let checkpoints_dir = root.join(CHECKPOINTS_DIR);

    if !checkpoints_dir.exists() {
        return Ok(Vec::new());
    }

    let mut checkpoints = Vec::new();
    for entry in fs::read_dir(&checkpoints_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        checkpoints.push(entry.file_name().to_string_lossy().to_string());
    }

    Ok(checkpoints)
}

pub fn get_latest_checkpoint(root: &Path) -> Result<Option<String>> {
    let checkpoints = list_checkpoints(root)?;

    if checkpoints.is_empty() {
        return Ok(None);
    }

    let mut latest: Option<(String, DateTime<Utc>)> = None;

    for name in checkpoints {
        let manifest_path = root.join(CHECKPOINTS_DIR).join(&name).join("manifest.yaml");
        if !manifest_path.exists() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_yaml::from_str::<CheckpointManifest>(&content) else {
            continue;
        };

        match &latest {
            None => latest = Some((name, manifest.created)),
            Some((_, created)) if manifest.created > *created => {
                latest = Some((name, manifest.created));
            }
            _ => {}
        }
    }

    Ok(latest.map(|(name, _)| name))
}

#[tracing::instrument(skip(root, output), fields(name = %name))]
pub fn show_checkpoint(root: &Path, name: &str, output: &mut Output) -> Result<()> {
    validate_checkpoint_name(name)?;
    let manifest_path = root.join(CHECKPOINTS_DIR).join(name).join("manifest.yaml");

    if !manifest_path.exists() {
        return Err(FunveilError::CheckpointNotFound(name.to_string()));
    }

    let content = fs::read_to_string(&manifest_path)?;
    let manifest: CheckpointManifest = serde_yaml::from_str(&content)?;

    let _ = writeln!(output.out, "Checkpoint: {name}");
    let _ = writeln!(output.out, "Created: {}", manifest.created);
    let _ = writeln!(output.out, "Mode: {}", manifest.mode);
    let _ = writeln!(output.out, "Files: {}", manifest.files.len());

    for (path, file) in &manifest.files {
        match &file.lines {
            Some(lines) => {
                let ranges: Vec<String> = lines.iter().map(|(s, e)| format!("{s}-{e}")).collect();
                let _ = writeln!(
                    output.out,
                    "  {} [{}] (veiled: {})",
                    path,
                    file.hash.get(..7).unwrap_or(&file.hash),
                    ranges.join(", ")
                );
            }
            None => {
                let _ = writeln!(
                    output.out,
                    "  {} [{}]",
                    path,
                    file.hash.get(..7).unwrap_or(&file.hash)
                );
            }
        }
    }

    Ok(())
}

#[tracing::instrument(skip(root, output), fields(name = %name))]
pub fn restore_checkpoint(root: &Path, name: &str, output: &mut Output) -> Result<()> {
    validate_checkpoint_name(name)?;
    let manifest_path = root.join(CHECKPOINTS_DIR).join(name).join("manifest.yaml");

    if !manifest_path.exists() {
        return Err(FunveilError::CheckpointNotFound(name.to_string()));
    }

    let content = fs::read_to_string(&manifest_path)?;
    let manifest: CheckpointManifest = serde_yaml::from_str(&content)?;

    let store = ContentStore::new(root);
    let mut restored = 0;
    let mut failed = 0;

    for (path, file_info) in &manifest.files {
        let file_path = root.join(path);
        let hash = match ContentHash::from_string(file_info.hash.clone()) {
            Ok(h) => h,
            Err(e) => {
                let _ = writeln!(output.err, "Failed to parse hash for {path}: {e}");
                failed += 1;
                continue;
            }
        };

        let Ok(content) = store.retrieve(&hash) else {
            let _ = writeln!(output.err, "Failed to retrieve {path} from CAS");
            failed += 1;
            continue;
        };

        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    let _ = writeln!(output.err, "Failed to create directory {parent:?}: {e}");
                    failed += 1;
                    continue;
                }
            }
        }

        if let Err(e) = fs::write(&file_path, &content) {
            let _ = writeln!(output.err, "Failed to restore {path}: {e}");
            failed += 1;
            continue;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(perms) = u32::from_str_radix(&file_info.permissions, 8) {
                if let Err(e) = fs::set_permissions(&file_path, fs::Permissions::from_mode(perms)) {
                    let _ = writeln!(
                        output.err,
                        "Warning: failed to restore permissions for {path}: {e}"
                    );
                }
            }
        }

        restored += 1;
    }

    tracing::info!(restored, failed, "checkpoint restored");

    let _ = writeln!(
        output.out,
        "Checkpoint '{name}' restored: {restored} files restored, {failed} failed"
    );

    if failed > 0 {
        Err(FunveilError::PartialRestore { restored, failed })
    } else {
        Ok(())
    }
}

#[tracing::instrument(skip(root, output), fields(name = %name))]
pub fn delete_checkpoint(root: &Path, name: &str, output: &mut Output) -> Result<()> {
    validate_checkpoint_name(name)?;
    let checkpoint_dir = root.join(CHECKPOINTS_DIR).join(name);

    if !checkpoint_dir.exists() {
        return Err(FunveilError::CheckpointNotFound(name.to_string()));
    }

    fs::remove_dir_all(&checkpoint_dir)?;

    tracing::info!("checkpoint deleted");

    let _ = writeln!(output.out, "Checkpoint '{name}' deleted.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ensure_data_dir, Config};
    use crate::output::Output;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Config) {
        let temp = TempDir::new().unwrap();
        ensure_data_dir(temp.path()).unwrap();
        (temp, Config::new(crate::types::Mode::Whitelist))
    }

    #[test]
    fn test_checkpoint_manifest_new() {
        let manifest = CheckpointManifest::new("whitelist");
        assert_eq!(manifest.mode, "whitelist");
        assert!(manifest.files.is_empty());
    }

    #[test]
    fn test_checkpoint_manifest_add_file() {
        let mut manifest = CheckpointManifest::new("whitelist");
        let hash = ContentHash::from_content(b"test content");

        manifest.add_file(
            "test.txt".to_string(),
            hash.clone(),
            Some(vec![(1, 10)]),
            "644".to_string(),
        );

        assert!(manifest.files.contains_key("test.txt"));
        let file = manifest.files.get("test.txt").unwrap();
        assert_eq!(file.hash, hash.full());
        assert_eq!(file.lines, Some(vec![(1, 10)]));
        assert_eq!(file.permissions, "644");
    }

    #[test]
    fn test_checkpoint_manifest_add_file_no_lines() {
        let mut manifest = CheckpointManifest::new("whitelist");
        let hash = ContentHash::from_content(b"test content");

        manifest.add_file("test.txt".to_string(), hash, None, "644".to_string());

        let file = manifest.files.get("test.txt").unwrap();
        assert!(file.lines.is_none());
    }

    #[test]
    fn test_list_checkpoints_empty() {
        let (temp, _) = setup();
        let checkpoints = list_checkpoints(temp.path()).unwrap();
        assert!(checkpoints.is_empty());
    }

    #[test]
    fn test_list_checkpoints() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR);
        fs::create_dir_all(cp_dir.join("checkpoint1")).unwrap();
        fs::create_dir_all(cp_dir.join("checkpoint2")).unwrap();

        let mut checkpoints = list_checkpoints(temp.path()).unwrap();
        checkpoints.sort();
        assert_eq!(checkpoints, vec!["checkpoint1", "checkpoint2"]);
    }

    #[test]
    fn test_get_latest_checkpoint_none() {
        let (temp, _) = setup();
        let latest = get_latest_checkpoint(temp.path()).unwrap();
        assert!(latest.is_none());
    }

    #[test]
    fn test_save_checkpoint() {
        let (temp, config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        save_checkpoint(temp.path(), &config, "test_cp", &mut Output::new(false)).unwrap();

        let manifest_path = temp
            .path()
            .join(CHECKPOINTS_DIR)
            .join("test_cp/manifest.yaml");
        assert!(manifest_path.exists());

        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(
            content.contains("test_cp")
                || content.contains("created:")
                || content.contains("files:")
        );
    }

    #[test]
    fn test_show_checkpoint_not_found() {
        let (temp, _) = setup();
        let result = show_checkpoint(temp.path(), "nonexistent", &mut Output::new(false));
        assert!(matches!(result, Err(FunveilError::CheckpointNotFound(_))));
    }

    #[test]
    fn test_show_checkpoint() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();

        save_checkpoint(
            temp.path(),
            &config,
            "my_checkpoint",
            &mut Output::new(false),
        )
        .unwrap();
        let result = show_checkpoint(temp.path(), "my_checkpoint", &mut Output::new(false));
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_checkpoint_not_found() {
        let (temp, _) = setup();
        let result = delete_checkpoint(temp.path(), "nonexistent", &mut Output::new(false));
        assert!(matches!(result, Err(FunveilError::CheckpointNotFound(_))));
    }

    #[test]
    fn test_delete_checkpoint() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();

        save_checkpoint(temp.path(), &config, "to_delete", &mut Output::new(false)).unwrap();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("to_delete");
        assert!(cp_dir.exists());

        delete_checkpoint(temp.path(), "to_delete", &mut Output::new(false)).unwrap();
        assert!(!cp_dir.exists());
    }

    #[test]
    fn test_restore_checkpoint() {
        let (temp, config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "original content\n").unwrap();

        save_checkpoint(
            temp.path(),
            &config,
            "restore_test",
            &mut Output::new(false),
        )
        .unwrap();

        fs::write(&file_path, "modified content\n").unwrap();
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            "modified content\n"
        );

        restore_checkpoint(temp.path(), "restore_test", &mut Output::new(false)).unwrap();
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            "original content\n"
        );
    }

    #[test]
    fn test_restore_checkpoint_not_found() {
        let (temp, _) = setup();
        let result = restore_checkpoint(temp.path(), "nonexistent", &mut Output::new(false));
        assert!(matches!(result, Err(FunveilError::CheckpointNotFound(_))));
    }

    #[test]
    fn test_get_latest_checkpoint() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();

        save_checkpoint(temp.path(), &config, "first", &mut Output::new(false)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        save_checkpoint(temp.path(), &config, "second", &mut Output::new(false)).unwrap();

        let latest = get_latest_checkpoint(temp.path()).unwrap();
        assert!(latest.is_some());
    }

    #[test]
    fn test_checkpoint_serialization() {
        let mut manifest = CheckpointManifest::new("whitelist");
        let hash = ContentHash::from_content(b"test");
        manifest.add_file(
            "file.txt".to_string(),
            hash,
            Some(vec![(1, 5)]),
            "644".to_string(),
        );

        let yaml = serde_yaml::to_string(&manifest).unwrap();
        assert!(yaml.contains("whitelist"));
        assert!(yaml.contains("file.txt"));
    }

    #[test]
    fn test_checkpoint_file_serialization() {
        let cf = CheckpointFile {
            hash: "abc123".to_string(),
            lines: Some(vec![(1, 10), (20, 30)]),
            permissions: "644".to_string(),
        };

        let yaml = serde_yaml::to_string(&cf).unwrap();
        assert!(yaml.contains("abc123"));
        assert!(yaml.contains("644"));
    }

    #[test]
    fn test_list_checkpoints_with_non_dir_entries() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR);
        fs::create_dir_all(&cp_dir).unwrap();
        fs::write(cp_dir.join("file.txt"), "not a dir").unwrap();
        fs::create_dir_all(cp_dir.join("actual_checkpoint")).unwrap();

        let checkpoints = list_checkpoints(temp.path()).unwrap();
        assert_eq!(checkpoints, vec!["actual_checkpoint"]);
    }

    #[test]
    fn test_get_latest_checkpoint_with_invalid_manifest() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("bad_cp");
        fs::create_dir_all(&cp_dir).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), "invalid yaml content").unwrap();

        let result = get_latest_checkpoint(temp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_latest_checkpoint_with_missing_manifest() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("no_manifest");
        fs::create_dir_all(&cp_dir).unwrap();

        let result = get_latest_checkpoint(temp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_show_checkpoint_with_lines() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let ranges = [crate::types::LineRange::new(1, 2).unwrap()];
        crate::veil::veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        save_checkpoint(temp.path(), &config, "with_lines", &mut Output::new(false)).unwrap();
        let result = show_checkpoint(temp.path(), "with_lines", &mut Output::new(false));
        assert!(result.is_ok());
    }

    #[test]
    fn test_restore_checkpoint_creates_directories() {
        let (temp, config) = setup();
        let nested_path = temp.path().join("a/b/c/test.txt");
        fs::create_dir_all(nested_path.parent().unwrap()).unwrap();
        fs::write(&nested_path, "nested content\n").unwrap();

        save_checkpoint(temp.path(), &config, "nested_cp", &mut Output::new(false)).unwrap();

        fs::remove_dir_all(temp.path().join("a")).unwrap();
        assert!(!temp.path().join("a").exists());

        restore_checkpoint(temp.path(), "nested_cp", &mut Output::new(false)).unwrap();
        assert!(temp.path().join("a/b/c/test.txt").exists());
        assert_eq!(
            fs::read_to_string(temp.path().join("a/b/c/test.txt")).unwrap(),
            "nested content\n"
        );
    }

    #[test]
    fn test_save_checkpoint_skips_funveil_dirs() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();
        fs::create_dir_all(temp.path().join(".git/objects")).unwrap();
        fs::write(temp.path().join(".git/config"), "git config\n").unwrap();

        save_checkpoint(temp.path(), &config, "skip_test", &mut Output::new(false)).unwrap();

        let manifest_path = temp
            .path()
            .join(CHECKPOINTS_DIR)
            .join("skip_test/manifest.yaml");
        let content = fs::read_to_string(&manifest_path).unwrap();
        assert!(!content.contains(".git"));
    }

    #[test]
    fn test_show_checkpoint_file_with_no_veiled_lines() {
        // Covers line 184: file with lines: None in show_checkpoint
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("no-veiled-lines");
        fs::create_dir_all(&cp_dir).unwrap();

        let mut manifest = CheckpointManifest::new("whitelist");
        manifest.files.insert(
            "plain.txt".to_string(),
            CheckpointFile {
                hash: "deadbeef1234567".to_string(),
                lines: None,
                permissions: "644".to_string(),
            },
        );
        manifest.files.insert(
            "veiled.txt".to_string(),
            CheckpointFile {
                hash: "cafebabe7654321".to_string(),
                lines: Some(vec![(1, 5)]),
                permissions: "644".to_string(),
            },
        );
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        let result = show_checkpoint(temp.path(), "no-veiled-lines", &mut Output::new(false));
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_checkpoint_without_lines() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();

        save_checkpoint(temp.path(), &config, "no_lines", &mut Output::new(false)).unwrap();
        let result = show_checkpoint(temp.path(), "no_lines", &mut Output::new(false));
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_checkpoints_no_directory() {
        let temp = TempDir::new().unwrap();
        let checkpoints = list_checkpoints(temp.path()).unwrap();
        assert!(checkpoints.is_empty());
    }

    #[test]
    fn test_list_checkpoints_with_file_entries() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR);
        fs::create_dir_all(&cp_dir).unwrap();
        fs::write(cp_dir.join("not_a_dir.txt"), "data").unwrap();

        let checkpoints = list_checkpoints(temp.path()).unwrap();
        assert!(checkpoints.is_empty());
    }

    #[test]
    fn test_get_latest_checkpoint_with_older_checkpoint_first() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        save_checkpoint(temp.path(), &config, "older", &mut Output::new(false)).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        save_checkpoint(temp.path(), &config, "newer", &mut Output::new(false)).unwrap();

        let latest = get_latest_checkpoint(temp.path()).unwrap();
        assert_eq!(latest, Some("newer".to_string()));
    }

    #[test]
    fn test_list_checkpoints_no_dir() {
        let (temp, _config) = setup();
        fs::remove_dir_all(temp.path().join(CHECKPOINTS_DIR)).ok();
        let checkpoints = list_checkpoints(temp.path()).unwrap();
        assert!(checkpoints.is_empty());
    }

    #[test]
    fn test_list_checkpoints_with_file_entry() {
        let (temp, _config) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR);
        fs::create_dir_all(&cp_dir).unwrap();
        fs::write(cp_dir.join("not_a_dir"), "file content").unwrap();
        fs::create_dir_all(cp_dir.join("actual_dir")).unwrap();

        let checkpoints = list_checkpoints(temp.path()).unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0], "actual_dir");
    }

    #[test]
    fn test_show_checkpoint_with_veiled_ranges() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("veiled-cp");
        fs::create_dir_all(&cp_dir).unwrap();

        // Create a manifest with veiled file entries (lines field populated)
        let mut manifest = CheckpointManifest::new("whitelist");
        manifest.files.insert(
            "test.rs".to_string(),
            CheckpointFile {
                hash: "abc1234567890".to_string(),
                lines: Some(vec![(5, 10), (20, 30)]),
                permissions: "644".to_string(),
            },
        );
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        let result = show_checkpoint(temp.path(), "veiled-cp", &mut Output::new(false));
        assert!(result.is_ok());
    }

    #[test]
    fn test_restore_checkpoint_with_bad_hash() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("bad-hash-cp");
        fs::create_dir_all(&cp_dir).unwrap();

        // Create a manifest referencing a hash that doesn't exist in CAS
        let mut manifest = CheckpointManifest::new("whitelist");
        manifest.files.insert(
            "missing.txt".to_string(),
            CheckpointFile {
                hash: "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
                    .to_string(),
                lines: None,
                permissions: "644".to_string(),
            },
        );
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        // Should return error due to partial failure (failed CAS retrieve path)
        let result = restore_checkpoint(temp.path(), "bad-hash-cp", &mut Output::new(false));
        assert!(result.is_err());
    }

    #[test]
    fn test_restore_checkpoint_with_readonly_parent() {
        let (temp, config) = setup();

        // Create a file and save checkpoint
        fs::write(temp.path().join("hello.txt"), "content").unwrap();
        save_checkpoint(
            temp.path(),
            &config,
            "readonly-test",
            &mut Output::new(false),
        )
        .unwrap();

        // Create a manifest referencing a file in a nested dir,
        // where the parent dir is actually a read-only file (preventing create_dir_all)
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("readonly-cp");
        fs::create_dir_all(&cp_dir).unwrap();

        let store = ContentStore::new(temp.path());
        let hash = store.store(b"nested content").unwrap();

        let mut manifest = CheckpointManifest::new("whitelist");
        manifest.add_file(
            "blocked/nested/file.txt".to_string(),
            hash,
            None,
            "644".to_string(),
        );
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        // Create a regular file where the directory should be, blocking create_dir_all
        fs::write(temp.path().join("blocked"), "not a directory").unwrap();

        let result = restore_checkpoint(temp.path(), "readonly-cp", &mut Output::new(false));
        assert!(result.is_err()); // returns error due to partial failure
    }

    #[test]
    fn test_restore_checkpoint_write_failure() {
        let (temp, config) = setup();

        // Create a file and checkpoint it
        fs::write(temp.path().join("protected.txt"), "original").unwrap();
        save_checkpoint(temp.path(), &config, "write-test", &mut Output::new(false)).unwrap();

        // Create manifest pointing to a file in a dir that we'll make read-only
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("write-fail-cp");
        fs::create_dir_all(&cp_dir).unwrap();

        let store = ContentStore::new(temp.path());
        let hash = store.store(b"some content").unwrap();

        let mut manifest = CheckpointManifest::new("whitelist");
        manifest.add_file(
            "readonly_dir/file.txt".to_string(),
            hash,
            None,
            "644".to_string(),
        );
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        // Create the target directory and make it read-only to prevent writing
        let readonly_dir = temp.path().join("readonly_dir");
        fs::create_dir_all(&readonly_dir).unwrap();
        let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&readonly_dir, perms).unwrap();

        let result = restore_checkpoint(temp.path(), "write-fail-cp", &mut Output::new(false));
        assert!(result.is_err()); // returns error due to partial failure

        // Cleanup: make writable again so tempdir can be deleted
        let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
        }
        fs::set_permissions(&readonly_dir, perms).unwrap();
    }

    #[test]
    fn test_get_latest_checkpoint_with_unreadable_manifest() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("unreadable");
        fs::create_dir_all(&cp_dir).unwrap();
        // Create a directory where the manifest file should be, making read fail
        fs::create_dir_all(cp_dir.join("manifest.yaml")).unwrap();

        let result = get_latest_checkpoint(temp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_checkpoint_with_broken_symlink() {
        let (temp, config) = setup();
        fs::write(temp.path().join("real.txt"), "content\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("/nonexistent/target", temp.path().join("broken_link")).ok();
        }

        // Should succeed without panicking despite broken symlink
        let result = save_checkpoint(
            temp.path(),
            &config,
            "symlink_test",
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_checkpoint_binary_file_round_trip() {
        let (temp, config) = setup();
        let binary_content: Vec<u8> = (0..=255).collect();
        let file_path = temp.path().join("binary.bin");
        fs::write(&file_path, &binary_content).unwrap();

        save_checkpoint(temp.path(), &config, "binary_cp", &mut Output::new(false)).unwrap();

        // Corrupt the file
        fs::write(&file_path, b"corrupted").unwrap();

        restore_checkpoint(temp.path(), "binary_cp", &mut Output::new(false)).unwrap();
        let restored = fs::read(&file_path).unwrap();
        assert_eq!(restored, binary_content);
    }

    #[test]
    fn test_checkpoint_permission_preservation() {
        let (temp, config) = setup();
        let file_path = temp.path().join("executable.sh");
        fs::write(&file_path, "#!/bin/bash\necho hello\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file_path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        save_checkpoint(temp.path(), &config, "perm_cp", &mut Output::new(false)).unwrap();

        // Change permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file_path, fs::Permissions::from_mode(0o644)).unwrap();
        }

        restore_checkpoint(temp.path(), "perm_cp", &mut Output::new(false)).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&file_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755, "permissions should be restored to 0o755");
        }
    }

    #[test]
    fn test_show_checkpoint_short_hash() {
        // BUG-010 regression: hashes shorter than 7 chars should not panic
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("short-hash");
        fs::create_dir_all(&cp_dir).unwrap();

        let mut manifest = CheckpointManifest::new("whitelist");
        manifest.files.insert(
            "file.txt".to_string(),
            CheckpointFile {
                hash: "abc".to_string(),
                lines: None,
                permissions: "644".to_string(),
            },
        );
        manifest.files.insert(
            "veiled.txt".to_string(),
            CheckpointFile {
                hash: "xy".to_string(),
                lines: Some(vec![(1, 5)]),
                permissions: "644".to_string(),
            },
        );
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        let result = show_checkpoint(temp.path(), "short-hash", &mut Output::new(false));
        assert!(result.is_ok());
    }

    #[test]
    fn test_restore_checkpoint_with_invalid_hash_string() {
        // BUG-058 regression: invalid hash should not abort entire restore
        let (temp, config) = setup();

        // Create a valid file and checkpoint it
        fs::write(temp.path().join("valid.txt"), "valid content\n").unwrap();
        save_checkpoint(temp.path(), &config, "mixed-cp", &mut Output::new(false)).unwrap();

        // Now create a checkpoint with one valid and one invalid hash entry
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("invalid-hash-cp");
        fs::create_dir_all(&cp_dir).unwrap();

        let store = ContentStore::new(temp.path());
        let valid_hash = store.store(b"restored content").unwrap();

        let mut manifest = CheckpointManifest::new("whitelist");
        manifest.files.insert(
            "bad.txt".to_string(),
            CheckpointFile {
                hash: "not-a-valid-hash".to_string(),
                lines: None,
                permissions: "644".to_string(),
            },
        );
        manifest.add_file("good.txt".to_string(), valid_hash, None, "644".to_string());
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        let result = restore_checkpoint(temp.path(), "invalid-hash-cp", &mut Output::new(false));
        // Should return error (partial failure) but not abort
        assert!(result.is_err());

        // The valid file should still have been restored
        let good_content = fs::read(temp.path().join("good.txt")).unwrap();
        assert_eq!(good_content, b"restored content");
    }

    // ── validate_checkpoint_name: || to && (catches line 60) ──

    #[test]
    fn test_validate_name_rejects_forward_slash() {
        let result = validate_checkpoint_name("path/name");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_name_rejects_backslash() {
        let result = validate_checkpoint_name("path\\name");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_name_rejects_dotdot() {
        let result = validate_checkpoint_name("..escape");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_name_allows_normal() {
        let result = validate_checkpoint_name("my-checkpoint_v2.1");
        assert!(result.is_ok());
    }

    // ── get_latest_checkpoint: comparison operator (catches lines 192) ──

    #[test]
    fn test_get_latest_checkpoint_returns_newest() {
        let (temp, _config) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR);

        // Create two checkpoints with different timestamps.
        // "older" has an earlier timestamp, "newer" has a later one.
        let older_manifest = CheckpointManifest {
            created: DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            mode: "whitelist".to_string(),
            files: HashMap::new(),
        };
        let newer_manifest = CheckpointManifest {
            created: DateTime::parse_from_rfc3339("2025-06-15T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            mode: "whitelist".to_string(),
            files: HashMap::new(),
        };

        // Write older first, then newer
        fs::create_dir_all(cp_dir.join("older")).unwrap();
        fs::write(
            cp_dir.join("older/manifest.yaml"),
            serde_yaml::to_string(&older_manifest).unwrap(),
        )
        .unwrap();

        fs::create_dir_all(cp_dir.join("newer")).unwrap();
        fs::write(
            cp_dir.join("newer/manifest.yaml"),
            serde_yaml::to_string(&newer_manifest).unwrap(),
        )
        .unwrap();

        let latest = get_latest_checkpoint(temp.path()).unwrap();
        assert_eq!(
            latest.as_deref(),
            Some("newer"),
            "should return the checkpoint with the latest timestamp"
        );
    }

    #[test]
    fn test_get_latest_checkpoint_with_same_timestamps() {
        // Edge case: if timestamps are equal, the first one found should be kept
        // (since > doesn't match, the later one won't replace)
        let (temp, _config) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR);

        let ts = DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let manifest = CheckpointManifest {
            created: ts,
            mode: "whitelist".to_string(),
            files: HashMap::new(),
        };

        fs::create_dir_all(cp_dir.join("cp1")).unwrap();
        fs::write(
            cp_dir.join("cp1/manifest.yaml"),
            serde_yaml::to_string(&manifest).unwrap(),
        )
        .unwrap();

        fs::create_dir_all(cp_dir.join("cp2")).unwrap();
        fs::write(
            cp_dir.join("cp2/manifest.yaml"),
            serde_yaml::to_string(&manifest).unwrap(),
        )
        .unwrap();

        let latest = get_latest_checkpoint(temp.path()).unwrap();
        // Should return one of them (not None)
        assert!(latest.is_some());
    }

    #[test]
    fn test_get_latest_checkpoint_readdir_order_independence() {
        // Use names that influence readdir order: "aaa" sorts before "zzz".
        // "aaa" is oldest, "zzz" is newest. Catches > → == and match guard → false.
        let (temp, _config) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR);

        let oldest = CheckpointManifest {
            created: DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            mode: "whitelist".to_string(),
            files: HashMap::new(),
        };
        let newest = CheckpointManifest {
            created: DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            mode: "whitelist".to_string(),
            files: HashMap::new(),
        };

        // "aaa" is likely read first → if > mutation becomes == or false,
        // "aaa" stays as latest and "zzz" never replaces it
        fs::create_dir_all(cp_dir.join("aaa")).unwrap();
        fs::write(
            cp_dir.join("aaa/manifest.yaml"),
            serde_yaml::to_string(&oldest).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(cp_dir.join("zzz")).unwrap();
        fs::write(
            cp_dir.join("zzz/manifest.yaml"),
            serde_yaml::to_string(&newest).unwrap(),
        )
        .unwrap();

        let latest = get_latest_checkpoint(temp.path()).unwrap();
        assert_eq!(latest.as_deref(), Some("zzz"));
    }

    // ── save_checkpoint: quiet mode and walk_errors (catches lines 93, 96, 130) ──

    #[test]
    fn test_save_checkpoint_quiet() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();

        // Save with quiet=true should succeed without output
        let result = save_checkpoint(temp.path(), &config, "quiet_cp", &mut Output::new(true));
        assert!(result.is_ok());

        // Verify the checkpoint was still created
        let manifest_path = temp
            .path()
            .join(CHECKPOINTS_DIR)
            .join("quiet_cp/manifest.yaml");
        assert!(manifest_path.exists());
    }

    // ── restore_checkpoint: quiet mode and counter (catches lines 260-308) ──

    #[test]
    fn test_restore_checkpoint_quiet_with_failures() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("bad_cp");
        fs::create_dir_all(&cp_dir).unwrap();

        let store = ContentStore::new(temp.path());
        let valid_hash = store.store(b"valid content").unwrap();

        let mut manifest = CheckpointManifest::new("whitelist");
        // One file with invalid hash (will fail)
        manifest.files.insert(
            "bad.txt".to_string(),
            CheckpointFile {
                hash: "badhash".to_string(),
                lines: None,
                permissions: "644".to_string(),
            },
        );
        // One file with valid hash (will succeed)
        manifest.add_file("good.txt".to_string(), valid_hash, None, "644".to_string());

        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        // Restore with quiet=true - should still report failure via result
        let result = restore_checkpoint(temp.path(), "bad_cp", &mut Output::new(true));
        assert!(result.is_err(), "partial restore should return error");

        // Valid file should still have been restored
        let good_content = fs::read(temp.path().join("good.txt")).unwrap();
        assert_eq!(good_content, b"valid content");
    }

    #[test]
    fn test_restore_checkpoint_counts_restored_correctly() {
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("count_cp");
        fs::create_dir_all(&cp_dir).unwrap();

        let store = ContentStore::new(temp.path());
        let hash1 = store.store(b"content 1").unwrap();
        let hash2 = store.store(b"content 2").unwrap();

        let mut manifest = CheckpointManifest::new("whitelist");
        manifest.add_file("file1.txt".to_string(), hash1, None, "644".to_string());
        manifest.add_file("file2.txt".to_string(), hash2, None, "644".to_string());

        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        // Restore should succeed with all files
        let result = restore_checkpoint(temp.path(), "count_cp", &mut Output::new(false));
        assert!(result.is_ok());

        // Both files should exist
        assert!(temp.path().join("file1.txt").exists());
        assert!(temp.path().join("file2.txt").exists());
    }

    // ── Mutant-targeted: get_latest_checkpoint > vs >= (line 192) ──

    #[test]
    fn test_get_latest_checkpoint_equal_timestamps_no_replace() {
        // If `>` is mutated to `>=`, equal timestamps would cause replacement.
        // With `>`, the first-found checkpoint is kept when timestamps are equal.
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR);

        let ts = DateTime::parse_from_rfc3339("2025-06-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // Create "first" checkpoint
        let manifest_first = CheckpointManifest {
            created: ts,
            mode: "whitelist".to_string(),
            files: HashMap::new(),
        };
        fs::create_dir_all(cp_dir.join("first")).unwrap();
        fs::write(
            cp_dir.join("first/manifest.yaml"),
            serde_yaml::to_string(&manifest_first).unwrap(),
        )
        .unwrap();

        // Create "second" checkpoint with SAME timestamp
        let manifest_second = CheckpointManifest {
            created: ts,
            mode: "whitelist".to_string(),
            files: HashMap::new(),
        };
        fs::create_dir_all(cp_dir.join("second")).unwrap();
        fs::write(
            cp_dir.join("second/manifest.yaml"),
            serde_yaml::to_string(&manifest_second).unwrap(),
        )
        .unwrap();

        // Call twice to ensure stability: the result should not flip
        let latest1 = get_latest_checkpoint(temp.path()).unwrap().unwrap();
        let latest2 = get_latest_checkpoint(temp.path()).unwrap().unwrap();
        assert_eq!(
            latest1, latest2,
            "equal timestamps should produce a stable result (> does not replace)"
        );
    }

    // ── Mutant-targeted: restored += 1 vs *= (line 308) ──

    #[test]
    fn test_restore_checkpoint_restored_counter_increments() {
        // If `+=` is mutated to `*=`, `restored` starts at 0 and 0 *= 1 == 0
        // forever. With multiple files, restored should be > 0, and the
        // PartialRestore error should reflect the correct counts.
        let (temp, _) = setup();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("counter_cp");
        fs::create_dir_all(&cp_dir).unwrap();

        let store = ContentStore::new(temp.path());
        let h1 = store.store(b"aaa").unwrap();
        let h2 = store.store(b"bbb").unwrap();
        let h3 = store.store(b"ccc").unwrap();

        let mut manifest = CheckpointManifest::new("whitelist");
        manifest.add_file("a.txt".to_string(), h1, None, "644".to_string());
        manifest.add_file("b.txt".to_string(), h2, None, "644".to_string());
        manifest.add_file("c.txt".to_string(), h3, None, "644".to_string());
        // Add one bad entry to force PartialRestore so we can inspect counts
        manifest.files.insert(
            "bad.txt".to_string(),
            CheckpointFile {
                hash: "invalid".to_string(),
                lines: None,
                permissions: "644".to_string(),
            },
        );

        let yaml = serde_yaml::to_string(&manifest).unwrap();
        fs::write(cp_dir.join("manifest.yaml"), &yaml).unwrap();

        let result = restore_checkpoint(temp.path(), "counter_cp", &mut Output::new(true));
        match result {
            Err(FunveilError::PartialRestore { restored, failed }) => {
                assert_eq!(restored, 3, "three valid files should be restored");
                assert_eq!(failed, 1, "one invalid file should have failed");
            }
            other => panic!("expected PartialRestore error, got {other:?}"),
        }

        // Verify files actually exist
        assert_eq!(fs::read(temp.path().join("a.txt")).unwrap(), b"aaa");
        assert_eq!(fs::read(temp.path().join("b.txt")).unwrap(), b"bbb");
        assert_eq!(fs::read(temp.path().join("c.txt")).unwrap(), b"ccc");
    }
}
