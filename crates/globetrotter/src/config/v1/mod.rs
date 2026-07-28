use super::ConfigError;
use super::settings::SettingsLayer;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use globetrotter_model::{
    self as model,
    diagnostics::{DiagnosticExt, DisplayRepr, Spanned},
};
use std::path::{Path, PathBuf};
use yaml_spanned::{Mapping, Sequence, Value, value::Kind};

/// A single parsed configuration together with its source location.
#[derive(Debug)]
pub struct ConfigFile<F> {
    /// The diagnostic file id of the source file, if any.
    pub file_id: Option<F>,
    /// The directory the configuration file was loaded from.
    pub config_dir: Option<PathBuf>,
    /// The parsed configuration.
    pub config: Config,
}

/// A list of parsed configurations.
pub type Configs<F> = Vec<ConfigFile<F>>;

/// Parse the `languages` configuration for a single config.
///
/// # Errors
///
/// Returns an error if the languages section is present but not a sequence, or if
/// any language value cannot be deserialized into a Language enum variant.
pub fn parse_languages<F>(
    value: &yaml_spanned::Spanned<Value>,
    file_id: F,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<F>>,
) -> Result<Vec<Spanned<model::Language>>, ConfigError> {
    match value.get("languages") {
        None => {
            let diagnostic = Diagnostic::warning_or_error(strict)
                .with_message("empty languages")
                .with_labels(vec![Label::primary(file_id, value.span).with_message(
                    "no languages specified - no JSON translation file will be generated",
                )]);
            diagnostics.push(diagnostic);
            Ok(vec![])
        }
        Some(value) => {
            let languages = value
                .as_sequence()
                .ok_or_else(|| ConfigError::UnexpectedType {
                    message: "list of languages must be a sequence".to_string(),
                    found: value.kind(),
                    expected: vec![Kind::Sequence],
                    span: value.span().into(),
                })?;

            let languages = languages.iter().map(parse).collect::<Result<Vec<_>, _>>()?;
            Ok(languages)
        }
    }
}

/// Parse a typed value from YAML.
///
/// # Errors
///
/// Returns an error if the value cannot be deserialized into the target type.
pub fn parse<T: serde::de::DeserializeOwned>(
    value: &yaml_spanned::Spanned<Value>,
) -> Result<Spanned<T>, ConfigError> {
    let inner: T = yaml_spanned::from_value(value).map_err(|source| ConfigError::Serde {
        source,
        span: value.span().into(),
    })?;
    Ok(Spanned::new(value.span, inner))
}

/// Parse an optional typed value from YAML.
///
/// # Errors
///
/// Returns an error if the value is present but cannot be deserialized into
/// the target type.
pub fn parse_optional<T: serde::de::DeserializeOwned>(
    value: Option<&yaml_spanned::Spanned<Value>>,
) -> Result<Option<Spanned<T>>, ConfigError> {
    value.map(|value| parse(value)).transpose()
}

/// Parse a single input entry from YAML.
///
/// # Errors
///
/// Returns an error if the input entry has an unexpected type or is missing
/// required fields.
pub fn parse_input<F>(
    value: &yaml_spanned::Spanned<Value>,
    file_id: F,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<F>>,
) -> Result<Option<Input>, ConfigError> {
    match value.as_ref() {
        Value::Null => {
            let diagnostic = Diagnostic::warning_or_error(strict)
                .with_message("empty input")
                .with_labels(vec![
                    Label::primary(file_id, value.span).with_message("empty input will be ignored"),
                ]);
            diagnostics.push(diagnostic);
            Ok(None)
        }
        Value::String(path) => Ok(Some(Input {
            path_or_glob_pattern: Spanned::new(value.span, path.clone()),
            exclude: Vec::new(),
            prefix: None,
            prepend_filename: None,
            prepend_relative_path: None,
            separator: None,
        })),
        Value::Mapping(mapping) => {
            let path_value = mapping.get("path").ok_or_else(|| ConfigError::MissingKey {
                key: "path".to_string(),
                message: "missing path to input file".to_string(),
                span: value.span.into(),
            })?;
            let path_or_glob_pattern = parse::<PathOrGlobPattern>(path_value)?;
            let exclude = match mapping.get("exclude") {
                None => Ok(vec![]),
                Some(yaml_spanned::Spanned {
                    span,
                    inner: Value::String(path_or_glob_pattern),
                }) => Ok(vec![Spanned::new(*span, path_or_glob_pattern.clone())]),
                Some(yaml_spanned::Spanned {
                    inner: Value::Sequence(_sequence),
                    ..
                }) => Ok(vec![]),
                Some(other) => Err(ConfigError::UnexpectedType {
                    message: "exclude must be a path or a sequence of paths".to_string(),
                    found: other.kind(),
                    expected: vec![Kind::Sequence, Kind::String],
                    span: other.span().into(),
                }),
            }?;
            let prefix = parse_optional::<String>(mapping.get("prefix"))?;
            let prepend_filename = parse_optional::<bool>(mapping.get("prepend_filename"))?;
            let prepend_relative_path =
                parse_optional::<bool>(mapping.get("prepend_relative_path"))?;
            let separator = parse_optional::<String>(mapping.get("separator"))?;
            Ok(Some(Input {
                path_or_glob_pattern,
                exclude,
                prefix,
                prepend_filename,
                prepend_relative_path,
                separator,
            }))
        }
        _ => Err(ConfigError::UnexpectedType {
            message: "input must be a path or a mapping".to_string(),
            found: value.kind(),
            expected: vec![Kind::Mapping, Kind::String],
            span: value.span().into(),
        }),
    }
}

/// Expect a YAML value to be a sequence.
///
/// # Errors
///
/// Returns an error if the value is not a sequence.
pub fn expect_sequence(value: &yaml_spanned::Spanned<Value>) -> Result<&Sequence, ConfigError> {
    value
        .as_sequence()
        .ok_or_else(|| ConfigError::UnexpectedType {
            message: "expected sequence".to_string(),
            expected: vec![Kind::Sequence],
            found: value.kind(),
            span: value.span().into(),
        })
}

/// Expect a YAML value to be a mapping.
///
/// # Errors
///
/// Returns an error if the value is not a mapping.
pub fn expect_mapping(
    value: &yaml_spanned::Spanned<Value>,
) -> Result<(&yaml_spanned::spanned::Span, &Mapping), ConfigError> {
    let mapping = value
        .as_mapping()
        .ok_or_else(|| ConfigError::UnexpectedType {
            message: "expected mapping".to_string(),
            expected: vec![Kind::Mapping],
            found: value.kind(),
            span: value.span().into(),
        })?;
    Ok((value.span(), mapping))
}

#[cfg(feature = "rust")]
/// Parse the Rust output configuration.
///
/// # Errors
///
/// Returns an error if the `rust`/`rs` output configuration has an unexpected
/// type or contains invalid output paths.
pub fn parse_rust_outputs(
    value: &Mapping,
) -> Result<Option<globetrotter_rust::OutputConfig>, ConfigError> {
    use globetrotter_rust::config::OutputConfig;

    let Some(outputs) = value.get("rust").or_else(|| value.get("rs")) else {
        return Ok(None);
    };
    let paths = match outputs.as_ref() {
        Value::String(path) => Ok(vec![path.into()]),
        Value::Sequence(paths) => paths
            .iter()
            .map(|path| {
                let path = path
                    .as_string()
                    .ok_or_else(|| ConfigError::UnexpectedType {
                        message: "expected file path".to_string(),
                        expected: vec![Kind::String],
                        found: path.kind(),
                        span: path.span().into(),
                    })?;
                Ok(path.into())
            })
            .collect::<Result<Vec<PathBuf>, ConfigError>>(),
        other => Err(ConfigError::UnexpectedType {
            message: "expected file path or sequence of file paths".to_string(),
            expected: vec![Kind::Sequence, Kind::String],
            found: other.kind(),
            span: outputs.span().into(),
        }),
    }?;
    Ok(Some(OutputConfig {
        output_paths: paths,
    }))
}

#[cfg(feature = "typescript")]
/// Parse the TypeScript output configuration.
///
/// # Errors
///
/// Returns an error if the `typescript`/`ts` output configuration has an
/// unexpected type or contains invalid output paths.
pub fn parse_typescript_outputs(
    value: &Mapping,
) -> Result<Option<globetrotter_typescript::OutputConfig>, ConfigError> {
    use globetrotter_typescript::config::InterfaceTypeOutputConfig;

    let Some(outputs) = value.get("typescript").or_else(|| value.get("ts")) else {
        return Ok(None);
    };
    let (_span, outputs) = expect_mapping(outputs)?;

    let interface_type: Vec<_> = outputs
        .get("type")
        .or_else(|| outputs.get("interface"))
        .or_else(|| outputs.get("dts"))
        .map(|path| match path.as_ref() {
            Value::String(path) => Ok(vec![InterfaceTypeOutputConfig { path: path.into() }]),
            Value::Sequence(sequence) => {
                let interfaces = sequence
                    .iter()
                    .map(|path| {
                        let path = path
                            .as_string()
                            .ok_or_else(|| ConfigError::UnexpectedType {
                                message: "expected file path".to_string(),
                                expected: vec![Kind::String],
                                found: path.kind(),
                                span: path.span().into(),
                            })?;
                        Ok(InterfaceTypeOutputConfig { path: path.into() })
                    })
                    .collect::<Result<Vec<_>, ConfigError>>()?;
                Ok(interfaces)
            }
            other => Err(ConfigError::UnexpectedType {
                message: "expected file path or sequence of file paths".to_string(),
                expected: vec![Kind::Sequence, Kind::String],
                found: other.kind(),
                span: path.span().into(),
            }),
        })
        .transpose()?
        .unwrap_or_default();

    Ok(Some(globetrotter_typescript::OutputConfig {
        interface_type,
    }))
}

/// Parse the JSON output configuration.
///
/// # Errors
///
/// Returns an error if the `json`/`translations` output configuration has an
/// unexpected type or is missing required fields.
pub fn parse_json_outputs(value: &Mapping) -> Result<Vec<JsonOutputConfig>, ConfigError> {
    let Some(outputs) = value.get("json").or_else(|| value.get("translations")) else {
        return Ok(vec![]);
    };

    let parse_json_output =
        |value: &yaml_spanned::Spanned<Value>| -> Result<JsonOutputConfig, ConfigError> {
            match value.as_ref() {
                Value::String(path) => Ok(JsonOutputConfig {
                    path: Spanned::new(value.span, path.into()),
                    style: None,
                }),
                Value::Mapping(mapping) => {
                    // get path
                    let path = mapping.get("path").ok_or_else(|| ConfigError::MissingKey {
                        key: "path".to_string(),
                        message: "missing path to output JSON file".to_string(),
                        span: value.span().into(),
                    })?;
                    let path = parse::<PathBuf>(path)?;
                    let style = parse_optional::<JsonOutputStyle>(mapping.get("style"))?;
                    Ok(JsonOutputConfig { path, style })
                }
                other => Err(ConfigError::UnexpectedType {
                    message: "expected file path or sequence of file paths".to_string(),
                    expected: vec![Kind::Sequence, Kind::String],
                    found: other.kind(),
                    span: value.span().into(),
                }),
            }
        };

    if let Value::Sequence(sequence) = outputs.as_ref() {
        let interfaces = sequence
            .iter()
            .map(&parse_json_output)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(interfaces)
    } else {
        let output = parse_json_output(outputs)?;
        Ok(vec![output])
    }
}

/// Parse the `inputs`/`translations` configuration for a single config.
///
/// # Errors
///
/// Returns an error if the inputs section is present but not a sequence, or if
/// any input entry has an unexpected structure.
pub fn parse_inputs<F: Copy + PartialEq>(
    value: &yaml_spanned::Spanned<Value>,
    config_span: Option<yaml_spanned::spanned::Span>,
    file_id: F,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<F>>,
) -> Result<Vec<Input>, ConfigError> {
    let Some(inputs) = value.get("inputs").or(value.get("translations")) else {
        let diagnostic = Diagnostic::warning_or_error(strict)
            .with_message("empty inputs")
            .with_labels(vec![
                Label::primary(file_id, config_span.unwrap_or(value.span))
                    .with_message("no inputs specified - nothing will be generated"),
            ]);
        diagnostics.push(diagnostic);
        return Ok(vec![]);
    };
    let inputs = inputs
        .as_sequence()
        .ok_or_else(|| ConfigError::UnexpectedType {
            message: "inputs must be a sequence".to_string(),
            found: inputs.kind(),
            expected: vec![Kind::Sequence],
            span: inputs.span().into(),
        })?;
    let inputs = inputs
        .iter()
        .filter_map(|input| parse_input(input, file_id, strict, diagnostics).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(inputs)
}

/// Parse the `outputs` configuration for a single config.
///
/// # Errors
///
/// Returns an error if the `outputs` field is present but not a mapping, or if
/// any output sub-configuration has an unexpected shape.
pub fn parse_outputs<F: Copy + PartialEq>(
    value: &yaml_spanned::Spanned<Value>,
    config_span: Option<yaml_spanned::spanned::Span>,
    file_id: F,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<F>>,
) -> Result<Outputs, ConfigError> {
    let Some(outputs) = value.get("outputs") else {
        let diagnostic = Diagnostic::warning_or_error(strict)
            .with_message("empty outputs")
            .with_labels(vec![
                Label::primary(file_id, config_span.unwrap_or(value.span))
                    .with_message("no outputs specified - nothing will be generated"),
            ]);
        diagnostics.push(diagnostic);
        return Ok(Outputs::default());
    };
    let (_span, outputs) = expect_mapping(outputs)?;

    Ok(Outputs {
        json: parse_json_outputs(outputs)?,
        #[cfg(feature = "typescript")]
        typescript: parse_typescript_outputs(outputs)?,
        #[cfg(feature = "rust")]
        rust: parse_rust_outputs(outputs)?,
        #[cfg(feature = "golang")]
        golang: None,
        #[cfg(feature = "python")]
        python: None,
    })
}

/// Parse a single configuration entry.
///
/// # Errors
///
/// Returns an error if required fields are missing, if fields have unexpected
/// types, or if nested `inputs`/`outputs` parsing fails.
pub fn parse_config<F: Copy + PartialEq>(
    name: Spanned<String>,
    config_span: Option<yaml_spanned::spanned::Span>,
    value: &yaml_spanned::Spanned<Value>,
    file_id: F,
    strict_override: Option<bool>,
    diagnostics: &mut Vec<Diagnostic<F>>,
) -> Result<Config, ConfigError> {
    let strict_config = parse_optional::<bool>(value.get("strict"))?.map(Spanned::into_inner);
    let strict = strict_override.unwrap_or(false);
    let languages = parse_languages(value, file_id, strict, diagnostics)?;
    let template_engine = parse_optional::<model::TemplateEngine>(
        value.get("engine").or_else(|| value.get("template_engine")),
    )?;
    let check_templates =
        parse_optional::<bool>(value.get("check_templates"))?.map(Spanned::into_inner);
    let dry_run = parse_optional::<bool>(value.get("dry_run"))?.map(Spanned::into_inner);
    let print_absolute_paths = parse_optional::<bool>(
        value
            .get("print_absolute_paths")
            .or_else(|| value.get("absolute")),
    )?
    .map(Spanned::into_inner);
    let inputs = parse_inputs(value, config_span, file_id, strict, diagnostics)?;
    let outputs = parse_outputs(value, config_span, file_id, strict, diagnostics)?;

    Ok(Config {
        name,
        languages,
        settings: SettingsLayer {
            strict: strict_config,
            check_templates,
            dry_run,
            print_absolute_paths,
            template_engine,
        },
        inputs,
        outputs,
    })
}

/// Parse the top-level configuration structure into a list of configs.
///
/// # Errors
///
/// Returns an error if the `config`/`configs` section has an unexpected type or
/// if any contained configuration cannot be parsed.
pub fn parse_configs<F: Copy + PartialEq>(
    value: &yaml_spanned::Spanned<Value>,
    config_dir: &Path,
    file_id: F,
    strict: Option<bool>,
    diagnostics: &mut Vec<Diagnostic<F>>,
) -> Result<Configs<F>, ConfigError> {
    if let Some(config) = value.get("config") {
        // single config
        let config = parse_config(
            Spanned::dummy("config".to_string()),
            None,
            config,
            file_id,
            strict,
            diagnostics,
        )?;
        return Ok(vec![ConfigFile {
            file_id: Some(file_id),
            config_dir: Some(config_dir.to_path_buf()),
            config,
        }]);
    }

    let Some(configs) = value.get("configs") else {
        let _diagnostic = Diagnostic::warning_or_error(strict.unwrap_or(false))
            .with_message("empty configurations")
            .with_labels(vec![Label::primary(file_id, value.span).with_message(
                "no configurations specified - no output will be generated",
            )]);
        return Ok(Configs::default());
    };

    // parse each config
    match configs.as_ref() {
        Value::Sequence(seq) => seq
            .iter()
            .enumerate()
            .map(|(idx, value)| {
                let name = format!("configs[{idx}]");
                let config = parse_config(
                    Spanned::dummy(name),
                    Some(value.span),
                    value,
                    file_id,
                    strict,
                    diagnostics,
                )?;
                Ok(ConfigFile {
                    file_id: Some(file_id),
                    config_dir: Some(config_dir.to_path_buf()),
                    config,
                })
            })
            .collect::<Result<Configs<F>, _>>(),
        Value::Mapping(mapping) => mapping
            .iter()
            .map(|(name_value, value)| {
                let name = Spanned::new(
                    name_value.span,
                    name_value.as_str().unwrap_or_default().to_string(),
                );
                let config = parse_config(
                    name,
                    Some(name_value.span),
                    value,
                    file_id,
                    strict,
                    diagnostics,
                )?;
                Ok(ConfigFile {
                    file_id: Some(file_id),
                    config_dir: Some(config_dir.to_path_buf()),
                    config,
                })
            })
            .collect::<Result<Configs<F>, _>>(),
        other => Err(ConfigError::UnexpectedType {
            message: "configurations must either be a sequence or a named mapping".to_string(),
            expected: vec![
                yaml_spanned::value::Kind::Mapping,
                yaml_spanned::value::Kind::Sequence,
            ],
            found: other.kind(),
            span: configs.span().into(),
        }),
    }
}

/// A file path or glob pattern, stored as a string.
pub type PathOrGlobPattern = String;

/// A single translation input source.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Input {
    /// The path or glob pattern selecting the input file(s).
    pub path_or_glob_pattern: Spanned<PathOrGlobPattern>,
    /// Paths or glob patterns to exclude from the matched inputs.
    pub exclude: Vec<Spanned<PathOrGlobPattern>>,
    /// An optional prefix prepended to every key from this input.
    pub prefix: Option<Spanned<String>>,
    /// Whether to prefix keys with the input file's stem.
    pub prepend_filename: Option<Spanned<bool>>,
    /// Whether to prefix keys with the input file's relative path segments.
    pub prepend_relative_path: Option<Spanned<bool>>,
    /// The separator used when joining prefix segments with keys.
    pub separator: Option<Spanned<String>>,
}

impl Input {
    /// Create a new input from a path or glob pattern.
    pub fn new(path_or_glob_pattern: impl Into<PathOrGlobPattern>) -> Self {
        Self {
            path_or_glob_pattern: Spanned::dummy(path_or_glob_pattern.into()),
            exclude: vec![],
            prefix: None,
            prepend_filename: None,
            prepend_relative_path: None,
            separator: None,
        }
    }

    /// Set the patterns to exclude from the matched inputs.
    #[must_use]
    pub fn with_exclude(mut self, exclude: impl IntoIterator<Item = PathOrGlobPattern>) -> Self {
        self.exclude = exclude.into_iter().map(Spanned::dummy).collect();
        self
    }

    /// Set the prefix prepended to every key from this input.
    #[must_use]
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(Spanned::dummy(prefix.into()));
        self
    }

    /// Set whether keys are prefixed with the input file's stem.
    #[must_use]
    pub fn with_prepend_filename(mut self, prepend_filename: bool) -> Self {
        self.prepend_filename = Some(Spanned::dummy(prepend_filename));
        self
    }

    /// Set whether keys are prefixed with the input file's relative path.
    #[must_use]
    pub fn with_prepend_relative_path(mut self, prepend_relative_path: bool) -> Self {
        self.prepend_relative_path = Some(Spanned::dummy(prepend_relative_path));
        self
    }

    /// Set the separator used when joining prefix segments with keys.
    #[must_use]
    pub fn with_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator = Some(Spanned::dummy(separator.into()));
        self
    }
}

impl std::fmt::Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input")
            .field("path_or_glob_pattern", &self.path_or_glob_pattern.display())
            .field("prefix", &self.prefix.as_ref().map(Spanned::display))
            .field(
                "prepend_filename",
                &self.prepend_filename.as_ref().map(Spanned::display),
            )
            .field(
                "prepend_relative_path",
                &self.prepend_relative_path.as_ref().map(Spanned::display),
            )
            .field("separator", &self.separator.as_ref().map(Spanned::display))
            .field(
                "exclude",
                &self
                    .exclude
                    .iter()
                    .map(Spanned::display)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// The layout style used when writing a JSON translation file.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default,
)]
pub enum JsonOutputStyle {
    /// A flat object mapping fully-qualified keys to translations.
    #[default]
    Flat,
}

/// Configuration for a single JSON translation output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsonOutputConfig {
    /// The output path template for the generated JSON file.
    pub path: Spanned<PathBuf>,
    /// The layout style of the generated JSON.
    pub style: Option<Spanned<JsonOutputStyle>>,
}

impl JsonOutputConfig {
    /// Create a new JSON output config writing to the given path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Spanned::dummy(path.into()),
            style: None,
        }
    }

    /// Set the layout style of the generated JSON.
    #[must_use]
    pub fn with_style(mut self, style: impl Into<JsonOutputStyle>) -> Self {
        self.style = Some(Spanned::dummy(style.into()));
        self
    }
}

/// The set of outputs to generate for a single configuration.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Outputs {
    /// JSON translation outputs.
    pub json: Vec<JsonOutputConfig>,

    /// TypeScript output configuration.
    #[cfg(feature = "typescript")]
    pub typescript: Option<globetrotter_typescript::OutputConfig>,

    /// Rust output configuration.
    #[cfg(feature = "rust")]
    pub rust: Option<globetrotter_rust::OutputConfig>,

    /// Go output configuration.
    #[cfg(feature = "golang")]
    pub golang: Option<globetrotter_golang::OutputConfig>,

    /// Python output configuration.
    #[cfg(feature = "python")]
    pub python: Option<globetrotter_python::OutputConfig>,
}

impl Outputs {
    /// Create an empty set of outputs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the JSON outputs.
    #[must_use]
    pub fn with_json(mut self, json: impl IntoIterator<Item = JsonOutputConfig>) -> Self {
        self.json = json.into_iter().collect();
        self
    }

    /// Set the TypeScript output configuration.
    #[cfg(feature = "typescript")]
    #[must_use]
    pub fn with_typescript(
        mut self,
        typescript: impl Into<globetrotter_typescript::OutputConfig>,
    ) -> Self {
        self.typescript = Some(typescript.into());
        self
    }

    /// Set the Rust output configuration.
    #[cfg(feature = "rust")]
    #[must_use]
    pub fn with_rust(mut self, rust: impl Into<globetrotter_rust::OutputConfig>) -> Self {
        self.rust = Some(rust.into());
        self
    }

    /// Set the Go output configuration.
    #[cfg(feature = "golang")]
    #[must_use]
    pub fn with_golang(mut self, golang: impl Into<globetrotter_golang::OutputConfig>) -> Self {
        self.golang = Some(golang.into());
        self
    }

    /// Set the Python output configuration.
    #[cfg(feature = "python")]
    #[must_use]
    pub fn with_python(mut self, python: impl Into<globetrotter_python::OutputConfig>) -> Self {
        self.python = Some(python.into());
        self
    }
}

impl std::fmt::Display for Outputs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO(roman): all the target configs should implement display

        let mut s = f.debug_struct("Outputs");
        s.field("json", &self.json);
        #[cfg(feature = "typescript")]
        s.field("typescript", &self.typescript);
        #[cfg(feature = "rust")]
        s.field("rust", &self.rust);
        #[cfg(feature = "golang")]
        s.field("golang", &self.golang);
        #[cfg(feature = "python")]
        s.field("python", &self.python);

        s.finish()
    }
}

impl Outputs {
    /// Returns `true` if no outputs are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        if !self.json.is_empty() {
            return false;
        }

        #[cfg(feature = "typescript")]
        if self.typescript.as_ref().is_some_and(|c| !c.is_empty()) {
            return false;
        }

        #[cfg(feature = "rust")]
        if self.rust.as_ref().is_some_and(|c| !c.is_empty()) {
            return false;
        }

        #[cfg(feature = "golang")]
        if self.golang.as_ref().is_some_and(|c| !c.is_empty()) {
            return false;
        }

        #[cfg(feature = "python")]
        if self.python.as_ref().is_some_and(|c| !c.is_empty()) {
            return false;
        }

        true
    }
}

/// A single named translation configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Config {
    /// The name of the configuration.
    pub name: Spanned<String>,
    /// The languages that must be present in the translations.
    pub languages: Vec<Spanned<model::Language>>,
    /// This config's settings layer.
    ///
    /// These are raw, unresolved values: caller overrides and built-in
    /// defaults are merged in by
    /// [`Settings::resolve`](super::settings::Settings::resolve). Read settled
    /// values from the resolved [`Settings`](super::settings::Settings), never
    /// from here.
    pub settings: SettingsLayer,

    /// The translation input sources.
    pub inputs: Vec<Input>,
    /// The outputs to generate.
    pub outputs: Outputs,
}

impl Config {
    /// Create a new configuration with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Spanned::dummy(name.into()),
            languages: vec![],
            settings: SettingsLayer::default(),
            inputs: vec![],
            outputs: Outputs::default(),
        }
    }

    /// Add a required language.
    #[must_use]
    pub fn with_language(mut self, language: impl Into<model::Language>) -> Self {
        self.languages.push(Spanned::dummy(language.into()));
        self
    }

    /// Add multiple required languages.
    #[must_use]
    pub fn with_languages(mut self, languages: impl IntoIterator<Item = model::Language>) -> Self {
        self.languages
            .extend(languages.into_iter().map(Spanned::dummy));
        self
    }

    /// Set whether templates are validated.
    #[must_use]
    pub fn with_check_templates(mut self, check_templates: bool) -> Self {
        self.settings.check_templates = Some(check_templates);
        self
    }

    /// Set whether warnings are promoted to errors.
    #[must_use]
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.settings.strict = Some(strict);
        self
    }

    /// Set whether outputs are computed but not written to disk.
    #[must_use]
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.settings.dry_run = Some(dry_run);
        self
    }

    /// Set whether output paths are logged as absolute paths.
    #[must_use]
    pub fn with_print_absolute_paths(mut self, print_absolute_paths: bool) -> Self {
        self.settings.print_absolute_paths = Some(print_absolute_paths);
        self
    }

    /// Set the template engine.
    #[must_use]
    pub fn with_template_engine(
        mut self,
        template_engine: impl Into<model::TemplateEngine>,
    ) -> Self {
        self.settings.template_engine = Some(Spanned::dummy(template_engine.into()));
        self
    }

    /// Add a single input source.
    #[must_use]
    pub fn with_input(mut self, input: impl Into<Input>) -> Self {
        self.inputs.push(input.into());
        self
    }

    /// Add multiple input sources.
    #[must_use]
    pub fn with_inputs(mut self, inputs: impl IntoIterator<Item = Input>) -> Self {
        self.inputs.extend(inputs);
        self
    }

    /// Set the outputs to generate.
    #[must_use]
    pub fn with_outputs(mut self, outputs: impl Into<Outputs>) -> Self {
        self.outputs = outputs.into();
        self
    }
}

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("name", &self.name.display())
            .field(
                "languages",
                &self
                    .languages
                    .iter()
                    .map(Spanned::display)
                    .collect::<Vec<_>>(),
            )
            .field(
                "template_engine",
                &self.settings.template_engine.as_ref().map(Spanned::display),
            )
            .field("check_templates", &self.settings.check_templates)
            .field("strict", &self.settings.strict)
            .field("dry_run", &self.settings.dry_run)
            .field("print_absolute_paths", &self.settings.print_absolute_paths)
            .field(
                "inputs",
                &self.inputs.iter().map(DisplayRepr).collect::<Vec<_>>(),
            )
            .field("outputs", &DisplayRepr(&self.outputs))
            .finish()
    }
}

impl Config {
    /// Returns `true` if the configuration has no inputs or no outputs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty() || self.outputs.is_empty()
    }
}
