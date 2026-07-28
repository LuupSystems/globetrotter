//! Adapter between globetrotter's translation model and the
//! [`globetrotter_llm_judge`] crate.
//!
//! This module is only compiled with the `llm-judge` feature. It converts
//! [`Translations`] into the judge's pure-data inputs, runs the judging, and
//! maps each finding back to a source-located diagnostic.
//!
//! Findings are emitted as [notes](codespan_reporting::diagnostic::Severity::Note),
//! never as warnings or errors: the judge is a review aid tuned for recall — a
//! finding is a suggestion for inspection, not a pass/fail signal.

use crate::executor::{LlmJudgeEffort, LlmJudgeParams};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use globetrotter_llm_judge::{Effort, Judge, KeyInput, LanguageText, Options};
use globetrotter_model::{
    Translations,
    diagnostics::{FileId, Span},
    lint::{LintCode, is_allowed},
};
use std::collections::HashMap;

pub use globetrotter_llm_judge::{Error, Stats};

/// Drives an [`indicatif::ProgressBar`] from judging progress.
pub struct BarProgress(
    /// The progress bar updated by judge callbacks.
    pub indicatif::ProgressBar,
);

impl globetrotter_llm_judge::Progress for BarProgress {
    fn set_length(&self, total: u64) {
        self.0.set_length(total);
    }
    fn inc(&self, delta: u64) {
        self.0.inc(delta);
    }
}

/// Creates a judge from executor parameters.
///
/// # Errors
///
/// Returns an error if the verdict cache cannot be created.
pub fn judge(params: &LlmJudgeParams) -> Result<Judge, Error> {
    Judge::new(Options {
        base_url: params.base_url.clone(),
        model: params.model.clone(),
        api_key_env: params.api_key_env.clone(),
        concurrency: params.concurrency,
        temperature: params.temperature,
        effort: params.effort.map(|effort| match effort {
            LlmJudgeEffort::Low => Effort::Low,
            LlmJudgeEffort::Medium => Effort::Medium,
            LlmJudgeEffort::High => Effort::High,
        }),
        template: params.template.clone(),
        min_confidence: params.min_confidence,
        cache_dir: params.cache_dir.clone(),
        cache_capacity: params.cache_capacity,
    })
}

/// Source spans for one key, used to attach diagnostics back to the file.
struct KeySpans<'a> {
    file_id: FileId,
    key_span: Span,
    language_spans: HashMap<&'a str, Span>,
}

/// Judges translations for cross-language drift.
///
/// A note diagnostic is passed to `emit` for every flagged language as its
/// verdict arrives. The returned [`Stats`] describe the complete run.
///
/// Keys that suppress the `llm-drift` code via their `allow` list, and keys
/// with fewer than two languages, are skipped.
///
/// # Errors
///
/// Returns an error if the endpoint keeps failing or the verdict cache cannot
/// be written.
pub async fn stream(
    judge: &Judge,
    translations: &Translations,
    progress: &dyn globetrotter_llm_judge::Progress,
    emit: &mut dyn FnMut(Diagnostic<FileId>),
) -> Result<Stats, Error> {
    let mut inputs: Vec<KeyInput<'_>> = Vec::new();
    let mut spans: HashMap<&str, KeySpans<'_>> = HashMap::new();

    for (key, translation) in translations {
        if is_allowed(&translation.allow, LintCode::LlmDrift) {
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
        return Ok(Stats::default());
    }

    judge
        .judge(&inputs, progress, &mut |finding| {
            if let Some(diagnostic) = diagnostic_for(&spans, &finding) {
                emit(diagnostic);
            }
        })
        .await
}

/// The finding's confidence as a color-coded percent badge: red when the model
/// is sure (≥ 80%), yellow when middling (≥ 50%), dimmed below that, so likely
/// real drift stands out when scanning many notes.
///
/// Colors go through [`colored`]'s global control, so `--color=never` (or a
/// non-terminal) yields plain text.
fn confidence_badge(confidence: f64) -> String {
    use colored::Colorize;
    let percent = format!("{:.0}% confident", confidence * 100.0);
    let badge = if confidence >= 0.8 {
        percent.red()
    } else if confidence >= 0.5 {
        percent.yellow()
    } else {
        percent.dimmed()
    };
    badge.to_string()
}

impl crate::executor::Executor {
    /// Judges one config's translations, streaming findings above a live
    /// progress bar as its verdict arrives.
    pub(crate) async fn stream_llm_judge(
        &self,
        judge: &Judge,
        translations: &std::sync::Arc<Translations>,
    ) -> Result<(), crate::error::Error> {
        let bar = judge_progress_bar();
        let progress = BarProgress(bar.clone());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Diagnostic<FileId>>();

        // The judge drives the requests while the drain prints findings as they
        // land; both run concurrently in this task, and the drain ends when the
        // judge future drops its channel sender.
        let judge_future = async move {
            let mut sink = |diagnostic| {
                let _ = tx.send(diagnostic);
            };
            stream(judge, translations.as_ref(), &progress, &mut sink).await
        };
        let drain_future = async {
            // In a terminal, print findings above the live bar; otherwise the
            // bar is hidden (and would swallow `println`), so emit normally.
            let interactive = std::io::IsTerminal::is_terminal(&std::io::stderr());
            while let Some(diagnostic) = rx.recv().await {
                if interactive {
                    let rendered = self.diagnostic_printer.render(&diagnostic).await?;
                    bar.println(rendered.trim_end());
                } else {
                    self.diagnostic_printer.emit(&diagnostic).await?;
                }
            }
            Ok::<_, crate::error::Error>(())
        };
        let (stats, drained) = tokio::join!(judge_future, drain_future);
        let stats = stats?;
        drained?;

        bar.finish_and_clear();
        tracing::info!(
            judged = stats.judged,
            cached = stats.cached,
            failed = stats.failed,
            flagged = stats.flagged,
            suppressed = stats.suppressed,
            "llm judge finished"
        );
        Ok(())
    }
}

/// Builds a key-counting progress bar hidden when stderr is not a terminal.
fn judge_progress_bar() -> indicatif::ProgressBar {
    use std::io::IsTerminal;
    let bar = indicatif::ProgressBar::new(0);
    if std::io::stderr().is_terminal() {
        let style = indicatif::ProgressStyle::with_template(
            "{spinner:.cyan} llm-judge: judging {bar:30.cyan/blue} {pos}/{len} keys ({eta})",
        )
        .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar());
        bar.set_style(style);
    } else {
        bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
    }
    bar.enable_steady_tick(std::time::Duration::from_millis(120));
    bar
}

/// Builds a note diagnostic for one finding, pointing at the flagged language's
/// translation string.
fn diagnostic_for(
    spans: &HashMap<&str, KeySpans<'_>>,
    finding: &globetrotter_llm_judge::Finding,
) -> Option<Diagnostic<FileId>> {
    let key_spans = spans.get(finding.key.as_str())?;

    let label = match key_spans.language_spans.get(finding.language.as_str()) {
        Some(span) => Label::primary(key_spans.file_id, span.clone())
            .with_message(format!("`{}` translation", finding.language)),
        None => {
            Label::primary(key_spans.file_id, key_spans.key_span.clone()).with_message("this key")
        }
    };

    Some(
        Diagnostic::note()
            .with_code(LintCode::LlmDrift)
            .with_message(format!(
                "`{}`: `{}` may tell users something different ({}): {}",
                finding.key,
                finding.language,
                confidence_badge(finding.confidence),
                finding.problem
            ))
            .with_labels(vec![label]),
    )
}
