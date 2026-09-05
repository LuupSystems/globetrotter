//! Regression tests distinguish interpolation from escaped and verbatim literal text.

use super::super::scan;
use std::path::Path;

#[test_util::test]
fn kotlin_dollar_interpolation_has_a_prefix_and_source_span() {
    for argument in [
        r#""app.$kind""#,
        r#""app.${kind}""#,
        r#""app.$kind.${other}""#,
        r#""""app.$kind""""#,
    ] {
        let source = format!("fun main() {{ t({argument}) }}");
        let references = scan(Path::new("app.kt"), &source, &["t".into()])?;
        assert!(references.literals.is_empty(), "{source}");
        assert_eq!(references.dynamic.len(), 1, "{source}");
        let reference = &references.dynamic[0];
        assert_eq!(reference.prefix.as_deref(), Some("app."), "{source}");
        assert_eq!(
            source.get(reference.span.clone()),
            Some(argument),
            "{source}"
        );
    }
}

#[test_util::test]
fn kotlin_dollar_escapes_and_raw_strings_follow_their_delimiters() {
    let source = indoc::indoc! {r#"
        fun main() {
            t("app.\$kind")
            t("app.$5")
            t("app.$")
        }
    "#};
    let references = scan(Path::new("app.kt"), source, &["t".into()])?;
    assert!(references.dynamic.is_empty());
    assert_eq!(
        references.literals,
        ["app.$kind".into(), "app.$5".into(), "app.$".into()].into(),
    );

    // A raw string's backslash cannot escape the following interpolation.
    let source = r#"fun main() { t("""app.\$kind""") }"#;
    let references = scan(Path::new("app.kt"), source, &["t".into()])?;
    assert!(references.literals.is_empty());
    assert_eq!(references.dynamic.len(), 1);
    assert!(references.dynamic[0].prefix.is_none());
}

#[test_util::test]
fn php_braced_dollar_interpolation_is_dynamic_but_escaped_dollars_are_static() {
    for argument in [r#""app.${kind}""#, r#""app.$kind""#] {
        let source = format!("<?php t({argument});");
        let references = scan(Path::new("app.php"), &source, &["t".into()])?;
        assert!(references.literals.is_empty(), "{source}");
        assert_eq!(references.dynamic.len(), 1, "{source}");
        let reference = &references.dynamic[0];
        assert_eq!(reference.prefix.as_deref(), Some("app."), "{source}");
        assert_eq!(
            source.get(reference.span.clone()),
            Some(argument),
            "{source}"
        );
    }

    let source = indoc::indoc! {r#"
        <?php
        t("app.\${kind}");
        t('app.${kind}');
    "#};
    let references = scan(Path::new("app.php"), source, &["t".into()])?;
    assert!(references.dynamic.is_empty());
    assert_eq!(references.literals, ["app.${kind}".into()].into());
}

#[test_util::test]
fn csharp_verbatim_literals_preserve_backslashes_and_decode_doubled_quotes() {
    let source = indoc::indoc! {r#"
        class App {
            void Run() {
                t(@"app.live");
                t(@"app.\name");
                t(@"app.""quoted""");
                t(@"""app.live""");
            }
        }
    "#};
    let references = scan(Path::new("App.cs"), source, &["t".into()])?;
    assert!(references.dynamic.is_empty());
    assert_eq!(
        references.literals,
        [
            "app.live".into(),
            r"app.\name".into(),
            "app.\"quoted\"".into(),
            "\"app.live\"".into(),
        ]
        .into(),
    );
}
