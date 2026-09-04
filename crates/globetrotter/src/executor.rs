//! Translation loading, validation, linting, and output orchestration.
//!
//! Each config follows a staged pipeline: resolve and parse source files, stop
//! on file-local errors, merge catalogs while reporting cross-file conflicts,
//! validate the complete catalog, and only then generate outputs.

use crate::{
    config::{
        settings::{Settings, SettingsLayer},
        v1::{self as config, PathOrGlobPattern},
    },
    error::{self, Error, FailedWithErrors, IoError, OutputError, Tally},
    model,
    progress::{Logger, relative_to},
};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use futures::future::{Future, TryFutureExt};
use futures::stream::{self, StreamExt, TryStreamExt};
use globetrotter_model::{
    diagnostics::{DiagnosticExt, FileId, Spanned, ToDiagnostics},
    lint::{AllowEntry, LintOptions},
    validation::ValidationOptions,
};
use itertools::Itertools;
use normalize_path::NormalizePath;
use std::collections::{BTreeSet, HashSet};
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

pub(crate) async fn write_to_file(path: &Path, data: impl AsRef<[u8]>) -> Result<(), IoError> {
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
    writer.flush().await.map_err(err)
}

pub(crate) fn resolve_path(base_dir: Option<&Path>, path: &Path) -> PathBuf {
    let output_path = match base_dir {
        None => path.to_path_buf(),
        Some(_) if path.is_absolute() => path.to_path_buf(),
        Some(base_dir) => base_dir.join(path),
    };
    output_path.normalize()
}

/// The files a path or glob pattern expands to, relative to `base_dir`.
struct Expanded {
    /// The pattern as resolved against `base_dir`, for diagnostics.
    pattern: String,
    /// Each match, or the error that stopped expansion.
    paths: Vec<Result<PathBuf, Error>>,
}

/// Expands a path or glob pattern; the matches are what the file system
/// yields and are not canonicalized.
fn expand_pattern(base_dir: Option<&Path>, path_or_glob_pattern: &str) -> Expanded {
    let pattern = resolve_path(base_dir, Path::new(path_or_glob_pattern))
        .to_string_lossy()
        .into_owned();
    let options = glob::MatchOptions {
        case_sensitive: false,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };
    let paths = match glob::glob_with(&pattern, options) {
        Err(source) => vec![Err(Error::Pattern {
            source,
            path: pattern.clone(),
        })],
        Ok(entries) => entries
            .map(|entry| {
                entry.map_err(|source| Error::Glob {
                    source,
                    path: pattern.clone(),
                })
            })
            .collect(),
    };
    Expanded { pattern, paths }
}

/// Expands one input pattern, warning when it matches nothing.
pub(crate) fn resolve_input_paths(
    base_dir: Option<&Path>,
    path_or_glob_pattern: &Spanned<PathOrGlobPattern>,
    file_id: Option<FileId>,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> Vec<Result<PathBuf, Error>> {
    let Expanded { pattern, paths } = expand_pattern(base_dir, path_or_glob_pattern.as_ref());

    if paths.is_empty() {
        let mut diagnostic = Diagnostic::warning_or_error(strict)
            .with_message(format!("pattern {pattern:?} did not match input"));

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
                    .with_message(format!("resolves to {pattern:?}")),
            ]);
            diagnostics.push(diagnostic);
        }
    }

    paths
}

type OutputFuture<'a> = Pin<Box<dyn Future<Output = Result<(), OutputError>> + 'a>>;

/// A translation file selected by an input and registered for diagnostics.
struct SourceFile {
    /// The input that selected the file, which decides its key prefix.
    input: config::Input,
    /// The canonical path of the file.
    path: PathBuf,
    /// The diagnostic file id the contents were registered under.
    file_id: FileId,
    /// The raw TOML contents.
    contents: String,
}

/// One parsed translation file with its file-local diagnostics.
struct ParsedFile {
    file_id: FileId,
    translations: model::Translations,
    diagnostics: Vec<Diagnostic<FileId>>,
}

/// The key prefix segments an input adds to every key of `path`.
///
/// `base_dir` is the config directory that relative-path prefixes are taken
/// from.
fn key_prefix(input: &config::Input, path: &Path, base_dir: Option<&Path>) -> Vec<String> {
    let mut prefix: Vec<String> = Vec::new();

    if input
        .prepend_relative_path
        .as_deref()
        .copied()
        .unwrap_or(false)
    {
        if let Some(base_dir) = base_dir
            && let Some(rel_path) = pathdiff::diff_paths(path, base_dir)
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
        let file_stem = path
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

    prefix
}

/// Parses one translation file and applies its input's key prefix.
///
/// A file that fails to parse yields an empty catalog and the parse error as
/// a diagnostic, so every file's problems are reported in one run.
fn parse_source_file(file: &SourceFile, base_dir: Option<&Path>, strict: bool) -> ParsedFile {
    let mut diagnostics = vec![];
    let mut translations =
        match model::Translations::from_str(&file.contents, file.file_id, strict, &mut diagnostics)
        {
            Err(err) => {
                diagnostics.extend(err.to_diagnostics(file.file_id));
                model::Translations::default()
            }
            Ok(translations) => translations,
        };

    let prefix = key_prefix(&file.input, &file.path, base_dir);
    if !prefix.is_empty() {
        let separator = file
            .input
            .separator
            .as_ref()
            .map_or(".", |sep| sep.as_ref().as_str());
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

    ParsedFile {
        file_id: file.file_id,
        translations,
        diagnostics,
    }
}

fn combine_translations(
    files: Vec<ParsedFile>,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> model::Translations {
    // Detect cross-file duplicates before map insertion can hide an occurrence.
    let duplicate_keys = files
        .iter()
        .flat_map(|file| file.translations.0.keys())
        .duplicates();

    for duplicate_key in duplicate_keys {
        let occurrences = files
            .iter()
            .flat_map(|file| {
                file.translations
                    .0
                    .keys()
                    .filter(|key| key == &duplicate_key)
                    .map(|key| (key.span.clone(), file.file_id))
            })
            .collect();

        let diagnostic = error::DuplicateKeyError {
            key: duplicate_key.as_ref().clone(),
            occurrences,
        };

        diagnostics.extend(diagnostic.to_diagnostics(true));
    }

    // Merge only after diagnostics capture the source occurrences.
    model::Translations(
        files
            .into_iter()
            .flat_map(|file| file.translations.0.into_iter())
            .collect(),
    )
}

/// Assembles one configuration's catalog from its parsed input files.
///
/// Every file's keys are merged, reporting any key defined more than once, the
/// optional `max_keys` limit is applied, and the config's own `allow` entries
/// are added to every key.
/// Adding them here, rather than in one consumer, is what lets every pass that
/// reads a key's `allow` — linting, dead-key detection, the LLM judge — see the
/// same suppressions.
fn assemble_catalog(
    files: Vec<ParsedFile>,
    max_keys: Option<usize>,
    allow: &BTreeSet<AllowEntry>,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> model::Translations {
    let mut translations = combine_translations(files, diagnostics);
    limit_keys(max_keys, &mut translations);
    translations.extend_allow(allow);
    translations
}

/// Truncates `translations` to its first `max_keys` keys, warning about what is
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

/// Drives the staged translation pipeline for a set of configs.
///
/// Configs may execute concurrently. Within one config, source diagnostics and
/// catalog validation must succeed before any output future is polled.
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
    /// Process only the first `N` translation keys of each config; `None`
    /// processes everything.
    ///
    /// A debugging aid: it lets a change (or the LLM judge) be tried against a
    /// small subset of a real corpus before paying for a full run. Truncation
    /// is warned about, never silent.
    pub max_keys: Option<usize>,
}

impl Executor {
    /// Creates an executor with default settings overrides and aligned logging.
    ///
    /// The remaining fields are public, so a caller sets what it needs with
    /// struct update syntax: `Executor { max_keys: Some(25), ..Executor::new(…) }`.
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

    /// Emits every diagnostic and counts what was emitted.
    async fn emit_all(&self, diagnostics: &[Diagnostic<FileId>]) -> Result<Tally, Error> {
        let mut tally = Tally::default();
        for diagnostic in diagnostics {
            tally.record(diagnostic.severity);
            self.diagnostic_printer.emit(diagnostic).await?;
        }
        Ok(tally)
    }

    /// The path as it is logged: absolute, or relative to the common base
    /// directory of all configs.
    pub(crate) fn display_path(&self, path: &Path, settings: &Settings) -> String {
        if settings.print_absolute_paths {
            path.display().to_string()
        } else {
            relative_to(self.global_base_dir_for_display.as_deref(), path)
                .display()
                .to_string()
        }
    }

    /// Writes one generated file, or only announces it during a dry run.
    ///
    /// Returns the outcome to log after a config's aligned prefix.
    pub(crate) async fn write_output(
        &self,
        path: &Path,
        contents: &[u8],
        settings: &Settings,
    ) -> Result<String, IoError> {
        if settings.dry_run {
            return Ok(self.logger.dry_run_would_write(path).to_string());
        }
        write_to_file(path, contents).await?;
        Ok(format!("wrote {}", self.display_path(path, settings)))
    }

    /// Reads one translation file and registers it for diagnostics.
    async fn read_source_file(
        &self,
        input: config::Input,
        path: PathBuf,
    ) -> Result<SourceFile, Error> {
        let path = tokio::fs::canonicalize(&path)
            .await
            .map_err(|source| IoError::new(path, source))?;

        tracing::debug!(path = ?path, "reading translations");

        let contents = tokio::fs::read_to_string(&path)
            .await
            .map_err(|source| IoError::new(&path, source))?;

        let source_name = self
            .global_base_dir_for_display
            .as_ref()
            .and_then(|base_dir| pathdiff::diff_paths(&path, base_dir))
            .unwrap_or(path.clone());
        let file_id = self
            .diagnostic_printer
            .add_source_file(&source_name, contents.clone())
            .await;

        Ok(SourceFile {
            input,
            path,
            file_id,
            contents,
        })
    }

    /// The files every input selects, in input order, paired with their
    /// input.
    ///
    /// An exclusion that matches nothing is not reported, unlike an input, but
    /// an invalid exclusion pattern is still an error.
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
                let included = resolve_input_paths(
                    base_dir,
                    &input.path_or_glob_pattern,
                    file_id,
                    strict,
                    diagnostics,
                );
                let (excluded, errors): (HashSet<PathBuf>, Vec<Error>) = input
                    .exclude
                    .iter()
                    .flat_map(|exclude| expand_pattern(base_dir, exclude.as_ref()).paths)
                    .partition_result();

                errors.into_iter().map(Err).chain(
                    included
                        .into_iter()
                        .filter_ok(move |path| !excluded.contains(path))
                        .map_ok(|path| (input.clone(), path)),
                )
            })
            // Preserve distinct input policies while removing exact repeats.
            .dedup_by(|a, b| match (a, b) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            })
    }

    /// Resolves, reads, and parses all translation inputs for a config.
    ///
    /// Diagnostics from input resolution are pushed onto `diagnostics`; per-file
    /// parse diagnostics travel with the returned files.
    async fn load_translations(
        &self,
        config_file: &config::ConfigFile<FileId>,
        strict: bool,
        diagnostics: &mut Vec<Diagnostic<FileId>>,
    ) -> Result<Vec<ParsedFile>, Error> {
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
                let (input, path) = input?;
                self.read_source_file(input, path).await
            })
            .buffer_unordered(16)
            .and_then(|file| async {
                // Parsing is CPU-bound, so it runs off the async runtime.
                let base_dir = config_file.config_dir.clone();
                let parsed = tokio::task::spawn_blocking(move || {
                    parse_source_file(&file, base_dir.as_deref(), strict)
                })
                .await?;
                Ok(parsed)
            })
            .try_collect::<Vec<_>>()
            .await
    }

    /// Loads one config's input files and emits their diagnostics.
    async fn load_files(
        &self,
        config_file: &config::ConfigFile<FileId>,
        strict: bool,
    ) -> Result<(Vec<ParsedFile>, Tally), Error> {
        let mut diagnostics = vec![];
        let mut files = self
            .load_translations(config_file, strict, &mut diagnostics)
            .await?;

        // Emit per-file diagnostics before catalogs are merged.
        let mut tally = self.emit_all(&diagnostics).await?;
        for file in &mut files {
            tally += self.emit_all(&file.diagnostics).await?;
            file.diagnostics.clear();
        }
        Ok((files, tally))
    }

    /// Merges parsed files into one catalog and emits the cross-file
    /// diagnostics.
    async fn assemble(
        &self,
        config_file: &config::ConfigFile<FileId>,
        files: Vec<ParsedFile>,
    ) -> Result<(Arc<model::Translations>, Tally), Error> {
        // Assemble the catalog off the async runtime.
        let max_keys = self.max_keys;
        let allow = config_file.config.allow.clone();
        let (translations, diagnostics) = tokio::task::spawn_blocking(move || {
            let mut diagnostics: Vec<Diagnostic<FileId>> = vec![];
            let translations = assemble_catalog(files, max_keys, &allow, &mut diagnostics);
            (Arc::new(translations), diagnostics)
        })
        .await?;

        let tally = self.emit_all(&diagnostics).await?;
        Ok((translations, tally))
    }

    /// Executes one configuration and generates all configured outputs.
    ///
    /// # Errors
    ///
    /// Returns an error if resolving or reading input files, parsing or
    /// validating translations, emitting diagnostics, or generating any
    /// outputs fails.
    pub async fn execute_config(
        &self,
        config_file: Arc<config::ConfigFile<FileId>>,
    ) -> Result<(), Error> {
        tracing::debug!(name = config_file.config.name.as_ref(), "executing");

        // Resolve settings, then load every input file, stopping on
        // file-local errors before catalogs are merged.
        let settings = Settings::resolve(&config_file.config.settings, &self.overrides);
        let (files, tally) = self.load_files(&config_file, settings.strict).await?;
        tally.fail_on_errors()?;

        // Merge the catalog, stopping on cross-file conflicts before validation.
        let (translations, tally) = self.assemble(&config_file, files).await?;
        tally.fail_on_errors()?;

        // Start validating the merged catalog before any output future is polled.
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
                diagnostics
            }
        });

        // Assemble target futures while validation runs in the worker pool.
        let output_futures: Vec<OutputFuture<'_>> = vec![
            Box::pin(
                self.generate_json_outputs(&config_file, &translations, &settings)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "typescript")]
            Box::pin(
                self.generate_typescript_outputs(&config_file, &translations, &settings)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "rust")]
            Box::pin(
                self.generate_rust_outputs(&config_file, &translations, &settings)
                    .map_err(OutputError::from),
            ),
        ];

        // Emit validation diagnostics before allowing any output side effects.
        let tally = self.emit_all(&validate_translations.await?).await?;
        tally.fail_on_errors()?;

        // Backends without a code generator only announce what they skip.
        #[cfg(feature = "golang")]
        Self::warn_missing_golang_generator(&config_file.config);
        #[cfg(feature = "python")]
        Self::warn_missing_python_generator(&config_file.config);

        // Generate independent outputs concurrently once validation succeeds.
        futures::future::join_all(output_futures)
            .await
            .into_iter()
            .collect::<Result<(), _>>()?;

        Ok(())
    }

    /// Executes every configuration and generates its outputs.
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

    /// Lints one configuration's translation files and emits diagnostics.
    ///
    /// Unlike [`Self::execute_config`], no outputs are generated, and every
    /// stage runs even after errors so that one run reports everything.
    /// Returns the tally of emitted diagnostics together with the linted
    /// catalog.
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
    ) -> Result<(Tally, Arc<model::Translations>), Error> {
        tracing::debug!(name = config_file.config.name.as_ref(), "linting");

        // Lint resolves like generation except for `strict`; see
        // `Self::lint_strict` for why the config's value is not consulted.
        let settings = Settings {
            strict: self.lint_strict(),
            ..Settings::resolve(&config_file.config.settings, &self.overrides)
        };

        let (files, mut tally) = self.load_files(&config_file, settings.strict).await?;
        let (translations, assembled) = self.assemble(&config_file, files).await?;
        tally += assembled;

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
        tally += self.emit_all(&lint_diagnostics).await?;

        Ok((tally, translations))
    }

    /// Lints every configuration's translation files.
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

        let mut tally = Tally::default();
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
            let (config_tally, translations) =
                self.lint_config(Arc::clone(&config_file), params).await?;
            tally += config_tally;

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
            tally += self.emit_all(&dead_diagnostics).await?;
        }

        if tally.has_issues() {
            return Err(FailedWithErrors(tally).into());
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

/// Resolves all unique translation input paths referenced by `configs`.
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

    /// The keys of a parsed file, in order.
    fn keys(file: &ParsedFile) -> Vec<String> {
        file.translations
            .0
            .keys()
            .map(|key| key.as_ref().clone())
            .collect()
    }

    fn source_file(input: config::Input, path: &str, contents: &str) -> SourceFile {
        SourceFile {
            input,
            path: PathBuf::from(path),
            file_id: 0,
            contents: contents.to_string(),
        }
    }

    /// Filename prefixing uses the source file stem without its extension.
    #[test_util::test]
    fn prepend_filename_prefixes_with_file_stem() {
        let input = config::Input::new("translations/a.toml").with_prepend_filename(true);
        let raw_translations = indoc::indoc! {r#"
            [section]
            en = "Hello"
        "#};

        let file = source_file(input, "/base/dialogs/delete-user.toml", raw_translations);
        let parsed = parse_source_file(&file, None, true);

        assert_eq!(keys(&parsed), vec!["delete-user.section".to_string()]);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    }

    /// Relative-path prefixing keeps every directory and filename segment.
    #[test_util::test]
    fn prepend_relative_path_prefixes_with_full_path_segments() {
        let input =
            config::Input::new("translations/airtype/**/*.toml").with_prepend_relative_path(true);
        let base_dir = PathBuf::from("/workspace/translations/airtype");
        let path = base_dir.join("dialogs/chat/too-many-files.toml");
        let raw_translations = indoc::indoc! {r#"
            [section]
            en = "Hello"
        "#};

        let file = source_file(input, &path.to_string_lossy(), raw_translations);
        let parsed = parse_source_file(&file, Some(&base_dir), true);

        assert_eq!(
            keys(&parsed),
            vec!["dialogs.chat.too-many-files.section".to_string()]
        );
    }

    /// An explicit prefix works without enabling relative-path prefixing.
    #[test_util::test]
    fn prepend_relative_path_disabled_preserves_existing_behavior() {
        let input = config::Input::new("translations/upload.toml").with_prefix("upload");
        let raw_translations = indoc::indoc! {r#"
            [message]
            en = "Hello"
        "#};

        let file = source_file(input, "/base/upload.toml", raw_translations);
        let parsed = parse_source_file(&file, None, true);

        assert_eq!(keys(&parsed), vec!["upload.message".to_string()]);
    }

    /// A file that does not parse yields its error as a diagnostic instead of
    /// stopping the run, so every file's problems are reported at once.
    #[test_util::test]
    fn parse_errors_become_diagnostics() {
        let input = config::Input::new("translations/broken.toml");
        let file = source_file(input, "/base/broken.toml", "[unterminated");
        let parsed = parse_source_file(&file, None, true);

        assert!(parsed.translations.is_empty());
        assert!(parsed.diagnostics.iter().any(DiagnosticExt::is_error));
    }

    /// A key defined in two files is reported with one label per definition,
    /// not one label per key in those files.
    #[test_util::test]
    fn duplicate_keys_label_only_their_own_definitions() {
        let parse = |file_id: FileId, raw: &str| {
            let mut diagnostics = vec![];
            let translations = model::Translations::from_str(raw, file_id, true, &mut diagnostics)?;
            Ok::<_, model::toml::Error>(ParsedFile {
                file_id,
                translations,
                diagnostics,
            })
        };
        let first = parse(
            1,
            indoc::indoc! {r#"
                [shared]
                en = "one"

                [only-here]
                en = "unique"
            "#},
        )?;
        let second = parse(
            2,
            indoc::indoc! {r#"
                [shared]
                en = "two"
            "#},
        )?;

        let mut diagnostics = vec![];
        let combined = combine_translations(vec![first, second], &mut diagnostics);

        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].message, "duplicate key `shared`");
        // Both definitions of `shared` are labelled, in both files; the
        // unrelated `only-here` key is not.
        let labelled_files: Vec<FileId> = diagnostics[0]
            .labels
            .iter()
            .map(|label| label.file_id)
            .collect();
        assert_eq!(labelled_files, vec![1, 2]);
        // The merged catalog keeps the later definition, like a map insert.
        assert_eq!(combined.len(), 2);
    }

    /// Exclusion patterns remove matched files without producing diagnostics,
    /// even when an exclusion matches nothing.
    #[test_util::test]
    fn unique_input_paths_respects_exclude_patterns() -> eyre::Result<()> {
        let dir = temp_dir("exclude-patterns")?;
        let keep = dir.join("keep.toml");
        let skip = dir.join("skip.toml");
        std::fs::write(
            &keep,
            indoc::indoc! {r#"
                [a]
                en = "Hello"
            "#},
        )?;
        std::fs::write(
            &skip,
            indoc::indoc! {r#"
                [b]
                en = "Bye"
            "#},
        )?;

        let input = config::Input::new(dir.join("*.toml").to_string_lossy().into_owned())
            .with_exclude([
                skip.to_string_lossy().into_owned(),
                dir.join("absent-*.toml").to_string_lossy().into_owned(),
            ]);
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

    /// An exclusion pattern that is not valid glob syntax is an error, like an
    /// invalid input pattern, rather than being silently ignored.
    #[test_util::test]
    fn invalid_exclude_pattern_is_an_error() -> eyre::Result<()> {
        let dir = temp_dir("invalid-exclude")?;
        std::fs::write(dir.join("a.toml"), "")?;

        let input = config::Input::new(dir.join("*.toml").to_string_lossy().into_owned())
            .with_exclude([dir.join("[unclosed").to_string_lossy().into_owned()]);
        let mut diagnostics = Vec::new();

        let results: Vec<_> =
            Executor::unique_input_paths(&[input], None, true, None, &mut diagnostics).collect();
        assert!(
            results
                .iter()
                .any(|result| matches!(result, Err(Error::Pattern { .. }))),
            "{results:?}"
        );

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Every enabled output backend contributes its parent to scan exclusions.
    #[test_util::test]
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
