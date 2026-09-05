//! Detects translation keys without references in their owning source roots.
//!
//! Each scan belongs to one config: source references are matched against that
//! catalog and its dynamic-call policy before diagnostics are combined.
//! The `tree-sitter` feature excludes comments and type-only references; without
//! it, a text scan provides conservative, less precise matches.

mod source;
#[cfg(feature = "tree-sitter")]
mod syntax;

use crate::config::usages::{DynamicUsages, UsageConfig};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use globetrotter_model::{
    diagnostics::{DiagnosticExt, FileId, Span},
    lint::{AllowEntry, LintCode, is_allowed},
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Oversized source files fail the scan instead of producing incomplete usage results.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// A translation key as defined in a translation file.
#[derive(Clone)]
pub struct DefinedKey {
    /// The fully-resolved dotted key.
    pub key: String,
    /// Identifiers emitted by this config's typed output targets.
    ///
    /// These are distinct from literal key spellings even when the text is equal.
    pub generated_identifiers: Vec<String>,
    /// The source file the key was defined in.
    pub file_id: FileId,
    /// The span of the key within its source file.
    pub span: Span,
    /// Lint codes suppressed for this key.
    pub allow: BTreeSet<AllowEntry>,
}

fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-')
}

fn is_source_file(path: &Path) -> bool {
    source::Dialect::for_path(path).is_some()
}

/// Keeps one catalog's usage outcomes separate from diagnostic ownership.
#[derive(Default)]
pub(crate) struct UsageScan {
    pub unused: Vec<DefinedKey>,
    pub dynamic: Vec<DynamicSource>,
}

pub(crate) struct DynamicSource {
    pub path: PathBuf,
    pub content: String,
    pub references: Vec<source::DynamicReference>,
}

/// Finds keys unused under the default dynamic-call policy.
///
/// Prefer configured roots and policy through the executor for multi-config linting.
/// Ignore files and generated-output exclusions apply to every source root.
///
/// # Errors
///
/// Returns an error when a root or eligible source cannot be read or parsed.
pub fn find_unused_keys(
    keys: &[DefinedKey],
    usage_dirs: &[PathBuf],
    excluded: &BTreeSet<PathBuf>,
    strict: bool,
) -> std::io::Result<Vec<Diagnostic<FileId>>> {
    let usages = UsageConfig {
        roots: usage_dirs.to_vec(),
        ..UsageConfig::default()
    };
    let result = scan_config(keys, &usages, excluded)?;
    Ok(result
        .unused
        .iter()
        .map(|key| unused_diagnostic(key, strict))
        .collect())
}

pub(crate) fn unused_diagnostic(key: &DefinedKey, strict: bool) -> Diagnostic<FileId> {
    Diagnostic::warning_or_error(strict)
        .with_code(LintCode::UnusedKey)
        .with_message(format!("translation key `{}` is never used", key.key))
        .with_labels(vec![
            Label::primary(key.file_id, key.span.clone())
                .with_message("defined here but not referenced in this config's source roots"),
        ])
}

fn source_files(
    usages: &UsageConfig,
    excluded: &BTreeSet<PathBuf>,
) -> std::io::Result<Option<ignore::Walk>> {
    let scan_roots: BTreeSet<PathBuf> = usages
        .roots
        .iter()
        .map(|path| {
            path.canonicalize().map_err(|err| {
                std::io::Error::new(err.kind(), format!("{}: {err}", path.display()))
            })
        })
        .collect::<Result<_, _>>()?;
    let mut roots = scan_roots.iter();
    let Some(first) = roots.next() else {
        return Ok(None);
    };
    let mut builder = ignore::WalkBuilder::new(first);
    for root in roots {
        builder.add(root);
    }
    // Respect ignore files even when the source directory is not a Git checkout.
    builder
        .require_git(false)
        .hidden(false)
        .ignore(usages.respect_ignore_files)
        .git_ignore(usages.respect_gitignore)
        .git_global(usages.respect_gitignore)
        .git_exclude(usages.respect_gitignore);
    let excluded_dirs = excluded.clone();
    builder.filter_entry(move |entry| {
        !entry.file_type().is_some_and(|kind| kind.is_dir())
            || (entry.file_name() != ".git"
                && !entry
                    .path()
                    .canonicalize()
                    .is_ok_and(|path| excluded_dirs.contains(&path)))
    });

    Ok(Some(builder.build()))
}

pub(crate) fn scan_config(
    keys: &[DefinedKey],
    usages: &UsageConfig,
    excluded: &BTreeSet<PathBuf>,
) -> std::io::Result<UsageScan> {
    let Some(files) = source_files(usages, excluded)? else {
        return Ok(UsageScan::default());
    };
    let key_names: BTreeSet<&str> = keys.iter().map(|key| key.key.as_str()).collect();
    let generated: BTreeSet<&str> = keys
        .iter()
        .flat_map(|key| key.generated_identifiers.iter().map(String::as_str))
        .collect();

    let mut literals = BTreeSet::new();
    let mut identifiers = BTreeSet::new();
    let mut prefixes = BTreeSet::new();
    let mut dynamic = Vec::new();
    let mut scanned = BTreeSet::new();
    for entry in files {
        let entry = entry.map_err(std::io::Error::other)?;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) || !is_source_file(entry.path()) {
            continue;
        }
        let path = entry.path().canonicalize()?;
        if excluded.contains(&path) || !scanned.insert(path.clone()) {
            continue;
        }
        let metadata = entry.metadata().map_err(std::io::Error::other)?;
        if metadata.len() > MAX_FILE_BYTES {
            return Err(std::io::Error::other(format!(
                "{} exceeds the usage scanner's 4 MiB limit; exclude generated files with an ignore rule",
                path.display()
            )));
        }
        let content = std::fs::read_to_string(&path)?;
        let mut references = source::scan(&path, &content, &usages.functions)
            .map_err(|err| std::io::Error::new(err.kind(), format!("{}: {err}", path.display())))?;
        for argument in references.rust_arguments {
            if generated.contains(argument.identifier.as_str()) {
                references.identifiers.insert(argument.identifier);
            } else {
                references.dynamic.push(source::DynamicReference {
                    prefix: None,
                    span: argument.span,
                });
            }
        }
        literals.extend(
            references
                .literals
                .into_iter()
                .filter(|literal| key_names.contains(literal.as_str())),
        );
        identifiers.extend(
            references
                .identifiers
                .into_iter()
                .filter(|identifier| generated.contains(identifier.as_str())),
        );
        if usages.dynamic != DynamicUsages::Deny {
            prefixes.extend(
                references
                    .dynamic
                    .iter()
                    .filter_map(|usage| usage.prefix.clone()),
            );
        }
        if usages.dynamic != DynamicUsages::Allow && !references.dynamic.is_empty() {
            dynamic.push(DynamicSource {
                path,
                content,
                references: references.dynamic,
            });
        }
    }
    let unused = keys
        .iter()
        .filter(|key| {
            let referenced = literals.contains(&key.key)
                || key
                    .generated_identifiers
                    .iter()
                    .any(|form| identifiers.contains(form))
                || prefixes.iter().any(|prefix| key.key.starts_with(prefix));
            !referenced && !is_allowed(&key.allow, LintCode::UnusedKey)
        })
        .cloned()
        .collect();
    Ok(UsageScan { unused, dynamic })
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

    /// An empty catalog has no unused keys, even when a source root is configured.
    #[test_util::test]
    fn empty_key_set_returns_no_diagnostics() -> eyre::Result<()> {
        let dir = temp_dir("empty")?;
        let diagnostics =
            find_unused_keys(&[], std::slice::from_ref(&dir), &BTreeSet::new(), false)?;
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// An explicitly requested root is scanned even when it contains a config.
    #[test_util::test]
    fn explicit_root_with_config_file_is_still_scanned() -> eyre::Result<()> {
        let dir = temp_dir("root-config")?;
        std::fs::write(
            dir.join(".globetrotter.yaml"),
            indoc::indoc! {"
                version: 1
                configs: []
            "},
        )?;
        std::fs::write(
            dir.join("app.ts"),
            indoc::indoc! {"
                export const title = t('upload.title');
            "},
        )?;

        let key = DefinedKey {
            key: "upload.title".to_string(),
            generated_identifiers: Vec::new(),
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
