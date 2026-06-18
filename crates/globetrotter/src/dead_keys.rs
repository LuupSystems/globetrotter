//! Detection of translation keys that are never referenced in source code.
//!
//! Globetrotter keys appear verbatim as string literals in generated output and
//! in call sites (e.g. `t("upload.title")`), so a key is considered used if its
//! exact dotted string occurs — as a whole token — anywhere in the scanned
//! source tree.

use codespan_reporting::diagnostic::{Diagnostic, Label};
use globetrotter_model::{
    diagnostics::{DiagnosticExt, FileId, Span},
    lint::{codes, is_allowed},
};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

/// Directory names never descended into while scanning for key usages.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "out",
    "target",
    ".next",
    ".turbo",
    ".svelte-kit",
    ".cache",
    "coverage",
    "vendor",
];

/// File extensions scanned for key usages. Notably excludes `json`/`toml` so
/// that generated translation files and the source `.toml` files do not mark
/// every key as used.
const SOURCE_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "vue", "svelte", "astro", "html", "htm", "rs", "go",
    "py", "rb", "php", "java", "kt", "swift", "dart", "ex", "exs", "lua", "zig", "cs",
];

/// Files larger than this are skipped (likely generated or binary).
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// A translation key as defined in a translation file.
pub struct DefinedKey {
    /// The fully-resolved dotted key.
    pub key: String,
    /// All canonical forms a usage may take: the dotted key plus each enabled
    /// target's generated identifier (e.g. the Rust enum variant). The key is
    /// considered used if any form is found in the scanned source.
    pub forms: Vec<String>,
    /// The source file the key was defined in.
    pub file_id: FileId,
    /// The span of the key within its source file.
    pub span: Span,
    /// Lint codes suppressed for this key.
    pub allow: BTreeSet<String>,
}

fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// Whether the match at `start..end` is bounded by non-key characters, so that
/// e.g. searching for `a.b` does not match inside `a.b.c`.
fn is_whole_token(content: &str, start: usize, end: usize) -> bool {
    let before_ok = content
        .get(..start)
        .and_then(|prefix| prefix.chars().next_back())
        .is_none_or(|c| !is_key_char(c));
    let after_ok = content
        .get(end..)
        .and_then(|suffix| suffix.chars().next())
        .is_none_or(|c| !is_key_char(c));
    before_ok && after_ok
}

/// Collect literal key prefixes that immediately precede a `${ … }`
/// interpolation, e.g. `` t(`a.b.${x}`) `` yields `a.b.` and
/// `` `a.b.step${n}.x` `` yields `a.b.step`. Keys beginning with such a prefix
/// are assumed to be referenced dynamically and are not reported as unused.
fn collect_dynamic_prefixes(content: &str, prefixes: &mut HashSet<String>) {
    for (pos, _) in content.match_indices("${") {
        let Some(before) = content.get(..pos) else {
            continue;
        };
        let run: String = before
            .chars()
            .rev()
            .take_while(|character| is_key_char(*character))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        // require a dot so trivial prefixes don't mask large key subtrees.
        if run.contains('.') {
            prefixes.insert(run);
        }
    }
}

fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| SOURCE_EXTENSIONS.contains(&extension.to_lowercase().as_str()))
}

/// Whether a directory should be pruned from the usage scan: a generated-output
/// directory, a built-in skip directory, or another globetrotter source tree
/// (any directory holding a globetrotter config file).
fn is_pruned_dir(
    path: &Path,
    name: &str,
    scan_roots: &BTreeSet<PathBuf>,
    excluded: &BTreeSet<PathBuf>,
) -> bool {
    if SKIP_DIRS.contains(&name) {
        return true;
    }

    let canonical = path.canonicalize().ok();
    if canonical
        .as_ref()
        .is_some_and(|canonical| excluded.contains(canonical))
    {
        return true;
    }

    if canonical
        .as_ref()
        .is_some_and(|canonical| scan_roots.contains(canonical))
    {
        return false;
    }

    crate::config::config_file_names().any(|config| path.join(config).exists())
}

/// Find keys that never appear as a whole token in any scanned source file.
///
/// The scan respects `.gitignore`, skips other globetrotter source trees (any
/// directory containing a config file), and skips `excluded` canonicalized
/// directories (e.g. generated output) and the built-in [`SKIP_DIRS`].
///
/// # Errors
///
/// Returns an error if the pattern automaton cannot be built.
pub fn find_unused_keys(
    keys: &[DefinedKey],
    usage_dirs: &[PathBuf],
    excluded: &BTreeSet<PathBuf>,
    strict: bool,
) -> std::io::Result<Vec<Diagnostic<FileId>>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }

    let patterns: Vec<&str> = keys
        .iter()
        .flat_map(|key| key.forms.iter().map(String::as_str))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let searcher = aho_corasick::AhoCorasick::new(&patterns).map_err(std::io::Error::other)?;

    let Some((first, rest)) = usage_dirs.split_first() else {
        return Ok(Vec::new());
    };
    let mut builder = ignore::WalkBuilder::new(first);
    for dir in rest {
        builder.add(dir);
    }
    // respect .gitignore/.ignore even outside a git checkout.
    builder.require_git(false);
    let scan_roots: BTreeSet<PathBuf> = usage_dirs
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .collect();
    let excluded = excluded.clone();
    builder.filter_entry(move |entry| {
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_dir())
        {
            return true;
        }
        !is_pruned_dir(
            entry.path(),
            entry.file_name().to_string_lossy().as_ref(),
            &scan_roots,
            &excluded,
        )
    });

    let mut used: HashSet<usize> = HashSet::new();
    let mut dynamic_prefixes: HashSet<String> = HashSet::new();
    for entry in builder.build().flatten() {
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let path = entry.path();
        if !is_source_file(path) {
            continue;
        }
        if entry
            .metadata()
            .is_ok_and(|metadata| metadata.len() > MAX_FILE_BYTES)
        {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        for found in searcher.find_overlapping_iter(&content) {
            if is_whole_token(&content, found.start(), found.end()) {
                used.insert(found.pattern().as_usize());
            }
        }
        collect_dynamic_prefixes(&content, &mut dynamic_prefixes);
    }

    let matched_forms: HashSet<&str> = used
        .iter()
        .filter_map(|&index| patterns.get(index).copied())
        .collect();
    let dynamic: Vec<&str> = dynamic_prefixes.iter().map(String::as_str).collect();

    let mut diagnostics = Vec::new();
    for key in keys {
        let referenced = key
            .forms
            .iter()
            .any(|form| matched_forms.contains(form.as_str()))
            || dynamic.iter().any(|prefix| key.key.starts_with(prefix));
        if referenced || is_allowed(&key.allow, codes::UNUSED_KEY) {
            continue;
        }
        diagnostics.push(
            Diagnostic::warning_or_error(strict)
                .with_code(codes::UNUSED_KEY)
                .with_message(format!("translation key `{}` is never used", key.key))
                .with_labels(vec![
                    Label::primary(key.file_id, key.span.clone())
                        .with_message("defined here but not referenced in the scanned source"),
                ]),
        );
    }
    Ok(diagnostics)
}

#[cfg(test)]
mod tests {
    use super::{DefinedKey, find_unused_keys};
    use color_eyre::eyre;
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir(prefix: &str) -> eyre::Result<PathBuf> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);

        let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "globetrotter-dead-keys-{prefix}-{}-{nanos}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    #[test]
    fn empty_key_set_returns_no_diagnostics() -> eyre::Result<()> {
        let dir = temp_dir("empty")?;
        let diagnostics =
            find_unused_keys(&[], std::slice::from_ref(&dir), &BTreeSet::new(), false)?;
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn explicit_root_with_config_file_is_still_scanned() -> eyre::Result<()> {
        let dir = temp_dir("root-config")?;
        std::fs::write(dir.join(".globetrotter.yaml"), "version: 1\nconfigs: []\n")?;
        std::fs::write(
            dir.join("app.ts"),
            "export const title = t('upload.title');\n",
        )?;

        let key = DefinedKey {
            key: "upload.title".to_string(),
            forms: vec!["upload.title".to_string()],
            file_id: 0,
            span: 0..0,
            allow: BTreeSet::new(),
        };

        let diagnostics =
            find_unused_keys(&[key], std::slice::from_ref(&dir), &BTreeSet::new(), false)?;
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
}
