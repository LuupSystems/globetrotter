use crate::options::LintOptions;
use color_eyre::eyre;
use globetrotter::config::v1::{Config, ConfigFile, Input};
use globetrotter::executor::LintParams;
use globetrotter::model::TemplateEngine;
use globetrotter::progress::Logger;
use std::path::PathBuf;

fn ad_hoc_translation_config(
    translations: &[PathBuf],
    template_engine: Option<&TemplateEngine>,
) -> Option<ConfigFile<globetrotter::model::diagnostics::FileId>> {
    if translations.is_empty() {
        return None;
    }

    let inputs = translations
        .iter()
        .map(|path| Input::new(path.to_string_lossy().into_owned()));
    let mut config = Config::new("translations").with_inputs(inputs);
    if let Some(template_engine) = template_engine.cloned() {
        config = config.with_template_engine(template_engine);
    }

    Some(ConfigFile {
        file_id: None,
        config_dir: None,
        config,
    })
}

impl crate::Globetrotter {
    /// Lint the configured translation files and report any issues.
    ///
    /// Checks for missing, empty, or whitespace-padded translations, templates
    /// that fail to compile, placeholders that are inconsistent across
    /// languages, template arguments that are used but not declared (or declared
    /// but never used), and exact duplicate strings. With `--usages`, also reports
    /// keys not referenced in the given source directories. No files are written.
    ///
    /// # Errors
    ///
    /// Returns an error if there are no translation files to lint, if a file
    /// cannot be read or parsed, or if any lint issues are found.
    pub async fn lint(self, options: &LintOptions) -> eyre::Result<()> {
        let mut configs = self.configs;

        // lint any files passed directly via `--translation` as an ad-hoc config.
        if let Some(config) = ad_hoc_translation_config(
            &self.options.translations,
            self.options.template_engine.as_ref(),
        ) {
            configs.push(config);
        }

        if configs.is_empty() {
            eyre::bail!(
                "no translation files found to lint; pass --translation <FILE> or --config <FILE>"
            );
        }

        let logger = Logger::new(&configs);
        let executor = globetrotter::Executor {
            strict: self.options.strict,
            check_templates: self.options.check_templates,
            dry_run: true,
            global_base_dir_for_display: self.global_base_dir_for_display,
            logger,
            diagnostic_printer: self.diagnostic_printer,
            handlebars: handlebars::Handlebars::default(),
        };

        let params = LintParams {
            detect_duplicates: !options.no_duplicates,
            usages: options.usages.clone(),
        };

        println!();
        executor.lint(configs, &params).await?;
        tracing::info!("no issues found");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ad_hoc_translation_config;
    use globetrotter::model::TemplateEngine;
    use std::path::PathBuf;

    #[test]
    fn ad_hoc_config_carries_template_engine() {
        let translations = vec![PathBuf::from("translations/common.toml")];
        let config =
            ad_hoc_translation_config(&translations, Some(&TemplateEngine::Handlebars)).unwrap();

        assert_eq!(
            config
                .config
                .template_engine
                .as_ref()
                .map(std::convert::AsRef::as_ref),
            Some(&TemplateEngine::Handlebars)
        );
        assert_eq!(config.config.inputs.len(), 1);
    }
}
