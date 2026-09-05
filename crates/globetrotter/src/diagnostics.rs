//! Concurrent diagnostic rendering backed by a shared source-file registry.

use codespan_reporting::{diagnostic::Diagnostic, files, term};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use tokio::sync::{Mutex, RwLock};

/// Renders diagnostics and tracks the source files their labels refer to.
///
/// Clones share the source registry and serialized stderr writer, so files may
/// be registered and diagnostics emitted safely from concurrent tasks.
#[derive(Clone)]
pub struct Printer {
    writer: Arc<Mutex<term::StylesWriter<'static, term::termcolor::StandardStream>>>,
    diagnostic_config: term::Config,
    files: Arc<RwLock<SourceFiles>>,
}

#[derive(Default)]
struct SourceFiles {
    files: files::SimpleFiles<String, String>,
    ids: HashMap<String, Vec<usize>>,
}

impl Default for Printer {
    fn default() -> Self {
        Self::new(term::termcolor::ColorChoice::Auto)
    }
}

static DEFAULT_STYLES: LazyLock<term::Styles> = LazyLock::new(term::Styles::default);

impl Printer {
    /// Creates a printer that writes to stderr using the given color choice.
    #[must_use]
    pub fn new(color_choice: term::termcolor::ColorChoice) -> Self {
        let writer = term::termcolor::StandardStream::stderr(color_choice);
        let writer = term::StylesWriter::new(writer, &DEFAULT_STYLES);
        let diagnostic_config = term::Config::default();
        Self {
            writer: Arc::new(Mutex::new(writer)),
            diagnostic_config,
            files: Arc::new(RwLock::new(SourceFiles::default())),
        }
    }

    /// Registers a source file and returns the id its diagnostic labels use.
    ///
    /// `name` is the path diagnostics display for the file, so callers pass
    /// the form they want shown, such as a path relative to the project.
    /// Registering the same name and contents again reuses its id, allowing
    /// diagnostics from overlapping configs to identify the same source.
    pub async fn add_source_file(&self, name: impl AsRef<Path>, source: String) -> usize {
        let mut files = self.files.write().await;
        let name = name.as_ref().to_string_lossy().into_owned();
        if let Some(ids) = files.ids.get(&name)
            && let Some(&id) = ids.iter().find(|&&id| {
                files
                    .files
                    .get(id)
                    .is_ok_and(|file| file.source() == &source)
            })
        {
            return id;
        }
        let id = files.files.add(name.clone(), source);
        files.ids.entry(name).or_default().push(id);
        id
    }

    /// Returns the display name registered for a diagnostic source.
    ///
    /// # Errors
    ///
    /// Returns an error if `file_id` is not registered.
    pub async fn source_name(&self, file_id: usize) -> Result<String, files::Error> {
        Ok(self.files.read().await.files.get(file_id)?.name().clone())
    }

    /// Renders a diagnostic to an ANSI-colored string for printing above a
    /// progress bar (where the normal streaming writer cannot be used directly).
    ///
    /// # Errors
    ///
    /// Returns an error if the diagnostic cannot be formatted.
    pub async fn render(&self, diagnostic: &Diagnostic<usize>) -> Result<String, files::Error> {
        let mut buffer = term::termcolor::Buffer::ansi();
        {
            let mut styled = term::StylesWriter::new(&mut buffer, &DEFAULT_STYLES);
            term::emit_to_write_style(
                &mut styled,
                &self.diagnostic_config,
                &self.files.read().await.files,
                diagnostic,
            )?;
        }
        Ok(String::from_utf8_lossy(buffer.as_slice()).into_owned())
    }

    /// Emit a single diagnostic to the configured writer.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the formatted diagnostic to the underlying
    /// output stream fails.
    pub async fn emit(&self, diagnostic: &Diagnostic<usize>) -> Result<(), files::Error> {
        let mut writer = self.writer.lock().await;

        term::emit_to_write_style(
            &mut *writer,
            &self.diagnostic_config,
            &self.files.read().await.files,
            diagnostic,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Printer;

    #[test_util::test]
    async fn repeated_source_versions_reuse_their_original_ids() {
        let printer = Printer::default();
        let first = printer
            .add_source_file("catalog.toml", "first".into())
            .await;
        let second = printer
            .add_source_file("catalog.toml", "second".into())
            .await;
        assert_ne!(first, second);
        assert_eq!(
            printer
                .add_source_file("catalog.toml", "first".into())
                .await,
            first
        );
        assert_ne!(
            printer.add_source_file("other.toml", "first".into()).await,
            first
        );
    }
}
