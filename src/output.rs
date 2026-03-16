use crate::budget::DisclosureEntry;
use crate::history::ActionRecord;
use serde::Serialize;
use std::io::{self, Write};

#[derive(Serialize)]
pub struct FileStatus {
    pub path: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub veil_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranges: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_disk: Option<bool>,
}

#[derive(Serialize)]
pub struct ActionSummary {
    pub id: u64,
    pub timestamp: String,
    pub command: String,
    pub affected_files: Vec<String>,
    pub summary: String,
}

impl ActionSummary {
    pub fn from_record(r: &ActionRecord) -> Self {
        Self {
            id: r.id,
            timestamp: r.timestamp.to_rfc3339(),
            command: r.command.clone(),
            affected_files: r.affected_files.clone(),
            summary: r.summary.clone(),
        }
    }
}

#[derive(Serialize)]
pub struct FileDiff {
    pub path: String,
    pub before: String,
    pub after: String,
}

#[derive(Serialize)]
#[serde(tag = "command")]
#[allow(clippy::enum_variant_names)]
pub enum CommandResult {
    #[serde(rename = "init")]
    Init { mode: String },
    #[serde(rename = "mode")]
    ModeResult { mode: String, changed: bool },
    #[serde(rename = "status")]
    Status {
        mode: String,
        veiled_count: usize,
        unveiled_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        files: Option<Vec<FileStatus>>,
    },
    #[serde(rename = "veil")]
    Veil { files: Vec<String>, dry_run: bool },
    #[serde(rename = "unveil")]
    Unveil { files: Vec<String>, dry_run: bool },
    #[serde(rename = "apply")]
    Apply {
        applied: usize,
        skipped: usize,
        dry_run: bool,
    },
    #[serde(rename = "history")]
    History {
        past: Vec<ActionSummary>,
        future: Vec<ActionSummary>,
        cursor_id: Option<u64>,
    },
    #[serde(rename = "history_show")]
    HistoryShow {
        action: ActionSummary,
        config_diff: Vec<String>,
        file_diffs: Vec<FileDiff>,
    },
    #[serde(rename = "undo")]
    Undo { undone: ActionSummary },
    #[serde(rename = "redo")]
    Redo { redone: ActionSummary },
    #[serde(rename = "gc")]
    Gc { deleted: usize, freed_bytes: u64 },
    #[serde(rename = "clean")]
    Clean { success: bool },
    #[serde(rename = "restore")]
    Restore { checkpoint: String },
    #[serde(rename = "checkpoint")]
    Checkpoint { action: String, name: String },
    #[serde(rename = "doctor")]
    Doctor { issues: Vec<String> },
    #[serde(rename = "version")]
    VersionResult { version: String },
    #[serde(rename = "context")]
    Context {
        function: String,
        unveiled_files: Vec<String>,
    },
    #[serde(rename = "disclose")]
    Disclose {
        budget: usize,
        used_tokens: usize,
        entries: Vec<DisclosureEntry>,
    },
    #[serde(rename = "other")]
    Other { message: String },
}

/// Controls program output based on quiet mode.
///
/// When quiet is true, all output goes to `io::sink()`.
/// When quiet is false, stdout/stderr are used normally.
///
/// In tests, pass `Vec<u8>` buffers to capture output.
pub struct Output {
    pub out: Box<dyn Write>,
    pub err: Box<dyn Write>,
}

impl Output {
    pub fn new(quiet: bool) -> Self {
        if quiet {
            Self {
                out: Box::new(io::sink()),
                err: Box::new(io::sink()),
            }
        } else {
            Self {
                out: Box::new(io::stdout()),
                err: Box::new(io::stderr()),
            }
        }
    }

    /// Create an Output that captures to buffers (for testing).
    #[cfg(test)]
    pub fn capture() -> (Self, SharedBuf, SharedBuf) {
        let out_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let err_buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = Self {
            out: Box::new(SharedWriter(out_buf.clone())),
            err: Box::new(SharedWriter(err_buf.clone())),
        };
        (output, out_buf, err_buf)
    }
}

#[cfg(test)]
type SharedBuf = std::sync::Arc<std::sync::Mutex<Vec<u8>>>;

/// A writer that writes to a shared buffer behind a mutex.
#[cfg(test)]
struct SharedWriter(SharedBuf);

#[cfg(test)]
impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_quiet_mode_suppresses_output() {
        let output = Output::new(true);
        let mut out = output.out;
        let mut err = output.err;
        // Writing to sink should succeed silently
        writeln!(out, "hello").unwrap();
        writeln!(err, "world").unwrap();
    }

    #[test]
    fn test_capture_mode() {
        let (mut output, out_buf, err_buf) = Output::capture();
        writeln!(output.out, "stdout line").unwrap();
        writeln!(output.err, "stderr line").unwrap();

        let out = String::from_utf8(out_buf.lock().unwrap().clone()).unwrap();
        let err = String::from_utf8(err_buf.lock().unwrap().clone()).unwrap();
        assert_eq!(out, "stdout line\n");
        assert_eq!(err, "stderr line\n");
    }

    #[test]
    fn test_quiet_vs_non_quiet() {
        // Quiet mode produces no output
        let (mut output, out_buf, _) = Output::capture();
        // Simulate quiet by using sink
        output.out = Box::new(std::io::sink());
        writeln!(output.out, "should vanish").unwrap();
        assert!(out_buf.lock().unwrap().is_empty());
    }
}
