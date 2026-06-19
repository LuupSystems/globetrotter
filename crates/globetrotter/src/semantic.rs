//! Adapter between globetrotter's translation model and the
//! [`globetrotter_semantic`] embedding crate.
//!
//! This module is only compiled with the `semantic` feature. It converts
//! [`Translations`] into the semantic crate's pure-data inputs, runs the
//! analysis, and maps each drift finding back to a source-located diagnostic.
//!
//! Drift findings are emitted as [notes](codespan_reporting::diagnostic::Severity::Note),
//! never as warnings or errors: cross-lingual similarity is a fuzzy,
//! threshold-dependent signal, so the output is a ranked review aid and does
//! not fail a lint run.

use crate::executor::SemanticParams;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use globetrotter_model::{
    Translations,
    diagnostics::{FileId, Span},
    lint::{LintCode, is_allowed},
};
use globetrotter_semantic::{Analyzer, KeyInput, LanguageText, Options};
use std::collections::HashMap;

pub use globetrotter_semantic::{Error, Model};

/// Drives an [`indicatif::ProgressBar`] from the semantic analysis.
pub struct BarProgress(pub indicatif::ProgressBar);

impl globetrotter_semantic::Progress for BarProgress {
    fn set_length(&self, total: u64) {
        self.0.set_length(total);
    }
    fn inc(&self, delta: u64) {
        self.0.inc(delta);
    }
    fn set_message(&self, message: String) {
        self.0.set_message(message);
    }
}

/// Ensure the hybrid data files for `languages` are present in `data_dir`,
/// downloading missing ones with byte progress.
///
/// # Errors
///
/// Returns an error if a download fails.
pub fn ensure_data(
    data_dir: &std::path::Path,
    languages: &[String],
    progress: &dyn globetrotter_semantic::Progress,
) -> Result<(), Error> {
    globetrotter_semantic::data::ensure_data(data_dir, languages, progress)
}

impl From<crate::executor::SemanticModel> for Model {
    fn from(model: crate::executor::SemanticModel) -> Self {
        match model {
            crate::executor::SemanticModel::MultilingualE5Small => Self::MultilingualE5Small,
            crate::executor::SemanticModel::Labse => Self::Labse,
            crate::executor::SemanticModel::BgeRerankerV2M3 => Self::BgeRerankerV2M3,
            crate::executor::SemanticModel::MultilingualMiniLmNli => Self::MultilingualMiniLmNli,
            crate::executor::SemanticModel::MdebertaV3Nli => Self::MdebertaV3Nli,
        }
    }
}

/// Load (downloading on first use) the embedding model named by `params`.
///
/// This is blocking and potentially slow on the first call for a given model
/// (it downloads the weights); callers should run it off the async runtime.
///
/// # Errors
///
/// Returns an error if the model cannot be downloaded or loaded.
pub fn load_analyzer(params: &SemanticParams) -> Result<Analyzer, Error> {
    Analyzer::load(&Options {
        model: params.model.into(),
        hybrid: params.hybrid,
        threshold: params.threshold,
        clwe_threshold: params.clwe_threshold,
        min_words: params.min_words,
        data_dir: params.data_dir.clone(),
        glossary: params.glossary.clone(),
        cache_dir: params.cache_dir.clone(),
    })
}

/// Source spans for one key, used to attach diagnostics back to the file.
struct KeySpans<'a> {
    file_id: FileId,
    key_span: Span,
    language_spans: HashMap<&'a str, Span>,
}

/// Analyze the translations for cross-lingual drift and return note diagnostics
/// for every language pair below the threshold, streamed via `emit` as it is
/// found. Returns the number of findings emitted.
///
/// Keys that suppress the `semantic-drift` code via their `allow` list, and
/// keys with fewer than two non-empty languages, are skipped. A positive
/// [`SemanticParams::top`] caps the number emitted.
///
/// # Errors
///
/// Returns an error if embedding fails.
pub fn stream(
    analyzer: &Analyzer,
    translations: &Translations,
    params: &SemanticParams,
    progress: &dyn globetrotter_semantic::Progress,
    emit: &mut dyn FnMut(Diagnostic<FileId>),
) -> Result<usize, Error> {
    let mut inputs: Vec<KeyInput<'_>> = Vec::new();
    let mut spans: HashMap<&str, KeySpans<'_>> = HashMap::new();

    for (key, translation) in translations {
        if is_allowed(&translation.allow, LintCode::SemanticDrift) {
            continue;
        }

        let mut languages: Vec<LanguageText<'_>> = Vec::new();
        let mut language_spans: HashMap<&str, Span> = HashMap::new();
        for (language, text) in &translation.language {
            let code: &'static str = (*language).into();
            languages.push(LanguageText {
                language: code,
                text: text.as_ref().as_str(),
            });
            language_spans.insert(code, text.span.clone());
        }
        if languages.len() < 2 {
            continue;
        }

        let key_str = key.as_ref().as_str();
        spans.insert(
            key_str,
            KeySpans {
                file_id: translation.file_id,
                key_span: key.span.clone(),
                language_spans,
            },
        );
        inputs.push(KeyInput {
            key: key_str,
            languages,
        });
    }

    if inputs.is_empty() {
        return Ok(0);
    }

    let cap = params.top;
    let mut emitted = 0usize;
    analyzer.analyze_each(&inputs, progress, &mut |drift| {
        if cap > 0 && emitted >= cap {
            return;
        }
        if let Some(diagnostic) = diagnostic_for(&spans, &drift) {
            emit(diagnostic);
            emitted += 1;
        }
    })?;
    Ok(emitted)
}

/// Build a note diagnostic for one drift finding, pointing at the two
/// translation strings.
fn diagnostic_for(
    spans: &HashMap<&str, KeySpans<'_>>,
    drift: &globetrotter_semantic::Drift,
) -> Option<Diagnostic<FileId>> {
    let key_spans = spans.get(drift.key.as_str())?;

    let mut labels = Vec::new();
    if let Some(span) = key_spans.language_spans.get(drift.language_a.as_str()) {
        labels.push(
            Label::primary(key_spans.file_id, span.clone())
                .with_message(format!("`{}` translation", drift.language_a)),
        );
    }
    if let Some(span) = key_spans.language_spans.get(drift.language_b.as_str()) {
        labels.push(
            Label::secondary(key_spans.file_id, span.clone())
                .with_message(format!("`{}` translation", drift.language_b)),
        );
    }
    if labels.is_empty() {
        labels.push(
            Label::primary(key_spans.file_id, key_spans.key_span.clone())
                .with_message("defined here"),
        );
    }

    Some(
        Diagnostic::note()
            .with_code(LintCode::SemanticDrift)
            .with_message(format!(
                "`{}`: `{}` and `{}` may have drifted apart (similarity {:.2})",
                drift.key, drift.language_a, drift.language_b, drift.similarity
            ))
            .with_labels(labels),
    )
}
