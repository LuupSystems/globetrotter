use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InterfaceTypeOutputConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DtsOutputConfig {
    pub path: PathBuf,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutputConfig {
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub interface_type: Vec<InterfaceTypeOutputConfig>,
}

impl OutputConfig {
    pub fn is_empty(&self) -> bool {
        self.interface_type.is_empty()
    }
}
