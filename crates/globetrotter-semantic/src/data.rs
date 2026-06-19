//! Downloading the hybrid data files (cross-lingual word vectors and bilingual
//! dictionaries) into a local directory, so `--semantic --semantic-hybrid` works
//! without the user manually fetching anything.
//!
//! Word vectors come from the MUSE aligned-vectors release; the enriched
//! dictionaries (MUSE + Wiktionary + Mozilla UI strings) are published as a
//! globetrotter GitHub release asset.

use crate::{Error, Progress};
use std::io::{Read, Write};
use std::path::Path;

/// Base URL for the MUSE aligned `wiki.multi.<lang>.vec` word vectors.
const VECTOR_BASE: &str = "https://dl.fbaipublicfiles.com/arrival/vectors";

/// Base URL for the enriched `<a>-<b>.txt` dictionaries (a release asset).
const DICT_BASE: &str =
    "https://github.com/LuupSystems/globetrotter/releases/download/semantic-data-v1";

/// Ensure every hybrid data file for `languages` exists in `data_dir`,
/// downloading the missing ones: a `wiki.multi.<lang>.vec` per language and an
/// `<a>-<b>.txt` dictionary per ordered language pair.
///
/// Existing files are left untouched, so a user-provided directory (or a warm
/// cache) downloads nothing.
///
/// # Errors
///
/// Returns an error if the directory cannot be created or a download fails.
pub fn ensure_data(
    data_dir: &Path,
    languages: &[String],
    progress: &dyn Progress,
) -> Result<(), Error> {
    std::fs::create_dir_all(data_dir)?;

    let mut langs: Vec<&str> = languages.iter().map(String::as_str).collect();
    langs.sort_unstable();
    langs.dedup();

    for lang in &langs {
        let name = format!("wiki.multi.{lang}.vec");
        download_if_missing(
            &data_dir.join(&name),
            &format!("{VECTOR_BASE}/{name}"),
            progress,
        )?;
    }
    for source in &langs {
        for target in &langs {
            if source == target {
                continue;
            }
            let name = format!("{source}-{target}.txt");
            download_if_missing(
                &data_dir.join(&name),
                &format!("{DICT_BASE}/{name}"),
                progress,
            )?;
        }
    }
    Ok(())
}

/// Download `url` to `path` (atomically, via a `.part` temp file) unless it
/// already exists, reporting byte progress.
fn download_if_missing(path: &Path, url: &str, progress: &dyn Progress) -> Result<(), Error> {
    if path.exists() {
        return Ok(());
    }
    let name = path
        .file_name()
        .map(|file| file.to_string_lossy().into_owned())
        .unwrap_or_default();
    progress.set_message(format!("downloading {name}"));

    let fail = |message: String| Error::Download {
        url: url.to_string(),
        message,
    };

    let response = ureq::get(url).call().map_err(|err| fail(err.to_string()))?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    progress.set_length(total);

    let tmp = path.with_extension("part");
    let mut file = std::fs::File::create(&tmp)?;
    let mut reader = response.into_reader();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let chunk = buffer
            .get(..read)
            .ok_or_else(|| fail("short read".to_string()))?;
        file.write_all(chunk)?;
        progress.inc(read as u64);
    }
    file.flush()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
