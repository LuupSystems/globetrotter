//! Bilingual lexicon used as a high-precision suppressor.
//!
//! If a word pair is a recorded translation in a bilingual dictionary, the pair
//! is almost certainly correct and is suppressed before any embedding check
//! runs. A *miss* is not evidence of drift (dictionaries have gaps), so missing
//! pairs fall through to the cross-lingual vector check.
//!
//! Dictionaries are read from MUSE `<a>-<b>.txt` files (one `word_a word_b` pair
//! per line) in a local data directory.

use crate::Error;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Recorded word-level translations between languages, queried in either
/// direction.
pub struct Lexicon {
    /// `(source_lang, target_lang) -> (source word -> {target words})`, both
    /// lowercased.
    by_direction: HashMap<(String, String), HashMap<String, HashSet<String>>>,
}

impl Lexicon {
    /// Load dictionaries for each ordered language pair from
    /// `<data_dir>/<a>-<b>.txt`. Missing files are skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if a present dictionary file cannot be read.
    pub fn load(data_dir: &Path, language_pairs: &[(String, String)]) -> Result<Self, Error> {
        let mut by_direction = HashMap::new();
        for (source, target) in language_pairs {
            let path = data_dir.join(format!("{source}-{target}.txt"));
            if !path.exists() {
                tracing::warn!(?path, "bilingual dictionary missing");
                continue;
            }
            let entries = load_dict_file(&path)?;
            by_direction.insert((source.clone(), target.clone()), entries);
        }
        Ok(Self { by_direction })
    }

    /// Whether `word_a` (in `language_a`) and `word_b` (in `language_b`) are a
    /// recorded translation in either direction.
    #[must_use]
    pub fn attested(&self, language_a: &str, word_a: &str, language_b: &str, word_b: &str) -> bool {
        let a = word_a.to_lowercase();
        let b = word_b.to_lowercase();
        self.has(language_a, &a, language_b, &b) || self.has(language_b, &b, language_a, &a)
    }

    fn has(&self, source: &str, word: &str, target: &str, translation: &str) -> bool {
        self.by_direction
            .get(&(source.to_string(), target.to_string()))
            .and_then(|entries| entries.get(word))
            .is_some_and(|translations| translations.contains(translation))
    }

    /// Merge a user-supplied glossary for app-specific terminology that general
    /// dictionaries miss.
    ///
    /// The file is tab-separated. The first non-comment line is a header of
    /// language codes (e.g. `en\tde\tfr`); each following row gives the term in
    /// each column's language (empty cells allowed). Every pair of non-empty
    /// cells in a row is recorded as a translation, in both directions.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn add_glossary(&mut self, path: &Path) -> Result<(), Error> {
        use std::io::BufRead;

        let reader = std::io::BufReader::new(std::fs::File::open(path)?);
        let mut languages: Vec<String> = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let cells: Vec<String> = line
                .split('\t')
                .map(|cell| cell.trim().to_string())
                .collect();
            if languages.is_empty() {
                languages = cells.iter().map(|code| code.to_lowercase()).collect();
                continue;
            }
            for (index, (lang_a, word_a)) in languages.iter().zip(&cells).enumerate() {
                if word_a.is_empty() {
                    continue;
                }
                for (lang_b, word_b) in languages.iter().zip(&cells).skip(index + 1) {
                    if word_b.is_empty() || lang_a == lang_b {
                        continue;
                    }
                    self.insert(lang_a, word_a, lang_b, word_b);
                    self.insert(lang_b, word_b, lang_a, word_a);
                }
            }
        }
        Ok(())
    }

    fn insert(&mut self, source: &str, word: &str, target: &str, translation: &str) {
        self.by_direction
            .entry((source.to_string(), target.to_string()))
            .or_default()
            .entry(word.to_lowercase())
            .or_default()
            .insert(translation.to_lowercase());
    }
}

/// Parse a bilingual dictionary: one `source<TAB>target` pair per line. As a
/// fallback (e.g. the original MUSE single-word dictionaries) a line with no tab
/// is split on whitespace into the first two tokens. Tab separation lets either
/// side be a multi-word phrase.
fn load_dict_file(path: &Path) -> Result<HashMap<String, HashSet<String>>, Error> {
    use std::io::BufRead;

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut entries: HashMap<String, HashSet<String>> = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let pair = if let Some((source, target)) = line.split_once('\t') {
            Some((source.trim(), target.trim()))
        } else {
            let mut parts = line.split_whitespace();
            match (parts.next(), parts.next()) {
                (Some(source), Some(target)) => Some((source, target)),
                _ => None,
            }
        };
        if let Some((source, target)) = pair
            && !source.is_empty()
            && !target.is_empty()
        {
            entries
                .entry(source.to_lowercase())
                .or_default()
                .insert(target.to_lowercase());
        }
    }
    Ok(entries)
}
