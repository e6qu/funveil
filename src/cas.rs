use crate::config::OBJECTS_DIR;
use crate::error::{FunveilError, Result};
use crate::output::Output;
use crate::types::ContentHash;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Content-addressable storage for veiled content
pub struct ContentStore {
    root: PathBuf,
}

impl ContentStore {
    /// Create a new CAS at the given project root
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.join(OBJECTS_DIR),
        }
    }

    /// Store content and return its hash
    pub fn store(&self, content: &[u8]) -> Result<ContentHash> {
        let hash = ContentHash::from_content(content);
        let (a, b, c) = hash.path_components()?;

        let dir = self.root.join(a).join(b);
        fs::create_dir_all(&dir)?;

        let path = dir.join(c);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(content)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = fs::read(&path)?;
                if existing != content {
                    return Err(FunveilError::HashCollision {
                        hash: hash.full().to_string(),
                        path,
                    });
                }
            }
            Err(e) => return Err(e.into()),
        }

        Ok(hash)
    }

    /// Retrieve content by hash
    pub fn retrieve(&self, hash: &ContentHash) -> Result<Vec<u8>> {
        let (a, b, c) = hash.path_components()?;
        let path = self.root.join(a).join(b).join(c);

        if !path.exists() {
            return Err(FunveilError::ObjectNotFound(hash.full().to_string()));
        }

        Ok(fs::read(&path)?)
    }

    /// Check if content exists
    pub fn exists(&self, hash: &ContentHash) -> bool {
        let (a, b, c) = hash
            .path_components()
            .expect("ContentHash invariant: len >= 7");
        self.root.join(a).join(b).join(c).exists()
    }

    /// Get the path for a hash (for debugging)
    pub fn path_for(&self, hash: &ContentHash) -> Result<PathBuf> {
        let (a, b, c) = hash.path_components()?;
        Ok(self.root.join(a).join(b).join(c))
    }

    /// Delete content by hash (for garbage collection)
    pub fn delete(&self, hash: &ContentHash) -> Result<()> {
        let (a, b, c) = hash.path_components()?;
        let path = self.root.join(a).join(b).join(c);

        if path.exists() {
            fs::remove_file(&path)?;
        }

        Ok(())
    }

    /// List all hashes in the store
    pub fn list_all(&self) -> Result<Vec<ContentHash>> {
        let mut hashes = Vec::new();

        if !self.root.exists() {
            return Ok(hashes);
        }

        for a_entry in fs::read_dir(&self.root)? {
            let a_entry = a_entry?;
            if !a_entry.file_type()?.is_dir() {
                continue;
            }
            let a = a_entry.file_name().to_string_lossy().to_string();

            for b_entry in fs::read_dir(a_entry.path())? {
                let b_entry = b_entry?;
                if !b_entry.file_type()?.is_dir() {
                    continue;
                }
                let b = b_entry.file_name().to_string_lossy().to_string();

                for c_entry in fs::read_dir(b_entry.path())? {
                    let c_entry = c_entry?;
                    if !c_entry.file_type()?.is_file() {
                        continue;
                    }
                    let c = c_entry.file_name().to_string_lossy().to_string();

                    let full_hash = format!("{a}{b}{c}");
                    if let Ok(hash) = ContentHash::from_string(full_hash) {
                        hashes.push(hash);
                    }
                }
            }
        }

        Ok(hashes)
    }

    /// Get total size of all objects
    pub fn total_size(&self) -> Result<u64> {
        let mut size = 0u64;

        if !self.root.exists() {
            return Ok(0);
        }

        for a_entry in fs::read_dir(&self.root)? {
            let a_entry = a_entry?;
            if !a_entry.file_type()?.is_dir() {
                continue;
            }

            for b_entry in fs::read_dir(a_entry.path())? {
                let b_entry = b_entry?;
                if !b_entry.file_type()?.is_dir() {
                    continue;
                }

                for c_entry in fs::read_dir(b_entry.path())? {
                    let c_entry = c_entry?;
                    if c_entry.file_type()?.is_file() {
                        size += c_entry.metadata()?.len();
                    }
                }
            }
        }

        Ok(size)
    }
}

/// Garbage collect unused objects
#[tracing::instrument(skip(root, referenced_hashes, output))]
pub fn garbage_collect(
    root: &Path,
    referenced_hashes: &[ContentHash],
    output: &mut Output,
) -> Result<(usize, u64)> {
    let store = ContentStore::new(root);
    let all_hashes = store.list_all()?;

    let referenced: std::collections::HashSet<_> = referenced_hashes
        .iter()
        .map(|h| h.full().to_string())
        .collect();

    let mut deleted = 0usize;
    let mut freed_bytes = 0u64;

    for hash in all_hashes {
        if !referenced.contains(hash.full()) {
            let path = store.path_for(&hash)?;
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            match store.delete(&hash) {
                Ok(()) => {
                    freed_bytes += size;
                    deleted += 1;
                }
                Err(e) => {
                    let _ = writeln!(
                        output.err,
                        "Warning: failed to delete unreferenced object {}: {e}",
                        hash.full()
                    );
                }
            }
        }
    }

    tracing::info!(deleted, freed_bytes, "garbage collection complete");

    Ok((deleted, freed_bytes))
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_store_and_retrieve() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let content = b"hello world";
        let hash = store.store(content).unwrap();

        assert!(store.exists(&hash));

        let retrieved = store.retrieve(&hash).unwrap();
        assert_eq!(retrieved, content);
    }

    #[test]
    fn test_deduplication() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let content = b"duplicate content";
        let hash1 = store.store(content).unwrap();
        let hash2 = store.store(content).unwrap();

        assert_eq!(hash1.full(), hash2.full());

        // Should only have one file
        let all = store.list_all().unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_path_components() {
        let hash = ContentHash::from_content(b"test");
        let (a, b, c) = hash.path_components().unwrap();

        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        assert!(!c.is_empty());
    }

    #[test]
    fn test_retrieve_not_found() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash = ContentHash::from_string("abcdef1234567890".repeat(4)).unwrap();
        let result = store.retrieve(&hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_exists() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let content = b"test content";
        let hash = store.store(content).unwrap();

        assert!(store.exists(&hash));
        let other_hash = ContentHash::from_content(b"other content");
        assert!(!store.exists(&other_hash));
    }

    #[test]
    fn test_path_for() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash = ContentHash::from_content(b"test");
        let path = store.path_for(&hash).unwrap();
        assert!(path.to_string_lossy().contains(".funveil/objects"));
    }

    #[test]
    fn test_delete() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let content = b"to be deleted";
        let hash = store.store(content).unwrap();
        assert!(store.exists(&hash));

        store.delete(&hash).unwrap();
        assert!(!store.exists(&hash));
    }

    #[test]
    fn test_delete_nonexistent() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash = ContentHash::from_content(b"nonexistent");
        let result = store.delete(&hash);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_all_empty() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hashes = store.list_all().unwrap();
        assert!(hashes.is_empty());
    }

    #[test]
    fn test_list_all() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash1 = store.store(b"content1").unwrap();
        let hash2 = store.store(b"content2").unwrap();

        let mut hashes = store.list_all().unwrap();
        hashes.sort_by_key(|h| h.full().to_string());
        assert_eq!(hashes.len(), 2);

        let hash_strs: Vec<&str> = hashes.iter().map(|h| h.full()).collect();
        assert!(hash_strs.contains(&hash1.full()));
        assert!(hash_strs.contains(&hash2.full()));
    }

    #[test]
    fn test_total_size_empty() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let size = store.total_size().unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_total_size() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        store.store(b"12345").unwrap();
        store.store(b"67890").unwrap();

        let size = store.total_size().unwrap();
        assert_eq!(size, 10);
    }

    #[test]
    fn test_garbage_collect() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash1 = store.store(b"keep me").unwrap();
        let _hash2 = store.store(b"delete me").unwrap();

        let (deleted, _bytes) =
            garbage_collect(temp.path(), &[hash1], &mut Output::new(false)).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(store.list_all().unwrap().len(), 1);
    }

    #[test]
    fn test_garbage_collect_none() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash1 = store.store(b"content").unwrap();

        let (deleted, _) = garbage_collect(
            temp.path(),
            std::slice::from_ref(&hash1),
            &mut Output::new(false),
        )
        .unwrap();
        assert_eq!(deleted, 0);
        assert!(store.exists(&hash1));
    }

    #[test]
    fn test_list_all_with_files() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash1 = store.store(b"content1").unwrap();
        let hash2 = store.store(b"content2").unwrap();

        let hashes = store.list_all().unwrap();
        assert_eq!(hashes.len(), 2);

        let hash_strs: Vec<&str> = hashes.iter().map(|h| h.full()).collect();
        assert!(hash_strs.contains(&hash1.full()));
        assert!(hash_strs.contains(&hash2.full()));
    }

    #[test]
    fn test_total_size_with_files() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        store.store(b"1234567890").unwrap();
        store.store(b"12345").unwrap();

        let size = store.total_size().unwrap();
        assert_eq!(size, 15);
    }

    #[test]
    fn test_retrieve_missing() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let fake_hash = ContentHash::from_string(
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
        )
        .unwrap();
        let result = store.retrieve(&fake_hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_missing() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let fake_hash = ContentHash::from_string(
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
        )
        .unwrap();
        let result = store.delete(&fake_hash);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_all_with_non_dirs() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        store.store(b"content").unwrap();

        let nested_path = store.root.join("ab").join("cd");
        fs::create_dir_all(&nested_path).unwrap();
        fs::write(nested_path.join("not_a_dir"), "file").unwrap();

        let a_path = store.root.join("xy");
        fs::create_dir_all(&a_path).unwrap();
        fs::write(a_path.join("file_in_first_level"), "data").unwrap();

        let hashes = store.list_all().unwrap();
        assert!(!hashes.is_empty());
    }

    #[test]
    fn test_total_size_with_non_dirs() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        store.store(b"content").unwrap();

        let nested_path = store.root.join("ab").join("cd");
        fs::create_dir_all(&nested_path).unwrap();
        fs::write(nested_path.join("extra_file"), "extra").unwrap();

        let a_path = store.root.join("xy");
        fs::create_dir_all(&a_path).unwrap();
        fs::write(a_path.join("file_in_first_level"), "data").unwrap();

        let size = store.total_size().unwrap();
        assert!(size > 0);
    }

    #[test]
    fn test_list_all_with_file_at_root() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        store.store(b"content").unwrap();

        fs::write(store.root.join("file_at_root"), "data").unwrap();

        let hashes = store.list_all().unwrap();
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn test_total_size_with_file_at_root() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        store.store(b"content").unwrap();

        fs::write(store.root.join("file_at_root"), "extra data").unwrap();

        let size = store.total_size().unwrap();
        assert!(size > 0);
    }

    #[test]
    fn test_list_all_with_symlink_in_path() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        store.store(b"content").unwrap();

        let dir_path = store.root.join("ab");
        fs::create_dir_all(&dir_path).unwrap();

        let link_path = dir_path.join("symlink");
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let target = store.root.join("ab").join("cd");
            fs::create_dir_all(&target).unwrap();
            symlink(&target, &link_path).ok();
        }

        let hashes = store.list_all().unwrap();
        assert!(!hashes.is_empty());
    }

    #[test]
    fn test_store_retrieve_binary_data() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let binary_data: Vec<u8> = (0..=255).collect();
        let hash = store.store(&binary_data).unwrap();
        let retrieved = store.retrieve(&hash).unwrap();
        assert_eq!(
            retrieved, binary_data,
            "binary round-trip should be byte-for-byte equal"
        );
    }

    #[test]
    fn test_store_double_store_returns_same_hash() {
        // BUG-034 regression: double-store should return the same hash without error
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let content = b"atomic test content";
        let hash1 = store.store(content).unwrap();
        let hash2 = store.store(content).unwrap();
        assert_eq!(hash1.full(), hash2.full());

        // Content should still be retrievable
        let retrieved = store.retrieve(&hash1).unwrap();
        assert_eq!(retrieved, content);
    }

    #[test]
    fn test_garbage_collect_freed_bytes_positive() {
        // BUG-036 regression: freed_bytes should be > 0 after collecting a known object
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash1 = store.store(b"keep me").unwrap();
        let _hash2 = store.store(b"delete me please").unwrap();

        let (deleted, freed) =
            garbage_collect(temp.path(), &[hash1], &mut Output::new(false)).unwrap();
        assert_eq!(deleted, 1);
        assert!(freed > 0, "freed_bytes should be positive after GC");
    }

    // --- Tests targeting specific missed mutants ---

    #[test]
    fn test_total_size_with_objects() {
        // Catches: is_file() check (line 141) and += mutation (line 142)
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let content1 = b"hello";
        let content2 = b"world!!";
        store.store(content1).unwrap();
        store.store(content2).unwrap();

        let size = store.total_size().unwrap();
        assert!(size >= content1.len() as u64 + content2.len() as u64);
    }

    #[test]
    fn test_total_size_empty_store() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());
        let size = store.total_size().unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_list_all_returns_stored_hashes() {
        // Catches: is_file()/is_dir() checks in list_all traversal (lines 89, 96, 103)
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash1 = store.store(b"content1").unwrap();
        let hash2 = store.store(b"content2").unwrap();

        let all = store.list_all().unwrap();
        let full_hashes: Vec<_> = all.iter().map(|h| h.full().to_string()).collect();
        assert!(full_hashes.contains(&hash1.full().to_string()));
        assert!(full_hashes.contains(&hash2.full().to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn test_store_fails_when_objects_dir_not_writable() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        // First store something so the objects dir structure exists
        store.store(b"seed").unwrap();

        // Make the objects directory read-only so new subdirs cannot be created
        let objects_dir = temp.path().join(OBJECTS_DIR);
        let perms = fs::Permissions::from_mode(0o444);
        fs::set_permissions(&objects_dir, perms).unwrap();

        // Attempting to store new content should fail (not AlreadyExists, but permission denied)
        let result = store.store(b"this should fail");
        assert!(
            result.is_err(),
            "store should fail when objects dir is read-only"
        );

        // Cleanup: restore permissions so tempdir can be deleted
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&objects_dir, perms).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn test_garbage_collect_warns_on_delete_failure() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let hash_keep = store.store(b"keep me").unwrap();
        let hash_delete = store.store(b"try to delete me").unwrap();

        // Make the directory containing the unreferenced object read-only
        // so that fs::remove_file fails
        let (a, b, _c) = hash_delete.path_components().unwrap();
        let obj_dir = temp.path().join(OBJECTS_DIR).join(a).join(b);
        let perms = fs::Permissions::from_mode(0o555);
        fs::set_permissions(&obj_dir, perms).unwrap();

        let mut output = Output::new(false);
        let (deleted, _freed) =
            garbage_collect(temp.path(), std::slice::from_ref(&hash_keep), &mut output).unwrap();

        // The delete should have failed, so deleted count should be 0
        // (the unreferenced object could not be removed)
        assert_eq!(deleted, 0, "should not count failed deletes");

        // The kept object should still exist
        assert!(store.exists(&hash_keep));

        // Cleanup: restore permissions
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&obj_dir, perms).unwrap();
    }

    #[test]
    fn test_list_all_with_nested_dir_at_third_level() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        store.store(b"content").unwrap();

        let nested_path = store.root.join("ab").join("cd").join("nested_dir");
        fs::create_dir_all(&nested_path).unwrap();

        let hashes = store.list_all().unwrap();
        assert!(!hashes.is_empty());
    }

    #[test]
    fn test_store_detects_hash_collision() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let content = b"original content";
        let hash = store.store(content).unwrap();

        // Corrupt the stored file
        let path = store.path_for(&hash).unwrap();
        fs::write(&path, b"corrupted content").unwrap();

        // Storing different content with the same hash should detect collision
        let result = store.store(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, FunveilError::HashCollision { .. }),
            "expected HashCollision, got: {err:?}"
        );
    }

    #[test]
    fn test_store_same_content_idempotent() {
        let temp = TempDir::new().unwrap();
        let store = ContentStore::new(temp.path());

        let content = b"idempotent test";
        let hash1 = store.store(content).unwrap();
        let hash2 = store.store(content).unwrap();
        assert_eq!(hash1.full(), hash2.full());
    }
}
