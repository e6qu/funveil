use std::fs;
use std::io;
use std::path::Path;

/// Saved permission state, to be restored on error.
pub struct SavedPerms {
    #[cfg(unix)]
    mode: u32,
    #[cfg(not(unix))]
    readonly: bool,
}

/// Save current permissions and make the file writable.
pub fn save_and_make_writable(path: &Path) -> io::Result<SavedPerms> {
    let metadata = fs::metadata(path)?;
    let mut perms = metadata.permissions();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let saved = SavedPerms { mode: perms.mode() };
        perms.set_mode(0o644);
        fs::set_permissions(path, perms)?;
        Ok(saved)
    }

    #[cfg(not(unix))]
    {
        let saved = SavedPerms {
            readonly: perms.readonly(),
        };
        perms.set_readonly(false);
        fs::set_permissions(path, perms)?;
        Ok(saved)
    }
}

/// Restore previously saved permissions.
pub fn restore(path: &Path, saved: &SavedPerms) -> io::Result<()> {
    if let Ok(md) = fs::metadata(path) {
        let mut p = md.permissions();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            p.set_mode(saved.mode);
        }

        #[cfg(not(unix))]
        {
            p.set_readonly(saved.readonly);
        }

        fs::set_permissions(path, p)?;
    }
    Ok(())
}

/// Set a file to read-only.
pub fn set_readonly(path: &Path) -> io::Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_readonly(true);
    fs::set_permissions(path, perms)
}

/// Set a file's Unix mode. No-op on non-Unix.
#[cfg(unix)]
pub fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(mode);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
pub fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

/// Get the Unix mode bits from file metadata. Returns 0o644 on non-Unix.
#[cfg(unix)]
pub fn file_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.mode()
}

#[cfg(not(unix))]
pub fn file_mode(_metadata: &fs::Metadata) -> u32 {
    0o644
}

/// Parse an octal permission string, defaulting to 0o644.
pub fn parse_mode(octal_str: &str) -> u32 {
    u32::from_str_radix(octal_str, 8).unwrap_or(0o644)
}

/// Format a mode as an octal string.
pub fn format_mode(mode: u32) -> String {
    format!("{mode:o}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_mode_valid() {
        assert_eq!(parse_mode("644"), 0o644);
        assert_eq!(parse_mode("755"), 0o755);
        assert_eq!(parse_mode("600"), 0o600);
    }

    #[test]
    fn test_parse_mode_invalid() {
        assert_eq!(parse_mode("invalid"), 0o644);
        assert_eq!(parse_mode(""), 0o644);
    }

    #[test]
    fn test_format_mode() {
        assert_eq!(format_mode(0o644), "644");
        assert_eq!(format_mode(0o755), "755");
    }

    #[test]
    fn test_save_and_make_writable_then_restore() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.txt");
        std::fs::write(&file, "content").unwrap();

        // Make it read-only first
        set_readonly(&file).unwrap();
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());

        // Save and make writable
        let saved = save_and_make_writable(&file).unwrap();
        assert!(!std::fs::metadata(&file).unwrap().permissions().readonly());

        // Restore
        restore(&file, &saved).unwrap();
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());
    }

    #[test]
    fn test_set_readonly() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.txt");
        std::fs::write(&file, "content").unwrap();

        set_readonly(&file).unwrap();
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());
    }

    #[cfg(unix)]
    #[test]
    fn test_set_mode() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.txt");
        std::fs::write(&file, "content").unwrap();

        set_mode(&file, 0o755).unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }

    #[test]
    fn test_restore_nonexistent_is_ok() {
        let saved = SavedPerms {
            #[cfg(unix)]
            mode: 0o644,
            #[cfg(not(unix))]
            readonly: false,
        };
        // Restoring a nonexistent file should not error (graceful)
        let result = restore(Path::new("/nonexistent/path/file.txt"), &saved);
        assert!(result.is_ok());
    }
}
