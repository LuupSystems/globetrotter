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
        _: &mut RenderContext,
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
        // Register your custom helpers here
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
                // Register template
                self.handlebars.register_template_string(key, template)?;
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
        self.handlebars.render(key, &values)
    }
}

fn main() -> eyre::Result<()> {
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

    fn build_translator(json_translations: &str) -> eyre::Result<MyTranslator> {
        let _ = dbg!(serde_json::from_str::<serde_json::Value>(json_translations));
        let reader = std::io::Cursor::new(json_translations);

        // load translations
        let mut translator = MyTranslator::new();
        translator.load_translations_from_reader(reader)?;
        Ok(translator)
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
