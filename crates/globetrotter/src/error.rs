//! Errors produced while loading, validating, and generating translations.

use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use globetrotter_model::diagnostics::Span;
use std::path::PathBuf;

/// Counts of the error and warning diagnostics emitted by a run.
///
/// Notes and help messages are not counted: they never affect whether a run
/// succeeds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// The number of error diagnostics.
    pub errors: usize,
    /// The number of warning diagnostics.
    pub warnings: usize,
}

impl Tally {
    /// Counts one diagnostic of `severity`.
    pub fn record(&mut self, severity: Severity) {
        match severity {
            Severity::Bug | Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
            Severity::Note | Severity::Help => {}
        }
    }

    /// Returns `true` if any error was counted.
    #[must_use]
    pub fn has_errors(self) -> bool {
        self.errors > 0
    }

    /// Returns `true` if any error or warning was counted.
    #[must_use]
    pub fn has_issues(self) -> bool {
        self.errors > 0 || self.warnings > 0
    }

    /// Fails with [`FailedWithErrors`] if any error was counted.
    ///
    /// # Errors
    ///
    /// Returns the tally as a [`FailedWithErrors`] when it holds an error.
    pub fn fail_on_errors(self) -> Result<Self, FailedWithErrors> {
        if self.has_errors() {
            Err(FailedWithErrors(self))
        } else {
            Ok(self)
        }
    }
}

impl std::ops::AddAssign for Tally {
    fn add_assign(&mut self, other: Self) {
        self.errors += other.errors;
        self.warnings += other.warnings;
    }
}

impl std::fmt::Display for Tally {
    /// Formats the counts, mentioning errors only when there are any.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.errors > 0 {
            write!(f, "{} {} and ", self.errors, plural(self.errors, "error"))?;
        }
        write!(f, "{} {}", self.warnings, plural(self.warnings, "warning"))
    }
}

/// The singular or plural form of `noun` for `count`.
fn plural(count: usize, noun: &'static str) -> String {
    if count == 1 {
        noun.to_string()
    } else {
        format!("{noun}s")
    }
}

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

/// Indicates that processing completed but surfaced diagnostics that fail the
/// run: errors during generation, or any finding during linting.
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("globetrotter failed with {0}")]
pub struct FailedWithErrors(pub Tally);

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

#[cfg(test)]
mod tests {
    use super::Tally;
    use codespan_reporting::diagnostic::Severity;

    /// Only errors and warnings count, and the summary reads naturally for
    /// every count.
    #[test_util::test]
    fn tally_counts_and_pluralizes() {
        let mut tally = Tally::default();
        for severity in [
            Severity::Error,
            Severity::Warning,
            Severity::Warning,
            Severity::Note,
            Severity::Help,
            Severity::Bug,
        ] {
            tally.record(severity);
        }
        assert_eq!(tally.to_string(), "2 errors and 2 warnings");
        assert!(tally.has_errors());
        assert!(tally.fail_on_errors().is_err());

        let one = Tally {
            errors: 1,
            warnings: 0,
        };
        assert_eq!(one.to_string(), "1 error and 0 warnings");

        let warnings_only = Tally {
            errors: 0,
            warnings: 1,
        };
        assert_eq!(warnings_only.to_string(), "1 warning");
        assert!(warnings_only.has_issues());
        assert_eq!(warnings_only.fail_on_errors(), Ok(warnings_only));
    }
}
