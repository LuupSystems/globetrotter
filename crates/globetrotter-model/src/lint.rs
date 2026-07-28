//! Linting of translation files.
//!
//! These checks go beyond what code generation strictly requires: they look for
//! missing or empty translations, stray whitespace, broken templates,
//! inconsistent or undeclared template arguments, and duplicated strings.
//!
//! Every diagnostic carries a stable [`LintCode`]; a translation key can
//! suppress a code by listing its kebab-case name in an `allow` key, e.g.
//! `allow = ["duplicate"]` (or `allow = "all"` to silence the key entirely).

use crate::{
    Language, TemplateEngine, Translation, Translations,
    diagnostics::{DiagnosticExt, FileId, Spanned},
};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use handlebars::template::{BlockParam, HelperTemplate, Parameter, Template, TemplateElement};
use handlebars::{Path, PathSeg};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// A stable identifier for each lint, shown in diagnostics (`warning[code]: …`)
/// and usable in a translation key's `allow` list to suppress it. The
/// kebab-case string form (`missing-language`, `unused-key`, …) is derived via
/// [`strum`].
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
    /// The template engine used to compile template translations, if any.
    pub template_engine: Option<&'a Spanned<TemplateEngine>>,
    /// Whether issues are reported as errors rather than warnings.
    pub strict: bool,
    /// Whether to report keys that share an identical translation, and keys
    /// whose languages are identical to each other.
    pub detect_duplicates: bool,
}

/// An entry in a translation key's `allow` list: a specific [`LintCode`] to
/// suppress, or the catch-all `all` that suppresses every lint for the key.
///
/// `all` is intentionally *not* a [`LintCode`] variant — no diagnostic is ever
/// emitted with code `all`; it is only meaningful as an allow directive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AllowEntry {
    /// Suppress every lint for this key.
    All,
    /// Suppress one specific lint.
    Code(LintCode),
}

impl std::fmt::Display for AllowEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => f.write_str("all"),
            Self::Code(code) => std::fmt::Display::fmt(code, f),
        }
    }
}

impl std::str::FromStr for AllowEntry {
    type Err = strum::ParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == "all" {
            Ok(Self::All)
        } else {
            text.parse::<LintCode>().map(Self::Code)
        }
    }
}

/// `true` if `code` (or the catch-all [`AllowEntry::All`]) is in the allow list.
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

/// Extract the top-level variable names referenced by a Handlebars template.
///
/// Returns `None` if the template does not compile. Helper names, block-local
/// parameters (`{{#each xs as |x|}}`), `this`, and `@`-variables are excluded,
/// so only the names that should be declared as arguments are returned.
#[must_use]
pub fn handlebars_variables(source: &str) -> Option<BTreeSet<String>> {
    let template = Template::compile(source).ok()?;
    let mut variables = BTreeSet::new();
    let mut locals = Vec::new();
    collect_elements(&template.elements, &mut variables, &mut locals);
    Some(variables)
}

fn collect_elements(
    elements: &[TemplateElement],
    variables: &mut BTreeSet<String>,
    locals: &mut Vec<String>,
) {
    for element in elements {
        match element {
            TemplateElement::Expression(helper)
            | TemplateElement::HtmlExpression(helper)
            | TemplateElement::HelperBlock(helper) => collect_helper(helper, variables, locals),
            _ => {}
        }
    }
}

fn collect_helper(
    helper: &HelperTemplate,
    variables: &mut BTreeSet<String>,
    locals: &mut Vec<String>,
) {
    collect_parameter(&helper.name, variables, locals);
    for parameter in &helper.params {
        collect_parameter(parameter, variables, locals);
    }
    for parameter in helper.hash.values() {
        collect_parameter(parameter, variables, locals);
    }

    // block parameters (`as |x|`) shadow outer names inside the block body.
    let depth = locals.len();
    if let Some(block_param) = &helper.block_param {
        match block_param {
            BlockParam::Single(Parameter::Name(name)) => locals.push(name.clone()),
            BlockParam::Pair((first, second)) => {
                for parameter in [first, second] {
                    if let Parameter::Name(name) = parameter {
                        locals.push(name.clone());
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(template) = &helper.template {
        collect_elements(&template.elements, variables, locals);
    }
    if let Some(template) = &helper.inverse {
        collect_elements(&template.elements, variables, locals);
    }
    locals.truncate(depth);
}

fn collect_parameter(
    parameter: &Parameter,
    variables: &mut BTreeSet<String>,
    locals: &mut Vec<String>,
) {
    match parameter {
        Parameter::Path(Path::Relative((segments, _))) => {
            if let Some(PathSeg::Named(first)) = segments.first()
                && first != "this"
                && !locals.iter().any(|local| local == first)
            {
                variables.insert(first.clone());
            }
        }
        Parameter::Subexpression(subexpression) => {
            collect_elements(
                std::slice::from_ref(subexpression.element.as_ref()),
                variables,
                locals,
            );
        }
        _ => {}
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

impl Translations {
    /// Lint the translations, pushing any issues onto `diagnostics`.
    ///
    /// Checks for missing and empty translations, surrounding whitespace,
    /// templates that fail to compile, placeholders that are inconsistent across
    /// languages, template arguments that are used but not declared (or declared
    /// but never used), and — when [`LintOptions::detect_duplicates`] is enabled
    /// — keys that share an identical translation. Issues are reported as
    /// warnings, or as errors when [`LintOptions::strict`] is set.
    pub fn lint(&self, diagnostics: &mut Vec<Diagnostic<FileId>>, options: &LintOptions<'_>) {
        let required: BTreeSet<Language> = options
            .required_languages
            .iter()
            .map(|lang| *lang.as_ref())
            .collect();
        // when the config declares no languages, expect every key to cover the
        // set of languages that appear anywhere in the translations.
        let expected: BTreeSet<Language> = if required.is_empty() {
            self.0
                .values()
                .flat_map(|translation| translation.language.keys().copied())
                .collect()
        } else {
            required
        };

        let handlebars = matches!(
            options.template_engine.map(Spanned::as_ref),
            None | Some(TemplateEngine::Handlebars)
        );

        for (key, translation) in &self.0 {
            lint_translation(
                key,
                translation,
                &expected,
                handlebars,
                options.strict,
                diagnostics,
            );
        }

        if options.detect_duplicates {
            for translation in self.0.values() {
                lint_identical_languages(translation, options.strict, diagnostics);
            }
            lint_duplicates(self, options.strict, diagnostics);
        }
    }
}

/// Within a single key, report languages that share an identical translation
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
    handlebars: bool,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) {
    let file_id = translation.file_id;
    let allow = &translation.allow;

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
            // only spaces and tabs; a trailing newline on a multi-line string is
            // idiomatic TOML and not flagged.
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

    if handlebars {
        lint_templates(key, translation, strict, diagnostics);
    }
}

fn lint_templates(
    key: &Spanned<String>,
    translation: &Translation,
    strict: bool,
    diagnostics: &mut Vec<Diagnostic<FileId>>,
) {
    let file_id = translation.file_id;
    let allow = &translation.allow;

    let mut per_language: Vec<(Language, &Spanned<String>, BTreeSet<String>)> = Vec::new();
    for (language, value) in &translation.language {
        match handlebars_variables(value.as_ref()) {
            Some(variables) => per_language.push((*language, value, variables)),
            None => emit(
                diagnostics,
                allow,
                LintCode::Template,
                Diagnostic::error()
                    .with_message(format!("`{}` template fails to compile", language.code()))
                    .with_labels(vec![
                        Label::primary(file_id, value.span.clone())
                            .with_message("invalid handlebars template"),
                    ]),
            ),
        }
    }

    let used: BTreeSet<&str> = per_language
        .iter()
        .flat_map(|(_, _, variables)| variables.iter().map(String::as_str))
        .collect();

    // a placeholder used in one language should be present in every language.
    for (language, value, variables) in &per_language {
        for missing in used.iter().filter(|name| !variables.contains(**name)) {
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

    let declared: BTreeSet<&str> = translation.arguments.keys().map(String::as_str).collect();

    for (language, value, variables) in &per_language {
        for undeclared in variables
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

/// Report different keys that share an identical translation (after normalizing
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

    // de-duplicate reports for key sets that coincide in more than one language.
    let mut reported: HashSet<Vec<usize>> = HashSet::new();

    for language in languages {
        let mut groups: BTreeMap<String, Vec<DupEntry<'_>>> = BTreeMap::new();
        for (index, translation) in translations.0.values().enumerate() {
            // a key that allows the lint is excluded so the others can still be
            // reported among themselves.
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
    use super::{LintOptions, handlebars_variables};
    use crate::{Language, Translations, diagnostics::Spanned};
    use similar_asserts::assert_eq as sim_assert_eq;
    use std::collections::BTreeSet;

    fn vars(source: &str) -> Vec<String> {
        handlebars_variables(source)
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    #[test]
    fn extracts_simple_and_helper_variables() {
        sim_assert_eq!(have: vars("{{name}}"), want: vec!["name".to_string()]);
        sim_assert_eq!(have: vars("{{uppercase name}}"), want: vec!["name".to_string()]);
        sim_assert_eq!(
            have: vars("Hello {{name}}, you are {{age}} years old."),
            want: vec!["age".to_string(), "name".to_string()]
        );
    }

    #[test]
    fn excludes_block_locals_and_this() {
        sim_assert_eq!(
            have: vars("{{#each items}}{{this}}{{/each}}"),
            want: vec!["items".to_string()]
        );
        sim_assert_eq!(
            have: vars("{{#each rows as |row|}}{{row}}{{/each}}"),
            want: vec!["rows".to_string()]
        );
    }

    #[test]
    fn invalid_template_returns_none() {
        assert!(handlebars_variables("{{#each}}").is_none());
        assert!(handlebars_variables("{{unclosed").is_none());
    }

    #[test]
    fn plain_text_has_no_variables() {
        sim_assert_eq!(have: handlebars_variables("just text").unwrap(), want: BTreeSet::new());
    }

    fn lint(
        raw: &str,
        required: &[Language],
        detect_duplicates: bool,
    ) -> Vec<(Option<String>, String)> {
        let mut parse_diagnostics = vec![];
        let translations = Translations::from_str(raw, 0, false, &mut parse_diagnostics).unwrap();
        let required: Vec<Spanned<Language>> =
            required.iter().copied().map(Spanned::dummy).collect();
        let options = LintOptions {
            required_languages: &required,
            template_engine: None,
            strict: false,
            detect_duplicates,
        };
        let mut diagnostics = vec![];
        translations.lint(&mut diagnostics, &options);
        diagnostics
            .into_iter()
            .map(|d| (d.code, d.message))
            .collect()
    }

    fn messages(raw: &str, required: &[Language]) -> Vec<String> {
        lint(raw, required, false)
            .into_iter()
            .map(|(_, m)| m)
            .collect()
    }

    #[test]
    fn flags_missing_required_language() {
        let raw = "\n[greeting]\nen = \"Hello\"\n";
        let msgs = messages(raw, &[Language::En, Language::De]);
        assert!(
            msgs.iter().any(|m| m == "missing `de` translation"),
            "{msgs:?}"
        );
    }

    #[test]
    fn flags_empty_and_whitespace_values() {
        let raw = "\n[a]\nen = \"\"\nde = \" Hallo \"\n";
        let msgs = messages(raw, &[]);
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

    #[test]
    fn flags_template_argument_problems() {
        let raw = "\n[greeting]\nen = \"Hello {{name}}\"\nde = \"Hallo\"\narguments = { title = \"string\" }\n";
        let msgs = messages(raw, &[]);
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

    #[test]
    fn flags_undeclared_arguments_without_arguments_table() {
        let raw = "\n[greeting]\nen = \"Hello {{name}}\"\n";
        let msgs = messages(raw, &[]);
        assert!(
            msgs.iter()
                .any(|m| m == "template uses `{{name}}` which is not declared in `arguments`"),
            "{msgs:?}"
        );
    }

    #[test]
    fn flags_template_compile_error() {
        let raw = "\n[a]\nen = \"{{#each}}\"\n";
        let msgs = messages(raw, &[]);
        assert!(
            msgs.iter().any(|m| m == "`en` template fails to compile"),
            "{msgs:?}"
        );
    }

    #[test]
    fn clean_translations_produce_no_diagnostics() {
        let raw = "\n[greeting]\nde = \"Hallo {{name}}\"\nen = \"Hello {{name}}\"\narguments = { name = \"string\" }\n";
        let msgs = messages(raw, &[Language::De, Language::En]);
        assert!(msgs.is_empty(), "{msgs:?}");
    }

    #[test]
    fn allow_key_suppresses_a_lint() {
        // `b` is missing `de` but allows the missing-language lint.
        let raw = concat!(
            "\n[a]\nen = \"Hello\"\nde = \"Hallo\"\n",
            "\n[b]\nen = \"Bye\"\nallow = [\"missing-language\"]\n"
        );
        let msgs = messages(raw, &[Language::En, Language::De]);
        assert!(msgs.iter().all(|m| !m.contains("missing `de`")), "{msgs:?}");
    }

    #[test]
    fn detects_identical_translations_ignoring_case() {
        let raw = concat!(
            "\n[one]\nen = \"please upload your documents now\"\n",
            "\n[two]\nen = \"Please upload your documents now\"\n"
        );
        let found = lint(raw, &[], true);
        assert!(
            found
                .iter()
                .any(|(code, _)| code.as_deref() == Some("duplicate")),
            "{found:?}"
        );
    }

    #[test]
    fn near_duplicates_are_not_reported() {
        // textually one character apart but semantically distinct (connect vs
        // connected): must NOT be flagged now that detection is exact-only.
        let raw = concat!(
            "\n[connect]\nen = \"connect to the Europace service\"\n",
            "\n[connected]\nen = \"connected to the Europace service\"\n"
        );
        let found = lint(raw, &[], true);
        assert!(
            found
                .iter()
                .all(|(code, _)| code.as_deref() != Some("duplicate")),
            "{found:?}"
        );
    }

    #[test]
    fn detects_single_word_duplicates() {
        // even a single shared word across keys is an exact duplicate.
        let raw = concat!("\n[save]\nen = \"Save\"\n", "\n[store]\nen = \"Save\"\n");
        let found = lint(raw, &[], true);
        assert!(
            found
                .iter()
                .any(|(code, _)| code.as_deref() == Some("duplicate")),
            "{found:?}"
        );
    }

    #[test]
    fn flags_identical_languages_within_a_key() {
        // en copied to de/fr — a stale/untranslated key.
        let raw = "\n[greeting]\nen = \"Hello\"\nde = \"Hello\"\nfr = \"Bonjour\"\n";
        let found = lint(raw, &[], true);
        assert!(
            found
                .iter()
                .any(|(code, _)| code.as_deref() == Some("identical-languages")),
            "{found:?}"
        );
        // distinct translations are not flagged.
        let ok = "\n[hi]\nen = \"Hello\"\nde = \"Hallo\"\nfr = \"Bonjour\"\n";
        let found = lint(ok, &[], true);
        assert!(
            found
                .iter()
                .all(|(code, _)| code.as_deref() != Some("identical-languages")),
            "{found:?}"
        );
    }

    #[test]
    fn allow_suppresses_duplicate() {
        let raw = concat!(
            "\n[one]\nen = \"please upload your documents now\"\n",
            "\n[two]\nen = \"please upload your documents now\"\nallow = [\"duplicate\"]\n"
        );
        let found = lint(raw, &[], true);
        assert!(
            found
                .iter()
                .all(|(code, _)| code.as_deref() != Some("duplicate")),
            "{found:?}"
        );
    }
}
