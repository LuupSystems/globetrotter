use std::path::PathBuf;

/// Configuration for Rust translation code generation outputs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutputConfig {
    /// File system paths where generated Rust translation bindings will be written.
    #[cfg_attr(feature = "serde", serde(default))]
    pub output_paths: Vec<PathBuf>,
}

impl OutputConfig {
    /// Create a new output configuration from an iterator of paths.
    pub fn new(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            output_paths: paths.into_iter().collect(),
        }
    }

    /// Return `true` if there are no configured output paths.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.output_paths.is_empty()
    }
}
