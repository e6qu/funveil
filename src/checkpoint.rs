use crate::cas::ContentStore;
use crate::config::{Config, CHECKPOINTS_DIR};
use crate::error::{FunveilError, Result};
use crate::types::ContentHash;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
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

pub fn save_checkpoint(root: &Path, config: &Config, name: &str) -> Result<()> {
    let checkpoint_dir = root.join(CHECKPOINTS_DIR).join(name);
    fs::create_dir_all(&checkpoint_dir)?;

    let mut manifest = CheckpointManifest::new(&config.mode.to_string());
    let store = ContentStore::new(root);

    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let relative = match path.strip_prefix(root) {
            Ok(r) => r,
            Err(_) => continue,
        };
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

    let manifest_path = checkpoint_dir.join("manifest.yaml");
    let manifest_yaml = serde_yaml::to_string(&manifest)?;
    fs::write(&manifest_path, manifest_yaml)?;

    println!(
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

pub fn show_checkpoint(root: &Path, name: &str) -> Result<()> {
    let manifest_path = root.join(CHECKPOINTS_DIR).join(name).join("manifest.yaml");

    if !manifest_path.exists() {
        return Err(FunveilError::CheckpointNotFound(name.to_string()));
    }

    let content = fs::read_to_string(&manifest_path)?;
    let manifest: CheckpointManifest = serde_yaml::from_str(&content)?;

    println!("Checkpoint: {name}");
    println!("Created: {}", manifest.created);
    println!("Mode: {}", manifest.mode);
    println!("Files: {}", manifest.files.len());

    for (path, file) in &manifest.files {
        match &file.lines {
            Some(lines) => {
                let ranges: Vec<String> = lines.iter().map(|(s, e)| format!("{s}-{e}")).collect();
                println!(
                    "  {} [{}] (veiled: {})",
                    path,
                    &file.hash[..7],
                    ranges.join(", ")
                );
            }
            None => println!("  {} [{}]", path, &file.hash[..7]),
        }
    }

    Ok(())
}

pub fn restore_checkpoint(root: &Path, name: &str) -> Result<()> {
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
        let hash = ContentHash::from_string(file_info.hash.clone());

        let Ok(content) = store.retrieve(&hash) else {
            eprintln!("Failed to retrieve {path} from CAS");
            failed += 1;
            continue;
        };

        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("Failed to create directory {parent:?}: {e}");
                    failed += 1;
                    continue;
                }
            }
        }

        if let Err(e) = fs::write(&file_path, &content) {
            eprintln!("Failed to restore {path}: {e}");
            failed += 1;
            continue;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(perms) = u32::from_str_radix(&file_info.permissions, 8) {
                let _ = fs::set_permissions(&file_path, fs::Permissions::from_mode(perms));
            }
        }

        restored += 1;
    }

    println!("Checkpoint '{name}' restored: {restored} files restored, {failed} failed");

    Ok(())
}

pub fn delete_checkpoint(root: &Path, name: &str) -> Result<()> {
    let checkpoint_dir = root.join(CHECKPOINTS_DIR).join(name);

    if !checkpoint_dir.exists() {
        return Err(FunveilError::CheckpointNotFound(name.to_string()));
    }

    fs::remove_dir_all(&checkpoint_dir)?;

    println!("Checkpoint '{name}' deleted.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ensure_data_dir, Config};
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

        save_checkpoint(temp.path(), &config, "test_cp").unwrap();

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
        let result = show_checkpoint(temp.path(), "nonexistent");
        assert!(matches!(result, Err(FunveilError::CheckpointNotFound(_))));
    }

    #[test]
    fn test_show_checkpoint() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();

        save_checkpoint(temp.path(), &config, "my_checkpoint").unwrap();
        let result = show_checkpoint(temp.path(), "my_checkpoint");
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_checkpoint_not_found() {
        let (temp, _) = setup();
        let result = delete_checkpoint(temp.path(), "nonexistent");
        assert!(matches!(result, Err(FunveilError::CheckpointNotFound(_))));
    }

    #[test]
    fn test_delete_checkpoint() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();

        save_checkpoint(temp.path(), &config, "to_delete").unwrap();
        let cp_dir = temp.path().join(CHECKPOINTS_DIR).join("to_delete");
        assert!(cp_dir.exists());

        delete_checkpoint(temp.path(), "to_delete").unwrap();
        assert!(!cp_dir.exists());
    }

    #[test]
    fn test_restore_checkpoint() {
        let (temp, config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "original content\n").unwrap();

        save_checkpoint(temp.path(), &config, "restore_test").unwrap();

        fs::write(&file_path, "modified content\n").unwrap();
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            "modified content\n"
        );

        restore_checkpoint(temp.path(), "restore_test").unwrap();
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            "original content\n"
        );
    }

    #[test]
    fn test_restore_checkpoint_not_found() {
        let (temp, _) = setup();
        let result = restore_checkpoint(temp.path(), "nonexistent");
        assert!(matches!(result, Err(FunveilError::CheckpointNotFound(_))));
    }

    #[test]
    fn test_get_latest_checkpoint() {
        let (temp, config) = setup();
        fs::write(temp.path().join("test.txt"), "content\n").unwrap();

        save_checkpoint(temp.path(), &config, "first").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        save_checkpoint(temp.path(), &config, "second").unwrap();

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
}
