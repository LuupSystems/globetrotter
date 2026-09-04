//! Validation of translation completeness and template syntax.
//!
//! Validation runs before outputs are generated and asks only what generation
//! needs: that every required language is present and, when requested, that
//! every template compiles with the configured engine.
//! Everything beyond that is a lint.

use crate::{
    Language, TemplateEngine, Translation, Translations,
    diagnostics::{DiagnosticExt, FileId, Spanned},
    template,
};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use std::collections::BTreeSet;

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
    /// Validates the translations and appends any issues to `diagnostics`.
    ///
    /// Existing diagnostics are retained.
    /// Validation checks required languages and, when requested, compiles
    /// templates using the configured engine.
    /// With the `rayon` feature, keys are validated in parallel.
    pub fn validate(
        &self,
        config_name: &Spanned<String>,
        config_file_id: Option<FileId>,
        diagnostics: &mut Vec<Diagnostic<FileId>>,
        options: &ValidationOptions<'_>,
    ) {
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

        // Resolve the shared requirements once before validating each key.
        let required_languages = options
            .required_languages
            .iter()
            .map(|language| *language.as_ref())
            .collect::<BTreeSet<_>>();
        let analyzer = self.template_analyzer(config_name, config_file_id, options, diagnostics);

        let validate = |(key, translation): (&Spanned<String>, &Translation)| {
            validate_translation(
                key,
                translation,
                &required_languages,
                analyzer,
                options.strict,
            )
        };
        #[cfg(feature = "rayon")]
        let per_key: Vec<Vec<Diagnostic<FileId>>> = {
            use rayon::prelude::*;
            self.0.par_iter().map(validate).collect()
        };
        #[cfg(not(feature = "rayon"))]
        let per_key: Vec<Vec<Diagnostic<FileId>>> = self.0.iter().map(validate).collect();

        diagnostics.extend(per_key.into_iter().flatten());
    }

    /// Decides once per catalog whether and how templates are compiled.
    ///
    /// A missing or unsupported engine is reported once here rather than for
    /// every template key, and only when there is a template to check.
    fn template_analyzer(
        &self,
        config_name: &Spanned<String>,
        config_file_id: Option<FileId>,
        options: &ValidationOptions<'_>,
        diagnostics: &mut Vec<Diagnostic<FileId>>,
    ) -> Option<template::Analyzer> {
        if !options.check_templates || !self.0.values().any(Translation::is_template) {
            return None;
        }
        let Some(engine) = options.template_engine else {
            diagnostics.push(
                Diagnostic::warning_or_error(options.strict).with_message(format!(
                    "running with `--check`, but no template engine is specified for `{config_name}`"
                )),
            );
            return None;
        };
        let analyzer = template::Analyzer::for_engine(engine.as_ref());
        if analyzer.is_none() {
            let mut diagnostic =
                Diagnostic::error().with_message(format!("unsupported template engine `{engine}`"));
            if let Some(config_file_id) = config_file_id {
                diagnostic = diagnostic.with_labels(vec![
                    Label::primary(config_file_id, engine.span.clone()).with_message(format!(
                        "`--check` is not supported for template engine `{engine}`"
                    )),
                ]);
            }
            diagnostics.push(diagnostic);
        }
        analyzer
    }
}

/// Validates one key: its required languages and, with an analyzer, that each
/// of its templates compiles.
fn validate_translation(
    key: &Spanned<String>,
    translation: &Translation,
    required_languages: &BTreeSet<Language>,
    analyzer: Option<template::Analyzer>,
    strict: bool,
) -> Vec<Diagnostic<FileId>> {
    let mut diagnostics = Vec::new();

    // Check that every required language is present.
    for language in required_languages {
        if translation.language.contains_key(language) {
            continue;
        }
        diagnostics.push(
            Diagnostic::warning_or_error(strict)
                .with_message(format!("missing `{}` translation", language.code()))
                .with_labels(vec![
                    Label::primary(translation.file_id, key.span.clone()).with_message(format!(
                        "`{}` has no `{}` translation",
                        key.as_ref(),
                        language.code()
                    )),
                ]),
        );
    }

    // A template that does not compile can never render, so it is always an
    // error, regardless of `strict`.
    if let Some(analyzer) = analyzer
        && translation.is_template()
    {
        for (language, value) in &translation.language {
            if let Err(error) = analyzer.analyze(value.as_ref()) {
                diagnostics.push(
                    Diagnostic::error()
                        .with_message(format!("`{}` template fails to compile", language.code()))
                        .with_labels(vec![
                            Label::primary(translation.file_id, value.span.clone())
                                .with_message(error.to_string()),
                        ]),
                );
            }
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::ValidationOptions;
    use crate::{
        ArgumentType, Arguments, IndexMap, Language, LanguageTranslations, TemplateEngine,
        Translation, Translations, diagnostics::Spanned,
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

    /// A catalog with two template keys, one of which does not compile.
    fn templates_with_one_broken() -> Translations {
        let template = |key: &str, text: &str| {
            (
                Spanned::dummy(key.to_string()),
                Translation {
                    language: LanguageTranslations::from([(
                        Language::En,
                        Spanned::new(30..40, text.to_string()),
                    )]),
                    arguments: Arguments::from([("name".to_string(), ArgumentType::String)]),
                    file_id: 7,
                    allow: BTreeSet::default(),
                },
            )
        };
        Translations(IndexMap::from([
            template("fine", "Hello {{name}}"),
            template("broken", "Hello {{name"),
        ]))
    }

    fn validate(
        translations: &Translations,
        required_languages: &[Spanned<Language>],
        template_engine: Option<&Spanned<TemplateEngine>>,
        strict: bool,
        check_templates: bool,
    ) -> Vec<codespan_reporting::diagnostic::Diagnostic<usize>> {
        let options = ValidationOptions {
            required_languages,
            template_engine,
            strict,
            check_templates,
        };
        let mut diagnostics = Vec::new();
        translations.validate(
            &Spanned::dummy("app".to_string()),
            Some(3),
            &mut diagnostics,
            &options,
        );
        diagnostics
    }

    /// Required languages missing during generation are reported once even
    /// when the configuration lists the same language more than once.
    #[test_util::test]
    fn reports_missing_required_languages() {
        let translations = translations_with_only_english();
        let required_languages = [
            Spanned::dummy(Language::En),
            Spanned::dummy(Language::De),
            Spanned::dummy(Language::De),
        ];
        let diagnostics = validate(&translations, &required_languages, None, false, false);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert_eq!(diagnostics[0].message, "missing `de` translation");
        assert_eq!(diagnostics[0].labels[0].file_id, 7);
        assert_eq!(diagnostics[0].labels[0].range, 4..12);
    }

    /// Strict generation promotes a missing required language to an error.
    #[test_util::test]
    fn strict_mode_promotes_missing_language_to_error() {
        let translations = translations_with_only_english();
        let required_languages = [Spanned::dummy(Language::De)];
        let diagnostics = validate(&translations, &required_languages, None, true, false);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    /// A template the engine rejects is an error that points at the template
    /// and carries the engine's reason.
    #[test_util::test]
    fn template_check_reports_the_engine_reason() {
        let translations = templates_with_one_broken();
        let engine = Spanned::dummy(TemplateEngine::Handlebars);
        let diagnostics = validate(&translations, &[], Some(&engine), false, true);

        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert_eq!(diagnostics[0].message, "`en` template fails to compile");
        assert_eq!(diagnostics[0].labels[0].range, 30..40);
        assert!(
            diagnostics[0].labels[0]
                .message
                .starts_with("invalid handlebars syntax"),
            "{}",
            diagnostics[0].labels[0].message
        );
    }

    /// Checking templates without an engine, or with one that cannot be
    /// compiled, is reported once per catalog rather than once per key.
    #[test_util::test]
    fn template_check_needs_a_supported_engine() {
        let translations = templates_with_one_broken();

        let diagnostics = validate(&translations, &[], None, false, true);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(
            diagnostics[0].message.contains("no template engine"),
            "{}",
            diagnostics[0].message
        );

        let engine = Spanned::new(50..56, TemplateEngine::Jinja2);
        let diagnostics = validate(&translations, &[], Some(&engine), false, true);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert_eq!(
            diagnostics[0].message,
            "unsupported template engine `jinja2`"
        );
        // The label points at the engine in the config file.
        assert_eq!(diagnostics[0].labels[0].file_id, 3);
        assert_eq!(diagnostics[0].labels[0].range, 50..56);
    }

    /// Without `check_templates`, a broken template is not validation's
    /// concern.
    #[test_util::test]
    fn template_check_is_opt_in() {
        let translations = templates_with_one_broken();
        let engine = Spanned::dummy(TemplateEngine::Handlebars);
        let diagnostics = validate(&translations, &[], Some(&engine), true, false);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}
