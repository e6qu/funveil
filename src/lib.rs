#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

pub mod analysis;
pub mod budget;
pub mod cas;
pub mod checkpoint;
pub mod commands;
pub mod config;
pub mod doctor;
pub mod error;
pub mod history;
pub mod logging;
pub mod metadata;
pub mod output;
pub mod parser;
pub mod patch;
pub mod perms;
pub mod strategies;
pub mod types;
pub mod veil;

pub use analysis::{
    AnalysisCache, CachedParser, CallGraph, CallGraphBuilder, Entrypoint, EntrypointDetector,
    EntrypointType, TraceDirection, TraceResult,
};
pub use budget::{compute_disclosure_plan, estimate_tokens, DisclosureEntry, DisclosurePlan};
pub use cas::{garbage_collect, ContentStore};
pub use checkpoint::{
    delete_checkpoint, get_latest_checkpoint, list_checkpoints, restore_checkpoint,
    save_checkpoint, show_checkpoint,
};
pub use commands::{
    collect_affected_files_for_pattern, handle_level_veil, run_command, update_metadata,
    version_long, CacheCmd, CheckpointCmd, Cli, Commands, EntrypointTypeArg, LanguageArg,
    ParseFormat, TraceFormat, VeilMode,
};
pub use config::{
    is_supported_source, normalize_path, walk_files, Config, ObjectMeta, CONFIG_FILE, DATA_DIR,
    HISTORY_DIR, METADATA_DIR, SUPPORTED_EXTENSIONS,
};
pub use doctor::{check_integrity, DoctorReport};
pub use error::{FunveilError, Result};
pub use history::{
    restore_action_state, snapshot_config, snapshot_files, ActionHistory, ActionRecord,
    ActionState, FileSnapshot, HistoryTracker,
};
pub use logging::{command_category, generate_trace_id, init_tracing, resolve_log_level};
pub use metadata::{
    build_call_graph_from_metadata, build_call_graph_from_parsed, generate_manifest, load_index,
    load_manifest, metadata_to_parsed_file, parse_all_sources, rebuild_index,
    rebuild_index_from_parsed, save_index, save_manifest, CallMeta, FileMetadata, Manifest,
    MetadataIndex, MetadataStore, SymbolMeta,
};
pub use output::{ActionSummary, CommandResult, FileDiff, FileStatus, Output};
pub use parser::{Language, ParsedFile, Symbol, TreeSitterParser};
pub use strategies::{apply_level, HeaderConfig, HeaderStrategy, LevelResult, VeilStrategy};
pub use types::{
    parse_pattern, validate_path_within_root, ConfigEntry, ConfigKey, ContentHash, LineRange, Mode,
    Pattern, ORIGINAL_SUFFIX,
};
pub use veil::{
    align_to_symbol_boundary, has_veils, is_legacy_marker, unveil_all, unveil_file, veil_file,
};

#[cfg(not(target_family = "wasm"))]
pub mod update;
