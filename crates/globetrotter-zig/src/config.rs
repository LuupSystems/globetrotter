use std::path::PathBuf;

/// Configuration for Zig translation code generation outputs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutputConfig {
    /// File system paths where generated Zig translation bindings will be written.
    #[cfg_attr(feature = "serde", serde(default))]
    pub output_paths: Vec<PathBuf>,
}

impl OutputConfig {
    /// Return `true` if there are no configured output paths.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.output_paths.is_empty()
    }
}
