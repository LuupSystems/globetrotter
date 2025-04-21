use crate::{
    diagnostics::{DiagnosticExt, FileId, Spanned},
    Language, TemplateEngine, Translation, Translations,
};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use itertools::Itertools;

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
                            .with_labels(vec![Label::primary(
                                translation.file_id,
                                template.span.clone(),
                            )
                            .with_message(err.to_string())]);
                        Some(diagnostic)
                    }
                }
            }),
    );
}

impl Translations {
    #[cfg(feature = "rayon")]
    pub fn validate(
        &self,
        config_name: &Spanned<String>,
        required_languages: &[Spanned<Language>],
        template_engine: Option<&Spanned<TemplateEngine>>,
        strict: bool,
        check_templates: bool,
        config_file_id: Option<FileId>,
        diagnostics: &mut Vec<Diagnostic<FileId>>,
    ) {
        use rayon::prelude::*;

        tracing::trace!(
            num_translations = self.0.len(),
            languages = ?required_languages.iter().map(Spanned::as_ref).collect::<Vec<_>>(),
            check_templates,
            "validating",
        );
        let partial_diagnostics = self.0.par_iter().flat_map(|(key, translation)| {
            let mut diagnostics = vec![];
            diagnostics.extend(required_languages.iter().unique().filter_map(|lang| {
                if translation.language.contains_key(lang.as_ref()) {
                    None
                } else {
                    None
                    // Some(ValidationError::MissingLanguage {
                    //     key: key.to_string(),
                    //     language: *lang,
                    // })
                }
            }));

            if check_templates && translation.is_template() {
                // check that templates compile
                match template_engine {
                    None => {
                        let diagnostic =
                            Diagnostic::warning_or_error(strict).with_message(format!(
                            "running with `--check`, but no template engine is specified for `{config_name}`",
                        ));
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
