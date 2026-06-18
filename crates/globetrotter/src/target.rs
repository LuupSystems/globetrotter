#[cfg(any(
    feature = "typescript",
    feature = "rust",
    feature = "golang",
    feature = "python"
))]
use crate::{
    config::v1::{self as config},
    error::IoError,
    model,
};
#[cfg(any(feature = "rust", feature = "typescript"))]
use crate::{executor, progress::relative_to};
#[cfg(any(feature = "rust", feature = "typescript"))]
use futures::stream::{self, StreamExt, TryStreamExt};
#[cfg(any(
    feature = "typescript",
    feature = "rust",
    feature = "golang",
    feature = "python"
))]
use std::sync::Arc;

/// A code generation target language.
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
    /// TypeScript output.
    Typescript,
    /// Rust output.
    Rust,
    /// Go output.
    Golang,
    /// Python output.
    Python,
}

impl Target {
    /// Iterate over all target variants.
    #[must_use]
    pub fn iter() -> <Self as strum::IntoEnumIterator>::Iterator {
        <Self as strum::IntoEnumIterator>::iter()
    }
}

/// An error produced while generating Rust output.
#[cfg(feature = "rust")]
#[derive(thiserror::Error, Debug)]
pub enum RustOutputError {
    /// Writing the generated code to disk failed.
    #[error(transparent)]
    Io(#[from] IoError),

    /// Generating the Rust code failed.
    #[error(transparent)]
    Codegen(#[from] globetrotter_rust::Error),

    /// A spawned task failed to join.
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

/// An error produced while generating TypeScript output.
#[cfg(feature = "typescript")]
#[derive(thiserror::Error, Debug)]
pub enum TypescriptOutputError {
    /// Writing the generated code to disk failed.
    #[error(transparent)]
    Io(#[from] IoError),

    /// Generating the TypeScript code failed.
    #[error(transparent)]
    Codegen(#[from] globetrotter_typescript::Error),

    /// A spawned task failed to join.
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

/// An error produced while generating Go output.
#[cfg(feature = "golang")]
#[derive(thiserror::Error, Debug)]
pub enum GolangOutputError {
    /// Writing the generated code to disk failed.
    #[error(transparent)]
    Io(#[from] IoError),

    // #[error(transparent)]
    // Codegen(#[from] globetrotter_typescript::Error),
    /// A spawned task failed to join.
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

/// An error produced while generating Python output.
#[cfg(feature = "python")]
#[derive(thiserror::Error, Debug)]
pub enum PythonOutputError {
    /// Writing the generated code to disk failed.
    #[error(transparent)]
    Io(#[from] IoError),
    // #[error(transparent)]
    // Codegen(#[from] globetrotter_typescript::Error),
    /// A spawned task failed to join.
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

impl crate::executor::Executor {
    #[cfg(feature = "python")]
    pub(crate) async fn generate_python_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        _translations: &Arc<model::Translations>,
        _strict: bool,
    ) -> Result<(), PythonOutputError> {
        let config = &config_file.config;
        if config.outputs.python.is_none() {
            return Ok(());
        }

        // Placeholder to keep this async until Python codegen is implemented.
        tokio::task::yield_now().await;
        Ok(())
    }

    #[cfg(feature = "golang")]
    pub(crate) async fn generate_golang_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        _translations: &Arc<model::Translations>,
        _strict: bool,
    ) -> Result<(), GolangOutputError> {
        let config = &config_file.config;
        if config.outputs.golang.is_none() {
            return Ok(());
        }

        // Placeholder to keep this async until Golang codegen is implemented.
        tokio::task::yield_now().await;
        Ok(())
    }

    #[cfg(feature = "rust")]
    pub(crate) async fn generate_rust_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &Arc<model::Translations>,
        _strict: bool,
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
                        let displayed_path = if self.logger.use_absolute_paths {
                            output_path.display().to_string()
                        } else {
                            relative_to(self.global_base_dir_for_display.as_deref(), &output_path)
                                .display()
                                .to_string()
                        };
                        println!(
                            "{} wrote {}",
                            self.logger.target_log_prefix(&config.name, Target::Rust),
                            displayed_path,
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
        _strict: bool,
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
                        let displayed_path = if self.logger.use_absolute_paths {
                            output_path.display().to_string()
                        } else {
                            relative_to(self.global_base_dir_for_display.as_deref(), &output_path)
                                .display()
                                .to_string()
                        };
                        println!(
                            "{} wrote {}",
                            self.logger
                                .target_log_prefix(&config.name, Target::Typescript),
                            displayed_path,
                        );
                    }
                    Ok::<_, TypescriptOutputError>(())
                }
            })
            .await
    }
}
