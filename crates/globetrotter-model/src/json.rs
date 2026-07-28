//! JSON representations of translations for one language.

use crate::{Language, TemplateEngine, diagnostics::Spanned};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A single translation in the JSON output for one language.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Translation {
    /// A plain, non-templated string.
    #[serde(rename = "literal")]
    Literal(String),
    /// A template string to be rendered by a template engine.
    #[serde(rename = "template")]
    Template(String),
}

/// Schema version of the JSON translation output.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize, Default,
)]
pub enum Version {
    /// Version 1 of the schema.
    #[serde(rename = "1", alias = "v1", alias = "latest")]
    #[default]
    V1,
}

/// Errors that can occur while producing JSON translations.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// A required translation key was missing for the requested language.
    #[error("missing translation of key {key:?} for language {language:?}")]
    MissingKey {
        /// The key that was missing a translation.
        key: Spanned<String>,
        /// The language the translation was missing for.
        language: Language,
    },
    /// JSON serialization failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// The JSON representation of all translations for a single language.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Translations {
    /// The schema version.
    #[serde(default)]
    pub version: Version,
    /// The template engine used to render template translations, if any.
    pub template_engine: Option<TemplateEngine>,
    /// The language these translations are for.
    pub language: Language,
    /// The translations, keyed by their dotted key path.
    pub translations: IndexMap<String, Translation>,
}

impl crate::Translations {
    /// Writes one language as pretty-printed JSON.
    ///
    /// The serialized form is identical to [`Self::translations_json`] and has
    /// no trailing newline.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization fails or if a required
    /// translation is missing while `strict` is enabled.
    pub fn write_translations_json(
        &self,
        language: Language,
        template_engine: Option<TemplateEngine>,
        strict: bool,
        writer: impl std::io::Write,
    ) -> Result<Translations, Error> {
        let translations = self.translations_json(language, strict, template_engine)?;
        serde_json::to_writer_pretty(writer, &translations)?;
        Ok(translations)
    }

    /// Builds the JSON representation for one language.
    ///
    /// When `strict` is `false`, a missing value is replaced with a descriptive
    /// placeholder string. When `strict` is `true`, it returns
    /// [`Error::MissingKey`] instead.
    ///
    /// # Errors
    ///
    /// Returns an error if a required translation is missing while `strict`
    /// is enabled.
    pub fn translations_json(
        &self,
        language: Language,
        strict: bool,
        template_engine: Option<TemplateEngine>,
    ) -> Result<Translations, Error> {
        let translations = self
            .0
            .iter()
            .map(
                |(key, translation)| match translation.language.get(&language) {
                    Some(t) => {
                        let value = if translation.is_template() {
                            Translation::Template(t.as_ref().clone())
                        } else {
                            Translation::Literal(t.as_ref().clone())
                        };
                        Ok((key.clone().into_inner(), value))
                    }
                    None if strict => Err(Error::MissingKey {
                        key: key.clone(),
                        language,
                    }),
                    None => Ok((
                        key.clone().into_inner(),
                        Translation::Literal(format!("missing translation {key} for {language:?}")),
                    )),
                },
            )
            .collect::<Result<IndexMap<_, _>, _>>()?;
        Ok(Translations {
            version: Version::V1,
            template_engine,
            translations,
            language,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Version;

    /// The moving `latest` input alias resolves to the only supported schema
    /// while serialization remains pinned to its stable version number.
    #[test_util::test]
    fn latest_alias_resolves_to_v1() -> serde_json::Result<()> {
        assert_eq!(serde_json::from_str::<Version>("\"latest\"")?, Version::V1);
        assert_eq!(serde_json::to_string(&Version::V1)?, "\"1\"");
        Ok(())
    }
}
