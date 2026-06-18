use crate::{Language, TemplateEngine, Translations, diagnostics::Spanned};

#[cfg(feature = "rayon")]
use crate::{
    Translation,
    diagnostics::{DiagnosticExt, FileId},
};
#[cfg(feature = "rayon")]
use codespan_reporting::diagnostic::{Diagnostic, Label};
#[cfg(feature = "rayon")]
use itertools::Itertools;

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
        let partial_diagnostics = self.0.par_iter().flat_map(|(_key, translation)| {
            let mut diagnostics = vec![];
            diagnostics.extend(options.required_languages.iter().unique().filter_map(|lang| {
                #[allow(
                    clippy::if_same_then_else,
                    reason = "placeholder: the else branch will emit a MissingLanguage diagnostic once that validation is implemented"
                )]
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

            if options.check_templates && translation.is_template() {
                // Check that templates compile
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
