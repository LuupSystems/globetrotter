use clap::builder::TypedValueParser;
use clap::{Parser, Subcommand, builder::PossibleValuesParser};
use globetrotter::model;
use globetrotter::model::Language;
use std::path::PathBuf;
use strum::VariantNames;

#[derive(Parser, Debug)]
pub struct FormatOptions {}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, strum::EnumString, strum::VariantNames,
)]
pub enum InputFormat {
    Json,
    Csv,
    Txt,
}

impl InputFormat {
    pub fn from_ext(extension: &str) -> Option<Self> {
        match extension.to_lowercase().as_str() {
            "json" => Some(Self::Json),
            "txt" => Some(Self::Txt),
            "csv" => Some(Self::Csv),
            other => None,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumString, strum::VariantNames,
)]
pub enum KeyStyle {
    /// my.variable.name
    #[strum(to_string = "dotted", serialize = "dotted")]
    Dotted,
    /// MY.VARIABLE.NAME
    #[strum(to_string = "upper-dotted", serialize = "upper-dotted")]
    UpperDotted,
    /// MY VARIABLE NAME
    #[strum(to_string = "upper", serialize = "upper")]
    Upper,
    /// my variable name
    #[strum(to_string = "lower", serialize = "lower")]
    Lower,
    /// My Variable Name
    #[strum(to_string = "title", serialize = "title")]
    Title,
    /// `MyVariableName`
    #[strum(to_string = "pascal", serialize = "pascal")]
    Pascal,
    // spellcheck:ignore-next-line
    /// mY vARIABLE nAME"
    #[strum(to_string = "toggle", serialize = "toggle")]
    Toggle,
    /// myVariableName
    #[strum(to_string = "camel", serialize = "camel")]
    Camel,
    /// alias for `Pascal`
    #[strum(to_string = "upper-camel", serialize = "upper-camel")]
    UpperCamel,
    /// `my_variable_name`
    #[strum(to_string = "snake", serialize = "snake")]
    Snake,
    /// `MY_VARIABLE_NAME`
    #[strum(to_string = "upper-snake", serialize = "upper-snake")]
    UpperSnake,
    /// alias for `UpperSnake`
    #[strum(to_string = "screaming-snake", serialize = "screaming-snake")]
    ScreamingSnake,
    /// my-variable-name
    #[strum(to_string = "kebab", serialize = "kebab")]
    Kebab,
    /// MY-VARIABLE-NAME
    #[strum(to_string = "upper-kebab", serialize = "upper-kebab")]
    UpperKebab,
    /// alias for `Kebab`
    #[strum(to_string = "hyphens", serialize = "hyphens")]
    Hyphens,
    /// alias for `UpperKebab`
    #[strum(to_string = "upper-hyphens", serialize = "upper-hyphens")]
    UpperHyphens,
    /// alias for `UpperKebab`
    #[strum(to_string = "cobol", serialize = "cobol")]
    Cobol,
    /// My-Variable-Name
    #[strum(to_string = "train", serialize = "train")]
    Train,
    /// myvariablename
    #[strum(to_string = "flat", serialize = "flat")]
    Flat,
    /// MYVARIABLENAME
    #[strum(to_string = "upper-flat", serialize = "upper-flat")]
    UpperFlat,
    /// mY vArIaBlE nAmE
    #[strum(to_string = "alternating", serialize = "alternating")]
    Alternating,
}

#[derive(thiserror::Error, Debug)]
#[error("custom key style {0:?}")]
pub struct CustomStyle(KeyStyle);

fn input_format_parser() -> impl TypedValueParser {
    PossibleValuesParser::new(InputFormat::VARIANTS).map(|s| s.parse::<InputFormat>().unwrap())
}

fn language_parser() -> impl TypedValueParser {
    PossibleValuesParser::new(Language::VARIANTS).map(|s| s.parse::<Language>().unwrap())
}

fn style_parser() -> impl TypedValueParser {
    PossibleValuesParser::new(KeyStyle::VARIANTS).map(|s| s.parse::<KeyStyle>().unwrap())
}

#[derive(Parser, Debug, Default)]
pub struct ConvertOptions {
    #[clap(short = 'i', long = "input")]
    pub input_path: PathBuf,

    #[clap(short = 'o', long = "output")]
    pub output_path: Option<PathBuf>,

    #[clap(
        short = 's',
        long = "sort",
        help = "sort translations, languages, and arguments"
    )]
    pub sort: Option<bool>,

    #[clap(long = "style",
        value_parser = style_parser(),
        help = "desired translation key style")]
    pub style: Option<KeyStyle>,

    #[clap(long = "prefix", help = "desired translation key prefix")]
    pub prefix: Option<String>,

    #[clap(long = "separator", help = "desired translation key prefix")]
    pub separator: Option<String>,

    #[clap(
        short = 'f',
        long = "format",
        value_parser = input_format_parser()
    )]
    pub input_format: Option<InputFormat>,

    #[clap(
        short = 'l',
        long = "lang",
        aliases = ["language"],
        value_parser = language_parser()
    )]
    pub languages: Vec<Language>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(name = "format", aliases = ["fmt"])]
    Format(FormatOptions),

    #[cfg(feature = "convert")]
    #[command(name = "convert")]
    Convert(ConvertOptions),
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Options {
    #[clap(flatten)]
    pub logging: crate::telemetry::LoggingOptions,

    #[clap(short = 'c', long = "config")]
    pub config_paths: Vec<PathBuf>,

    #[clap(
        short = 'i',
        long = "translation",
        aliases = ["input"],
    )]
    pub translations: Vec<PathBuf>,

    #[clap(
        long = "engine",
        aliases = ["template-engine"],
    )]
    pub template_engine: Option<model::TemplateEngine>,

    #[clap(
        long = "strict",
        action = clap::ArgAction::SetTrue,
    )]
    pub strict: Option<bool>,

    #[clap(
        long = "check",
        aliases = ["check-templates"],
        action = clap::ArgAction::SetTrue,
    )]
    pub check_templates: Option<bool>,

    #[clap(
        long = "absolute",
        aliases = ["print-absolute", "print-absolute-paths"],
        action = clap::ArgAction::SetTrue,
    )]
    pub print_absolute_paths: Option<bool>,

    #[clap(
        long = "dry-run",
        action = clap::ArgAction::SetTrue,
    )]
    pub dry_run: Option<bool>,

    #[clap(subcommand)]
    pub command: Option<Command>,
}
