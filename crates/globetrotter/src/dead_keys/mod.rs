//! Detects translation keys without references in their owning source roots.
//!
//! Each scan belongs to one config: source references are matched against that
//! catalog and its dynamic-call policy before diagnostics are combined.
//! Static calls are checked across all configs by [`call_checks`], because a call
//! may belong to any catalog that recognizes it.
//! The `tree-sitter` feature excludes comments and type-only references; without
//! it, a text scan provides conservative, less precise matches and checks no calls.

pub(crate) mod call_checks;
mod source;
#[cfg(feature = "tree-sitter")]
mod syntax;

use crate::config::usages::{DynamicUsages, UsageConfig};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use globetrotter_model::{
    Arguments,
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
    /// The key's declared template arguments, which static calls must match.
    pub arguments: Arguments,
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
    /// Static and prefixed dynamic calls, checked across configs by [`call_checks`].
    pub calls: Vec<call_checks::CallSource>,
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
        .with_message(format!(
            "translation key `{}` is potentially unused",
            key.key
        ))
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
    let mut calls = Vec::new();
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
        let references = source::scan(&path, &content, &usages.functions)
            .map_err(|err| std::io::Error::new(err.kind(), format!("{}: {err}", path.display())))?;
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
        // Text scans infer prefixes without call context, so only prefixes from recognized
        // calls are checked against the catalog.
        let prefixes: Vec<_> = if cfg!(feature = "tree-sitter") {
            references
                .dynamic
                .iter()
                .filter(|usage| usage.prefix.is_some())
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        if !references.calls.is_empty() || !prefixes.is_empty() {
            calls.push(call_checks::CallSource {
                path: path.clone(),
                content: content.clone(),
                calls: references.calls,
                prefixes,
            });
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
    Ok(UsageScan {
        unused,
        dynamic,
        calls,
    })
}

#[cfg(test)]
mod tests {
    use super::{DefinedKey, find_unused_keys};
    #[cfg(feature = "tree-sitter")]
    use crate::config::usages::{DynamicUsages, UsageConfig};
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
            arguments: globetrotter_model::Arguments::default(),
        };

        let diagnostics =
            find_unused_keys(&[key], std::slice::from_ref(&dir), &BTreeSet::new(), false)?;
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[cfg(feature = "tree-sitter")]
    fn defined_key(key: &str) -> DefinedKey {
        DefinedKey {
            key: key.to_string(),
            generated_identifiers: Vec::new(),
            file_id: 0,
            span: 0..0,
            allow: BTreeSet::new(),
            arguments: globetrotter_model::Arguments::default(),
        }
    }

    /// Typst documents are discovered by extension and follow the configured dynamic policy.
    #[cfg(feature = "tree-sitter")]
    #[test_util::test]
    fn typst_documents_follow_the_dynamic_usage_policy() -> eyre::Result<()> {
        let dir = temp_dir("typst")?;
        std::fs::write(
            dir.join("report.typ"),
            indoc::indoc! {r#"
                #import "translations.typ": translator
                #let t = translator("en")
                = #t("report.title")
                // #t("report.commented")
                #for kind in kinds [#t("report.status." + kind)]
            "#},
        )?;
        let keys = ["report.title", "report.commented", "report.status.open"].map(defined_key);
        let unused_under = |dynamic| -> eyre::Result<_> {
            let usages = UsageConfig {
                roots: vec![dir.clone()],
                dynamic,
                ..UsageConfig::default()
            };
            let scan = super::scan_config(&keys, &usages, &BTreeSet::new())?;
            let unused: Vec<String> = scan.unused.into_iter().map(|key| key.key).collect();
            let dynamic: usize = scan.dynamic.iter().map(|file| file.references.len()).sum();
            Ok((unused, dynamic))
        };

        // The literal call counts, the commented call does not, and the prefix keeps the
        // status subtree alive unless the policy denies dynamic usages.
        assert_eq!(
            unused_under(DynamicUsages::Allow)?,
            (vec!["report.commented".to_owned()], 0)
        );
        assert_eq!(
            unused_under(DynamicUsages::Warn)?,
            (vec!["report.commented".to_owned()], 1)
        );
        assert_eq!(
            unused_under(DynamicUsages::Deny)?,
            (
                vec![
                    "report.commented".to_owned(),
                    "report.status.open".to_owned()
                ],
                1
            )
        );

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
}
