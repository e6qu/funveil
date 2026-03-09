use crate::cas::ContentStore;
use crate::config::{is_config_file, is_data_dir, Config, ObjectMeta};
use crate::error::{FunveilError, Result};
use crate::types::{
    is_binary_file, is_funveil_protected, is_vcs_directory, ContentHash, LineRange,
};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::str::FromStr;

/// Veil a file, directory, or line range
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
            format!("file not found: {file}"),
        )));
    }

    // Handle directories recursively
    if file_path.is_dir() {
        return veil_directory(root, config, &file_path, ranges);
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

                let key = format!("{file}#{range}");

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
                        let key = format!("{file}#{range}");
                        let meta = config.get_object(&key).unwrap();
                        let hash = ContentHash::from_string(meta.hash.clone());
                        output.push_str(&format!("...[{}]...\n", hash.short()));
                    } else if pos_in_range == 1 {
                        // First line of multi-line range
                        let key = format!("{file}#{range}");
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

/// Recursively veil all files in a directory
fn veil_directory(
    root: &Path,
    config: &mut Config,
    dir_path: &Path,
    ranges: Option<&[LineRange]>,
) -> Result<()> {
    let entries = fs::read_dir(dir_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let relative_path = path.strip_prefix(root).unwrap_or(&path);
        let path_str = relative_path.to_string_lossy();

        // Skip protected paths
        if is_config_file(&path_str)
            || is_data_dir(&path_str)
            || is_funveil_protected(&path_str)
            || is_vcs_directory(&path_str)
        {
            continue;
        }

        if path.is_dir() {
            // Recursively veil subdirectory
            veil_directory(root, config, &path, ranges)?;
        } else if path.is_file() {
            // Veil the file - ignore errors for individual files (e.g., binary files)
            let _ = veil_file(root, config, &path_str, ranges);
        }
    }

    Ok(())
}

/// Unveil a file, directory, or line range
pub fn unveil_file(
    root: &Path,
    config: &mut Config,
    file: &str,
    ranges: Option<&[LineRange]>,
) -> Result<()> {
    let store = ContentStore::new(root);
    let file_path = root.join(file);

    // Check if path exists
    if !file_path.exists() {
        return Err(FunveilError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("file not found: {file}"),
        )));
    }

    // Handle directories recursively
    if file_path.is_dir() {
        return unveil_directory(root, config, &file_path, ranges);
    }

    // Make file writable first (in case it's read-only from previous veil)
    // Use 0o644 (rw-r--r--) instead of set_readonly(false) to avoid world-writable files
    #[cfg(unix)]
    {
        let metadata = fs::metadata(&file_path)?;
        let mut permissions = metadata.permissions();
        // Set owner read/write, group read, others read (0o644)
        permissions.set_mode(0o644);
        fs::set_permissions(&file_path, permissions)?;
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(&file_path)?.permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&file_path, permissions)?;
    }

    match ranges {
        None => {
            // Full file unveil
            let key = file.to_string();

            // Check if fully veiled first
            if let Some(meta) = config.get_object(&key) {
                let hash = ContentHash::from_string(meta.hash.clone());
                let content = store.retrieve(&hash)?;

                // Restore content
                fs::write(&file_path, content)?;

                // Restore permissions
                let perms = u32::from_str_radix(&meta.permissions, 8).unwrap_or(0o644);
                let mut permissions = fs::metadata(&file_path)?.permissions();
                permissions.set_mode(perms);
                fs::set_permissions(&file_path, permissions)?;

                // Remove from config
                config.unregister_object(&key);
                return Ok(());
            }

            // Check for partial veils - unveil them all
            let partial_keys: Vec<String> = config
                .objects
                .keys()
                .filter(|k| k.starts_with(&format!("{file}#")))
                .cloned()
                .collect();

            if partial_keys.is_empty() {
                return Err(FunveilError::NotVeiled(file.to_string()));
            }

            // For partial veils, we need to reconstruct the full file
            // For simplicity, we'll unveil each range one by one
            // This requires retrieving the original from the first partial veil's stored content
            // Actually, we need the full original which isn't stored with partial veils
            // So we'll reconstruct by unveiling each range
            let mut full_content = String::new();
            let veiled_content = fs::read_to_string(&file_path)?;
            let _lines: Vec<&str> = veiled_content.lines().collect();

            // Parse all veiled ranges and their content
            let mut veiled_ranges: Vec<(LineRange, Vec<u8>)> = Vec::new();
            for key in &partial_keys {
                if let Some(pos) = key.find('#') {
                    let range_str = &key[pos + 1..];
                    if let Ok(range) = LineRange::from_str(range_str) {
                        if let Some(meta) = config.get_object(key) {
                            let hash = ContentHash::from_string(meta.hash.clone());
                            if let Ok(content) = store.retrieve(&hash) {
                                veiled_ranges.push((range, content));
                            }
                        }
                    }
                }
            }

            // Sort ranges by start line
            veiled_ranges.sort_by_key(|(r, _)| r.start());

            // Reconstruct the file
            let mut _current_line = 1;
            for (range, content) in &veiled_ranges {
                // Add content for this range
                let content_str = String::from_utf8_lossy(content);
                full_content.push_str(&content_str);
                full_content.push('\n');
                _current_line = range.end() + 1;
            }

            fs::write(&file_path, full_content)?;

            // Restore permissions from first range
            if let Some(meta) = config.get_object(&partial_keys[0]) {
                let perms = u32::from_str_radix(&meta.permissions, 8).unwrap_or(0o644);
                let mut permissions = fs::metadata(&file_path)?.permissions();
                permissions.set_mode(perms);
                fs::set_permissions(&file_path, permissions)?;
            }

            // Remove all partial veils from config
            for key in partial_keys {
                config.unregister_object(&key);
            }

            return Ok(());
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
                            let key = format!("{file}#{range}");
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
                let meta = config.get_object(&format!(
                    "{}#{}-{}",
                    file,
                    ranges[0].start(),
                    ranges[0].end()
                ));
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

/// Recursively unveil all files in a directory
fn unveil_directory(
    root: &Path,
    config: &mut Config,
    dir_path: &Path,
    ranges: Option<&[LineRange]>,
) -> Result<()> {
    let entries = fs::read_dir(dir_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let relative_path = path.strip_prefix(root).unwrap_or(&path);
        let path_str = relative_path.to_string_lossy();

        // Skip protected paths
        if is_config_file(&path_str)
            || is_data_dir(&path_str)
            || is_funveil_protected(&path_str)
            || is_vcs_directory(&path_str)
        {
            continue;
        }

        if path.is_dir() {
            // Recursively unveil subdirectory
            unveil_directory(root, config, &path, ranges)?;
        } else if path.is_file() {
            // Unveil the file - ignore errors for individual files
            let _ = unveil_file(root, config, &path_str, ranges);
        }
    }

    Ok(())
}

/// Unveil all files
pub fn unveil_all(root: &Path, config: &mut Config) -> Result<()> {
    // Collect all unique files that have veils (both full and partial)
    let mut files_to_unveil: Vec<String> = Vec::new();

    for key in config.objects.keys() {
        let file = if let Some(pos) = key.find('#') {
            key[..pos].to_string()
        } else {
            key.clone()
        };

        if !files_to_unveil.contains(&file) {
            files_to_unveil.push(file);
        }
    }

    // Unveil each file completely
    for file in files_to_unveil {
        unveil_file(root, config, &file, None)?;
    }

    Ok(())
}

/// Check if a file has any veils
pub fn is_veiled(config: &Config, file: &str) -> bool {
    config.get_object(file).is_some()
        || config
            .objects
            .keys()
            .any(|k| k.starts_with(&format!("{file}#")))
}
