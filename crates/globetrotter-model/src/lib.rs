//! Core data model for globetrotter translations.
//!
//! This crate defines the in-memory representation of translation files
//! (keys, per-language strings, and template arguments) along with the
//! supporting types for parsing, serialization, validation, and diagnostics.

/// Source-span aware diagnostic helpers shared across the model.
pub mod diagnostics;
/// Extension traits used throughout the crate.
pub mod ext;
/// JSON representation of translations for a single language.
pub mod json;
/// Supported languages.
pub mod language;
/// Linting of translation files.
pub mod lint;
/// TOML parsing of translation files.
pub mod toml;
/// Validation of translations against a set of options.
pub mod validation;

use diagnostics::{DisplayRepr, FileId, Spanned};
pub use indexmap::IndexMap;
pub use language::Language;

use serde::{Deserialize, Serialize};

/// Templating engine used to render template translations.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, strum::Display,
)]
pub enum TemplateEngine {
    /// The [Handlebars](https://handlebarsjs.com/) template engine.
    #[serde(rename = "handlebars")]
    Handlebars,
    /// The Go `text/template` template engine.
    #[serde(rename = "golang", alias = "go")]
    Golang,
    /// The [Mustache](https://mustache.github.io/) template engine.
    #[serde(rename = "mustache")]
    Mustache,
    /// The [Jinja2](https://jinja.palletsprojects.com/) template engine.
    #[serde(rename = "jinja2")]
    Jinja2,
    /// Any other template engine, identified by name.
    Other(String),
}

impl std::str::FromStr for TemplateEngine {
    type Err = ::strum::ParseError;

    /// Parses the serde names and aliases declared on the variants, preserving
    /// any other non-empty name as [`TemplateEngine::Other`].
    ///
    /// # Errors
    ///
    /// Returns [`strum::ParseError::VariantNotFound`] only for an empty name.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "handlebars" => Ok(Self::Handlebars),
            "golang" | "go" => Ok(Self::Golang),
            "mustache" => Ok(Self::Mustache),
            "jinja2" => Ok(Self::Jinja2),
            "" => Err(::strum::ParseError::VariantNotFound),
            other => Ok(Self::Other(other.to_string())),
        }
    }
}

#[cfg(test)]
mod template_engine_tests {
    use super::TemplateEngine;

    /// Known names, aliases, and custom engine names parse without recursion.
    #[test_util::test]
    fn parses_engine_names_without_recursing() {
        assert_eq!("handlebars".parse(), Ok(TemplateEngine::Handlebars));
        assert_eq!("golang".parse(), Ok(TemplateEngine::Golang));
        assert_eq!("go".parse(), Ok(TemplateEngine::Golang));
        assert_eq!("mustache".parse(), Ok(TemplateEngine::Mustache));
        assert_eq!("jinja2".parse(), Ok(TemplateEngine::Jinja2));
        assert_eq!(
            "tera".parse(),
            Ok(TemplateEngine::Other("tera".to_string()))
        );
        assert_eq!(
            "".parse::<TemplateEngine>(),
            Err(strum::ParseError::VariantNotFound)
        );
    }
}

/// The declared type of a template argument.
#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    strum::Display,
    strum::EnumString,
    strum::VariantNames,
    strum::IntoStaticStr,
    strum::EnumCount,
    strum::EnumIter,
    Serialize,
    Deserialize,
)]
pub enum ArgumentType {
    /// An argument of any type.
    #[serde(rename = "any")]
    #[strum(to_string = "any")]
    Any,
    /// A string argument.
    #[serde(rename = "string")]
    #[strum(to_string = "string")]
    String,
    /// A numeric argument.
    #[serde(rename = "number")]
    #[strum(to_string = "number")]
    Number,
    /// An ISO 8601 date-time string argument.
    #[serde(rename = "isodatetime")]
    #[strum(to_string = "isodatetime")]
    Iso8601DateTimeString,
}

impl ArgumentType {
    /// Returns a [`std::fmt::Display`] adapter for this argument type.
    #[must_use]
    pub fn display(&self) -> DisplayRepr<'_, Self> {
        DisplayRepr(self)
    }
}

/// Template arguments, keyed by name and mapped to their declared type.
pub type Arguments = IndexMap<String, ArgumentType>;
/// Per-language translation strings for a single key.
pub type LanguageTranslations = IndexMap<Language, Spanned<String>>;

/// A single translation entry: its per-language strings and template arguments.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Translation {
    /// The translated string for each language.
    #[serde(flatten)]
    pub language: LanguageTranslations,
    /// The template arguments referenced by this translation.
    #[serde(skip_serializing_if = "Arguments::is_empty")]
    pub arguments: Arguments,
    /// The id of the source file this translation was parsed from.
    #[serde(skip)]
    pub file_id: FileId,
    /// Lint codes explicitly allowed (suppressed) for this translation key,
    /// declared via an `allow` key in the translation file.
    #[serde(skip)]
    pub allow: std::collections::BTreeSet<lint::AllowEntry>,
}

impl std::fmt::Display for Translation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Translation")
            .field(
                "arguments",
                &self
                    .arguments
                    .iter()
                    .map(|(k, v)| (k, v.display()))
                    .collect::<IndexMap<_, _>>(),
            )
            .field(
                "language",
                &self
                    .language
                    .iter()
                    .map(|(k, v)| (k, v.display()))
                    .collect::<IndexMap<_, _>>(),
            )
            .field("file_id", &self.file_id)
            .finish()
    }
}

impl Translation {
    /// Returns `true` if this translation has neither arguments nor languages.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.arguments.is_empty() && self.language.is_empty()
    }

    /// Returns `true` if this translation declares template arguments.
    #[must_use]
    pub fn is_template(&self) -> bool {
        !self.arguments.is_empty()
    }
}

/// A collection of translations, keyed by their dotted key path.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Translations(pub IndexMap<Spanned<String>, Translation>);

impl Translations {
    /// Sorts translation keys, argument names, and languages deterministically.
    #[cfg(not(feature = "rayon"))]
    pub fn sort(&mut self) {
        self.0.sort_keys();
        for (_key, translation) in &mut self.0 {
            translation.arguments.sort_keys();
            translation.language.sort_keys();
        }
    }

    /// Sorts translation keys, argument names, and languages deterministically.
    #[cfg(feature = "rayon")]
    pub fn sort(&mut self) {
        self.0.par_sort_keys();
        for translation in self.0.values_mut() {
            translation.arguments.par_sort_keys();
            translation.language.par_sort_keys();
        }
    }

    /// Returns an iterator over the translations.
    #[must_use]
    pub fn iter(&self) -> indexmap::map::Iter<'_, Spanned<String>, Translation> {
        self.0.iter()
    }

    /// Returns the number of translations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if there are no translations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<(Spanned<String>, Translation)> for Translations {
    fn from_iter<T: IntoIterator<Item = (Spanned<String>, Translation)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<'a> IntoIterator for &'a Translations {
    type Item = (&'a Spanned<String>, &'a Translation);
    type IntoIter = indexmap::map::Iter<'a, Spanned<String>, Translation>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for Translations {
    type Item = (Spanned<String>, Translation);
    type IntoIter = indexmap::map::IntoIter<Spanned<String>, Translation>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
