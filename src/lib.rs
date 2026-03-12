pub mod analysis;
pub mod cas;
pub mod checkpoint;
pub mod config;
pub mod error;
pub mod logging;
pub mod output;
pub mod parser;
pub mod patch;
pub mod strategies;
pub mod types;
pub mod veil;

pub use analysis::{
    AnalysisCache, CachedParser, CallGraph, CallGraphBuilder, Entrypoint, EntrypointDetector,
    EntrypointType, TraceDirection, TraceResult,
};
pub use cas::{garbage_collect, ContentStore};
pub use checkpoint::{
    delete_checkpoint, get_latest_checkpoint, list_checkpoints, restore_checkpoint,
    save_checkpoint, show_checkpoint,
};
pub use config::{Config, ObjectMeta, CONFIG_FILE, DATA_DIR};
pub use error::{FunveilError, Result};
pub use logging::{command_category, generate_trace_id, init_tracing, resolve_log_level};
pub use output::Output;
pub use parser::{Language, ParsedFile, Symbol, TreeSitterParser};
pub use strategies::{HeaderConfig, HeaderStrategy, VeilStrategy};
pub use types::{validate_path_within_root, ConfigEntry, ContentHash, LineRange, Mode, Pattern};
pub use veil::{has_veils, unveil_all, unveil_file, veil_file};
