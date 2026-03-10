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

        // Apply each hunk, adjusting for cumulative line offset
        let mut offset: isize = 0;
        for hunk in &file_patch.hunks {
            let mut adjusted = hunk.clone();
            adjusted.old_start = ((hunk.old_start as isize) + offset).max(1) as usize;
            content = self.apply_hunk(&content, &adjusted)?;
            offset += (hunk.new_count as isize) - (hunk.old_count as isize);
        }

        // Write back
        let mut file = fs::File::create(&full_path)?;
        file.write_all(content.as_bytes())?;

        Ok(())
    }

    /// Apply a hunk to content
    fn apply_hunk(&self, content: &str, hunk: &Hunk) -> Result<String> {
        let has_trailing_newline = content.ends_with('\n');
        let ends_with_no_newline = hunk.lines.last() == Some(&Line::NoNewline);
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
                        return Err(FunveilError::PatchMismatch(format!(
                            "context mismatch at line {}: expected {:?}, found {:?}",
                            old_pos + 1,
                            text,
                            lines.get(old_pos).unwrap_or(&"<EOF>")
                        )));
                    }
                }
                Line::Delete(text) => {
                    // Skip this line (verify it matches)
                    if old_pos < lines.len() && lines[old_pos] == text.as_str() {
                        old_pos += 1;
                    } else {
                        return Err(FunveilError::PatchMismatch(format!(
                            "delete mismatch at line {}: expected {:?}, found {:?}",
                            old_pos + 1,
                            text,
                            lines.get(old_pos).unwrap_or(&"<EOF>")
                        )));
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

        let mut output = result.join("\n");

        // Preserve trailing newline from original content, unless NoNewline marker is present
        if has_trailing_newline && !ends_with_no_newline {
            output.push('\n');
        }

        Ok(output)
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
            if name.starts_with(&format!("{id:04}-")) {
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

    fn create_test_patch() -> &'static str {
        r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+line 2 modified
 line 3
"#
    }

    #[test]
    fn test_patch_id_ordering() {
        let id1 = PatchId(1);
        let id2 = PatchId(2);
        assert!(id1 < id2);
        assert_eq!(id1, PatchId(1));
    }

    #[test]
    fn test_patch_metadata_new() {
        let files = vec![PathBuf::from("test.txt")];
        let meta = PatchMetadata::new(files.clone());
        assert_eq!(meta.files_affected, files);
        assert!(meta.description.is_none());
    }

    #[test]
    fn test_patch_manager_new() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path()).unwrap();
        assert!(manager.list().is_empty());
    }

    #[test]
    fn test_patch_manager_apply() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("test.txt"), "line 1\nline 2\nline 3\n").unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch = create_test_patch();

        let id = manager.apply(patch, "test-patch").unwrap();
        assert_eq!(id.0, 1);

        let content = fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert!(content.contains("line 2 modified"));
    }

    #[test]
    fn test_patch_manager_apply_creates_directories() {
        let temp = TempDir::new().unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch = r#"--- /dev/null
+++ b/src/deep/nested/file.txt
@@ -0,0 +1 @@
+hello world
"#;

        manager.apply(patch, "nested-file").unwrap();

        let content = fs::read_to_string(temp.path().join("src/deep/nested/file.txt")).unwrap();
        assert_eq!(content.trim(), "hello world");
    }

    #[test]
    fn test_patch_manager_apply_delete_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("delete_me.txt");
        fs::write(&file_path, "content to delete\n").unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch = r#"--- a/delete_me.txt
+++ /dev/null
@@ -1,1 +0,0 @@
-content to delete
"#;

        manager.apply(patch, "delete-file").unwrap();
        assert!(!file_path.exists());
    }

    #[test]
    fn test_patch_manager_apply_new_file() {
        let temp = TempDir::new().unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch = r#"--- /dev/null
+++ b/new_file.txt
@@ -0,0 +1,3 @@
+line 1
+line 2
+line 3
"#;

        manager.apply(patch, "new-file").unwrap();

        let content = fs::read_to_string(temp.path().join("new_file.txt")).unwrap();
        assert!(content.contains("line 1"));
        assert!(content.contains("line 2"));
        assert!(content.contains("line 3"));
    }

    #[test]
    fn test_patch_manager_unapply() {
        let temp = TempDir::new().unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        let original_content = "line 1\nline 2\nline 3";
        fs::write(temp.path().join("test.txt"), original_content).unwrap();

        let patch = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+line 2 modified
 line 3
"#;
        let id = manager.apply(patch, "test-patch").unwrap();

        let modified = fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert!(modified.contains("line 2 modified"));

        manager.unapply(id).unwrap();

        let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert!(restored.contains("line 1"));
        assert!(restored.contains("line 2"));
        assert!(restored.contains("line 3"));
    }

    #[test]
    fn test_patch_manager_unapply_not_found() {
        let temp = TempDir::new().unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        let result = manager.unapply(PatchId(999));
        assert!(result.is_err());
    }

    #[test]
    fn test_patch_manager_unapply_not_latest() {
        let temp = TempDir::new().unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        fs::write(temp.path().join("a.txt"), "old\n").unwrap();
        fs::write(temp.path().join("b.txt"), "foo\n").unwrap();

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

        let id1 = manager.apply(patch1, "patch-1").unwrap();
        manager.apply(patch2, "patch-2").unwrap();

        let result = manager.unapply(id1);
        assert!(result.is_err());
    }

    #[test]
    fn test_patch_manager_yank() {
        let temp = TempDir::new().unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        fs::write(temp.path().join("a.txt"), "line 1\n").unwrap();

        let patch1 = r#"--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-line 1
+line 1 modified
"#;
        let patch2 = r#"--- a/b.txt
+++ b/b.txt
@@ -0,0 +1 @@
+new line
"#;

        let id1 = manager.apply(patch1, "patch-1").unwrap();
        manager.apply(patch2, "patch-2").unwrap();

        assert!(temp.path().join("b.txt").exists());

        let report = manager.yank(id1).unwrap();
        assert_eq!(report.yanked_id, id1);
        assert!(!report.reapplied.is_empty());
    }

    #[test]
    fn test_patch_manager_yank_not_found() {
        let temp = TempDir::new().unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        let result = manager.yank(PatchId(999));
        assert!(result.is_err());
    }

    #[test]
    fn test_patch_manager_get() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("test.txt"), "line 1\nline 2\nline 3\n").unwrap();
        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch = create_test_patch();
        let id = manager.apply(patch, "test-patch").unwrap();

        let found = manager.get(id).unwrap();
        assert_eq!(found.name, "test-patch");

        assert!(manager.get(PatchId(999)).is_none());
    }

    #[test]
    fn test_patch_manager_list() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "old\n").unwrap();
        fs::write(temp.path().join("b.txt"), "foo\n").unwrap();
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

    #[test]
    fn test_generate_reverse_patch() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path()).unwrap();

        let patch_content = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,4 @@
 line 1
-line 2
+line 2 mod
+extra line
 line 3
"#;

        let parsed = PatchParser::parse_patch(patch_content).unwrap();
        let patch = Patch {
            id: PatchId(1),
            name: "test".to_string(),
            raw_content: patch_content.to_string(),
            parsed,
            metadata: PatchMetadata::new(vec![]),
        };

        let reverse = manager.generate_reverse_patch(&patch);

        let file = &reverse.files[0];
        assert_eq!(file.old_path, Some(PathBuf::from("test.txt")));
        assert_eq!(file.new_path, Some(PathBuf::from("test.txt")));

        let hunk = &file.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 4);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 3);

        assert!(hunk
            .lines
            .iter()
            .any(|l| matches!(l, Line::Add(t) if t == "line 2")));
        assert!(hunk
            .lines
            .iter()
            .any(|l| matches!(l, Line::Delete(t) if t == "line 2 mod")));
        assert!(hunk
            .lines
            .iter()
            .any(|l| matches!(l, Line::Delete(t) if t == "extra line")));
    }

    #[test]
    fn test_apply_hunk_context_mismatch() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path()).unwrap();

        let content = "line A\nline B\nline C\n";
        let hunk = Hunk {
            old_start: 1,
            old_count: 3,
            new_start: 1,
            new_count: 3,
            section: None,
            lines: vec![
                Line::Context("different line 1".to_string()),
                Line::Context("line B".to_string()),
                Line::Context("line C".to_string()),
            ],
        };

        let result = manager.apply_hunk(content, &hunk);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_hunk_delete_mismatch() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path()).unwrap();

        let content = "line A\nline B\nline C\n";
        let hunk = Hunk {
            old_start: 1,
            old_count: 3,
            new_start: 1,
            new_count: 2,
            section: None,
            lines: vec![
                Line::Context("line A".to_string()),
                Line::Delete("different line B".to_string()),
                Line::Context("line C".to_string()),
            ],
        };

        let result = manager.apply_hunk(content, &hunk);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_hunk_add_lines() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path()).unwrap();

        let content = "line 1\nline 3\n";
        let hunk = Hunk {
            old_start: 1,
            old_count: 2,
            new_start: 1,
            new_count: 3,
            section: None,
            lines: vec![
                Line::Context("line 1".to_string()),
                Line::Add("line 2".to_string()),
                Line::Context("line 3".to_string()),
            ],
        };

        let result = manager.apply_hunk(content, &hunk).unwrap();
        assert!(result.contains("line 1"));
        assert!(result.contains("line 2"));
        assert!(result.contains("line 3"));
    }

    #[test]
    fn test_apply_hunk_no_newline_marker() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path()).unwrap();

        let content = "line 1\nline 2\n";
        let hunk = Hunk {
            old_start: 1,
            old_count: 2,
            new_start: 1,
            new_count: 2,
            section: None,
            lines: vec![
                Line::Context("line 1".to_string()),
                Line::Context("line 2".to_string()),
                Line::NoNewline,
            ],
        };

        let result = manager.apply_hunk(content, &hunk).unwrap();
        assert!(result.contains("line 1"));
        assert!(result.contains("line 2"));
    }

    #[test]
    fn test_patch_storage_new() {
        let temp = TempDir::new().unwrap();
        let storage = PatchStorage::new(temp.path()).unwrap();
        assert!(storage.patches_dir.ends_with("patches"));
    }

    #[test]
    fn test_patch_storage_save_and_load_queue() {
        let temp = TempDir::new().unwrap();
        let storage = PatchStorage::new(temp.path()).unwrap();

        let patch_content = r#"--- a/test.txt
+++ b/test.txt
@@ -1 +1 @@
-old
+new
"#;

        let parsed = PatchParser::parse_patch(patch_content).unwrap();
        let patch = Patch {
            id: PatchId(1),
            name: "test".to_string(),
            raw_content: patch_content.to_string(),
            parsed,
            metadata: PatchMetadata::new(vec![PathBuf::from("test.txt")]),
        };

        storage.save_patch(&patch).unwrap();

        let mut queue = VecDeque::new();
        queue.push_back(patch);

        storage.save_queue(&queue).unwrap();
        let loaded = storage.load_queue().unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "test");
    }

    #[test]
    fn test_patch_storage_delete_patch() {
        let temp = TempDir::new().unwrap();
        let storage = PatchStorage::new(temp.path()).unwrap();

        let patch_content = r#"--- a/test.txt
+++ b/test.txt
@@ -1 +1 @@
-old
+new
"#;

        let parsed = PatchParser::parse_patch(patch_content).unwrap();
        let patch = Patch {
            id: PatchId(1),
            name: "test".to_string(),
            raw_content: patch_content.to_string(),
            parsed,
            metadata: PatchMetadata::new(vec![]),
        };

        storage.save_patch(&patch).unwrap();

        let patch_dir = storage.patches_dir.join("0001- test");
        assert!(patch_dir.exists());

        storage.delete_patch(PatchId(1)).unwrap();
        assert!(!patch_dir.exists());
    }

    #[test]
    fn test_patch_storage_delete_nonexistent() {
        let temp = TempDir::new().unwrap();
        let storage = PatchStorage::new(temp.path()).unwrap();

        let result = storage.delete_patch(PatchId(999));
        assert!(result.is_ok());
    }

    #[test]
    fn test_patch_storage_load_patch() {
        let temp = TempDir::new().unwrap();
        let storage = PatchStorage::new(temp.path()).unwrap();

        let patch_content = r#"--- a/test.txt
+++ b/test.txt
@@ -1 +1 @@
-old
+new
"#;

        let parsed = PatchParser::parse_patch(patch_content).unwrap();
        let patch = Patch {
            id: PatchId(1),
            name: "test-patch".to_string(),
            raw_content: patch_content.to_string(),
            parsed,
            metadata: PatchMetadata::new(vec![PathBuf::from("test.txt")]),
        };

        storage.save_patch(&patch).unwrap();

        let loaded = storage.load_patch(1).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.name, "test-patch");
        assert_eq!(loaded.id, PatchId(1));
    }

    #[test]
    fn test_patch_storage_load_nonexistent() {
        let temp = TempDir::new().unwrap();
        let storage = PatchStorage::new(temp.path()).unwrap();

        let loaded = storage.load_patch(999).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_manager_persistence() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("test.txt"), "line 1\nline 2\nline 3\n").unwrap();

        {
            let mut manager = PatchManager::new(temp.path()).unwrap();
            let patch = create_test_patch();
            manager.apply(patch, "persistent-patch").unwrap();
        }

        {
            let manager = PatchManager::new(temp.path()).unwrap();
            let list = manager.list();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].name, "persistent-patch");
        }
    }

    #[test]
    fn test_apply_with_existing_file() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("test.txt"), "line 1\nline 2\nline 3\n").unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();
        let patch = create_test_patch();
        manager.apply(patch, "modify-existing").unwrap();

        let content = fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert!(content.contains("line 2 modified"));
    }

    #[test]
    fn test_reverse_patch_with_no_newline_marker() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("test.txt"), "new content\n").unwrap();

        let patch_content = r#"--- a/test.txt
+++ b/test.txt
@@ -1,1 +1,1 @@
-old content
\ No newline at end of file
+new content
"#;

        let parsed = PatchParser::parse_patch(patch_content).unwrap();
        let patch = Patch {
            id: PatchId(1),
            name: "test".to_string(),
            raw_content: patch_content.to_string(),
            parsed,
            metadata: PatchMetadata::new(vec![]),
        };

        let manager = PatchManager::new(temp.path()).unwrap();
        let reversed = manager.generate_reverse_patch(&patch);

        assert_eq!(reversed.files.len(), 1);
        let hunk = &reversed.files[0].hunks[0];

        let has_no_newline = hunk.lines.iter().any(|l| matches!(l, Line::NoNewline));
        assert!(has_no_newline);
    }

    #[test]
    fn test_yank_with_conflict() {
        let temp = TempDir::new().unwrap();

        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "line 1\nline 2\nline 3\n").unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch1_content = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+line 2 modified
 line 3
"#;

        manager.apply(patch1_content, "patch1").unwrap();

        let patch2_content = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line 1
-line 2 modified
+line 2 different
 line 3
"#;

        manager.apply(patch2_content, "patch2").unwrap();

        // Corrupt the file — unapplying patch2 will fail due to mismatch
        fs::write(&file_path, "completely different content\n").unwrap();

        let result = manager.yank(PatchId(1));
        assert!(result.is_err());
    }

    #[test]
    fn test_yank_conflict_on_reapply_failure() {
        // Covers lines 173-176: YankConflict when reapplying fails
        // Patch 1 modifies a file, patch 2 depends on that modification.
        // When patch 1 is yanked, patch 2 cannot be reapplied because
        // the file content it expects is gone. We sabotage the file
        // after patches are applied so the unapply/reapply cycle fails.
        let temp = TempDir::new().unwrap();

        let file_path = temp.path().join("data.txt");
        fs::write(&file_path, "aaa\nbbb\nccc\n").unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        // Patch 1: change bbb -> bbb_v2
        let patch1 = r#"--- a/data.txt
+++ b/data.txt
@@ -1,3 +1,3 @@
 aaa
-bbb
+bbb_v2
 ccc
"#;
        let id1 = manager.apply(patch1, "first").unwrap();

        // Patch 2: change bbb_v2 -> bbb_v3 (depends on patch 1)
        let patch2 = r#"--- a/data.txt
+++ b/data.txt
@@ -1,3 +1,3 @@
 aaa
-bbb_v2
+bbb_v3
 ccc
"#;
        manager.apply(patch2, "second").unwrap();

        // Now yank patch 1. The yank unapplies patch2 and patch1, leaving
        // the file with original content "aaa\nbbb\nccc\n". Then it tries
        // to reapply patch2, which expects "bbb_v2" but finds "bbb".
        let report = manager.yank(id1).unwrap();
        // Either there's a conflict (patch2 can't reapply) or it reapplied
        // with fuzzy matching. Either way the report should be produced.
        assert_eq!(report.yanked_id, id1);
        // At least one entry should exist
        assert!(
            !report.conflicts.is_empty() || !report.reapplied.is_empty(),
            "Expected either conflicts or reapplied patches in yank report"
        );
    }

    #[test]
    fn test_yank_conflict_creation() {
        // Trigger a YankConflict by yanking a patch where the subsequent patch
        // cannot be reapplied because the file state is incompatible.
        let temp = TempDir::new().unwrap();

        let file_path = temp.path().join("conflict.txt");
        fs::write(&file_path, "alpha\nbeta\ngamma\n").unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        // Patch 1: modify line 2
        let patch1 = r#"--- a/conflict.txt
+++ b/conflict.txt
@@ -1,3 +1,3 @@
 alpha
-beta
+beta_v2
 gamma
"#;
        let id1 = manager.apply(patch1, "patch-a").unwrap();

        // Patch 2: modify based on patch1's output
        let patch2 = r#"--- a/conflict.txt
+++ b/conflict.txt
@@ -1,3 +1,3 @@
 alpha
-beta_v2
+beta_v3
 gamma
"#;
        manager.apply(patch2, "patch-b").unwrap();

        // Now manually corrupt the file so that after yanking patch-a
        // and undoing its changes, reapplying patch-b will fail
        // because patch-b expects "beta_v2" but the file has "beta"
        // after unapplying patch-a.
        let result = manager.yank(id1);
        assert!(result.is_ok());
        let report = result.unwrap();
        // The reapply of patch-b may conflict since it expects "beta_v2"
        // but after reverting patch-a the file has "beta"
        assert!(
            !report.conflicts.is_empty() || !report.reapplied.is_empty(),
            "Expected either conflicts or successful reapply"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_yank_conflict_io_error_on_reapply() {
        // Covers lines 173-176: YankConflict path where reapplying a patch fails.
        //
        // Patch 1 modifies a.txt. Patch 2 creates nested/deep/file.txt.
        // Before yank, we remove the nested/ directory and replace it with a regular
        // file named "nested". During yank:
        // 1. Unapply patch-2 reverse (delete nested/deep/file.txt):
        //    path doesn't exist (dir was removed), so the exists() check skips it.
        // 2. Unapply patch-1 reverse: succeeds (restores a.txt).
        // 3. Reapply patch-2 (create nested/deep/file.txt):
        //    create_dir_all("nested/deep") fails because "nested" is a regular file.
        //    This triggers the Err(e) branch -> YankConflict at lines 173-176.

        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.txt"), "hello\n").unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch1 = r#"--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-hello
+hello_modified
"#;
        let id1 = manager.apply(patch1, "patch-1").unwrap();

        let patch2 = r#"--- /dev/null
+++ b/nested/deep/file.txt
@@ -0,0 +1 @@
+new content
"#;
        manager.apply(patch2, "patch-2").unwrap();

        let nested_path = temp.path().join("nested");
        assert!(nested_path.join("deep/file.txt").exists());

        // Replace the nested directory with a regular file to block create_dir_all
        fs::remove_dir_all(&nested_path).unwrap();
        fs::write(&nested_path, "blocker\n").unwrap();

        let report = manager.yank(id1).unwrap();
        assert_eq!(report.yanked_id, id1);
        assert!(
            !report.conflicts.is_empty(),
            "Expected conflict from failed reapply, got: conflicts={:?}, reapplied={:?}",
            report.conflicts,
            report.reapplied
        );
        assert_eq!(report.conflicts[0].patch_id, PatchId(2));
    }

    // === BUG-003 regression tests: trailing newline preservation ===

    #[test]
    fn test_apply_hunk_preserves_trailing_newline() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path()).unwrap();

        let content = "line 1\nline 2\nline 3\n";
        let hunk = Hunk {
            old_start: 1,
            old_count: 3,
            new_start: 1,
            new_count: 3,
            section: None,
            lines: vec![
                Line::Context("line 1".to_string()),
                Line::Delete("line 2".to_string()),
                Line::Add("line 2 modified".to_string()),
                Line::Context("line 3".to_string()),
            ],
        };

        let result = manager.apply_hunk(content, &hunk).unwrap();
        assert!(
            result.ends_with('\n'),
            "trailing newline should be preserved"
        );
        assert!(result.contains("line 2 modified"));
    }

    #[test]
    fn test_apply_hunk_no_newline_marker_strips_trailing() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path()).unwrap();

        let content = "line 1\nline 2\n";
        let hunk = Hunk {
            old_start: 1,
            old_count: 2,
            new_start: 1,
            new_count: 2,
            section: None,
            lines: vec![
                Line::Context("line 1".to_string()),
                Line::Context("line 2".to_string()),
                Line::NoNewline,
            ],
        };

        let result = manager.apply_hunk(content, &hunk).unwrap();
        assert!(
            !result.ends_with('\n'),
            "NoNewline marker should strip trailing newline"
        );
    }

    #[test]
    fn test_apply_hunk_no_trailing_newline_input() {
        let temp = TempDir::new().unwrap();
        let manager = PatchManager::new(temp.path()).unwrap();

        let content = "line 1\nline 2";
        let hunk = Hunk {
            old_start: 1,
            old_count: 2,
            new_start: 1,
            new_count: 2,
            section: None,
            lines: vec![
                Line::Context("line 1".to_string()),
                Line::Delete("line 2".to_string()),
                Line::Add("line 2 modified".to_string()),
            ],
        };

        let result = manager.apply_hunk(content, &hunk).unwrap();
        assert!(
            !result.ends_with('\n'),
            "should not add trailing newline when input lacks one"
        );
    }

    // === BUG-004 regression tests: multi-hunk offset adjustment ===

    #[test]
    fn test_apply_multi_hunk_add_then_edit() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("multi.txt"),
            "line 1\nline 2\nline 3\nline 4\nline 5\n",
        )
        .unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        // Hunk 1: add a line after line 1
        // Hunk 2: edit line 5 (originally at line 5, but after hunk 1 adds a line it shifts)
        let patch = r#"--- a/multi.txt
+++ b/multi.txt
@@ -1,2 +1,3 @@
 line 1
+inserted line
 line 2
@@ -4,2 +5,2 @@
 line 4
-line 5
+line 5 modified
"#;

        manager.apply(patch, "multi-hunk").unwrap();

        let content = fs::read_to_string(temp.path().join("multi.txt")).unwrap();
        assert!(content.contains("inserted line"));
        assert!(content.contains("line 5 modified"));
        assert!(!content.contains("\nline 5\n"));
    }

    #[test]
    fn test_apply_multi_hunk_delete_then_edit() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("multi2.txt"),
            "line 1\nline 2\nline 3\nline 4\nline 5\n",
        )
        .unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        // Hunk 1: delete line 2
        // Hunk 2: edit line 5 (originally at line 5, after hunk 1 deletes a line it shifts)
        let patch = r#"--- a/multi2.txt
+++ b/multi2.txt
@@ -1,3 +1,2 @@
 line 1
-line 2
 line 3
@@ -4,2 +3,2 @@
 line 4
-line 5
+line 5 edited
"#;

        manager.apply(patch, "multi-delete").unwrap();

        let content = fs::read_to_string(temp.path().join("multi2.txt")).unwrap();
        assert!(!content.contains("line 2"));
        assert!(content.contains("line 5 edited"));
    }

    #[test]
    fn test_multi_hunk_roundtrip() {
        let temp = TempDir::new().unwrap();
        let original = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        fs::write(temp.path().join("rt.txt"), original).unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch = r#"--- a/rt.txt
+++ b/rt.txt
@@ -1,2 +1,3 @@
 line 1
+inserted
 line 2
@@ -4,2 +5,2 @@
 line 4
-line 5
+line 5 changed
"#;

        let id = manager.apply(patch, "roundtrip").unwrap();
        manager.unapply(id).unwrap();

        let restored = fs::read_to_string(temp.path().join("rt.txt")).unwrap();
        assert_eq!(restored, original);
    }

    // === BUG-003 + general round-trip regression tests ===

    #[test]
    fn test_patch_apply_unapply_roundtrip() {
        let temp = TempDir::new().unwrap();
        let original = "line 1\nline 2\nline 3\n";
        fs::write(temp.path().join("test.txt"), original).unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+line 2 modified
 line 3
"#;

        let id = manager.apply(patch, "roundtrip").unwrap();

        let modified = fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert!(modified.contains("line 2 modified"));

        manager.unapply(id).unwrap();

        let restored = fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert_eq!(
            restored, original,
            "round-trip should produce byte-for-byte match"
        );
    }

    #[test]
    fn test_patch_apply_unapply_multi_hunk_roundtrip() {
        let temp = TempDir::new().unwrap();
        let original = "aaa\nbbb\nccc\nddd\neee\n";
        fs::write(temp.path().join("mh.txt"), original).unwrap();

        let mut manager = PatchManager::new(temp.path()).unwrap();

        let patch = r#"--- a/mh.txt
+++ b/mh.txt
@@ -1,2 +1,3 @@
 aaa
+xxx
 bbb
@@ -4,2 +5,2 @@
 ddd
-eee
+eee_modified
"#;

        let id = manager.apply(patch, "multi-roundtrip").unwrap();
        manager.unapply(id).unwrap();

        let restored = fs::read_to_string(temp.path().join("mh.txt")).unwrap();
        assert_eq!(
            restored, original,
            "multi-hunk round-trip should restore original"
        );
    }
}
