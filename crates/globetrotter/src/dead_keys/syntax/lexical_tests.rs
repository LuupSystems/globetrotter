//! Literal references do not depend on resolving their surrounding expressions.

use super::scan;
use std::path::Path;

#[test_util::test]
fn backend_branches_keep_literals_and_ignore_opaque_values() {
    for (name, source, unknown) in [
        (
            "app.php",
            r#"<?php t($open ? "app.live" : "app.other");"#,
            "$key",
        ),
        ("app.php", r#"<?php t("app.live" ?: "app.other");"#, "$key"),
        ("app.rb", r#"t(open ? "app.live" : "app.other")"#, "key"),
        (
            "app.swift",
            r#"let result = t(open ? "app.live" : "app.other")"#,
            "key",
        ),
        (
            "app.swift",
            r#"let result = t(["app.live", "app.other"])"#,
            "key",
        ),
        (
            "app.kt",
            r#"fun main() { t(when (kind) { 1 -> "app.live"; else -> "app.other" }) }"#,
            "key",
        ),
        (
            "app.kt",
            r#"fun main() { t(when (kind) { 1 -> { "app.live" }; else -> { "app.other" } }) }"#,
            "key",
        ),
        (
            "app.dart",
            r#"void main() { t(["app.live", "app.other"]); }"#,
            "key",
        ),
        (
            "app.dart",
            r#"void main() { t(<String>["app.live", "app.other"]); }"#,
            "key",
        ),
        (
            "app.zig",
            r#"pub fn main() void { t(if (open) "app.live" else "app.other"); }"#,
            "key",
        ),
        (
            "app.ex",
            r#"t(if open, do: "app.live", else: "app.other")"#,
            "key",
        ),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert_eq!(
            references.literals,
            ["app.live".into(), "app.other".into()].into(),
            "{source}"
        );
        assert!(references.dynamic.is_empty(), "{source}");

        // Opaque branches neither erase literal evidence nor imply string construction.
        let source = source.replace("\"app.other\"", unknown);
        let references = scan(Path::new(name), &source, &["t".into()])?;
        assert_eq!(references.literals, ["app.live".into()].into(), "{source}");
        assert!(references.dynamic.is_empty(), "{source}");
    }
}

#[test_util::test]
fn backend_fallbacks_keep_complete_literals_without_assuming_unknown_values() {
    for (name, source) in [
        ("app.py", r#"t(key or "app.live")"#),
        ("app.py", r#"t(key and "app.live")"#),
        ("app.rb", r#"t(key || "app.live")"#),
        ("app.rb", r#"t(key && "app.live")"#),
        ("app.swift", r#"let result = t(key ?? "app.live")"#),
        ("app.kt", r#"fun main() { t(key ?: "app.live") }"#),
        ("app.dart", r#"void main() { t(key ?? "app.live"); }"#),
        ("app.php", r#"<?php t($key ?: "app.live");"#),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert_eq!(references.literals, ["app.live".into()].into(), "{source}");
        assert!(references.dynamic.is_empty(), "{source}");
    }
}

#[test_util::test]
fn missing_conditional_branches_do_not_imply_string_construction() {
    let source = r#"t(if open, do: "app.live")"#;
    let references = scan(Path::new("app.ex"), source, &["t".into()])?;
    assert!(references.literals.contains("app.live"));
    assert!(references.dynamic.is_empty());
}

#[test_util::test]
fn elixir_block_literals_count_without_evaluating_branches() {
    let source = indoc::indoc! {r#"
        t(if open do
          prepare()
          "app.live"
        else
          "app.other"
        end)
    "#};
    let references = scan(Path::new("app.ex"), source, &["t".into()])?;
    assert_eq!(
        references.literals,
        ["app.live".into(), "app.other".into()].into()
    );
    assert!(references.dynamic.is_empty());

    let source = source.replace("\"app.other\"", "key");
    let references = scan(Path::new("app.ex"), &source, &["t".into()])?;
    assert_eq!(references.literals, ["app.live".into()].into());
    assert!(references.dynamic.is_empty());
}

#[test_util::test]
fn opaque_arguments_do_not_construct_strings() {
    let source = indoc::indoc! {r#"
        const key: TranslationKey = "account.title";
        t(key);
        t?.(key);
        object.t(key);
        t(keys[kind]);
        props.t(getKey(value));
        t(getKey(`unrelated.${kind}`));
        t(getKey("account.fallback"));
        t(index + offset);
        t(open ? "dialog.open" : "dialog.closed");
    "#};
    let references = scan(Path::new("app.ts"), source, &["t".into()])?;
    assert_eq!(
        references.literals,
        [
            "account.title",
            "account.fallback",
            "dialog.open",
            "dialog.closed"
        ]
        .map(str::to_owned)
        .into()
    );
    assert!(references.dynamic.is_empty());
}

#[test_util::test]
fn only_direct_visible_string_construction_is_dynamic() {
    let source = indoc::indoc! {r#"
        const unrelated = `unused.${kind}`;
        t(`account.${section}`);
        t("account." + section);
        t(section + ".title");
        i18n.t(`dialog.${kind}.title`);
        t(keys[`lookup.${kind}`]);
    "#};
    let references = scan(Path::new("app.ts"), source, &["t".into()])?;
    assert_eq!(
        references
            .dynamic
            .iter()
            .map(|usage| source.get(usage.span.clone()))
            .collect::<Vec<_>>(),
        [
            Some("`account.${section}`"),
            Some("\"account.\" + section"),
            Some("section + \".title\""),
            Some("`dialog.${kind}.title`")
        ]
    );
    assert_eq!(
        references
            .dynamic
            .iter()
            .filter_map(|usage| usage.prefix.as_deref())
            .collect::<Vec<_>>(),
        ["account.", "account.", "dialog."]
    );
}

#[test_util::test]
fn frontend_opaque_calls_follow_the_shared_literal_rule() {
    for (name, source) in [
        (
            "app.ts",
            r#"const key: TranslationKey = "account.title"; t(key); t?.(key);"#,
        ),
        (
            "react.tsx",
            r#"const key = "account.title"; export const View = () => <p>{props.t(key)}</p>;"#,
        ),
        (
            "component.vue",
            r#"<script setup lang="ts">const key: TranslationKey = "account.title";</script><template><p>{{ t(key) }}</p></template>"#,
        ),
        (
            "component.svelte",
            r#"<script lang="ts">const key: TranslationKey = "account.title";</script><p>{t(key)}</p>"#,
        ),
        (
            "component.astro",
            r#"<script>const key: TranslationKey = "account.title";</script><p>{t(key)}</p>"#,
        ),
        (
            "app.html",
            r#"<script>const key = "account.title";</script><p>{{ t(key) }} {{ key | translate }}</p>"#,
        ),
    ] {
        let references = scan(Path::new(name), source, &["t".into(), "translate".into()])?;
        assert_eq!(
            references.literals,
            ["account.title".into()].into(),
            "{name}"
        );
        assert!(references.dynamic.is_empty(), "{name}");
    }
}

#[test_util::test]
fn backend_visible_concatenations_share_the_string_classifier() {
    for (name, source) in [
        ("app.go", r#"package app; func run() { t("app." + kind) }"#),
        ("app.rs", r#"fn main() { t("app." + kind); }"#),
        ("app.py", r#"t("app." + kind)"#),
        ("app.rb", r#"t("app." + kind)"#),
        ("app.php", r#"<?php t("app." . $kind);"#),
        (
            "App.java",
            r#"class App { void run() { t("app." + kind); } }"#,
        ),
        ("app.kt", r#"fun main() { t("app." + kind) }"#),
        ("app.swift", r#"let value = t("app." + kind)"#),
        ("app.dart", r#"void main() { t("app." + kind); }"#),
        ("app.ex", r#"t("app." <> kind)"#),
        ("app.lua", r#"t("app." .. kind)"#),
        ("app.zig", r#"pub fn main() void { t("app." ++ kind); }"#),
        (
            "App.cs",
            r#"class App { void Run() { t("app." + kind); } }"#,
        ),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert_eq!(references.dynamic.len(), 1, "{name}: {source}");
        assert_eq!(
            references.dynamic[0].prefix.as_deref(),
            Some("app."),
            "{name}"
        );
        assert!(references.literals.contains("app."), "{name}");
    }
}

#[test_util::test]
fn python_adjacent_strings_preserve_visible_interpolation() {
    for argument in [r#""app." f"{kind}""#, r#"f"app.{kind}" "suffix""#] {
        let source = format!("t({argument})");
        let references = scan(Path::new("app.py"), &source, &["t".into()])?;
        assert_eq!(references.dynamic.len(), 1, "{source}");
        assert_eq!(references.dynamic[0].prefix.as_deref(), Some("app."));
        assert_eq!(
            source.get(references.dynamic[0].span.clone()),
            Some(argument)
        );
    }
    let source = indoc::indoc! {r#"
        key = "app." f"{kind}"
        t(key)
    "#};
    let references = scan(Path::new("app.py"), source, &["t".into()])?;
    assert!(references.dynamic.is_empty());
}

#[test_util::test]
fn angular_pipe_right_string_operands_are_visible_construction() {
    let source =
        r#"<p [title]="kind + '.title' | translate">{{ kind + '.title' | translate }}</p>"#;
    let references = scan(Path::new("app.html"), source, &["translate".into()])?;
    assert_eq!(references.dynamic.len(), 2);
    assert!(
        references
            .dynamic
            .iter()
            .all(|usage| usage.prefix.is_none())
    );
    assert!(
        references
            .dynamic
            .iter()
            .all(|usage| source.get(usage.span.clone()) == Some("kind + '.title'"))
    );
    assert_eq!(references.literals, [".title".into()].into());
}
