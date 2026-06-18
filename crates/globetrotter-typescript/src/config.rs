use std::path::PathBuf;

/// Configuration for a generated TypeScript interface type output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InterfaceTypeOutputConfig {
    /// File system path where the generated interface type will be written.
    pub path: PathBuf,
}

/// Configuration for a generated TypeScript declaration (`.d.ts`) output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DtsOutputConfig {
    /// File system path where the generated declaration file will be written.
    pub path: PathBuf,
}

/// Configuration for TypeScript translation code generation outputs.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutputConfig {
    /// Interface type outputs to generate.
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub interface_type: Vec<InterfaceTypeOutputConfig>,
}

impl OutputConfig {
    /// Return `true` if there are no configured outputs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.interface_type.is_empty()
    }
}
