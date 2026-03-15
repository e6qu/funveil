use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FunveilError {
    #[error("relative paths not allowed: {0}")]
    RelativePath(String),

    #[error("hidden files must use full path: {0}")]
    HiddenFileWithoutPath(String),

    #[error("path '{path}' resolves outside project root: {resolved:?}")]
    SymlinkOutsideProject { path: String, resolved: PathBuf },

    #[error("binary files can only be veiled in full, not partially: {0}")]
    BinaryFilePartialVeil(String),

    #[error("binary files cannot be veiled as text: {0}")]
    BinaryFileVeil(String),

    #[error("directory contains binary files and cannot be veiled: {0}")]
    DirectoryContainsBinary(String),

    #[error("invalid checkpoint name: {0}")]
    InvalidCheckpointName(String),

    #[error("directories cannot have line ranges: {0}")]
    DirectoryWithLineRanges(String),

    #[error("invalid line range '{range}': {reason}")]
    InvalidLineRange { range: String, reason: String },

    #[error("ranges must not overlap in same file")]
    OverlappingRanges,

    #[error("cannot veil empty file: {0}")]
    EmptyFile(String),

    #[error("file already veiled: {0}")]
    AlreadyVeiled(String),

    #[error(
        "overlapping veil ranges: new range {new_range} overlaps existing range {existing_range}"
    )]
    OverlappingVeil {
        new_range: String,
        existing_range: String,
    },

    #[error("file contains text matching veil marker patterns: {0}")]
    MarkerCollision(String),

    #[error("on-disk veil markers are inconsistent with config: {0}")]
    MarkerIntegrityError(String),

    #[error("file not veiled: {0}")]
    NotVeiled(String),

    #[error("object not found in CAS: {0}")]
    ObjectNotFound(String),

    #[error("config file is protected and cannot be veiled")]
    ConfigFileProtected,

    #[error("funveil data directory is protected")]
    DataDirectoryProtected,

    #[error("VCS directories are excluded by default: {0}")]
    VcsDirectoryExcluded(String),

    #[error("invalid regex pattern: {0}")]
    InvalidRegex(String),

    #[error("invalid content hash: {0}")]
    InvalidHash(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("checkpoint not found: {0}")]
    CheckpointNotFound(String),

    #[error("veil marker corrupted: {0}")]
    CorruptedMarker(String),

    #[error("parse error at line {line}, column {column}: {message}")]
    ParseError {
        line: usize,
        column: usize,
        message: String,
        found: String,
        suggestion: Option<String>,
    },

    #[error("tree-sitter error: {0}")]
    TreeSitterError(String),

    #[error("cache error: {0}")]
    CacheError(String),

    #[error("patch mismatch: {0}")]
    PatchMismatch(String),

    #[error("partial restore: {restored} restored, {failed} failed")]
    PartialRestore { restored: usize, failed: usize },

    #[error("history is empty — nothing to undo")]
    HistoryEmpty,

    #[error("nothing to redo")]
    NothingToRedo,

    #[error("action #{0} is not undoable")]
    ActionNotUndoable(u64),
}

impl FunveilError {
    pub fn code(&self) -> &'static str {
        match self {
            FunveilError::RelativePath(_) => "E001",
            FunveilError::HiddenFileWithoutPath(_) => "E002",
            FunveilError::SymlinkOutsideProject { .. } => "E003",
            FunveilError::BinaryFilePartialVeil(_) => "E004",
            FunveilError::BinaryFileVeil(_) => "E005",
            FunveilError::DirectoryContainsBinary(_) => "E006",
            FunveilError::InvalidCheckpointName(_) => "E007",
            FunveilError::DirectoryWithLineRanges(_) => "E008",
            FunveilError::InvalidLineRange { .. } => "E009",
            FunveilError::OverlappingRanges => "E010",
            FunveilError::EmptyFile(_) => "E011",
            FunveilError::AlreadyVeiled(_) => "E012",
            FunveilError::OverlappingVeil { .. } => "E013",
            FunveilError::MarkerCollision(_) => "E014",
            FunveilError::MarkerIntegrityError(_) => "E015",
            FunveilError::NotVeiled(_) => "E016",
            FunveilError::ObjectNotFound(_) => "E017",
            FunveilError::ConfigFileProtected => "E018",
            FunveilError::DataDirectoryProtected => "E019",
            FunveilError::VcsDirectoryExcluded(_) => "E020",
            FunveilError::InvalidRegex(_) => "E021",
            FunveilError::InvalidHash(_) => "E022",
            FunveilError::Io(_) => "E023",
            FunveilError::Yaml(_) => "E024",
            FunveilError::CheckpointNotFound(_) => "E025",
            FunveilError::CorruptedMarker(_) => "E026",
            FunveilError::ParseError { .. } => "E027",
            FunveilError::TreeSitterError(_) => "E028",
            FunveilError::CacheError(_) => "E029",
            FunveilError::PatchMismatch(_) => "E030",
            FunveilError::PartialRestore { .. } => "E031",
            FunveilError::HistoryEmpty => "E032",
            FunveilError::NothingToRedo => "E033",
            FunveilError::ActionNotUndoable(_) => "E034",
        }
    }
}

pub type Result<T> = std::result::Result<T, FunveilError>;

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes_all_variants() {
        let cases: Vec<(FunveilError, &str)> = vec![
            (FunveilError::RelativePath("p".into()), "E001"),
            (FunveilError::HiddenFileWithoutPath("p".into()), "E002"),
            (
                FunveilError::SymlinkOutsideProject {
                    path: "p".into(),
                    resolved: PathBuf::from("/x"),
                },
                "E003",
            ),
            (FunveilError::BinaryFilePartialVeil("p".into()), "E004"),
            (FunveilError::BinaryFileVeil("p".into()), "E005"),
            (FunveilError::DirectoryContainsBinary("p".into()), "E006"),
            (FunveilError::InvalidCheckpointName("p".into()), "E007"),
            (FunveilError::DirectoryWithLineRanges("p".into()), "E008"),
            (
                FunveilError::InvalidLineRange {
                    range: "1-0".into(),
                    reason: "r".into(),
                },
                "E009",
            ),
            (FunveilError::OverlappingRanges, "E010"),
            (FunveilError::EmptyFile("p".into()), "E011"),
            (FunveilError::AlreadyVeiled("p".into()), "E012"),
            (
                FunveilError::OverlappingVeil {
                    new_range: "1-2".into(),
                    existing_range: "1-2".into(),
                },
                "E013",
            ),
            (FunveilError::MarkerCollision("p".into()), "E014"),
            (FunveilError::MarkerIntegrityError("p".into()), "E015"),
            (FunveilError::NotVeiled("p".into()), "E016"),
            (FunveilError::ObjectNotFound("p".into()), "E017"),
            (FunveilError::ConfigFileProtected, "E018"),
            (FunveilError::DataDirectoryProtected, "E019"),
            (FunveilError::VcsDirectoryExcluded("p".into()), "E020"),
            (FunveilError::InvalidRegex("p".into()), "E021"),
            (FunveilError::InvalidHash("p".into()), "E022"),
            (
                FunveilError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "x")),
                "E023",
            ),
            (
                FunveilError::Yaml(serde_yaml::from_str::<serde_yaml::Value>("{{").unwrap_err()),
                "E024",
            ),
            (FunveilError::CheckpointNotFound("p".into()), "E025"),
            (FunveilError::CorruptedMarker("p".into()), "E026"),
            (
                FunveilError::ParseError {
                    line: 1,
                    column: 1,
                    message: "m".into(),
                    found: "f".into(),
                    suggestion: None,
                },
                "E027",
            ),
            (FunveilError::TreeSitterError("p".into()), "E028"),
            (FunveilError::CacheError("p".into()), "E029"),
            (FunveilError::PatchMismatch("p".into()), "E030"),
            (
                FunveilError::PartialRestore {
                    restored: 1,
                    failed: 1,
                },
                "E031",
            ),
            (FunveilError::HistoryEmpty, "E032"),
            (FunveilError::NothingToRedo, "E033"),
            (FunveilError::ActionNotUndoable(1), "E034"),
        ];

        for (err, expected_code) in &cases {
            assert_eq!(err.code(), *expected_code, "Wrong code for {:?}", err);
        }
    }

    #[test]
    fn test_error_display_messages() {
        assert_eq!(
            FunveilError::RelativePath("foo".into()).to_string(),
            "relative paths not allowed: foo"
        );
        assert_eq!(
            FunveilError::HistoryEmpty.to_string(),
            "history is empty — nothing to undo"
        );
        assert_eq!(FunveilError::NothingToRedo.to_string(), "nothing to redo");
        assert_eq!(
            FunveilError::ActionNotUndoable(5).to_string(),
            "action #5 is not undoable"
        );
    }
}
