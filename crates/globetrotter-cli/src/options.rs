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

/// Cross-lingual model used for `--semantic`.
#[cfg(feature = "semantic")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum SemanticModel {
    /// `intfloat/multilingual-e5-small` — small, fast bi-encoder (~470 MB).
    #[clap(name = "e5-small", alias = "e5")]
    E5Small,
    /// `sentence-transformers/LaBSE` — bi-encoder purpose-built for cross-lingual
    /// matching (~1.9 GB); the default and the most reliable single model.
    #[default]
    #[clap(name = "labse")]
    Labse,
    /// `BAAI/bge-reranker-v2-m3` — cross-encoder reranker (~2.3 GB).
    #[clap(name = "bge-reranker", aliases = ["reranker", "bge"])]
    BgeRerankerV2M3,
    /// `MoritzLaurer/multilingual-MiniLMv2-L6-mnli-xnli` — NLI cross-encoder
    /// scoring semantic entailment (~430 MB).
    #[clap(name = "minilm-nli", aliases = ["nli", "minilm"])]
    MultilingualMiniLmNli,
    /// `MoritzLaurer/mDeBERTa-v3-base-mnli-xnli` — NLI cross-encoder on a
    /// DeBERTa-v3 backbone (~560 MB). Note: NLI models score cross-lingual
    /// *sentences* poorly (correct translations look like contradictions).
    #[clap(name = "mdeberta-nli", aliases = ["mdeberta", "deberta"])]
    MdebertaV3Nli,
}

#[cfg(feature = "semantic")]
impl From<SemanticModel> for globetrotter::executor::SemanticModel {
    fn from(model: SemanticModel) -> Self {
        match model {
            SemanticModel::E5Small => Self::MultilingualE5Small,
            SemanticModel::Labse => Self::Labse,
            SemanticModel::BgeRerankerV2M3 => Self::BgeRerankerV2M3,
            SemanticModel::MultilingualMiniLmNli => Self::MultilingualMiniLmNli,
            SemanticModel::MdebertaV3Nli => Self::MdebertaV3Nli,
        }
    }
}

/// Options for `--semantic` drift detection, flattened into [`LintOptions`].
///
/// Compiled in only with the `semantic` feature, so the whole flag group (and
/// its embedding dependency) disappears cleanly when the feature is off.
#[cfg(feature = "semantic")]
#[derive(Parser, Debug)]
pub struct SemanticOptions {
    /// [experimental] Detect cross-lingual semantic drift between a key's
    /// languages.
    ///
    /// Embeds each language string with a multilingual model (downloaded on
    /// first use) and reports pairs whose meanings appear to have drifted. This
    /// is a best-effort review aid printed as notes; it never fails the lint and
    /// produces many false positives on legitimately divergent translations, so
    /// every finding needs human review.
    #[clap(long = "semantic", action = clap::ArgAction::SetTrue)]
    pub enabled: bool,

    /// Model used by `--semantic`.
    #[clap(long = "semantic-model", value_enum, default_value_t = SemanticModel::default())]
    pub model: SemanticModel,

    /// Only report language pairs with similarity below this value.
    #[clap(long = "semantic-threshold", value_name = "SIM", default_value_t = 0.6)]
    pub threshold: f32,

    /// Skip pairs where either string has fewer than this many words.
    ///
    /// Short labels (e.g. single words) are unreliable for every model, so the
    /// default keeps the check to longer strings where drift is detectable. Set
    /// to 0 or 1 to check everything. In `--semantic-hybrid` mode this is the
    /// boundary between the word-level and multi-word routes instead.
    #[clap(long = "semantic-min-words", value_name = "N", default_value_t = 3)]
    pub min_words: usize,

    /// Enable the hybrid router for short strings.
    ///
    /// Instead of skipping short strings, check them with a bilingual lexicon
    /// (suppressing known translations) plus cross-lingual word vectors, which
    /// are far more reliable than the transformer models on single words.
    /// Requires `--semantic-data-dir`. Multi-word strings still use the model.
    #[clap(long = "semantic-hybrid", action = clap::ArgAction::SetTrue)]
    pub hybrid: bool,

    /// Word-vector similarity threshold for short strings in hybrid mode.
    #[clap(
        long = "semantic-clwe-threshold",
        value_name = "SIM",
        default_value_t = 0.35
    )]
    pub clwe_threshold: f32,

    /// Directory with the hybrid data files: `wiki.multi.<lang>.vec` aligned
    /// word vectors and `<a>-<b>.txt` bilingual dictionaries.
    #[clap(long = "semantic-data-dir", value_name = "DIR")]
    pub data_dir: Option<PathBuf>,

    /// User glossary of app-specific terms merged into the lexicon.
    ///
    /// Tab-separated; the first non-comment line is a language-code header
    /// (e.g. `en<TAB>de<TAB>fr`) and each following row gives the term in each
    /// column's language (empty cells allowed). Every pair of non-empty cells in
    /// a row is treated as a correct translation in both directions.
    #[clap(long = "semantic-glossary", value_name = "FILE")]
    pub glossary: Option<PathBuf>,

    /// Cap the number of findings reported (`0` = no cap, the default).
    ///
    /// Every language pair scoring below `--semantic-threshold` is reported, so
    /// a file with many drifted keys lists them all. Set a positive value only
    /// to truncate noisy output.
    #[clap(long = "semantic-top", value_name = "N", default_value_t = 0)]
    pub top: usize,
}

#[cfg(feature = "semantic")]
impl SemanticOptions {
    /// The executor parameters when `--semantic` is set, otherwise `None`.
    ///
    /// `explicit_cache` is `--cache-dir`/`GLOBETROTTER_CACHE_DIR` if the user set
    /// it, `default_cache` the OS user-cache fallback. Models reuse the standard
    /// Hugging Face cache unless an explicit cache is given; downloaded vectors
    /// and dictionaries go in `<cache>/semantic-data`.
    #[must_use]
    pub fn params(
        &self,
        explicit_cache: Option<&std::path::Path>,
        default_cache: &std::path::Path,
    ) -> Option<globetrotter::executor::SemanticParams> {
        let data_root = explicit_cache.unwrap_or(default_cache);
        self.enabled
            .then(|| globetrotter::executor::SemanticParams {
                model: self.model.into(),
                hybrid: self.hybrid,
                threshold: self.threshold,
                clwe_threshold: self.clwe_threshold,
                min_words: self.min_words,
                top: self.top,
                data_dir: self
                    .data_dir
                    .clone()
                    .or_else(|| self.hybrid.then(|| data_root.join("semantic-data"))),
                glossary: self.glossary.clone(),
                cache_dir: explicit_cache.map(|cache| cache.join("models")),
            })
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

    /// Semantic drift detection (only with the `semantic` feature).
    #[cfg(feature = "semantic")]
    #[clap(flatten)]
    pub semantic: SemanticOptions,
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

    /// Directory for cached models and downloaded semantic data.
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
