use clap::{Parser, Subcommand};
use globetrotter::model;
use std::path::PathBuf;

/// Order in which translation keys are sorted when formatting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum SortOrder {
    /// Sort keys from A to Z.
    #[default]
    Ascending,
    /// Sort keys from Z to A.
    Descending,
}

/// Options for the `format` subcommand.
#[derive(Parser, Debug)]
pub struct FormatOptions {
    /// Order in which translation keys are sorted.
    #[clap(long = "order", value_enum, default_value_t = SortOrder::Ascending)]
    pub order: SortOrder,

    /// Check whether files are already formatted instead of rewriting them.
    ///
    /// Exits with a non-zero status if any file would be reformatted.
    #[clap(long = "check", action = clap::ArgAction::SetTrue)]
    pub check: bool,
}

/// Reasoning effort requested from the judge model via `--llm-effort`.
#[cfg(feature = "llm-judge")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum LlmEffort {
    /// Send no reasoning-effort field at all.
    None,
    /// Minimal reasoning; fastest, least reliable.
    Low,
    /// Balanced reasoning; the tested sweet spot for drift detection.
    #[default]
    Medium,
    /// Maximal reasoning; slowest.
    High,
}

#[cfg(feature = "llm-judge")]
impl From<LlmEffort> for Option<globetrotter::executor::LlmJudgeEffort> {
    fn from(effort: LlmEffort) -> Self {
        use globetrotter::executor::LlmJudgeEffort;
        match effort {
            LlmEffort::None => None,
            LlmEffort::Low => Some(LlmJudgeEffort::Low),
            LlmEffort::Medium => Some(LlmJudgeEffort::Medium),
            LlmEffort::High => Some(LlmJudgeEffort::High),
        }
    }
}

/// Options for the `--llm-judge` review, flattened into [`LintOptions`].
///
/// Compiled in only with the `llm-judge` feature, so the whole flag group (and
/// its HTTP client dependency) disappears cleanly when the feature is off.
#[cfg(feature = "llm-judge")]
#[derive(Parser, Debug)]
pub struct LlmJudgeOptions {
    /// (experimental) Ask an LLM whether each key's languages all tell the user
    /// the same thing.
    ///
    /// Each key is judged in one request against an OpenAI-compatible endpoint
    /// (a local ollama by default). Findings are printed as notes with the
    /// model's reason and never fail the lint: the judge is tuned for recall,
    /// so treat every finding as a suggestion for inspection. Verdicts are
    /// cached, so re-runs only pay for changed keys.
    ///
    /// Use a capable model: in testing, 4B-class models missed real drift and
    /// hallucinated justifications, while `gemma4:12b` and `qwen3.5:9b` (Q4,
    /// 4K context) with medium reasoning effort worked well.
    #[clap(long = "llm-judge", action = clap::ArgAction::SetTrue)]
    pub enabled: bool,

    /// Base URL of the OpenAI-compatible endpoint.
    #[clap(
        long = "llm-base-url",
        value_name = "URL",
        default_value = "http://localhost:11434/v1",
        requires = "enabled"
    )]
    pub base_url: String,

    /// Model name as known to the endpoint.
    #[clap(
        long = "llm-model",
        value_name = "MODEL",
        default_value = "gemma4:12b",
        requires = "enabled"
    )]
    pub model: String,

    /// Name of the environment variable holding the API key.
    ///
    /// Local servers ignore the key, so leaving the variable unset is fine.
    #[clap(
        long = "llm-api-key-env",
        value_name = "ENV",
        default_value = "OPENAI_API_KEY",
        requires = "enabled"
    )]
    pub api_key_env: String,

    /// Maximum number of concurrent requests.
    #[clap(
        long = "llm-concurrency",
        value_name = "N",
        default_value_t = 8,
        requires = "enabled"
    )]
    pub concurrency: usize,

    /// Sampling temperature. The default `0` keeps verdicts reproducible (and
    /// cacheable) across runs.
    #[clap(
        long = "llm-temperature",
        value_name = "T",
        default_value_t = 0.0,
        requires = "enabled"
    )]
    pub temperature: f32,

    /// Reasoning effort, for models that support it.
    #[clap(
        long = "llm-effort",
        value_enum,
        default_value_t = LlmEffort::Medium,
        requires = "enabled"
    )]
    pub effort: LlmEffort,

    /// Minimum confidence a finding needs to be reported.
    ///
    /// Each finding carries the model's self-reported confidence (0 to 1),
    /// which is only loosely calibrated — treat it as a ranking of findings,
    /// not a probability. The default `0` reports everything, keeping recall
    /// maximal; raise it to trade recall for fewer false positives. The
    /// threshold applies after the verdict cache, so changing it re-filters
    /// cached verdicts without new requests.
    #[clap(
        long = "llm-min-confidence",
        value_name = "MIN",
        value_parser = parse_confidence,
        default_value_t = 0.0,
        requires = "enabled"
    )]
    pub min_confidence: f64,

    /// File with a custom judge prompt template.
    ///
    /// The template must contain the `{key}` and `{languages}` placeholders;
    /// all other braces pass through verbatim. Prompt wording strongly affects
    /// which findings a given model reports, and the best wording differs per
    /// model, so tune the template together with `--llm-model`.
    #[clap(long = "llm-prompt", value_name = "FILE", requires = "enabled")]
    pub prompt: Option<PathBuf>,

    /// Maximum number of cached verdicts kept on disk (least-recently-used
    /// eviction); `0` disables the cache entirely.
    #[clap(
        long = "llm-cache-capacity",
        value_name = "N",
        default_value_t = 100_000,
        requires = "enabled"
    )]
    pub cache_capacity: usize,
}

/// Parse a `--llm-min-confidence` value, requiring the 0–1 range so an
/// out-of-range threshold errors instead of silently suppressing everything.
#[cfg(feature = "llm-judge")]
fn parse_confidence(value: &str) -> Result<f64, String> {
    let parsed: f64 = value.parse().map_err(|error| format!("{error}"))?;
    if (0.0..=1.0).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(format!("`{parsed}` is not between 0 and 1"))
    }
}

#[cfg(feature = "llm-judge")]
impl LlmJudgeOptions {
    /// The executor parameters when `--llm-judge` is set, otherwise `None`.
    ///
    /// `cache_dir` is the resolved globetrotter cache directory; verdicts go in
    /// its `llm-judge` subdirectory.
    ///
    /// # Errors
    ///
    /// Returns an error if the `--llm-prompt` file cannot be read.
    pub fn params(
        &self,
        cache_dir: &std::path::Path,
    ) -> std::io::Result<Option<globetrotter::executor::LlmJudgeParams>> {
        if !self.enabled {
            return Ok(None);
        }
        let template = match &self.prompt {
            Some(path) => Some(std::fs::read_to_string(path).map_err(|error| {
                std::io::Error::new(
                    error.kind(),
                    format!("failed to read --llm-prompt {}: {error}", path.display()),
                )
            })?),
            None => None,
        };
        Ok(Some(globetrotter::executor::LlmJudgeParams {
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            api_key_env: self.api_key_env.clone(),
            concurrency: self.concurrency,
            temperature: self.temperature,
            effort: self.effort.into(),
            template,
            min_confidence: self.min_confidence,
            cache_dir: Some(cache_dir.join("llm-judge")),
            cache_capacity: self.cache_capacity,
        }))
    }
}

/// Options for the `lint` subcommand.
#[derive(Parser, Debug)]
pub struct LintOptions {
    /// Report translation keys never referenced in this source directory.
    ///
    /// Repeatable. When omitted, the unused-key check is skipped.
    #[clap(long = "usages", value_name = "DIR")]
    pub usages: Vec<PathBuf>,

    /// Disable duplicate-translation detection entirely.
    #[clap(long = "no-duplicates", action = clap::ArgAction::SetTrue)]
    pub no_duplicates: bool,

    /// LLM-judged consistency review (only with the `llm-judge` feature).
    #[cfg(feature = "llm-judge")]
    #[clap(flatten)]
    pub llm_judge: LlmJudgeOptions,
}

/// Top-level CLI commands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Format translation files in place.
    #[command(name = "format", aliases = ["fmt"])]
    Format(FormatOptions),

    /// Lint translation files and report any issues.
    #[command(name = "lint")]
    Lint(LintOptions),
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
    #[clap(short = 'c', long = "config", global = true)]
    pub config_paths: Vec<PathBuf>,

    /// Paths to translation files to process.
    #[clap(
        short = 'i',
        long = "translation",
        aliases = ["input"],
        global = true,
    )]
    pub translations: Vec<PathBuf>,

    /// Template engine to use for rendering translations.
    #[clap(
        long = "engine",
        aliases = ["template-engine"],
        global = true,
    )]
    pub template_engine: Option<model::TemplateEngine>,

    /// Treat warnings as errors.
    //
    // `num_args`/`default_missing_value` rather than `ArgAction::SetTrue`: `SetTrue` implies a
    // `default_value` of `false`, which leaves the field `Some(false)` when the flag is absent.
    // That is indistinguishable from an explicit `--strict=false`, so the overrides layer would
    // always win during `Settings::resolve` and the config file's key would never be consulted.
    // `None` has to mean "not specified" for the config fallback and the built-in default to be
    // reachable.
    #[clap(
        long = "strict",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        global = true,
    )]
    pub strict: Option<bool>,

    /// Validate that all templates render successfully.
    //
    // Not `global`: its `--check` long flag would collide with the `format`
    // subcommand's `--check`, and template checking only applies to the default
    // generation flow, which has no subcommand.
    // Same `None`-means-unspecified requirement as `--strict`: this one also falls back to
    // the config file's `check_templates`.
    #[clap(
        long = "check",
        aliases = ["check-templates"],
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
    )]
    pub check_templates: Option<bool>,

    /// Print absolute paths instead of paths relative to the common base directory.
    #[clap(
        long = "absolute",
        aliases = ["print-absolute", "print-absolute-paths"],
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        global = true,
    )]
    pub print_absolute_paths: Option<bool>,

    /// Run without writing any output files.
    #[clap(
        long = "dry-run",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        global = true,
    )]
    pub dry_run: Option<bool>,

    /// Process only the first N translation keys of each config.
    ///
    /// A debugging aid for large corpora: try a change — or the LLM judge —
    /// against a small subset of real translations before paying for a full
    /// run. Applies to linting and generation alike; the truncation is warned
    /// about, never silent.
    #[clap(long = "max-keys", value_name = "N", global = true)]
    pub max_keys: Option<usize>,

    /// Directory for cached data (e.g. LLM judge verdicts).
    ///
    /// Defaults to a `globetrotter` folder in the OS user cache directory
    /// (e.g. `~/.cache/globetrotter` on Linux).
    #[clap(
        long = "cache-dir",
        env = "GLOBETROTTER_CACHE_DIR",
        value_name = "DIR",
        global = true
    )]
    pub cache_dir: Option<PathBuf>,

    /// Subcommand to execute. Runs the default generation flow when omitted.
    #[clap(subcommand)]
    pub command: Option<Command>,
}

impl Options {
    /// The resolved cache directory: `--cache-dir`/`GLOBETROTTER_CACHE_DIR` if
    /// set, else `<os-cache>/globetrotter`, else a temp-dir fallback.
    #[must_use]
    pub fn cache_dir(&self) -> PathBuf {
        self.cache_dir
            .clone()
            .or_else(|| dirs::cache_dir().map(|dir| dir.join("globetrotter")))
            .unwrap_or_else(|| std::env::temp_dir().join("globetrotter"))
    }

    /// The settings overrides layer formed by the explicitly passed flags.
    ///
    /// Flags the user did not pass stay `None`, so each config's own settings
    /// and the built-in defaults stay reachable during resolution.
    #[must_use]
    pub fn settings_layer(&self) -> globetrotter::config::SettingsLayer {
        globetrotter::config::SettingsLayer {
            strict: self.strict,
            check_templates: self.check_templates,
            dry_run: self.dry_run,
            print_absolute_paths: self.print_absolute_paths,
            template_engine: self
                .template_engine
                .clone()
                .map(model::diagnostics::Spanned::dummy),
        }
    }
}
