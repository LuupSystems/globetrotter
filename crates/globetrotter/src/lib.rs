//! Polyglot, type-safe internationalization.
//!
//! This crate parses globetrotter configuration files, reads translation
//! sources, validates them, and generates JSON output. The `typescript` and
//! `rust` features add typed bindings; `golang` and `python` currently expose
//! configuration targets without emitters.
//!
//! Feature flags select language-specific generators: `typescript`, `rust`,
//! `golang`, and `python`. The `llm-judge` feature adds optional semantic-drift
//! review during linting.
//!
//! The same builder API used by the CLI is available for build scripts and
//! other programmatic integrations:
//!
//! ```no_run
//! use globetrotter::{
//!     Executor, Language,
//!     config::v1::{Config, ConfigFile, Input, JsonOutputConfig, Outputs},
//!     diagnostics::Printer,
//! };
//!
//! # async fn generate() -> Result<(), globetrotter::Error> {
//! let config = Config::new("app")
//!     .with_languages([Language::En, Language::De])
//!     .with_input(Input::new("translations.toml"))
//!     .with_outputs(
//!         Outputs::new().with_json([JsonOutputConfig::new(
//!             "generated/translations_{{language}}.json",
//!         )]),
//!     );
//! let configs: Vec<ConfigFile<usize>> = vec![ConfigFile {
//!     file_id: None,
//!     config_dir: None,
//!     config,
//! }];
//! let executor = Executor::new(&configs, Printer::default());
//! executor.execute(configs).await?;
//! # Ok(())
//! # }
//! ```

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
/// LLM-judged translation-consistency review during linting.
#[cfg(feature = "llm-judge")]
pub mod llm_judge;
/// Progress logging and output path formatting.
pub mod progress;
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
