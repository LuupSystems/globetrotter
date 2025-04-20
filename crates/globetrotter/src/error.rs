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
    pub fn to_diagnostics(&self, all: bool) -> Vec<Diagnostic<F>> {
        assert!(
            self.occurrences.len() >= 2,
            "duplicated key must have at least two occurrences"
        );

        let mut labels = vec![];
        if all {
            labels.extend(self.occurrences[..self.occurrences.len() - 1].iter().map(
                |(span, file_id)| {
                    Label::secondary(file_id.clone(), span.clone())
                        .with_message(format!("previous use of key `{}`", self.key))
                },
            ));
        } else {
            let (span, file_id) = &self.occurrences[self.occurrences.len() - 2];
            let label = Label::secondary(file_id.clone(), span.clone()).with_message(format!(
                "first use of key `{}`{}",
                self.key,
                if self.occurrences.len() > 2 {
                    format!(" (duplicated {} more time)", self.occurrences.len() - 2)
                } else {
                    "".to_string()
                },
            ));
            labels.push(label);
        }

        let (span, file_id) = &self.occurrences[self.occurrences.len() - 1];
        labels.push(
            Label::primary(file_id.clone(), span.clone())
                .with_message("cannot set the same key twice"),
        );

        vec![Diagnostic::error()
            // .with_code("E0384")
            .with_message(format!("duplicate key `{}`", self.key))
            .with_labels(labels)]
    }
}
