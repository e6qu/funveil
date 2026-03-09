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
                            format!("{:o}", permissions),
                            trailing,
                        )
                    }
                } else {
                    let trailing = content.ends_with('\n');
                    (
                        content.lines().map(|s| s.to_string()).collect(),
                        format!("{:o}", permissions),
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

            let all_veiled_ranges: Vec<LineRange> = config
                .objects
                .keys()
                .filter_map(|k| {
                    if k.starts_with(&format!("{file}#")) && !k.ends_with(ORIGINAL_SUFFIX) {
                        if let Some(pos) = k.find('#') {
                            let range_str = &k[pos + 1..];
                            LineRange::from_str(range_str).ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
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
                        let meta = config.get_object(&key).unwrap();
                        let hash = ContentHash::from_string(meta.hash.clone());
                        output.push_str(&format!("...[{}]...\n", hash.short()));
                    } else if pos_in_range == 1 {
                        let key = format!("{file}#{range}");
                        let meta = config.get_object(&key).unwrap();
                        let hash = ContentHash::from_string(meta.hash.clone());
                        output.push_str(&format!("...[{}]\n", hash.short()));
                    } else if pos_in_range == range_len {
                        output.push_str("...\n");
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

            if let Some(meta) = config.get_object(&partial_keys[0]) {
                let perms = u32::from_str_radix(&meta.permissions, 8).unwrap_or(0o644);
                let mut permissions = fs::metadata(&file_path)?.permissions();
                permissions.set_mode(perms);
                fs::set_permissions(&file_path, permissions)?;
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
                        let mut in_unveiling_range = None;
                        for range in ranges {
                            if range.contains(line_num) {
                                in_unveiling_range = Some(range);
                                break;
                            }
                        }

                        if in_unveiling_range.is_none() {
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
                                } else if pos_in_range == range_len {
                                    output.push_str("...\n");
                                } else {
                                    output.push('\n');
                                }
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
            Ok(())
        }
    }
}

fn find_veiled_range_for_line(config: &Config, file: &str, line_num: usize) -> Option<LineRange> {
    for key in config.objects.keys() {
        if key.starts_with(&format!("{file}#")) && !key.ends_with(ORIGINAL_SUFFIX) {
            if let Some(pos) = key.find('#') {
                let range_str = &key[pos + 1..];
                if let Ok(range) = LineRange::from_str(range_str) {
                    if range.contains(line_num) {
                        return Some(range);
                    }
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

pub fn is_veiled(config: &Config, file: &str) -> bool {
    config.get_object(file).is_some()
        || config
            .objects
            .keys()
            .any(|k| k.starts_with(&format!("{file}#")) && !k.ends_with(ORIGINAL_SUFFIX))
}
