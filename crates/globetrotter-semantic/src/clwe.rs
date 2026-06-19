//! Static cross-lingual word embeddings (the word2vec/MUSE approach).
//!
//! Unlike the sentence-transformer models, these are aligned per-word vectors:
//! translation-equivalent words across languages have high cosine similarity,
//! which is far more reliable than sentence encoders on single words. Vectors
//! are read from MUSE/fastText aligned `wiki.multi.<lang>.vec` files in a local
//! data directory.

use crate::Error;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Aligned word vectors for one or more languages, in a shared cross-lingual
/// space so cosine similarity is meaningful across languages.
pub struct WordVectors {
    /// `language -> (lowercased word -> L2-normalized vector)`.
    by_language: HashMap<String, HashMap<String, Vec<f32>>>,
}

impl WordVectors {
    /// Load only the `needed` words for each language from
    /// `<data_dir>/wiki.multi.<lang>.vec`, keeping memory small.
    ///
    /// Languages whose vector file is missing are skipped (their pairs simply
    /// score as out-of-vocabulary later).
    ///
    /// # Errors
    ///
    /// Returns an error if a present vector file cannot be read.
    pub fn load(data_dir: &Path, needed: &HashMap<String, HashSet<String>>) -> Result<Self, Error> {
        let mut by_language = HashMap::new();
        for (language, words) in needed {
            let path = data_dir.join(format!("wiki.multi.{language}.vec"));
            if !path.exists() {
                tracing::warn!(?path, "cross-lingual vectors missing for language");
                continue;
            }
            let vectors = load_vec_file(&path, words)?;
            by_language.insert(language.clone(), vectors);
        }
        Ok(Self { by_language })
    }

    /// The L2-normalized vector for a single word in a language, if present.
    fn word(&self, language: &str, word: &str) -> Option<&Vec<f32>> {
        self.by_language
            .get(language)
            .and_then(|words| words.get(&word.to_lowercase()))
    }

    /// A normalized vector for a (possibly multi-word) string: the L2-normalized
    /// mean of its in-vocabulary word vectors. Returns `None` if no word is in
    /// vocabulary.
    fn embed(&self, language: &str, text: &str) -> Option<Vec<f32>> {
        let mut sum: Vec<f32> = Vec::new();
        let mut count = 0u32;
        for token in text.split_whitespace() {
            let Some(cleaned) = clean(token) else {
                continue;
            };
            if let Some(vector) = self.word(language, &cleaned) {
                if sum.is_empty() {
                    sum.clone_from(vector);
                } else {
                    for (acc, value) in sum.iter_mut().zip(vector) {
                        *acc += value;
                    }
                }
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        normalize(&sum)
    }

    /// Cross-lingual cosine similarity of two strings, or `None` if either has
    /// no in-vocabulary words.
    #[must_use]
    pub fn similarity(
        &self,
        language_a: &str,
        text_a: &str,
        language_b: &str,
        text_b: &str,
    ) -> Option<f32> {
        let a = self.embed(language_a, text_a)?;
        let b = self.embed(language_b, text_b)?;
        let dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        Some(dot.clamp(-1.0, 1.0))
    }
}

/// Parse a `word2vec`-style text vector file, keeping only `wanted` words
/// (matched lowercased). The first line is a `<count> <dim>` header.
fn load_vec_file(
    path: &Path,
    wanted: &HashSet<String>,
) -> Result<HashMap<String, Vec<f32>>, Error> {
    use std::io::BufRead;

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut vectors = HashMap::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if index == 0 {
            continue; // header
        }
        let mut parts = line.split(' ');
        let Some(word) = parts.next() else {
            continue;
        };
        let lowered = word.to_lowercase();
        if !wanted.contains(&lowered) || vectors.contains_key(&lowered) {
            continue;
        }
        let values: Vec<f32> = parts
            .filter_map(|value| value.parse::<f32>().ok())
            .collect();
        if let Some(normalized) = normalize(&values) {
            vectors.insert(lowered, normalized);
        }
    }
    Ok(vectors)
}

/// Normalize a token for vector lookup: strip surrounding non-alphanumerics and
/// lowercase. Returns `None` if nothing remains.
pub(crate) fn clean(token: &str) -> Option<String> {
    let trimmed = token.trim_matches(|c: char| !c.is_alphanumeric());
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_lowercase())
    }
}

/// L2-normalize a vector, returning `None` if it is empty or zero.
fn normalize(values: &[f32]) -> Option<Vec<f32>> {
    if values.is_empty() {
        return None;
    }
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm == 0.0 {
        return None;
    }
    Some(values.iter().map(|value| value / norm).collect())
}
