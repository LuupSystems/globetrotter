use clap::{Parser, Subcommand};
use globetrotter::model;
use std::path::PathBuf;

/// Options for the `format` subcommand.
#[derive(Parser, Debug)]
pub struct FormatOptions {}

/// Top-level CLI commands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Format translation files in place.
    #[command(name = "format", aliases = ["fmt"])]
    Format(FormatOptions),
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
