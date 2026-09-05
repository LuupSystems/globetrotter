use super::super::scan;
use std::path::Path;

#[test_util::test]
fn backend_branches_keep_both_values_and_unknown_branches_remain_dynamic() {
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

        // An unknown branch must not erase a known complete key or escape dynamic policy.
        let source = source.replace("\"app.other\"", unknown);
        let references = scan(Path::new(name), &source, &["t".into()])?;
        assert_eq!(references.literals, ["app.live".into()].into(), "{source}");
        assert_eq!(references.dynamic.len(), 1, "{source}");
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
        assert_eq!(references.dynamic.len(), 1, "{source}");
    }
}

#[test_util::test]
fn missing_conditional_branches_are_not_proven_static() {
    let source = r#"t(if open, do: "app.live")"#;
    let references = scan(Path::new("app.ex"), source, &["t".into()])?;
    assert!(references.literals.contains("app.live"));
    assert_eq!(references.dynamic.len(), 1);
}

#[test_util::test]
fn elixir_block_branches_use_their_final_values() {
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
    assert_eq!(references.dynamic.len(), 1);
}
