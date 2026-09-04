//! Linting of translation files.
//!
//! These checks go beyond what code generation strictly requires: they look for
//! missing or empty translations, stray whitespace, broken templates,
//! inconsistent or undeclared template arguments, conditions that have no
//! effect, and duplicated strings.
//!
//! Every diagnostic carries a stable [`crate::lint::LintCode`]; a translation key
//! can suppress a code by listing its `lint:`-prefixed name in an `allow` key,
//! e.g. `allow = ["lint:duplicate"]` (or `allow = "lint:all"` to silence the key
//! entirely).
//! An `allow` on an enclosing table applies to every key below it.

use crate::{
    Language, TemplateEngine, Translation, Translations,
    diagnostics::{DiagnosticExt, FileId, Spanned},
    template,
};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// A stable identifier for a translation lint.
///
/// Diagnostics display it as `warning[code]: …`, and an `allow` list names it in
/// its prefixed kebab-case form (`lint:missing-language`, `lint:unused-key`, …).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    strum::Display,
    strum::EnumString,
    strum::VariantNames,
    strum::IntoStaticStr,
    strum::EnumIter,
    serde::Serialize,
    serde::Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum LintCode {
    /// A key is missing a required (or otherwise expected) language.
    MissingLanguage,
    /// A translation value is empty.
    Empty,
    /// A translation value has leading or trailing spaces/tabs.
    Whitespace,
    /// A template fails to compile.
    Template,
    /// A placeholder is used in some languages but missing in another.
    Placeholder,
    /// A template uses a placeholder not declared in `arguments`.
    UndeclaredArgument,
    /// A declared argument is never referenced by any template.
    UnusedArgument,
    /// A `{{#if}}`-style condition whose branches are identical, so it has no
    /// effect on the translation.
    DeadCondition,
    /// Two keys share an identical translation.
    Duplicate,
    /// Within one key, two or more languages have an identical translation
    /// (often a stale copy or an untranslated placeholder).
    IdenticalLanguages,
    /// A key is never referenced in the scanned source (see `--usages`).
    UnusedKey,
    /// A language of one key tells the user something different than the
    /// others, as judged by an LLM (see `--llm-judge`).
    LlmDrift,
}

/// Options controlling how translations are linted.
#[derive(Debug, Clone, Copy)]
pub struct LintOptions<'a> {
    /// Languages every key is required to provide. When empty, the union of
    /// languages present across all keys is expected instead.
    pub required_languages: &'a [Spanned<Language>],
    /// The template engine used to analyze template translations.
    ///
    /// Without one, the template checks are skipped and a note says so; an
    /// engine is never guessed.
    pub template_engine: Option<&'a Spanned<TemplateEngine>>,
    /// Whether issues are reported as errors rather than warnings.
    pub strict: bool,
    /// Whether to report keys that share an identical translation, and keys
    /// whose languages are identical to each other.
    pub detect_duplicates: bool,
}

/// An entry in an `allow` list: a specific [`LintCode`] to suppress, or the
/// catch-all `all` that suppresses every lint.
///
/// Entries always carry the `lint:` prefix (`lint:duplicate`, `lint:all`), which
/// keeps room for future non-lint directives without risking a collision with a
/// lint code.
///
/// `all` is intentionally *not* a [`LintCode`] variant — no diagnostic is ever
/// emitted with code `all`; it is only meaningful as an allow directive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AllowEntry {
    /// Suppress every lint.
    All,
    /// Suppress one specific lint.
    Code(LintCode),
}

impl AllowEntry {
    /// The namespace prefix every entry carries.
    pub const PREFIX: &'static str = "lint:";

    /// Every accepted entry, spelled exactly as it must be written.
    pub fn variants() -> impl Iterator<Item = String> {
        use strum::VariantNames;
        LintCode::VARIANTS
            .iter()
            .map(|code| format!("{}{code}", Self::PREFIX))
            .chain(std::iter::once(Self::All.to_string()))
    }
}

impl std::fmt::Display for AllowEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(Self::PREFIX)?;
        match self {
            Self::All => f.write_str("all"),
            Self::Code(code) => std::fmt::Display::fmt(code, f),
        }
    }
}

/// Why an `allow` entry could not be parsed into an [`AllowEntry`].
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseAllowEntryError {
    /// The entry does not carry the required namespace prefix.
    #[error("missing `{}` prefix", AllowEntry::PREFIX)]
    MissingPrefix,
    /// The name after the prefix is neither a lint code nor `all`.
    #[error("unknown lint code")]
    UnknownCode,
}

impl ParseAllowEntryError {
    /// A diagnostic note that helps fix `entry`.
    ///
    /// An entry that is otherwise valid and only lacks the prefix gets the
    /// corrected spelling; anything else gets the full list of accepted entries.
    #[must_use]
    pub fn note(self, entry: &str) -> String {
        let prefixed = format!("{}{entry}", AllowEntry::PREFIX);
        if self == Self::MissingPrefix && prefixed.parse::<AllowEntry>().is_ok() {
            return format!("write `{prefixed}` instead");
        }
        let valid = AllowEntry::variants()
            .map(|entry| format!("`{entry}`"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("valid entries are: {valid}")
    }
}

impl std::str::FromStr for AllowEntry {
    type Err = ParseAllowEntryError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let code = text
            .strip_prefix(Self::PREFIX)
            .ok_or(ParseAllowEntryError::MissingPrefix)?;
        if code == "all" {
            Ok(Self::All)
        } else {
            code.parse::<LintCode>()
                .map(Self::Code)
                .map_err(|_source| ParseAllowEntryError::UnknownCode)
        }
    }
}

/// Returns `true` if `code` or [`AllowEntry::All`] is in the allow list.
#[must_use]
pub fn is_allowed(allow: &BTreeSet<AllowEntry>, code: LintCode) -> bool {
    allow.contains(&AllowEntry::All) || allow.contains(&AllowEntry::Code(code))
}

fn emit(
    diagnostics: &mut Vec<Diagnostic<FileId>>,
    allow: &BTreeSet<AllowEntry>,
    code: LintCode,
    diagnostic: Diagnostic<FileId>,
) {
    if !is_allowed(allow, code) {
        diagnostics.push(diagnostic.with_code(code));
    }
}

/// Wrap a variable name in Handlebars delimiters for display, e.g. `{{name}}`.
fn braces(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    out.push_str("{{");
    out.push_str(name);
    out.push_str("}}");
    out
}

/// Decides once per catalog whether templates can be analyzed.
///
/// Templates are never analyzed with a guessed engine: a catalog written for
/// another engine would fail to compile under the wrong parser and drown the
/// real findings.
/// Skipping is announced with a note rather than a warning, so a catalog
/// without an engine still lints clean while pointing at the setting; a
/// catalog that declares no arguments has nothing to announce.
fn lint_analyzer(
    translations: &Translations,
    engine: Option<&Spanned<TemplateEngine>>,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> Option<template::Analyzer> {
    let has_templates = translations.0.values().any(Translation::is_template);
    let Some(engine) = engine else {
        if has_templates {
            diagnostics.push(
                Diagnostic::note()
                    .with_message("no template engine is configured, so templates were not checked")
                    .with_notes(vec![
                        "set `engine` in the config or pass `--engine` to check templates"
                            .to_string(),
                    ]),
            );
        }
        return None;
    };
    let analyzer = template::Analyzer::for_engine(engine.as_ref());
    if analyzer.is_none() && has_templates {
        diagnostics.push(Diagnostic::note().with_message(format!(
            "template checks are not supported for the `{engine}` engine, so templates were not checked"
        )));
    }
    analyzer
}

impl Translations {
    /// Lints the translations and appends any issues to `diagnostics`.
    ///
    /// Checks for missing and empty translations, surrounding whitespace,
    /// templates that fail to compile, placeholders that are inconsistent across
    /// languages, template arguments that are used but not declared (or declared
    /// but never used), and — when [`LintOptions::detect_duplicates`] is enabled
    /// — keys that share an identical translation.
    /// The template checks need [`LintOptions::template_engine`].
    /// Existing diagnostics are retained.
    /// Issues are warnings unless [`LintOptions::strict`] promotes them to
    /// errors.
    pub fn lint(&self, diagnostics: &mut Vec<Diagnostic<FileId>>, options: &LintOptions<'_>) {
        // Determine the language set against which every key is checked.
        let required: BTreeSet<Language> = options
            .required_languages
            .iter()
            .map(|lang| *lang.as_ref())
            .collect();
        // With no declared languages, use the catalog-wide union so partial
        // keys are still detected.
        let expected: BTreeSet<Language> = if required.is_empty() {
            self.0
                .values()
                .flat_map(|translation| translation.language.keys().copied())
                .collect()
        } else {
            required
        };

        let analyzer = lint_analyzer(self, options.template_engine, diagnostics);

        // Run completeness, content, and template checks per key.
        for (key, translation) in &self.0 {
            lint_translation(
                key,
                translation,
                &expected,
                analyzer,
                options.strict,
                diagnostics,
            );
        }

        // Run catalog-wide duplicate checks only when requested.
        if options.detect_duplicates {
            for translation in self.0.values() {
                lint_identical_languages(translation, options.strict, diagnostics);
            }
            lint_duplicates(self, options.strict, diagnostics);
        }
    }

    /// Adds `allow` entries to every key in the catalog.
    ///
    /// This is how suppressions declared outside the translation files reach
    /// the keys they cover, such as a config file's config-wide `allow` list.
    pub fn extend_allow(&mut self, allow: &BTreeSet<AllowEntry>) {
        for translation in self.0.values_mut() {
            translation.allow.extend(allow.iter().copied());
        }
    }
}

/// Reports languages within one key that share an identical translation
/// (after normalizing case and whitespace) — typically a value copied across
/// languages or an untranslated placeholder.
fn lint_identical_languages(
    translation: &Translation,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) {
    if is_allowed(&translation.allow, LintCode::IdenticalLanguages) {
        return;
    }

    let mut groups: BTreeMap<String, Vec<Language>> = BTreeMap::new();
    for (language, value) in &translation.language {
        let normalized = normalize(value.as_ref());
        if !normalized.is_empty() {
            groups.entry(normalized).or_default().push(*language);
        }
    }

    for (_normalized, mut languages) in groups {
        if languages.len() < 2 {
            continue;
        }
        languages.sort_unstable();

        let listed = languages
            .iter()
            .map(|language| format!("`{}`", language.code()))
            .collect::<Vec<_>>()
            .join(", ");
        let labels = languages
            .iter()
            .filter_map(|language| {
                let value = translation.language.get(language)?;
                Some(
                    Label::primary(translation.file_id, value.span.clone())
                        .with_message(format!("`{}`", language.code())),
                )
            })
            .collect();

        diagnostics.push(
            Diagnostic::warning_or_error(strict)
                .with_code(LintCode::IdenticalLanguages)
                .with_message(format!(
                    "{listed} translations are identical (possibly untranslated)"
                ))
                .with_labels(labels),
        );
    }
}

fn lint_translation(
    key: &Spanned<String>,
    translation: &Translation,
    expected_languages: &BTreeSet<Language>,
    analyzer: Option<template::Analyzer>,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) {
    let file_id = translation.file_id;
    let allow = &translation.allow;

    // Report missing required languages.
    for language in expected_languages {
        if !translation.language.contains_key(language) {
            emit(
                diagnostics,
                allow,
                LintCode::MissingLanguage,
                Diagnostic::warning_or_error(strict)
                    .with_message(format!("missing `{}` translation", language.code()))
                    .with_labels(vec![
                        Label::primary(file_id, key.span.clone()).with_message(format!(
                            "`{}` has no `{}` translation",
                            key.as_ref(),
                            language.code()
                        )),
                    ]),
            );
        }
    }

    // Check each available translation's content.
    for (language, value) in &translation.language {
        let text = value.as_ref();
        if text.trim().is_empty() {
            emit(
                diagnostics,
                allow,
                LintCode::Empty,
                Diagnostic::warning_or_error(strict)
                    .with_message(format!("empty `{}` translation", language.code()))
                    .with_labels(vec![
                        Label::primary(file_id, value.span.clone())
                            .with_message("this translation is empty"),
                    ]),
            );
        } else if text.starts_with([' ', '\t']) || text.ends_with([' ', '\t']) {
            // Only spaces and tabs count here; a trailing newline on a
            // multiline TOML string is idiomatic and remains valid.
            emit(
                diagnostics,
                allow,
                LintCode::Whitespace,
                Diagnostic::warning_or_error(strict)
                    .with_message(format!(
                        "`{}` translation has surrounding whitespace",
                        language.code()
                    ))
                    .with_labels(vec![
                        Label::primary(file_id, value.span.clone())
                            .with_message("leading or trailing space"),
                    ]),
            );
        }
    }

    // Validate template syntax and arguments after basic content checks.
    if let Some(analyzer) = analyzer {
        lint_templates(key, translation, analyzer, strict, diagnostics);
    }
}

/// One language's template together with what the analysis found in it.
struct Analyzed<'a> {
    language: Language,
    value: &'a Spanned<String>,
    analysis: template::Analysis,
}

/// Analyzes every language of a key, reporting the templates that do not
/// compile and returning the rest for the cross-language checks.
fn analyze_templates<'a>(
    translation: &'a Translation,
    analyzer: template::Analyzer,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) -> Vec<Analyzed<'a>> {
    let mut per_language = Vec::new();
    for (language, value) in &translation.language {
        match analyzer.analyze(value.as_ref()) {
            Ok(analysis) => per_language.push(Analyzed {
                language: *language,
                value,
                analysis,
            }),
            Err(error) => emit(
                diagnostics,
                &translation.allow,
                LintCode::Template,
                Diagnostic::error()
                    .with_message(format!("`{}` template fails to compile", language.code()))
                    .with_labels(vec![
                        Label::primary(translation.file_id, value.span.clone())
                            .with_message(error.to_string()),
                    ]),
            ),
        }
    }
    per_language
}

fn lint_templates(
    key: &Spanned<String>,
    translation: &Translation,
    analyzer: template::Analyzer,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) {
    let file_id = translation.file_id;
    let allow = &translation.allow;
    let per_language = analyze_templates(translation, analyzer, diagnostics);

    let used: BTreeSet<String> = per_language
        .iter()
        .flat_map(|analyzed| analyzed.analysis.variables())
        .collect();
    let substituted: BTreeSet<&str> = per_language
        .iter()
        .flat_map(|analyzed| analyzed.analysis.substituted.iter().map(String::as_str))
        .collect();

    // A value substituted in one language must be substituted in every
    // language.
    // Names that only select a wording are exempt: a language without the
    // distinction simply does not branch on them.
    for Analyzed {
        language,
        value,
        analysis,
    } in &per_language
    {
        for missing in substituted
            .iter()
            .filter(|name| !analysis.substituted.contains(**name))
        {
            emit(
                diagnostics,
                allow,
                LintCode::Placeholder,
                Diagnostic::warning_or_error(strict)
                    .with_message(format!(
                        "placeholder `{}` is missing from the `{}` translation",
                        braces(missing),
                        language.code()
                    ))
                    .with_labels(vec![
                        Label::primary(file_id, value.span.clone()).with_message(format!(
                            "`{}` is used in other languages but not here",
                            braces(missing)
                        )),
                    ]),
            );
        }
    }

    lint_dead_conditions(&per_language, file_id, allow, strict, diagnostics);

    let declared: BTreeSet<&str> = translation.arguments.keys().map(String::as_str).collect();

    // Report placeholders that have no matching argument declaration.
    for Analyzed {
        language,
        value,
        analysis,
    } in &per_language
    {
        for undeclared in analysis
            .variables()
            .iter()
            .filter(|name| !declared.contains(name.as_str()))
        {
            emit(
                diagnostics,
                allow,
                LintCode::UndeclaredArgument,
                Diagnostic::warning_or_error(strict)
                    .with_message(format!(
                        "template uses `{}` which is not declared in `arguments`",
                        braces(undeclared)
                    ))
                    .with_labels(vec![
                        Label::primary(file_id, value.span.clone()).with_message(format!(
                            "`{}` is undeclared in the `{}` translation",
                            undeclared,
                            language.code()
                        )),
                    ]),
            );
        }
    }

    // Report argument declarations that no language uses.
    for unused in declared.iter().filter(|name| !used.contains(**name)) {
        emit(
            diagnostics,
            allow,
            LintCode::UnusedArgument,
            Diagnostic::warning_or_error(strict)
                .with_message(format!("argument `{unused}` is declared but never used"))
                .with_labels(vec![
                    Label::primary(file_id, key.span.clone())
                        .with_message(format!("`{unused}` is not referenced by any template")),
                ]),
        );
    }
}

/// Reports conditions with identical branches.
/// They change nothing and are usually a leftover from working around the
/// placeholder check.
fn lint_dead_conditions(
    per_language: &[Analyzed<'_>],
    file_id: FileId,
    allow: &BTreeSet<AllowEntry>,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) {
    for Analyzed {
        language,
        value,
        analysis,
    } in per_language
    {
        for dead in &analysis.dead_conditions {
            emit(
                diagnostics,
                allow,
                LintCode::DeadCondition,
                Diagnostic::warning_or_error(strict)
                    .with_message(format!(
                        "`{dead}` has no effect on the `{}` translation",
                        language.code()
                    ))
                    .with_labels(vec![
                        Label::primary(file_id, value.span.clone())
                            .with_message("both branches render the same text"),
                    ])
                    .with_notes(vec![
                        "a language need not vary on every condition; remove the block or give \
                         the branches different text"
                            .to_string(),
                    ]),
            );
        }
    }
}

/// Lower-cased, whitespace-collapsed form used for duplicate comparison.
fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

struct DupEntry<'a> {
    index: usize,
    value: &'a Spanned<String>,
    file_id: FileId,
}

/// Reports different keys that share an identical translation after normalizing
/// case and whitespace) in some language.
fn lint_duplicates(
    translations: &Translations,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) {
    let languages: BTreeSet<Language> = translations
        .0
        .values()
        .flat_map(|translation| translation.language.keys().copied())
        .collect();

    // Report a matching set of keys only once when several languages coincide.
    let mut reported: HashSet<Vec<usize>> = HashSet::new();

    for language in languages {
        let mut groups: BTreeMap<String, Vec<DupEntry<'_>>> = BTreeMap::new();
        for (index, translation) in translations.0.values().enumerate() {
            // Exclude allowed keys without suppressing duplicates among the
            // remaining keys.
            if is_allowed(&translation.allow, LintCode::Duplicate) {
                continue;
            }
            if let Some(value) = translation.language.get(&language) {
                groups
                    .entry(normalize(value.as_ref()))
                    .or_default()
                    .push(DupEntry {
                        index,
                        value,
                        file_id: translation.file_id,
                    });
            }
        }

        for entries in groups.into_values() {
            if entries.len() < 2 {
                continue;
            }
            if !reported.insert(entries.iter().map(|entry| entry.index).collect()) {
                continue;
            }

            let labels = entries
                .iter()
                .enumerate()
                .map(|(position, entry)| {
                    if position == 0 {
                        Label::primary(entry.file_id, entry.value.span.clone())
                            .with_message("this translation")
                    } else {
                        Label::secondary(entry.file_id, entry.value.span.clone())
                            .with_message("is duplicated here")
                    }
                })
                .collect();

            diagnostics.push(
                Diagnostic::warning_or_error(strict)
                    .with_code(LintCode::Duplicate)
                    .with_message(format!(
                        "{} keys share an identical `{}` translation",
                        entries.len(),
                        language.code()
                    ))
                    .with_labels(labels),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LintOptions;
    use crate::{Language, TemplateEngine, Translations, diagnostics::Spanned};
    use codespan_reporting::diagnostic::Severity;
    use color_eyre::eyre;
    use similar_asserts::assert_eq as sim_assert_eq;

    /// Lints `raw` with the given engine, returning each diagnostic's
    /// severity, code, and message.
    fn lint_with_engine(
        raw: &str,
        required: &[Language],
        engine: Option<TemplateEngine>,
        detect_duplicates: bool,
    ) -> eyre::Result<Vec<(Severity, Option<String>, String)>> {
        let mut parse_diagnostics = vec![];
        let translations = Translations::from_str(raw, 0, false, &mut parse_diagnostics)?;
        let required: Vec<Spanned<Language>> =
            required.iter().copied().map(Spanned::dummy).collect();
        let engine = engine.map(Spanned::dummy);
        let options = LintOptions {
            required_languages: &required,
            template_engine: engine.as_ref(),
            strict: false,
            detect_duplicates,
        };
        let mut diagnostics = vec![];
        translations.lint(&mut diagnostics, &options);
        Ok(diagnostics
            .into_iter()
            .map(|d| (d.severity, d.code, d.message))
            .collect())
    }

    /// Lints `raw` as Handlebars, the engine the template checks are written
    /// against.
    fn lint(
        raw: &str,
        required: &[Language],
        detect_duplicates: bool,
    ) -> eyre::Result<Vec<(Option<String>, String)>> {
        Ok(lint_with_engine(
            raw,
            required,
            Some(TemplateEngine::Handlebars),
            detect_duplicates,
        )?
        .into_iter()
        .map(|(_, code, message)| (code, message))
        .collect())
    }

    fn messages(raw: &str, required: &[Language]) -> eyre::Result<Vec<String>> {
        Ok(lint(raw, required, false)?
            .into_iter()
            .map(|(_, m)| m)
            .collect())
    }

    #[test_util::test]
    fn flags_missing_required_language() {
        let raw = indoc::indoc! {r#"
            [greeting]
            en = "Hello"
        "#};
        let msgs = messages(raw, &[Language::En, Language::De])?;
        assert!(
            msgs.iter().any(|m| m == "missing `de` translation"),
            "{msgs:?}"
        );
    }

    #[test_util::test]
    fn flags_empty_and_whitespace_values() {
        let raw = indoc::indoc! {r#"
            [a]
            en = ""
            de = " Hallo "
        "#};
        let msgs = messages(raw, &[])?;
        assert!(
            msgs.iter().any(|m| m == "empty `en` translation"),
            "{msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m == "`de` translation has surrounding whitespace"),
            "{msgs:?}"
        );
    }

    #[test_util::test]
    fn flags_template_argument_problems() {
        let raw = indoc::indoc! {r#"
            [greeting]
            en = "Hello {{name}}"
            de = "Hallo"
            arguments = { title = "string" }
        "#};
        let msgs = messages(raw, &[])?;
        assert!(
            msgs.iter()
                .any(|m| m == "placeholder `{{name}}` is missing from the `de` translation"),
            "{msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m == "template uses `{{name}}` which is not declared in `arguments`"),
            "{msgs:?}"
        );
        assert!(
            msgs.iter()
                .any(|m| m == "argument `title` is declared but never used"),
            "{msgs:?}"
        );
    }

    /// A name that only selects a wording may be absent from a language
    /// without the distinction; the empty-block workaround is not needed.
    #[test_util::test]
    fn condition_only_names_are_not_required_everywhere() {
        let raw = indoc::indoc! {r#"
            [greeting]
            en = "Hello {{name}}"
            de = "{{#if my_condition}}Hallo {{name}}{{else}}Hi {{name}}{{/if}}"
            arguments = { name = "string", my_condition = "boolean" }
        "#};
        let msgs = messages(raw, &[Language::En, Language::De])?;
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    /// The empty `{{#if}}` block that used to satisfy the placeholder check is
    /// reported, naming the language it does nothing for.
    #[test_util::test]
    fn flags_dead_conditions() {
        let raw = indoc::indoc! {r#"
            [greeting]
            en = "{{#if my_condition}}{{/if}}Hello {{name}}"
            de = "{{#if my_condition}}Hallo {{name}}{{else}}Hi {{name}}{{/if}}"
            arguments = { name = "string", my_condition = "boolean" }
        "#};
        let found = lint(raw, &[], false)?;
        let dead: Vec<_> = found
            .iter()
            .filter(|(code, _)| code.as_deref() == Some("dead-condition"))
            .map(|(_, message)| message.as_str())
            .collect();
        assert_eq!(
            dead,
            vec!["`{{#if my_condition}}` has no effect on the `en` translation"]
        );
    }

    #[test_util::test]
    fn allow_suppresses_dead_condition() {
        let raw = indoc::indoc! {r#"
            [greeting]
            en = "{{#if my_condition}}{{/if}}Hello"
            allow = ["lint:dead-condition"]
        "#};
        let found = lint(raw, &[], false)?;
        assert!(
            found
                .iter()
                .all(|(code, _)| code.as_deref() != Some("dead-condition")),
            "{found:?}"
        );
    }

    #[test_util::test]
    fn flags_undeclared_arguments_without_arguments_table() {
        let raw = indoc::indoc! {r#"
            [greeting]
            en = "Hello {{name}}"
        "#};
        let msgs = messages(raw, &[])?;
        assert!(
            msgs.iter()
                .any(|m| m == "template uses `{{name}}` which is not declared in `arguments`"),
            "{msgs:?}"
        );
    }

    #[test_util::test]
    fn flags_template_compile_error() {
        let raw = indoc::indoc! {r#"
            [a]
            en = "{{#each}}"
        "#};
        let msgs = messages(raw, &[])?;
        assert!(
            msgs.iter().any(|m| m == "`en` template fails to compile"),
            "{msgs:?}"
        );
    }

    /// Without a configured engine the templates are left alone, since a
    /// catalog written for another engine would fail under a guessed parser;
    /// one note points at the setting instead.
    #[test_util::test]
    fn no_engine_skips_template_checks_with_a_note() {
        let raw = indoc::indoc! {r#"
            [greeting]
            en = "Hello {{ name|upper }}"
            de = "Hallo {% if formal %}there{% endif %}"
            arguments = { name = "string" }
        "#};
        let found = lint_with_engine(raw, &[], None, false)?;
        sim_assert_eq!(
            have: found,
            want: vec![(
                Severity::Note,
                None,
                "no template engine is configured, so templates were not checked".to_string(),
            )]
        );

        // An engine without analysis support is reported the same way.
        let found = lint_with_engine(raw, &[], Some(TemplateEngine::Jinja2), false)?;
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, Severity::Note);
        assert!(found[0].2.contains("`jinja2`"), "{}", found[0].2);

        // A catalog without templates has nothing to announce.
        let plain = indoc::indoc! {r#"
            [greeting]
            en = "Hello"
        "#};
        let found = lint_with_engine(plain, &[], None, false)?;
        assert!(found.is_empty(), "{found:?}");
    }

    #[test_util::test]
    fn clean_translations_produce_no_diagnostics() {
        let raw = indoc::indoc! {r#"
            [greeting]
            de = "Hallo {{name}}"
            en = "Hello {{name}}"
            arguments = { name = "string" }
        "#};
        let msgs = messages(raw, &[Language::De, Language::En])?;
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    #[test_util::test]
    fn allow_key_suppresses_a_lint() {
        // `b` is missing `de` but allows the missing-language lint.
        let raw = indoc::indoc! {r#"
            [a]
            en = "Hello"
            de = "Hallo"

            [b]
            en = "Bye"
            allow = ["lint:missing-language"]
        "#};
        let msgs = messages(raw, &[Language::En, Language::De])?;
        assert!(msgs.iter().all(|m| !m.contains("missing `de`")), "{msgs:?}");
    }

    /// An `allow` on a group table reaches the keys nested under it instead of
    /// being silently dropped.
    #[test_util::test]
    fn group_allow_suppresses_nested_keys() {
        let raw = indoc::indoc! {r#"
            [checkout]
            allow = ["lint:missing-language"]

            [checkout.button]
            en = "Continue"

            [greeting]
            en = "Hello"
        "#};
        let msgs = messages(raw, &[Language::En, Language::De])?;
        // Only `greeting`, outside the group, is still reported.
        sim_assert_eq!(have: msgs, want: vec!["missing `de` translation".to_string()]);
    }

    /// Entries added by an outer scope, such as a config file, suppress lints
    /// for every key.
    #[test_util::test]
    fn extend_allow_suppresses_every_key() {
        use crate::lint::{AllowEntry, LintCode};

        let raw = indoc::indoc! {r#"
            [a]
            en = "Hello"

            [b]
            en = "Bye"
        "#};
        let mut parse_diagnostics = vec![];
        let mut translations = Translations::from_str(raw, 0, false, &mut parse_diagnostics)?;
        translations.extend_allow(&[AllowEntry::Code(LintCode::MissingLanguage)].into());

        let required = [Spanned::dummy(Language::En), Spanned::dummy(Language::De)];
        let options = LintOptions {
            required_languages: &required,
            template_engine: None,
            strict: false,
            detect_duplicates: false,
        };
        let mut diagnostics = vec![];
        translations.lint(&mut diagnostics, &options);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test_util::test]
    fn detects_identical_translations_ignoring_case() {
        let raw = indoc::indoc! {r#"
            [one]
            en = "please upload your documents now"

            [two]
            en = "Please upload your documents now"
        "#};
        let found = lint(raw, &[], true)?;
        assert!(
            found
                .iter()
                .any(|(code, _)| code.as_deref() == Some("duplicate")),
            "{found:?}"
        );
    }

    /// Similar but unequal strings do not count as duplicates.
    #[test_util::test]
    fn near_duplicates_are_not_reported() {
        // A one-word variation (`connect`/`connected`) must not satisfy exact
        // duplicate matching.
        let raw = indoc::indoc! {r#"
            [connect]
            en = "connect to the Europace service"

            [connected]
            en = "connected to the Europace service"
        "#};
        let found = lint(raw, &[], true)?;
        assert!(
            found
                .iter()
                .all(|(code, _)| code.as_deref() != Some("duplicate")),
            "{found:?}"
        );
    }

    /// Exact duplicate detection also applies to single-word translations.
    #[test_util::test]
    fn detects_single_word_duplicates() {
        // One shared word still forms an exact duplicate.
        let raw = indoc::indoc! {r#"
            [save]
            en = "Save"

            [store]
            en = "Save"
        "#};
        let found = lint(raw, &[], true)?;
        assert!(
            found
                .iter()
                .any(|(code, _)| code.as_deref() == Some("duplicate")),
            "{found:?}"
        );
    }

    /// A likely untranslated copy is reported without flagging distinct values.
    #[test_util::test]
    fn flags_identical_languages_within_a_key() {
        // English copied into German is likely untranslated.
        let raw = indoc::indoc! {r#"
            [greeting]
            en = "Hello"
            de = "Hello"
            fr = "Bonjour"
        "#};
        let found = lint(raw, &[], true)?;
        assert!(
            found
                .iter()
                .any(|(code, _)| code.as_deref() == Some("identical-languages")),
            "{found:?}"
        );

        // Distinct translations are not flagged.
        let ok = indoc::indoc! {r#"
            [hi]
            en = "Hello"
            de = "Hallo"
            fr = "Bonjour"
        "#};
        let found = lint(ok, &[], true)?;
        assert!(
            found
                .iter()
                .all(|(code, _)| code.as_deref() != Some("identical-languages")),
            "{found:?}"
        );
    }

    #[test_util::test]
    fn allow_suppresses_duplicate() {
        let raw = indoc::indoc! {r#"
            [one]
            en = "please upload your documents now"

            [two]
            en = "please upload your documents now"
            allow = ["lint:duplicate"]
        "#};
        let found = lint(raw, &[], true)?;
        assert!(
            found
                .iter()
                .all(|(code, _)| code.as_deref() != Some("duplicate")),
            "{found:?}"
        );
    }
}
