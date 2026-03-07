use crate::config::OBJECTS_DIR;
use crate::error::{FunveilError, Result};
use crate::types::ContentHash;
use std::fs;
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
        let (a, b, c) = hash.path_components();

        let dir = self.root.join(a).join(b);
        fs::create_dir_all(&dir)?;

        let path = dir.join(c);
        if !path.exists() {
            fs::write(&path, content)?;
        }

        Ok(hash)
    }

    /// Retrieve content by hash
    pub fn retrieve(&self, hash: &ContentHash) -> Result<Vec<u8>> {
        let (a, b, c) = hash.path_components();
        let path = self.root.join(a).join(b).join(c);

        if !path.exists() {
            return Err(FunveilError::ObjectNotFound(hash.full().to_string()));
        }

        Ok(fs::read(&path)?)
    }

    /// Check if content exists
    pub fn exists(&self, hash: &ContentHash) -> bool {
        let (a, b, c) = hash.path_components();
        self.root.join(a).join(b).join(c).exists()
    }

    /// Get the path for a hash (for debugging)
    pub fn path_for(&self, hash: &ContentHash) -> PathBuf {
        let (a, b, c) = hash.path_components();
        self.root.join(a).join(b).join(c)
    }

    /// Delete content by hash (for garbage collection)
    pub fn delete(&self, hash: &ContentHash) -> Result<()> {
        let (a, b, c) = hash.path_components();
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
                    hashes.push(ContentHash::from_string(full_hash));
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
pub fn garbage_collect(root: &Path, referenced_hashes: &[ContentHash]) -> Result<(usize, u64)> {
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
            let path = store.path_for(&hash);
            if let Ok(metadata) = fs::metadata(&path) {
                freed_bytes += metadata.len();
            }
            store.delete(&hash)?;
            deleted += 1;
        }
    }

    Ok((deleted, freed_bytes))
}

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
        let (a, b, c) = hash.path_components();

        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        assert!(!c.is_empty());
    }
}
