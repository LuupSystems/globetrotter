pub mod helpers;

use clap::{
    Parser,
    builder::{PossibleValuesParser, TypedValueParser},
};
use color_eyre::eyre;
use globetrotter_model as model;
use handlebars::Handlebars;

/// The generated translations as JSON.
///
/// Of course, you could dynamically load them from the file system too..
pub static JSON_TRANSLATIONS_DE: &str =
    include_str!(concat!(env!("OUT_DIR"), "/translations_de.json"));
pub static JSON_TRANSLATIONS_EN: &str =
    include_str!(concat!(env!("OUT_DIR"), "/translations_en.json"));
pub static JSON_TRANSLATIONS_FR: &str =
    include_str!(concat!(env!("OUT_DIR"), "/translations_fr.json"));

/// The generated rust bindings for the translations
#[allow(clippy::all, clippy::pedantic)]
#[allow(unreachable_pub)]
#[allow(missing_docs)]
#[rustfmt::skip]
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/translations.rs"));
}
pub use generated::Translation;

pub trait TranslationKey: Clone + std::fmt::Debug {
    fn key(&self) -> &'static str;
}

impl TranslationKey for Translation<'_> {
    fn key(&self) -> &'static str {
        Translation::key(self)
    }
}

#[derive(Debug, Clone)]
pub struct MyTranslator<K> {
    translations: model::json::Translations,
    handlebars: Handlebars<'static>,
    _key: std::marker::PhantomData<K>,
}

impl<K> MyTranslator<K> {
    pub fn from_reader(reader: impl std::io::BufRead) -> eyre::Result<Self> {
        let mut handlebars = Handlebars::new();

        // Register your custom helpers here
        handlebars.register_helper(helpers::PLURALIZE_HELPER_NAME, Box::new(helpers::pluralize));

        let translations: model::json::Translations = serde_json::from_reader(reader)?;
        for (key, value) in &translations.translations {
            if let model::json::Translation::Template(template) = value {
                // Register template
                handlebars.register_template_string(key, template)?;
            }
        }
        Ok(Self {
            handlebars,
            translations,
            _key: std::marker::PhantomData::default(),
        })
    }
}

impl<K> MyTranslator<K>
where
    K: TranslationKey + serde::Serialize,
{
    #[must_use]
    pub fn translate(&self, key_with_args: K) -> Option<Result<String, handlebars::RenderError>> {
        let key = key_with_args.key();
        let translation = self.translations.translations.get(key)?;
        match translation {
            model::json::Translation::Literal(value) => Some(Ok(value.to_string())),
            model::json::Translation::Template(_) => {
                Some(self.handlebars.render(key, &key_with_args))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::VariantNames)]
pub enum Language {
    #[strum(serialize = "de")]
    De,
    #[strum(serialize = "en")]
    En,
    #[strum(serialize = "fr")]
    Fr,
}

fn language_parser() -> impl TypedValueParser {
    use strum::VariantNames;
    PossibleValuesParser::new(Language::VARIANTS).map(|s| s.parse::<Language>().unwrap())
}

#[derive(Parser, Debug)]
pub struct Options {
    #[clap(short = 'l', long = "language", value_parser = language_parser())]
    pub language: Language,

    #[clap(short = 'n', long = "name")]
    pub name: String,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let options = Options::parse();
    let json_translations = match options.language {
        Language::De => JSON_TRANSLATIONS_DE,
        Language::En => JSON_TRANSLATIONS_EN,
        Language::Fr => JSON_TRANSLATIONS_FR,
    };
    let reader = std::io::Cursor::new(json_translations);
    let translator: MyTranslator<Translation> = MyTranslator::from_reader(reader)?;
    let translated = translator
        .translate(Translation::TranslationGreeting {
            name: &options.name,
        })
        .transpose()?;
    println!("{}", translated.as_deref().unwrap_or_default());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn build_translator(json_translations: &str) -> eyre::Result<MyTranslator<Translation<'_>>> {
        let _ = dbg!(serde_json::from_str::<serde_json::Value>(json_translations));
        let reader = std::io::Cursor::new(json_translations);
        MyTranslator::from_reader(reader)
    }

    #[test]
    fn test_translation_key() -> eyre::Result<()> {
        crate::tests::init();

        sim_assert_eq!(
            have: serde_json::to_value(Translation::TranslationLiteral {})?,
            want: serde_json::json!({})
        );
        sim_assert_eq!(
            have: serde_json::to_value(Translation::TranslationGreeting { name: "Roman" })?,
            want: serde_json::json!({"name": "Roman" })
        );
        Ok(())
    }

    #[test]
    fn test_de() -> eyre::Result<()> {
        crate::tests::init();

        let translator = build_translator(JSON_TRANSLATIONS_DE)?;
        let have = translator
            .translate(Translation::TranslationLiteral {})
            .transpose()?;
        sim_assert_eq!(have: have.as_deref(), want: Some("German"));

        let have = translator
            .translate(Translation::TranslationGreeting { name: "Roman" })
            .transpose()?;
        sim_assert_eq!(have: have.as_deref(), want: Some("Hallo Roman"));
        Ok(())
    }

    #[test]
    fn test_en() -> eyre::Result<()> {
        crate::tests::init();

        let translator = build_translator(JSON_TRANSLATIONS_EN)?;
        let have = translator
            .translate(Translation::TranslationLiteral {})
            .transpose()?;
        sim_assert_eq!(have: have.as_deref(), want: Some("English"));

        let have = translator
            .translate(Translation::TranslationGreeting { name: "Roman" })
            .transpose()?;
        sim_assert_eq!(have: have.as_deref(), want: Some("Hello Roman"));
        Ok(())
    }

    #[test]
    fn test_fr() -> eyre::Result<()> {
        crate::tests::init();

        let translator = build_translator(JSON_TRANSLATIONS_FR)?;
        let have = translator
            .translate(Translation::TranslationLiteral {})
            .transpose()?;
        sim_assert_eq!(have: have.as_deref(), want: Some("French"));

        let have = translator
            .translate(Translation::TranslationGreeting { name: "Roman" })
            .transpose()?;
        sim_assert_eq!(have: have.as_deref(), want: Some("Bonjour Roman"));
        Ok(())
    }
}
