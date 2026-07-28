use crate::options::LintOptions;
use color_eyre::eyre;
use globetrotter::config::v1::{Config, ConfigFile, Input};
use globetrotter::executor::LintParams;
use globetrotter::progress::Logger;
use std::path::PathBuf;
use std::process::ExitCode;

/// One config with an input per `--translation` file.
///
/// Settings such as `--engine` are deliberately not copied into this config:
/// they reach it through the executor's overrides layer, like any other config.
fn ad_hoc_translation_config(
    translations: &[PathBuf],
) -> Option<ConfigFile<globetrotter::model::diagnostics::FileId>> {
    if translations.is_empty() {
        return None;
    }

    let inputs = translations
        .iter()
        .map(|path| Input::new(path.to_string_lossy().into_owned()));
    Some(ConfigFile {
        file_id: None,
        config_dir: None,
        config: Config::new("translations").with_inputs(inputs),
    })
}

impl crate::Globetrotter {
    /// Lint the configured translation files and report any issues.
    ///
    /// Checks for missing, empty, or whitespace-padded translations, templates
    /// that fail to compile, placeholders that are inconsistent across
    /// languages, template arguments that are used but not declared (or declared
    /// but never used), and exact duplicate strings. With `--usages`, also reports
    /// keys not referenced in the given source directories. With `--semantic`,
    /// reports language pairs that appear to have semantically drifted apart
    /// (a ranked review aid, emitted as notes). No files are written.
    ///
    /// Returns [`ExitCode::FAILURE`] (with a one-line summary) if any issues
    /// were found, otherwise [`ExitCode::SUCCESS`]. Genuine errors (missing or
    /// unparsable files) are returned as `Err` and reported normally.
    ///
    /// # Errors
    ///
    /// Returns an error if there are no translation files to lint or if a file
    /// cannot be read or parsed.
    pub async fn lint(self, options: &LintOptions) -> eyre::Result<ExitCode> {
        let start = std::time::Instant::now();
        let mut configs = self.configs;

        // lint any files passed directly via `--translation` as an ad-hoc config.
        if let Some(config) = ad_hoc_translation_config(&self.options.translations) {
            configs.push(config);
        }

        if configs.is_empty() {
            eyre::bail!(
                "no translation files found to lint; pass --translation <FILE> or --config <FILE>"
            );
        }

        let logger = Logger::new(&configs);
        let executor = globetrotter::Executor {
            overrides: globetrotter::config::SettingsLayer {
                // lint never writes outputs, so this is not a user-facing setting to resolve.
                dry_run: Some(true),
                ..self.options.settings_layer()
            },
            global_base_dir_for_display: self.global_base_dir_for_display,
            logger,
            diagnostic_printer: self.diagnostic_printer,
            handlebars: handlebars::Handlebars::default(),
        };

        #[cfg(feature = "semantic")]
        let semantic = options
            .semantic
            .params(self.options.cache_dir.as_deref(), &self.options.cache_dir());
        #[cfg(feature = "semantic")]
        if semantic.is_some() {
            tracing::warn!(
                "`--semantic` is experimental: a best-effort review aid that flags many false positives on legitimately divergent translations — review every finding manually"
            );
        }
        #[cfg(not(feature = "semantic"))]
        let semantic = None;

        let params = LintParams {
            detect_duplicates: !options.no_duplicates,
            usages: options.usages.clone(),
            semantic,
        };

        println!();
        let result = executor.lint(configs, &params).await;
        let elapsed = format_duration(start.elapsed());
        match result {
            Ok(_) => {
                tracing::info!("no issues found in {elapsed}");
                Ok(ExitCode::SUCCESS)
            }
            // Findings are an expected outcome, not a crash: report a clean
            // summary and exit non-zero without an error trace.
            Err(globetrotter::Error::Failed(failed)) => {
                let errors = pluralize(failed.num_errors, "error");
                let warnings = pluralize(failed.num_warnings, "warning");
                if failed.num_errors > 0 {
                    tracing::error!("found {errors} and {warnings} in {elapsed}");
                } else {
                    tracing::warn!("found {warnings} in {elapsed}");
                }
                Ok(ExitCode::FAILURE)
            }
            Err(error) => Err(error.into()),
        }
    }
}

/// `"1 error"` / `"3 errors"`.
fn pluralize(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

/// Human-friendly elapsed time: `"2m 43s"`, `"12.3s"`, or `"450ms"`.
fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs >= 1 {
        format!("{:.1}s", duration.as_secs_f64())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::ad_hoc_translation_config;
    use std::path::PathBuf;

    /// `--translation` files become one config with an input per file, with no
    /// settings of their own: flags like `--engine` apply through the
    /// executor's overrides layer instead.
    #[test]
    fn ad_hoc_config_collects_inputs_without_settings() {
        assert!(ad_hoc_translation_config(&[]).is_none());

        let translations = vec![PathBuf::from("translations/common.toml")];
        let config = ad_hoc_translation_config(&translations).unwrap();

        assert_eq!(config.config.inputs.len(), 1);
        assert_eq!(
            config.config.settings,
            globetrotter::config::SettingsLayer::default()
        );
    }
}
