//! Common path-prefix calculation for concise CLI output.

use std::path::{Component, Path, PathBuf};

/// Returns the deepest common base directory containing all given file paths.
///
/// Each input path is treated as a file, so only its parent participates. An
/// empty input or paths without a shared component return `None`.
pub fn common_base_directory(paths: &[impl AsRef<Path>]) -> Option<PathBuf> {
    // Seed the common prefix from the first file's directory.
    let first_path = paths.first().map(AsRef::as_ref)?;
    let mut common: Vec<Component> = match first_path.parent() {
        Some(parent) => parent.components().collect(),
        None => first_path.components().collect(),
    };

    // Narrow the prefix against each remaining file.
    for path in paths.iter().skip(1) {
        let p = path.as_ref();
        // Treat every input as a file by comparing its containing directory.
        let dir = p.parent().unwrap_or(p);
        let comps: Vec<Component> = dir.components().collect();

        // Keep only the leading components shared with the current prefix.
        let mut i = 0;
        while i < common.len() && i < comps.len() && common.get(i) == comps.get(i) {
            i += 1;
        }
        common.truncate(i);

        // Stop once no shared directory component remains.
        if common.is_empty() {
            return None;
        }
    }

    Some(PathBuf::from_iter(common))
}
