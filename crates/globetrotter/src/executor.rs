use crate::{
    config::{
        config_file_names,
        v1::{self as config, PathOrGlobPattern},
    },
    error::{self, Error, FailedWithErrors, IoError, OutputError},
    model,
    progress::Logger,
    target::Target,
};
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use colored::Colorize;
use futures::future::{Future, TryFutureExt};
use futures::stream::{self, Stream, StreamExt, TryStreamExt};
use globetrotter_model::diagnostics::{DiagnosticExt, FileId, Span, Spanned, ToDiagnostics};
use handlebars::Handlebars;
use itertools::Itertools;
use normalize_path::NormalizePath;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

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

fn combine_translations(
    translations: Vec<(
        config::Input,
        PathBuf,
        usize,
        model::Translations,
        Vec<Diagnostic<FileId>>,
    )>,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> Result<model::Translations, Error> {
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

        dbg!(&occurrences);

        let diagnostic = error::DuplicateKeyError {
            key: duplicate_key.as_ref().to_string(),
            occurrences,
        };

        diagnostics.extend(diagnostic.to_diagnostics(true).into_iter());
    }

    // combine all the translations
    let translations = model::Translations(
        translations
            .into_iter()
            .flat_map(|res| (res.3).0.into_iter())
            .collect(),
    );
    Ok(translations)
}

#[derive(Debug)]
pub struct Executor {
    pub strict: Option<bool>,
    pub check_templates: Option<bool>,
    pub dry_run: bool,
    pub global_base_dir_for_display: Option<PathBuf>,
    pub handlebars: handlebars::Handlebars<'static>,
    pub diagnostic_printer: crate::diagnostics::Printer,
    pub logger: Logger,
}

impl Executor {
    pub fn new<F>(
        configs: &config::Configs<F>,
        diagnostic_printer: crate::diagnostics::Printer,
    ) -> Self {
        let logger = Logger::new(&configs);
        Self {
            strict: None,
            check_templates: None,
            dry_run: false,
            global_base_dir_for_display: None,
            handlebars: handlebars::Handlebars::default(),
            diagnostic_printer,
            logger,
        }
    }

    async fn read_translation_file(
        &self,
        input: (config::Input, PathBuf),
    ) -> Result<(config::Input, PathBuf, FileId, String), Error> {
        let (input, input_path) = input;

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

        Ok((input, input_path, file_id, raw_translations))
    }

    async fn process_translation_file<'a>(
        &self,
        input: (config::Input, PathBuf, FileId, String),
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
        let (input, input_path, file_id, raw_translations) = input;
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

            let file_stem = input_path
                .file_stem()
                .map(|name| name.to_string_lossy().to_string());

            let prefix: &[Option<&str>] =
                if input.prepend_filename.as_deref().copied().unwrap_or(false) {
                    &[
                        file_stem.as_deref(),
                        input.prefix.as_ref().map(|prefix| prefix.as_ref().as_str()),
                    ]
                } else {
                    &[input.prefix.as_ref().map(|prefix| prefix.as_ref().as_str())]
                };

            let prefix: Vec<_> = prefix
                .iter()
                .filter_map(|p| *p)
                .filter(|p| !p.is_empty())
                .collect();

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
                            .chain([&key.as_ref().as_str()])
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
        &'a self,
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
                let mut input_paths = resolve_input_paths(
                    base_dir,
                    &input.path_or_glob_pattern,
                    file_id.into(),
                    strict,
                    diagnostics,
                );

                // resolve excluded files
                let exclude: HashSet<PathBuf> = input
                    .exclude
                    .iter()
                    .flat_map(|exclude| {
                        resolve_input_paths(
                            base_dir,
                            &input.path_or_glob_pattern,
                            file_id.into(),
                            strict,
                            diagnostics,
                        )
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

    pub async fn execute_config(
        &self,
        config_file: Arc<config::ConfigFile<FileId>>,
    ) -> Result<(), Error> {
        tracing::debug!(name = config_file.config.name.as_ref(), "executing");

        let strict = self.strict.or(config_file.config.strict).unwrap_or(true);
        let check_templates = self
            .check_templates
            .or(config_file.config.check_templates)
            .unwrap_or(true);

        let mut diagnostics = vec![];

        let inputs = self.unique_input_paths(
            &config_file.config.inputs,
            config_file.config_dir.as_deref(),
            strict,
            config_file.file_id,
            &mut diagnostics,
        );
        let mut translations = stream::iter(inputs)
            .map(|input| async {
                let (input, input_path) = input?;
                let input_path = tokio::fs::canonicalize(&input_path)
                    .await
                    .map_err(|source| IoError::new(input_path, source))?;
                Ok::<_, Error>((input, input_path))
            })
            .buffer_unordered(16)
            .and_then(|input| async { self.read_translation_file(input).await })
            .and_then(|input| async { self.process_translation_file(input, strict).await })
            .try_collect::<Vec<_>>()
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
            let _ = self.diagnostic_printer.emit(&diagnostic);
        }
        if num_errors > 0 {
            return Err(FailedWithErrors {
                num_errors,
                num_warnings,
            }
            .into());
        }

        let (translations, mut diagnostics) = tokio::task::spawn_blocking(|| {
            let mut diagnostics: Vec<Diagnostic<FileId>> = vec![];
            let translations = combine_translations(translations, &mut diagnostics)?;
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
            let _ = self.diagnostic_printer.emit(&diagnostic);
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
            move || {
                let mut diagnostics = vec![];
                translations.validate(
                    &config_file.config.name,
                    &config_file.config.languages,
                    config_file.config.template_engine.as_ref(),
                    strict,
                    check_templates,
                    config_file.file_id.into(),
                    &mut diagnostics,
                );
                Ok::<_, Error>(diagnostics)
            }
        });

        let output_futures: Vec<Pin<Box<dyn Future<Output = Result<(), OutputError>>>>> = vec![
            Box::pin(
                self.generate_json_outputs(&*config_file, &translations, strict)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "typescript")]
            Box::pin(
                self.generate_typescript_outputs(&*config_file, &translations, strict)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "rust")]
            Box::pin(
                self.generate_rust_outputs(&*config_file, &translations, strict)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "python")]
            Box::pin(
                self.generate_python_outputs(&*config_file, &translations, strict)
                    .map_err(OutputError::from),
            ),
            #[cfg(feature = "golang")]
            Box::pin(
                self.generate_golang_outputs(&*config_file, &translations, strict)
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
            let _ = self.diagnostic_printer.emit(&diagnostic);
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

    pub async fn execute(self, configs: config::Configs<FileId>) -> Result<Self, Error> {
        tracing::trace!(num_configs = configs.len(), "executing");

        stream::iter(configs.into_iter())
            .map(|config_file| async move { Ok(Arc::new(config_file)) })
            .buffer_unordered(8)
            .try_for_each(|config| async { self.execute_config(config).await })
            .await?;

        Ok(self)
    }
}
