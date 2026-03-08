//! Patch parsing and management module
//!
//! Provides PEG-based parsing for multiple patch formats including:
//! - Unified Diff
//! - Git Diff
//! - Ed Script
//!
//! Also includes patch management with apply/unapply/yank capabilities.

pub mod manager;
pub mod parser;

pub use manager::{Patch, PatchId, PatchManager, PatchMetadata, PatchSummary, YankReport};
pub use parser::{FilePatch, Hunk, Line, ParsedPatch, PatchFormat, PatchParser};
