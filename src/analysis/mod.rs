//! Code analysis for intelligent veiling operations.
//!
//! This module provides analysis capabilities including:
//! - Call graph construction and traversal
//! - Cross-file symbol resolution
//! - Entrypoint detection
//! - Caching layer

pub mod cache;
pub mod call_graph;
pub mod entrypoints;

pub use cache::{AnalysisCache, CachedParser, CacheStats};
pub use call_graph::{CallGraph, CallGraphBuilder, TraceDirection, TraceResult};
pub use entrypoints::{Entrypoint, EntrypointDetector, EntrypointType};
