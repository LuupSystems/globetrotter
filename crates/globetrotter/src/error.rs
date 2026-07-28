//! Errors produced while loading, validating, and generating translations.

use codespan_reporting::diagnostic::{Diagnostic, Label};
use globetrotter_model::diagnostics::Span;
use std::path::PathBuf;

/// An I/O error annotated with the path that produced it.
#[derive(thiserror::Error, Debug)]
#[error("{path}: {inner}")]
pub struct IoError {
    /// The path that was being operated on.
    pub path: PathBuf,
    /// The underlying I/O error.
    pub inner: std::io::Error,
}

impl IoError {
    /// Creates an [`IoError`] for the given path and source error.
    pub fn new(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self {
            inner: source,
            path: path.into(),
        }
    }
}

/// An error produced while generating one of the configured outputs.
#[derive(thiserror::Error, Debug)]
pub enum OutputError {
    /// Generating JSON output failed.
    #[error("failed to generate JSON output")]
    Json(#[from] crate::json::JsonOutputError),

    /// Generating TypeScript output failed.
    #[cfg(feature = "typescript")]
    #[error("failed to generate typescript output")]
    Typescript(#[from] crate::target::TypescriptOutputError),

    /// Generating Rust output failed.
    #[cfg(feature = "rust")]
    #[error("failed to generate rust output")]
    Rust(#[from] crate::target::RustOutputError),

    /// Generating Go output failed.
    #[cfg(feature = "golang")]
    #[error("failed to generate golang output")]
    Golang(#[from] crate::target::GolangOutputError),

    /// Generating Python output failed.
    #[cfg(feature = "python")]
    #[error("failed to generate python output")]
    Python(#[from] crate::target::PythonOutputError),
}

/// The top-level error type returned by the executor.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// An input glob pattern was malformed.
    #[error("invalid glob pattern {path:?}")]
    Pattern {
        /// The underlying pattern error.
        #[source]
        source: glob::PatternError,
        /// The pattern that could not be compiled.
        path: String,
    },

    /// Iterating the matches of a glob pattern failed.
    #[error("failed to glob for pattern {path}")]
    Glob {
        /// The underlying glob error.
        #[source]
        source: glob::GlobError,
        /// The pattern that was being expanded.
        path: String,
    },

    /// An I/O operation failed.
    #[error(transparent)]
    Io(#[from] IoError),

    /// Generating an output failed.
    #[error(transparent)]
    Output(#[from] OutputError),

    /// Parsing TOML translation input failed.
    #[error(transparent)]
    Toml(#[from] crate::model::toml::Error),

    /// Processing finished with diagnostic errors.
    #[error(transparent)]
    Failed(#[from] FailedWithErrors),

    /// A spawned task failed to join.
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),

    /// Emitting a diagnostic to the output failed.
    #[error("failed to emit diagnostic")]
    Diagnostic(#[from] codespan_reporting::files::Error),

    /// The LLM judge failed.
    #[cfg(feature = "llm-judge")]
    #[error(transparent)]
    LlmJudge(#[from] globetrotter_llm_judge::Error),
}

/// Indicates that processing completed but surfaced one or more error
/// diagnostics.
#[derive(thiserror::Error, Debug)]
pub struct FailedWithErrors {
    /// The number of error diagnostics emitted.
    pub num_errors: usize,
    /// The number of warning diagnostics emitted.
    pub num_warnings: usize,
}

impl std::fmt::Display for FailedWithErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "globetrotter failed with {} {} and {} {}",
            self.num_errors,
            if self.num_errors > 1 {
                "errors"
            } else {
                "error"
            },
            self.num_warnings,
            if self.num_warnings > 1 {
                "warnings"
            } else {
                "warning"
            },
        )
    }
}

/// A translation key that was defined more than once across input files.
#[derive(thiserror::Error, Debug)]
#[error("duplicate key {key:?}")]
pub struct DuplicateKeyError<F: Copy + PartialEq> {
    /// The duplicated key.
    pub key: String,
    /// Definitions in encounter order, with the final one treated as the
    /// duplicate that triggered the error.
    pub occurrences: Vec<(Span, F)>,
}

impl<F> DuplicateKeyError<F>
where
    F: Copy + PartialEq,
{
    /// Renders this duplicate-key error into diagnostics.
    ///
    /// When `all` is `true`, every prior occurrence is highlighted. Otherwise,
    /// only the most recent prior occurrence is labelled. An empty occurrence
    /// list produces an unlabelled diagnostic.
    #[must_use]
    pub fn to_diagnostics(&self, all: bool) -> Vec<Diagnostic<F>> {
        let mut labels = vec![];

        match self.occurrences.split_last() {
            None => {
                // Without an occurrence, the diagnostic has no source label.
            }
            Some((last, rest)) => {
                if all {
                    labels.extend(rest.iter().map(|(span, file_id)| {
                        Label::secondary(*file_id, span.clone())
                            .with_message(format!("previous use of key `{}`", self.key))
                    }));
                } else if let Some((span, file_id)) = rest.last() {
                    let label = Label::secondary(*file_id, span.clone()).with_message(format!(
                        "first use of key `{}`{}",
                        self.key,
                        if rest.len() > 1 {
                            format!(" (duplicated {} more time)", rest.len() - 1)
                        } else {
                            String::new()
                        },
                    ));
                    labels.push(label);
                }

                let (span, file_id) = last;
                labels.push(
                    Label::primary(*file_id, span.clone())
                        .with_message("cannot set the same key twice"),
                );
            }
        }

        vec![
            Diagnostic::error()
                .with_message(format!("duplicate key `{}`", self.key))
                .with_labels(labels),
        ]
    }
}
