use crate::{
    diagnostics::{DiagnosticExt, FileId, Span, Spanned},
    ArgumentType, Language, Translation,
};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use indexmap::IndexMap;
use std::path::{Path, PathBuf};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{message}")]
    UnexpectedType {
        message: String,
        expected: Vec<ValueKind>,
        found: ValueKind,
        span: Span,
    },
    #[error("{message}")]
    TODO {
        message: String,
        // expected: Vec<ValueKind>,
        // found: ValueKind,
        span: Span,
    },
    #[error("{source}")]
    Serde {
        #[source]
        source: serde_json::Error,
        span: Span,
    },
    #[error("{source}")]
    TOML {
        #[source]
        source: toml_span::Error,
    },
}

mod diagnostics {
    use crate::diagnostics::ToDiagnostics;
    use codespan_reporting::diagnostic::{self, Diagnostic, Label};

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
                        .with_labels(vec![Label::primary(file_id, span.clone())
                            .with_message(format!("expected {expected}"))])
                        .with_notes(vec![unindent::unindent(&format!(
                            "
                        expected type {expected}
                           found type `{found:?}`
                        "
                        ))]);
                    vec![diagnostic]
                }
                Self::TODO {
                    message,
                    // expected,
                    // found,
                    span,
                    ..
                } => {
                    let diagnostic = Diagnostic::error()
                        .with_message(self.to_string())
                        .with_labels(vec![
                            Label::primary(file_id, span.clone()).with_message("?".to_string())
                        ]);
                    vec![diagnostic]
                }
                Self::Serde { source, span } => {
                    let diagnostic = Diagnostic::error()
                        .with_message(self.to_string())
                        .with_labels(vec![
                            Label::primary(file_id, span.clone()).with_message(source.to_string())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueKind {
    String,
    Integer,
    Float,
    Boolean,
    Array,
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

pub fn parse_translation(
    table: &mut toml_span::value::Table,
    file_id: FileId,
) -> Result<Option<crate::Translation>, Error> {
    let arguments = table.remove("arguments").or(table.remove("args"));
    let arguments = arguments
        .map(|arguments| match arguments.as_ref() {
            toml_span::value::ValueInner::Array(array) => array
                .into_iter()
                .map(|name_value| {
                    let name = name_value
                        .as_str()
                        .map(|name| name.to_string())
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
                .into_iter()
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
            other => Err(Error::UnexpectedType {
                message: "arguments must be a array or table".to_string(),
                expected: vec![ValueKind::Array, ValueKind::Table],
                found: arguments.as_ref().into(),
                span: arguments.span.into(),
            }),
        })
        .transpose()?;

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
                table.remove_entry(language.as_str()).expect("remove key");

            let translation = translation_value
                .as_str()
                .map(|translation| translation.to_string())
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
        }))
    }
}

fn flatten_toml_span<'doc>(
    value: &'doc mut toml_span::value::ValueInner,
    span: toml_span::Span,
    key: String,
    out: &mut super::Translations,
    file_id: usize,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> Result<(), Error> {
    match value {
        toml_span::value::ValueInner::Table(table) => {
            // treat as terminal
            if let Some(translation) = parse_translation(table, file_id)? {
                out.0.insert(Spanned::new(span, key.clone()), translation);
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
                    format!("{}.{}", key, child_key)
                };

                match value.take() {
                    toml_span::value::ValueInner::Array(mut tables) => {
                        for nested_table in tables.iter_mut() {
                            flatten_toml_span(
                                &mut nested_table.take(),
                                nested_table.span,
                                new_key.clone(),
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
                            new_key.clone(),
                            out,
                            file_id,
                            strict,
                            diagnostics,
                        )?;
                    }
                    other => {
                        return Err(Error::TODO {
                            message: format!("extra stuff {:?}", other),
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
    };

    Ok(())
}

impl crate::Translations {
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
            "".to_string(),
            &mut translations,
            file_id,
            strict,
            diagnostics,
        )?;
        Ok(translations)
    }

    pub fn from_str(
        raw_translations: &str,
        file_id: FileId,
        strict: bool,
        diagnostics: &mut Vec<Diagnostic<FileId>>,
    ) -> Result<crate::Translations, Error> {
        let translations =
            toml_span::parse(&raw_translations).map_err(|source| Error::TOML { source })?;
        Self::from_value(translations, file_id, strict, diagnostics)
    }
}
