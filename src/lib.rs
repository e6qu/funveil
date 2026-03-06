pub mod cas;
pub mod checkpoint;
pub mod config;
pub mod error;
pub mod types;
pub mod veil;

pub use error::{FunveilError, Result};
pub use types::{Mode, Pattern, LineRange, ContentHash, ConfigEntry};
pub use config::{Config, CONFIG_FILE, DATA_DIR};
pub use cas::ContentStore;
pub use veil::{veil_file, unveil_file, unveil_all, is_veiled};
pub use checkpoint::{save_checkpoint, list_checkpoints, show_checkpoint};
