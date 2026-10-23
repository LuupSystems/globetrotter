//! Checks static translation calls against every catalog that scans their source.
//!
//! Several configs may scan the same file with overlapping `functions`, and a call may belong to
//! any of their catalogs.
//! A call is only reported when no config that recognized it accepts it: an undefined key needs
//! every such config to lack the key, and an argument mismatch needs every config that defines
//! the key to reject the call's arguments.
//! Each finding is attributed to the configs whose policy reports it.

use super::DefinedKey;
use super::source::{ArgumentValue, CallArgument, CallReference, DynamicReference};
use crate::config::usages::CallUsages;
use globetrotter_model::{
    ArgumentType, Arguments,
    diagnostics::{FileId, Span},
    lint::{AllowEntry, LintCode, is_allowed},
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

/// The static calls and prefixed dynamic calls one config recognized in one source file.
pub(crate) struct CallSource {
    pub path: PathBuf,
    pub content: String,
    pub calls: Vec<CallReference>,
    /// Dynamic calls with a known key prefix.
    pub prefixes: Vec<DynamicReference>,
}

/// A config that scanned sources, and how it reports call findings.
pub(crate) struct CallOwner {
    /// The config's display name in diagnostics.
    pub name: String,
    pub policy: CallUsages,
    /// The config-wide `allow` entries.
    pub allow: BTreeSet<AllowEntry>,
    /// Arguments this config's calls may omit because the host supplies them.
    pub implied: BTreeSet<String>,
}

impl CallOwner {
    fn reports(&self, code: LintCode) -> bool {
        self.policy != CallUsages::Allow && !is_allowed(&self.allow, code)
    }
}

/// A finding about one call, attributed to the configs that report it.
#[derive(Debug, PartialEq)]
pub(crate) struct CallFinding {
    pub code: LintCode,
    /// Whether any reporting config denies call findings.
    pub deny: bool,
    pub message: String,
    pub span: Span,
    pub label: String,
    /// Where the catalog declares the key, for argument findings.
    pub definition: Option<(FileId, Span)>,
    pub owners: BTreeSet<String>,
}

/// The findings within one source file.
pub(crate) struct FileFindings {
    pub path: PathBuf,
    pub content: String,
    pub findings: Vec<CallFinding>,
}

struct Definition {
    arguments: Arguments,
    file_id: FileId,
    span: Span,
    allow: BTreeSet<AllowEntry>,
}

struct Call {
    reference: CallReference,
    seen: Vec<(Arc<CallOwner>, Option<Definition>)>,
}

struct DynamicCall {
    prefix: String,
    span: Span,
    /// Each recognizing config, and whether its catalog has a key with the prefix.
    seen: Vec<(Arc<CallOwner>, bool)>,
}

struct File {
    content: String,
    calls: BTreeMap<(usize, usize), Call>,
    prefixes: BTreeMap<(usize, usize), DynamicCall>,
}

/// Collects the calls every config recognized, keyed by source location.
#[derive(Default)]
pub(crate) struct CallChecks {
    files: BTreeMap<PathBuf, File>,
}

impl CallChecks {
    /// Records the calls `owner` recognized in its sources, resolved against its catalog.
    pub(crate) fn add(
        &mut self,
        owner: &Arc<CallOwner>,
        keys: &[DefinedKey],
        sources: Vec<CallSource>,
    ) {
        let catalog: BTreeMap<&str, &DefinedKey> =
            keys.iter().map(|key| (key.key.as_str(), key)).collect();
        for source in sources {
            let file = self.files.entry(source.path).or_insert_with(|| File {
                content: source.content,
                calls: BTreeMap::new(),
                prefixes: BTreeMap::new(),
            });
            for reference in source.calls {
                let definition = catalog.get(reference.key.as_str()).map(|key| Definition {
                    arguments: key.arguments.clone(),
                    file_id: key.file_id,
                    span: key.span.clone(),
                    allow: key.allow.clone(),
                });
                file.calls
                    .entry((reference.span.start, reference.span.end))
                    .or_insert_with(|| Call {
                        reference,
                        seen: Vec::new(),
                    })
                    .seen
                    .push((Arc::clone(owner), definition));
            }
            for reference in source.prefixes {
                let Some(prefix) = reference.prefix else {
                    continue;
                };
                let matched = catalog.keys().any(|key| key.starts_with(&prefix));
                file.prefixes
                    .entry((reference.span.start, reference.span.end))
                    .or_insert_with(|| DynamicCall {
                        prefix,
                        span: reference.span,
                        seen: Vec::new(),
                    })
                    .seen
                    .push((Arc::clone(owner), matched));
            }
        }
    }

    /// Returns the findings of every file that has any.
    pub(crate) fn finish(self) -> Vec<FileFindings> {
        self.files
            .into_iter()
            .filter_map(|(path, file)| {
                let mut findings: Vec<CallFinding> =
                    file.calls.values().flat_map(call_findings).collect();
                findings.extend(file.prefixes.into_values().filter_map(prefix_finding));
                (!findings.is_empty()).then_some(FileFindings {
                    path,
                    content: file.content,
                    findings,
                })
            })
            .collect()
    }
}

/// Builds a finding reported by every config in `reporters`, or none if no config reports it.
fn attributed<'a>(
    reporters: impl IntoIterator<Item = &'a CallOwner>,
    code: LintCode,
    message: String,
    span: Span,
    label: String,
    definition: Option<(FileId, Span)>,
) -> Option<CallFinding> {
    let mut owners = BTreeSet::new();
    let mut deny = false;
    for owner in reporters {
        owners.insert(owner.name.clone());
        deny |= owner.policy == CallUsages::Deny;
    }
    (!owners.is_empty()).then_some(CallFinding {
        code,
        deny,
        message,
        span,
        label,
        definition,
        owners,
    })
}

fn call_findings(call: &Call) -> Vec<CallFinding> {
    let key = &call.reference.key;
    let definitions: Vec<_> = call
        .seen
        .iter()
        .filter_map(|(owner, definition)| Some((owner, definition.as_ref()?)))
        .collect();

    // Undefined key
    if definitions.is_empty() {
        return attributed(
            call.seen
                .iter()
                .map(|(owner, _)| owner.as_ref())
                .filter(|owner| owner.reports(LintCode::UndefinedKey)),
            LintCode::UndefinedKey,
            format!("translation key `{key}` is not defined"),
            call.reference.span.clone(),
            "no config that recognizes this call defines this key".to_owned(),
            None,
        )
        .into_iter()
        .collect();
    }

    // Argument mismatches count only when every defining catalog rejects the call.
    let Some(arguments) = &call.reference.arguments else {
        return Vec::new();
    };
    let issues: Vec<_> = definitions
        .iter()
        .map(|(owner, definition)| {
            let issues = if is_allowed(&definition.allow, LintCode::ArgumentMismatch) {
                Vec::new()
            } else {
                argument_issues(
                    key,
                    &call.reference.span,
                    arguments,
                    &definition.arguments,
                    &owner.implied,
                )
            };
            (owner, definition, issues)
        })
        .collect();
    if issues.iter().any(|(_, _, issues)| issues.is_empty()) {
        return Vec::new();
    }

    // Identical issues from several catalogs become one finding.
    let mut merged: BTreeMap<IssueKey, MergedIssue<'_>> = BTreeMap::new();
    for (owner, definition, issues) in &issues {
        if !owner.reports(LintCode::ArgumentMismatch) {
            continue;
        }
        for issue in issues {
            merged
                .entry(IssueKey {
                    start: issue.span.start,
                    end: issue.span.end,
                    message: issue.message.clone(),
                    label: issue.label.clone(),
                })
                .or_insert_with(|| MergedIssue {
                    owners: Vec::new(),
                    definition: (definition.file_id, definition.span.clone()),
                })
                .owners
                .push(owner);
        }
    }
    merged
        .into_iter()
        .filter_map(|(key, issue)| {
            attributed(
                issue.owners,
                LintCode::ArgumentMismatch,
                key.message,
                key.start..key.end,
                key.label,
                Some(issue.definition),
            )
        })
        .collect()
}

fn prefix_finding(prefix: DynamicCall) -> Option<CallFinding> {
    if prefix.seen.iter().any(|(_, matched)| *matched) {
        return None;
    }
    attributed(
        prefix
            .seen
            .iter()
            .map(|(owner, _)| owner.as_ref())
            .filter(|owner| owner.reports(LintCode::UndefinedKey)),
        LintCode::UndefinedKey,
        format!("no translation key starts with `{}`", prefix.prefix),
        prefix.span,
        "no config that recognizes this call defines a key with this prefix".to_owned(),
        None,
    )
}

/// Orders identical argument issues from several catalogs together.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct IssueKey {
    start: usize,
    end: usize,
    message: String,
    label: String,
}

struct MergedIssue<'a> {
    owners: Vec<&'a CallOwner>,
    definition: (FileId, Span),
}

#[derive(Debug, PartialEq)]
struct ArgumentIssue {
    message: String,
    span: Span,
    label: String,
}

fn accepts(declared: ArgumentType, value: ArgumentValue) -> bool {
    match value {
        ArgumentValue::Other => true,
        ArgumentValue::String => matches!(
            declared,
            ArgumentType::Any | ArgumentType::String | ArgumentType::Iso8601DateTimeString
        ),
        ArgumentValue::Number => matches!(
            declared,
            ArgumentType::Any | ArgumentType::Number | ArgumentType::Integer | ArgumentType::Float
        ),
        ArgumentValue::Boolean => matches!(declared, ArgumentType::Any | ArgumentType::Boolean),
    }
}

fn literal_kind(value: ArgumentValue) -> &'static str {
    match value {
        ArgumentValue::String => "a string literal",
        ArgumentValue::Number => "a number literal",
        ArgumentValue::Boolean => "a boolean literal",
        ArgumentValue::Other => "an expression",
    }
}

/// Compares the arguments a call passes with the key's declared arguments.
///
/// A missing argument in `implied` is supplied by the host and not reported.
fn argument_issues(
    key: &str,
    key_span: &Span,
    arguments: &[CallArgument],
    declared: &Arguments,
    implied: &BTreeSet<String>,
) -> Vec<ArgumentIssue> {
    let mut issues = Vec::new();
    for (name, typ) in declared {
        if !implied.contains(name) && !arguments.iter().any(|argument| &argument.name == name) {
            issues.push(ArgumentIssue {
                message: format!("translation key `{key}` is missing argument `{name}`"),
                span: key_span.clone(),
                label: format!("`{name}` is declared as `{}` but not passed", typ.display()),
            });
        }
    }
    for argument in arguments {
        match declared.get(&argument.name) {
            None => issues.push(ArgumentIssue {
                message: format!(
                    "translation key `{key}` has no argument `{}`",
                    argument.name
                ),
                span: argument.span.clone(),
                label: "not declared for this key".to_owned(),
            }),
            Some(typ) if !accepts(*typ, argument.value) => issues.push(ArgumentIssue {
                message: format!(
                    "argument `{}` of translation key `{key}` must be `{}`",
                    argument.name,
                    typ.display()
                ),
                span: argument.span.clone(),
                label: format!("this is {}", literal_kind(argument.value)),
            }),
            Some(_) => {}
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::{CallChecks, CallFinding, CallOwner, CallSource};
    use crate::config::usages::CallUsages;
    use crate::dead_keys::DefinedKey;
    use crate::dead_keys::source::{ArgumentValue, CallArgument, CallReference, DynamicReference};
    use globetrotter_model::{
        ArgumentType,
        lint::{AllowEntry, LintCode},
    };
    use std::collections::BTreeSet;
    use std::sync::Arc;

    fn owner(name: &str, policy: CallUsages) -> Arc<CallOwner> {
        Arc::new(CallOwner {
            name: name.to_owned(),
            policy,
            allow: BTreeSet::new(),
            implied: BTreeSet::new(),
        })
    }

    fn key(name: &str, arguments: &[(&str, ArgumentType)]) -> DefinedKey {
        DefinedKey {
            key: name.to_owned(),
            generated_identifiers: Vec::new(),
            file_id: 7,
            span: 1..2,
            allow: BTreeSet::new(),
            arguments: arguments
                .iter()
                .map(|(name, typ)| ((*name).to_owned(), *typ))
                .collect(),
        }
    }

    fn call(key: &str, arguments: Option<&[(&str, ArgumentValue)]>) -> CallReference {
        CallReference {
            key: key.to_owned(),
            span: 10..20,
            arguments: arguments.map(|arguments| {
                arguments
                    .iter()
                    .enumerate()
                    .map(|(index, (name, value))| CallArgument {
                        name: (*name).to_owned(),
                        span: 30 + index..31 + index,
                        value: *value,
                    })
                    .collect()
            }),
        }
    }

    fn source(calls: Vec<CallReference>) -> Vec<CallSource> {
        vec![CallSource {
            path: "doc.typ".into(),
            content: String::new(),
            calls,
            prefixes: Vec::new(),
        }]
    }

    fn findings(checks: CallChecks) -> Vec<CallFinding> {
        checks
            .finish()
            .into_iter()
            .flat_map(|file| file.findings)
            .collect()
    }

    fn summary(findings: &[CallFinding]) -> Vec<(LintCode, bool, String, Vec<String>)> {
        findings
            .iter()
            .map(|finding| {
                (
                    finding.code,
                    finding.deny,
                    finding.message.clone(),
                    finding.owners.iter().cloned().collect(),
                )
            })
            .collect()
    }

    /// A key defined by any config that recognized the call is not undefined.
    #[test_util::test]
    fn undefined_keys_need_every_recognizing_config_to_lack_them() {
        let mut checks = CallChecks::default();
        checks.add(
            &owner("app", CallUsages::Warn),
            &[key("app.title", &[])],
            source(vec![call("pdf.title", None), call("app.title", None)]),
        );
        checks.add(
            &owner("pdf", CallUsages::Deny),
            &[key("pdf.title", &[])],
            source(vec![call("pdf.title", None), call("app.title", None)]),
        );
        assert_eq!(findings(checks), Vec::new());

        // A key no recognizing config defines is reported by all of them, with the strictest
        // severity.
        let mut checks = CallChecks::default();
        checks.add(
            &owner("app", CallUsages::Warn),
            &[key("app.title", &[])],
            source(vec![call("app.typo", None)]),
        );
        checks.add(
            &owner("pdf", CallUsages::Deny),
            &[key("pdf.title", &[])],
            source(vec![call("app.typo", None)]),
        );
        similar_asserts::assert_eq!(
            summary(&findings(checks)),
            vec![(
                LintCode::UndefinedKey,
                true,
                "translation key `app.typo` is not defined".to_owned(),
                vec!["app".to_owned(), "pdf".to_owned()],
            )]
        );
    }

    /// A config that does not recognize a call neither defines nor reports it.
    #[test_util::test]
    fn configs_that_do_not_recognize_a_call_do_not_vouch_for_it() {
        let mut checks = CallChecks::default();
        checks.add(
            &owner("app", CallUsages::Warn),
            &[],
            source(vec![call("pdf.title", None)]),
        );
        // `pdf` defines the key but scans other files or uses other functions.
        checks.add(
            &owner("pdf", CallUsages::Deny),
            &[key("pdf.title", &[])],
            Vec::new(),
        );
        similar_asserts::assert_eq!(
            summary(&findings(checks)),
            vec![(
                LintCode::UndefinedKey,
                false,
                "translation key `pdf.title` is not defined".to_owned(),
                vec!["app".to_owned()],
            )]
        );
    }

    /// `allow` policies and config-wide suppressions still define keys but report nothing.
    #[test_util::test]
    fn allowed_configs_define_keys_without_reporting() {
        let mut checks = CallChecks::default();
        checks.add(
            &owner("quiet", CallUsages::Allow),
            &[],
            source(vec![call("app.typo", None)]),
        );
        let suppressed = Arc::new(CallOwner {
            name: "suppressed".to_owned(),
            policy: CallUsages::Deny,
            allow: [AllowEntry::Code(LintCode::UndefinedKey)].into(),
            implied: BTreeSet::new(),
        });
        checks.add(&suppressed, &[], source(vec![call("app.typo", None)]));
        assert_eq!(findings(checks), Vec::new());
    }

    /// Missing, unexpected, and mistyped arguments are each reported.
    #[test_util::test]
    fn argument_mismatches_are_reported_against_the_declaration() {
        use ArgumentValue::{Boolean, Number, Other, String};
        let mut checks = CallChecks::default();
        checks.add(
            &owner("app", CallUsages::Warn),
            &[key(
                "greeting",
                &[
                    ("name", ArgumentType::String),
                    ("count", ArgumentType::Integer),
                    ("paid", ArgumentType::Boolean),
                    ("date", ArgumentType::Iso8601DateTimeString),
                    ("value", ArgumentType::Any),
                ],
            )],
            source(vec![call(
                "greeting",
                Some(&[
                    ("name", Number),
                    ("count", String),
                    ("paid", Other),
                    ("value", Boolean),
                    ("extra", String),
                ]),
            )]),
        );
        let findings = findings(checks);
        similar_asserts::assert_eq!(
            summary(&findings),
            [
                "translation key `greeting` is missing argument `date`",
                "argument `name` of translation key `greeting` must be `string`",
                "argument `count` of translation key `greeting` must be `integer`",
                "translation key `greeting` has no argument `extra`",
            ]
            .map(|message| (
                LintCode::ArgumentMismatch,
                false,
                message.to_owned(),
                vec!["app".to_owned()],
            ))
            .to_vec()
        );
        // Argument findings point back to the key's declaration.
        assert!(
            findings
                .iter()
                .all(|finding| finding.definition == Some((7, 1..2)))
        );
    }

    /// Opaque arguments and accepted literals produce no argument findings.
    #[test_util::test]
    fn opaque_and_matching_arguments_are_accepted() {
        use ArgumentValue::{Number, String};
        let mut checks = CallChecks::default();
        checks.add(
            &owner("app", CallUsages::Deny),
            &[key(
                "greeting",
                &[
                    ("name", ArgumentType::String),
                    ("price", ArgumentType::Float),
                ],
            )],
            source(vec![
                call("greeting", None),
                call("greeting", Some(&[("name", String), ("price", Number)])),
            ]),
        );
        assert_eq!(findings(checks), Vec::new());
    }

    /// A call is valid when any defining catalog accepts its arguments.
    #[test_util::test]
    fn arguments_need_every_defining_catalog_to_reject_them() {
        let mut checks = CallChecks::default();
        checks.add(
            &owner("old", CallUsages::Warn),
            &[key("greeting", &[("name", ArgumentType::String)])],
            source(vec![call("greeting", Some(&[]))]),
        );
        checks.add(
            &owner("new", CallUsages::Warn),
            &[key("greeting", &[])],
            source(vec![call("greeting", Some(&[]))]),
        );
        assert_eq!(findings(checks), Vec::new());

        // When both reject the call, each catalog's own findings are reported, and identical
        // findings merge.
        let mut checks = CallChecks::default();
        for name in ["first", "second"] {
            checks.add(
                &owner(name, CallUsages::Warn),
                &[key("greeting", &[("name", ArgumentType::String)])],
                source(vec![call("greeting", Some(&[]))]),
            );
        }
        similar_asserts::assert_eq!(
            summary(&findings(checks)),
            vec![(
                LintCode::ArgumentMismatch,
                false,
                "translation key `greeting` is missing argument `name`".to_owned(),
                vec!["first".to_owned(), "second".to_owned()],
            )]
        );
    }

    /// An implied argument may be omitted, but an explicit value is still type-checked.
    #[test_util::test]
    fn implied_arguments_may_be_omitted_but_are_still_type_checked() {
        use ArgumentValue::{Boolean, String};
        let implying = Arc::new(CallOwner {
            name: "app".to_owned(),
            policy: CallUsages::Deny,
            allow: BTreeSet::new(),
            implied: ["is_formal".to_owned()].into(),
        });
        let greeting = key(
            "greeting",
            &[
                ("name", ArgumentType::String),
                ("is_formal", ArgumentType::Boolean),
            ],
        );
        let mut checks = CallChecks::default();
        checks.add(
            &implying,
            std::slice::from_ref(&greeting),
            source(vec![
                call("greeting", Some(&[("name", String)])),
                call(
                    "greeting",
                    Some(&[("name", String), ("is_formal", Boolean)]),
                ),
            ]),
        );
        assert_eq!(findings(checks), Vec::new());

        let mut checks = CallChecks::default();
        checks.add(
            &implying,
            &[greeting],
            source(vec![call(
                "greeting",
                Some(&[("name", String), ("is_formal", String)]),
            )]),
        );
        similar_asserts::assert_eq!(
            summary(&findings(checks)),
            vec![(
                LintCode::ArgumentMismatch,
                true,
                "argument `is_formal` of translation key `greeting` must be `boolean`".to_owned(),
                vec!["app".to_owned()],
            )]
        );
    }

    /// Each config's implied arguments apply only to its own catalog's decision.
    #[test_util::test]
    fn implied_arguments_belong_to_their_own_config() {
        let implying = Arc::new(CallOwner {
            name: "wrapped".to_owned(),
            policy: CallUsages::Warn,
            allow: BTreeSet::new(),
            implied: ["is_formal".to_owned()].into(),
        });
        let greeting = || key("greeting", &[("is_formal", ArgumentType::Boolean)]);

        // The implying config accepts the call, so the other config's rejection is not reported.
        let mut checks = CallChecks::default();
        checks.add(
            &implying,
            &[greeting()],
            source(vec![call("greeting", Some(&[]))]),
        );
        checks.add(
            &owner("plain", CallUsages::Warn),
            &[greeting()],
            source(vec![call("greeting", Some(&[]))]),
        );
        assert_eq!(findings(checks), Vec::new());

        // An implying config that does not define the key does not excuse another catalog.
        let mut checks = CallChecks::default();
        checks.add(&implying, &[], source(vec![call("greeting", Some(&[]))]));
        checks.add(
            &owner("plain", CallUsages::Warn),
            &[greeting()],
            source(vec![call("greeting", Some(&[]))]),
        );
        similar_asserts::assert_eq!(
            summary(&findings(checks)),
            vec![(
                LintCode::ArgumentMismatch,
                false,
                "translation key `greeting` is missing argument `is_formal`".to_owned(),
                vec!["plain".to_owned()],
            )]
        );
    }

    /// A key-level `allow` entry accepts any arguments for that key.
    #[test_util::test]
    fn key_level_allow_accepts_mismatched_arguments() {
        let mut allowed = key("greeting", &[("name", ArgumentType::String)]);
        allowed.allow = [AllowEntry::Code(LintCode::ArgumentMismatch)].into();
        let mut checks = CallChecks::default();
        checks.add(
            &owner("app", CallUsages::Deny),
            &[allowed],
            source(vec![call("greeting", Some(&[]))]),
        );
        assert_eq!(findings(checks), Vec::new());
    }

    /// A dynamic call's prefix must match a key of some recognizing config.
    #[test_util::test]
    fn dynamic_prefixes_need_a_matching_key() {
        let prefixed = |prefix: &str, start: usize| {
            vec![CallSource {
                path: "doc.typ".into(),
                content: String::new(),
                calls: Vec::new(),
                prefixes: vec![DynamicReference {
                    prefix: Some(prefix.to_owned()),
                    span: start..start + 4,
                }],
            }]
        };
        let mut checks = CallChecks::default();
        checks.add(
            &owner("app", CallUsages::Warn),
            &[key("status.open", &[])],
            prefixed("status.", 5),
        );
        checks.add(
            &owner("app", CallUsages::Warn),
            &[key("status.open", &[])],
            prefixed("state.", 50),
        );
        similar_asserts::assert_eq!(
            summary(&findings(checks)),
            vec![(
                LintCode::UndefinedKey,
                false,
                "no translation key starts with `state.`".to_owned(),
                vec!["app".to_owned()],
            )]
        );
    }
}
