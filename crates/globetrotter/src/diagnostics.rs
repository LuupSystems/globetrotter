use codespan_reporting::{diagnostic::Diagnostic, files, term};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
// #[derive(Debug)]
pub struct Printer {
    writer: Arc<term::termcolor::StandardStream>,
    // writer: term::termcolor::StandardStream,
    diagnostic_config: term::Config,
    files: Arc<RwLock<files::SimpleFiles<String, String>>>,
    // files: RwLock<files::SimpleFiles<String, String>>,
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

impl Printer {
    #[must_use] pub fn new(color_choice: term::termcolor::ColorChoice) -> Self {
        let writer = term::termcolor::StandardStream::stderr(color_choice);
        let diagnostic_config = term::Config {
            styles: term::Styles::with_blue(term::termcolor::Color::Blue),
            ..term::Config::default()
        };
        Self {
            writer: Arc::new(writer),
            // writer,
            diagnostic_config,
            files: Arc::new(RwLock::new(files::SimpleFiles::new())),
            // files: RwLock::new(files::SimpleFiles::new()),
        }
    }

    pub async fn add_source_file(&self, name: impl ToSourceName, source: String) -> usize {
        let mut files = self.files.write().await;
        files.add(name.to_source_name(), source)
    }

    pub async fn emit(&self, diagnostic: &Diagnostic<usize>) -> Result<(), files::Error> {
        term::emit(
            &mut self.writer.lock(),
            &self.diagnostic_config,
            &*self.files.read().await,
            diagnostic,
        )
    }
}
