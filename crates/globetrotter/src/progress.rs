//! Alignment and path display for human-readable progress output.

use crate::{
    config::v1::{ConfigFile, Configs},
    model::Language,
    target::Target,
};
use colored::Colorize;
use std::path::{Path, PathBuf};

fn strip_color(value: &str) -> String {
    strip_ansi_escapes::strip_str(value)
}

fn safe_length(value: &str) -> usize {
    strip_color(value).len()
}

fn pad_left(value: &str, width: usize, fill: char) -> String {
    let len = safe_length(value);
    let pad_len = width.saturating_sub(len);
    format!(
        "{}{value}",
        std::iter::repeat_n(fill, pad_len).collect::<String>()
    )
}

fn pad_right(value: &str, width: usize, fill: char) -> String {
    let len = safe_length(value);
    let pad_len = width.saturating_sub(len);
    format!(
        "{value}{}",
        std::iter::repeat_n(fill, pad_len).collect::<String>()
    )
}

/// Computes `path` relative to `base_dir`, falling back to `path` unchanged.
///
/// The fallback applies when `base_dir` is `None` or no relative path can be
/// formed, such as between different Windows path prefixes.
#[must_use]
pub fn relative_to(base_dir: Option<&Path>, path: &Path) -> PathBuf {
    base_dir
        .as_ref()
        .and_then(|base_dir| pathdiff::diff_paths(path, base_dir))
        .unwrap_or(path.to_path_buf())
}

/// Formats aligned progress log lines for configs and targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logger {
    /// Display width reserved for config names.
    pub longest_config_name: usize,
    /// Display width reserved for target or language names.
    pub longest_target_name: usize,
}

impl Logger {
    /// Creates a logger sized to align output for the given configs.
    #[must_use]
    pub fn new<F>(configs: &Configs<F>) -> Self {
        let longest_config_name = configs
            .iter()
            .filter(|ConfigFile { config, .. }| !config.is_empty())
            .map(|ConfigFile { config, .. }| config.name.len())
            .max()
            .unwrap_or(0)
            + 3;

        let target_names = Target::iter().map(|target| target.to_string());
        let language_names = Language::iter().map(|language| language.to_string());
        let longest_target_name = target_names
            .chain(language_names)
            .map(|name| name.len())
            .max()
            .unwrap_or(0);

        Self {
            longest_config_name,
            longest_target_name,
        }
    }

    /// Formats the aligned log prefix for a config name and code target.
    #[must_use]
    pub fn target_log_prefix(&self, name: &str, target: Target) -> String {
        format!(
            "{}{}",
            pad_left(&name.green().to_string(), self.longest_config_name, ' '),
            pad_right(
                &format!("[{}]", target.to_string().to_lowercase().blue()),
                self.longest_target_name + 2,
                ' ',
            ),
        )
    }

    /// Formats the aligned log prefix for a config name and language.
    #[must_use]
    pub fn language_log_prefix(&self, name: &str, language: Language) -> String {
        format!(
            "{}{}",
            pad_left(&name.green().to_string(), self.longest_config_name, ' '),
            pad_right(
                &format!("[{}]", language.to_string().to_lowercase().bright_blue()),
                self.longest_target_name + 2,
                ' '
            )
        )
    }

    /// Formats a `completed in <duration>` line aligned to the output columns.
    #[must_use]
    pub fn completed(&self, duration: &std::time::Duration) -> String {
        format!(
            "{} {}",
            std::iter::repeat_n(' ', self.longest_config_name + self.longest_target_name + 2)
                .collect::<String>(),
            format!("completed in {duration:?}").bright_black(),
        )
    }

    /// Formats a dry-run notice for a path that would be written.
    #[must_use]
    pub fn dry_run_would_write(&self, path: &Path) -> colored::ColoredString {
        format!("{} would write {}", "DRY RUN:".yellow(), path.display()).bright_black()
    }
}

#[cfg(test)]
mod test {
    use colored::Colorize;
    use similar_asserts::assert_eq as sim_assert_eq;

    /// Right padding measures visible text rather than ANSI escape bytes.
    #[test]
    fn test_pad_right() {
        sim_assert_eq!(have: super::pad_right("test", 7, ' '), want: "test   ");
        sim_assert_eq!(have: super::pad_right("test", 2, ' '), want: "test");

        let width = 20;
        let colored = format!("-> {} <-", "color".green().bold());
        let want = "-> color <-         ";
        sim_assert_eq!(have: want.len(), want: width);
        sim_assert_eq!(
            have: super::strip_color(&super::pad_right(&colored, width, ' ')),
            want: want
        );
    }

    /// Left padding measures visible text rather than ANSI escape bytes.
    #[test]
    fn test_pad_left() {
        sim_assert_eq!(have: super::pad_left("test", 7, ' '), want: "   test");
        sim_assert_eq!(have: super::pad_left("test", 2, ' '), want: "test");

        let width = 20;
        let colored = format!("-> {} <-", "color".green().bold());
        let want = "         -> color <-";
        sim_assert_eq!(have: want.len(), want: width);
        sim_assert_eq!(
            have: super::strip_color(&super::pad_left(&colored, width, ' ')),
            want: want
        );
    }
}
