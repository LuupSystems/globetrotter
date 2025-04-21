use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutputConfig {
    #[cfg_attr(feature = "serde", serde(default))]
    pub output_paths: Vec<PathBuf>,
}

impl OutputConfig {
    #[must_use] pub fn is_empty(&self) -> bool {
        self.output_paths.is_empty()
    }
}
