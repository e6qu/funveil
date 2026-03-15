#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

pub mod analysis;
pub mod budget;
pub mod cas;
pub mod checkpoint;
pub mod config;
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
pub use config::{
    is_supported_source, normalize_path, walk_files, Config, ObjectMeta, CONFIG_FILE, DATA_DIR,
    HISTORY_DIR, METADATA_DIR, SUPPORTED_EXTENSIONS,
};
pub use error::{FunveilError, Result};
pub use history::{ActionHistory, ActionRecord, ActionState, FileSnapshot};
pub use logging::{command_category, generate_trace_id, init_tracing, resolve_log_level};
pub use metadata::{
    build_call_graph_from_metadata, generate_manifest, load_index, load_manifest, rebuild_index,
    save_index, save_manifest, FileMetadata, Manifest, MetadataIndex, MetadataStore, SymbolMeta,
};
pub use output::Output;
pub use parser::{Language, ParsedFile, Symbol, TreeSitterParser};
pub use strategies::{HeaderConfig, HeaderStrategy, VeilStrategy};
pub use types::{
    validate_path_within_root, ConfigEntry, ConfigKey, ContentHash, LineRange, Mode, Pattern,
    ORIGINAL_SUFFIX,
};
pub use veil::{
    align_to_symbol_boundary, has_veils, is_legacy_marker, unveil_all, unveil_file, veil_file,
};

#[cfg(not(target_family = "wasm"))]
pub mod update;
