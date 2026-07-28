use crate::{
    config::{
        settings::{Settings, SettingsLayer},
        v1::{self as config, PathOrGlobPattern},
    },
    error::{self, Error, FailedWithErrors, IoError, OutputError},
    model,
    progress::Logger,
};
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use futures::future::{Future, TryFutureExt};
use futures::stream::{self, StreamExt, TryStreamExt};
use globetrotter_model::{
    diagnostics::{DiagnosticExt, FileId, Spanned, ToDiagnostics},
    lint::LintOptions,
    validation::ValidationOptions,
};
use itertools::Itertools;
use normalize_path::NormalizePath;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

/// Parameters for the cross-cutting lint checks beyond per-key validation.
#[derive(Debug, Clone, Default)]
pub struct LintParams {
    /// Whether to report duplicate translations (across keys and within a key).
    pub detect_duplicates: bool,
    /// Source directories to scan for unused keys; empty disables the check.
    pub usages: Vec<PathBuf>,
    /// LLM-judged translation-consistency review; `None` disables the check.
    ///
    /// Requires the `llm-judge` feature to be enabled in this build; otherwise
    /// a request is ignored with a warning.
    pub llm_judge: Option<LlmJudgeParams>,
}

/// Settings for the LLM-judged translation-consistency review.
///
/// Kept independent of the `llm-judge` feature so callers (e.g. the CLI) can
/// always construct [`LintParams`]; requests only go out when the feature is
/// compiled in. Field semantics match `globetrotter_llm_judge::Options` (not
/// linked: that crate is absent from default-feature builds).
#[derive(Debug, Clone)]
pub struct LlmJudgeParams {
    /// Base URL of the OpenAI-compatible endpoint.
    pub base_url: String,
    /// Model name as known to the endpoint.
    pub model: String,
    /// Environment variable read for the API key.
    pub api_key_env: String,
    /// Maximum number of in-flight requests.
    pub concurrency: usize,
    /// Sampling temperature.
    pub temperature: f32,
    /// Reasoning effort for models that support it; `None` sends none.
    pub effort: Option<LlmJudgeEffort>,
    /// Prompt template overriding the built-in one.
    pub template: Option<String>,
    /// Findings below this confidence are suppressed; `0.0` reports everything.
    pub min_confidence: f64,
    /// Verdict cache location; `None` uses the OS user cache directory.
    pub cache_dir: Option<PathBuf>,
    /// Maximum number of cached verdicts kept on disk; `0` disables caching.
    pub cache_capacity: usize,
}

/// Reasoning effort requested from the judge model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LlmJudgeEffort {
    /// Minimal reasoning; fastest.
    Low,
    /// Balanced reasoning; the tested sweet spot for drift detection.
    #[default]
    Medium,
    /// Maximal reasoning; slowest.
    High,
}

pub(crate) async fn write_to_file(path: &Path, data: impl AsRef<[u8]>) -> Result<PathBuf, IoError> {
    use tokio::io::AsyncWriteExt;

    let err = |source: std::io::Error| IoError::new(path, source);

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(err)?;
    }

    let output_file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .await
        .map_err(err)?;

    let mut writer = tokio::io::BufWriter::new(output_file);
    writer.write_all(data.as_ref()).await.map_err(err)?;
    writer.flush().await.map_err(err)?;

    path.canonicalize().map_err(err)
}

pub(crate) fn resolve_path(base_dir: Option<&Path>, path: &Path) -> PathBuf {
    let output_path = match base_dir {
        None => path.to_path_buf(),
        Some(_) if path.is_absolute() => path.to_path_buf(),
        Some(base_dir) => base_dir.join(path),
    };
    output_path.normalize()
}

pub(crate) fn resolve_input_paths<'a>(
    base_dir: Option<&'a Path>,
    path_or_glob_pattern: &'a Spanned<PathOrGlobPattern>,
    file_id: Option<FileId>,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> Vec<Result<PathBuf, Error>> {
    let input_path = resolve_path(base_dir, &PathBuf::from(path_or_glob_pattern.as_ref()));
    let options = glob::MatchOptions {
        case_sensitive: false,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };
    let input_path = input_path.to_string_lossy().to_string();

    let entries = match glob::glob_with(input_path.as_str(), options) {
        Err(source) => {
            return vec![Err(Error::Pattern {
                source,
                path: input_path,
            })];
        }
        Ok(entries) => entries,
    };

    let valid_entries: Vec<_> = entries
        .into_iter()
        .map(|entry| match entry {
            Err(source) => Err(Error::Glob {
                source,
                path: input_path.clone(),
            }),
            Ok(input_path) => Ok(input_path),
        })
        .dedup_by(|a, b| match (a, b) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        })
        .collect();

    if valid_entries.is_empty() {
        let mut diagnostic = Diagnostic::warning_or_error(strict)
            .with_message(format!("pattern {input_path:?} did not match input"));

        if let Some(file_id) = file_id {
            diagnostic = diagnostic.with_labels(vec![
                Label::primary(file_id, path_or_glob_pattern.span.clone())
                    .with_message("this file path or glob pattern matched zero files"),
            ]);
        }
        diagnostics.push(diagnostic);

        if let Some(file_id) = file_id {
            let diagnostic = Diagnostic::note().with_labels(vec![
                Label::secondary(file_id, path_or_glob_pattern.span.clone())
                    .with_message(format!("resolves to {input_path:?}")),
            ]);
            diagnostics.push(diagnostic);
        }
    }

    valid_entries
}

type OutputFuture<'a> = Pin<Box<dyn Future<Output = Result<(), OutputError>> + 'a>>;

type TranslationResult = (
    config::Input,
    PathBuf,
    usize,
    model::Translations,
    Vec<Diagnostic<FileId>>,
);

fn combine_translations(
    translations: Vec<TranslationResult>,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> model::Translations {
    // check for duplicate keys across translation files
    let duplicate_keys = translations
        .iter()
        .flat_map(|res| (res.3).0.keys())
        .duplicates();

    for duplicate_key in duplicate_keys {
        let occurrences = translations
            .iter()
            .flat_map(|res| (res.3).0.keys().map(|key| (key.span.clone(), res.2)))
            .collect();

        let diagnostic = error::DuplicateKeyError {
            key: duplicate_key.as_ref().clone(),
            occurrences,
        };

        diagnostics.extend(diagnostic.to_diagnostics(true));
    }

    // combine all the translations
    model::Translations(
        translations
            .into_iter()
            .flat_map(|res| (res.3).0.into_iter())
            .collect(),
    )
}

/// Truncate `translations` to its first `max_keys` keys, warning about what is
/// dropped; `None` is a no-op. See [`Executor::max_keys`].
fn limit_keys(max_keys: Option<usize>, translations: &mut model::Translations) {
    let Some(max_keys) = max_keys else {
        return;
    };
    let total = translations.0.len();
    if total > max_keys {
        translations.0.truncate(max_keys);
        tracing::warn!(
            "processing only the first {max_keys} of {total} translation keys (max-keys limit)"
        );
    }
}

fn tally(severity: Severity, num_errors: &mut usize, num_warnings: &mut usize) {
    match severity {
        Severity::Bug | Severity::Error => *num_errors += 1,
        Severity::Warning => *num_warnings += 1,
        Severity::Note | Severity::Help => {}
    }
}

/// Drives loading, validation, and output generation for a set of configs.
pub struct Executor {
    /// The caller's settings overrides, applied over each config's own
    /// settings by [`Settings::resolve`].
    pub overrides: SettingsLayer,
    /// Base directory used to render output paths relative for display.
    pub global_base_dir_for_display: Option<PathBuf>,
    /// Handlebars engine used to template output paths.
    pub handlebars: handlebars::Handlebars<'static>,
    /// Renders diagnostics produced during execution.
    pub diagnostic_printer: crate::diagnostics::Printer,
    /// Formats progress log lines.
    pub logger: Logger,
    /// Process only the first N translation keys of each config; `None`
    /// processes everything.
    ///
    /// A debugging aid: it lets a change (or the LLM judge) be tried against a
    /// small subset of a real corpus before paying for a full run. Truncation
    /// is warned about, never silent.
    pub max_keys: Option<usize>,
}

impl Executor {
    /// Create a new executor for the given configs and diagnostic printer.
    #[must_use]
    pub fn new<F>(
        configs: &config::Configs<F>,
        diagnostic_printer: crate::diagnostics::Printer,
    ) -> Self {
        let logger = Logger::new(configs);
        Self {
            overrides: SettingsLayer::default(),
            global_base_dir_for_display: None,
            handlebars: handlebars::Handlebars::default(),
            diagnostic_printer,
            logger,
            max_keys: None,
        }
    }

    /// The lint-time `strict` value.
    ///
    /// Lint reports warnings by default regardless of a config's `strict`
    /// (which governs generation); only an explicit override escalates.
    fn lint_strict(&self) -> bool {
        self.overrides.strict.unwrap_or(false)
    }

    async fn read_translation_file(
        &self,
        input: (config::Input, PathBuf, Option<PathBuf>),
    ) -> Result<(config::Input, PathBuf, FileId, String, Option<PathBuf>), Error> {
        let (input, input_path, relative_base_dir) = input;

        let input_path = tokio::fs::canonicalize(&input_path)
            .await
            .map_err(|source| IoError::new(input_path, source))?;

        tracing::debug!(path = ?input_path, "reading translations");

        let raw_translations = tokio::fs::read_to_string(&input_path)
            .await
            .map_err(|source| IoError::new(&input_path, source))?;

        let source_file_path = self
            .global_base_dir_for_display
            .as_ref()
            .and_then(|base_dir| pathdiff::diff_paths(&input_path, base_dir))
            .unwrap_or(input_path.clone());
        let file_id = self
            .diagnostic_printer
            .add_source_file(
                &source_file_path,
                // input.path_or_glob_pattern.as_ref().to_string(),
                raw_translations.clone(),
            )
            .await;

        Ok((
            input,
            input_path,
            file_id,
            raw_translations,
            relative_base_dir,
        ))
    }

    async fn process_translation_file(
        &self,
        input: (config::Input, PathBuf, FileId, String, Option<PathBuf>),
        strict: bool,
    ) -> Result<
        (
            config::Input,
            PathBuf,
            FileId,
            model::Translations,
            Vec<Diagnostic<usize>>,
        ),
        Error,
    > {
        let (input, input_path, file_id, raw_translations, relative_base_dir) = input;
        let handle = tokio::task::spawn_blocking(move || {
            let mut diagnostics = vec![];

            let mut translations = match model::Translations::from_str(
                &raw_translations,
                file_id,
                strict,
                &mut diagnostics,
            ) {
                Err(err) => {
                    diagnostics.extend(err.to_diagnostics(file_id));
                    model::Translations::default()
                }
                Ok(translations) => translations,
            };

            let mut prefix: Vec<String> = Vec::new();

            if input
                .prepend_relative_path
                .as_deref()
                .copied()
                .unwrap_or(false)
            {
                if let Some(base_dir) = relative_base_dir.as_ref()
                    && let Some(rel_path) = pathdiff::diff_paths(&input_path, base_dir)
                {
                    let mut components: Vec<String> = rel_path
                        .components()
                        .filter_map(|c| {
                            use std::path::Component;
                            match c {
                                Component::Normal(os) => Some(os.to_string_lossy().to_string()),
                                _ => None,
                            }
                        })
                        .collect();

                    if let Some(last) = components.last_mut()
                        && let Some(stripped) = Path::new(last).file_stem()
                    {
                        *last = stripped.to_string_lossy().to_string();
                    }

                    prefix.extend(components.into_iter().filter(|p| !p.is_empty()));
                }
            } else if input.prepend_filename.as_deref().copied().unwrap_or(false) {
                let file_stem = input_path
                    .file_stem()
                    .map(|name| name.to_string_lossy().to_string());
                if let Some(file_stem) = file_stem
                    && !file_stem.is_empty()
                {
                    prefix.push(file_stem);
                }
            }

            if let Some(extra_prefix) = input
                .prefix
                .as_ref()
                .map(|prefix| prefix.as_ref().as_str())
                .filter(|extra_prefix| !extra_prefix.is_empty())
            {
                prefix.push(extra_prefix.to_string());
            }

            let separator = input
                .separator
                .as_ref()
                .map_or(".", |sep| sep.as_ref().as_str());

            if !prefix.is_empty() {
                translations.0 = translations
                    .0
                    .into_iter()
                    .map(|(key, value)| {
                        let prefixed_key = prefix
                            .iter()
                            .map(String::as_str)
                            .chain([key.as_ref().as_str()])
                            .join(separator);
                        (Spanned::new(key.span, prefixed_key), value)
                    })
                    .collect();
            }

            Ok::<_, Error>((input, input_path, file_id, translations, diagnostics))
        });

        handle.await?
    }

    fn unique_input_paths<'a>(
        inputs: &'a [config::Input],
        base_dir: Option<&'a Path>,
        strict: bool,
        file_id: Option<FileId>,
        diagnostics: &'a mut Vec<Diagnostic<FileId>>,
    ) -> impl Iterator<Item = Result<(config::Input, PathBuf), Error>> + use<'a> {
        inputs
            .iter()
            .flat_map(move |input| {
                use std::collections::HashSet;

                // resolve input files
                let input_paths = resolve_input_paths(
                    base_dir,
                    &input.path_or_glob_pattern,
                    file_id,
                    strict,
                    diagnostics,
                );

                // resolve excluded files
                let exclude: HashSet<PathBuf> = input
                    .exclude
                    .iter()
                    .flat_map(|exclude| {
                        resolve_input_paths(base_dir, exclude, file_id, strict, diagnostics)
                    })
                    .filter_map(Result::ok)
                    .collect();

                input_paths
                    .into_iter()
                    .filter_ok(move |input_path| !exclude.contains(input_path))
                    .map_ok(|input_path| (input.clone(), input_path))
            })
            // remove duplicates (same input config and input file)
            .dedup_by(|a, b| match (a, b) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            })
    }

    /// Resolve, read, and parse all input translation files for a config.
    ///
    /// Diagnostics from input resolution are pushed onto `diagnostics`; per-file
    /// parse diagnostics travel in the returned tuples.
    async fn load_translations(
        &self,
        config_file: &config::ConfigFile<FileId>,
        strict: bool,
        diagnostics: &mut Vec<Diagnostic<FileId>>,
    ) -> Result<Vec<TranslationResult>, Error> {
        let inputs: Vec<_> = Self::unique_input_paths(
            &config_file.config.inputs,
            config_file.config_dir.as_deref(),
            strict,
            config_file.file_id,
            diagnostics,
        )
        .collect();

        stream::iter(inputs)
            .map(|input| async {
                let (input, input_path) = input?;
                let input_path = tokio::fs::canonicalize(&input_path)
                    .await
                    .map_err(|source| IoError::new(input_path, source))?;
                let relative_base_dir = config_file.config_dir.clone();
                Ok::<_, Error>((input, input_path, relative_base_dir))
            })
            .buffer_unordered(16)
            .and_then(|input| async { self.read_translation_file(input).await })
            .and_then(|input| async { self.process_translation_file(input, strict).await })
            .try_collect::<Vec<_>>()
            .await
    }

    /// Execute a single configuration and generate all configured outputs.
    ///
    /// # Errors
    ///
    /// Returns an error if resolving or reading input files, parsing or
    /// validating translations, emitting diagnostics, or generating any
    /// outputs fails.
    #[allow(
        clippy::too_many_lines,
        reason = "single linear pipeline for one config; splitting would obscure the sequential flow"
    )]
    pub async fn execute_config(
        &self,
        config_file: Arc<config::ConfigFile<FileId>>,
    ) -> Result<(), Error> {
        tracing::debug!(name = config_file.config.name.as_ref(), "executing");

        let settings = Settings::resolve(&config_file.config.settings, &self.overrides);

        let mut diagnostics = vec![];
        let mut translations = self
            .load_translations(&config_file, settings.strict, &mut diagnostics)
            .await?;

        let mut num_errors = 0;
        let mut num_warnings = 0;
        for diagnostic in diagnostics
            .drain(..)
            .chain(translations.iter_mut().flat_map(|res| res.4.drain(..)))
        {
            match diagnostic.severity {
                Severity::Bug | Severity::Error => num_errors += 1,
                Severity::Warning => num_warnings += 1,
                Severity::Note | Severity::Help => {}
            }
            self.diagnostic_printer.emit(&diagnostic).await?;
        }
        if num_errors > 0 {
            return Err(FailedWithErrors {
                num_errors,
                num_warnings,
            }
            .into());
        }

        let max_keys = self.max_keys;
        let (translations, mut diagnostics) = tokio::task::spawn_blocking(move || {
            let mut diagnostics: Vec<Diagnostic<FileId>> = vec![];
            let mut translations = combine_translations(translations, &mut diagnostics);
            limit_keys(max_keys, &mut translations);
            Ok::<_, Error>((Arc::new(translations), diagnostics))
        })
        .await??;

        let mut num_errors = 0;
        let mut num_warnings = 0;
        for diagnostic in diagnostics.drain(..) {
            match diagnostic.severity {
                Severity::Bug | Severity::Error => num_errors += 1,
                Severity::Warning => num_warnings += 1,
                Severity::Note | Severity::Help => {}
            }
            self.diagnostic_printer.emit(&diagnostic).await?;
        }
        if num_errors > 0 {
            return Err(FailedWithErrors {
                num_errors,
                num_warnings,
            }
            .into());
        }

        // validate all translations
        let validate_translations = tokio::task::spawn_blocking({
            let translations = Arc::clone(&translations);
            let config_file = Arc::clone(&config_file);
            let settings = settings.clone();
            move || {
                let mut diagnostics = vec![];
                let options = ValidationOptions {
                    required_languages: &config_file.config.languages,
                    template_engine: settings.template_engine.as_ref(),
                    strict: settings.strict,
                    check_templates: settings.check_templates,
                };
                translations.validate(
                    &config_file.config.name,
                    config_file.file_id,
                    &mut diagnostics,
                    &options,
                );
                Ok::<_, Error>(diagnostics)
            }
        });

        let output_futures: Vec<OutputFuture<'_>> = vec![
            Box::pin(
                self.generate_json_outputs(&*config_file, &translations, &settings)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "typescript")]
            Box::pin(
                self.generate_typescript_outputs(&*config_file, &translations, &settings)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "rust")]
            Box::pin(
                self.generate_rust_outputs(&*config_file, &translations, &settings)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "python")]
            Box::pin(
                self.generate_python_outputs(&*config_file, &translations, &settings)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "golang")]
            Box::pin(
                self.generate_golang_outputs(&*config_file, &translations, &settings)
                    .map_err(OutputError::from),
            ),
        ];

        let mut num_errors = 0;
        let mut num_warnings = 0;
        for diagnostic in validate_translations.await??.drain(..) {
            match diagnostic.severity {
                Severity::Bug | Severity::Error => num_errors += 1,
                Severity::Warning => num_warnings += 1,
                Severity::Note | Severity::Help => {}
            }
            let _ = self.diagnostic_printer.emit(&diagnostic).await;
        }
        if num_errors > 0 {
            return Err(FailedWithErrors {
                num_errors,
                num_warnings,
            }
            .into());
        }

        // wait for all outputs to complete
        futures::future::join_all(output_futures)
            .await
            .into_iter()
            .collect::<Result<(), _>>()?;

        Ok(())
    }

    /// Execute all configurations in the given list and generate outputs.
    ///
    /// # Errors
    ///
    /// Returns an error if reading translation files, validating configurations,
    /// emitting diagnostics, or generating any of the configured outputs fails.
    pub async fn execute(self, configs: config::Configs<FileId>) -> Result<Self, Error> {
        tracing::trace!(num_configs = configs.len(), "executing");

        stream::iter(configs)
            .map(|config_file| async move { Ok(Arc::new(config_file)) })
            .buffer_unordered(8)
            .try_for_each(|config| async { self.execute_config(config).await })
            .await?;

        Ok(self)
    }

    /// Lint a single configuration's translation files, emitting diagnostics.
    ///
    /// Unlike [`Self::execute_config`], no outputs are generated. Returns the
    /// number of error and warning diagnostics emitted.
    ///
    /// Findings are reported as warnings regardless of the config file's
    /// `strict` (which governs generation); only [`overrides`](Self::overrides)
    /// escalates them to errors.
    ///
    /// # Errors
    ///
    /// Returns an error if input files cannot be read or parsed, a spawned task
    /// fails to join, or emitting a diagnostic fails.
    pub async fn lint_config(
        &self,
        config_file: Arc<config::ConfigFile<FileId>>,
        params: &LintParams,
    ) -> Result<(usize, usize, Arc<model::Translations>), Error> {
        tracing::debug!(name = config_file.config.name.as_ref(), "linting");

        // Lint resolves like generation except for `strict`; see
        // `Self::lint_strict` for why the config's value is not consulted.
        let settings = Settings {
            strict: self.lint_strict(),
            ..Settings::resolve(&config_file.config.settings, &self.overrides)
        };

        let mut diagnostics = vec![];
        let mut translations = self
            .load_translations(&config_file, settings.strict, &mut diagnostics)
            .await?;

        let mut num_errors = 0;
        let mut num_warnings = 0;

        for diagnostic in diagnostics
            .drain(..)
            .chain(translations.iter_mut().flat_map(|res| res.4.drain(..)))
        {
            tally(diagnostic.severity, &mut num_errors, &mut num_warnings);
            self.diagnostic_printer.emit(&diagnostic).await?;
        }

        let max_keys = self.max_keys;
        let (translations, combine_diagnostics) = tokio::task::spawn_blocking(move || {
            let mut diagnostics = vec![];
            let mut translations = combine_translations(translations, &mut diagnostics);
            limit_keys(max_keys, &mut translations);
            (Arc::new(translations), diagnostics)
        })
        .await?;

        for diagnostic in &combine_diagnostics {
            tally(diagnostic.severity, &mut num_errors, &mut num_warnings);
            self.diagnostic_printer.emit(diagnostic).await?;
        }

        let lint_diagnostics = tokio::task::spawn_blocking({
            let translations = Arc::clone(&translations);
            let config_file = Arc::clone(&config_file);
            let detect_duplicates = params.detect_duplicates;
            move || {
                let mut diagnostics = vec![];
                let options = LintOptions {
                    required_languages: &config_file.config.languages,
                    template_engine: settings.template_engine.as_ref(),
                    strict: settings.strict,
                    detect_duplicates,
                };
                translations.lint(&mut diagnostics, &options);
                diagnostics
            }
        })
        .await?;

        for diagnostic in &lint_diagnostics {
            tally(diagnostic.severity, &mut num_errors, &mut num_warnings);
            self.diagnostic_printer.emit(diagnostic).await?;
        }

        Ok((num_errors, num_warnings, translations))
    }

    /// Lint all configurations' translation files.
    ///
    /// Every configuration is linted and its diagnostics emitted. When
    /// [`LintParams::usages`] is non-empty, keys not referenced anywhere in
    /// those directories are reported too. The call then fails if any issues
    /// (warnings or errors) were found.
    ///
    /// # Errors
    ///
    /// Returns an error if loading or parsing fails, scanning for usages fails,
    /// or — via [`FailedWithErrors`] — if any lint issues were found.
    pub async fn lint(
        self,
        configs: config::Configs<FileId>,
        params: &LintParams,
    ) -> Result<Self, Error> {
        tracing::trace!(num_configs = configs.len(), "linting");

        let scan_usages = !params.usages.is_empty();
        let excluded = if scan_usages {
            output_dirs(&configs)
        } else {
            BTreeSet::new()
        };

        let mut num_errors = 0;
        let mut num_warnings = 0;
        let mut defined_keys: Vec<crate::dead_keys::DefinedKey> = Vec::new();

        // Create the judge once up front (cheap: no request is made until keys
        // are judged), reusing its HTTP client and verdict cache across configs.
        #[cfg(feature = "llm-judge")]
        let llm_judge = match &params.llm_judge {
            Some(params) => Some(crate::llm_judge::judge(params)?),
            None => None,
        };
        #[cfg(not(feature = "llm-judge"))]
        if params.llm_judge.is_some() {
            tracing::warn!(
                "the LLM judge was requested but this build was compiled without the `llm-judge` feature; skipping"
            );
        }

        for config_file in configs {
            let config_file = Arc::new(config_file);
            let (errors, warnings, translations) =
                self.lint_config(Arc::clone(&config_file), params).await?;
            num_errors += errors;
            num_warnings += warnings;

            if scan_usages {
                for (key, translation) in &translations.0 {
                    defined_keys.push(crate::dead_keys::DefinedKey {
                        key: key.as_ref().clone(),
                        forms: key_forms(&config_file.config, key.as_ref()),
                        file_id: translation.file_id,
                        span: key.span.clone(),
                        allow: translation.allow.clone(),
                    });
                }
            }

            // Judge findings are emitted as notes and deliberately not tallied:
            // they are a review aid, not a pass/fail signal. They are streamed
            // above the live progress bar as each verdict arrives.
            #[cfg(feature = "llm-judge")]
            if let Some(judge) = llm_judge.as_ref() {
                self.stream_llm_judge(judge, &translations).await?;
            }
        }

        if scan_usages {
            let strict = self.lint_strict();
            let usages = params.usages.clone();
            let dead_diagnostics = tokio::task::spawn_blocking(move || {
                crate::dead_keys::find_unused_keys(&defined_keys, &usages, &excluded, strict)
            })
            .await?
            .map_err(|source| IoError::new("<usages>", source))?;

            for diagnostic in &dead_diagnostics {
                tally(diagnostic.severity, &mut num_errors, &mut num_warnings);
                self.diagnostic_printer.emit(diagnostic).await?;
            }
        }

        if num_errors > 0 || num_warnings > 0 {
            return Err(FailedWithErrors {
                num_errors,
                num_warnings,
            }
            .into());
        }

        Ok(self)
    }
}

/// All canonical forms a usage of `key` may take across a config's enabled
/// output targets: the dotted key (used by JSON/TypeScript) plus each target's
/// generated identifier (e.g. the Rust enum variant `TranslationGreeting`).
fn key_forms(config: &config::Config, key: &str) -> Vec<String> {
    let mut forms = vec![key.to_string()];
    forms.extend(target_identifiers(config, key));
    forms
}

/// The generated identifiers for `key` across the config's typed output targets.
#[cfg(feature = "rust")]
fn target_identifiers(config: &config::Config, key: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    if config.outputs.rust.is_some() {
        identifiers.push(crate::rust::key_to_rust_enum_variant(key));
    }
    identifiers
}

#[cfg(not(feature = "rust"))]
fn target_identifiers(_config: &config::Config, _key: &str) -> Vec<String> {
    Vec::new()
}

fn insert_output_dir(dirs: &mut BTreeSet<PathBuf>, base: Option<&Path>, path: &Path) {
    let path = resolve_path(base, path);
    if let Some(parent) = path.parent()
        && let Ok(canonical) = parent.canonicalize()
    {
        dirs.insert(canonical);
    }
}

/// Canonicalized directories holding generated output, excluded from the
/// dead-key scan so generated files do not mark every key as used.
fn output_dirs(configs: &config::Configs<FileId>) -> BTreeSet<PathBuf> {
    let mut dirs = BTreeSet::new();
    for config_file in configs {
        let base = config_file.config_dir.as_deref();
        for output in &config_file.config.outputs.json {
            insert_output_dir(&mut dirs, base, output.path.as_ref());
        }

        #[cfg(feature = "typescript")]
        if let Some(output) = &config_file.config.outputs.typescript {
            for interface in &output.interface_type {
                insert_output_dir(&mut dirs, base, &interface.path);
            }
        }

        #[cfg(feature = "rust")]
        if let Some(output) = &config_file.config.outputs.rust {
            for path in &output.output_paths {
                insert_output_dir(&mut dirs, base, path);
            }
        }

        #[cfg(feature = "golang")]
        if let Some(output) = &config_file.config.outputs.golang {
            for path in &output.output_paths {
                insert_output_dir(&mut dirs, base, path);
            }
        }

        #[cfg(feature = "python")]
        if let Some(output) = &config_file.config.outputs.python {
            for path in &output.output_paths {
                insert_output_dir(&mut dirs, base, path);
            }
        }
    }
    dirs
}

/// Resolve all unique translation input file paths referenced by `configs`.
///
/// Patterns that match no files push a diagnostic into `diagnostics`. The
/// returned paths are sorted and de-duplicated but not canonicalized.
#[must_use]
pub fn resolve_input_files(
    configs: &config::Configs<FileId>,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for config_file in configs {
        let resolved: Vec<PathBuf> = Executor::unique_input_paths(
            &config_file.config.inputs,
            config_file.config_dir.as_deref(),
            strict,
            config_file.file_id,
            diagnostics,
        )
        .filter_map(Result::ok)
        .map(|(_input, path)| path)
        .collect();
        paths.extend(resolved);
    }
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use color_eyre::eyre;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir(prefix: &str) -> eyre::Result<PathBuf> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);

        let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "globetrotter-{prefix}-{}-{nanos}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    #[tokio::test]
    async fn prepend_filename_prefixes_with_file_stem() -> eyre::Result<()> {
        let configs: config::Configs<FileId> = vec![];
        let printer = crate::diagnostics::Printer::default();
        let executor = Executor::new(&configs, printer);

        let input = config::Input::new("translations/a.toml").with_prepend_filename(true);
        let input_path = PathBuf::from("/base/dialogs/delete-user.toml");
        let file_id: FileId = 0;
        let raw_translations = r#"
            [section]
            en = "Hello"
        "#;

        let (_input, _path, _file_id, translations, _diagnostics) = executor
            .process_translation_file(
                (input, input_path, file_id, raw_translations.into(), None),
                true,
            )
            .await?;

        let keys: Vec<_> = translations
            .0
            .keys()
            .map(|k| k.as_ref().as_str().to_string())
            .collect();

        assert_eq!(keys, vec!["delete-user.section".to_string()]);

        Ok(())
    }

    #[tokio::test]
    async fn prepend_relative_path_prefixes_with_full_path_segments() -> eyre::Result<()> {
        let configs: config::Configs<FileId> = vec![];
        let printer = crate::diagnostics::Printer::default();
        let executor = Executor::new(&configs, printer);

        let input =
            config::Input::new("translations/airtype/**/*.toml").with_prepend_relative_path(true);

        let base_dir = PathBuf::from("/workspace/translations/airtype");
        let input_path = base_dir.join("dialogs/chat/too-many-files.toml");
        let file_id: FileId = 0;
        let raw_translations = r#"
            [section]
            en = "Hello"
        "#;

        let (_input, _path, _file_id, translations, _diagnostics) = executor
            .process_translation_file(
                (
                    input,
                    input_path,
                    file_id,
                    raw_translations.into(),
                    Some(base_dir),
                ),
                true,
            )
            .await?;

        let keys: Vec<_> = translations
            .0
            .keys()
            .map(|k| k.as_ref().as_str().to_string())
            .collect();

        assert_eq!(
            keys,
            vec!["dialogs.chat.too-many-files.section".to_string()]
        );

        Ok(())
    }

    #[tokio::test]
    async fn prepend_relative_path_disabled_preserves_existing_behavior() -> eyre::Result<()> {
        let configs: config::Configs<FileId> = vec![];
        let printer = crate::diagnostics::Printer::default();
        let executor = Executor::new(&configs, printer);

        let input = config::Input::new("translations/upload.toml").with_prefix("upload");
        let input_path = PathBuf::from("/base/upload.toml");
        let file_id: FileId = 0;
        let raw_translations = r#"
            [message]
            en = "Hello"
        "#;

        let (_input, _path, _file_id, translations, _diagnostics) = executor
            .process_translation_file(
                (input, input_path, file_id, raw_translations.into(), None),
                true,
            )
            .await?;

        let keys: Vec<_> = translations
            .0
            .keys()
            .map(|k| k.as_ref().as_str().to_string())
            .collect();

        assert_eq!(keys, vec!["upload.message".to_string()]);

        Ok(())
    }

    #[test]
    fn unique_input_paths_respects_exclude_patterns() -> eyre::Result<()> {
        let dir = temp_dir("exclude-patterns")?;
        let keep = dir.join("keep.toml");
        let skip = dir.join("skip.toml");
        std::fs::write(&keep, "[a]\nen = \"Hello\"\n")?;
        std::fs::write(&skip, "[b]\nen = \"Bye\"\n")?;

        let input = config::Input::new(dir.join("*.toml").to_string_lossy().into_owned())
            .with_exclude([skip.to_string_lossy().into_owned()]);
        let mut diagnostics = Vec::new();

        let mut resolved: Vec<PathBuf> =
            Executor::unique_input_paths(&[input], None, true, None, &mut diagnostics)
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|(_input, path)| path)
                .collect();
        resolved.sort();

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(resolved, vec![keep]);

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn output_dirs_include_all_generated_output_parents() -> eyre::Result<()> {
        let base = temp_dir("output-dirs")?;

        let json_dir = base.join("generated/json");
        std::fs::create_dir_all(&json_dir)?;

        #[cfg(feature = "typescript")]
        let ts_dir = {
            let dir = base.join("generated/ts");
            std::fs::create_dir_all(&dir)?;
            dir
        };

        #[cfg(feature = "rust")]
        let rust_dir = {
            let dir = base.join("generated/rust");
            std::fs::create_dir_all(&dir)?;
            dir
        };

        #[cfg(feature = "golang")]
        let go_dir = {
            let dir = base.join("generated/go");
            std::fs::create_dir_all(&dir)?;
            dir
        };

        #[cfg(feature = "python")]
        let py_dir = {
            let dir = base.join("generated/python");
            std::fs::create_dir_all(&dir)?;
            dir
        };

        let outputs = config::Outputs::new()
            .with_json([config::JsonOutputConfig::new("generated/json/en.json")]);

        #[cfg(feature = "typescript")]
        let outputs = outputs.with_typescript(globetrotter_typescript::OutputConfig {
            interface_type: vec![globetrotter_typescript::config::InterfaceTypeOutputConfig {
                path: PathBuf::from("generated/ts/translations.d.ts"),
            }],
        });

        #[cfg(feature = "rust")]
        let outputs = outputs.with_rust(globetrotter_rust::OutputConfig::new([PathBuf::from(
            "generated/rust/translations.rs",
        )]));

        #[cfg(feature = "golang")]
        let outputs = outputs.with_golang(globetrotter_golang::OutputConfig {
            output_paths: vec![PathBuf::from("generated/go/translations.go")],
        });

        #[cfg(feature = "python")]
        let outputs = outputs.with_python(globetrotter_python::OutputConfig {
            output_paths: vec![PathBuf::from("generated/python/translations.py")],
        });

        let configs = vec![config::ConfigFile {
            file_id: None,
            config_dir: Some(base.clone()),
            config: config::Config::new("demo")
                .with_input(config::Input::new("translations/*.toml"))
                .with_outputs(outputs),
        }];

        let dirs = output_dirs(&configs);

        assert!(dirs.contains(&json_dir.canonicalize()?));

        #[cfg(feature = "typescript")]
        assert!(dirs.contains(&ts_dir.canonicalize()?));

        #[cfg(feature = "rust")]
        assert!(dirs.contains(&rust_dir.canonicalize()?));

        #[cfg(feature = "golang")]
        assert!(dirs.contains(&go_dir.canonicalize()?));

        #[cfg(feature = "python")]
        assert!(dirs.contains(&py_dir.canonicalize()?));

        std::fs::remove_dir_all(base)?;
        Ok(())
    }
}
