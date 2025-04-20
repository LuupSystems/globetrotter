use std::path::{Component, Path, PathBuf};

/// Returns the deepest common base directory containing all given file paths.
///
/// Each input path is treated as a file and its parent is used.
/// If there is no common base, returns None.
pub fn common_base_directory(paths: &[impl AsRef<Path>]) -> Option<PathBuf> {
    // Convert first path into a vector of components (use its parent directory)
    let Some(first_path) = paths.first().map(AsRef::as_ref) else {
        return None;
    };
    let mut common: Vec<Component> = match first_path.parent() {
        Some(parent) => parent.components().collect(),
        None => first_path.components().collect(),
    };

    // Iterate over the rest of the paths
    for path in paths.iter().skip(1) {
        let p = path.as_ref();
        // Get the directory for this file
        let dir = p.parent().unwrap_or(p);
        let comps: Vec<Component> = dir.components().collect();

        // Find common prefix between `common` and `comps`
        let mut i = 0;
        while i < common.len() && i < comps.len() && common[i] == comps[i] {
            i += 1;
        }
        common.truncate(i);

        // If no common prefix remains, there's no common base directory.
        if common.is_empty() {
            return None;
        }
    }

    Some(PathBuf::from_iter(common))
}
