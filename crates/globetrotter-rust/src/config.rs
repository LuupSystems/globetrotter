use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutputConfig {
    #[cfg_attr(feature = "serde", serde(default))]
    pub output_paths: Vec<PathBuf>,
}

impl OutputConfig {
    pub fn is_empty(&self) -> bool {
        self.output_paths.is_empty()
    }
}
