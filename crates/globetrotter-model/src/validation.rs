use crate::{Language, TemplateEngine, Translations, diagnostics::Spanned};

#[cfg(feature = "rayon")]
use crate::{
    Translation,
    diagnostics::{DiagnosticExt, FileId},
};
#[cfg(feature = "rayon")]
use codespan_reporting::diagnostic::{Diagnostic, Label};

#[cfg(feature = "rayon")]
fn validate_handlebars_template(translation: &Translation, errors: &mut Vec<Diagnostic<FileId>>) {
    errors.extend(
        translation
            .language
            .iter()
            .filter_map(|(language, template)| {
                tracing::trace!(
                    lang = ?language,
                    template = template.as_ref(),
                    engine = ?TemplateEngine::Handlebars,
                    "validating",
                );
                match handlebars::template::Template::compile(template.as_ref()) {
                    Ok(_) => None,
                    Err(err) => {
                        let diagnostic = Diagnostic::error()
                            .with_message("handlebars template failed to compile")
                            .with_labels(vec![
                                Label::primary(translation.file_id, template.span.clone())
                                    .with_message(err.to_string()),
                            ]);
                        Some(diagnostic)
                    }
                }
            }),
    );
}

/// Options controlling how translations are validated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValidationOptions<'a> {
    /// Languages that every translation key is required to provide.
    pub required_languages: &'a [Spanned<Language>],
    /// The template engine used to compile template translations, if any.
    pub template_engine: Option<&'a Spanned<TemplateEngine>>,
    /// Whether validation issues are reported as errors rather than warnings.
    pub strict: bool,
    /// Whether template translations are compiled to check for errors.
    pub check_templates: bool,
}

impl Translations {
    /// Validate the translations, pushing any issues onto `diagnostics`.
    #[cfg(feature = "rayon")]
    pub fn validate(
        &self,
        config_name: &Spanned<String>,
        config_file_id: Option<FileId>,
        diagnostics: &mut Vec<Diagnostic<FileId>>,
        options: &ValidationOptions<'_>,
    ) {
        use rayon::prelude::*;

        tracing::trace!(
            num_translations = self.0.len(),
            languages = ?options
                .required_languages
                .iter()
                .map(Spanned::as_ref)
                .collect::<Vec<_>>(),
            check_templates = options.check_templates,
            "validating",
        );
        let required_languages = options
            .required_languages
            .iter()
            .map(|language| *language.as_ref())
            .collect::<std::collections::BTreeSet<_>>();
        let partial_diagnostics = self.0.par_iter().flat_map(|(key, translation)| {
            let mut diagnostics = vec![];
            diagnostics.extend(
                required_languages
                    .iter()
                    .filter(|language| !translation.language.contains_key(*language))
                    .map(|language| {
                        Diagnostic::warning_or_error(options.strict)
                            .with_message(format!(
                                "missing `{}` translation",
                                language.code()
                            ))
                            .with_labels(vec![
                                Label::primary(translation.file_id, key.span.clone()).with_message(
                                    format!(
                                        "`{}` has no `{}` translation",
                                        key.as_ref(),
                                        language.code()
                                    ),
                                ),
                            ])
                    }),
            );

            if options.check_templates && translation.is_template() {
                match options.template_engine {
                    None => {
                        let message = format!(
                            "running with `--check`, but no template engine is specified for `{config_name}`",
                        );
                        let diagnostic =
                            Diagnostic::warning_or_error(options.strict).with_message(message);
                        diagnostics.push(diagnostic);
                    }
                    Some(Spanned {
                        inner: TemplateEngine::Handlebars,
                        ..
                    }) => validate_handlebars_template(translation, &mut diagnostics),
                    Some(other) => {
                        let mut diagnostic = Diagnostic::error().with_message(format!(
                            "unsupported template engine {:?}",
                            other.as_ref()
                        ));
                        if let Some(config_file_id) = config_file_id {
                            diagnostic = diagnostic.with_labels(vec![Label::primary(
                                config_file_id,
                                other.span.clone(),
                            )
                            .with_message(format!(
                                "`--check` is not supported for template engine {:?}",
                                other.as_ref()
                            ))]);
                        }
                        diagnostics.push(diagnostic);
                    }
                }
            }
            diagnostics
        });

        diagnostics.extend(partial_diagnostics.collect::<Vec<_>>());
    }
}

#[cfg(all(test, feature = "rayon"))]
mod tests {
    use super::ValidationOptions;
    use crate::{
        Arguments, IndexMap, Language, LanguageTranslations, Translation, Translations,
        diagnostics::Spanned,
    };
    use codespan_reporting::diagnostic::Severity;
    use std::collections::BTreeSet;

    fn translations_with_only_english() -> Translations {
        Translations(IndexMap::from([(
            Spanned::new(4..12, "greeting".to_string()),
            Translation {
                language: LanguageTranslations::from([(
                    Language::En,
                    Spanned::new(20..25, "Hello".to_string()),
                )]),
                arguments: Arguments::default(),
                file_id: 7,
                allow: BTreeSet::default(),
            },
        )]))
    }

    /// Required languages missing during generation are reported once even
    /// when the configuration lists the same language more than once.
    #[test]
    fn reports_missing_required_languages() {
        let translations = translations_with_only_english();
        let required_languages = [
            Spanned::dummy(Language::En),
            Spanned::dummy(Language::De),
            Spanned::dummy(Language::De),
        ];
        let options = ValidationOptions {
            required_languages: &required_languages,
            template_engine: None,
            strict: false,
            check_templates: false,
        };
        let mut diagnostics = Vec::new();

        translations.validate(
            &Spanned::dummy("app".to_string()),
            Some(3),
            &mut diagnostics,
            &options,
        );

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert_eq!(diagnostics[0].message, "missing `de` translation");
        assert_eq!(diagnostics[0].labels[0].file_id, 7);
        assert_eq!(diagnostics[0].labels[0].range, 4..12);
    }

    /// Strict generation promotes a missing required language to an error.
    #[test]
    fn strict_mode_promotes_missing_language_to_error() {
        let translations = translations_with_only_english();
        let required_languages = [Spanned::dummy(Language::De)];
        let options = ValidationOptions {
            required_languages: &required_languages,
            template_engine: None,
            strict: true,
            check_templates: false,
        };
        let mut diagnostics = Vec::new();

        translations.validate(
            &Spanned::dummy("app".to_string()),
            Some(3),
            &mut diagnostics,
            &options,
        );

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }
}
