use crate::{
    ArgumentType, Language, Translation,
    diagnostics::{DiagnosticExt, FileId, Span, Spanned},
};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use indexmap::IndexMap;

/// Errors that can occur while parsing translations from TOML.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// A value had a type other than the one expected.
    #[error("{message}")]
    UnexpectedType {
        /// Human-readable description of the mismatch.
        message: String,
        /// The value kinds that would have been accepted.
        expected: Vec<ValueKind>,
        /// The value kind that was actually found.
        found: ValueKind,
        /// The source span of the offending value.
        span: Span,
    },
    /// A language key was referenced but not present in the table.
    #[error("missing language key {language}")]
    MissingLanguageKey {
        /// The missing language key.
        language: String,
    },
    /// A not-yet-handled TOML structure was encountered.
    #[error("{message}")]
    TODO {
        /// Human-readable description of the unhandled case.
        message: String,
        // expected: Vec<ValueKind>,
        // found: ValueKind,
        /// The source span of the offending value.
        span: Span,
    },
    /// Deserializing a value via serde failed.
    #[error("{source}")]
    Serde {
        /// The underlying serde error.
        #[source]
        source: serde_json::Error,
        /// The source span of the offending value.
        span: Span,
    },
    /// Parsing the raw TOML document failed.
    #[error("{source}")]
    TOML {
        /// The underlying TOML parse error.
        #[source]
        source: toml_span::Error,
    },
}

mod diagnostics {
    use crate::diagnostics::ToDiagnostics;
    use codespan_reporting::diagnostic::{Diagnostic, Label};

    impl ToDiagnostics for super::Error {
        fn to_diagnostics<F: Copy + PartialEq>(&self, file_id: F) -> Vec<Diagnostic<F>> {
            match self {
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
                Self::MissingLanguageKey { language } => {
                    let diagnostic = Diagnostic::error()
                        .with_message(self.to_string())
                        .with_notes(vec![format!(
                            "language key `{language}` was referenced but not found in table"
                        )]);
                    vec![diagnostic]
                }
                Self::TODO {
                    // expected,
                    // found,
                    span,
                    ..
                } => {
                    let diagnostic = Diagnostic::error()
                        .with_message(self.to_string())
                        .with_labels(vec![
                            Label::primary(file_id, span.clone()).with_message("?".to_string()),
                        ]);
                    vec![diagnostic]
                }
                Self::Serde { source, span } => {
                    let diagnostic = Diagnostic::error()
                        .with_message(self.to_string())
                        .with_labels(vec![
                            Label::primary(file_id, span.clone()).with_message(source.to_string()),
                        ]);
                    vec![diagnostic]
                }
                Self::TOML { source } => {
                    vec![source.to_diagnostic(file_id)]
                }
            }
        }
    }
}

/// The kind of a TOML value, used to describe type mismatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueKind {
    /// A string value.
    String,
    /// An integer value.
    Integer,
    /// A floating-point value.
    Float,
    /// A boolean value.
    Boolean,
    /// An array value.
    Array,
    /// A table value.
    Table,
}

impl<'de> From<&toml_span::Value<'de>> for ValueKind {
    fn from(value: &toml_span::Value<'de>) -> Self {
        value.as_ref().into()
    }
}

impl<'de> From<&toml_span::value::ValueInner<'de>> for ValueKind {
    fn from(value: &toml_span::value::ValueInner<'de>) -> Self {
        use toml_span::value::ValueInner;
        match value {
            ValueInner::String(..) => ValueKind::String,
            ValueInner::Integer(..) => ValueKind::Integer,
            ValueInner::Float(..) => ValueKind::Float,
            ValueInner::Boolean(..) => ValueKind::Boolean,
            ValueInner::Array(..) => ValueKind::Array,
            ValueInner::Table(..) => ValueKind::Table,
        }
    }
}

/// Parse the optional `allow` key listing lint codes to suppress for a key.
///
/// # Errors
///
/// Returns an error if `allow` is present but is not a string or an array of
/// strings.
fn parse_allow(
    table: &mut toml_span::value::Table,
) -> Result<std::collections::BTreeSet<String>, Error> {
    let Some(value) = table.remove("allow") else {
        return Ok(std::collections::BTreeSet::new());
    };
    match value.as_ref() {
        toml_span::value::ValueInner::String(code) => Ok([code.to_string()].into_iter().collect()),
        toml_span::value::ValueInner::Array(codes) => codes
            .iter()
            .map(|code| {
                code.as_str()
                    .map(std::string::ToString::to_string)
                    .ok_or_else(|| Error::UnexpectedType {
                        message: "allow entries must be strings".to_string(),
                        expected: vec![ValueKind::String],
                        found: code.into(),
                        span: code.span.into(),
                    })
            })
            .collect(),
        _other => Err(Error::UnexpectedType {
            message: "allow must be a string or an array of strings".to_string(),
            expected: vec![ValueKind::String, ValueKind::Array],
            found: value.as_ref().into(),
            span: value.span.into(),
        }),
    }
}

/// Parse a single translation table from a TOML value.
///
/// # Errors
///
/// Returns an error if the TOML structure does not match the expected
/// translation layout (for example, if argument or language values have
/// an unexpected type).
pub fn parse_translation(
    table: &mut toml_span::value::Table,
    file_id: FileId,
) -> Result<Option<crate::Translation>, Error> {
    let arguments = table.remove("arguments").or(table.remove("args"));
    let arguments = arguments
        .map(|arguments| match arguments.as_ref() {
            toml_span::value::ValueInner::Array(array) => array
                .iter()
                .map(|name_value| {
                    let name = name_value
                        .as_str()
                        .map(std::string::ToString::to_string)
                        .ok_or_else(|| Error::UnexpectedType {
                            message: "argument name must be a string".to_string(),
                            expected: vec![ValueKind::String],
                            found: name_value.into(),
                            span: name_value.span.into(),
                        })?;
                    Ok((name, ArgumentType::Any))
                })
                .collect::<Result<IndexMap<_, _>, _>>(),
            toml_span::value::ValueInner::Table(table) => table
                .iter()
                .map(|(name_value, typ_value)| {
                    let name = name_value.name.to_string();
                    let typ = typ_value.as_str().ok_or_else(|| Error::UnexpectedType {
                        message: "argument type must be a string".to_string(),
                        expected: vec![ValueKind::String],
                        found: typ_value.as_ref().into(),
                        span: typ_value.span.into(),
                    })?;
                    let typ: ArgumentType =
                        serde_json::from_value(serde_json::Value::String(typ.to_string()))
                            .map_err(|source| Error::Serde {
                                source,
                                span: typ_value.span.into(),
                            })?;
                    Ok((name, typ))
                })
                .collect::<Result<IndexMap<_, _>, _>>(),
            _other => Err(Error::UnexpectedType {
                message: "arguments must be a array or table".to_string(),
                expected: vec![ValueKind::Array, ValueKind::Table],
                found: arguments.as_ref().into(),
                span: arguments.span.into(),
            }),
        })
        .transpose()?;

    // removed before language parsing so it is not mistaken for a language entry.
    let allow = parse_allow(table)?;

    let languages: Vec<String> = table
        .iter()
        .filter_map(|(language_value, translation_value)| {
            let terminal = translation_value.as_str().is_some();
            if terminal {
                Some(language_value.name.to_string())
            } else {
                None
            }
        })
        .collect();

    let language = languages
        .into_iter()
        .map(|language| {
            // // skip non-terminal values
            let (language_value, translation_value) =
                table
                    .remove_entry(language.as_str())
                    .ok_or(Error::MissingLanguageKey {
                        language: language.clone(),
                    })?;

            let translation = translation_value
                .as_str()
                .map(std::string::ToString::to_string)
                .ok_or_else(|| Error::UnexpectedType {
                    message: "translation must be a string".to_string(),
                    expected: vec![ValueKind::String],
                    found: translation_value.as_ref().into(),
                    span: translation_value.span.into(),
                })?;

            let lang_json_value = serde_json::Value::String(language_value.name.to_string());
            let language: Language =
                serde_json::from_value(lang_json_value).map_err(|source| Error::Serde {
                    source,
                    span: language_value.span.into(),
                })?;

            Ok((language, Spanned::new(translation_value.span, translation)))
        })
        .collect::<Result<IndexMap<_, _>, Error>>()?;

    if arguments.is_none() && language.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Translation {
            language,
            arguments: arguments.unwrap_or_default(),
            file_id,
            allow,
        }))
    }
}

fn flatten_toml_span(
    value: &mut toml_span::value::ValueInner,
    span: toml_span::Span,
    key: &str,
    out: &mut super::Translations,
    file_id: usize,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> Result<(), Error> {
    match value {
        toml_span::value::ValueInner::Table(table) => {
            // treat as terminal
            if let Some(translation) = parse_translation(table, file_id)? {
                out.0
                    .insert(Spanned::new(span, key.to_owned()), translation);
            }

            // let table_tmp: Vec<(String, String)> = {
            //     table
            //         .iter()
            //         .map(|(k, v)| (k.name.to_string(), format!("{:?}", v.as_ref())))
            //         .collect()
            // };

            // treat as non-terminal
            for (child_key, value) in table.iter_mut() {
                let new_key: String = if key.is_empty() {
                    child_key.to_string()
                } else {
                    format!("{key}.{child_key}")
                };

                match value.take() {
                    toml_span::value::ValueInner::Array(mut tables) => {
                        for nested_table in &mut tables {
                            flatten_toml_span(
                                &mut nested_table.take(),
                                nested_table.span,
                                &new_key,
                                out,
                                file_id,
                                strict,
                                diagnostics,
                            )?;
                        }
                    }
                    mut nested_table @ toml_span::value::ValueInner::Table(_) => {
                        flatten_toml_span(
                            &mut nested_table,
                            value.span,
                            &new_key,
                            out,
                            file_id,
                            strict,
                            diagnostics,
                        )?;
                    }
                    other => {
                        return Err(Error::TODO {
                            message: format!("extra stuff {other:?}"),
                            span: value.span.into(),
                        });
                    }
                }
            }
        }
        other => {
            let diagnostic = Diagnostic::warning_or_error(strict)
                .with_message("unexpected value")
                .with_labels(vec![Label::primary(file_id, span).with_message(format!(
                    "ignoring {} value at key {key:?}",
                    other.type_str()
                ))]);
            diagnostics.push(diagnostic);
        }
    }

    Ok(())
}

impl crate::Translations {
    /// Construct translations from a parsed TOML value.
    ///
    /// # Errors
    ///
    /// Returns an error if the TOML data contains values of an unexpected
    /// type or otherwise cannot be converted into translations.
    pub fn from_value(
        mut value: toml_span::Value,
        file_id: FileId,
        strict: bool,
        diagnostics: &mut Vec<Diagnostic<FileId>>,
    ) -> Result<Self, Error> {
        let mut translations = Self::default();
        flatten_toml_span(
            &mut value.take(),
            value.span,
            "",
            &mut translations,
            file_id,
            strict,
            diagnostics,
        )?;
        Ok(translations)
    }

    /// Parse translations from a raw TOML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not valid TOML or if any values
    /// cannot be converted into translations.
    pub fn from_str(
        raw_translations: &str,
        file_id: FileId,
        strict: bool,
        diagnostics: &mut Vec<Diagnostic<FileId>>,
    ) -> Result<crate::Translations, Error> {
        let translations =
            toml_span::parse(raw_translations).map_err(|source| Error::TOML { source })?;
        Self::from_value(translations, file_id, strict, diagnostics)
    }
}
