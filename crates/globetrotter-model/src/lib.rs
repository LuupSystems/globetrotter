#![allow(warnings)]

pub mod diagnostics;
pub mod json;
pub mod language;
pub mod toml;
pub mod validation;

use diagnostics::{DisplayRepr, FileId, Spanned};
pub use indexmap::IndexMap;
pub use language::Language;

use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, strum::Display,
)]
pub enum TemplateEngine {
    #[serde(rename = "handlebars")]
    Handlebars,
    #[serde(rename = "golang", alias = "go")]
    Golang,
    #[serde(rename = "mustache")]
    Mustache,
    #[serde(rename = "jinja2")]
    Jinja2,
    Other(String),
}

impl std::str::FromStr for TemplateEngine {
    type Err = ::strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse()
    }
}

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
    #[serde(rename = "any")]
    #[strum(to_string = "any", serialize = "any")]
    Any,
    #[serde(rename = "string")]
    #[strum(to_string = "string", serialize = "string")]
    String,
    #[serde(rename = "number")]
    #[strum(to_string = "number", serialize = "number")]
    Number,
    #[serde(rename = "isodatetime")]
    #[strum(to_string = "isodatetime", serialize = "isodatetime")]
    Iso8601DateTimeString,
    // i8,
    // u8,
    // i16,
    // u16,
    // i32,
    // u32,
    // i64,
    // u64,
    // i128,
    // u128,
    // isize,
    // usize,
}

impl ArgumentType {
    #[must_use]
    pub fn display(&self) -> DisplayRepr<'_, Self> {
        DisplayRepr(self)
    }
}

pub type Arguments = IndexMap<String, ArgumentType>;
pub type LanguageTranslations = IndexMap<Language, Spanned<String>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Translation {
    #[serde(flatten)]
    pub language: LanguageTranslations,
    #[serde(skip_serializing_if = "Arguments::is_empty")]
    pub arguments: Arguments,
    #[serde(skip)]
    pub file_id: FileId,
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
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.arguments.is_empty() && self.language.is_empty()
    }

    #[must_use]
    pub fn is_template(&self) -> bool {
        !self.arguments.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Translations(pub IndexMap<Spanned<String>, Translation>);

impl Translations {
    #[cfg(not(feature = "rayon"))]
    pub fn sort(&mut self) {
        self.0.sort_keys();
        for (_key, translation) in self.0.iter_mut() {
            translation.arguments.sort_keys();
            translation.language.sort_keys();
        }
    }

    #[cfg(feature = "rayon")]
    pub fn sort(&mut self) {
        use rayon::prelude::*;
        self.0.par_sort_keys();
        for translation in self.0.values_mut() {
            translation.arguments.par_sort_keys();
            translation.language.par_sort_keys();
        }
    }

    #[must_use]
    pub fn iter(&self) -> indexmap::map::Iter<'_, Spanned<String>, Translation> {
        self.0.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl FromIterator<(Spanned<String>, Translation)> for Translations {
    fn from_iter<T: IntoIterator<Item = (Spanned<String>, Translation)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for Translations {
    type Item = (Spanned<String>, Translation);
    type IntoIter = indexmap::map::IntoIter<Spanned<String>, Translation>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    static INIT: std::sync::Once = std::sync::Once::new();

    /// Initialize test
    ///
    /// This ensures `color_eyre` is setup once.
    pub fn init() {
        INIT.call_once(|| {
            color_eyre::install().ok();
        });
    }
}
