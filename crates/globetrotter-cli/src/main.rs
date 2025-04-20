#![allow(warnings)]

mod base_dir;
#[cfg(feature = "convert")]
mod convert;
mod format;
mod options;
mod telemetry;

use clap::Parser;
use codespan_reporting::diagnostic::{self, Diagnostic, Severity};
use color_eyre::eyre::{self, WrapErr};
use futures::stream::{Stream, StreamExt, TryStream, TryStreamExt};
use globetrotter::{
    config,
    diagnostics::Printer as DiagnosticsPrinter,
    model::{
        self, Language,
        diagnostics::{FileId, Spanned, ToDiagnostics},
    },
    progress::Logger,
};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

trait Invert {
    fn invert(self) -> Self;
}

impl Invert for Option<bool> {
    fn invert(self) -> Self {
        self.map(|value| !value)
    }
}

#[derive(Debug)]
pub struct Globetrotter {
    pub options: options::Options,
    pub diagnostic_printer: DiagnosticsPrinter,
    pub global_base_dir_for_display: Option<PathBuf>,
    pub configs: config::v1::Configs<FileId>,
}

impl Globetrotter {
    pub async fn new(options: options::Options) -> eyre::Result<Self> {
        let color_choice = options
            .logging
            .color_choice
            .unwrap_or(termcolor::ColorChoice::Auto);

        let diagnostic_printer = DiagnosticsPrinter::new(color_choice);

        let mut config_file_paths = futures::stream::iter(&options.config_paths)
            .map(|config_path| async move {
                let config_path = tokio::fs::canonicalize(&config_path)
                    .await
                    .wrap_err_with(|| "failed to open: {config_path:?}")?;
                let metadata = tokio::fs::metadata(&config_path)
                    .await
                    .wrap_err_with(|| "failed to open: {config_path:?}")?;

                if metadata.is_file() {
                    Ok(config_path.to_owned())
                } else if metadata.is_dir() {
                    globetrotter::config::find_config_file(&config_path).await?.ok_or_else(|| eyre::eyre!("directory {config_path:?} does not contain a globetrotter config file"))
                } else {
                    Err(eyre::eyre!("neither file nor directory: {config_path:?}"))
                }
            })
            .buffered(8)
            .try_collect::<Vec<_>>()
            .await?;

        if config_file_paths.is_empty() {
            // if no config file is provided, try to find config file in current directory
            let cwd = std::env::current_dir()?;
            if let Some(config_file_path) = globetrotter::config::find_config_file(&cwd).await? {
                config_file_paths.push(config_file_path);
            }
        }

        let global_base_dir_for_display = base_dir::common_base_directory(&config_file_paths);
        tracing::debug!(
            configs = ?config_file_paths,
            dir = ?global_base_dir_for_display,
            "configurations",
        );

        let configs = futures::stream::iter(config_file_paths)
            .map(|config_file_path| async move {
                let raw_config = tokio::fs::read_to_string(&config_file_path).await?;
                Ok((config_file_path, raw_config))
            })
            .buffered(8)
            .and_then(|(config_file_path, raw_config)| {
                let diagnostic_printer = diagnostic_printer.clone();
                async move {
                    let config_dir = config_file_path.parent().ok_or_else(|| {
                        eyre::eyre!("failed to get parent directory of {config_file_path:?}")
                    })?;
                    debug_assert!(tokio::fs::metadata(&config_dir).await?.is_dir());
                    let file_id = diagnostic_printer
                        .add_source_file(&config_file_path, raw_config.clone())
                        .await;
                    let mut diagnostics: Vec<Diagnostic<usize>> = vec![];
                    match globetrotter::config::from_str(
                        &raw_config,
                        config_dir,
                        file_id,
                        options.strict,
                        &mut diagnostics,
                    ) {
                        Err(err) => {
                            diagnostics.extend(err.to_diagnostics(file_id));
                            Ok::<_, eyre::Report>((vec![], diagnostics))
                        }
                        Ok(valid_configs) => Ok::<_, eyre::Report>((valid_configs, diagnostics)),
                    }
                }
            })
            .try_collect::<Vec<_>>()
            .await?;

        // need at least an input file?
        // right now i do not see how cli interface should
        // look like...
        //
        // maybe there is a compelling use case to quickly generate
        // a JSON from a translation file, without creating a
        // config file...
        //
        // until we know what we want in life, we complain
        // eyre::bail!("no config file found, try --config");

        // here is how this could start:
        //
        // configs.push(config::v1::Config {
        //     name: Spanned::dummy("default".to_string()),
        //     languages: vec![Spanned::dummy(Language::De)],
        //     template_engine: options.template_engine.clone().map(Spanned::dummy),
        //     check_templates: options.check_templates,
        //     strict: options.strict,
        //     inputs: vec![config::v1::Input {
        //         path_or_glob_pattern: Spanned::dummy("test".to_string()),
        //         exclude: vec![],
        //         prefix: None,
        //         prepend_filename: None,
        //         separator: None,
        //     }],
        //     outputs: config::v1::Outputs {
        //         json: vec![],
        //         #[cfg(feature = "typescript")]
        //         typescript: None,
        //         #[cfg(feature = "rust")]
        //         rust: None,
        //         #[cfg(feature = "golang")]
        //         golang: None,
        //         #[cfg(feature = "python")]
        //         python: None,
        //     },
        // });

        let (configs, diagnostics): (Vec<_>, Vec<_>) = configs.into_iter().unzip();
        let configs = configs.into_iter().flatten().collect();

        // emit diagnostics
        let has_error = diagnostics
            .iter()
            .flatten()
            .any(|d| d.severity == Severity::Error);
        for diagnostic in diagnostics.into_iter().flatten() {
            diagnostic_printer.emit(&diagnostic).await?;
        }
        if has_error {
            eyre::bail!("failed to parse config");
        }

        Ok(Self {
            options,
            diagnostic_printer,
            global_base_dir_for_display,
            configs,
        })
    }

    pub async fn execute(self) -> Result<(), globetrotter::Error> {
        let start = std::time::Instant::now();
        let mut logger = Logger::new(&self.configs);
        logger.use_absolute_paths = self.options.print_absolute_paths.unwrap_or(false);

        let executor = globetrotter::Executor {
            strict: self.options.strict,
            check_templates: self.options.check_templates,
            dry_run: self.options.dry_run.unwrap_or(false),
            global_base_dir_for_display: self.global_base_dir_for_display,
            logger: logger.clone(),
            diagnostic_printer: self.diagnostic_printer,
            handlebars: Default::default(),
        };

        println!("");
        executor.execute(self.configs).await?;
        println!("{}", logger.completed(&start.elapsed()));

        Ok(())
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let start = std::time::Instant::now();
    let mut options = options::Options::parse();

    let color_choice = options
        .logging
        .color_choice
        .unwrap_or(termcolor::ColorChoice::Auto);

    let (log_format, use_color) = telemetry::setup_logging(
        options.logging.log_level,
        options.logging.log_format,
        color_choice,
    )?;

    let command = options.command.take();
    let globetrotter = Globetrotter::new(options).await?;
    match command {
        None => {
            globetrotter.execute().await?;
        }
        Some(options::Command::Format(format_options)) => {
            globetrotter.format().await?;
        }
        #[cfg(feature = "convert")]
        Some(options::Command::Convert(convert_options)) => {
            convert::convert(convert_options).await?;
        }
    }

    tracing::debug!(elapsed = ?start.elapsed(), "completed");
    Ok(())
}

#[cfg(test)]
pub mod tests {
    static INIT: std::sync::Once = std::sync::Once::new();

    /// Initialize test
    ///
    /// This ensures `color_eyre` is setup once.
    pub fn init() {
        INIT.call_once(|| {
            color_eyre::install().ok();
        });
    }
}
