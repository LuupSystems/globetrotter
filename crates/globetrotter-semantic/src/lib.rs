//! Cross-lingual semantic drift detection for translation files.
//!
//! Given a translation key and its strings in several languages, this crate
//! embeds each string with a multilingual sentence-embedding model and reports
//! language pairs whose meanings have drifted apart (e.g. one says "houses",
//! another "horses"). It is intentionally a *ranked review aid*, not a
//! pass/fail check: similarity is fuzzy and threshold-dependent, so the output
//! is meant to be eyeballed, lowest-similarity pairs first.
//!
//! The public API is pure data in, pure data out ([`KeyInput`] →
//! [`Drift`]); it has no knowledge of globetrotter's model, diagnostics, or
//! configuration types.
//!
//! Embeddings run on the CPU via [`candle`](https://github.com/huggingface/candle).
//! Model weights are downloaded on demand and cached locally.
//!
//! Each model uses its own trained sentence-embedding head: mean-pooling for the
//! e5 family, and `CLS` → dense+`tanh` → normalize for `LaBSE`. Using the wrong
//! head badly degrades similarity, especially on short strings.

pub mod clwe;
pub mod data;
mod embed;
pub mod lexicon;
mod model;
pub mod normalize;
mod rerank;

pub use clwe::WordVectors;
pub use embed::Embedder;
pub use lexicon::Lexicon;
pub use model::{Model, UnknownModel};
pub use normalize::normalize;
pub use rerank::CrossEncoder;

use model::Architecture;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// How many texts are embedded in a single forward pass.
const EMBED_BATCH: usize = 16;

/// Errors that can occur while loading a model or computing embeddings.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Downloading or resolving model files from the Hugging Face Hub failed.
    #[error("failed to fetch model files: {0}")]
    Hub(#[from] hf_hub::api::sync::ApiError),

    /// Reading a model file from disk failed.
    #[error("failed to read model file: {0}")]
    Io(#[from] std::io::Error),

    /// Parsing the model `config.json` failed.
    #[error("failed to parse model config: {0}")]
    Config(#[from] serde_json::Error),

    /// Loading or running the tokenizer failed.
    #[error("tokenizer error: {0}")]
    Tokenizer(Box<dyn std::error::Error + Send + Sync>),

    /// An embedding tensor operation failed.
    #[error("embedding failed: {0}")]
    Candle(#[from] candle_core::Error),

    /// A cross-encoder was requested for a model that is not one.
    #[error("model is not a cross-encoder")]
    NotACrossEncoder,

    /// Hybrid mode was requested without a data directory.
    #[error("hybrid mode requires a data directory with word vectors and dictionaries")]
    MissingDataDir,

    /// Downloading a hybrid data file failed.
    #[error("failed to download {url}: {message}")]
    Download {
        /// The URL that failed.
        url: String,
        /// A human-readable cause.
        message: String,
    },
}

impl Error {
    /// Wrap a boxed tokenizer error (its error type is not `'static`-friendly
    /// for `#[from]`).
    pub(crate) fn tokenizer(source: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self::Tokenizer(source)
    }
}

/// Options controlling a [`Analyzer`].
#[derive(Debug, Clone)]
pub struct Options {
    /// The model used for multi-word strings (and, when `hybrid` is off, for
    /// every pair). In hybrid mode this must be a cross-encoder.
    pub model: Model,
    /// Enable the hybrid router: short strings are checked with a bilingual
    /// lexicon plus cross-lingual word vectors, multi-word strings with `model`.
    pub hybrid: bool,
    /// Only language pairs with similarity strictly below this value are
    /// reported. Range is roughly `[0.0, 1.0]`; `1.0` reports every pair.
    pub threshold: f32,
    /// Similarity threshold for the cross-lingual word-vector check in hybrid
    /// mode. Word-vector cosines run lower than transformer scores, so this is
    /// lower than `threshold`.
    pub clwe_threshold: f32,
    /// In single-model mode, skip any pair where either string has fewer than
    /// this many words (short labels are unreliable for every model). In hybrid
    /// mode it is the boundary between the word-level and multi-word routes.
    pub min_words: usize,
    /// Directory holding the hybrid data files (`wiki.multi.<lang>.vec` and
    /// `<a>-<b>.txt` dictionaries). Required for hybrid mode.
    pub data_dir: Option<PathBuf>,
    /// Optional user glossary (tab-separated, language-code header) merged into
    /// the lexicon for app-specific terminology.
    pub glossary: Option<PathBuf>,
    /// Override for the Hugging Face cache directory.
    pub cache_dir: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            model: Model::default(),
            hybrid: false,
            threshold: 0.7,
            clwe_threshold: 0.35,
            min_words: 3,
            data_dir: None,
            glossary: None,
            cache_dir: None,
        }
    }
}

/// One language's string for a key.
#[derive(Debug, Clone, Copy)]
pub struct LanguageText<'a> {
    /// The language code (e.g. `en`, `de`).
    pub language: &'a str,
    /// The translated text.
    pub text: &'a str,
}

/// A translation key together with its per-language strings to compare.
#[derive(Debug, Clone)]
pub struct KeyInput<'a> {
    /// The dotted key path.
    pub key: &'a str,
    /// The key's strings, one per language.
    pub languages: Vec<LanguageText<'a>>,
}

/// A language pair within one key whose meanings appear to have drifted.
#[derive(Debug, Clone, PartialEq)]
pub struct Drift {
    /// The dotted key path.
    pub key: String,
    /// The first language code.
    pub language_a: String,
    /// The second language code.
    pub language_b: String,
    /// Cosine similarity of the two strings' embeddings, in `[-1.0, 1.0]`.
    pub similarity: f32,
}

/// Receives progress updates from a long-running analysis so a caller can drive
/// a progress bar. The no-op `()` implementation is used when progress isn't
/// needed (e.g. [`Analyzer::analyze`]).
pub trait Progress: Send + Sync {
    /// Set the total number of work units, called once before work begins.
    fn set_length(&self, total: u64) {
        let _ = total;
    }
    /// Advance completed work by `delta` units.
    fn inc(&self, delta: u64) {
        let _ = delta;
    }
    /// Update the status message (e.g. the file currently downloading).
    fn set_message(&self, message: String) {
        let _ = message;
    }
}

impl Progress for () {}

/// The loaded scoring backend behind an [`Analyzer`].
enum Backend {
    /// Bi-encoder: embed each string, compare with cosine similarity.
    BiEncoder(Box<Embedder>),
    /// Cross-encoder: score each pair of strings jointly.
    CrossEncoder(Box<CrossEncoder>),
    /// Hybrid router: lexicon + word vectors for short strings, a cross-encoder
    /// for multi-word strings.
    Hybrid(Box<Hybrid>),
}

/// A loaded model that scores cross-lingual similarity between translations.
pub struct Analyzer {
    backend: Backend,
    threshold: f32,
    clwe_threshold: f32,
    min_words: usize,
}

impl Analyzer {
    /// Load the configured model(s), downloading on first use.
    ///
    /// # Errors
    ///
    /// Returns an error if a model cannot be downloaded or loaded, or if hybrid
    /// mode is requested without a data directory or with a non-cross-encoder.
    pub fn load(options: &Options) -> Result<Self, Error> {
        let backend = if options.hybrid {
            let data_dir = options.data_dir.clone().ok_or(Error::MissingDataDir)?;
            let long = match options.model.architecture() {
                Architecture::BiEncoder => LongModel::Bi(Box::new(Embedder::load(
                    options.model,
                    options.cache_dir.clone(),
                )?)),
                Architecture::CrossEncoder => LongModel::Cross(Box::new(CrossEncoder::load(
                    options.model,
                    options.cache_dir.clone(),
                )?)),
            };
            Backend::Hybrid(Box::new(Hybrid {
                long,
                data_dir,
                glossary: options.glossary.clone(),
            }))
        } else {
            match options.model.architecture() {
                Architecture::BiEncoder => Backend::BiEncoder(Box::new(Embedder::load(
                    options.model,
                    options.cache_dir.clone(),
                )?)),
                Architecture::CrossEncoder => Backend::CrossEncoder(Box::new(CrossEncoder::load(
                    options.model,
                    options.cache_dir.clone(),
                )?)),
            }
        };
        Ok(Self {
            backend,
            threshold: options.threshold,
            clwe_threshold: options.clwe_threshold,
            min_words: options.min_words,
        })
    }

    /// Score every within-key language pair and return the drifted ones, sorted
    /// most-divergent first.
    ///
    /// Empty (or whitespace-only) strings and keys with fewer than two
    /// non-empty languages are skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if scoring fails.
    pub fn analyze(&self, keys: &[KeyInput<'_>]) -> Result<Vec<Drift>, Error> {
        let mut drifts = Vec::new();
        self.analyze_each(keys, &(), &mut |drift| drifts.push(drift))?;
        drifts.sort_by(|a, b| a.similarity.total_cmp(&b.similarity));
        Ok(drifts)
    }

    /// Score every within-key pair and call `sink` with each finding (pair below
    /// the threshold) *as it is computed*, so a caller can stream results during
    /// a long run. Progress is reported to `progress`.
    ///
    /// Findings arrive in scoring order (not sorted by similarity); the threshold
    /// is the filter, so every drifted pair is reported.
    ///
    /// # Errors
    ///
    /// Returns an error if scoring fails.
    pub fn analyze_each(
        &self,
        keys: &[KeyInput<'_>],
        progress: &dyn Progress,
        sink: &mut dyn FnMut(Drift),
    ) -> Result<(), Error> {
        let mut on_pair = |pair: ScoredPair| {
            if let Some(drift) = drift_for(keys, &pair) {
                sink(drift);
            }
        };
        match &self.backend {
            Backend::BiEncoder(embedder) => score_bi_encoder(
                embedder,
                keys,
                self.min_words,
                self.threshold,
                progress,
                &mut on_pair,
            ),
            Backend::CrossEncoder(model) => score_cross_encoder(
                model,
                keys,
                self.min_words,
                self.threshold,
                progress,
                &mut on_pair,
            ),
            Backend::Hybrid(hybrid) => hybrid.flag(
                keys,
                self.threshold,
                self.clwe_threshold,
                self.min_words,
                progress,
                &mut on_pair,
            ),
        }
    }
}

/// A within-key language pair, identified by key index and two language slots.
type PairIndex = (usize, usize, usize);

/// The model used for the multi-word route in a [`Hybrid`].
enum LongModel {
    /// Cross-encoder (NLI/reranker). Note: NLI models score cross-lingual
    /// sentence equivalence poorly; a bi-encoder like `LaBSE` is preferred here.
    Cross(Box<CrossEncoder>),
    /// Bi-encoder (e.g. `LaBSE`), purpose-built for cross-lingual matching.
    Bi(Box<Embedder>),
}

/// The hybrid router: bilingual lexicon + cross-lingual word vectors for short
/// strings, a sentence model for multi-word strings.
struct Hybrid {
    long: LongModel,
    data_dir: PathBuf,
    glossary: Option<PathBuf>,
}

impl Hybrid {
    /// Route every within-key pair and return the flagged (drifted) ones.
    ///
    /// Multi-word pairs (both sides `>= min_words` words) go to the cross-encoder
    /// and are flagged below `nli_threshold`. Shorter pairs are suppressed if the
    /// lexicon attests them, otherwise scored by word-vector cosine and flagged
    /// below `clwe_threshold`. Out-of-vocabulary short pairs are left unflagged.
    fn flag(
        &self,
        keys: &[KeyInput<'_>],
        nli_threshold: f32,
        clwe_threshold: f32,
        min_words: usize,
        progress: &dyn Progress,
        sink: &mut dyn FnMut(ScoredPair),
    ) -> Result<(), Error> {
        let mut long: Vec<(usize, usize, usize)> = Vec::new();
        let mut short: Vec<(usize, usize, usize)> = Vec::new();
        for triple in within_key_pairs(keys, 1) {
            let (key_index, lang_a, lang_b) = triple;
            // route on the normalized word count, so a placeholder-heavy label
            // like `{passed}/{total} passed` is treated as the single word it is.
            let words_a =
                text_of(keys, key_index, lang_a).map_or(0, |text| word_count(&normalize(text)));
            let words_b =
                text_of(keys, key_index, lang_b).map_or(0, |text| word_count(&normalize(text)));
            if words_a >= min_words && words_b >= min_words {
                long.push(triple);
            } else {
                short.push(triple);
            }
        }

        // count each pair once; the long route's own batching reports via `&()`.
        progress.set_length((long.len() + short.len()) as u64);
        self.flag_long(keys, &long, nli_threshold, sink)?;
        progress.inc(long.len() as u64);
        self.flag_short(keys, &short, clwe_threshold, sink, progress)
    }

    /// Multi-word route: score with the sentence model, emit pairs below
    /// threshold.
    fn flag_long(
        &self,
        keys: &[KeyInput<'_>],
        pairs: &[(usize, usize, usize)],
        threshold: f32,
        sink: &mut dyn FnMut(ScoredPair),
    ) -> Result<(), Error> {
        if pairs.is_empty() {
            return Ok(());
        }
        match &self.long {
            LongModel::Cross(model) => {
                let inputs: Vec<(String, String)> = pairs
                    .iter()
                    .filter_map(|&(key_index, lang_a, lang_b)| {
                        let a = text_of(keys, key_index, lang_a)?;
                        let b = text_of(keys, key_index, lang_b)?;
                        Some((a.to_string(), b.to_string()))
                    })
                    .collect();
                if inputs.len() != pairs.len() {
                    return Ok(());
                }
                emit_scored_pairs(model, &inputs, pairs, threshold, &(), sink)?;
            }
            LongModel::Bi(embedder) => {
                for ((key_index, lang_a, lang_b), similarity) in
                    embed_and_score(embedder, keys, pairs, &())?
                {
                    if similarity < threshold {
                        sink(ScoredPair {
                            key_index,
                            lang_a,
                            lang_b,
                            similarity,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Word-level route: lexicon suppression, then word-vector cosine.
    fn flag_short(
        &self,
        keys: &[KeyInput<'_>],
        pairs: &[(usize, usize, usize)],
        threshold: f32,
        sink: &mut dyn FnMut(ScoredPair),
        progress: &dyn Progress,
    ) -> Result<(), Error> {
        if pairs.is_empty() {
            return Ok(());
        }

        // collect the words each language needs (for a targeted vector load)
        // and the ordered language pairs (for the lexicon).
        let mut needed: HashMap<String, HashSet<String>> = HashMap::new();
        let mut lang_pairs: HashSet<(String, String)> = HashSet::new();
        for &(key_index, lang_a, lang_b) in pairs {
            if let (Some(a), Some(b)) = (
                lang_code(keys, key_index, lang_a),
                lang_code(keys, key_index, lang_b),
            ) {
                lang_pairs.insert((a.to_string(), b.to_string()));
                lang_pairs.insert((b.to_string(), a.to_string()));
            }
            for lang_index in [lang_a, lang_b] {
                let (Some(code), Some(text)) = (
                    lang_code(keys, key_index, lang_index),
                    text_of(keys, key_index, lang_index),
                ) else {
                    continue;
                };
                let entry = needed.entry(code.to_string()).or_default();
                for token in normalize(text).split_whitespace() {
                    if let Some(word) = clwe::clean(token) {
                        entry.insert(word);
                    }
                }
            }
        }
        let mut lexicon =
            Lexicon::load(&self.data_dir, &lang_pairs.into_iter().collect::<Vec<_>>())?;
        if let Some(glossary) = &self.glossary {
            lexicon.add_glossary(glossary)?;
        }
        let vectors = WordVectors::load(&self.data_dir, &needed)?;

        for &(key_index, lang_a, lang_b) in pairs {
            progress.inc(1);
            let (Some(code_a), Some(code_b)) = (
                lang_code(keys, key_index, lang_a),
                lang_code(keys, key_index, lang_b),
            ) else {
                continue;
            };
            let (Some(text_a), Some(text_b)) = (
                text_of(keys, key_index, lang_a),
                text_of(keys, key_index, lang_b),
            ) else {
                continue;
            };
            // strip placeholders/markup before matching.
            let norm_a = normalize(text_a);
            let norm_b = normalize(text_b);
            // lexicon hit -> almost certainly correct, suppress.
            if lexicon.attested(code_a, &norm_a, code_b, &norm_b) {
                continue;
            }
            // otherwise fall back to the word-vector cosine; out-of-vocabulary
            // pairs are left unflagged (unverified, not evidence of drift).
            if let Some(similarity) = vectors.similarity(code_a, &norm_a, code_b, &norm_b)
                && similarity < threshold
            {
                sink(ScoredPair {
                    key_index,
                    lang_a,
                    lang_b,
                    similarity,
                });
            }
        }
        Ok(())
    }
}

/// The language code of a key's language slot.
fn lang_code<'a>(keys: &[KeyInput<'a>], key_index: usize, lang_index: usize) -> Option<&'a str> {
    keys.get(key_index)
        .and_then(|key| key.languages.get(lang_index))
        .map(|language| language.language)
}

/// A scored language pair within a key, before threshold filtering.
struct ScoredPair {
    key_index: usize,
    lang_a: usize,
    lang_b: usize,
    similarity: f32,
}

/// Build a [`Drift`] from a scored pair, resolving the language codes.
fn drift_for(keys: &[KeyInput<'_>], pair: &ScoredPair) -> Option<Drift> {
    let key = keys.get(pair.key_index)?;
    let name_a = key.languages.get(pair.lang_a)?;
    let name_b = key.languages.get(pair.lang_b)?;
    Some(Drift {
        key: key.key.to_string(),
        language_a: name_a.language.to_string(),
        language_b: name_b.language.to_string(),
        similarity: pair.similarity,
    })
}

/// Indices of each key's languages that have at least `min_words` words, paired
/// up within the key. The word-count guard skips short labels, which every
/// model scores unreliably.
fn within_key_pairs(keys: &[KeyInput<'_>], min_words: usize) -> Vec<(usize, usize, usize)> {
    let required = min_words.max(1);
    let mut pairs = Vec::new();
    for (key_index, key) in keys.iter().enumerate() {
        let eligible: Vec<usize> = key
            .languages
            .iter()
            .enumerate()
            .filter(|(_, language)| word_count(language.text) >= required)
            .map(|(index, _)| index)
            .collect();
        for (offset, &lang_a) in eligible.iter().enumerate() {
            for &lang_b in eligible.iter().skip(offset + 1) {
                pairs.push((key_index, lang_a, lang_b));
            }
        }
    }
    pairs
}

/// Number of whitespace-separated words in `text`.
fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Look up the trimmed text of a key's language slot.
fn text_of<'a>(keys: &[KeyInput<'a>], key_index: usize, lang_index: usize) -> Option<&'a str> {
    keys.get(key_index)
        .and_then(|key| key.languages.get(lang_index))
        .map(|language| language.text.trim())
}

/// Bi-encoder scoring: embed every eligible string once, then emit each pair
/// scoring below `threshold`.
fn score_bi_encoder(
    embedder: &Embedder,
    keys: &[KeyInput<'_>],
    min_words: usize,
    threshold: f32,
    progress: &dyn Progress,
    sink: &mut dyn FnMut(ScoredPair),
) -> Result<(), Error> {
    let pairs = within_key_pairs(keys, min_words);
    for ((key_index, lang_a, lang_b), similarity) in
        embed_and_score(embedder, keys, &pairs, progress)?
    {
        if similarity < threshold {
            sink(ScoredPair {
                key_index,
                lang_a,
                lang_b,
                similarity,
            });
        }
    }
    Ok(())
}

/// Embed every unique string referenced by `pairs` once, then return each pair's
/// cosine similarity.
fn embed_and_score(
    embedder: &Embedder,
    keys: &[KeyInput<'_>],
    pairs: &[PairIndex],
    progress: &dyn Progress,
) -> Result<Vec<(PairIndex, f32)>, Error> {
    // collect just the (key, lang) slots that take part in a pair.
    let mut owners: Vec<(usize, usize)> = Vec::new();
    let mut position_of: HashMap<(usize, usize), usize> = HashMap::new();
    for &(key_index, lang_a, lang_b) in pairs {
        for slot in [(key_index, lang_a), (key_index, lang_b)] {
            if let std::collections::hash_map::Entry::Vacant(entry) = position_of.entry(slot) {
                entry.insert(owners.len());
                owners.push(slot);
            }
        }
    }

    let mut texts: Vec<String> = Vec::with_capacity(owners.len());
    for &(key_index, lang_index) in &owners {
        texts.push(
            text_of(keys, key_index, lang_index)
                .unwrap_or_default()
                .to_string(),
        );
    }

    progress.set_length(texts.len() as u64);
    let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
    for chunk in texts.chunks(EMBED_BATCH) {
        vectors.extend(embedder.embed(chunk)?);
        progress.inc(chunk.len() as u64);
    }

    let mut scored = Vec::with_capacity(pairs.len());
    for &(key_index, lang_a, lang_b) in pairs {
        let (Some(&pos_a), Some(&pos_b)) = (
            position_of.get(&(key_index, lang_a)),
            position_of.get(&(key_index, lang_b)),
        ) else {
            continue;
        };
        let (Some(va), Some(vb)) = (vectors.get(pos_a), vectors.get(pos_b)) else {
            continue;
        };
        scored.push(((key_index, lang_a, lang_b), cosine(va, vb)));
    }
    Ok(scored)
}

/// Cross-encoder scoring: score each within-key pair jointly, emitting each pair
/// below `threshold` as its batch completes.
fn score_cross_encoder(
    model: &CrossEncoder,
    keys: &[KeyInput<'_>],
    min_words: usize,
    threshold: f32,
    progress: &dyn Progress,
    sink: &mut dyn FnMut(ScoredPair),
) -> Result<(), Error> {
    let pairs = within_key_pairs(keys, min_words);
    let inputs: Vec<(String, String)> = pairs
        .iter()
        .filter_map(|&(key_index, lang_a, lang_b)| {
            let a = text_of(keys, key_index, lang_a)?;
            let b = text_of(keys, key_index, lang_b)?;
            Some((a.to_string(), b.to_string()))
        })
        .collect();

    // `within_key_pairs` only yields non-empty languages, so `inputs` lines up
    // 1:1 with `pairs`.
    if inputs.len() != pairs.len() {
        return Ok(());
    }

    progress.set_length(inputs.len() as u64);
    emit_scored_pairs(model, &inputs, &pairs, threshold, progress, sink)
}

/// Score `inputs` with the cross-encoder and emit each `pairs[index]` whose
/// score is below `threshold`.
fn emit_scored_pairs(
    model: &CrossEncoder,
    inputs: &[(String, String)],
    pairs: &[PairIndex],
    threshold: f32,
    progress: &dyn Progress,
    sink: &mut dyn FnMut(ScoredPair),
) -> Result<(), Error> {
    model.score_pairs(inputs, progress, &mut |index, similarity| {
        if similarity < threshold
            && let Some(&(key_index, lang_a, lang_b)) = pairs.get(index)
        {
            sink(ScoredPair {
                key_index,
                lang_a,
                lang_b,
                similarity,
            });
        }
    })
}

/// Dot product of two equal-length vectors. Inputs are already L2-normalized,
/// so this is their cosine similarity, clamped to a valid range.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot.clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{Model, cosine};
    use std::str::FromStr;

    #[test]
    fn cosine_of_identical_unit_vectors_is_one() {
        let v = [0.6_f32, 0.8];
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_orthogonal_vectors_is_zero() {
        let a = [1.0_f32, 0.0];
        let b = [0.0_f32, 1.0];
        assert!(cosine(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn model_parses_aliases() {
        assert_eq!(Model::from_str("e5").unwrap(), Model::MultilingualE5Small);
        assert_eq!(
            Model::from_str("E5-Small").unwrap(),
            Model::MultilingualE5Small
        );
        assert_eq!(Model::from_str("labse").unwrap(), Model::Labse);
        assert!(Model::from_str("gpt").is_err());
    }

    #[test]
    fn model_ids_round_trip() {
        for model in Model::ALL {
            assert_eq!(Model::from_str(model.id()).unwrap(), model);
        }
    }

    /// End-to-end smoke test against a real model. Ignored by default because it
    /// downloads the weights; run with
    /// `cargo test -p globetrotter-semantic -- --ignored --nocapture`.
    #[test]
    #[ignore = "downloads model weights from the Hugging Face Hub"]
    fn embeds_and_ranks_real_translations() {
        use super::{Analyzer, KeyInput, LanguageText, Model, Options};

        let analyzer = Analyzer::load(&Options {
            model: Model::MultilingualE5Small,
            hybrid: false,
            threshold: 1.0,
            clwe_threshold: 0.35,
            min_words: 1,
            data_dir: None,
            glossary: None,
            cache_dir: None,
        })
        .expect("load model");

        let faithful = KeyInput {
            key: "faithful",
            languages: vec![
                LanguageText {
                    language: "en",
                    text: "Save your changes",
                },
                LanguageText {
                    language: "de",
                    text: "Speichern Sie Ihre Änderungen",
                },
            ],
        };
        let drifted = KeyInput {
            key: "drifted",
            languages: vec![
                LanguageText {
                    language: "en",
                    text: "Save your changes",
                },
                LanguageText {
                    language: "de",
                    text: "Das Pferd rennt über die Wiese",
                },
            ],
        };

        let drifts = analyzer.analyze(&[faithful, drifted]).expect("analyze");
        for drift in &drifts {
            eprintln!(
                "{}: {}/{} = {:.3}",
                drift.key, drift.language_a, drift.language_b, drift.similarity
            );
        }

        let score = |key: &str| {
            drifts
                .iter()
                .find(|d| d.key == key)
                .map(|d| d.similarity)
                .expect("both pairs reported at threshold 1.0")
        };
        assert!(
            score("faithful") > score("drifted"),
            "a faithful translation should score higher than a drifted one"
        );
        assert!(
            score("faithful") > 0.8,
            "a faithful translation should be highly similar"
        );
    }
}
