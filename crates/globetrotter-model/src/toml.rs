//! Parsing of source-located TOML translation files.

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
    /// An `allow` entry was neither a known lint code nor the catch-all `all`.
    #[error("unknown lint code `{code}` in `allow`")]
    UnknownLintCode {
        /// The unrecognized code.
        code: String,
        /// The source span of the offending entry.
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
                Self::UnknownLintCode { span, .. } => {
                    use strum::VariantNames;
                    let valid = crate::lint::LintCode::VARIANTS
                        .iter()
                        .map(|code| format!("`{code}`"))
                        .chain(std::iter::once("`all`".to_string()))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let diagnostic = Diagnostic::error()
                        .with_message(self.to_string())
                        .with_labels(vec![
                            Label::primary(file_id, span.clone())
                                .with_message("not a known lint code"),
                        ])
                        .with_notes(vec![format!("valid codes are: {valid}")]);
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

/// Parses the optional `allow` key listing lint codes to suppress for a key.
///
/// # Errors
///
/// Returns an error if `allow` is present but is not a string or an array of
/// strings, or if any entry is not a known [`LintCode`](crate::lint::LintCode)
/// (or the catch-all `all`).
fn parse_allow(
    table: &mut toml_span::value::Table,
) -> Result<std::collections::BTreeSet<crate::lint::AllowEntry>, Error> {
    let Some(value) = table.remove("allow") else {
        return Ok(std::collections::BTreeSet::new());
    };
    match value.as_ref() {
        toml_span::value::ValueInner::String(code) => {
            Ok([parse_allow_entry(code, value.span.into())?]
                .into_iter()
                .collect())
        }
        toml_span::value::ValueInner::Array(codes) => codes
            .iter()
            .map(|code| {
                let text = code.as_str().ok_or_else(|| Error::UnexpectedType {
                    message: "allow entries must be strings".to_string(),
                    expected: vec![ValueKind::String],
                    found: code.into(),
                    span: code.span.into(),
                })?;
                parse_allow_entry(text, code.span.into())
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

/// Parses one `allow` entry into a typed [`AllowEntry`](crate::lint::AllowEntry),
/// rejecting anything that is neither a known lint code nor `all` so typos fail
/// loudly rather than silently doing nothing.
fn parse_allow_entry(code: &str, span: Span) -> Result<crate::lint::AllowEntry, Error> {
    code.parse::<crate::lint::AllowEntry>()
        .map_err(|_| Error::UnknownLintCode {
            code: code.to_string(),
            span,
        })
}

/// Parses a single translation table from a TOML value.
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

    // Remove `allow` before scanning scalar entries so it cannot be mistaken
    // for a language code.
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
            // Parse translation fields attached directly to this table.
            if let Some(translation) = parse_translation(table, file_id)? {
                out.0
                    .insert(Spanned::new(span, key.to_owned()), translation);
            }

            // Descend into any remaining nested translation tables.
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
                        return Err(Error::UnexpectedType {
                            message: format!(
                                "translation value at `{new_key}` must be a string, table, or array of tables"
                            ),
                            expected: vec![ValueKind::String, ValueKind::Table, ValueKind::Array],
                            found: (&other).into(),
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
    /// Constructs translations from a parsed TOML value.
    ///
    /// Recoverable top-level type mismatches are appended to `diagnostics`.
    /// `strict` controls whether those diagnostics are warnings or errors;
    /// existing diagnostics are retained.
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

    /// Parses translations from a raw TOML string.
    ///
    /// Recoverable structural issues are appended to `diagnostics`. `strict`
    /// controls whether they are warnings or errors; existing diagnostics are
    /// retained.
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

#[cfg(test)]
mod tests {
    use super::Error;

    fn parse(raw: &str) -> Result<crate::Translations, Error> {
        let mut diagnostics = vec![];
        crate::Translations::from_str(raw, 0, false, &mut diagnostics)
    }

    #[test_util::test]
    fn rejects_unknown_allow_code() {
        // spellcheck:ignore-next-line
        let result = parse("[greeting]\nen = \"Hi\"\nallow = [\"duplicat\"]\n");
        assert!(
            // spellcheck:ignore-next-line
            matches!(&result, Err(Error::UnknownLintCode { code, .. }) if code == "duplicat"),
            "{result:?}"
        );
    }

    #[test_util::test]
    fn accepts_known_allow_codes_and_all() {
        for code in ["duplicate", "llm-drift", "missing-language", "all"] {
            let raw = format!("[greeting]\nen = \"Hi\"\nallow = [\"{code}\"]\n");
            assert!(parse(&raw).is_ok(), "{code}: {:?}", parse(&raw));
        }
    }

    #[test_util::test]
    fn rejects_unknown_single_string_allow() {
        let result = parse("[greeting]\nen = \"Hi\"\nallow = \"nope\"\n");
        assert!(
            matches!(result, Err(Error::UnknownLintCode { .. })),
            "{result:?}"
        );
    }

    /// Non-string leaf values produce the normal typed parse error instead of
    /// falling through an unfinished catch-all error path.
    #[test_util::test]
    fn rejects_non_string_translation_values() {
        let result = parse("[greeting]\nen = 42\n");
        assert!(
            matches!(
                result,
                Err(Error::UnexpectedType {
                    found: super::ValueKind::Integer,
                    ..
                })
            ),
            "{result:?}"
        );
    }
}
