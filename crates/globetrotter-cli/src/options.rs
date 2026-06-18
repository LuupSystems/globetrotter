use clap::{
    Parser, Subcommand,
    builder::{PossibleValuesParser, TypedValueParser},
};
use globetrotter::model;
use globetrotter::model::Language;
use std::path::PathBuf;
use strum::VariantNames;

/// Options for the `format` subcommand.
#[derive(Parser, Debug)]
pub struct FormatOptions {}

/// Supported input formats when converting translations.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, strum::EnumString, strum::VariantNames,
)]
pub enum InputFormat {
    /// JSON input.
    Json,
    /// Comma-separated values input.
    Csv,
    /// Plain-text input.
    Txt,
}

impl InputFormat {
    /// Infer the input format from a file extension.
    #[cfg(feature = "convert")]
    #[must_use]
    pub fn from_ext(extension: &str) -> Option<Self> {
        match extension.to_lowercase().as_str() {
            "json" => Some(Self::Json),
            "txt" => Some(Self::Txt),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}

/// Styles for generated translation keys.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::VariantNames,
)]
pub enum KeyStyle {
    /// my.variable.name
    #[strum(to_string = "dotted")]
    Dotted,
    /// MY.VARIABLE.NAME
    #[strum(to_string = "upper-dotted")]
    UpperDotted,
    /// MY VARIABLE NAME
    #[strum(to_string = "upper")]
    Upper,
    /// my variable name
    #[strum(to_string = "lower")]
    Lower,
    /// My Variable Name
    #[strum(to_string = "title")]
    Title,
    /// `MyVariableName`
    #[strum(to_string = "pascal")]
    Pascal,
    /// myVariableName
    #[strum(to_string = "camel")]
    Camel,
    /// alias for `Pascal`
    #[strum(to_string = "upper-camel")]
    UpperCamel,
    /// `my_variable_name`
    #[strum(to_string = "snake")]
    Snake,
    /// `MY_VARIABLE_NAME`
    #[strum(to_string = "upper-snake")]
    UpperSnake,
    /// alias for `UpperSnake`
    #[strum(to_string = "screaming-snake")]
    ScreamingSnake,
    /// my-variable-name
    #[strum(to_string = "kebab")]
    Kebab,
    /// MY-VARIABLE-NAME
    #[strum(to_string = "upper-kebab")]
    UpperKebab,
    /// alias for `Kebab`
    #[strum(to_string = "hyphens")]
    Hyphens,
    /// alias for `UpperKebab`
    #[strum(to_string = "upper-hyphens")]
    UpperHyphens,
    /// alias for `UpperKebab`
    #[strum(to_string = "cobol")]
    Cobol,
    /// My-Variable-Name
    #[strum(to_string = "train")]
    Train,
    /// myvariablename
    #[strum(to_string = "flat")]
    Flat,
    /// MYVARIABLENAME
    #[strum(to_string = "upper-flat")]
    UpperFlat,
}

fn input_format_parser() -> impl TypedValueParser {
    PossibleValuesParser::new(InputFormat::VARIANTS).try_map(|s| s.parse::<InputFormat>())
}

fn language_parser() -> impl TypedValueParser {
    PossibleValuesParser::new(Language::VARIANTS).try_map(|s| s.parse::<Language>())
}

fn style_parser() -> impl TypedValueParser {
    PossibleValuesParser::new(KeyStyle::VARIANTS).try_map(|s| s.parse::<KeyStyle>())
}

/// Options for the `convert` subcommand.
#[derive(Parser, Debug, Default)]
pub struct ConvertOptions {
    /// Path to the input file to convert.
    #[clap(short = 'i', long = "input")]
    pub input_path: PathBuf,

    /// Path to write the converted output to.
    #[clap(short = 'o', long = "output")]
    pub output_path: Option<PathBuf>,

    /// Sort translations, languages, and arguments.
    #[clap(
        short = 's',
        long = "sort",
        help = "sort translations, languages, and arguments"
    )]
    pub sort: Option<bool>,

    /// Desired translation key style.
    #[clap(long = "style",
        value_parser = style_parser(),
        help = "desired translation key style")]
    pub style: Option<KeyStyle>,

    /// Desired translation key prefix.
    #[clap(long = "prefix", help = "desired translation key prefix")]
    pub prefix: Option<String>,

    /// Desired translation key separator.
    #[clap(long = "separator", help = "desired translation key prefix")]
    pub separator: Option<String>,

    /// Format of the input file. Inferred from the file extension when omitted.
    #[clap(
        short = 'f',
        long = "format",
        value_parser = input_format_parser()
    )]
    pub input_format: Option<InputFormat>,

    /// Languages to extract translations for.
    #[clap(
        short = 'l',
        long = "lang",
        aliases = ["language"],
        value_parser = language_parser()
    )]
    pub languages: Vec<Language>,
}

/// Top-level CLI commands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Format translation files in place.
    #[command(name = "format", aliases = ["fmt"])]
    Format(FormatOptions),

    /// Convert translations from another format into a globetrotter file.
    #[cfg(feature = "convert")]
    #[command(name = "convert")]
    Convert(ConvertOptions),
}

/// Top-level CLI options for the `globetrotter` binary.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Options {
    /// Logging and color output options.
    #[clap(flatten)]
    pub logging: crate::telemetry::LoggingOptions,

    /// Paths to globetrotter config files or directories to search for one.
    #[clap(short = 'c', long = "config")]
    pub config_paths: Vec<PathBuf>,

    /// Paths to translation files to process.
    #[clap(
        short = 'i',
        long = "translation",
        aliases = ["input"],
    )]
    pub translations: Vec<PathBuf>,

    /// Template engine to use for rendering translations.
    #[clap(
        long = "engine",
        aliases = ["template-engine"],
    )]
    pub template_engine: Option<model::TemplateEngine>,

    /// Treat warnings as errors.
    #[clap(
        long = "strict",
        action = clap::ArgAction::SetTrue,
    )]
    pub strict: Option<bool>,

    /// Validate that all templates render successfully.
    #[clap(
        long = "check",
        aliases = ["check-templates"],
        action = clap::ArgAction::SetTrue,
    )]
    pub check_templates: Option<bool>,

    /// Print absolute paths instead of paths relative to the common base directory.
    #[clap(
        long = "absolute",
        aliases = ["print-absolute", "print-absolute-paths"],
        action = clap::ArgAction::SetTrue,
    )]
    pub print_absolute_paths: Option<bool>,

    /// Run without writing any output files.
    #[clap(
        long = "dry-run",
        action = clap::ArgAction::SetTrue,
    )]
    pub dry_run: Option<bool>,

    /// Subcommand to execute. Runs the default generation flow when omitted.
    #[clap(subcommand)]
    pub command: Option<Command>,
}
