#![allow(warnings)]

use color_eyre::eyre;
use globetrotter_model as model;
use handlebars::Handlebars;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Translation<'a> {
    ValueOne {},
    ValueTwo {
        arg1: &'a str,
        arg2: i32,
        arg3: bool,
    },
}

// impl<'a> std::hash::Hash for Translation<'a> {
//     fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
//         self.key().hash(state)
//         // self.metadata().hash(state)
//     }
// }

// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
// pub enum TranslationKind {
//     Literal,
//     Templated,
// }

// #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
// pub struct Metadata {
//     pub key: &'static str,
//     // pub kind: TranslationKind,
// }

pub trait TranslationKey: serde::Serialize {
    // fn metadata(&self) -> Metadata;
    // fn translate(&self) -> Metadata;
    fn key(&self) -> &'static str;
    // fn values(&self) -> Result<serde_json::Value, serde_json::Error> {
    //     serde_json::to_value(self)
    // }
    // fn values(&self) -> &'static str;
}

// static METADATA: phf::Map<&'static str, Keyword> = phf::phf_map! {
//     "loop" => Keyword::Loop,
//     "continue" => Keyword::Continue,
//     "break" => Keyword::Break,
//     "fn" => Keyword::Fn,
//     "extern" => Keyword::Extern,
// };
// "static KEYWORDS: phf::Map<&'static str, Keyword> = {}",
//     phf_codegen::Map::new()
//         .entry("loop", "Keyword::Loop")
//         .entry("continue", "Keyword::Continue")
//         .entry("break", "Keyword::Break")
//         .entry("fn", "Keyword::Fn")
//         .entry("extern", "Keyword::Extern")
//         .build()

// impl<'a> TranslationKey for Translation<'a> {
impl Translation<'_> {
    #[must_use]
    pub fn key(&self) -> &'static str {
        match self {
            Self::ValueOne {} => "value.one",
            Self::ValueTwo { .. } => "value.two",
        }
    }
    // fn metadata(&self) -> Metadata {
    //     match self {
    //         Self::ValueOne {} => Metadata {
    //             key: "value.one",
    //             // kind: TranslationKind::Literal,
    //         },
    //         Self::ValueTwo { .. } => Metadata {
    //             key: "value.two",
    //             // kind: TranslationKind::Templated,
    //         },
    //     }
    // }
}

pub mod helpers {
    use handlebars::{
        Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderError,
        RenderErrorReason,
    };

    fn param_not_found(helper_name: &'static str, index: usize) -> RenderError {
        RenderError::from(RenderErrorReason::ParamNotFoundForIndex(helper_name, index))
    }

    fn invalid_param_type(
        helper_name: &'static str,
        param_name: &str,
        expected_type: &str,
    ) -> RenderError {
        RenderError::from(RenderErrorReason::ParamTypeMismatchForName(
            helper_name,
            param_name.to_string(),
            expected_type.to_string(),
        ))
    }

    pub const PLURALIZE_HELPER_NAME: &str = "pluralize";

    pub fn pluralize(
        h: &Helper,
        _: &Handlebars,
        _: &Context,
        rc: &mut RenderContext,
        out: &mut dyn Output,
    ) -> HelperResult {
        let value = h
            .param(0)
            .ok_or(param_not_found(PLURALIZE_HELPER_NAME, 0))?
            .value();
        let value =
            value
                .as_str()
                .ok_or(invalid_param_type(PLURALIZE_HELPER_NAME, "value", "string"))?;

        let count = h
            .param(1)
            .ok_or(param_not_found(PLURALIZE_HELPER_NAME, 1))?
            .value();
        let count = count
            .as_number()
            .and_then(serde_json::Number::as_i64)
            .ok_or(invalid_param_type(PLURALIZE_HELPER_NAME, "count", "number"))?;

        if count == 1 {
            out.write(value)?;
        } else {
            out.write(&format!("{value}s"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct MyTranslator {
    translations: Option<model::json::Translations>,
    handlebars: Handlebars<'static>,
}

impl MyTranslator {
    pub fn new() -> Self {
        let mut handlebars = Handlebars::new();
        // register your custom helpers here

        handlebars.register_helper(helpers::PLURALIZE_HELPER_NAME, Box::new(helpers::pluralize));
        Self {
            translations: None,
            handlebars,
        }
    }

    pub fn load_translations_from_reader(
        &mut self,
        reader: impl std::io::BufRead,
    ) -> eyre::Result<()> {
        let translations: model::json::Translations = serde_json::from_reader(reader)?;
        for (key, value) in &translations.translations {
            if let model::json::Translation::Template(template) = value {
                // register template
                self.handlebars.register_template_string(key, template);
            }
        }

        self.translations = Some(translations);
        Ok(())
    }

    pub fn load_translations(&mut self, path: &std::path::Path) -> eyre::Result<()> {
        let file = std::fs::OpenOptions::new().read(true).open(path)?;
        let reader = std::io::BufReader::new(file);
        self.load_translations_from_reader(reader)
    }

    #[must_use]
    pub fn translate(
        &self,
        spec: Translation<'_>,
    ) -> Option<Result<String, handlebars::RenderError>> {
        // let metadata = spec.metadata();
        let key = spec.key();
        let translations = self.translations.as_ref()?;
        let translation = translations.translations.get(key)?;
        match translation {
            model::json::Translation::Literal(value) => Some(Ok(value.to_string())),
            model::json::Translation::Template(_) => Some(self.template(key, spec)),
        }
    }
}

pub trait Template {
    type Error;
    fn template<T>(&self, key: &str, values: T) -> Result<String, Self::Error>
    where
        T: serde::Serialize;
}

impl Template for MyTranslator {
    type Error = handlebars::RenderError;
    fn template<T>(&self, key: &str, values: T) -> Result<String, Self::Error>
    where
        T: serde::Serialize,
    {
        // requires that
        self.handlebars.render(key, &values)
    }
}

fn main() -> eyre::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MyTranslator, Translation};
    use color_eyre::eyre;
    use similar_asserts::assert_eq as sim_assert_eq;

    static INIT: std::sync::Once = std::sync::Once::new();

    /// Initialize test
    ///
    /// This ensures `color_eyre` is setup once.
    pub fn init() {
        INIT.call_once(|| {
            color_eyre::install().ok();
        });
    }

    #[test]
    fn test_translation_key() -> eyre::Result<()> {
        crate::tests::init();

        sim_assert_eq!(
            serde_json::to_value(Translation::ValueOne {})?,
            serde_json::json!({})
        );
        sim_assert_eq!(
            serde_json::to_value(Translation::ValueTwo {
                arg1: "something",
                arg2: 3,
                arg3: true
            })?,
            serde_json::json!({"arg1": "something", "arg2": 3, "arg3": true })
        );
        Ok(())
    }

    #[test]
    fn translate() -> eyre::Result<()> {
        crate::tests::init();

        let json_translations = serde_json::json!({
            "version": "1",
            "language": "en",
            "translations": {
                "value.one": { "literal": "value one translated" },
                "value.two": { "template": "value two templated: arg1 = {{ arg1 }}" },
            },
        });
        let raw_json_translations = serde_json::to_string_pretty(&json_translations)?;
        println!("{}", raw_json_translations);
        let cursor = std::io::Cursor::new(raw_json_translations);
        let reader = std::io::BufReader::new(cursor);

        // load translations
        let mut translator = MyTranslator::new();
        translator.load_translations_from_reader(reader)?;

        sim_assert_eq!(
            translator
                .translate(Translation::ValueOne {})
                .transpose()?
                .as_deref(),
            Some("value one translated")
        );
        sim_assert_eq!(
            translator
                .translate(Translation::ValueTwo {
                    arg1: "works",
                    arg2: 0,
                    arg3: false
                })
                .transpose()?
                .as_deref(),
            Some("value two templated: arg1 = works")
        );
        Ok(())
    }
}
