use crate::{
    config::{
        config_file_names,
        v1::{self as config, PathOrGlobPattern},
    },
    error::{self, Error, IoError},
    executor, model,
    progress::{Logger, relative_to},
};
use colored::Colorize;
use futures::stream::{self, Stream, StreamExt, TryStreamExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    strum::Display,
    strum::EnumIter,
)]
pub enum Target {
    Typescript,
    Rust,
    Golang,
    Python,
}

impl Target {
    #[must_use]
    pub fn iter() -> <Self as strum::IntoEnumIterator>::Iterator {
        <Self as strum::IntoEnumIterator>::iter()
    }
}

#[cfg(feature = "rust")]
#[derive(thiserror::Error, Debug)]
pub enum RustOutputError {
    #[error(transparent)]
    Io(#[from] IoError),

    #[error(transparent)]
    Codegen(#[from] globetrotter_rust::Error),

    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

#[cfg(feature = "typescript")]
#[derive(thiserror::Error, Debug)]
pub enum TypescriptOutputError {
    #[error(transparent)]
    Io(#[from] IoError),

    #[error(transparent)]
    Codegen(#[from] globetrotter_typescript::Error),

    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

#[cfg(feature = "golang")]
#[derive(thiserror::Error, Debug)]
pub enum GolangOutputError {
    #[error(transparent)]
    Io(#[from] IoError),

    // #[error(transparent)]
    // Codegen(#[from] globetrotter_typescript::Error),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

#[cfg(feature = "python")]
#[derive(thiserror::Error, Debug)]
pub enum PythonOutputError {
    #[error(transparent)]
    Io(#[from] IoError),
    // #[error(transparent)]
    // Codegen(#[from] globetrotter_typescript::Error),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

impl executor::Executor {
    #[cfg(feature = "python")]
    pub(crate) async fn generate_python_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &Arc<model::Translations>,
        strict: bool,
    ) -> Result<(), PythonOutputError> {
        let config = &config_file.config;
        let Some(ref python_config) = config.outputs.python else {
            return Ok(());
        };
        Ok(())
    }

    #[cfg(feature = "golang")]
    pub(crate) async fn generate_golang_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &Arc<model::Translations>,
        strict: bool,
    ) -> Result<(), GolangOutputError> {
        let config = &config_file.config;
        let Some(ref golang_config) = config.outputs.golang else {
            return Ok(());
        };
        Ok(())
    }

    #[cfg(feature = "rust")]
    pub(crate) async fn generate_rust_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &Arc<model::Translations>,
        strict: bool,
    ) -> Result<(), RustOutputError> {
        let config = &config_file.config;
        let Some(ref rust_config) = config.outputs.rust else {
            return Ok(());
        };
        stream::iter(rust_config.output_paths.iter())
            .map(|output_path| async move { Ok(output_path) })
            .buffer_unordered(16)
            .try_for_each(|output_path| {
                let translations = Arc::clone(translations);
                async move {
                    let output_path =
                        executor::resolve_path(config_file.config_dir.as_deref(), output_path);

                    let code = tokio::task::spawn_blocking(move || {
                        globetrotter_rust::generate_translation_enum(&translations)
                    })
                    .await??;

                    if self.dry_run {
                        println!(
                            "{} {}",
                            self.logger.target_log_prefix(&config.name, Target::Rust),
                            self.logger.dry_run_would_write(&output_path),
                        );
                    } else {
                        executor::write_to_file(&output_path, code.as_bytes()).await?;
                        println!(
                            "{} wrote {:?}",
                            self.logger.target_log_prefix(&config.name, Target::Rust),
                            if self.logger.use_absolute_paths {
                                output_path
                            } else {
                                relative_to(
                                    self.global_base_dir_for_display.as_deref(),
                                    &output_path,
                                )
                            }
                        );
                    }

                    Ok::<_, RustOutputError>(())
                }
            })
            .await
    }

    #[cfg(feature = "typescript")]
    pub(crate) async fn generate_typescript_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &Arc<model::Translations>,
        strict: bool,
    ) -> Result<(), TypescriptOutputError> {
        let config = &config_file.config;
        let Some(ref typescript_config) = config.outputs.typescript else {
            return Ok(());
        };
        stream::iter(typescript_config.interface_type.iter())
            .map(|interface| async move { Ok(interface) })
            .buffer_unordered(16)
            .try_for_each(|interface| {
                let translations = Arc::clone(translations);
                async move {
                    let output_path =
                        executor::resolve_path(config_file.config_dir.as_deref(), &interface.path);

                    let code = tokio::task::spawn_blocking(move || {
                        globetrotter_typescript::generate_translations_type_export(&translations)
                    })
                    .await??;

                    if self.dry_run {
                        println!(
                            "{} {}",
                            self.logger
                                .target_log_prefix(&config.name, Target::Typescript),
                            self.logger.dry_run_would_write(&output_path),
                        );
                    } else {
                        executor::write_to_file(&output_path, code.as_bytes()).await?;
                        println!(
                            "{} wrote {:?}",
                            self.logger
                                .target_log_prefix(&config.name, Target::Typescript),
                            if self.logger.use_absolute_paths {
                                output_path
                            } else {
                                relative_to(
                                    self.global_base_dir_for_display.as_deref(),
                                    &output_path,
                                )
                            }
                        );
                    }
                    Ok::<_, TypescriptOutputError>(())
                }
            })
            .await
    }
}
