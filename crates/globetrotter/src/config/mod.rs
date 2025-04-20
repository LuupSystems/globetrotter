pub mod v1;

use codespan_reporting::diagnostic::{Diagnostic, Label};

use globetrotter_model::{
    self as model,
    diagnostics::{self, DiagnosticExt, Span, ToDiagnostics},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use yaml_spanned::{Spanned, Value};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub enum Version {
    #[serde(rename = "1", alias = "v1", alias = "V1")]
    V1,
    #[serde(rename = "latest")]
    Latest,
}

impl Default for Version {
    fn default() -> Self {
        Self::Latest
    }
}

pub fn config_file_names() -> impl Iterator<Item = &'static str> {
    [".globetrotter.yaml", "globetrotter.yaml"].into_iter()
}

pub async fn find_config_file(dir: &Path) -> std::io::Result<Option<PathBuf>> {
    use futures::{StreamExt, TryStreamExt, stream};
    let mut found = stream::iter(config_file_names().map(|path| dir.join(path)))
        .map(|path| async move {
            match tokio::fs::canonicalize(&path).await {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(err) => Err(err),
                Ok(path) => Ok(Some(path)),
            }
        })
        .buffered(8)
        .into_stream();

    while let Some(path) = found.try_next().await? {
        if let Some(path) = path {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

#[allow(dead_code)]
pub fn find_config_file_sync(dir: &Path) -> std::io::Result<Option<PathBuf>> {
    for path in config_file_names().map(|path| dir.join(path)) {
        match std::fs::exists(&path) {
            Err(err) => return Err(err),
            Ok(true) => return Ok(Some(path)),
            Ok(false) => {
                // skip
            }
        }
    }
    Ok(None)
}

pub fn from_str<F: Copy + PartialEq>(
    raw_config: &str,
    config_dir: &Path,
    file_id: F,
    strict: Option<bool>,
    diagnostics: &mut Vec<Diagnostic<F>>,
) -> Result<v1::Configs<F>, ConfigError> {
    let value = yaml_spanned::from_str(&raw_config).map_err(ConfigError::YAML)?;
    let version = parse_version(&value, file_id, strict, diagnostics)?;
    let configs = match version {
        Version::Latest | Version::V1 => {
            v1::parse_configs(&value, config_dir, file_id, strict, diagnostics)
        }
    };

    configs
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("{message}")]
    MissingKey {
        key: String,
        message: String,
        span: Span,
    },
    #[error("{message}")]
    UnexpectedType {
        message: String,
        expected: Vec<yaml_spanned::value::Kind>,
        found: yaml_spanned::value::Kind,
        span: Span,
    },
    #[error("{source}")]
    Serde {
        #[source]
        source: yaml_spanned::error::SerdeError,
        span: Span,
    },
    #[error(transparent)]
    YAML(#[from] yaml_spanned::Error),
}

impl ToDiagnostics for ConfigError {
    fn to_diagnostics<F: Copy + PartialEq>(&self, file_id: F) -> Vec<Diagnostic<F>> {
        match self {
            Self::MissingKey {
                message, key, span, ..
            } => vec![
                Diagnostic::error()
                    .with_message(format!("missing required key `{key}`"))
                    .with_labels(vec![
                        Label::secondary(file_id, span.clone()).with_message(message),
                    ]),
            ],
            Self::UnexpectedType {
                expected,
                found,
                span,
                ..
            } => {
                let expected = expected
                    .iter()
                    .map(|ty| format!("`{ty:?}`"))
                    .collect::<Vec<_>>()
                    .join(", or ");
                let diagnostic = Diagnostic::error()
                    .with_message(self.to_string())
                    .with_labels(vec![
                        Label::primary(file_id, span.clone())
                            .with_message(format!("expected {expected}")),
                    ])
                    .with_notes(vec![unindent::unindent(&format!(
                        "
                        expected type {expected}
                           found type `{found:?}`
                        "
                    ))]);
                vec![diagnostic]
            }
            Self::Serde { source, span } => vec![
                Diagnostic::error()
                    .with_message(self.to_string())
                    .with_labels(vec![
                        Label::primary(file_id, span.clone()).with_message(source.to_string()),
                    ]),
            ],
            Self::YAML(source) => {
                use yaml_spanned::error::ToDiagnostics;
                source.to_diagnostics(file_id)
            }
        }
    }
}

pub fn parse_version<F>(
    value: &yaml_spanned::Spanned<Value>,
    file_id: F,
    strict: Option<bool>,
    diagnostics: &mut Vec<Diagnostic<F>>,
) -> Result<Version, ConfigError> {
    match value.get("version") {
        None => {
            let diagnostic = Diagnostic::warning_or_error(strict.unwrap_or(false))
                .with_message("missing version")
                .with_labels(vec![
                    Label::primary(file_id, value.span)
                        .with_message("no version is specified - assuming version 1"),
                ]);
            diagnostics.push(diagnostic);
            Ok(Version::Latest)
        }
        Some(yaml_spanned::Spanned {
            inner: Value::Number(n),
            ..
        }) if n.as_f64() == Some(1.0) => Ok(Version::V1),
        Some(value) => {
            let version = v1::parse::<Version>(value)?;
            Ok(version.into_inner())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigError;
    use codespan_reporting::diagnostic::Diagnostic;
    use color_eyre::eyre;
    use similar_asserts::assert_eq as sim_assert_eq;
    use yaml_spanned::{Mapping, Spanned, Value};

    #[test]
    fn parse_version() -> eyre::Result<()> {
        // fn parse_version_diagnostics(
        //     value: impl Into<Spanned<Value>>,
        //     strict: bool,
        // ) -> (Result<super::Version, ConfigError>, Vec<Diagnostic<()>>) {
        //     let mut diagnostics = vec![];
        //     let res = super::parse_version(&value.into(), (), Some(strict), &mut diagnostics);
        //     (res, diagnostics)
        // }

        fn parse_version(
            value: impl Into<Spanned<Value>>,
            strict: bool,
        ) -> Result<super::Version, ConfigError> {
            let mut diagnostics = vec![];
            let res = super::parse_version(&value.into(), (), Some(strict), &mut diagnostics);
            res
        }

        sim_assert_eq!(
            parse_version(
                Value::Mapping([("version".into(), 1.into())].into_iter().collect()),
                true
            )
            .ok(),
            Some(super::Version::V1)
        );

        sim_assert_eq!(
            parse_version(
                Value::Mapping([("version".into(), "1".into())].into_iter().collect()),
                true
            )
            .ok(),
            Some(super::Version::V1)
        );

        sim_assert_eq!(
            parse_version(
                Value::Mapping([("version".into(), "v1".into())].into_iter().collect()),
                true
            )
            .ok(),
            Some(super::Version::V1)
        );
        Ok(())
    }
}
