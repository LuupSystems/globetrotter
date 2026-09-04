//! Typed-language output generation selected by Cargo features.

#[cfg(any(
    feature = "rust",
    feature = "typescript",
    feature = "golang",
    feature = "python"
))]
use crate::config::v1::{self as config};
#[cfg(any(feature = "rust", feature = "typescript"))]
use crate::{config::settings::Settings, error::IoError, executor, model};
#[cfg(any(feature = "rust", feature = "typescript"))]
use std::path::PathBuf;
#[cfg(any(feature = "rust", feature = "typescript"))]
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
    /// Iterates over all target variants.
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

impl crate::executor::Executor {
    /// Writes one generated file to every configured path of a target.
    #[cfg(any(feature = "rust", feature = "typescript"))]
    async fn write_target_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        target: Target,
        output_paths: impl IntoIterator<Item = &PathBuf>,
        code: &str,
        settings: &Settings,
    ) -> Result<(), IoError> {
        let config = &config_file.config;
        for output_path in output_paths {
            let output_path =
                executor::resolve_path(config_file.config_dir.as_deref(), output_path);
            let outcome = self
                .write_output(&output_path, code.as_bytes(), settings)
                .await?;
            println!(
                "{} {outcome}",
                self.logger.target_log_prefix(&config.name, target)
            );
        }
        Ok(())
    }

    /// Warns when Go output is configured, since nothing generates it yet.
    #[cfg(feature = "golang")]
    pub(crate) fn warn_missing_golang_generator(config: &config::Config) {
        if config
            .outputs
            .golang
            .as_ref()
            .is_some_and(|go| !go.is_empty())
        {
            tracing::warn!(
                config = config.name.as_ref(),
                "Go output is configured, but there is no Go code generator yet; nothing is written"
            );
        }
    }

    /// Warns when Python output is configured, since nothing generates it yet.
    #[cfg(feature = "python")]
    pub(crate) fn warn_missing_python_generator(config: &config::Config) {
        if config
            .outputs
            .python
            .as_ref()
            .is_some_and(|python| !python.is_empty())
        {
            tracing::warn!(
                config = config.name.as_ref(),
                "Python output is configured, but there is no Python code generator yet; nothing is written"
            );
        }
    }

    #[cfg(feature = "rust")]
    pub(crate) async fn generate_rust_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &Arc<model::Translations>,
        settings: &Settings,
    ) -> Result<(), RustOutputError> {
        let Some(rust_config) = &config_file.config.outputs.rust else {
            return Ok(());
        };
        if rust_config.is_empty() {
            return Ok(());
        }

        // Generate once, then write the same code to every configured path.
        let code = tokio::task::spawn_blocking({
            let translations = Arc::clone(translations);
            move || globetrotter_rust::generate_translation_enum(&translations)
        })
        .await??;
        self.write_target_outputs(
            config_file,
            Target::Rust,
            &rust_config.output_paths,
            &code,
            settings,
        )
        .await?;
        Ok(())
    }

    #[cfg(feature = "typescript")]
    pub(crate) async fn generate_typescript_outputs<F>(
        &self,
        config_file: &config::ConfigFile<F>,
        translations: &Arc<model::Translations>,
        settings: &Settings,
    ) -> Result<(), TypescriptOutputError> {
        let Some(typescript_config) = &config_file.config.outputs.typescript else {
            return Ok(());
        };
        if typescript_config.is_empty() {
            return Ok(());
        }

        // Generate once, then write the same code to every configured path.
        let code = tokio::task::spawn_blocking({
            let translations = Arc::clone(translations);
            move || globetrotter_typescript::generate_translations_type_export(&translations)
        })
        .await??;
        self.write_target_outputs(
            config_file,
            Target::Typescript,
            typescript_config
                .interface_type
                .iter()
                .map(|interface| &interface.path),
            &code,
            settings,
        )
        .await?;
        Ok(())
    }
}
