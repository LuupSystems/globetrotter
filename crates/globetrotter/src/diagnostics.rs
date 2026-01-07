use codespan_reporting::{diagnostic::Diagnostic, files, term};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub struct Printer {
    writer: Arc<Mutex<term::StylesWriter<'static, term::termcolor::StandardStream>>>,
    diagnostic_config: term::Config,
    files: Arc<RwLock<files::SimpleFiles<String, String>>>,
}

pub trait ToSourceName {
    fn to_source_name(self) -> String;
}

impl ToSourceName for String {
    fn to_source_name(self) -> String {
        self
    }
}

impl ToSourceName for &Path {
    fn to_source_name(self) -> String {
        self.to_string_lossy().to_string()
    }
}

impl ToSourceName for &PathBuf {
    fn to_source_name(self) -> String {
        self.as_path().to_source_name()
    }
}

impl Default for Printer {
    fn default() -> Self {
        Self::new(term::termcolor::ColorChoice::Auto)
    }
}

static DEFAULT_STYLES: LazyLock<term::Styles> = LazyLock::new(term::Styles::default);

impl Printer {
    #[must_use]
    pub fn new(color_choice: term::termcolor::ColorChoice) -> Self {
        let writer = term::termcolor::StandardStream::stderr(color_choice);
        let writer = term::StylesWriter::new(writer, &DEFAULT_STYLES);
        let diagnostic_config = term::Config::default();
        Self {
            writer: Arc::new(Mutex::new(writer)),
            diagnostic_config,
            files: Arc::new(RwLock::new(files::SimpleFiles::new())),
        }
    }

    pub async fn add_source_file(&self, name: impl ToSourceName, source: String) -> usize {
        let mut files = self.files.write().await;
        files.add(name.to_source_name(), source)
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
            &*self.files.read().await,
            diagnostic,
        )
    }
}
