pub mod settings;
/// Version 1 of the configuration schema and its parsing routines.
pub mod v1;

pub use settings::{Settings, SettingsLayer};

#[cfg(feature = "python")]
pub use globetrotter_python as python;

#[cfg(feature = "rust")]
pub use globetrotter_rust as rust;

#[cfg(feature = "typescript")]
pub use globetrotter_typescript as typescript;

#[cfg(feature = "golang")]
pub use globetrotter_golang as golang;

use codespan_reporting::diagnostic::{Diagnostic, Label};

use globetrotter_model::diagnostics::{DiagnosticExt, Span, ToDiagnostics};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use yaml_spanned::Value;

/// The configuration schema version.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize, Default,
)]
pub enum Version {
    /// Version 1 of the configuration schema.
    #[serde(rename = "1", alias = "v1", alias = "V1")]
    V1,
    /// The latest supported schema version.
    #[serde(rename = "latest")]
    #[default]
    Latest,
}

/// The supported configuration file names, in search order.
pub fn config_file_names() -> impl Iterator<Item = &'static str> {
    [".globetrotter.yaml", "globetrotter.yaml"].into_iter()
}

/// Search for a globetrotter configuration file in the given directory.
///
/// # Errors
///
/// Returns an error if accessing the filesystem fails while probing for the
/// supported configuration file names.
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

/// Synchronously search for a globetrotter configuration file in the given
/// directory.
///
/// # Errors
///
/// Returns an error if accessing the filesystem fails while probing for the
/// supported configuration file names.
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

/// Parse a raw YAML configuration string into typed `Configs`.
///
/// # Errors
///
/// Returns an error if the YAML cannot be parsed or if the configuration
/// schema is invalid for the detected version.
pub fn from_str<F: Copy + PartialEq>(
    raw_config: &str,
    config_dir: &Path,
    file_id: F,
    strict: Option<bool>,
    diagnostics: &mut Vec<Diagnostic<F>>,
) -> Result<v1::Configs<F>, ConfigError> {
    let value = yaml_spanned::from_str(raw_config).map_err(ConfigError::YAML)?;
    let version = parse_version(&value, file_id, strict, diagnostics)?;

    match version {
        Version::Latest | Version::V1 => {
            v1::parse_configs(&value, config_dir, file_id, strict, diagnostics)
        }
    }
}

/// An error that can occur while parsing a configuration file.
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    /// A required key was missing from the configuration.
    #[error("{message}")]
    MissingKey {
        /// The name of the missing key.
        key: String,
        /// A human-readable description of the problem.
        message: String,
        /// The span of the surrounding value.
        span: Span,
    },
    /// A value had a type other than the one expected.
    #[error("{message}")]
    UnexpectedType {
        /// A human-readable description of the problem.
        message: String,
        /// The kinds that would have been accepted.
        expected: Vec<yaml_spanned::value::Kind>,
        /// The kind that was actually found.
        found: yaml_spanned::value::Kind,
        /// The span of the offending value.
        span: Span,
    },
    /// Deserialization of a value into a typed representation failed.
    #[error("{source}")]
    Serde {
        /// The underlying deserialization error.
        #[source]
        source: yaml_spanned::error::SerdeError,
        /// The span of the offending value.
        span: Span,
    },
    /// The underlying YAML could not be parsed.
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

/// Parse the configuration `version` field from the given YAML value.
///
/// # Errors
///
/// Returns an error if the `version` field is present but cannot be parsed
/// into a supported `Version` value.
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
    use color_eyre::eyre;
    use similar_asserts::assert_eq as sim_assert_eq;
    use yaml_spanned::{Spanned, Value};

    /// Every settings key parses into the config's [`SettingsLayer`],
    /// including the `engine` and `absolute` spelling aliases.
    #[test]
    fn parses_settings_keys_and_aliases() -> eyre::Result<()> {
        use globetrotter_model::{TemplateEngine, diagnostics::Spanned};

        let raw = unindent::unindent(
            r#"
            version: 1
            config:
              languages: ["en"]
              engine: handlebars
              strict: true
              check_templates: false
              dry_run: true
              absolute: true
              inputs:
                - ./translations/a.toml
              outputs:
                json:
                  - ./out/{{language}}.json
            "#,
        );
        let mut diagnostics = vec![];
        let configs = super::from_str(&raw, std::path::Path::new("."), (), None, &mut diagnostics)?;

        // Spanned comparisons ignore spans, so dummy spans match parsed ones.
        sim_assert_eq!(
            have: configs[0].config.settings,
            want: super::SettingsLayer {
                strict: Some(true),
                check_templates: Some(false),
                dry_run: Some(true),
                print_absolute_paths: Some(true),
                template_engine: Some(Spanned::dummy(TemplateEngine::Handlebars)),
            }
        );
        Ok(())
    }

    #[test]
    fn test_parse_version() -> eyre::Result<()> {
        fn parse_version_wrapper(
            value: impl Into<Spanned<Value>>,
            strict: bool,
        ) -> Result<super::Version, ConfigError> {
            let mut diagnostics = vec![];

            super::parse_version(&value.into(), (), Some(strict), &mut diagnostics)
        }

        let have = parse_version_wrapper(
            Value::Mapping([("version".into(), 1.into())].into_iter().collect()),
            true,
        )?;
        sim_assert_eq!(
            have: have,
            want: super::Version::V1
        );

        let have = parse_version_wrapper(
            Value::Mapping([("version".into(), "1".into())].into_iter().collect()),
            true,
        )?;
        sim_assert_eq!(
            have: have,
            want: super::Version::V1
        );

        let have = parse_version_wrapper(
            Value::Mapping([("version".into(), "v1".into())].into_iter().collect()),
            true,
        )?;
        sim_assert_eq!(
            have: have,
            want: super::Version::V1
        );
        Ok(())
    }
}
