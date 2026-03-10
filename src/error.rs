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
}

pub type Result<T> = std::result::Result<T, FunveilError>;
