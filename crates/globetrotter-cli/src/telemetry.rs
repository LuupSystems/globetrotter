//! Tracing subscriber and terminal-color configuration.

use color_eyre::eyre;
use termcolor::ColorChoice;
use tracing_subscriber::layer::SubscriberExt;

/// Output encoding for tracing events.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogFormat {
    /// Newline-delimited JSON events.
    Json,
    /// Compact human-readable events.
    PrettyCompact,
    /// Expanded human-readable events with structured fields.
    Pretty,
}

/// Command-line controls for tracing and terminal color.
#[derive(clap::Parser, Debug)]
pub struct LoggingOptions {
    /// Default tracing level when `RUST_LOG` does not provide a filter.
    #[arg(long = "log", env = "LOG_LEVEL", aliases = ["log-level"], global = true, help = "Log level. When using a more sophisticated logging setup using RUST_LOG environment variable, this option is overwritten.")]
    pub log_level: Option<tracing::metadata::Level>,

    /// Encoding used for tracing events.
    #[arg(
        long = "log-format",
        env = "LOG_FORMAT",
        global = true,
        help = "log format (json or pretty)"
    )]
    pub log_format: Option<crate::telemetry::LogFormat>,

    /// Color policy shared by tracing, progress output, and diagnostics.
    #[arg(
        long = "color",
        env = "GLOBETROTTER_COLOR",
        global = true,
        help = "enable or disable color"
    )]
    pub color_choice: Option<termcolor::ColorChoice>,
}

impl std::str::FromStr for LogFormat {
    type Err = eyre::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            s if s.eq_ignore_ascii_case("json") => Ok(LogFormat::Json),
            s if s.eq_ignore_ascii_case("pretty") => Ok(LogFormat::Pretty),
            s if s.eq_ignore_ascii_case("pretty-compact") => Ok(LogFormat::PrettyCompact),
            other => Err(eyre::eyre!("unknown log format: {other:?}")),
        }
    }
}

/// Installs the process-wide tracing subscriber.
///
/// Returns the selected format and whether ANSI color is enabled. An invalid
/// `RUST_LOG` filter is reported to stderr and falls back to `log_level`.
///
/// # Errors
///
/// Returns an error if the default filter cannot be parsed or another global
/// tracing subscriber has already been installed.
pub fn setup_logging(
    log_level: Option<tracing::metadata::Level>,
    log_format: Option<LogFormat>,
    color_choice: ColorChoice,
) -> eyre::Result<(LogFormat, bool)> {
    // Build the fallback filter, then let a valid `RUST_LOG` override it.
    let default_log_level = log_level.unwrap_or(tracing::metadata::Level::INFO);
    let default_log_directive = format!(
        "none,globetrotter={}",
        default_log_level.to_string().to_ascii_lowercase()
    );
    let default_env_filter = tracing_subscriber::filter::EnvFilter::builder()
        .with_regex(true)
        .with_default_directive(default_log_level.into())
        .parse(default_log_directive)?;

    let env_filter_directive = std::env::var("RUST_LOG").ok();
    let env_filter = match env_filter_directive {
        Some(directive) => {
            match tracing_subscriber::filter::EnvFilter::builder()
                .with_env_var(directive)
                .try_from_env()
            {
                Ok(env_filter) => env_filter,
                Err(err) => {
                    eprintln!("invalid log filter: {err}");
                    eprintln!("falling back to default logging");
                    default_env_filter
                }
            }
        }
        None => default_env_filter,
    };

    // Resolve the output format and whether it may emit ANSI escapes.
    let log_format = log_format.unwrap_or(LogFormat::PrettyCompact);
    let use_color = match color_choice {
        ColorChoice::Always | ColorChoice::AlwaysAnsi => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::io::IsTerminal::is_terminal(&std::io::stdout()),
    };

    // Build each supported formatting layer with the shared color policy.
    let fmt_layer_pretty = tracing_subscriber::fmt::Layer::new()
        .pretty()
        .without_time()
        .with_ansi(use_color)
        .fmt_fields(tracing_subscriber::fmt::format::PrettyFields::new())
        .with_writer(std::io::stdout);
    let fmt_layer_pretty_compact = tracing_subscriber::fmt::Layer::new()
        .compact()
        .without_time()
        .with_ansi(use_color)
        .with_writer(std::io::stdout);
    let fmt_layer_json = tracing_subscriber::fmt::Layer::new()
        .json()
        .compact()
        .without_time()
        .with_ansi(use_color)
        .with_writer(std::io::stdout);

    // Install only the layer selected for this process.
    let subscriber = tracing_subscriber::registry()
        .with(if log_format == LogFormat::Json {
            Some(fmt_layer_json)
        } else {
            None
        })
        .with(if log_format == LogFormat::PrettyCompact {
            Some(fmt_layer_pretty_compact)
        } else {
            None
        })
        .with(if log_format == LogFormat::Pretty {
            Some(fmt_layer_pretty)
        } else {
            None
        })
        .with(env_filter);
    tracing::subscriber::set_global_default(subscriber)?;
    Ok((log_format, use_color))
}
