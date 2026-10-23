//! Regression tests for the keys and template arguments of static translation calls.

use super::super::scan;
use crate::dead_keys::source::{ArgumentValue, CallReference};
use std::path::Path;

/// A call's key and its arguments as `(name, source text, value)`, or `None` when opaque.
type Shape<'a> = (String, Option<Vec<(String, &'a str, ArgumentValue)>>);

fn shapes<'a>(name: &str, source: &'a str, functions: &[&str]) -> std::io::Result<Vec<Shape<'a>>> {
    let functions: Vec<String> = functions
        .iter()
        .map(|&function| function.to_owned())
        .collect();
    let references = scan(Path::new(name), source, &functions)?;
    references
        .calls
        .into_iter()
        .map(
            |CallReference {
                 key,
                 span,
                 arguments,
             }| {
                // The key span covers exactly the literal key argument.
                let key_text = source.get(span).unwrap_or_default();
                assert!(
                    key_text.contains(key.as_str()),
                    "{key_text} does not contain {key}"
                );
                let arguments = arguments.map(|arguments| {
                    arguments
                        .into_iter()
                        .map(|argument| {
                            (
                                argument.name,
                                source.get(argument.span).unwrap_or_default(),
                                argument.value,
                            )
                        })
                        .collect()
                });
                Ok((key, arguments))
            },
        )
        .collect()
}

fn args<'a>(arguments: &[(&str, &'a str, ArgumentValue)]) -> Vec<(String, &'a str, ArgumentValue)> {
    arguments
        .iter()
        .map(|&(name, text, value)| (name.to_owned(), text, value))
        .collect()
}

/// Typst named arguments are the template arguments, with literal kinds where visible.
#[test_util::test]
fn typst_named_arguments() {
    use ArgumentValue::{Boolean, Number, Other, String};
    let source = indoc::indoc! {r#"
        #t("app.plain")
        #t("app.args", name: "Ada", count: 2, price: 2.5, paid: true, user: user, empty: none)
        #t(count: 1, "app.named-first")
        #t("app.spread", ..args)
        #t("app.positional", "extra")
        #t("app.content")[body]
        #t("app.dynamic-value", name: "a" + b)
    "#};
    let have = shapes("doc.typ", source, &["t"])?;
    let want: Vec<Shape<'_>> = vec![
        ("app.plain".into(), Some(args(&[]))),
        (
            "app.args".into(),
            Some(args(&[
                ("name", "name: \"Ada\"", String),
                ("count", "count: 2", Number),
                ("price", "price: 2.5", Number),
                ("paid", "paid: true", Boolean),
                ("user", "user: user", Other),
                ("empty", "empty: none", Other),
            ])),
        ),
        (
            "app.named-first".into(),
            Some(args(&[("count", "count: 1", Number)])),
        ),
        ("app.spread".into(), None),
        ("app.positional".into(), None),
        ("app.content".into(), None),
        (
            "app.dynamic-value".into(),
            Some(args(&[("name", "name: \"a\" + b", String)])),
        ),
    ];
    similar_asserts::assert_eq!(have, want);
}

/// JavaScript-family calls take their template arguments from an object literal after the key.
#[test_util::test]
fn javascript_object_arguments() {
    use ArgumentValue::{Boolean, Number, Other, String};
    let source = indoc::indoc! {r#"
        t("app.plain");
        t("app.args", { name: "Ada", "count": 2, paid: false, user, label: `x`, [computed]: 1 });
        t("app.static", { name: "Ada", count: 2, paid: false, user, label: `x`, fn() {} });
        t("app.spread", { ...rest });
        t("app.variable", values);
        i18n.t("app.member", { name: "Ada" } as const);
        t(`app.template`, { count: 1 });
    "#};
    let have = shapes("app.ts", source, &["t"])?;
    let want: Vec<Shape<'_>> = vec![
        ("app.plain".into(), Some(args(&[]))),
        ("app.args".into(), None),
        (
            "app.static".into(),
            Some(args(&[
                ("name", "name: \"Ada\"", String),
                ("count", "count: 2", Number),
                ("paid", "paid: false", Boolean),
                ("user", "user", Other),
                ("label", "label: `x`", String),
                ("fn", "fn() {}", Other),
            ])),
        ),
        ("app.spread".into(), None),
        ("app.variable".into(), None),
        (
            "app.member".into(),
            Some(args(&[("name", "name: \"Ada\"", String)])),
        ),
        (
            "app.template".into(),
            Some(args(&[("count", "count: 1", Number)])),
        ),
    ];
    similar_asserts::assert_eq!(have, want);
}

/// Python keyword arguments are the template arguments; splats make them opaque.
#[test_util::test]
fn python_keyword_arguments() {
    use ArgumentValue::{Boolean, Number, String};
    let source = indoc::indoc! {r#"
        t("app.plain")
        t("app.args", name="Ada", count=2, paid=True, price=2.5)
        t("app.splat", **values)
        t("app.positional", "extra")
        t(key="app.keyword", name=f"{first} {last}")
    "#};
    let have = shapes("app.py", source, &["t"])?;
    let want: Vec<Shape<'_>> = vec![
        ("app.plain".into(), Some(args(&[]))),
        (
            "app.args".into(),
            Some(args(&[
                ("name", "name=\"Ada\"", String),
                ("count", "count=2", Number),
                ("paid", "paid=True", Boolean),
                ("price", "price=2.5", Number),
            ])),
        ),
        ("app.splat".into(), None),
        ("app.positional".into(), None),
        (
            "app.keyword".into(),
            Some(args(&[("name", "name=f\"{first} {last}\"", String)])),
        ),
    ];
    similar_asserts::assert_eq!(have, want);
}

/// Other languages and Angular pipes contribute their keys with opaque arguments.
#[test_util::test]
fn other_languages_check_keys_only() {
    for (name, source) in [
        ("app.rb", "t('app.live', name: 'Ada')"),
        (
            "app.go",
            "package main\nfunc main() { t(\"app.live\", args) }",
        ),
        ("app.rs", "fn main() { t(\"app.live\"); }"),
        (
            "app.html",
            "<p>{{ 'app.live' | translate: { name: 'Ada' } }}</p>",
        ),
        ("app.html", "<p>{{ t('app.live', { name: 'Ada' }) }}</p>"),
    ] {
        let have = shapes(name, source, &["t", "translate"])?;
        similar_asserts::assert_eq!(have, vec![("app.live".to_owned(), None)], "{source}");
    }
}

/// Calls inside template attributes keep spans in the original source.
#[test_util::test]
fn embedded_calls_map_spans_to_the_original_source() {
    let source = r#"<template><p :title="t('app.live', { name: &quot;Ada&quot; })">{{ t("app.other") }}</p></template>"#;
    let have = shapes("component.vue", source, &["t"])?;
    similar_asserts::assert_eq!(
        have,
        vec![
            (
                "app.live".to_owned(),
                Some(args(&[(
                    "name",
                    "name: &quot;Ada&quot;",
                    ArgumentValue::String
                )]))
            ),
            ("app.other".to_owned(), Some(args(&[]))),
        ]
    );
}

/// Dynamic and unrelated calls are not static calls.
#[test_util::test]
fn dynamic_and_unrelated_calls_are_not_static_calls() {
    let source = indoc::indoc! {r#"
        t(`app.${kind}`);
        t("app." + kind);
        t(key);
        other("app.live");
    "#};
    assert_eq!(shapes("app.ts", source, &["t"])?, Vec::new());
}
