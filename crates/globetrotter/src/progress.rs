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
        std::iter::repeat(fill).take(pad_len).collect::<String>()
    )
}

fn pad_right(value: &str, width: usize, fill: char) -> String {
    let len = safe_length(value);
    let pad_len = width.saturating_sub(len);
    format!(
        "{value}{}",
        std::iter::repeat(fill).take(pad_len).collect::<String>()
    )
}

pub fn relative_to(base_dir: Option<&Path>, path: &Path) -> PathBuf {
    base_dir
        .as_ref()
        .and_then(|base_dir| pathdiff::diff_paths(path, base_dir))
        .unwrap_or(path.to_path_buf())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logger {
    pub use_absolute_paths: bool,
    pub longest_config_name: usize,
    pub longest_target_name: usize,
}

impl Logger {
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
            use_absolute_paths: false,
            longest_target_name,
            longest_config_name,
        }
    }

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

    pub fn completed(&self, duration: &std::time::Duration) -> String {
        format!(
            "{} {}",
            std::iter::repeat(' ')
                .take(self.longest_config_name + self.longest_target_name + 2)
                .collect::<String>(),
            format!("completed in {duration:?}").bright_black(),
        )
    }

    pub fn dry_run_would_write(&self, path: &Path) -> colored::ColoredString {
        format!("{} would write {}", "DRY RUN:".yellow(), path.display()).bright_black()
    }
}

#[cfg(test)]
mod test {
    use colored::Colorize;
    use similar_asserts::assert_eq as sim_assert_eq;

    #[test]
    fn test_pad_right() {
        sim_assert_eq!(super::pad_right("test", 7, ' '), "test   ");
        sim_assert_eq!(super::pad_right("test", 2, ' '), "test");

        let width = 20;
        let colored = format!("-> {} <-", "color".green().bold());
        let expected = "-> color <-         ";
        sim_assert_eq!(expected.len(), width);
        sim_assert_eq!(
            super::strip_color(&super::pad_right(&colored, width, ' ')),
            expected
        );
    }

    #[test]
    fn test_pad_left() {
        sim_assert_eq!(super::pad_left("test", 7, ' '), "   test");
        sim_assert_eq!(super::pad_left("test", 2, ' '), "test");

        let width = 20;
        let colored = format!("-> {} <-", "color".green().bold());
        let expected = "         -> color <-";
        sim_assert_eq!(expected.len(), width);
        sim_assert_eq!(
            super::strip_color(&super::pad_left(&colored, width, ' ')),
            expected
        );
    }
}
