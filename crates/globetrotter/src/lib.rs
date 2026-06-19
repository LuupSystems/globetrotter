//! Polyglot, type-safe internationalization.
//!
//! This crate parses globetrotter configuration files, reads translation
//! sources, validates them, and generates JSON and language-specific outputs
//! (TypeScript, Rust, Go, Python) for the enabled feature targets.

/// Configuration file discovery, parsing, and versioned schema types.
pub mod config;
/// Detection of translation keys unused in a source tree.
pub mod dead_keys;
/// Diagnostic rendering and source-file management.
pub mod diagnostics;
/// Error types surfaced while loading and generating outputs.
pub mod error;
/// Orchestration of translation loading, validation, and output generation.
pub mod executor;
/// Gzip size estimation for generated JSON outputs.
pub mod gzip;
/// JSON translation output generation.
pub mod json;
/// Progress logging and output path formatting.
pub mod progress;
/// Cross-lingual semantic drift detection during linting.
#[cfg(feature = "semantic")]
pub mod semantic;
/// Code generation targets and their per-target output errors.
pub mod target;

#[cfg(feature = "typescript")]
pub use globetrotter_typescript as typescript;

#[cfg(feature = "rust")]
pub use globetrotter_rust as rust;

#[cfg(feature = "golang")]
pub use globetrotter_golang as golang;

#[cfg(feature = "python")]
pub use globetrotter_python as python;

pub use error::Error;
pub use executor::Executor;
pub use globetrotter_model as model;
pub use model::{Language, Translation, Translations};
