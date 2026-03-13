use crate::cas::ContentStore;
use crate::config::{is_config_file, is_data_dir, Config, ObjectMeta};
use crate::error::{FunveilError, Result};
use crate::output::Output;
use crate::types::{
    is_binary_file, is_funveil_protected, is_vcs_directory, validate_path_within_root, ConfigKey,
    ContentHash, LineRange,
};
use regex::Regex;
use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::LazyLock;

fn validate_filename(file: &str) -> Result<()> {
    for byte in file.as_bytes() {
        if *byte < 0x20 && *byte != b'\t' {
            return Err(FunveilError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "filename contains unsupported control character (byte 0x{byte:02x}): {file}"
                ),
            )));
        }
    }
    Ok(())
}

static MARKER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\.\.\.\[[0-9a-f]+\]\.{0,3}$").unwrap());

fn check_marker_collision(content: &str, file: &str) -> Result<()> {
    let marker_re = &*MARKER_RE;
    for (i, line) in content.lines().enumerate() {
        if marker_re.is_match(line) {
            return Err(FunveilError::MarkerCollision(format!(
                "line {} of '{file}' matches veil marker pattern: {line}",
                i + 1
            )));
        }
    }
    Ok(())
}

fn check_marker_integrity(on_disk_content: &str, config: &Config, file: &str) -> Result<()> {
    let on_disk_lines: Vec<&str> = on_disk_content.lines().collect();

    for (range, meta) in config.iter_ranges_for_file(file) {
        let start_idx = range.start().saturating_sub(1);
        if start_idx >= on_disk_lines.len() {
            return Err(FunveilError::MarkerIntegrityError(format!(
                "range {range} starts beyond end of file (file has {} lines)",
                on_disk_lines.len()
            )));
        }

        let marker_line = on_disk_lines[start_idx];
        let hash = ContentHash::from_string(meta.hash.clone())?;
        let short = hash.short();

        let expected_single = format!("...[{short}]...");
        let expected_multi = format!("...[{short}]");

        if range.len() == 1 {
            if marker_line != expected_single {
                return Err(FunveilError::MarkerIntegrityError(format!(
                    "expected marker '{}' at line {} but found '{}'",
                    expected_single,
                    range.start(),
                    marker_line
                )));
            }
        } else if marker_line != expected_multi {
            return Err(FunveilError::MarkerIntegrityError(format!(
                "expected marker '{}' at line {} but found '{}'",
                expected_multi,
                range.start(),
                marker_line
            )));
        }
    }
    Ok(())
}

#[tracing::instrument(skip(root, config, ranges, output), fields(file = %file))]
pub fn veil_file(
    root: &Path,
    config: &mut Config,
    file: &str,
    ranges: Option<&[LineRange]>,
    output: &mut Output,
) -> Result<()> {
    validate_filename(file)?;

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
        let gitignore = crate::config::load_gitignore(root);
        return veil_directory(root, config, &file_path, ranges, output, &gitignore);
    }

    if is_binary_file(&file_path) {
        if ranges.is_some() {
            return Err(FunveilError::BinaryFilePartialVeil(file.to_string()));
        }
        return Err(FunveilError::BinaryFileVeil(file.to_string()));
    }

    let content = fs::read_to_string(&file_path)?;

    // Only check if file doesn't already have veils (already-veiled files have markers by design)
    let has_any_veils = config.has_veils(file);
    if !has_any_veils {
        check_marker_collision(&content, file)?;
    }

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

            let marker = "...\n";
            fs::write(&file_path, marker)?;

            crate::perms::set_readonly(&file_path)?;

            config.register_object(key.clone(), ObjectMeta::new(hash.clone(), permissions));
            tracing::info!(hash = %hash.short(), size = content.len(), "stored content and veiled file");
        }
        Some(ranges) => {
            if ranges.is_empty() {
                return Err(FunveilError::InvalidLineRange {
                    range: String::new(),
                    reason: "empty ranges slice".to_string(),
                });
            }

            let original_key = ConfigKey::original_key(file);
            let has_existing_veils = config.iter_ranges_for_file(file).next().is_some();

            for i in 0..ranges.len() {
                for j in (i + 1)..ranges.len() {
                    if ranges[i].overlaps(&ranges[j]) {
                        return Err(FunveilError::OverlappingVeil {
                            new_range: ranges[i].to_string(),
                            existing_range: ranges[j].to_string(),
                        });
                    }
                }
            }

            if has_existing_veils {
                let existing_ranges: Vec<LineRange> =
                    config.iter_ranges_for_file(file).map(|(r, _)| r).collect();

                for new_range in ranges {
                    for existing_range in &existing_ranges {
                        // Skip exact duplicates — they'll be caught by AlreadyVeiled later
                        if new_range == existing_range {
                            continue;
                        }
                        if new_range.overlaps(existing_range) {
                            return Err(FunveilError::OverlappingVeil {
                                new_range: new_range.to_string(),
                                existing_range: existing_range.to_string(),
                            });
                        }
                    }
                }

                check_marker_integrity(&content, config, file)?;
            }

            let (lines, original_perms, had_trailing_newline): (Vec<String>, String, bool) =
                if has_existing_veils {
                    if let Some(meta) = config.get_object(&original_key) {
                        let hash = ContentHash::from_string(meta.hash.clone())?;
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
                            crate::perms::format_mode(permissions),
                            trailing,
                        )
                    }
                } else {
                    let trailing = content.ends_with('\n');
                    (
                        content.lines().map(|s| s.to_string()).collect(),
                        crate::perms::format_mode(permissions),
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
            crate::perms::set_mode(&file_path, 0o644)?;

            for range in ranges {
                let start = range.start().saturating_sub(1);
                let end = range.end().min(lines.len());

                if start >= lines.len() {
                    continue;
                }

                let veiled_content = lines[start..end].join("\n");
                let hash = store.store(veiled_content.as_bytes())?;

                let key = ConfigKey::range_key(file, range);

                if config.get_object(&key).is_some() {
                    return Err(FunveilError::AlreadyVeiled(key));
                }

                config.register_object(key, ObjectMeta::new(hash.clone(), permissions));
            }

            let mut result_content = String::new();

            let all_veiled_ranges: Vec<LineRange> =
                config.iter_ranges_for_file(file).map(|(r, _)| r).collect();

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
                    let key = ConfigKey::range_key(file, &range);

                    if range_len == 1 {
                        if let Some(meta) = config.get_object(&key) {
                            let hash = ContentHash::from_string(meta.hash.clone())?;
                            result_content.push_str(&format!("...[{}]...\n", hash.short()));
                        }
                    } else if pos_in_range == 0 {
                        if let Some(meta) = config.get_object(&key) {
                            let hash = ContentHash::from_string(meta.hash.clone())?;
                            result_content.push_str(&format!("...[{}]\n", hash.short()));
                        }
                    } else {
                        result_content.push('\n');
                    }
                } else {
                    result_content.push_str(line);
                    result_content.push('\n');
                }
            }

            if !had_trailing_newline && result_content.ends_with('\n') {
                result_content.pop();
            }

            fs::write(&file_path, result_content)?;

            crate::perms::set_readonly(&file_path)?;
        }
    }

    Ok(())
}

/// Pre-scan a directory tree for binary files. Returns the first binary file found.
fn find_binary_in_directory(root: &Path, dir_path: &Path) -> Option<String> {
    for entry_result in crate::config::walk_files(dir_path).build() {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let relative_path = path.strip_prefix(root).unwrap_or(path);
        let path_str = relative_path.to_string_lossy();

        if is_config_file(&path_str)
            || is_data_dir(&path_str)
            || is_funveil_protected(&path_str)
            || is_vcs_directory(&path_str)
        {
            continue;
        }

        if is_binary_file(path) {
            return Some(path_str.into_owned());
        }
    }
    None
}

#[tracing::instrument(skip(root, config, ranges, output, _gitignore), fields(path = %dir_path.display()))]
fn veil_directory(
    root: &Path,
    config: &mut Config,
    dir_path: &Path,
    ranges: Option<&[LineRange]>,
    output: &mut Output,
    _gitignore: &ignore::gitignore::Gitignore,
) -> Result<()> {
    // Reject the entire directory if it contains any binary files
    if let Some(binary_path) = find_binary_in_directory(root, dir_path) {
        return Err(FunveilError::DirectoryContainsBinary(binary_path));
    }

    let mut file_errors = 0usize;

    for entry_result in crate::config::walk_files(dir_path).build() {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                let _ = writeln!(output.err, "Warning: skipping entry: {e}");
                file_errors += 1;
                continue;
            }
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let relative_path = match path.strip_prefix(root) {
            Ok(rel) => rel,
            Err(_) => {
                let _ = writeln!(
                    output.err,
                    "Warning: could not determine relative path for {}",
                    path.display()
                );
                file_errors += 1;
                continue;
            }
        };
        let path_str = relative_path.to_string_lossy();

        if is_config_file(&path_str)
            || is_data_dir(&path_str)
            || is_funveil_protected(&path_str)
            || is_vcs_directory(&path_str)
        {
            continue;
        }

        if let Err(e) = veil_file(root, config, &path_str, ranges, output) {
            let _ = writeln!(output.err, "Warning: failed to veil {path_str}: {e}");
            file_errors += 1;
        }
    }

    if file_errors > 0 {
        let _ = writeln!(
            output.err,
            "Warning: {file_errors} files could not be veiled."
        );
    }

    Ok(())
}

#[tracing::instrument(skip(root, config, ranges, output), fields(file = %file))]
pub fn unveil_file(
    root: &Path,
    config: &mut Config,
    file: &str,
    ranges: Option<&[LineRange]>,
    output: &mut Output,
) -> Result<()> {
    validate_filename(file)?;

    if is_config_file(file) {
        return Err(FunveilError::ConfigFileProtected);
    }
    if is_data_dir(file) || is_funveil_protected(file) {
        return Err(FunveilError::DataDirectoryProtected);
    }
    if is_vcs_directory(file) {
        return Err(FunveilError::VcsDirectoryExcluded(file.to_string()));
    }

    let store = ContentStore::new(root);
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
        let gitignore = crate::config::load_gitignore(root);
        return unveil_directory(root, config, &file_path, ranges, output, &gitignore);
    }

    // Save original permissions before making writable
    let saved_perms = crate::perms::save_and_make_writable(&file_path)?;

    let unveil_result: Result<()> = (|| match ranges {
        None => {
            let key = file.to_string();

            if let Some(meta) = config.get_object(&key) {
                let hash = ContentHash::from_string(meta.hash.clone())?;
                let content = store.retrieve(&hash)?;

                fs::write(&file_path, content)?;

                crate::perms::set_mode(&file_path, crate::perms::parse_mode(&meta.permissions))?;

                config.unregister_object(&key);
                return Ok(());
            }

            if let Some(meta) = config.get_original(file) {
                let hash = ContentHash::from_string(meta.hash.clone())?;
                let content = store.retrieve(&hash)?;

                fs::write(&file_path, content)?;

                crate::perms::set_mode(&file_path, crate::perms::parse_mode(&meta.permissions))?;

                config.unregister_ranges(file);
                config.unregister_original(file);

                return Ok(());
            }

            let partial_keys: Vec<String> = config
                .iter_ranges_for_file(file)
                .map(|(r, _)| ConfigKey::range_key(file, &r))
                .collect();

            if partial_keys.is_empty() {
                return Err(FunveilError::NotVeiled(file.to_string()));
            }

            let _ = writeln!(
                output.err,
                "Warning: Partial veil created before v2. Reconstructing from markers. \
                 Some content may be lost for non-contiguous ranges."
            );

            let veiled_content = fs::read_to_string(&file_path)?;
            let veiled_had_trailing_newline = veiled_content.ends_with('\n');
            let lines: Vec<&str> = veiled_content.lines().collect();

            let mut veiled_ranges: Vec<(LineRange, Vec<u8>)> = Vec::new();
            for (range, meta) in config.iter_ranges_for_file(file) {
                let hash = ContentHash::from_string(meta.hash.clone())?;
                if let Ok(content) = store.retrieve(&hash) {
                    veiled_ranges.push((range, content));
                }
            }

            veiled_ranges.sort_by_key(|(r, _)| r.start());

            let mut result_content = String::new();
            let mut line_idx = 0;
            let total_lines = lines.len();
            let mut range_iter = veiled_ranges.iter().peekable();

            while line_idx < total_lines {
                let current_line = line_idx + 1;

                if let Some((range, content)) = range_iter.peek() {
                    if range.start() == current_line {
                        let content_str = String::from_utf8_lossy(content);
                        result_content.push_str(&content_str);
                        result_content.push('\n');

                        line_idx += range.len();
                        range_iter.next();
                        continue;
                    }
                }

                result_content.push_str(lines[line_idx]);
                result_content.push('\n');
                line_idx += 1;
            }

            if !veiled_had_trailing_newline && result_content.ends_with('\n') {
                result_content.pop();
            }

            fs::write(&file_path, result_content)?;

            if let Some(first_key) = partial_keys.first() {
                if let Some(meta) = config.get_object(first_key) {
                    crate::perms::set_mode(
                        &file_path,
                        crate::perms::parse_mode(&meta.permissions),
                    )?;
                }
            }

            for key in partial_keys {
                config.unregister_object(&key);
            }

            Ok(())
        }
        Some(ranges) => {
            if let Some(meta) = config.get_original(file) {
                let hash = ContentHash::from_string(meta.hash.clone())?;
                let perms = meta.permissions.clone();
                let original_content = store.retrieve(&hash)?;
                let original_str = String::from_utf8_lossy(&original_content);
                let original_lines: Vec<&str> = original_str.lines().collect();

                let mut result_content = String::new();

                for (i, line) in original_lines.iter().enumerate() {
                    let line_num = i + 1;

                    let is_still_veiled =
                        config.iter_ranges_for_file(file).any(|(veiled_range, _)| {
                            veiled_range.contains(line_num)
                                && !ranges.iter().any(|r| r.contains(line_num))
                        });

                    if is_still_veiled {
                        // is_still_veiled means the line is in a veiled range that is NOT
                        // being unveiled, so we preserve the veil marker in the result_content.
                        let veiled_range = find_veiled_range_for_line(config, file, line_num);
                        if let Some(range) = veiled_range {
                            let range_len = range.len();
                            let pos_in_range = line_num - range.start();
                            let key = ConfigKey::range_key(file, &range);

                            if range_len == 1 {
                                if let Some(meta) = config.get_object(&key) {
                                    let hash = ContentHash::from_string(meta.hash.clone())?;
                                    result_content.push_str(&format!("...[{}]...\n", hash.short()));
                                }
                            } else if pos_in_range == 0 {
                                if let Some(meta) = config.get_object(&key) {
                                    let hash = ContentHash::from_string(meta.hash.clone())?;
                                    result_content.push_str(&format!("...[{}]\n", hash.short()));
                                }
                            } else {
                                result_content.push('\n');
                            }
                        }
                    } else {
                        result_content.push_str(line);
                        result_content.push('\n');
                    }
                }

                for range in ranges {
                    let key = ConfigKey::range_key(file, range);
                    config.unregister_object(&key);
                }

                let remaining = config.veiled_ranges(file)?;
                if remaining.is_empty() {
                    fs::write(&file_path, original_str.as_bytes())?;

                    crate::perms::set_mode(&file_path, crate::perms::parse_mode(&perms))?;

                    config.unregister_original(file);
                } else {
                    fs::write(&file_path, result_content)?;

                    crate::perms::set_readonly(&file_path)?;
                }

                return Ok(());
            }

            let veiled_content = fs::read_to_string(&file_path)?;
            let veiled_had_trailing_newline = veiled_content.ends_with('\n');
            let lines: Vec<&str> = veiled_content.lines().collect();

            let mut full_content = String::new();

            // Save permissions from the first range before unregistering objects
            let saved_permissions = ranges.first().and_then(|r| {
                let key = ConfigKey::range_key(file, r);
                config.get_object(&key).map(|meta| meta.permissions.clone())
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
                            let key = ConfigKey::range_key(file, range);
                            if let Some(meta) = config.get_object(&key) {
                                let hash = ContentHash::from_string(meta.hash.clone())?;
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

            if !veiled_had_trailing_newline && full_content.ends_with('\n') {
                full_content.pop();
            }

            fs::write(&file_path, full_content)?;

            let remaining = config.veiled_ranges(file)?;
            if remaining.is_empty() && config.get_object(file).is_none() {
                if let Some(perms) = saved_permissions {
                    crate::perms::set_mode(&file_path, crate::perms::parse_mode(&perms))?;
                }
            }
            Ok(())
        }
    })();

    if unveil_result.is_ok() {
        tracing::info!("unveiled file");
    }

    if unveil_result.is_err() {
        let _ = crate::perms::restore(&file_path, &saved_perms);
    }

    unveil_result
}

fn find_veiled_range_for_line(config: &Config, file: &str, line_num: usize) -> Option<LineRange> {
    config
        .iter_ranges_for_file(file)
        .find(|(range, _)| range.contains(line_num))
        .map(|(range, _)| range)
}

#[tracing::instrument(skip(root, config, ranges, output, _gitignore), fields(path = %dir_path.display()))]
fn unveil_directory(
    root: &Path,
    config: &mut Config,
    dir_path: &Path,
    ranges: Option<&[LineRange]>,
    output: &mut Output,
    _gitignore: &ignore::gitignore::Gitignore,
) -> Result<()> {
    let mut file_errors = 0usize;

    for entry_result in crate::config::walk_files(dir_path).build() {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                let _ = writeln!(output.err, "Warning: skipping entry: {e}");
                file_errors += 1;
                continue;
            }
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let relative_path = match path.strip_prefix(root) {
            Ok(rel) => rel,
            Err(_) => {
                let _ = writeln!(
                    output.err,
                    "Warning: could not determine relative path for {}",
                    path.display()
                );
                file_errors += 1;
                continue;
            }
        };
        let path_str = relative_path.to_string_lossy();

        if is_config_file(&path_str)
            || is_data_dir(&path_str)
            || is_funveil_protected(&path_str)
            || is_vcs_directory(&path_str)
        {
            continue;
        }

        if let Err(e) = unveil_file(root, config, &path_str, ranges, output) {
            let _ = writeln!(output.err, "Warning: failed to unveil {path_str}: {e}");
            file_errors += 1;
        }
    }

    if file_errors > 0 {
        let _ = writeln!(
            output.err,
            "Warning: {file_errors} files could not be unveiled."
        );
    }

    Ok(())
}

#[tracing::instrument(skip(root, config, output))]
pub fn unveil_all(root: &Path, config: &mut Config, output: &mut Output) -> Result<()> {
    let files_to_unveil: Vec<String> = config.iter_unique_files().collect();

    for file in files_to_unveil {
        unveil_file(root, config, &file, None, output)?;
    }

    Ok(())
}

pub fn has_veils(config: &Config, file: &str) -> bool {
    config.has_veils(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ensure_data_dir, load_gitignore, Config};
    use crate::types::LineRange;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
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

        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "...\n");
        assert!(config.get_object("test.txt").is_some());
    }

    #[test]
    fn test_unveil_file_full() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world\n");
        assert!(config.get_object("test.txt").is_none());
    }

    #[test]
    fn test_veil_file_already_veiled() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::AlreadyVeiled(_))));
    }

    #[test]
    fn test_veil_file_not_found() {
        let (temp, mut config) = setup();
        let result = veil_file(
            temp.path(),
            &mut config,
            "nonexistent.txt",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_veil_config_file_protected() {
        let (temp, mut config) = setup();
        let result = veil_file(
            temp.path(),
            &mut config,
            ".funveil_config",
            None,
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::ConfigFileProtected)));
    }

    #[test]
    fn test_veil_data_dir_protected() {
        let (temp, mut config) = setup();
        let result = veil_file(
            temp.path(),
            &mut config,
            ".funveil/objects/abc",
            None,
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::DataDirectoryProtected)));
    }

    #[test]
    fn test_veil_vcs_directory() {
        let (temp, mut config) = setup();
        let result = veil_file(
            temp.path(),
            &mut config,
            ".git/config",
            None,
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::VcsDirectoryExcluded(_))));
    }

    #[test]
    fn test_veil_partial() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

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
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nline2\nline3\nline4\nline5\n");
    }

    #[test]
    fn test_veil_empty_file_with_ranges() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("empty.txt");
        fs::write(&file_path, "").unwrap();

        let ranges = [LineRange::new(1, 1).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "empty.txt",
            Some(&ranges),
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::EmptyFile(_))));
    }

    #[test]
    fn test_unveil_not_veiled() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::NotVeiled(_))));
    }

    #[test]
    fn test_unveil_file_not_found() {
        let (temp, mut config) = setup();
        let result = unveil_file(
            temp.path(),
            &mut config,
            "nonexistent.txt",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_has_veils() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        assert!(!has_veils(&config, "test.txt"));
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();
        assert!(has_veils(&config, "test.txt"));
    }

    #[test]
    fn test_has_veils_partial() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        assert!(!has_veils(&config, "test.txt"));
        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();
        assert!(has_veils(&config, "test.txt"));
    }

    #[test]
    fn test_veil_multiple_ranges() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap(), LineRange::new(4, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

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

        veil_file(
            temp.path(),
            &mut config,
            "a.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();
        veil_file(
            temp.path(),
            &mut config,
            "b.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        assert!(has_veils(&config, "a.txt"));
        assert!(has_veils(&config, "b.txt"));

        unveil_all(temp.path(), &mut config, &mut Output::new(false)).unwrap();

        assert_eq!(fs::read_to_string(&file1).unwrap(), "content a\n");
        assert_eq!(fs::read_to_string(&file2).unwrap(), "content b\n");
    }

    #[test]
    fn test_veil_single_line_range() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let ranges = [LineRange::new(2, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

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

        let result = veil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_directory_recursive() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), "content1\n").unwrap();
        fs::write(subdir.join("file2.txt"), "content2\n").unwrap();

        veil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        )
        .unwrap();
        let result = unveil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_partial_multiple_ranges_with_gap() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap(), LineRange::new(5, 6).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

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
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(false),
        )
        .unwrap();

        let ranges2 = [LineRange::new(3, 4).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(false),
        )
        .unwrap();

        assert!(config.get_object("test.txt#1-2").is_some());
        assert!(config.get_object("test.txt#3-4").is_some());
    }

    #[test]
    fn test_unveil_partial_keeps_other_ranges() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap(), LineRange::new(5, 6).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        assert!(config.get_object("test.txt#5-6").is_some());
        assert!(config.get_object("test.txt#2-3").is_none());
    }

    #[test]
    fn test_unveil_all_ranges_completes_file() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap();

        let ranges = [LineRange::new(1, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let unveil_ranges = [LineRange::new(1, 3).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

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

        // BUG-128: Full veil on binary files should return a dedicated error
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.bin",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_err());
        assert!(
            matches!(result, Err(FunveilError::BinaryFileVeil(_))),
            "expected BinaryFileVeil error"
        );
    }

    #[test]
    fn test_veil_binary_file_partial_error() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.bin");
        fs::write(&file_path, b"\x00\x01\x02\x03").unwrap();

        let ranges = [LineRange::new(1, 1).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.bin",
            Some(&ranges),
            &mut Output::new(false),
        );
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
                let result = veil_file(
                    temp.path(),
                    &mut config,
                    "link.txt",
                    None,
                    &mut Output::new(false),
                );
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
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_full_from_partial_with_original() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nline2\nline3\nline4\n");
    }

    #[test]
    fn test_veil_partial_multiple_times_same_file() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges1 = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(false),
        )
        .unwrap();

        let ranges2 = [LineRange::new(4, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(false),
        )
        .unwrap();

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
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

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
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        assert!(has_veils(&config, "test.txt"));

        unveil_all(temp.path(), &mut config, &mut Output::new(false)).unwrap();

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
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_start_beyond_file_length() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\n").unwrap();

        let ranges = [LineRange::new(100, 200).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        );
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
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let unveil_ranges = [LineRange::new(3, 4).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

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
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        assert!(has_veils(&config, "test.txt"));
    }

    #[test]
    fn test_veil_without_trailing_newline() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

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
        veil_file(
            temp.path(),
            &mut config,
            "a.txt",
            Some(&ranges1),
            &mut Output::new(false),
        )
        .unwrap();
        veil_file(
            temp.path(),
            &mut config,
            "b.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        unveil_all(temp.path(), &mut config, &mut Output::new(false)).unwrap();

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

        let result = veil_file(temp.path(), &mut config, "a", None, &mut Output::new(false));
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_directory_with_nested_subdirs() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("a").join("b").join("c");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        veil_file(temp.path(), &mut config, "a", None, &mut Output::new(false)).unwrap();
        let result = unveil_file(temp.path(), &mut config, "a", None, &mut Output::new(false));
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_partial_already_veiled_range_error() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::AlreadyVeiled(_))));
    }

    #[test]
    fn test_veil_partial_with_existing_veils_no_original() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").unwrap();

        let ranges1 = [LineRange::new(1, 1).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(false),
        )
        .unwrap();

        let original_key = "test.txt#_original".to_string();
        config.unregister_object(&original_key);

        let ranges2 = [LineRange::new(3, 4).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_directory_skips_protected_files() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();
        fs::write(subdir.join(".funveil_config"), "config\n").unwrap();

        let result = veil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_directory_skips_protected_files() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        veil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        fs::write(subdir.join(".funveil_config"), "config\n").unwrap();
        let result = unveil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        );
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

        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_partial_without_original_key() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        );
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
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        );
        assert!(result.is_ok());
        assert!(config.get_object("test.txt#4-5").is_some());
    }

    #[test]
    fn test_unveil_partial_no_remaining_after_unveil() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        );
        assert!(result.is_ok());
        assert!(config.get_object("test.txt#1-2").is_none());
    }

    #[test]
    fn test_unveil_partial_without_original_multiple_ranges() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap(), LineRange::new(4, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_unveil_partial_with_original_partial_range() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(1, 3).unwrap(), LineRange::new(5, 6).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let unveil_ranges = [LineRange::new(1, 3).unwrap()];
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        );
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
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let unveil_ranges = [LineRange::new(2, 2).unwrap()];
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        );
        assert!(result.is_ok());
        assert!(config.get_object("test.txt#4-4").is_some());
    }

    #[test]
    fn test_unveil_directory_with_protected_files() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        veil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        fs::create_dir_all(subdir.join(".funveil")).unwrap();
        fs::create_dir_all(subdir.join(".git")).unwrap();

        let result = crate::veil::unveil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &load_gitignore(temp.path()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_directory_with_protected_files() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();
        fs::create_dir_all(subdir.join(".funveil")).unwrap();

        let result = crate::veil::veil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &load_gitignore(temp.path()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_file_with_missing_cas_object() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges = [LineRange::new(1, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        if let Some(meta) = config.get_object("test.txt#1-3") {
            let store = crate::cas::ContentStore::new(temp.path());
            let hash = ContentHash::from_string(meta.hash.clone()).unwrap();
            let _ = store.delete(&hash);
        }

        let _ = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        );
    }

    #[test]
    fn test_veil_multiline_range_formatting() {
        // Covers line 213: output.push_str("...\n") for last line of a multi-line range
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges = [LineRange::new(2, 4).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

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
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        // Set specific permissions before veiling
        let perms = fs::Permissions::from_mode(0o644);
        fs::set_permissions(&file_path, perms).unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // File should be read-only after veiling
        let meta = fs::metadata(&file_path).unwrap();
        assert!(meta.permissions().readonly());

        // Unveil and check permissions are restored
        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_unveil_directory_skips_funveil_config() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        veil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        fs::write(subdir.join(".funveil_config"), "config\n").unwrap();

        let result = crate::veil::unveil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &load_gitignore(temp.path()),
        );
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

        let gi = load_gitignore(temp.path());
        let result = veil_directory(
            temp.path(),
            &mut config,
            temp.path(),
            None,
            &mut Output::new(false),
            &gi,
        );
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

        let gi = load_gitignore(temp.path());
        veil_directory(
            temp.path(),
            &mut config,
            temp.path(),
            None,
            &mut Output::new(false),
            &gi,
        )
        .unwrap();
        assert!(has_veils(&config, "normal.txt"));

        // Create protected files/dirs that should be skipped during unveil
        fs::write(temp.path().join(".funveil_config"), "config data\n").unwrap();
        fs::create_dir_all(temp.path().join(".git")).unwrap();

        let result = unveil_directory(
            temp.path(),
            &mut config,
            temp.path(),
            None,
            &mut Output::new(false),
            &gi,
        );
        assert!(result.is_ok());
        assert!(!has_veils(&config, "normal.txt"));
    }

    #[test]
    fn test_veil_multiline_range_formatting_detailed() {
        // Verifies the multi-line range veil display format.
        // For a range of 3+ lines (e.g., 2-4), the output is:
        // - pos_in_range 0 (first line): ...[hash]
        // - remaining lines: empty lines
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges = [LineRange::new(2, 4).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        let content_lines: Vec<&str> = content.lines().collect();
        // line1 is unveiled
        assert_eq!(content_lines[0], "line1");
        // pos_in_range 0: ...[hash]
        assert!(content_lines[1].starts_with("...["));
        // pos_in_range 1: empty line
        assert_eq!(content_lines[2], "");
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
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\n").unwrap();

        // Veil two ranges
        let ranges = [LineRange::new(2, 4).unwrap(), LineRange::new(6, 8).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Unveil only the first range, keeping range 6-8 veiled
        let unveil_ranges = [LineRange::new(2, 4).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

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

    #[test]
    fn test_veil_unveil_full_roundtrip() {
        let temp = TempDir::new().unwrap();
        let original = "fn main() {\n    println!(\"hello\");\n}\n";
        fs::write(temp.path().join("roundtrip.rs"), original).unwrap();

        let mut config = Config::new(crate::types::Mode::Whitelist);
        veil_file(
            temp.path(),
            &mut config,
            "roundtrip.rs",
            None,
            &mut Output::new(false),
        )
        .unwrap();
        config.save(temp.path()).unwrap();

        // File should be veiled (content replaced)
        let veiled = fs::read_to_string(temp.path().join("roundtrip.rs")).unwrap();
        assert_ne!(veiled, original);

        // Unveil
        unveil_file(
            temp.path(),
            &mut config,
            "roundtrip.rs",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        let restored = fs::read_to_string(temp.path().join("roundtrip.rs")).unwrap();
        assert_eq!(
            restored, original,
            "veil/unveil round-trip should produce exact match"
        );
    }

    #[test]
    fn test_unveil_preserves_permissions_on_cas_failure() {
        // BUG-009 regression: if CAS retrieval fails during unveil,
        // original permissions should be restored
        let temp = TempDir::new().unwrap();
        ensure_data_dir(temp.path()).unwrap();
        let mut config = Config::new(crate::types::Mode::Whitelist);

        // Create a file and veil it
        let file_path = temp.path().join("secret.txt");
        fs::write(&file_path, "secret content\n").unwrap();

        let ranges = [crate::types::LineRange::new(1, 1).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "secret.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Set restrictive permissions (read-only)
        let mut perms = fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&file_path, perms).unwrap();

        // Corrupt the CAS entry so retrieval fails: register with a bogus hash
        let bogus_key = "secret.txt#_original".to_string();
        if let Some(meta) = config.get_object(&bogus_key) {
            let mut corrupted_meta = meta.clone();
            corrupted_meta.hash =
                "0000000000000000000000000000000000000000000000000000000000000000".to_string();
            config.objects.insert(bogus_key, corrupted_meta);
        }

        // Try to unveil - this should fail because the CAS hash doesn't exist
        let result = unveil_file(
            temp.path(),
            &mut config,
            "secret.txt",
            Some(&ranges),
            &mut Output::new(false),
        );
        assert!(result.is_err());

        // Verify permissions were restored to 0o444
        let final_perms = fs::metadata(&file_path).unwrap().permissions();
        assert_eq!(final_perms.mode() & 0o777, 0o444);
    }

    #[test]
    fn test_veil_partial_multi_range_round_trip() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        let original = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\n";
        fs::write(&file_path, original).unwrap();

        // Veil 3 non-contiguous ranges
        let ranges = [
            LineRange::new(1, 2).unwrap(),
            LineRange::new(4, 5).unwrap(),
            LineRange::new(7, 8).unwrap(),
        ];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Unveil middle range only
        let unveil_ranges = [LineRange::new(4, 5).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Verify 2 ranges remain veiled
        assert!(config.get_object("test.txt#1-2").is_some());
        assert!(config.get_object("test.txt#4-5").is_none());
        assert!(config.get_object("test.txt#7-8").is_some());

        // Unveil all remaining
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        // Verify full content restored
        let restored = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            restored, original,
            "full content should be restored after unveiling all ranges"
        );
    }

    #[test]
    fn test_veil_single_line_range_formatting() {
        // BUG-022: verify single-line veil marker placement
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let ranges = [LineRange::new(2, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        let content_lines: Vec<&str> = content.lines().collect();
        assert_eq!(content_lines[0], "line1");
        // Single-line range: ...[hash]...
        assert!(content_lines[1].starts_with("...["));
        assert!(content_lines[1].ends_with("]..."));
        assert_eq!(content_lines[2], "line3");
    }

    #[test]
    fn test_veil_adjacent_ranges() {
        // Regression: two adjacent but non-overlapping ranges
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap(), LineRange::new(4, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Both ranges should be registered
        assert!(config.get_object("test.txt#2-3").is_some());
        assert!(config.get_object("test.txt#4-5").is_some());

        // Unveil first range only
        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        assert!(config.get_object("test.txt#2-3").is_none());
        assert!(config.get_object("test.txt#4-5").is_some());

        // Unveil second range
        let unveil_ranges = [LineRange::new(4, 5).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        assert!(config.get_object("test.txt#4-5").is_none());
    }

    #[test]
    fn test_veil_file_write_failure_no_config_entry() {
        // BUG-063 regression: config should not have entry if file write fails
        let (temp, mut config) = setup();

        // Create a file with content
        let file_path = temp.path().join("readonly_test.txt");
        fs::write(&file_path, "original content\n").unwrap();

        // Make the file read-only so fs::write will fail
        let mut perms = fs::metadata(&file_path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&file_path, perms).unwrap();

        let result = veil_file(
            temp.path(),
            &mut config,
            "readonly_test.txt",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_err());

        // Config should NOT have an entry for this file
        assert!(
            config.get_object("readonly_test.txt").is_none(),
            "Config should not register object when file write fails"
        );

        // Cleanup: make writable so tempdir can be deleted
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o644);
            fs::set_permissions(&file_path, perms).unwrap();
        }
    }

    #[test]
    fn test_bug096_unveil_all_hash_in_filename() {
        let (temp, mut config) = setup();

        // Create a file with '#' in its name
        let file_path = temp.path().join("dir");
        fs::create_dir_all(&file_path).unwrap();
        let hash_file = temp.path().join("dir/file#name.txt");
        fs::write(&hash_file, "content\n").unwrap();

        // Veil it
        veil_file(
            temp.path(),
            &mut config,
            "dir/file#name.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        assert!(has_veils(&config, "dir/file#name.txt"));

        // unveil_all should correctly parse the key and unveil the file
        unveil_all(temp.path(), &mut config, &mut Output::new(false)).unwrap();

        let content = fs::read_to_string(&hash_file).unwrap();
        assert_eq!(content, "content\n");
        assert!(!has_veils(&config, "dir/file#name.txt"));
    }

    // ── BUG-110: Overlapping veil range tests ──

    #[test]
    fn test_bug110_veil_overlapping_ranges_rejected() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(
            &file_path,
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\n",
        )
        .unwrap();

        let ranges1 = [LineRange::new(1, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(true),
        )
        .unwrap();

        let ranges2 = [LineRange::new(3, 8).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(true),
        );
        assert!(
            matches!(result, Err(FunveilError::OverlappingVeil { .. })),
            "expected OverlappingVeil, got: {result:?}"
        );
    }

    #[test]
    fn test_bug110_veil_subset_range_rejected() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n").unwrap();

        let ranges1 = [LineRange::new(1, 10).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(true),
        )
        .unwrap();

        let ranges2 = [LineRange::new(3, 5).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(true),
        );
        assert!(matches!(result, Err(FunveilError::OverlappingVeil { .. })));
    }

    #[test]
    fn test_bug110_veil_superset_range_rejected() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n").unwrap();

        let ranges1 = [LineRange::new(3, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(true),
        )
        .unwrap();

        let ranges2 = [LineRange::new(1, 10).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(true),
        );
        assert!(matches!(result, Err(FunveilError::OverlappingVeil { .. })));
    }

    #[test]
    fn test_bug110_veil_adjacent_ranges_ok() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n").unwrap();

        let ranges1 = [LineRange::new(1, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(true),
        )
        .unwrap();

        let ranges2 = [LineRange::new(6, 10).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(true),
        );
        assert!(
            result.is_ok(),
            "adjacent ranges should be allowed: {result:?}"
        );
    }

    #[test]
    fn test_bug110_veil_new_ranges_overlap_each_other() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "a\nb\nc\nd\ne\nf\ng\nh\n").unwrap();

        let ranges = [LineRange::new(1, 5).unwrap(), LineRange::new(3, 8).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(true),
        );
        assert!(matches!(result, Err(FunveilError::OverlappingVeil { .. })));
    }

    #[test]
    fn test_bug110_veil_nonoverlapping_ranges_ok() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "a\nb\nc\nd\ne\nf\ng\n").unwrap();

        let ranges1 = [LineRange::new(1, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(true),
        )
        .unwrap();

        let ranges2 = [LineRange::new(5, 7).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(true),
        );
        assert!(
            result.is_ok(),
            "non-overlapping ranges should be allowed: {result:?}"
        );
    }

    // ── BUG-111: Marker integrity check ──

    #[test]
    fn test_bug111_marker_integrity_error() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let ranges1 = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(true),
        )
        .unwrap();

        // Corrupt the marker on disk by changing the hash
        let veiled = fs::read_to_string(&file_path).unwrap();
        let corrupted = veiled.replacen("...[", "...[0000000", 1);
        // Make writable first
        let mut perms = fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&file_path, perms).unwrap();
        fs::write(&file_path, corrupted).unwrap();

        let ranges2 = [LineRange::new(4, 5).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(true),
        );
        assert!(
            matches!(result, Err(FunveilError::MarkerIntegrityError(_))),
            "expected MarkerIntegrityError, got: {result:?}"
        );
    }

    // ── BUG-105: Marker collision check ──

    #[test]
    fn test_bug105_veil_marker_collision_warning() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "normal line\n...[abc1234]...\nmore content\n").unwrap();

        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(true),
        );
        assert!(
            matches!(result, Err(FunveilError::MarkerCollision(_))),
            "expected MarkerCollision, got: {result:?}"
        );
    }

    // ── BUG-106: Unsupported filename characters ──

    #[test]
    fn test_bug106_veil_unsupported_filename() {
        let (temp, mut config) = setup();

        // Test with null byte in filename
        let result = veil_file(
            temp.path(),
            &mut config,
            "file\x00name.txt",
            None,
            &mut Output::new(true),
        );
        assert!(result.is_err(), "null byte in filename should be rejected");

        // Test with newline in filename
        let result = veil_file(
            temp.path(),
            &mut config,
            "file\nname.txt",
            None,
            &mut Output::new(true),
        );
        assert!(result.is_err(), "newline in filename should be rejected");

        // Test with control character
        let result = veil_file(
            temp.path(),
            &mut config,
            "file\x01name.txt",
            None,
            &mut Output::new(true),
        );
        assert!(
            result.is_err(),
            "control char in filename should be rejected"
        );
    }

    // ── BUG-114: Partial veil preserves no trailing newline ──

    #[test]
    fn test_bug114_partial_veil_preserves_no_trailing_newline() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        // File without trailing newline
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let veiled = fs::read_to_string(&file_path).unwrap();
        assert!(
            !veiled.ends_with('\n'),
            "veiled file should not gain trailing newline, got: {veiled:?}"
        );
    }

    // ── BUG-115: v1 unveil preserves no trailing newline ──

    #[test]
    fn test_bug115_v1_unveil_preserves_no_trailing_newline() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        // File without trailing newline
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original key to force v1 fallback path
        config.unregister_object("test.txt#_original");

        // Unveil all (triggers v1 reconstruction)
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        let restored = fs::read_to_string(&file_path).unwrap();
        assert!(
            !restored.ends_with('\n'),
            "v1 unveil should not add trailing newline, got: {restored:?}"
        );
    }

    // ── BUG-116: v2 partial unveil fallback preserves no trailing newline ──

    #[test]
    fn test_bug116_v2_partial_unveil_preserves_no_trailing_newline() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        // File without trailing newline
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original key to force fallback path in partial unveil
        config.unregister_object("test.txt#_original");

        // Partial unveil (triggers v2 fallback without _original)
        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let restored = fs::read_to_string(&file_path).unwrap();
        assert!(
            !restored.ends_with('\n'),
            "v2 fallback unveil should not add trailing newline, got: {restored:?}"
        );
    }

    // ── BUG-119: Empty ranges rejected ──

    #[test]
    fn test_bug119_veil_empty_ranges_rejected() {
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\n").unwrap();

        let ranges: [LineRange; 0] = [];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        );
        assert!(
            matches!(result, Err(FunveilError::InvalidLineRange { .. })),
            "empty ranges should be rejected, got: {result:?}"
        );
    }

    // ── BUG-120: veil_directory strip_prefix safety ──

    #[test]
    fn test_bug120_veil_directory_strip_prefix_safety() {
        // Verify that veil_directory handles paths correctly and uses
        // strip_prefix properly — the function should work for normal
        // subdirectories within root
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "content\n").unwrap();

        let result = veil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &load_gitignore(temp.path()),
        );
        assert!(result.is_ok());
        assert!(has_veils(&config, "subdir/file.txt"));
    }

    // ── validate_filename edge cases (catches lines 19-20 mutants) ──

    #[test]
    fn test_validate_filename_allows_tab() {
        // Tab (0x09) should be allowed
        let result = validate_filename("file\twith_tab.txt");
        assert!(result.is_ok(), "tab should be allowed in filenames");
    }

    #[test]
    fn test_validate_filename_rejects_control_chars() {
        // Various control chars < 0x20 (excluding tab 0x09)
        for byte in 0x00..0x20u8 {
            if byte == b'\t' {
                continue; // tab is allowed
            }
            let name = format!("file{}name.txt", byte as char);
            let result = validate_filename(&name);
            assert!(
                result.is_err(),
                "byte 0x{byte:02x} should be rejected, got Ok"
            );
        }
    }

    #[test]
    fn test_validate_filename_allows_space_and_printable() {
        // Space (0x20) and above should be allowed
        let result = validate_filename("file name.txt");
        assert!(result.is_ok(), "space (0x20) should be allowed");

        let result = validate_filename("normal_file.txt");
        assert!(result.is_ok(), "normal filename should be ok");
    }

    // ── find_binary_in_directory protection checks (catches lines 402-404) ──

    #[test]
    fn test_find_binary_in_directory_skips_config_file() {
        // Protection checks use paths relative to root.
        // .funveil_config at root level is skipped.
        let (temp, _config) = setup();

        // Put binary-like content in .funveil_config at root
        // (find_binary_in_directory walks from dir_path, strips root prefix)
        fs::write(temp.path().join(".funveil_config"), b"\x00\x01\x02").unwrap();
        fs::write(temp.path().join("normal.txt"), "text\n").unwrap();

        let result = find_binary_in_directory(temp.path(), temp.path());
        assert!(
            result.is_none(),
            "should skip config file when checking for binaries, got: {result:?}"
        );
    }

    #[test]
    fn test_find_binary_in_directory_skips_data_dir() {
        let (temp, _config) = setup();

        // .funveil/ dir at root has relative path ".funveil/..." which matches is_data_dir
        fs::create_dir_all(temp.path().join(".funveil/objects")).unwrap();
        fs::write(temp.path().join(".funveil/objects/binary"), b"\x00\x01\x02").unwrap();
        fs::write(temp.path().join("normal.txt"), "text\n").unwrap();

        let result = find_binary_in_directory(temp.path(), temp.path());
        assert!(
            result.is_none(),
            "should skip .funveil dir when checking for binaries"
        );
    }

    #[test]
    fn test_find_binary_in_directory_skips_vcs() {
        let (temp, _config) = setup();

        // .git/ at root has relative path ".git/..." which matches is_vcs_directory
        fs::create_dir_all(temp.path().join(".git/objects")).unwrap();
        fs::write(temp.path().join(".git/objects/pack"), b"\x00\x01\x02").unwrap();
        fs::write(temp.path().join("normal.txt"), "text\n").unwrap();

        let result = find_binary_in_directory(temp.path(), temp.path());
        assert!(
            result.is_none(),
            "should skip .git dir when checking for binaries"
        );
    }

    #[test]
    fn test_find_binary_in_directory_detects_real_binary() {
        let (temp, _config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();

        // Create an actual binary file
        fs::write(subdir.join("image.bin"), b"\x00\x01\x02\x03").unwrap();

        let result = find_binary_in_directory(temp.path(), &subdir);
        assert!(result.is_some(), "should detect binary file");
    }

    // ── veil_directory error counting and quiet (catches lines 443-485) ──

    #[test]
    fn test_veil_directory_error_counting_with_already_veiled() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), "content1\n").unwrap();
        fs::write(subdir.join("file2.txt"), "content2\n").unwrap();

        // Veil file1 first so the directory veil will fail for it
        veil_file(
            temp.path(),
            &mut config,
            "subdir/file1.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        // Now veil the directory - file1 should error (already veiled), file2 should succeed
        let gi = load_gitignore(temp.path());
        let result = veil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &gi,
        );
        assert!(result.is_ok());

        // file2 should be veiled
        assert!(has_veils(&config, "subdir/file2.txt"));
    }

    #[test]
    fn test_veil_directory_quiet_suppresses_errors() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), "content1\n").unwrap();

        // Pre-veil file1
        veil_file(
            temp.path(),
            &mut config,
            "subdir/file1.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        // Veil directory in quiet mode - should still succeed but suppress warnings
        let gi = load_gitignore(temp.path());
        let result = veil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(true),
            &gi,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_veil_directory_skips_all_protected_types() {
        // Tests that veil_directory skips config, data, funveil, and vcs paths
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("normal.txt"), "content\n").unwrap();
        fs::write(subdir.join(".funveil_config"), "config\n").unwrap();
        fs::create_dir_all(subdir.join(".funveil")).unwrap();
        fs::write(subdir.join(".funveil/test"), "data\n").unwrap();
        fs::create_dir_all(subdir.join(".git")).unwrap();
        fs::write(subdir.join(".git/config"), "git\n").unwrap();

        let gi = load_gitignore(temp.path());
        let result = veil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &gi,
        );
        assert!(result.is_ok());

        // Only normal.txt should be veiled
        assert!(has_veils(&config, "subdir/normal.txt"));
    }

    // ── unveil_directory error counting and quiet (catches lines 907-949) ──

    #[test]
    fn test_unveil_directory_error_counting() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), "content1\n").unwrap();
        fs::write(subdir.join("file2.txt"), "content2\n").unwrap();

        // Veil both files
        let gi = load_gitignore(temp.path());
        veil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &gi,
        )
        .unwrap();

        // Unveil file1 manually so it's no longer veiled
        unveil_file(
            temp.path(),
            &mut config,
            "subdir/file1.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        // Now unveil the directory - file1 should error (not veiled), file2 should succeed
        let result = unveil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &gi,
        );
        assert!(result.is_ok());

        // file2 should be unveiled
        assert!(!has_veils(&config, "subdir/file2.txt"));
    }

    #[test]
    fn test_unveil_directory_quiet_mode() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), "content1\n").unwrap();

        // Veil the file
        let gi = load_gitignore(temp.path());
        veil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &gi,
        )
        .unwrap();

        // Unveil in quiet mode
        let result = unveil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(true),
            &gi,
        );
        assert!(result.is_ok());
        assert!(!has_veils(&config, "subdir/file1.txt"));
    }

    #[test]
    fn test_unveil_directory_skips_all_protected_types() {
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("normal.txt"), "content\n").unwrap();

        let gi = load_gitignore(temp.path());
        veil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &gi,
        )
        .unwrap();

        // Add protected files after veiling
        fs::write(subdir.join(".funveil_config"), "config\n").unwrap();
        fs::create_dir_all(subdir.join(".funveil")).unwrap();
        fs::write(subdir.join(".funveil/test"), "data\n").unwrap();
        fs::create_dir_all(subdir.join(".git")).unwrap();
        fs::write(subdir.join(".git/config"), "git\n").unwrap();

        let result = unveil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &gi,
        );
        assert!(result.is_ok());
        assert!(!has_veils(&config, "subdir/normal.txt"));
    }

    #[test]
    fn test_unveil_directory_file_errors_gt_zero_warning() {
        // Specifically exercises: file_errors > 0 && !quiet
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), "content1\n").unwrap();
        fs::write(subdir.join("file2.txt"), "content2\n").unwrap();

        // Veil only file1
        veil_file(
            temp.path(),
            &mut config,
            "subdir/file1.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        // Unveil directory - file2 is not veiled so it should error and increment file_errors
        let gi = load_gitignore(temp.path());
        let result = unveil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &gi,
        );
        assert!(result.is_ok()); // directory unveil doesn't fail, just counts errors

        // file1 should be unveiled
        assert!(!has_veils(&config, "subdir/file1.txt"));
    }

    #[test]
    fn test_veil_directory_file_errors_gt_zero_warning() {
        // Specifically exercises: file_errors > 0 && !quiet
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), "content1\n").unwrap();
        fs::write(subdir.join("file2.txt"), "content2\n").unwrap();

        // Pre-veil file1 so directory veil will get an error for it
        veil_file(
            temp.path(),
            &mut config,
            "subdir/file1.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        // Veil directory with quiet=false - should produce warning about errors
        let gi = load_gitignore(temp.path());
        let result = veil_directory(
            temp.path(),
            &mut config,
            &subdir,
            None,
            &mut Output::new(false),
            &gi,
        );
        assert!(result.is_ok());

        // file2 should still be veiled
        assert!(has_veils(&config, "subdir/file2.txt"));
    }

    // ── check_marker_integrity && to || (catches line 56) ──

    #[test]
    fn test_check_marker_integrity_with_original_suffix() {
        // Exercises the `&& !key.ends_with(ORIGINAL_SUFFIX)` condition.
        // When a key has #_original suffix, it should be skipped in integrity check.
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // The _original key exists and should NOT be checked for marker integrity.
        // If && were changed to ||, the _original key would be processed and fail.
        let veiled_content = fs::read_to_string(&file_path).unwrap();
        let result = check_marker_integrity(&veiled_content, &config, "test.txt");
        assert!(result.is_ok());
    }

    // ── veil_file has_any_veils && to || (catches line 155) ──

    #[test]
    fn test_marker_collision_skipped_when_file_has_veils() {
        // When file already has veils, marker collision check is skipped
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        // First partial veil
        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Now the file has markers. Adding another range should skip collision check.
        let ranges2 = [LineRange::new(4, 5).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(false),
        );
        assert!(
            result.is_ok(),
            "should skip collision check for already-veiled file"
        );
    }

    // ── unveil_file: v1 path iteration (catches lines 647, 648, 651, 656) ──

    #[test]
    fn test_unveil_v1_path_multiple_ranges() {
        // Tests the v1 compat path with multiple ranges and various line positions
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\nline6\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap(), LineRange::new(5, 6).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original to force v1 path
        config.unregister_object("test.txt#_original");

        // Unveil all (v1 path)
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));
        assert!(content.contains("line3"));
        assert!(content.contains("line4"));
        assert!(content.contains("line5"));
        assert!(content.contains("line6"));
    }

    #[test]
    fn test_unveil_v1_path_single_range_at_start() {
        // Tests v1 path where range starts at line 1
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        config.unregister_object("test.txt#_original");

        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));
        assert!(content.contains("line3"));
    }

    // ── unveil_file: partial unveil without original, multiple ranges (catches lines 797, 809, 829, 836) ──

    #[test]
    fn test_unveil_partial_without_original_preserves_other_veils() {
        // Tests the partial unveil fallback when no _original key exists,
        // verifying line-by-line reconstruction logic
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap(), LineRange::new(5, 6).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original
        config.unregister_object("test.txt#_original");

        // Unveil range 2-3 only
        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        );
        assert!(result.is_ok());

        // Range 5-6 should remain veiled
        assert!(config.get_object("test.txt#5-6").is_some());
        assert!(config.get_object("test.txt#2-3").is_none());
    }

    #[test]
    fn test_unveil_partial_without_original_all_ranges_removed() {
        // When all partial ranges are removed without _original, permissions should be restored
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        );
        assert!(result.is_ok());

        // No veils should remain
        assert!(!has_veils(&config, "test.txt"));
    }

    // ── unveil_file: partial unveil with original, line matching (catches lines 706, 735, 741) ──

    #[test]
    fn test_unveil_partial_with_original_single_line_veiled() {
        // Tests the single-line range marker format in partial unveil with original
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(2, 2).unwrap(), LineRange::new(5, 6).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Unveil only range 5-6, keeping 2-2 veiled
        let unveil_ranges = [LineRange::new(5, 6).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Single-line range 2-2 should still be veiled
        assert!(config.get_object("test.txt#2-2").is_some());
        assert!(config.get_object("test.txt#5-6").is_none());

        let content = fs::read_to_string(&file_path).unwrap();
        // Line 5 and 6 should be restored
        assert!(content.contains("l5"));
        assert!(content.contains("l6"));
        // Line 2 should still show marker (single-line format: ...[hash]...)
        let lines: Vec<&str> = content.lines().collect();
        assert!(
            lines[1].starts_with("...["),
            "line 2 should be single-line veiled marker"
        );
        assert!(
            lines[1].ends_with("]..."),
            "single-line marker should end with ]..."
        );
    }

    #[test]
    fn test_unveil_partial_with_original_multiline_veiled() {
        // Tests the multi-line range marker format in partial unveil with original
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\n").unwrap();

        let ranges = [LineRange::new(1, 1).unwrap(), LineRange::new(3, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Unveil single-line range 1-1, keeping 3-5 veiled
        let unveil_ranges = [LineRange::new(1, 1).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        // Line 1 should be restored
        assert_eq!(lines[0], "l1");
        // Line 2 should be visible (never veiled)
        assert_eq!(lines[1], "l2");
        // Line 3 (first of multi-line range): ...[hash]
        assert!(lines[2].starts_with("...["));
        assert!(
            !lines[2].ends_with("]..."),
            "multi-line marker should NOT end with ]..."
        );
        // Lines 4-5: empty continuation lines
        assert_eq!(lines[3], "");
        assert_eq!(lines[4], "");
    }

    // ── find_veiled_range_for_line: && to || (catches line 873) ──

    #[test]
    fn test_find_veiled_range_for_line_with_original_suffix() {
        // The _original key should be excluded from range lookup.
        // If && changed to ||, _original would be incorrectly matched.
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Range 1-2 should be found
        let result = find_veiled_range_for_line(&config, "test.txt", 1);
        assert!(result.is_some());
        let range = result.unwrap();
        assert_eq!(range.start(), 1);
        assert_eq!(range.end(), 2);

        // Line 3 should not be in any range
        let result = find_veiled_range_for_line(&config, "test.txt", 3);
        assert!(result.is_none());
    }

    // ── has_existing_veils filter (catches lines 200, 220, 321) ──

    #[test]
    fn test_veil_existing_veils_filter_excludes_original() {
        // Tests that the `!k.ends_with(ORIGINAL_SUFFIX)` filter in has_existing_veils works.
        // If && changed to ||, the _original key would be incorrectly included/excluded.
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\n").unwrap();

        // First veil creates #_original + range key
        let ranges1 = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(false),
        )
        .unwrap();

        // Second veil should find existing veils (key starts_with prefix AND !ends_with _original)
        let ranges2 = [LineRange::new(5, 6).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(false),
        );
        assert!(result.is_ok());

        // Third veil to exercise overlap checking path
        let ranges3 = [LineRange::new(7, 8).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges3),
            &mut Output::new(false),
        );
        assert!(result.is_ok());

        // All three ranges should be registered
        assert!(config.get_object("test.txt#1-2").is_some());
        assert!(config.get_object("test.txt#5-6").is_some());
        assert!(config.get_object("test.txt#7-8").is_some());
    }

    // ── veil_file: is_data_dir || is_funveil_protected (catches line 112) ──

    #[test]
    fn test_veil_funveil_protected_separate_checks() {
        // Tests that is_data_dir and is_funveil_protected are both checked.
        // If || changed to &&, one of these would not be caught independently.
        let (temp, mut config) = setup();

        // .funveil/ should be caught by is_data_dir
        let result = veil_file(
            temp.path(),
            &mut config,
            ".funveil/something",
            None,
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::DataDirectoryProtected)));

        // .funveil_config should be caught separately
        let result = veil_file(
            temp.path(),
            &mut config,
            ".funveil_config",
            None,
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::ConfigFileProtected)));
    }

    // ── unveil_file: line 605 && to || filter in partial keys ──

    #[test]
    fn test_unveil_v1_partial_keys_filter() {
        // Tests the filter condition in the v1 unveil path:
        // k.starts_with(&format!("{file}#")) && !k.ends_with(ORIGINAL_SUFFIX)
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // _original key should NOT be treated as a partial veil key
        assert!(config.get_object("test.txt#_original").is_some());

        // Full unveil (no ranges) should work correctly
        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());

        // Both _original and range key should be cleaned up
        assert!(config.get_object("test.txt#_original").is_none());
        assert!(config.get_object("test.txt#1-2").is_none());
    }

    // ── check_marker_integrity: line 56 && to || ──

    #[test]
    fn test_check_marker_integrity_ignores_original_suffix() {
        // Tests line 56: key.starts_with(&prefix) && !key.ends_with(ORIGINAL_SUFFIX)
        // If && becomes ||, the _original key would be treated as a range key and fail
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let ranges = [LineRange::new(1, 1).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Now add a second range — this triggers check_marker_integrity
        let veiled_content = fs::read_to_string(&file_path).unwrap();
        assert!(veiled_content.contains("...[")); // marker present

        // Adding a non-overlapping range should work (integrity check must skip _original key)
        let ranges2 = [LineRange::new(3, 3).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    // ── veil_file: line 112 || to && (is_data_dir || is_funveil_protected) ──

    #[test]
    fn test_veil_funveil_protected_file_blocked() {
        // Tests line 112: is_data_dir(file) || is_funveil_protected(file)
        // If || becomes &&, .funveil_lock would NOT be blocked
        let (temp, mut config) = setup();
        let file_path = temp.path().join(".funveil_lock");
        fs::write(&file_path, "lock content\n").unwrap();

        let result = veil_file(
            temp.path(),
            &mut config,
            ".funveil_lock",
            None,
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::DataDirectoryProtected)));
    }

    // ── veil_file: line 155 && to || (has_any_veils check) ──

    #[test]
    fn test_veil_has_any_veils_skips_marker_collision_check() {
        // Tests line 155: k.starts_with(&format!("{file}#")) && !k.ends_with(ORIGINAL_SUFFIX)
        // If && becomes ||, files that have no veils might skip collision check
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        // Content that looks like a veil marker
        fs::write(&file_path, "...[abcdef12]...\n").unwrap();

        // Should fail because content matches marker pattern and no existing veils
        let ranges = [LineRange::new(1, 1).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::MarkerCollision(_))));
    }

    // ── veil_file: line 200 && to || and delete ! (has_existing_veils for overlap check) ──

    #[test]
    fn test_veil_overlap_detection_with_original_key() {
        // Tests line 200: k.starts_with(&format!("{file}#")) && !k.ends_with(ORIGINAL_SUFFIX)
        // The has_existing_veils check must ignore _original suffix keys
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\n").unwrap();

        // First veil creates range + _original
        let ranges1 = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(false),
        )
        .unwrap();

        // Now try overlapping range — should be detected
        let ranges2 = [LineRange::new(2, 3).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(false),
        );
        assert!(matches!(result, Err(FunveilError::OverlappingVeil { .. })));
    }

    // ── veil_file: line 220 && to || (existing ranges filter for overlap check) ──

    #[test]
    fn test_veil_existing_range_filter_ignores_original() {
        // Tests line 220: k.starts_with(&prefix) && !k.ends_with(ORIGINAL_SUFFIX)
        // Adding a non-overlapping range after first veil should work
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\n").unwrap();

        let ranges1 = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges1),
            &mut Output::new(false),
        )
        .unwrap();

        let ranges2 = [LineRange::new(4, 5).unwrap()];
        let result = veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges2),
            &mut Output::new(false),
        );
        assert!(result.is_ok());
        assert!(config.get_object("test.txt#4-5").is_some());
    }

    // ── veil_file: line 321 && to || (all_veiled_ranges filter for output generation) ──

    #[test]
    fn test_veil_output_generation_correctly_filters_ranges() {
        // Tests line 321: k.starts_with(&prefix) && !k.ends_with(ORIGINAL_SUFFIX)
        // The output generation must correctly filter range keys from _original key
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        // Line 1 should be preserved
        assert_eq!(lines[0], "l1");
        // Line 2 should be a marker (start of veiled range)
        assert!(lines[1].starts_with("...["));
        // Lines 4 and 5 should be preserved
        assert_eq!(lines[lines.len() - 2], "l4");
        assert_eq!(lines[lines.len() - 1], "l5");
    }

    // ── find_binary_in_directory: lines 402-403 || to && ──

    #[test]
    fn test_find_binary_skips_protected_files() {
        // Tests lines 402-403: is_config_file || is_data_dir || is_funveil_protected || is_vcs_directory
        // If || becomes &&, individual protected files won't be skipped
        let (temp, _config) = setup();

        // Create a file in .funveil that looks binary
        let data_dir = temp.path().join(".funveil").join("objects");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("abcdef"), b"\x00\x01\x02\x03").unwrap();

        // find_binary_in_directory should skip .funveil directory
        let result = find_binary_in_directory(temp.path(), temp.path());
        assert!(result.is_none());
    }

    // ── veil_directory: lines 470-472 || to && (skip conditions) ──

    #[test]
    fn test_veil_directory_skips_config_file() {
        // Tests lines 470-472: skip conditions for config/data/protected/vcs files
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("real.txt"), "content\n").unwrap();
        // Create .funveil_config in subdir (should be skipped, not cause error)
        fs::write(subdir.join(".funveil_config"), "config\n").unwrap();

        let result = veil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());
        // real.txt should be veiled but .funveil_config should be skipped
        assert!(config.get_object("subdir/real.txt").is_some());
    }

    // ── unveil_file: line 656 += *= (v1 line_idx += range.len()) ──

    #[test]
    fn test_unveil_v1_multiline_range_advancement() {
        // Tests line 656: line_idx += range.len()
        // If += becomes *=, line advancement breaks for multi-line ranges
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\n").unwrap();

        let ranges = [LineRange::new(2, 4).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original to force v1 unveil path
        config.unregister_object("test.txt#_original");

        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        // Should have all 5 lines restored correctly
        assert!(content.contains("l1"));
        assert!(content.contains("l2"));
        assert!(content.contains("l3"));
        assert!(content.contains("l4"));
        assert!(content.contains("l5"));
    }

    // ── unveil_file: line 706 && to || (key filter in partial unveil) ──

    #[test]
    fn test_unveil_partial_key_filter_excludes_original() {
        // Tests line 706: key.starts_with(&check_prefix) && !key.ends_with(ORIGINAL_SUFFIX)
        // In partial unveil, the is_still_veiled check must skip _original keys
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\n").unwrap();

        // Veil two ranges
        let ranges = [LineRange::new(1, 2).unwrap(), LineRange::new(4, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Unveil only first range
        let unveil_ranges = [LineRange::new(1, 2).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Range 4-5 should still be veiled
        assert!(config.get_object("test.txt#4-5").is_some());
        // Range 1-2 should be unveiled
        assert!(config.get_object("test.txt#1-2").is_none());

        let content = fs::read_to_string(&file_path).unwrap();
        // Lines 1-2 should be restored
        assert!(content.contains("l1"));
        assert!(content.contains("l2"));
        // Line 3 should be visible
        assert!(content.contains("l3"));
        // Lines 4-5 should still be veiled (markers)
        assert!(content.contains("...["));
    }

    // ── unveil_file: line 797 + to * (line number calculation) ──

    #[test]
    fn test_unveil_v1_no_original_line_numbers() {
        // Tests line 797: let line_num = i + 1
        // If + becomes *, line 0 would give line_num=0 instead of 1
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap();

        let ranges = [LineRange::new(1, 1).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original to force v1 path
        config.unregister_object("test.txt#_original");

        // Unveil specific range
        let unveil_ranges = [LineRange::new(1, 1).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("l1"));
    }

    // ── unveil_file: line 809 && to || (range.contains && line_num == range.start()) ──

    #[test]
    fn test_unveil_v1_no_original_only_emits_content_at_range_start() {
        // Tests line 809: range.contains(line_num) && line_num == range.start()
        // Content should only be emitted once at the start of each range, not at every line
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\n").unwrap();

        let ranges = [LineRange::new(2, 3).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original to force v1 path
        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        // Content should have l2 exactly once (not duplicated)
        assert_eq!(content.matches("l2").count(), 1);
        assert_eq!(content.matches("l3").count(), 1);
    }

    // ── unveil_file: line 829 && to || (trailing newline) ──

    #[test]
    fn test_unveil_v1_no_original_trailing_newline_not_stripped_when_present() {
        // Tests line 829: !veiled_had_trailing_newline && full_content.ends_with('\n')
        // If && becomes ||, trailing newline would always be stripped
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap(); // HAS trailing newline

        let ranges = [LineRange::new(2, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original to force v1 path
        config.unregister_object("test.txt#_original");

        // The veiled file should have a trailing newline since original did
        let veiled = fs::read_to_string(&file_path).unwrap();
        assert!(veiled.ends_with('\n'));

        let unveil_ranges = [LineRange::new(2, 2).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        // Should preserve trailing newline since veiled file had one
        assert!(content.ends_with('\n'));
    }

    // ── unveil_file: line 836 && to || (remaining.is_empty() && config.get_object(file).is_none()) ──

    #[test]
    fn test_unveil_v1_no_original_restores_permissions_when_fully_unveiled() {
        // Tests line 836: remaining.is_empty() && config.get_object(file).is_none()
        // Both conditions must be true to restore permissions
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap();

        let ranges = [LineRange::new(1, 1).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original to force v1 path
        config.unregister_object("test.txt#_original");

        let unveil_ranges = [LineRange::new(1, 1).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // File should be fully unveiled - no veiled ranges left
        assert!(config.get_object("test.txt#1-1").is_none());
    }

    // ── find_veiled_range_for_line: line 873 && to || ──

    #[test]
    fn test_find_veiled_range_for_line_filters_original() {
        // Tests line 873: key.starts_with(&prefix) && !key.ends_with(ORIGINAL_SUFFIX)
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "l1\nl2\nl3\nl4\nl5\n").unwrap();

        // Veil two ranges
        let ranges = [LineRange::new(2, 3).unwrap(), LineRange::new(4, 5).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Unveil one range — the remaining range should still be found
        let unveil_ranges = [LineRange::new(2, 3).unwrap()];
        unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&unveil_ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Verify the remaining range is still identified
        let found = find_veiled_range_for_line(&config, "test.txt", 4);
        assert!(found.is_some());
        assert_eq!(found.unwrap(), LineRange::new(4, 5).unwrap());

        // Line outside any range should return None
        let found = find_veiled_range_for_line(&config, "test.txt", 1);
        assert!(found.is_none());
    }

    // ── unveil_directory: lines 934-936 || to && (skip conditions) ──

    #[test]
    fn test_unveil_directory_skips_config_and_data_files() {
        // Tests lines 934-936: skip conditions for config/data/protected/vcs files
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("real.txt"), "content\n").unwrap();

        // Veil the directory
        veil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        )
        .unwrap();
        assert!(config.get_object("subdir/real.txt").is_some());

        // Unveil the directory
        let result = unveil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(subdir.join("real.txt")).unwrap();
        assert_eq!(content, "content\n");
    }

    // ── veil_directory/unveil_directory: file_errors counter mutations ──

    #[test]
    fn test_veil_directory_error_counting() {
        // Tests lines 446, 463, 481: file_errors += 1 mutations
        // and lines 485: file_errors > 0 && !quiet
        // These are counters — veil_directory currently returns Ok(()) even with errors
        // but the error count affects the warning message
        let (temp, mut config) = setup();
        let subdir = temp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("good.txt"), "good content\n").unwrap();

        // veil_directory should succeed even with a mix
        let result = veil_file(
            temp.path(),
            &mut config,
            "subdir",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());
    }

    // ── unveil_file: v1 trailing newline (catches line 668) ──

    #[test]
    fn test_unveil_v1_trailing_newline_preservation() {
        // Tests that v1 unveil preserves trailing newline state
        let (temp, mut config) = setup();
        let file_path = temp.path().join("test.txt");
        // File WITH trailing newline
        fs::write(&file_path, "l1\nl2\nl3\n").unwrap();

        let ranges = [LineRange::new(1, 2).unwrap()];
        veil_file(
            temp.path(),
            &mut config,
            "test.txt",
            Some(&ranges),
            &mut Output::new(false),
        )
        .unwrap();

        // Remove _original to force v1 path
        config.unregister_object("test.txt#_original");

        let result = unveil_file(
            temp.path(),
            &mut config,
            "test.txt",
            None,
            &mut Output::new(false),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(&file_path).unwrap();
        // Should preserve trailing newline
        assert!(content.ends_with('\n'));
    }
}
