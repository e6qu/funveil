use crate::cas::ContentStore;
use crate::config::{Config, HISTORY_DIR};
use crate::error::{FunveilError, Result};
use crate::perms;
use crate::types::ContentHash;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

const MAX_ENTRIES: usize = 500;
const HISTORY_FILE: &str = "log.yaml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: String,
    pub cas_hash: Option<String>,
    pub permissions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionState {
    pub config_yaml: Option<String>,
    pub file_snapshots: Vec<FileSnapshot>,
}

impl ActionState {
    pub fn capture(root: &Path, config: &Config, files: &[String]) -> Self {
        ActionState {
            config_yaml: snapshot_config(config),
            file_snapshots: snapshot_files(root, files),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub id: u64,
    pub timestamp: DateTime<Utc>,
    pub command: String,
    pub args: Vec<String>,
    pub summary: String,
    pub affected_files: Vec<String>,
    pub undoable: bool,
    pub pre_state: ActionState,
    pub post_state: ActionState,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionHistory {
    pub cursor: usize,
    pub entries: Vec<ActionRecord>,
}

impl ActionHistory {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            entries: Vec::new(),
        }
    }

    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(HISTORY_DIR).join(HISTORY_FILE);
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(Self::new());
        }
        let history: ActionHistory = serde_yaml::from_str(&content)?;
        Ok(history)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let dir = root.join(HISTORY_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(HISTORY_FILE);
        let mut history = Self {
            cursor: self.cursor,
            entries: self.entries.clone(),
        };
        // Truncate to MAX_ENTRIES oldest entries
        if history.entries.len() > MAX_ENTRIES {
            let excess = history.entries.len() - MAX_ENTRIES;
            history.entries.drain(0..excess);
            // Adjust cursor
            history.cursor = history.cursor.saturating_sub(excess);
            // Renumber IDs
            for (i, entry) in history.entries.iter_mut().enumerate() {
                entry.id = (i + 1) as u64;
            }
        }
        let yaml = serde_yaml::to_string(&history)?;
        std::fs::write(&path, yaml)?;
        Ok(())
    }

    pub fn push(&mut self, record: ActionRecord) {
        // Discard all entries after cursor (discard future on new action)
        if !self.entries.is_empty() {
            self.entries.truncate(self.cursor + 1);
        }
        self.entries.push(record);
        self.cursor = self.entries.len() - 1;
    }

    pub fn undo(&mut self) -> std::result::Result<&ActionRecord, FunveilError> {
        if self.entries.is_empty() || self.cursor == 0 {
            return Err(FunveilError::HistoryEmpty);
        }
        let idx = self.cursor;
        self.cursor -= 1;
        Ok(&self.entries[idx])
    }

    pub fn can_undo(&self) -> bool {
        !self.entries.is_empty() && self.cursor > 0
    }

    pub fn redo(&mut self) -> std::result::Result<&ActionRecord, FunveilError> {
        if self.entries.is_empty() || self.cursor >= self.entries.len() - 1 {
            return Err(FunveilError::NothingToRedo);
        }
        self.cursor += 1;
        Ok(&self.entries[self.cursor])
    }

    pub fn past(&self) -> &[ActionRecord] {
        if self.entries.is_empty() {
            return &[];
        }
        &self.entries[..=self.cursor]
    }

    pub fn future(&self) -> &[ActionRecord] {
        if self.entries.is_empty() || self.cursor >= self.entries.len() - 1 {
            return &[];
        }
        &self.entries[self.cursor + 1..]
    }

    pub fn get(&self, id: u64) -> Option<&ActionRecord> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn next_id(&self) -> u64 {
        self.entries.last().map(|e| e.id + 1).unwrap_or(1)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ActionHistory {
    fn default() -> Self {
        Self::new()
    }
}

pub fn snapshot_config(config: &Config) -> Option<String> {
    serde_yaml::to_string(config).ok()
}

pub fn snapshot_files(root: &Path, files: &[String]) -> Vec<FileSnapshot> {
    let store = ContentStore::new(root);
    files
        .iter()
        .filter_map(|f| {
            let path = root.join(f);
            if path.exists() {
                let content = std::fs::read(&path).ok()?;
                let hash = store.store(&content).ok()?;
                let perms = perms::file_mode(&std::fs::metadata(&path).ok()?);
                Some(FileSnapshot {
                    path: f.clone(),
                    cas_hash: Some(hash.full().to_string()),
                    permissions: perms::format_mode(perms),
                })
            } else {
                Some(FileSnapshot {
                    path: f.clone(),
                    cas_hash: None,
                    permissions: "644".to_string(),
                })
            }
        })
        .collect()
}

pub struct HistoryTracker {
    command: String,
    args: Vec<String>,
    affected_files: Vec<String>,
    undoable: bool,
    pre_config: Option<String>,
    pre_files: Vec<FileSnapshot>,
}

impl HistoryTracker {
    pub fn begin(
        config: &Config,
        command: &str,
        args: Vec<String>,
        affected_files: &[String],
        root: &Path,
        undoable: bool,
    ) -> Self {
        Self {
            command: command.to_string(),
            args,
            affected_files: affected_files.to_vec(),
            undoable,
            pre_config: snapshot_config(config),
            pre_files: snapshot_files(root, affected_files),
        }
    }

    pub fn commit(self, root: &Path, config: &Config, summary: String) -> Result<()> {
        let post_config = snapshot_config(config);
        let post_files = snapshot_files(root, &self.affected_files);
        let mut history = ActionHistory::load(root)?;
        history.push(ActionRecord {
            id: history.next_id(),
            timestamp: chrono::Utc::now(),
            command: self.command,
            args: self.args,
            summary,
            affected_files: self.affected_files,
            undoable: self.undoable,
            pre_state: ActionState {
                config_yaml: self.pre_config,
                file_snapshots: self.pre_files,
            },
            post_state: ActionState {
                config_yaml: post_config,
                file_snapshots: post_files,
            },
        });
        history.save(root)?;
        Ok(())
    }
}

pub fn restore_action_state(root: &std::path::Path, state: &ActionState) -> Result<()> {
    if let Some(ref config_yaml) = state.config_yaml {
        let config: Config = serde_yaml::from_str(config_yaml)?;
        config.save(root)?;
    }

    let store = ContentStore::new(root);

    // Phase 1: write all content to temp files, collecting (temp, target, mode) tuples.
    // If any write fails, clean up all temps and return error.
    let mut staged: Vec<(std::path::PathBuf, std::path::PathBuf, Option<u32>)> = Vec::new();
    let mut to_remove: Vec<std::path::PathBuf> = Vec::new();

    let cleanup_temps = |staged: &[(std::path::PathBuf, std::path::PathBuf, Option<u32>)]| {
        for (tmp, _, _) in staged {
            let _ = std::fs::remove_file(tmp);
        }
    };

    for snap in &state.file_snapshots {
        let file_path = root.join(&snap.path);
        if let Some(ref hash_str) = snap.cas_hash {
            let hash = ContentHash::from_string(hash_str.clone())?;
            let content = match store.retrieve(&hash) {
                Ok(c) => c,
                Err(e) => {
                    cleanup_temps(&staged);
                    return Err(e);
                }
            };
            if let Some(parent) = file_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    cleanup_temps(&staged);
                    return Err(e.into());
                }
            }
            let tmp_path = file_path.with_extension("fv_restore_tmp");
            if let Err(e) = std::fs::write(&tmp_path, content) {
                cleanup_temps(&staged);
                return Err(e.into());
            }
            let mode = perms::parse_mode(&snap.permissions);
            staged.push((tmp_path, file_path, Some(mode)));
        } else {
            to_remove.push(file_path);
        }
    }

    // Phase 2: rename all temps to targets (atomic on same filesystem)
    for (tmp, target, mode) in &staged {
        if target.exists() {
            let _ = perms::save_and_make_writable(target);
        }
        if let Err(e) = std::fs::rename(tmp, target) {
            cleanup_temps(&staged);
            return Err(e.into());
        }
        if let Some(m) = mode {
            let _ = perms::set_mode(target, *m);
        }
    }

    // Phase 3: remove files that should not exist
    for file_path in &to_remove {
        if file_path.exists() {
            let _ = perms::save_and_make_writable(file_path);
            std::fs::remove_file(file_path)?;
        }
    }

    Ok(())
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_record(id: u64, command: &str, undoable: bool) -> ActionRecord {
        ActionRecord {
            id,
            timestamp: Utc::now(),
            command: command.to_string(),
            args: vec![],
            summary: format!("Test {command}"),
            affected_files: vec![],
            undoable,
            pre_state: ActionState {
                config_yaml: Some("pre".to_string()),
                file_snapshots: vec![],
            },
            post_state: ActionState {
                config_yaml: Some("post".to_string()),
                file_snapshots: vec![],
            },
        }
    }

    #[test]
    fn test_new_history_is_empty() {
        let h = ActionHistory::new();
        assert!(h.is_empty());
        assert_eq!(h.cursor, 0);
        assert_eq!(h.next_id(), 1);
    }

    #[test]
    fn test_push_advances_cursor() {
        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        assert_eq!(h.cursor, 0);
        assert_eq!(h.entries.len(), 1);

        h.push(make_record(2, "veil", true));
        assert_eq!(h.cursor, 1);
        assert_eq!(h.entries.len(), 2);
    }

    #[test]
    fn test_undo_moves_cursor_back() {
        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        h.push(make_record(2, "veil", true));
        h.push(make_record(3, "unveil", true));

        assert_eq!(h.cursor, 2);
        let entry = h.undo().unwrap();
        assert_eq!(entry.id, 3);
        assert_eq!(h.cursor, 1);

        let entry = h.undo().unwrap();
        assert_eq!(entry.id, 2);
        assert_eq!(h.cursor, 0);
    }

    #[test]
    fn test_undo_empty_history() {
        let mut h = ActionHistory::new();
        assert!(matches!(h.undo(), Err(FunveilError::HistoryEmpty)));
    }

    #[test]
    fn test_undo_at_beginning() {
        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        // cursor is 0, can't go further back
        assert!(matches!(h.undo(), Err(FunveilError::HistoryEmpty)));
    }

    #[test]
    fn test_redo_moves_cursor_forward() {
        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        h.push(make_record(2, "veil", true));
        h.push(make_record(3, "unveil", true));

        h.undo().unwrap(); // cursor: 2 -> 1
        h.undo().unwrap(); // cursor: 1 -> 0

        let entry = h.redo().unwrap();
        assert_eq!(entry.id, 2);
        assert_eq!(h.cursor, 1);

        let entry = h.redo().unwrap();
        assert_eq!(entry.id, 3);
        assert_eq!(h.cursor, 2);
    }

    #[test]
    fn test_redo_nothing_to_redo() {
        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        assert!(matches!(h.redo(), Err(FunveilError::NothingToRedo)));
    }

    #[test]
    fn test_push_after_undo_discards_future() {
        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        h.push(make_record(2, "veil", true));
        h.push(make_record(3, "unveil", true));

        h.undo().unwrap(); // cursor: 2 -> 1
                           // Now push a new action — should discard entry #3
        h.push(make_record(4, "mode", true));
        assert_eq!(h.entries.len(), 3); // [init, veil, mode]
        assert_eq!(h.cursor, 2);
        assert_eq!(h.entries[2].command, "mode");
    }

    #[test]
    fn test_past_and_future() {
        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        h.push(make_record(2, "veil", true));
        h.push(make_record(3, "unveil", true));

        h.undo().unwrap(); // cursor at 1
        assert_eq!(h.past().len(), 2); // [init, veil]
        assert_eq!(h.future().len(), 1); // [unveil]
    }

    #[test]
    fn test_get_by_id() {
        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        h.push(make_record(2, "veil", true));

        assert_eq!(h.get(1).unwrap().command, "init");
        assert_eq!(h.get(2).unwrap().command, "veil");
        assert!(h.get(99).is_none());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(HISTORY_DIR)).unwrap();

        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        h.push(make_record(2, "veil", true));
        h.save(root).unwrap();

        let loaded = ActionHistory::load(root).unwrap();
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.cursor, 1);
        assert_eq!(loaded.entries[0].command, "init");
        assert_eq!(loaded.entries[1].command, "veil");
    }

    #[test]
    fn test_load_missing_file_returns_empty() {
        let temp = TempDir::new().unwrap();
        let h = ActionHistory::load(temp.path()).unwrap();
        assert!(h.is_empty());
    }

    #[test]
    fn test_save_truncates_to_500() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(HISTORY_DIR)).unwrap();

        let mut h = ActionHistory::new();
        for i in 1..=510 {
            h.push(make_record(i, "veil", true));
        }
        assert_eq!(h.entries.len(), 510);
        h.cursor = 509; // pointing at last entry

        h.save(root).unwrap();
        let loaded = ActionHistory::load(root).unwrap();
        assert_eq!(loaded.entries.len(), 500);
        // IDs renumbered to 1..500
        assert_eq!(loaded.entries[0].id, 1);
        assert_eq!(loaded.entries[499].id, 500);
        // Cursor adjusted
        assert_eq!(loaded.cursor, 499);
    }

    #[test]
    fn test_next_id() {
        let mut h = ActionHistory::new();
        assert_eq!(h.next_id(), 1);
        h.push(make_record(1, "init", false));
        assert_eq!(h.next_id(), 2);
        h.push(make_record(2, "veil", true));
        assert_eq!(h.next_id(), 3);
    }

    #[test]
    fn test_can_undo() {
        let mut h = ActionHistory::new();
        assert!(!h.can_undo());
        h.push(make_record(1, "init", false));
        assert!(!h.can_undo()); // only one entry, cursor at 0
        h.push(make_record(2, "veil", true));
        assert!(h.can_undo()); // cursor at 1, can go to 0
    }

    #[test]
    fn test_file_snapshot_serialization() {
        let snap = FileSnapshot {
            path: "src/main.rs".to_string(),
            cas_hash: Some("abc123".to_string()),
            permissions: "644".to_string(),
        };
        let yaml = serde_yaml::to_string(&snap).unwrap();
        let deserialized: FileSnapshot = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized.path, "src/main.rs");
        assert_eq!(deserialized.cas_hash.unwrap(), "abc123");
    }

    #[test]
    fn test_action_state_with_no_config() {
        let state = ActionState {
            config_yaml: None,
            file_snapshots: vec![],
        };
        let yaml = serde_yaml::to_string(&state).unwrap();
        let deserialized: ActionState = serde_yaml::from_str(&yaml).unwrap();
        assert!(deserialized.config_yaml.is_none());
        assert!(deserialized.file_snapshots.is_empty());
    }

    #[test]
    fn test_empty_past_and_future() {
        let h = ActionHistory::new();
        assert!(h.past().is_empty());
        assert!(h.future().is_empty());
    }

    #[test]
    fn test_future_empty_when_at_end() {
        let mut h = ActionHistory::new();
        h.push(make_record(1, "init", false));
        assert!(h.future().is_empty());
    }

    #[test]
    fn test_snapshot_config_roundtrips() {
        use crate::config::Config;

        let config = Config::default();
        let yaml = snapshot_config(&config).expect("serialization should succeed");
        let restored: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(restored.version, config.version);
    }

    #[test]
    fn test_snapshot_files_existing_and_missing() {
        use crate::config::OBJECTS_DIR;

        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(OBJECTS_DIR)).unwrap();

        std::fs::write(root.join("hello.txt"), b"hello world").unwrap();

        let files = vec!["hello.txt".to_string(), "nonexistent.txt".to_string()];
        let snaps = snapshot_files(root, &files);

        assert_eq!(snaps.len(), 2);

        assert_eq!(snaps[0].path, "hello.txt");
        assert!(snaps[0].cas_hash.is_some());

        assert_eq!(snaps[1].path, "nonexistent.txt");
        assert!(snaps[1].cas_hash.is_none());
        assert_eq!(snaps[1].permissions, "644");
    }

    #[test]
    fn test_history_tracker_begin_commit() {
        use crate::config::{Config, OBJECTS_DIR};

        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(OBJECTS_DIR)).unwrap();
        std::fs::create_dir_all(root.join(HISTORY_DIR)).unwrap();

        std::fs::write(root.join("file.txt"), b"before").unwrap();

        let config = Config::default();
        let tracker = HistoryTracker::begin(
            &config,
            "veil",
            vec!["--all".to_string()],
            &["file.txt".to_string()],
            root,
            true,
        );

        std::fs::write(root.join("file.txt"), b"after").unwrap();

        tracker
            .commit(root, &config, "veiled file.txt".to_string())
            .unwrap();

        let history = ActionHistory::load(root).unwrap();
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].command, "veil");
        assert_eq!(history.entries[0].args, vec!["--all"]);
        assert!(history.entries[0].undoable);
        assert_eq!(history.entries[0].summary, "veiled file.txt");
        assert_eq!(history.entries[0].affected_files, vec!["file.txt"]);
        assert!(history.entries[0].pre_state.file_snapshots[0]
            .cas_hash
            .is_some());
        assert!(history.entries[0].post_state.file_snapshots[0]
            .cas_hash
            .is_some());
        assert_ne!(
            history.entries[0].pre_state.file_snapshots[0].cas_hash,
            history.entries[0].post_state.file_snapshots[0].cas_hash,
        );
    }

    #[test]
    fn test_restore_action_state_with_files() {
        use crate::config::{Config, OBJECTS_DIR};

        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(OBJECTS_DIR)).unwrap();

        std::fs::write(root.join("a.txt"), b"original-a").unwrap();

        let config = Config::default();
        config.save(root).unwrap();

        let store = ContentStore::new(root);
        let hash = store.store(b"restored-a").unwrap();

        let state = ActionState {
            config_yaml: snapshot_config(&config),
            file_snapshots: vec![
                FileSnapshot {
                    path: "a.txt".to_string(),
                    cas_hash: Some(hash.full().to_string()),
                    permissions: "644".to_string(),
                },
                FileSnapshot {
                    path: "gone.txt".to_string(),
                    cas_hash: None,
                    permissions: "644".to_string(),
                },
            ],
        };

        restore_action_state(root, &state).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "restored-a"
        );
        assert!(!root.join("gone.txt").exists());
    }

    #[test]
    fn test_restore_action_state_removes_existing_file() {
        use crate::config::{Config, OBJECTS_DIR};

        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(OBJECTS_DIR)).unwrap();
        let config = Config::default();
        config.save(root).unwrap();

        std::fs::write(root.join("delete_me.txt"), b"bye").unwrap();
        assert!(root.join("delete_me.txt").exists());

        let state = ActionState {
            config_yaml: snapshot_config(&config),
            file_snapshots: vec![FileSnapshot {
                path: "delete_me.txt".to_string(),
                cas_hash: None,
                permissions: "644".to_string(),
            }],
        };

        restore_action_state(root, &state).unwrap();
        assert!(!root.join("delete_me.txt").exists());
    }

    #[test]
    fn test_restore_action_state_bad_hash() {
        use crate::config::{Config, OBJECTS_DIR};

        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(OBJECTS_DIR)).unwrap();
        let config = Config::default();
        config.save(root).unwrap();

        let state = ActionState {
            config_yaml: snapshot_config(&config),
            file_snapshots: vec![FileSnapshot {
                path: "a.txt".to_string(),
                cas_hash: Some("not_a_valid_hash_at_all_nope".to_string()),
                permissions: "644".to_string(),
            }],
        };

        let result = restore_action_state(root, &state);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_empty_history_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(HISTORY_DIR)).unwrap();
        std::fs::write(root.join(HISTORY_DIR).join(HISTORY_FILE), "   \n  ").unwrap();
        let h = ActionHistory::load(root).unwrap();
        assert!(h.is_empty());
    }

    #[test]
    fn test_restore_cleanup_on_cas_retrieve_failure_after_staging() {
        use crate::config::{Config, OBJECTS_DIR};

        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(OBJECTS_DIR)).unwrap();
        let config = Config::default();
        config.save(root).unwrap();

        let store = ContentStore::new(root);
        let hash_good = store.store(b"good content").unwrap();

        let state = ActionState {
            config_yaml: snapshot_config(&config),
            file_snapshots: vec![
                FileSnapshot {
                    path: "good.txt".to_string(),
                    cas_hash: Some(hash_good.full().to_string()),
                    permissions: "644".to_string(),
                },
                FileSnapshot {
                    path: "bad.txt".to_string(),
                    cas_hash: Some(
                        "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
                            .to_string(),
                    ),
                    permissions: "644".to_string(),
                },
            ],
        };

        let result = restore_action_state(root, &state);
        assert!(result.is_err());
        // The temp file for good.txt should have been cleaned up
        assert!(!root.join("good.fv_restore_tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_restore_cleanup_on_write_failure() {
        use crate::config::{Config, OBJECTS_DIR};
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(OBJECTS_DIR)).unwrap();
        let config = Config::default();
        config.save(root).unwrap();

        let store = ContentStore::new(root);
        let hash1 = store.store(b"content one").unwrap();
        let hash2 = store.store(b"content two").unwrap();

        // Create a read-only subdir so writing the second temp file fails
        let subdir = root.join("readonly_dir");
        std::fs::create_dir_all(&subdir).unwrap();
        let perms = std::fs::Permissions::from_mode(0o444);
        std::fs::set_permissions(&subdir, perms).unwrap();

        let state = ActionState {
            config_yaml: snapshot_config(&config),
            file_snapshots: vec![
                FileSnapshot {
                    path: "writable.txt".to_string(),
                    cas_hash: Some(hash1.full().to_string()),
                    permissions: "644".to_string(),
                },
                FileSnapshot {
                    path: "readonly_dir/fail.txt".to_string(),
                    cas_hash: Some(hash2.full().to_string()),
                    permissions: "644".to_string(),
                },
            ],
        };

        let result = restore_action_state(root, &state);
        assert!(result.is_err());

        // Cleanup permissions
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&subdir, perms).unwrap();
    }

    #[test]
    fn test_restore_action_state_missing_cas_object() {
        use crate::config::{Config, OBJECTS_DIR};

        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join(OBJECTS_DIR)).unwrap();
        let config = Config::default();
        config.save(root).unwrap();

        let state = ActionState {
            config_yaml: snapshot_config(&config),
            file_snapshots: vec![FileSnapshot {
                path: "a.txt".to_string(),
                cas_hash: Some(
                    "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
                ),
                permissions: "644".to_string(),
            }],
        };

        let result = restore_action_state(root, &state);
        assert!(result.is_err());
    }
}
