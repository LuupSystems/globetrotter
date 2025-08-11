use crate::{Language, TemplateEngine, diagnostics::Spanned};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Translation {
    #[serde(rename = "literal")]
    Literal(String),
    #[serde(rename = "template")]
    Template(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub enum Version {
    #[serde(rename = "1", alias = "v1")]
    V1,
    // #[serde(rename = "latest")]
    // Latest,
}

impl Default for Version {
    fn default() -> Self {
        Self::V1
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("missing translation of key {key:?} for language {language:?}")]
    MissingKey {
        key: Spanned<String>,
        language: Language,
    },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Translations {
    #[serde(default)]
    pub version: Version,
    pub template_engine: Option<TemplateEngine>,
    pub language: Language,
    pub translations: IndexMap<String, Translation>,
}

impl crate::Translations {
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
                            Translation::Template(t.as_ref().to_string())
                        } else {
                            Translation::Literal(t.as_ref().to_string())
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
