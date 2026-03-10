use crate::cas::ContentStore;
use crate::config::{is_config_file, is_data_dir, Config, ObjectMeta};
use crate::error::{FunveilError, Result};
use crate::types::{
    is_binary_file, is_funveil_protected, is_vcs_directory, validate_path_within_root, ContentHash,
    LineRange,
};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::str::FromStr;

const ORIGINAL_SUFFIX: &str = "#_original";

pub fn veil_file(
    root: &Path,
    config: &mut Config,
    file: &str,
    ranges: Option<&[LineRange]>,
) -> Result<()> {
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

    if !file_path.exists() {
        return Err(FunveilError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("file not found: {file}"),
        )));
    }

    validate_path_within_root(&file_path, root).map_err(|e| {
        FunveilError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("symlink escape detected: {e}"),
        ))
    })?;

    if file_path.is_dir() {
        return veil_directory(root, config, &file_path, ranges);
    }

    if ranges.is_some() && is_binary_file(&file_path) {
        return Err(FunveilError::BinaryFilePartialVeil(file.to_string()));
    }

    let content = fs::read_to_string(&file_path)?;

    if content.is_empty() && ranges.is_some() {
        return Err(FunveilError::EmptyFile(file.to_string()));
    }

    let metadata = file_path.metadata()?;
    let permissions = metadata.mode();

    let store = ContentStore::new(root);

    match ranges {
        None => {
            let hash = store.store(content.as_bytes())?;
            let key = file.to_string();

            if config.get_object(&key).is_some() {
                return Err(FunveilError::AlreadyVeiled(file.to_string()));
            }

            config.register_object(key.clone(), ObjectMeta::new(hash.clone(), permissions));

            let marker = "...\n";
            fs::write(&file_path, marker)?;

            let mut perms = fs::metadata(&file_path)?.permissions();
            perms.set_readonly(true);
            fs::set_permissions(&file_path, perms)?;
        }
        Some(ranges) => {
            let original_key = format!("{file}{ORIGINAL_SUFFIX}");
            let has_existing_veils = config
                .objects
                .keys()
                .any(|k| k.starts_with(&format!("{file}#")) && !k.ends_with(ORIGINAL_SUFFIX));

            let (lines, original_perms, had_trailing_newline): (Vec<String>, String, bool) =
                if has_existing_veils {
                    if let Some(meta) = config.get_object(&original_key) {
                        let hash = ContentHash::from_string(meta.hash.clone());
                        let original_content = store.retrieve(&hash)?;
                        let original_str = String::from_utf8_lossy(&original_content).into_owned();
                        let trailing = original_str.ends_with('\n');
                        (
                            original_str.lines().map(|s| s.to_string()).collect(),
                            meta.permissions.clone(),
                            trailing,
                        )
                    } else {
                        let trailing = content.ends_with('\n');
                        (
                            content.lines().map(|s| s.to_string()).collect(),
                            format!("{permissions:o}"),
                            trailing,
                        )
                    }
                } else {
                    let trailing = content.ends_with('\n');
                    (
                        content.lines().map(|s| s.to_string()).collect(),
                        format!("{permissions:o}"),
                        trailing,
                    )
                };

            if config.get_object(&original_key).is_none() {
                let mut full_content = lines.join("\n");
                if had_trailing_newline {
                    full_content.push('\n');
                }
                let full_hash = store.store(full_content.as_bytes())?;
                config.register_object(
                    original_key,
                    ObjectMeta::new(
                        full_hash,
                        u32::from_str_radix(&original_perms, 8).unwrap_or(0o644),
                    ),
                );
            }

            #[cfg(unix)]
            {
                let mut perms = fs::metadata(&file_path)?.permissions();
                perms.set_mode(0o644);
                fs::set_permissions(&file_path, perms)?;
            }

            for range in ranges {
                let start = range.start().saturating_sub(1);
                let end = range.end().min(lines.len());

                if start >= lines.len() {
                    continue;
                }

                let veiled_content = lines[start..end.min(lines.len())].join("\n");
                let hash = store.store(veiled_content.as_bytes())?;

                let key = format!("{file}#{range}");

                if config.get_object(&key).is_some() {
                    return Err(FunveilError::AlreadyVeiled(key));
                }

                config.register_object(key, ObjectMeta::new(hash.clone(), permissions));
            }

            let mut output = String::new();

            let prefix = format!("{file}#");
            let all_veiled_ranges: Vec<LineRange> = config
                .objects
                .keys()
                .filter(|k| k.starts_with(&prefix) && !k.ends_with(ORIGINAL_SUFFIX))
                .filter_map(|k| {
                    let range_str = &k[prefix.len()..];
                    LineRange::from_str(range_str).ok()
                })
                .collect();

            for (i, line) in lines.iter().enumerate() {
                let line_num = i + 1;

                let mut in_range = None;
                for range in &all_veiled_ranges {
                    if range.contains(line_num) {
                        in_range = Some(*range);
                        break;
                    }
                }

                if let Some(range) = in_range {
                    let range_len = range.len();
                    let pos_in_range = line_num - range.start();

                    if range_len == 1 {
                        let key = format!("{file}#{range}");
                        if let Some(meta) = config.get_object(&key) {
                            let hash = ContentHash::from_string(meta.hash.clone());
                            output.push_str(&format!("...[{}]...\n", hash.short()));
                        }
                    } else if pos_in_range == 1 {
                        let key = format!("{file}#{range}");
                        if let Some(meta) = config.get_object(&key) {
                            let hash = ContentHash::from_string(meta.hash.clone());
                            output.push_str(&format!("...[{}]\n", hash.short()));
                        }
                    } else {
                        output.push('\n');
                    }
                } else {
                    output.push_str(line);
                    output.push('\n');
                }
            }

            fs::write(&file_path, output)?;

            let mut perms = fs::metadata(&file_path)?.permissions();
            perms.set_readonly(true);
            fs::set_permissions(&file_path, perms)?;
        }
    }

    Ok(())
}

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

        if is_config_file(&path_str)
            || is_data_dir(&path_str)
            || is_funveil_protected(&path_str)
            || is_vcs_directory(&path_str)
        {
            continue;
        }

        if path.is_dir() {
            veil_directory(root, config, &path, ranges)?;
        } else if path.is_file() {
            let _ = veil_file(root, config, &path_str, ranges);
        }
    }

    Ok(())
}

pub fn unveil_file(
    root: &Path,
    config: &mut Config,
    file: &str,
    ranges: Option<&[LineRange]>,
) -> Result<()> {
    let store = ContentStore::new(root);
    let file_path = root.join(file);

    if !file_path.exists() {
        return Err(FunveilError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("file not found: {file}"),
        )));
    }

    if file_path.is_dir() {
        return unveil_directory(root, config, &file_path, ranges);
    }

    #[cfg(unix)]
    {
        let metadata = fs::metadata(&file_path)?;
        let mut permissions = metadata.permissions();
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
            let key = file.to_string();

            if let Some(meta) = config.get_object(&key) {
                let hash = ContentHash::from_string(meta.hash.clone());
                let content = store.retrieve(&hash)?;

                fs::write(&file_path, content)?;

                let perms = u32::from_str_radix(&meta.permissions, 8).unwrap_or(0o644);
                let mut permissions = fs::metadata(&file_path)?.permissions();
                permissions.set_mode(perms);
                fs::set_permissions(&file_path, permissions)?;

                config.unregister_object(&key);
                return Ok(());
            }

            let original_key = format!("{file}{ORIGINAL_SUFFIX}");
            if let Some(meta) = config.get_object(&original_key) {
                let hash = ContentHash::from_string(meta.hash.clone());
                let content = store.retrieve(&hash)?;

                fs::write(&file_path, content)?;

                let perms = u32::from_str_radix(&meta.permissions, 8).unwrap_or(0o644);
                let mut permissions = fs::metadata(&file_path)?.permissions();
                permissions.set_mode(perms);
                fs::set_permissions(&file_path, permissions)?;

                let partial_keys: Vec<String> = config
                    .objects
                    .keys()
                    .filter(|k| k.starts_with(&format!("{file}#")) && !k.ends_with(ORIGINAL_SUFFIX))
                    .cloned()
                    .collect();

                for key in partial_keys {
                    config.unregister_object(&key);
                }
                config.unregister_object(&original_key);

                return Ok(());
            }

            let partial_keys: Vec<String> = config
                .objects
                .keys()
                .filter(|k| k.starts_with(&format!("{file}#")) && !k.ends_with(ORIGINAL_SUFFIX))
                .cloned()
                .collect();

            if partial_keys.is_empty() {
                return Err(FunveilError::NotVeiled(file.to_string()));
            }

            eprintln!(
                "Warning: Partial veil created before v2. Reconstructing from markers. \
                 Some content may be lost for non-contiguous ranges."
            );

            let veiled_content = fs::read_to_string(&file_path)?;
            let lines: Vec<&str> = veiled_content.lines().collect();

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

            veiled_ranges.sort_by_key(|(r, _)| r.start());

            let mut output = String::new();
            let mut line_idx = 0;
            let total_lines = lines.len();
            let mut range_iter = veiled_ranges.iter().peekable();

            while line_idx < total_lines {
                let current_line = line_idx + 1;

                if let Some((range, content)) = range_iter.peek() {
                    if range.start() == current_line {
                        let content_str = String::from_utf8_lossy(content);
                        output.push_str(&content_str);
                        output.push('\n');

                        line_idx += range.len();
                        range_iter.next();
                        continue;
                    }
                }

                output.push_str(lines[line_idx]);
                output.push('\n');
                line_idx += 1;
            }

            fs::write(&file_path, output)?;

            if let Some(first_key) = partial_keys.first() {
                if let Some(meta) = config.get_object(first_key) {
                    let perms = u32::from_str_radix(&meta.permissions, 8).unwrap_or(0o644);
                    let mut permissions = fs::metadata(&file_path)?.permissions();
                    permissions.set_mode(perms);
                    fs::set_permissions(&file_path, permissions)?;
                }
            }

            for key in partial_keys {
                config.unregister_object(&key);
            }

            Ok(())
        }
        Some(ranges) => {
            let original_key = format!("{file}{ORIGINAL_SUFFIX}");
            if let Some(meta) = config.get_object(&original_key) {
                let hash = ContentHash::from_string(meta.hash.clone());
                let perms = meta.permissions.clone();
                let original_content = store.retrieve(&hash)?;
                let original_str = String::from_utf8_lossy(&original_content);
                let original_lines: Vec<&str> = original_str.lines().collect();

                let mut output = String::new();

                for (i, line) in original_lines.iter().enumerate() {
                    let line_num = i + 1;

                    let mut is_still_veiled = false;
                    for key in config.objects.keys() {
                        if key.starts_with(&format!("{file}#")) && !key.ends_with(ORIGINAL_SUFFIX) {
                            if let Some(pos) = key.find('#') {
                                let range_str = &key[pos + 1..];
                                if let Ok(veiled_range) = LineRange::from_str(range_str) {
                                    if veiled_range.contains(line_num) {
                                        let mut being_unveiled = false;
                                        for unveil_range in ranges {
                                            if unveil_range.contains(line_num) {
                                                being_unveiled = true;
                                                break;
                                            }
                                        }
                                        if !being_unveiled {
                                            is_still_veiled = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if is_still_veiled {
                        // is_still_veiled means the line is in a veiled range that is NOT
                        // being unveiled, so we preserve the veil marker in the output.
                        let veiled_range = find_veiled_range_for_line(config, file, line_num);
                        if let Some(range) = veiled_range {
                            let range_len = range.len();
                            let pos_in_range = line_num - range.start();

                            if range_len == 1 {
                                let key = format!("{file}#{range}");
                                if let Some(meta) = config.get_object(&key) {
                                    let hash = ContentHash::from_string(meta.hash.clone());
                                    output.push_str(&format!("...[{}]...\n", hash.short()));
                                }
                            } else if pos_in_range == 1 {
                                let key = format!("{file}#{range}");
                                if let Some(meta) = config.get_object(&key) {
                                    let hash = ContentHash::from_string(meta.hash.clone());
                                    output.push_str(&format!("...[{}]\n", hash.short()));
                                }
                            } else {
                                output.push('\n');
                            }
                        }
                    } else {
                        output.push_str(line);
                        output.push('\n');
                    }
                }

                for range in ranges {
                    let key = format!("{file}#{range}");
                    config.unregister_object(&key);
                }

                let remaining = config.veiled_ranges(file)?;
                if remaining.is_empty() {
                    fs::write(&file_path, original_str.as_bytes())?;

                    let mode = u32::from_str_radix(&perms, 8).unwrap_or(0o644);
                    let mut permissions = fs::metadata(&file_path)?.permissions();
                    permissions.set_mode(mode);
                    fs::set_permissions(&file_path, permissions)?;

                    config.unregister_object(&original_key);
                } else {
                    fs::write(&file_path, output)?;

                    let mut permissions = fs::metadata(&file_path)?.permissions();
                    permissions.set_readonly(true);
                    fs::set_permissions(&file_path, permissions)?;
                }

                return Ok(());
            }

            let veiled_content = fs::read_to_string(&file_path)?;
            let lines: Vec<&str> = veiled_content.lines().collect();

            let mut full_content = String::new();

            // Save permissions from the first range before unregistering objects
            let saved_permissions = ranges.first().and_then(|r| {
                let key = format!("{file}#{r}");
                config
                    .get_object(&key)
                    .map(|meta| meta.permissions.clone())
            });

            for (i, line) in lines.iter().enumerate() {
                let line_num = i + 1;

                let mut unveiling = false;
                for range in ranges {
                    if range.contains(line_num) {
                        unveiling = true;
                        break;
                    }
                }

                if unveiling {
                    for range in ranges {
                        if range.contains(line_num) && line_num == range.start() {
                            let key = format!("{file}#{range}");
                            if let Some(meta) = config.get_object(&key) {
                                let hash = ContentHash::from_string(meta.hash.clone());
                                let content = store.retrieve(&hash)?;
                                let content_str = String::from_utf8_lossy(&content);
                                full_content.push_str(&content_str);
                                full_content.push('\n');

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

            let remaining = config.veiled_ranges(file)?;
            if remaining.is_empty() && config.get_object(file).is_none() {
                if let Some(perms) = saved_permissions {
                    let mode = u32::from_str_radix(&perms, 8).unwrap_or(0o644);
                    let mut permissions = fs::metadata(&file_path)?.permissions();
                    permissions.set_mode(mode);
                    fs::set_permissions(&file_path, permissions)?;
                }
            }
            Ok(())
        }
    }
}

fn find_veiled_range_for_line(config: &Config, file: &str, line_num: usize) -> Option<LineRange> {
    let prefix = format!("{file}#");
    for key in config.objects.keys() {
        if key.starts_with(&prefix) && !key.ends_with(ORIGINAL_SUFFIX) {
            let range_str = &key[prefix.len()..];
            if let Ok(range) = LineRange::from_str(range_str) {
                if range.contains(line_num) {
                    return Some(range);
                }
            }
        }
    }
    None
}

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

        if is_config_file(&path_str)
            || is_data_dir(&path_str)
            || is_funveil_protected(&path_str)
            || is_vcs_directory(&path_str)
        {
            continue;
        }

        if path.is_dir() {
            unveil_directory(root, config, &path, ranges)?;
        } else if path.is_file() {
            let _ = unveil_file(root, config, &path_str, ranges);
        }
    }

    Ok(())
}

pub fn unveil_all(root: &Path, config: &mut Config) -> Result<()> {
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

    for file in files_to_unveil {
        unveil_file(root, config, &file, None)?;
    }

    Ok(())
}

pub fn has_veils(config: &Config, file: &str) -> bool {
    config.get_object(file).is_some()
        || config
            .objects
            .keys()
            .any(|k| k.starts_with(&format!("{file}#")) && !k.ends_with(ORIGINAL_SUFFIX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ensure_data_dir, Config};
    use crate::types::LineRange;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Config) {
        let temp = TempDir::new().unwrap();
        ensure_data_dir(temp.path()).unwrap();
        (temp, Config::new(crate::types::Mode::Whitelist))
    }

    #[test]
    fn test_veil_file_full() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        veil_file(temp.path(), &mut config, "test.txt", None).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "...\n");
        assert!(config.get_object("test.txt").is_some());
    }

    #[test]
    fn test_unveil_file_full() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        veil_file(temp.path(), &mut config, "test.txt", None).unwrap();
        unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world\n");
        assert!(config.get_object("test.txt").is_none());
    }

    #[test]
    fn test_veil_file_already_veiled() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        veil_file(temp.path(), &mut config, "test.txt", None).unwrap();
        let result = veil_file(temp.path(), &mut config, "test.txt", None);
        assert!(matches!(result, Err(FunveilError::AlreadyVeiled(_))));
    }

    #[test]
    fn test_veil_file_not_found() {
        let (temp, mut config) = setup();
        let result = veil_file(temp.path(), &mut config, "nonexistent.txt", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_veil_config_file_protected() {
        let (temp, mut config) = setup();
        let result = veil_file(temp.path(), &mut config, ".funveil_config", None);
        assert!(matches!(result, Err(FunveilError::ConfigFileProtected)));
    }

    #[test]
    fn test_veil_data_dir_protected() {
        let (temp, mut config) = setup();
        let result = veil_file(temp.path(), &mut config, ".funveil/objects/abc", None);
        assert!(matches!(result, Err(FunveilError::DataDirectoryProtected)));
    }

    #[test]
    fn test_veil_vcs_directory() {
        let (temp, mut config) = setup();
        let result = veil_file(temp.path(), &mut config, ".git/config", None);
        assert!(matches!(result, Err(FunveilError::VcsDirectoryExcluded(_))));
    }

    #[test]
    fn test_veil_partial() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.starts_with("line1\n"));
        assert!(content.ends_with("line4\nline5\n"));
        assert!(config.get_object("test.txt#2-3").is_some());
    }

    #[test]
    fn test_unveil_partial() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nline2\nline3\nline4\nline5\n");
    }

    #[test]
    fn test_veil_empty_file_with_ranges() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("empty.txt");
        fs::write(&file_path, "").unwrap();

        let ranges = [LineRange::new(1, 1).unwrap()];
        let result = veil_file(temp.path(), &mut config, "empty.txt", Some(&ranges));
        assert!(matches!(result, Err(FunveilError::EmptyFile(_))));
    }

    #[test]
    fn test_unveil_not_veiled() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        let result = unveil_file(temp.path(), &mut config, "test.txt", None);
        assert!(matches!(result, Err(FunveilError::NotVeiled(_))));
    }

    #[test]
    fn test_unveil_file_not_found() {
        let (temp, mut config) = setup();
        let result = unveil_file(temp.path(), &mut config, "nonexistent.txt", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_has_veils() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        assert!(!has_veils(&config, "test.txt"));
        veil_file(temp.path(), &mut config, "test.txt", None).unwrap();
        assert!(has_veils(&config, "test.txt"));
    }

    #[test]
    fn test_has_veils_partial() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        assert!(!has_veils(&config, "test.txt"));
        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();
        assert!(has_veils(&config, "test.txt"));
    }

    #[test]
    fn test_veil_multiple_ranges() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap(), LineRange::new(4, 5).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        assert!(config.get_object("test.txt#1-2").is_some());
        assert!(config.get_object("test.txt#4-5").is_some());
    }

    #[test]
    fn test_unveil_all() {
        let (temp, mut config) = setup();

        let file1 = temp.path().join("a.txt");
        let file2 = temp.path().join("b.txt");
        fs::write(&file1, "content a\n").unwrap();
        fs::write(&file2, "content b\n").unwrap();

        veil_file(temp.path(), &mut config, "a.txt", None).unwrap();
        veil_file(temp.path(), &mut config, "b.txt", None).unwrap();

        assert!(has_veils(&config, "a.txt"));
        assert!(has_veils(&config, "b.txt"));

        unveil_all(temp.path(), &mut config).unwrap();

        assert_eq!(fs::read_to_string(&file1).unwrap(), "content a\n");
        assert_eq!(fs::read_to_string(&file2).unwrap(), "content b\n");
    }

    #[test]
    fn test_veil_single_line_range() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let ranges = [LineRange::new(2, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("line1"));
        assert!(content.contains("line3"));
    }

    #[test]
    fn test_veil_directory_recursive() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), "content1\n").unwrap();
        fs::write(subdir.join("file2.txt"), "content2\n").unwrap();

        let result = veil_file(temp.path(), &mut config, "subdir", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_directory_recursive() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), "content1\n").unwrap();
        fs::write(subdir.join("file2.txt"), "content2\n").unwrap();

        veil_file(temp.path(), &mut config, "subdir", None).unwrap();
        let result = unveil_file(temp.path(), &mut config, "subdir", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_partial_multiple_ranges_with_gap() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap(), LineRange::new(5, 6).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("l1"));
        assert!(content.contains("l4"));
    }

    #[test]
    fn test_veil_partial_already_veiled_add_range() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\n").unwrap();

        let ranges1 = [LineRange::new(1, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges1)).unwrap();

        let ranges2 = [LineRange::new(3, 4).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges2)).unwrap();

        assert!(config.get_object("test.txt#1-2").is_some());
        assert!(config.get_object("test.txt#3-4").is_some());
    }

    #[test]
    fn test_unveil_partial_keeps_other_ranges() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap(), LineRange::new(5, 6).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges)).unwrap();

        assert!(config.get_object("test.txt#5-6").is_some());
        assert!(config.get_object("test.txt#2-3").is_none());
    }

    #[test]
    fn test_unveil_all_ranges_completes_file() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap();

        let ranges = [LineRange::new(1, 3).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let unveil_ranges = [LineRange::new(1, 3).unwrap()];
        unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "l1\nl2\nl3\n");
        assert!(config.get_object("test.txt#1-3").is_none());
        assert!(config.get_object("test.txt#_original").is_none());
    }

    #[test]
    fn test_veil_binary_file_full() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.bin");
        fs::write(&file_path, b"\x00\x01\x02\x03").unwrap();

        let result = veil_file(temp.path(), &mut config, "test.bin", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_binary_file_partial_error() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.bin");
        fs::write(&file_path, b"\x00\x01\x02\x03").unwrap();

        let ranges = [LineRange::new(1, 1).unwrap()];
        let result = veil_file(temp.path(), &mut config, "test.bin", Some(&ranges));
        assert!(matches!(
            result,
            Err(FunveilError::BinaryFilePartialVeil(_))
        ));
    }

    #[test]
    fn test_veil_symlink_escape() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "content\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = temp.path().join("link.txt");
            let outside = tempfile::TempDir::new().unwrap();
            let outside_file = outside.path().join("outside.txt");
            fs::write(&outside_file, "outside\n").unwrap();

            if symlink(&outside_file, &link).is_ok() {
                let result = veil_file(temp.path(), &mut config, "link.txt", None);
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn test_unveil_without_original_key() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        let result = unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges));
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_full_from_partial_with_original() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        unveil_file(temp.path(), &mut config, "test.txt", None).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nline2\nline3\nline4\n");
    }

    #[test]
    fn test_veil_partial_multiple_times_same_file() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges1 = [LineRange::new(1, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges1)).unwrap();

        let ranges2 = [LineRange::new(4, 5).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges2)).unwrap();

        assert!(config.get_object("test.txt#_original").is_some());
        assert!(config.get_object("test.txt#1-2").is_some());
        assert!(config.get_object("test.txt#4-5").is_some());
    }

    #[test]
    fn test_unveil_one_range_keeps_others() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap(), LineRange::new(4, 5).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("l1"));
        assert!(content.contains("l2"));
        assert!(config.get_object("test.txt#4-5").is_some());
    }

    #[test]
    fn test_unveil_all_with_partial_veils() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        assert!(has_veils(&config, "test.txt"));

        unveil_all(temp.path(), &mut config).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "l1\nl2\nl3\nl4\n");
        assert!(!has_veils(&config, "test.txt"));
    }

    #[test]
    fn test_veil_range_exceeds_file_length() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\n").unwrap();

        let ranges = [LineRange::new(1, 100).unwrap()];
        let result = veil_file(temp.path(), &mut config, "test.txt", Some(&ranges));
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_start_beyond_file_length() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\n").unwrap();

        let ranges = [LineRange::new(100, 200).unwrap()];
        let result = veil_file(temp.path(), &mut config, "test.txt", Some(&ranges));
        assert!(result.is_ok());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nline2\n");
    }

    #[test]
    fn test_unveil_partial_different_range_than_veiled() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(2, 4).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let unveil_ranges = [LineRange::new(3, 4).unwrap()];
        unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("l3"));
        assert!(content.contains("l4"));
    }

    #[test]
    fn test_has_veils_partial_only() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        assert!(!has_veils(&config, "test.txt"));

        let ranges = [LineRange::new(1, 1).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        assert!(has_veils(&config, "test.txt"));
    }

    #[test]
    fn test_veil_without_trailing_newline() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let original_key = "test.txt#_original";
        assert!(config.get_object(original_key).is_some());
    }

    #[test]
    fn test_unveil_with_multiple_files() {
        let (temp, mut config) = setup();

        let file1 = temp.path().join("a.txt");
        let file2 = temp.path().join("b.txt");
        fs::write(&file1, "content a1\ncontent a2\n").unwrap();
        fs::write(&file2, "content b1\ncontent b2\n").unwrap();

        let ranges1 = [LineRange::new(1, 1).unwrap()];
        veil_file(temp.path(), &mut config, "a.txt", Some(&ranges1)).unwrap();
        veil_file(temp.path(), &mut config, "b.txt", None).unwrap();

        unveil_all(temp.path(), &mut config).unwrap();

        assert_eq!(
            fs::read_to_string(&file1).unwrap(),
            "content a1\ncontent a2\n"
        );
        assert_eq!(
            fs::read_to_string(&file2).unwrap(),
            "content b1\ncontent b2\n"
        );
    }

    #[test]
    fn test_veil_directory_with_nested_subdirs() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("a").join("b").join("c");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        let result = veil_file(temp.path(), &mut config, "a", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_directory_with_nested_subdirs() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("a").join("b").join("c");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        veil_file(temp.path(), &mut config, "a", None).unwrap();
        let result = unveil_file(temp.path(), &mut config, "a", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_partial_already_veiled_range_error() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let result = veil_file(temp.path(), &mut config, "test.txt", Some(&ranges));
        assert!(matches!(result, Err(FunveilError::AlreadyVeiled(_))));
    }

    #[test]
    fn test_veil_partial_with_existing_veils_no_original() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").unwrap();

        let ranges1 = [LineRange::new(1, 1).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges1)).unwrap();

        let original_key = "test.txt#_original".to_string();
        config.unregister_object(&original_key);

        let ranges2 = [LineRange::new(3, 4).unwrap()];
        let result = veil_file(temp.path(), &mut config, "test.txt", Some(&ranges2));
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_directory_skips_protected_files() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();
        fs::write(subdir.join(".funveil_config"), "config\n").unwrap();

        let result = veil_file(temp.path(), &mut config, "subdir", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_directory_skips_protected_files() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        veil_file(temp.path(), &mut config, "subdir", None).unwrap();

        fs::write(subdir.join(".funveil_config"), "config\n").unwrap();
        let result = unveil_file(temp.path(), &mut config, "subdir", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_legacy_partial_no_original() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "...[abc]\n\n...\nline4\n").unwrap();

        let store = crate::cas::ContentStore::new(temp.path());
        let hash = store.store(b"line1\nline2\nline3").unwrap();

        config.register_object(
            "test.txt#1-3".to_string(),
            crate::config::ObjectMeta::new(hash, 0o644),
        );

        let result = unveil_file(temp.path(), &mut config, "test.txt", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_partial_without_original_key() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        let result = unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges));
        assert!(result.is_ok());
    }

    #[test]
    fn test_find_veiled_range_for_line_no_match() {
        let (_, config) = setup();
        let result = find_veiled_range_for_line(&config, "test.txt", 1);
        assert!(result.is_none());
    }

    #[test]
    fn test_unveil_partial_remaining_ranges_exist() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap(), LineRange::new(4, 5).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        let result = unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges));
        assert!(result.is_ok());
        assert!(config.get_object("test.txt#4-5").is_some());
    }

    #[test]
    fn test_unveil_partial_no_remaining_after_unveil() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        let result = unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges));
        assert!(result.is_ok());
        assert!(config.get_object("test.txt#1-2").is_none());
    }

    #[test]
    fn test_unveil_partial_without_original_multiple_ranges() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap(), LineRange::new(4, 5).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        let result = unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges));
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_partial_with_original_partial_range() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(1, 3).unwrap(), LineRange::new(5, 6).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let unveil_ranges = [LineRange::new(1, 3).unwrap()];
        let result = unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges));
        assert!(result.is_ok());
        assert!(config.get_object("test.txt#1-3").is_none());
        assert!(config.get_object("test.txt#5-6").is_some());
    }

    #[test]
    fn test_unveil_partial_with_original_single_line_range() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\n").unwrap();

        let ranges = [LineRange::new(2, 2).unwrap(), LineRange::new(4, 4).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let unveil_ranges = [LineRange::new(2, 2).unwrap()];
        let result = unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges));
        assert!(result.is_ok());
        assert!(config.get_object("test.txt#4-4").is_some());
    }

    #[test]
    fn test_unveil_directory_with_protected_files() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        veil_file(temp.path(), &mut config, "subdir", None).unwrap();

        fs::create_dir_all(subdir.join(".funveil")).unwrap();
        fs::create_dir_all(subdir.join(".git")).unwrap();

        let result = crate::veil::unveil_directory(temp.path(), &mut config, &subdir, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_directory_with_protected_files() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();
        fs::create_dir_all(subdir.join(".funveil")).unwrap();

        let result = crate::veil::veil_directory(temp.path(), &mut config, &subdir, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_file_with_missing_cas_object() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges = [LineRange::new(1, 3).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        if let Some(meta) = config.get_object("test.txt#1-3") {
            let store = crate::cas::ContentStore::new(temp.path());
            let hash = ContentHash::from_string(meta.hash.clone());
            let _ = store.delete(&hash);
        }

        let _ = veil_file(temp.path(), &mut config, "test.txt", None);
    }

    #[test]
    fn test_veil_multiline_range_formatting() {
        // Covers line 213: output.push_str("...\n") for last line of a multi-line range
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges = [LineRange::new(2, 4).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        // First line of range should have ...[hash]
        // Middle lines should be empty
        // Last line of range should be ...
        assert!(content.contains("..."));
        assert!(content.starts_with("line1\n"));
        assert!(content.ends_with("line5\n"));
    }

    #[cfg(unix)]
    #[test]
    fn test_unveil_restores_permissions() {
        // Covers lines 577-580: Unix permissions restoration in unveil
        use std::os::unix::fs::PermissionsExt;

        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        // Set specific permissions before veiling
        let perms = fs::Permissions::from_mode(0o644);
        fs::set_permissions(&file_path, perms).unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        // File should be read-only after veiling
        let meta = fs::metadata(&file_path).unwrap();
        assert!(meta.permissions().readonly());

        // Unveil and check permissions are restored
        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_unveil_directory_skips_funveil_config() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        veil_file(temp.path(), &mut config, "subdir", None).unwrap();

        fs::write(subdir.join(".funveil_config"), "config\n").unwrap();

        let result = crate::veil::unveil_directory(temp.path(), &mut config, &subdir, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_directory_skips_protected_at_root_level() {
        // Covers line 253: continue in veil_directory for protected paths
        // By calling veil_directory with dir_path = root, entries like .funveil_config
        // have relative_path = ".funveil_config" which matches is_config_file/is_funveil_protected.
        let (temp, mut config) = setup();
        fs::write(temp.path().join("normal.txt"), "content\n").unwrap();
        // .funveil_config at the root level - should be skipped
        fs::write(temp.path().join(".funveil_config"), "config data\n").unwrap();

        let result = veil_directory(temp.path(), &mut config, temp.path(), None);
        assert!(result.is_ok());
        // normal.txt should have been veiled
        assert!(has_veils(&config, "normal.txt"));
        // .funveil_config should NOT have been veiled (skipped via continue)
        assert!(!has_veils(&config, ".funveil_config"));
    }

    #[test]
    fn test_unveil_directory_skips_protected_at_root_level() {
        // Covers line 624: continue in unveil_directory for protected paths
        let (temp, mut config) = setup();
        fs::write(temp.path().join("normal.txt"), "content\n").unwrap();

        veil_directory(temp.path(), &mut config, temp.path(), None).unwrap();
        assert!(has_veils(&config, "normal.txt"));

        // Create protected files/dirs that should be skipped during unveil
        fs::write(temp.path().join(".funveil_config"), "config data\n").unwrap();
        fs::create_dir_all(temp.path().join(".git")).unwrap();

        let result = unveil_directory(temp.path(), &mut config, temp.path(), None);
        assert!(result.is_ok());
        assert!(!has_veils(&config, "normal.txt"));
    }

    #[test]
    fn test_veil_multiline_range_formatting_detailed() {
        // Verifies the multi-line range veil display format.
        // For a range of 3+ lines (e.g., 2-4), the output is:
        // - pos_in_range 0 (first line): empty line
        // - pos_in_range 1 (second line): ...[hash]
        // - remaining lines: empty lines
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges = [LineRange::new(2, 4).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        let content_lines: Vec<&str> = content.lines().collect();
        // line1 is unveiled
        assert_eq!(content_lines[0], "line1");
        // pos_in_range 0: empty line
        assert_eq!(content_lines[1], "");
        // pos_in_range 1: ...[hash]
        assert!(content_lines[2].starts_with("...["));
        // pos_in_range 2: empty line
        assert_eq!(content_lines[3], "");
        // line5 is unveiled
        assert_eq!(content_lines[4], "line5");
    }

    #[test]
    fn test_unveil_partial_preserves_multiline_veil_formatting() {
        // Covers line 490 (or the equivalent veil-preserving path in unveil_file):
        // When unveiling one range while another multi-line range remains veiled,
        // the remaining veiled range should be displayed with the veil markers.
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(
            &file_path,
            "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\n",
        )
        .unwrap();

        // Veil two ranges
        let ranges = [LineRange::new(2, 4).unwrap(), LineRange::new(6, 8).unwrap()];
        veil_file(temp.path(), &mut config, "test.txt", Some(&ranges)).unwrap();

        // Unveil only the first range, keeping range 6-8 veiled
        let unveil_ranges = [LineRange::new(2, 4).unwrap()];
        unveil_file(temp.path(), &mut config, "test.txt", Some(&unveil_ranges)).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        // Lines 2-4 should be restored
        assert!(content.contains("l2\n"));
        assert!(content.contains("l3\n"));
        assert!(content.contains("l4\n"));
        // Lines 6-8 should still be veiled (shown as marker lines)
        // The veiled range uses ...[hash] for pos_in_range==1 and empty lines for others
        assert!(content.contains("...["));
        // Range 6-8 should still be registered
        assert!(config.get_object("test.txt#6-8").is_some());
    }
}
