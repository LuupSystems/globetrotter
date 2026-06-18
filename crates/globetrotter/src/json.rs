use crate::{
    config::v1::{self as config},
    error::IoError,
    executor, model,
    progress::relative_to,
};
use colored::Colorize;
use futures::stream::{self, StreamExt, TryStreamExt};
use std::io::Write;
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

    pub(crate) async fn generate_json_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &Arc<model::Translations>,
        strict: bool,
    ) -> Result<(), JsonOutputError> {
        let config = &config_file.config;
        let json_output_paths = config.languages.iter().flat_map(|language| {
            config.outputs.json.iter().cloned().map(move |config| {
                let output_path = self.resolve_json_output_path(&config.path, **language)?;
                Ok::<_, JsonOutputError>((config, output_path, language))
            })
        });
        stream::iter(json_output_paths)
            .map(|res| async { res })
            .buffer_unordered(16)
            .try_for_each(|res| {
                let translations = Arc::clone(translations);
                async move {
                    let (_json_config, json_output_path, language) = res;
                    let json_output_path = executor::resolve_path(
                        config_file.config_dir.as_deref(),
                        &json_output_path,
                    );

                    let mut json = Vec::new();
                    {
                        let mut writer = std::io::BufWriter::new(std::io::Cursor::new(&mut json));
                        translations.write_translations_json(
                            **language,
                            config
                                .template_engine
                                .as_ref()
                                .map(|tpl| tpl.as_ref().clone()),
                            strict,
                            &mut writer,
                        )?;
                        let _ = writer.flush();
                    }

                    let json = Arc::new(json);

                    // compute gzipped size
                    let gzip_task = tokio::task::spawn_blocking({
                        let json = Arc::clone(&json);
                        move || crate::gzip::gzipped_size(&*json)
                    });

                    // write to file
                    let dry_run = self.dry_run;
                    let write_task = tokio::task::spawn({
                        let json_output_path = json_output_path.clone();
                        let json = Arc::clone(&json);
                        async move {
                            if dry_run {
                                return Ok(());
                            }
                            executor::write_to_file(&json_output_path, &*json).await?;
                            Ok::<_, JsonOutputError>(())
                        }
                    });

                    // join tasks
                    let () = write_task.await??;
                    let num_bytes_gzip = gzip_task.await?.unwrap_or(0);

                    if dry_run {
                        println!(
                            "{} {} {}",
                            self.logger.language_log_prefix(&config.name, **language),
                            self.logger.dry_run_would_write(&json_output_path),
                            format!(
                                "({}, {} gzipped)",
                                human_readable_bytes(json.len()),
                                human_readable_bytes(num_bytes_gzip).bold()
                            )
                            .bright_black()
                        );
                    } else {
                        let displayed_path = if self.logger.use_absolute_paths {
                            json_output_path.display().to_string()
                        } else {
                            relative_to(
                                self.global_base_dir_for_display.as_deref(),
                                &json_output_path,
                            )
                            .display()
                            .to_string()
                        };
                        println!(
                            "{} wrote {} ({}, {} gzipped)",
                            self.logger.language_log_prefix(&config.name, **language),
                            displayed_path,
                            human_readable_bytes(json.len()),
                            human_readable_bytes(num_bytes_gzip).bold().magenta()
                        );
                    }

                    Ok::<_, JsonOutputError>(())
                }
            })
            .await
    }
}
