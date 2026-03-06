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
    
    pub fn add_file(&mut self, path: String, hash: ContentHash, lines: Option<Vec<(usize, usize)>>, permissions: String) {
        self.files.insert(path, CheckpointFile {
            hash: hash.full().to_string(),
            lines,
            permissions,
        });
    }
}

/// Save a checkpoint
pub fn save_checkpoint(root: &Path, config: &Config, name: &str) -> Result<()> {
    let checkpoint_dir = root.join(CHECKPOINTS_DIR).join(name);
    fs::create_dir_all(&checkpoint_dir)?;
    
    let mut manifest = CheckpointManifest::new(&config.mode.to_string());
    let store = ContentStore::new(root);
    
    // Walk all files in the project
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap();
        let relative_str = relative.to_string_lossy().to_string();
        
        // Skip funveil directories and VCS
        if relative_str.starts_with(".funveil") || 
           relative_str.starts_with(".git") ||
           relative_str == ".funveil_config" {
            continue;
        }
        
        // Store content in CAS
        let content = fs::read(path)?;
        let hash = store.store(&content)?;
        
        // Get permissions
        let metadata = fs::metadata(path)?;
        let permissions = format!("{:o}", metadata.mode() & 0o777);
        
        // Check if veiled
        let lines = config.veiled_ranges(&relative_str).ok()
            .map(|ranges| ranges.iter()
                .map(|r| (r.start(), r.end()))
                .collect());
        
        manifest.add_file(relative_str, hash, lines, permissions);
    }
    
    // Write manifest
    let manifest_path = checkpoint_dir.join("manifest.yaml");
    let manifest_yaml = serde_yaml::to_string(&manifest)?;
    fs::write(&manifest_path, manifest_yaml)?;
    
    println!("Checkpoint '{}' saved with {} files.", name, manifest.files.len());
    Ok(())
}

/// List all checkpoints
pub fn list_checkpoints(root: &Path) -> Result<Vec<String>> {
    let checkpoints_dir = root.join(CHECKPOINTS_DIR);
    
    if !checkpoints_dir.exists() {
        return Ok(Vec::new());
    }
    
    let mut checkpoints = Vec::new();
    for entry in fs::read_dir(&checkpoints_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            checkpoints.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    
    Ok(checkpoints)
}

/// Show checkpoint details
pub fn show_checkpoint(root: &Path, name: &str) -> Result<()> {
    let manifest_path = root.join(CHECKPOINTS_DIR).join(name).join("manifest.yaml");
    
    if !manifest_path.exists() {
        return Err(FunveilError::CheckpointNotFound(name.to_string()));
    }
    
    let content = fs::read_to_string(&manifest_path)?;
    let manifest: CheckpointManifest = serde_yaml::from_str(&content)?;
    
    println!("Checkpoint: {}", name);
    println!("Created: {}", manifest.created);
    println!("Mode: {}", manifest.mode);
    println!("Files: {}", manifest.files.len());
    
    for (path, file) in &manifest.files {
        if let Some(lines) = &file.lines {
            let ranges: Vec<String> = lines.iter()
                .map(|(s, e)| format!("{}-{}", s, e))
                .collect();
            println!("  {} [{}] (veiled: {})", path, &file.hash[..7], ranges.join(", "));
        } else {
            println!("  {} [{}]", path, &file.hash[..7]);
        }
    }
    
    Ok(())
}
