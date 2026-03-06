use crate::cas::ContentStore;
use crate::config::{is_config_file, is_data_dir, Config, ObjectMeta};
use crate::error::{FunveilError, Result};
use crate::types::{is_binary_file, is_funveil_protected, is_vcs_directory, ContentHash, LineRange};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

/// Veil a file or line range
pub fn veil_file(
    root: &Path,
    config: &mut Config,
    file: &str,
    ranges: Option<&[LineRange]>,
) -> Result<()> {
    // Check for protected paths
    if is_config_file(file) {
        return Err(FunveilError::ConfigFileProtected);
    }
    if is_data_dir(file) || is_funveil_protected(file) {
        return Err(FunveilError::DataDirectoryProtected);
    }
    if is_vcs_directory(file) {
        return Err(FunveilError::VcsDirectoryExcluded(file.to_string()));
    }
    
    let file_path = root.join(file);
    
    // Check if file exists
    if !file_path.exists() {
        return Err(FunveilError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("file not found: {}", file),
        )));
    }
    
    // Check if binary file with partial veiling
    if ranges.is_some() && is_binary_file(&file_path) {
        return Err(FunveilError::BinaryFilePartialVeil(file.to_string()));
    }
    
    // Read file content
    let content = fs::read_to_string(&file_path)?;
    
    // Check if empty file
    if content.is_empty() && ranges.is_some() {
        return Err(FunveilError::EmptyFile(file.to_string()));
    }
    
    // Get original permissions
    let metadata = file_path.metadata()?;
    let permissions = metadata.mode();
    
    let store = ContentStore::new(root);
    
    match ranges {
        None => {
            // Full file veil
            let hash = store.store(content.as_bytes())?;
            let key = file.to_string();
            
            // Check if already veiled
            if config.get_object(&key).is_some() {
                return Err(FunveilError::AlreadyVeiled(file.to_string()));
            }
            
            // Store metadata
            config.register_object(key.clone(), ObjectMeta::new(hash.clone(), permissions));
            
            // Replace with marker
            let marker = "...\n";
            fs::write(&file_path, marker)?;
            
            // Set read-only
            let mut perms = fs::metadata(&file_path)?.permissions();
            perms.set_readonly(true);
            fs::set_permissions(&file_path, perms)?;
        }
        Some(ranges) => {
            // Partial veil
            let lines: Vec<&str> = content.lines().collect();
            
            for range in ranges {
                // Extract content for this range
                let start = range.start().saturating_sub(1);
                let end = range.end().min(lines.len());
                
                if start >= lines.len() {
                    continue; // Range beyond file length, skip
                }
                
                let veiled_content = lines[start..end.min(lines.len())].join("\n");
                let hash = store.store(veiled_content.as_bytes())?;
                
                let key = format!("{}#{}", file, range);
                
                // Check if already veiled
                if config.get_object(&key).is_some() {
                    return Err(FunveilError::AlreadyVeiled(key));
                }
                
                config.register_object(key, ObjectMeta::new(hash.clone(), permissions));
            }
            
            // Build veiled file
            let mut output = String::new();
            let _line_idx = 0;
            
            for (i, line) in lines.iter().enumerate() {
                let line_num = i + 1; // 1-indexed
                
                // Check if this line is in any veiled range
                let mut in_range = None;
                for range in ranges {
                    if range.contains(line_num) {
                        in_range = Some(range);
                        break;
                    }
                }
                
                if let Some(range) = in_range {
                    // This line is veiled
                    let range_len = range.len();
                    let pos_in_range = line_num - range.start();
                    
                    if range_len == 1 {
                        // Single line veiled
                        let key = format!("{}#{}", file, range);
                        let meta = config.get_object(&key).unwrap();
                        let hash = ContentHash::from_string(meta.hash.clone());
                        output.push_str(&format!("...[{}]...\n", hash.short()));
                    } else if pos_in_range == 1 {
                        // First line of multi-line range
                        let key = format!("{}#{}", file, range);
                        let meta = config.get_object(&key).unwrap();
                        let hash = ContentHash::from_string(meta.hash.clone());
                        output.push_str(&format!("...[{}]\n", hash.short()));
                    } else if pos_in_range == range_len {
                        // Last line of multi-line range
                        output.push_str("...\n");
                    } else {
                        // Middle line
                        output.push('\n');
                    }
                } else {
                    // Visible line
                    output.push_str(line);
                    output.push('\n');
                }
            }
            
            fs::write(&file_path, output)?;
            
            // Set read-only
            let mut perms = fs::metadata(&file_path)?.permissions();
            perms.set_readonly(true);
            fs::set_permissions(&file_path, perms)?;
        }
    }
    
    Ok(())
}

/// Unveil a file or line range
pub fn unveil_file(
    root: &Path,
    config: &mut Config,
    file: &str,
    ranges: Option<&[LineRange]>,
) -> Result<()> {
    let store = ContentStore::new(root);
    let file_path = root.join(file);
    
    match ranges {
        None => {
            // Full file unveil
            let key = file.to_string();
            let meta = config.get_object(&key)
                .ok_or_else(|| FunveilError::NotVeiled(file.to_string()))?;
            
            let hash = ContentHash::from_string(meta.hash.clone());
            let content = store.retrieve(&hash)?;
            
            // Restore content
            fs::write(&file_path, content)?;
            
            // Restore permissions
            let perms = u32::from_str_radix(&meta.permissions, 8)
                .unwrap_or(0o644);
            let mut permissions = fs::metadata(&file_path)?.permissions();
            permissions.set_mode(perms);
            fs::set_permissions(&file_path, permissions)?;
            
            // Remove from config
            config.unregister_object(&key);
        }
        Some(ranges) => {
            // Partial unveil
            // For now, partial unveil requires reading the original file,
            // reconstructing it, and removing specific ranges
            
            // Read current veiled file
            let veiled_content = fs::read_to_string(&file_path)?;
            let lines: Vec<&str> = veiled_content.lines().collect();
            
            let mut full_content = String::new();
            
            for (i, line) in lines.iter().enumerate() {
                let line_num = i + 1;
                
                // Check if this line is in any range we're unveiling
                let mut unveiling = false;
                for range in ranges {
                    if range.contains(line_num) {
                        unveiling = true;
                        break;
                    }
                }
                
                if unveiling {
                    // Find the object key for this range
                    for range in ranges {
                        if range.contains(line_num) && line_num == range.start() {
                            let key = format!("{}#{}", file, range);
                            if let Some(meta) = config.get_object(&key) {
                                let hash = ContentHash::from_string(meta.hash.clone());
                                let content = store.retrieve(&hash)?;
                                let content_str = String::from_utf8_lossy(&content);
                                full_content.push_str(&content_str);
                                full_content.push('\n');
                                
                                // Remove from config
                                config.unregister_object(&key);
                            }
                        }
                    }
                } else {
                    full_content.push_str(line);
                    full_content.push('\n');
                }
            }
            
            fs::write(&file_path, full_content)?;
            
            // Check if any veils remain
            let remaining = config.veiled_ranges(file)?;
            if remaining.is_empty() && config.get_object(file).is_none() {
                // No more veils, restore permissions
                let meta = config.get_object(&format!("{}#{}-{}", file, ranges[0].start(), ranges[0].end()));
                if let Some(meta) = meta {
                    let perms = u32::from_str_radix(&meta.permissions, 8).unwrap_or(0o644);
                    let mut permissions = fs::metadata(&file_path)?.permissions();
                    permissions.set_mode(perms);
                    fs::set_permissions(&file_path, permissions)?;
                }
            }
        }
    }
    
    Ok(())
}

/// Unveil all files
pub fn unveil_all(root: &Path, config: &mut Config) -> Result<()> {
    let keys: Vec<String> = config.objects.keys().cloned().collect();
    
    for key in keys {
        if let Some(pos) = key.find('#') {
            let file = &key[..pos];
            unveil_file(root, config, file, None)?;
        } else {
            unveil_file(root, config, &key, None)?;
        }
    }
    
    Ok(())
}

/// Check if a file has any veils
pub fn is_veiled(config: &Config, file: &str) -> bool {
    config.get_object(file).is_some() ||
    config.objects.keys().any(|k| k.starts_with(&format!("{}#", file)))
}
