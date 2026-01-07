use crate::{
    config::{
        config_file_names,
        v1::{self as config, PathOrGlobPattern},
    },
    model,
    progress::Logger,
    target::{self, Target},
};
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use globetrotter_model::diagnostics::{DiagnosticExt, FileId, Span, Spanned, ToDiagnostics};
use std::path::{Path, PathBuf};

#[derive(thiserror::Error, Debug)]
#[error("{path}: {inner}")]
pub struct IoError {
    pub path: PathBuf,
    pub inner: std::io::Error,
}

impl IoError {
    pub fn new(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self {
            inner: source,
            path: path.into(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum OutputError {
    #[error("failed to generate JSON output")]
    Json(#[from] crate::json::JsonOutputError),

    #[cfg(feature = "typescript")]
    #[error("failed to generate typescript output")]
    Typescript(#[from] target::TypescriptOutputError),

    #[cfg(feature = "rust")]
    #[error("failed to generate rust output")]
    Rust(#[from] target::RustOutputError),

    #[cfg(feature = "golang")]
    #[error("failed to generate golang output")]
    Golang(#[from] target::GolangOutputError),

    #[cfg(feature = "python")]
    #[error("failed to generate python output")]
    Python(#[from] target::PythonOutputError),
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("invalid glob pattern {path:?}")]
    Pattern {
        #[source]
        source: glob::PatternError,
        path: String,
    },

    #[error("failed to glob for pattern {path}")]
    Glob {
        #[source]
        source: glob::GlobError,
        path: String,
    },

    #[error(transparent)]
    Io(#[from] IoError),

    #[error(transparent)]
    Output(#[from] OutputError),

    #[error(transparent)]
    Toml(#[from] crate::model::toml::Error),

    #[error(transparent)]
    Failed(#[from] FailedWithErrors),

    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),

    #[error("failed to emit diagnostic")]
    Diagnostic(#[from] codespan_reporting::files::Error),
}

#[derive(thiserror::Error, Debug)]
pub struct FailedWithErrors {
    pub num_errors: usize,
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

#[derive(thiserror::Error, Debug)]
#[error("duplicate key {key:?}")]
pub struct DuplicateKeyError<F: Copy + PartialEq> {
    pub key: String,
    pub occurrences: Vec<(Span, F)>,
}

impl<F> DuplicateKeyError<F>
where
    F: Copy + PartialEq,
{
    #[must_use]
    pub fn to_diagnostics(&self, all: bool) -> Vec<Diagnostic<F>> {
        let mut labels = vec![];

        match self.occurrences.split_last() {
            None => {
                // No occurrences recorded; nothing to highlight.
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
                // .with_code("E0384")
                .with_message(format!("duplicate key `{}`", self.key))
                .with_labels(labels),
        ]
    }
}
