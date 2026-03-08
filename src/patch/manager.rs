//! Patch Management System
//!
//! Tracks applied patches with ordering, supports apply/unapply/yank

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{FunveilError, Result};

use super::parser::{FilePatch, Hunk, Line, ParsedPatch, PatchParser};

/// Unique identifier for a patch
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PatchId(pub u64);

/// A patch with metadata
#[derive(Debug, Clone)]
pub struct Patch {
    pub id: PatchId,
    pub name: String,
    pub raw_content: String,
    pub parsed: ParsedPatch,
    pub metadata: PatchMetadata,
}

/// Metadata for a patch
#[derive(Debug, Clone)]
pub struct PatchMetadata {
    pub applied_at: chrono::DateTime<chrono::Utc>,
    pub files_affected: Vec<PathBuf>,
    pub description: Option<String>,
}

impl PatchMetadata {
    pub fn new(files: Vec<PathBuf>) -> Self {
        Self {
            applied_at: chrono::Utc::now(),
            files_affected: files,
            description: None,
        }
    }
}

/// Manages the queue of applied patches
pub struct PatchManager {
    queue: VecDeque<Patch>,
    storage: PatchStorage,
    next_id: u64,
}

impl PatchManager {
    /// Create a new patch manager
    pub fn new(project_root: &Path) -> Result<Self> {
        let storage = PatchStorage::new(project_root)?;
        let queue = storage.load_queue()?;
        let next_id = queue.back().map(|p| p.id.0 + 1).unwrap_or(1);

        Ok(Self {
            queue,
            storage,
            next_id,
        })
    }

    /// Apply a new patch
    pub fn apply(&mut self, patch_content: &str, name: &str) -> Result<PatchId> {
        // Parse the patch
        let parsed = PatchParser::parse_patch(patch_content)?;

        // Validate the patch doesn't modify veiled lines
        // TODO: Check against veiled regions

        // Create patch
        let id = PatchId(self.next_id);
        self.next_id += 1;

        let files_affected = parsed
            .files
            .iter()
            .filter_map(|f| f.new_path.clone().or(f.old_path.clone()))
            .collect();

        let patch = Patch {
            id,
            name: name.to_string(),
            raw_content: patch_content.to_string(),
            parsed,
            metadata: PatchMetadata::new(files_affected),
        };

        // Apply to working tree
        self.apply_to_working_tree(&patch)?;

        // Save to storage
        self.storage.save_patch(&patch)?;

        // Add to queue
        self.queue.push_back(patch);

        // Save queue
        self.storage.save_queue(&self.queue)?;

        Ok(id)
    }

    /// Unapply (revert) the latest patch
    pub fn unapply(&mut self, id: PatchId) -> Result<()> {
        // Find the patch
        let pos = self
            .queue
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| FunveilError::NotVeiled(format!("Patch {:?} not found", id.0)))?;

        // Check if it's the last patch
        if pos != self.queue.len() - 1 {
            return Err(FunveilError::TreeSitterError(format!(
                "Can only unapply the latest patch. Patch {:?} is not the latest.",
                id.0
            )));
        }

        // Get the patch
        let patch = self.queue.pop_back().unwrap();

        // Unapply from working tree (apply reverse)
        self.unapply_from_working_tree(&patch)?;

        // Update storage
        self.storage.save_queue(&self.queue)?;

        Ok(())
    }

    /// Yank (remove) a patch from the middle
    pub fn yank(&mut self, id: PatchId) -> Result<YankReport> {
        // Find the patch position
        let pos = self
            .queue
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| FunveilError::NotVeiled(format!("Patch {:?} not found", id.0)))?;

        // Get patches after the target
        let subsequent: Vec<_> = self.queue.iter().skip(pos + 1).cloned().collect();

        // Unapply subsequent patches in reverse order
        for patch in subsequent.iter().rev() {
            self.unapply_from_working_tree(patch)?;
        }

        // Unapply target patch
        let target = self.queue.remove(pos).unwrap();
        self.unapply_from_working_tree(&target)?;

        // Delete from storage
        self.storage.delete_patch(id)?;

        // Re-apply subsequent patches
        let mut reapplied = Vec::new();
        let mut conflicts = Vec::new();

        for patch in subsequent {
            match self.apply_to_working_tree(&patch) {
                Ok(_) => {
                    reapplied.push(patch.id);
                    // Update the patch in queue
                    if let Some(existing) = self.queue.iter_mut().find(|p| p.id == patch.id) {
                        *existing = patch;
                    }
                }
                Err(e) => {
                    conflicts.push(YankConflict {
                        patch_id: patch.id,
                        error: e.to_string(),
                    });
                }
            }
        }

        // Save queue
        self.storage.save_queue(&self.queue)?;

        Ok(YankReport {
            yanked_id: id,
            reapplied,
            conflicts,
        })
    }

    /// List all patches in order
    pub fn list(&self) -> Vec<PatchSummary> {
        self.queue
            .iter()
            .map(|p| PatchSummary {
                id: p.id,
                name: p.name.clone(),
                applied_at: p.metadata.applied_at,
                files: p.metadata.files_affected.clone(),
            })
            .collect()
    }

    /// Get a specific patch
    pub fn get(&self, id: PatchId) -> Option<&Patch> {
        self.queue.iter().find(|p| p.id == id)
    }

    /// Apply patch to working tree
    fn apply_to_working_tree(&self, patch: &Patch) -> Result<()> {
        for file_patch in &patch.parsed.files {
            self.apply_file_patch(file_patch)?;
        }
        Ok(())
    }

    /// Apply a single file patch
    fn apply_file_patch(&self, file_patch: &FilePatch) -> Result<()> {
        use std::io::Write;

        let path = match &file_patch.new_path {
            Some(p) => p,
            None => {
                // Deleted file
                if let Some(old) = &file_patch.old_path {
                    let full_path = self.storage.project_root.join(old);
                    if full_path.exists() {
                        fs::remove_file(&full_path)?;
                    }
                }
                return Ok(());
            }
        };

        let full_path = self.storage.project_root.join(path);

        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Read existing content or start empty
        let mut content = if full_path.exists() {
            fs::read_to_string(&full_path)?
        } else {
            String::new()
        };

        // Apply each hunk
        for hunk in &file_patch.hunks {
            content = self.apply_hunk(&content, hunk)?;
        }

        // Write back
        let mut file = fs::File::create(&full_path)?;
        file.write_all(content.as_bytes())?;

        Ok(())
    }

    /// Apply a hunk to content
    fn apply_hunk(&self, content: &str, hunk: &Hunk) -> Result<String> {
        let lines: Vec<&str> = content.lines().collect();
        let mut result = Vec::new();

        // Add lines before the hunk (1-indexed to 0-indexed)
        let start_idx = hunk.old_start.saturating_sub(1);
        result.extend_from_slice(&lines[..start_idx]);

        // Track position in original file
        let mut old_pos = start_idx;

        // Process hunk lines
        for line in &hunk.lines {
            match line {
                Line::Context(text) => {
                    // Verify context matches
                    if old_pos < lines.len() && lines[old_pos] == text.as_str() {
                        result.push(lines[old_pos]);
                        old_pos += 1;
                    } else {
                        // Context mismatch - still add the expected line
                        result.push(text.as_str());
                        old_pos += 1;
                    }
                }
                Line::Delete(text) => {
                    // Skip this line (verify it matches)
                    if old_pos < lines.len() && lines[old_pos] == text.as_str() {
                        old_pos += 1;
                    }
                }
                Line::Add(text) => {
                    // Add new line
                    result.push(text.as_str());
                }
                Line::NoNewline => {
                    // Marker for no newline at end of file
                }
            }
        }

        // Skip any remaining deleted lines
        old_pos = old_pos.min(lines.len());

        // Add lines after the hunk
        result.extend_from_slice(&lines[old_pos..]);

        Ok(result.join("\n"))
    }

    /// Unapply (revert) patch from working tree
    fn unapply_from_working_tree(&self, patch: &Patch) -> Result<()> {
        // Generate reverse patch
        let reverse = self.generate_reverse_patch(patch);

        // Apply reverse
        for file_patch in &reverse.files {
            self.apply_file_patch(file_patch)?;
        }

        Ok(())
    }

    /// Generate a reverse patch for unapplying
    fn generate_reverse_patch(&self, patch: &Patch) -> ParsedPatch {
        let mut reversed_files = Vec::new();

        for file in &patch.parsed.files {
            let mut reversed_hunks = Vec::new();

            for hunk in &file.hunks {
                // Swap old and new ranges
                let reversed_lines: Vec<_> = hunk
                    .lines
                    .iter()
                    .map(|line| match line {
                        Line::Context(t) => Line::Context(t.clone()),
                        Line::Delete(t) => Line::Add(t.clone()),
                        Line::Add(t) => Line::Delete(t.clone()),
                        Line::NoNewline => Line::NoNewline,
                    })
                    .collect();

                reversed_hunks.push(Hunk {
                    old_start: hunk.new_start,
                    old_count: hunk.new_count,
                    new_start: hunk.old_start,
                    new_count: hunk.old_count,
                    section: hunk.section.clone(),
                    lines: reversed_lines,
                });
            }

            reversed_files.push(FilePatch {
                old_path: file.new_path.clone(),
                new_path: file.old_path.clone(),
                old_mode: file.new_mode.clone(),
                new_mode: file.old_mode.clone(),
                is_new_file: file.is_deleted,
                is_deleted: file.is_new_file,
                is_rename: file.is_rename,
                is_copy: file.is_copy,
                is_binary: file.is_binary,
                hunks: reversed_hunks,
                similarity: file.similarity,
            });
        }

        ParsedPatch {
            files: reversed_files,
            format: patch.parsed.format,
        }
    }
}

/// Report of a yank operation
#[derive(Debug, Clone)]
pub struct YankReport {
    pub yanked_id: PatchId,
    pub reapplied: Vec<PatchId>,
    pub conflicts: Vec<YankConflict>,
}

/// A conflict during yank
#[derive(Debug, Clone)]
pub struct YankConflict {
    pub patch_id: PatchId,
    pub error: String,
}

/// Summary of a patch for listing
#[derive(Debug, Clone)]
pub struct PatchSummary {
    pub id: PatchId,
    pub name: String,
    pub applied_at: chrono::DateTime<chrono::Utc>,
    pub files: Vec<PathBuf>,
}

/// Storage for patches
pub struct PatchStorage {
    project_root: PathBuf,
    patches_dir: PathBuf,
}

impl PatchStorage {
    /// Create new patch storage
    pub fn new(project_root: &Path) -> Result<Self> {
        let patches_dir = project_root.join(".funveil").join("patches");
        fs::create_dir_all(&patches_dir)?;

        Ok(Self {
            project_root: project_root.to_path_buf(),
            patches_dir,
        })
    }

    /// Save a patch
    pub fn save_patch(&self, patch: &Patch) -> Result<()> {
        let patch_dir = self
            .patches_dir
            .join(format!("{:04}- {}", patch.id.0, patch.name));
        fs::create_dir_all(&patch_dir)?;

        // Save raw content
        fs::write(patch_dir.join("patch.raw"), &patch.raw_content)?;

        // Save metadata
        let metadata = serde_yaml::to_string(&PatchMetadataSer {
            id: patch.id.0,
            name: patch.name.clone(),
            applied_at: patch.metadata.applied_at,
            files: patch
                .metadata
                .files_affected
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            description: patch.metadata.description.clone(),
        })?;
        fs::write(patch_dir.join("metadata.yaml"), metadata)?;

        Ok(())
    }

    /// Delete a patch
    pub fn delete_patch(&self, id: PatchId) -> Result<()> {
        // Find patch directory
        for entry in fs::read_dir(&self.patches_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&format!("{:04}-", id.0)) {
                fs::remove_dir_all(entry.path())?;
                return Ok(());
            }
        }
        Ok(())
    }

    /// Load the patch queue
    pub fn load_queue(&self) -> Result<VecDeque<Patch>> {
        let queue_file = self.patches_dir.join("queue.yaml");
        if !queue_file.exists() {
            return Ok(VecDeque::new());
        }

        let content = fs::read_to_string(&queue_file)?;
        let queue_data: Vec<PatchQueueEntry> = serde_yaml::from_str(&content)?;

        let mut queue = VecDeque::new();
        for entry in queue_data {
            // Load patch from directory
            if let Some(patch) = self.load_patch(entry.id)? {
                queue.push_back(patch);
            }
        }

        Ok(queue)
    }

    /// Save the patch queue
    pub fn save_queue(&self, queue: &VecDeque<Patch>) -> Result<()> {
        let queue_file = self.patches_dir.join("queue.yaml");

        let entries: Vec<PatchQueueEntry> = queue
            .iter()
            .map(|p| PatchQueueEntry {
                id: p.id.0,
                name: p.name.clone(),
            })
            .collect();

        let content = serde_yaml::to_string(&entries)?;
        fs::write(&queue_file, content)?;

        Ok(())
    }

    /// Load a specific patch
    fn load_patch(&self, id: u64) -> Result<Option<Patch>> {
        // Find patch directory
        for entry in fs::read_dir(&self.patches_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&format!("{:04}-", id)) {
                // Load metadata
                let metadata_content = fs::read_to_string(entry.path().join("metadata.yaml"))?;
                let metadata: PatchMetadataSer = serde_yaml::from_str(&metadata_content)?;

                // Load raw content
                let raw_content = fs::read_to_string(entry.path().join("patch.raw"))?;

                // Parse the patch
                let parsed = PatchParser::parse_patch(&raw_content)?;

                return Ok(Some(Patch {
                    id: PatchId(id),
                    name: metadata.name,
                    raw_content,
                    parsed,
                    metadata: PatchMetadata {
                        applied_at: metadata.applied_at,
                        files_affected: metadata.files.iter().map(PathBuf::from).collect(),
                        description: metadata.description,
                    },
                }));
            }
        }

        Ok(None)
    }
}

/// Serializable patch metadata
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PatchMetadataSer {
    id: u64,
    name: String,
    applied_at: chrono::DateTime<chrono::Utc>,
    files: Vec<String>,
    description: Option<String>,
}

/// Entry in the patch queue
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct PatchQueueEntry {
    id: u64,
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_patch_manager_apply() {
        let temp = TempDir::new().unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+line 2 modified
 line 3
"#;

        let id = manager.apply(patch, "test-patch").unwrap();
        assert_eq!(id.0, 1);

        // Check file was created
        let content = fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert!(content.contains("line 2 modified"));
    }

    #[test]
    fn test_patch_manager_list() {
        let temp = TempDir::new().unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch1 = r#"--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-old
+new
"#;
        let patch2 = r#"--- a/b.txt
+++ b/b.txt
@@ -1 +1 @@
-foo
+bar
"#;

        manager.apply(patch1, "patch-1").unwrap();
        manager.apply(patch2, "patch-2").unwrap();

        let list = manager.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "patch-1");
        assert_eq!(list[1].name, "patch-2");
    }
}
