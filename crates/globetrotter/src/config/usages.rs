//! Source ownership and dynamic-call policy determine which keys count as used.

use std::path::PathBuf;

/// Dynamic translation calls follow this policy during unused-key checks.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Deserialize,
    serde::Serialize,
    strum::EnumString,
    strum::Display,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum DynamicUsages {
    /// Accept a specific literal prefix without reporting the dynamic call.
    #[default]
    Allow,
    /// Accept a specific literal prefix and report the dynamic call as a warning.
    Warn,
    /// Report the dynamic call as an error and do not accept its prefix as a usage.
    Deny,
}

/// Static translation calls are checked against the catalog under this policy.
///
/// A call is checked when its key argument is a string literal: its key must be defined, and the
/// arguments it visibly passes must match the key's declared arguments.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Deserialize,
    serde::Serialize,
    strum::EnumString,
    strum::Display,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum CallUsages {
    /// Do not report undefined keys or mismatched arguments.
    Allow,
    /// Report undefined keys and mismatched arguments as warnings.
    #[default]
    Warn,
    /// Report undefined keys and mismatched arguments as errors.
    Deny,
}

/// Associates a translation catalog with its application sources and usage policy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UsageConfig {
    /// Relative source roots resolve from the declaring configuration file.
    ///
    /// Empty roots disable scanning unless the caller supplies an override.
    pub roots: Vec<PathBuf>,
    /// This policy controls whether inferred dynamic prefixes keep keys alive.
    pub dynamic: DynamicUsages,
    /// This policy controls how undefined keys and mismatched arguments in static calls are
    /// reported.
    pub calls: CallUsages,
    /// Matching callees treat their first argument as a translation key.
    ///
    /// Names without a dot also match the final member of a callee, such as
    /// `i18n.t` for `t`; dotted names match the complete callee.
    pub functions: Vec<String>,
    /// Argument names that calls may omit because the host supplies them.
    ///
    /// A wrapper that fills in a default, such as a formality flag, makes the argument optional
    /// at call sites.
    /// Passing an implied argument explicitly is still checked against its declared type.
    pub implied_arguments: Vec<String>,
    /// Enables `.ignore` rules during source discovery.
    pub respect_ignore_files: bool,
    /// Enables `.gitignore`, Git's global excludes, and `.git/info/exclude` rules.
    pub respect_gitignore: bool,
}

impl Default for UsageConfig {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            dynamic: DynamicUsages::default(),
            calls: CallUsages::default(),
            functions: vec!["t".into(), "$t".into(), "translate".into()],
            implied_arguments: Vec::new(),
            respect_ignore_files: true,
            respect_gitignore: true,
        }
    }
}
