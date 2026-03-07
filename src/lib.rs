pub mod analysis;
pub mod cas;
pub mod checkpoint;
pub mod config;
pub mod error;
pub mod parser;
pub mod strategies;
pub mod types;
pub mod veil;

pub use analysis::{
    AnalysisCache, CachedParser, CallGraph, CallGraphBuilder, Entrypoint, EntrypointDetector,
    EntrypointType, TraceDirection, TraceResult,
};
pub use cas::ContentStore;
pub use checkpoint::{list_checkpoints, save_checkpoint, show_checkpoint};
pub use config::{Config, CONFIG_FILE, DATA_DIR};
pub use error::{FunveilError, Result};
pub use parser::{Language, ParsedFile, Symbol, TreeSitterParser};
pub use strategies::{HeaderConfig, HeaderStrategy, VeilStrategy};
pub use types::{ConfigEntry, ContentHash, LineRange, Mode, Pattern};
pub use veil::{is_veiled, unveil_all, unveil_file, veil_file};
