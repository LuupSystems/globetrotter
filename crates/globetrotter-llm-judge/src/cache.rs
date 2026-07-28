//! Content-addressed on-disk cache of judge verdicts.
//!
//! Each verdict is one small JSON file named by the BLAKE3 hash of everything
//! that determines it (rendered prompt, model, endpoint, generation settings),
//! sharded into 256 subdirectories by hash prefix. A hit means the key's text
//! and the judge configuration are both unchanged, so incremental lint runs
//! only pay for keys that actually changed.
//!
//! The cache is bounded by entry count, evicted least-recently-used: a hit
//! bumps the file's modification time, and [`Cache::enforce_capacity`] removes
//! the oldest entries once per run when over capacity.

use crate::Verdict;
use std::path::{Path, PathBuf};

/// Default [`Cache`] capacity, in verdicts.
///
/// Deliberately large: entries are ~100-byte files, so even at capacity the
/// cache stays in the tens of megabytes while covering many projects, models,
/// and prompt variants without evicting anything in practice.
pub const DEFAULT_CAPACITY: usize = 100_000;

/// A bounded, content-addressed verdict cache rooted at one directory.
pub struct Cache {
    root: PathBuf,
    capacity: usize,
}

impl Cache {
    /// Opens or creates the cache under `dir`, or under
    /// `<user cache dir>/globetrotter/llm-judge` when `dir` is `None`.
    ///
    /// Returns `Ok(None)` — caching disabled — when `capacity` is `0` or no
    /// cache location can be determined.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache directory cannot be created.
    pub fn open(dir: Option<&Path>, capacity: usize) -> Result<Option<Self>, std::io::Error> {
        if capacity == 0 {
            return Ok(None);
        }
        let root = match dir {
            Some(dir) => dir.to_path_buf(),
            None => match dirs::cache_dir() {
                Some(cache) => cache.join("globetrotter").join("llm-judge"),
                None => return Ok(None),
            },
        };
        std::fs::create_dir_all(&root)?;
        Ok(Some(Self { root, capacity }))
    }

    /// Computes a cache key from length-prefixed string parts.
    ///
    /// Length prefixes preserve part boundaries, so `("ab", "c")` and
    /// `("a", "bc")` cannot collide by concatenation.
    #[must_use]
    pub fn key(&self, parts: &[&str]) -> String {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }

    /// The sharded path for a cache `key`.
    fn entry_path(&self, key: &str) -> PathBuf {
        let shard = key.get(..2).unwrap_or("00");
        self.root.join(shard).join(format!("{key}.json"))
    }

    /// Look up a verdict, bumping its recency on a hit.
    ///
    /// An unreadable or unparsable entry (partial write, format change) is
    /// treated as a miss so it gets overwritten by the fresh verdict.
    #[must_use]
    pub fn lookup(&self, key: &str) -> Option<Verdict> {
        let path = self.entry_path(key);
        let data = std::fs::read(&path).ok()?;
        let verdict: Verdict = serde_json::from_slice(&data).ok()?;
        // Recency for LRU eviction; failure to bump is harmless (worst case the
        // entry ages out earlier than it should).
        if let Ok(file) = std::fs::File::open(&path) {
            let _ = file.set_modified(std::time::SystemTime::now());
        }
        Some(verdict)
    }

    /// Stores a verdict atomically.
    ///
    /// Writing to a temporary file before renaming prevents concurrent or
    /// interrupted runs from leaving torn entries.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry cannot be written.
    pub fn store(&self, key: &str, verdict: &Verdict) -> Result<(), std::io::Error> {
        let path = self.entry_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec(verdict)?;
        // The temp name is per-process: concurrent runs (e.g. two repos
        // sharing the user cache) storing the same key must not interleave
        // writes into one temp file before the rename.
        let tmp = path.with_extension(format!("{}.part", std::process::id()));
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Evicts least-recently-used entries until the cache is within capacity.
    ///
    /// Called once per run rather than per store: listing every entry is the
    /// expensive part, and a run can only exceed capacity by the number of keys
    /// it just judged.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache directory cannot be listed.
    pub fn enforce_capacity(&self) -> Result<(), std::io::Error> {
        let mut entries: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        for shard in std::fs::read_dir(&self.root)? {
            let shard = shard?;
            if !shard.file_type()?.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(shard.path())? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                if metadata.is_file() {
                    let modified = metadata
                        .modified()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    entries.push((modified, entry.path()));
                }
            }
        }
        if entries.len() <= self.capacity {
            return Ok(());
        }
        entries.sort_by_key(|(modified, _)| *modified);
        let excess = entries.len() - self.capacity;
        for (_, path) in entries.into_iter().take(excess) {
            // Racing runs may have removed it already; eviction is best-effort.
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Cache;
    use crate::Verdict;
    use color_eyre::eyre::{self, OptionExt, WrapErr};

    fn temp_cache(capacity: usize) -> eyre::Result<(tempfile::TempDir, Cache)> {
        let dir = tempfile::tempdir()?;
        let cache = Cache::open(Some(dir.path()), capacity)
            .wrap_err("failed to open the test cache")?
            .ok_or_eyre("test cache was disabled")?;
        Ok((dir, cache))
    }

    /// Stored verdicts can be retrieved by their content key.
    #[test_util::test]
    fn round_trips_a_verdict() {
        let (_dir, cache) = temp_cache(10)?;
        let key = cache.key(&["model", "prompt"]);
        assert!(cache.lookup(&key).is_none());

        let verdict = Verdict {
            consistent: true,
            issues: vec![],
        };
        cache.store(&key, &verdict)?;
        let hit = cache.lookup(&key).ok_or_eyre("cached verdict missing")?;
        assert!(hit.consistent);
    }

    /// Length-prefixed hashing must keep part boundaries significant.
    #[test_util::test]
    fn adjacent_parts_do_not_collide() {
        let (_dir, cache) = temp_cache(10)?;
        assert_ne!(cache.key(&["ab", "c"]), cache.key(&["a", "bc"]));
    }

    /// A zero capacity disables cache creation explicitly.
    #[test_util::test]
    fn zero_capacity_disables_caching() {
        let dir = tempfile::tempdir()?;
        assert!(Cache::open(Some(dir.path()), 0)?.is_none());
    }

    /// Capacity enforcement retains exactly the configured number of entries.
    #[test_util::test]
    fn evicts_down_to_capacity() {
        let (dir, cache) = temp_cache(2)?;
        let verdict = Verdict {
            consistent: true,
            issues: vec![],
        };
        for name in ["a", "b", "c", "d"] {
            let key = cache.key(&[name]);
            cache.store(&key, &verdict)?;
        }
        cache.enforce_capacity()?;

        // Exactly `capacity` entries survive eviction.
        let remaining: usize = walkdir_count(dir.path())?;
        assert_eq!(remaining, 2);
    }

    /// Counts regular files under `root` recursively.
    fn walkdir_count(root: &std::path::Path) -> eyre::Result<usize> {
        let mut count = 0;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let file_type = entry.file_type()?;
                if file_type.is_dir() {
                    stack.push(entry.path());
                } else {
                    count += 1;
                }
            }
        }
        Ok(count)
    }
}
