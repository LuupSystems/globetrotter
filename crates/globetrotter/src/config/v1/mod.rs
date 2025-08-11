use super::ConfigError;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use globetrotter_model::{
    self as model,
    diagnostics::{self, DiagnosticExt, DisplayRepr, Span, Spanned},
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use yaml_spanned::{Mapping, Sequence, Value, value::Kind};

#[derive(Debug)]
pub struct ConfigFile<F> {
    pub file_id: Option<F>,
    pub config_dir: Option<PathBuf>,
    pub config: Config,
}

pub type Configs<F> = Vec<ConfigFile<F>>;

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

pub fn parse<T: serde::de::DeserializeOwned>(
    value: &yaml_spanned::Spanned<Value>,
) -> Result<Spanned<T>, ConfigError> {
    let inner: T = yaml_spanned::from_value(value).map_err(|source| ConfigError::Serde {
        source,
        span: value.span().into(),
    })?;
    Ok(Spanned::new(value.span, inner))
}

pub fn parse_optional<T: serde::de::DeserializeOwned>(
    value: Option<&yaml_spanned::Spanned<Value>>,
) -> Result<Option<Spanned<T>>, ConfigError> {
    value.map(|value| parse(value)).transpose()
}

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
            path_or_glob_pattern: Spanned::new(value.span, path.to_string()),
            exclude: Vec::new(),
            prefix: None,
            prepend_filename: None,
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
                }) => Ok(vec![Spanned::new(*span, path_or_glob_pattern.to_string())]),
                Some(yaml_spanned::Spanned {
                    inner: Value::Sequence(sequence),
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
            let separator = parse_optional::<String>(mapping.get("separator"))?;
            Ok(Some(Input {
                path_or_glob_pattern,
                exclude,
                prefix,
                prepend_filename,
                separator,
            }))
        }
        other => Err(ConfigError::UnexpectedType {
            message: "input must be a path or a mapping".to_string(),
            found: value.kind(),
            expected: vec![Kind::Mapping, Kind::String],
            span: value.span().into(),
        }),
    }
}

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
pub fn parse_typescript_outputs(
    value: &Mapping,
) -> Result<Option<globetrotter_typescript::OutputConfig>, ConfigError> {
    use globetrotter_typescript::config::InterfaceTypeOutputConfig;

    let Some(outputs) = value.get("typescript").or_else(|| value.get("ts")) else {
        return Ok(None);
    };
    let (span, outputs) = expect_mapping(outputs)?;

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
    let (span, outputs) = expect_mapping(outputs)?;

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
    let inputs = parse_inputs(value, config_span, file_id, strict, diagnostics)?;
    let outputs = parse_outputs(value, config_span, file_id, strict, diagnostics)?;

    Ok(Config {
        name,
        languages,
        template_engine,
        check_templates,
        strict: strict_config,
        inputs,
        outputs,
    })
}

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
        let diagnostic = Diagnostic::warning_or_error(strict.unwrap_or(false))
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

pub type PathOrGlobPattern = String;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Input {
    pub path_or_glob_pattern: Spanned<PathOrGlobPattern>,
    pub exclude: Vec<Spanned<PathOrGlobPattern>>,
    pub prefix: Option<Spanned<String>>,
    pub prepend_filename: Option<Spanned<bool>>,
    pub separator: Option<Spanned<String>>,
}

impl Input {
    pub fn new(path_or_glob_pattern: impl Into<PathOrGlobPattern>) -> Self {
        Self {
            path_or_glob_pattern: Spanned::dummy(path_or_glob_pattern.into()),
            exclude: vec![],
            prefix: None,
            prepend_filename: None,
            separator: None,
        }
    }

    pub fn with_exclude(mut self, exclude: impl IntoIterator<Item = PathOrGlobPattern>) -> Self {
        self.exclude = exclude.into_iter().map(Spanned::dummy).collect();
        self
    }

    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(Spanned::dummy(prefix.into()));
        self
    }

    pub fn with_prepend_filename(mut self, prepend_filename: bool) -> Self {
        self.prepend_filename = Some(Spanned::dummy(prepend_filename));
        self
    }

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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default,
)]
pub enum JsonOutputStyle {
    #[default]
    Flat,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct JsonOutputConfig {
    pub path: Spanned<PathBuf>,
    pub style: Option<Spanned<JsonOutputStyle>>,
}

impl JsonOutputConfig {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Spanned::dummy(path.into()),
            style: None,
        }
    }

    pub fn with_style(mut self, style: impl Into<JsonOutputStyle>) -> Self {
        self.style = Some(Spanned::dummy(style.into()));
        self
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Outputs {
    #[cfg_attr(feature = "serde", serde(alias = "translations"))]
    pub json: Vec<JsonOutputConfig>,

    #[cfg(feature = "typescript")]
    #[cfg_attr(feature = "serde", serde(alias = "ts"))]
    pub typescript: Option<globetrotter_typescript::OutputConfig>,

    #[cfg(feature = "rust")]
    pub rust: Option<globetrotter_rust::OutputConfig>,

    #[cfg(feature = "golang")]
    #[cfg_attr(feature = "serde", serde(alias = "go"))]
    pub golang: Option<globetrotter_golang::OutputConfig>,

    #[cfg(feature = "python")]
    pub python: Option<globetrotter_python::OutputConfig>,
}

impl Outputs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_json(mut self, json: impl IntoIterator<Item = JsonOutputConfig>) -> Self {
        self.json = json.into_iter().collect();
        self
    }

    #[cfg(feature = "typescript")]
    pub fn with_typescript(
        mut self,
        typescript: impl Into<globetrotter_typescript::OutputConfig>,
    ) -> Self {
        self.typescript = Some(typescript.into());
        self
    }

    #[cfg(feature = "rust")]
    pub fn with_rust(mut self, rust: impl Into<globetrotter_rust::OutputConfig>) -> Self {
        self.rust = Some(rust.into());
        self
    }

    #[cfg(feature = "golang")]
    pub fn with_golang(mut self, golang: impl Into<globetrotter_golang::OutputConfig>) -> Self {
        self.golang = Some(golang.into());
        self
    }

    #[cfg(feature = "python")]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Config {
    pub name: Spanned<String>,
    pub languages: Vec<Spanned<model::Language>>,
    #[cfg_attr(feature = "serde", serde(alias = "template_engine"))]
    pub template_engine: Option<Spanned<model::TemplateEngine>>,
    pub check_templates: Option<bool>,
    pub strict: Option<bool>,

    pub inputs: Vec<Input>,
    pub outputs: Outputs,
}

impl Config {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Spanned::dummy(name.into()),
            languages: vec![],
            template_engine: None,
            check_templates: None,
            strict: None,
            inputs: vec![],
            outputs: Outputs::default(),
        }
    }

    pub fn with_language(mut self, language: impl Into<model::Language>) -> Self {
        self.languages.push(Spanned::dummy(language.into()));
        self
    }

    pub fn with_languages(mut self, languages: impl IntoIterator<Item = model::Language>) -> Self {
        self.languages.extend(
            languages
                .into_iter()
                .map(|language| Spanned::dummy(language)),
        );
        self
    }

    pub fn with_check_templates(mut self, check_templates: bool) -> Self {
        self.check_templates = Some(check_templates);
        self
    }

    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = Some(strict);
        self
    }

    pub fn with_template_engine(
        mut self,
        template_engine: impl Into<model::TemplateEngine>,
    ) -> Self {
        self.template_engine = Some(Spanned::dummy(template_engine.into()));
        self
    }

    pub fn with_input(mut self, input: impl Into<Input>) -> Self {
        self.inputs.push(input.into());
        self
    }

    pub fn with_inputs(mut self, inputs: impl IntoIterator<Item = Input>) -> Self {
        self.inputs.extend(inputs.into_iter());
        self
    }

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
                &self.template_engine.as_ref().map(Spanned::display),
            )
            .field("check_templates", &self.check_templates)
            .field("strict", &self.strict)
            .field(
                "inputs",
                &self.inputs.iter().map(DisplayRepr).collect::<Vec<_>>(),
            )
            .field("outputs", &DisplayRepr(&self.outputs))
            .finish()
    }
}

impl Config {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty() || self.outputs.is_empty()
    }
}
