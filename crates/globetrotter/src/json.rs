//! Per-language JSON output generation and output-path templating.

use crate::{
    config::{
        settings::Settings,
        v1::{self as config},
    },
    error::IoError,
    executor, model,
};
use colored::Colorize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// An error produced while generating JSON translation output.
#[derive(thiserror::Error, Debug)]
pub enum JsonOutputError {
    /// Writing the JSON output to disk failed.
    #[error(transparent)]
    Io(#[from] IoError),

    /// Serializing the translations to JSON failed.
    #[error(transparent)]
    Json(#[from] model::json::Error),

    /// Rendering the output path template failed.
    #[error("failed to template {template:?}")]
    Template {
        /// The template that could not be rendered.
        template: String,
        /// The underlying render error.
        #[source]
        source: handlebars::RenderError,
    },

    /// A spawned task failed to join.
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

#[allow(
    clippy::cast_precision_loss,
    reason = "byte counts are well within f64's exact-integer range; precision loss is irrelevant for human-readable display"
)]
fn human_readable_bytes(len: usize) -> String {
    human_bytes::human_bytes(len as f64)
}

impl executor::Executor {
    fn resolve_json_output_path(
        &self,
        path: &Path,
        language: model::Language,
    ) -> Result<PathBuf, JsonOutputError> {
        #[derive(Debug, serde::Serialize)]
        struct TemplateData {
            language: model::Language,
        }
        let template = path.to_string_lossy().to_string();
        let path = self
            .handlebars
            .render_template(&template, &TemplateData { language })
            .map_err(|source| JsonOutputError::Template { template, source })?;
        Ok(path.into())
    }

    /// Writes one language's JSON to every configured JSON output.
    async fn generate_json_language<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &model::Translations,
        settings: &Settings,
        language: model::Language,
    ) -> Result<(), JsonOutputError> {
        let config = &config_file.config;

        // Serialize the language once for every output path and for sizing.
        let json = translations.translations_json(
            language,
            settings
                .template_engine
                .as_ref()
                .map(|engine| engine.as_ref().clone()),
            settings.strict,
        )?;
        let json = Arc::new(serde_json::to_vec_pretty(&json).map_err(model::json::Error::from)?);

        // Compression is CPU-bound and only informs the log line, so it runs
        // off the async runtime while the files are written.
        let gzip_task = tokio::task::spawn_blocking({
            let json = Arc::clone(&json);
            move || crate::gzip::gzipped_size(&*json)
        });

        let mut outcomes = Vec::with_capacity(config.outputs.json.len());
        for output in &config.outputs.json {
            let output_path = self.resolve_json_output_path(&output.path, language)?;
            let output_path =
                executor::resolve_path(config_file.config_dir.as_deref(), &output_path);
            outcomes.push(self.write_output(&output_path, &json, settings).await?);
        }

        let num_bytes_gzip = gzip_task.await?.unwrap_or(0);
        let prefix = self.logger.language_log_prefix(&config.name, language);
        for outcome in outcomes {
            let sizes = if settings.dry_run {
                format!(
                    "({}, {} gzipped)",
                    human_readable_bytes(json.len()),
                    human_readable_bytes(num_bytes_gzip).bold()
                )
                .bright_black()
            } else {
                format!(
                    "({}, {} gzipped)",
                    human_readable_bytes(json.len()),
                    human_readable_bytes(num_bytes_gzip).bold().magenta()
                )
                .normal()
            };
            println!("{prefix} {outcome} {sizes}");
        }
        Ok(())
    }

    pub(crate) async fn generate_json_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &Arc<model::Translations>,
        settings: &Settings,
    ) -> Result<(), JsonOutputError> {
        if config_file.config.outputs.json.is_empty() {
            return Ok(());
        }
        // Languages are independent, so their files are written concurrently.
        futures::future::try_join_all(config_file.config.languages.iter().map(|language| {
            self.generate_json_language(config_file, translations, settings, **language)
        }))
        .await?;
        Ok(())
    }
}
