//! Regression tests distinguish runtime references from non-runtime source text.

use super::*;

mod calls;
mod lexical;
mod literals;

#[test_util::test]
fn argument_wrappers_and_alternate_literals_are_static() {
    for (name, source) in [
        ("app.ts", "t(<string>\"app.live\");"),
        ("app.py", "t(key=\"app.live\")"),
        ("app.kt", "fun main() { t(key = \"app.live\") }"),
        (
            "App.cs",
            "class App { void Run() { t(key: \"app.live\"); } }",
        ),
        ("app.lua", "t([[app.live]])"),
        ("app.rb", "t(%q{app.live})"),
        ("app.py", "t(\"app.\" \"live\")"),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert_eq!(
            references.literals,
            ["app.live".to_owned()].into(),
            "{name}"
        );
        assert!(references.dynamic.is_empty(), "{name}");
    }
}

#[test_util::test]
fn documentation_and_exports_do_not_hide_live_expression_children() {
    for (name, source) in [
        (
            "app.ts",
            "export type { Key } from 'app.dead'; export const value = t('app.live');",
        ),
        (
            "app.py",
            indoc::indoc! {r#"
            "app.dead" " docs"
            f"{t('app.live')}""#},
        ),
        (
            "app.rs",
            indoc::indoc! {r#"
                #[doc = "app.dead"]
                #[strum(serialize = "app.live")]
                struct Key;"#},
        ),
        (
            "app.ex",
            indoc::indoc! {r#"
            @doc "app.dead"
            t("app.live")"#},
        ),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert_eq!(
            references.literals,
            ["app.live".to_owned()].into(),
            "{name}"
        );
    }
}

#[test_util::test]
fn named_dynamic_arguments_cover_the_entire_computation() {
    let source = "fun main() { t(key = \"app.live\" + suffix) }";
    let references = scan(Path::new("app.kt"), source, &["t".into()])?;
    assert!(references.literals.contains("app.live"));
    assert_eq!(references.dynamic.len(), 1);
    assert_eq!(
        source.get(references.dynamic[0].span.clone()),
        Some("\"app.live\" + suffix")
    );
}

#[test_util::test]
fn concatenation_operators_follow_the_source_language() {
    for (source, prefix) in [
        ("<?php t(\"app.\" . $key);", Some("app.")),
        ("<?php t(\"app.\" <> $key);", None),
    ] {
        let references = scan(Path::new("app.php"), source, &["t".into()])?;
        assert!(references.literals.contains("app."));
        assert_eq!(references.dynamic.len(), usize::from(prefix.is_some()));
        if let Some(reference) = references.dynamic.first() {
            assert_eq!(reference.prefix.as_deref(), prefix);
        }
    }
}

#[test_util::test]
fn framework_expressions_decode_entities_and_use_their_own_grammar() {
    for (name, source, dynamic_text) in [
        (
            "component.vue",
            r#"<template><div v-for="item of items" :title="t(&quot;app.live&quot;)">{{ t(`app.${kind}`) }}</div></template>"#,
            "`app.${kind}`",
        ),
        (
            "component.vue",
            r#"<div :title="t(&#96;app.${kind}&#96;)" :aria-label="t('app.&#108;ive')"/>"#,
            "&#96;app.${kind}&#96;",
        ),
        (
            "component.html",
            r"<div>{{ 'app.live' | translate: params }} {{ `app.${kind}` | translate: { count: 1 } }}</div>",
            "`app.${kind}`",
        ),
        (
            "component.html",
            r#"<div [title]="t(&quot;app.live&quot;)">{{ `app.${kind}` | translate }}</div>"#,
            "`app.${kind}`",
        ),
    ] {
        let references = scan(Path::new(name), source, &["t".into(), "translate".into()])?;
        assert_eq!(
            references.literals,
            ["app.live".to_owned()].into(),
            "{name}"
        );
        assert_eq!(references.dynamic.len(), 1, "{name}");
        assert_eq!(
            source.get(references.dynamic[0].span.clone()),
            Some(dynamic_text),
            "{name}"
        );
    }
}

#[test_util::test]
fn exact_literals_count_independently_of_visible_string_construction() {
    let source = indoc::indoc! {r#"
        t("app.fragment" + suffix);
        t(`app.${kind}`);
        t("app.outer" + t("app.nested"));
        t(/* A note about the call. */ "app.literal");
        i18n.t?.("app.optional");
        (t)("app.parenthesized");
        t("app.\u0065scaped");
        t("app.\x68ex");
    "#};
    let references = scan(Path::new("app.ts"), source, &["t".into()])?;
    assert_eq!(references.dynamic.len(), 3);
    assert_eq!(
        references.literals,
        [
            "app.fragment",
            "app.outer",
            "app.nested",
            "app.literal",
            "app.optional",
            "app.parenthesized",
            "app.escaped",
            "app.hex"
        ]
        .map(str::to_owned)
        .into()
    );
}

#[test_util::test]
fn types_and_unrelated_templates_produce_no_references() {
    let source = indoc::indoc! {r#"
        // t("app.comment");
        type Key = `app.${string}`;
        type Literal = "app.type";
        interface Props { label: "app.interface"; }
        const unrelated = `app.${kind}`;
        expect(`app.${kind}`).toBe(result);
        const regex = /app.regex/;
    "#};
    let references = scan(Path::new("app.ts"), source, &["t".into()])?;
    assert!(references.literals.is_empty());
    assert!(references.dynamic.is_empty());
}

#[test_util::test]
fn injected_regions_exclude_data_scripts_css_and_vue_preformatted_content() {
    let source = indoc::indoc! {r#"
        <script type="application/ld+json">{"label": "app.data"}</script>
        <script setup lang="ts">
        type Key = `app.${string}`;
        const value = t("app.script");
        </script>
        <template>
          <div v-pre>{{ t("app.example") }}</div>
          <div :title="t('app.attribute')">{{ t("app.expression") }}</div>
        </template>
        <style lang="scss">.label { content: "app.style"; }</style>
    "#};
    let references = scan(Path::new("component.vue"), source, &["t".into()])?;
    assert_eq!(
        references.literals,
        ["app.script", "app.attribute", "app.expression"]
            .map(str::to_owned)
            .into()
    );
    assert!(references.dynamic.is_empty());
}

#[test_util::test]
fn angular_interpolations_with_surrounding_text_and_pipes_keep_original_spans() {
    let source =
        r"<p>Hello {{ 'app.literal' | translate }} and {{ `app.${kind}` | translate }}!</p>";
    let references = scan(Path::new("component.html"), source, &["translate".into()])?;
    assert!(references.literals.contains("app.literal"));
    assert_eq!(references.dynamic.len(), 1);
    assert_eq!(
        source.get(references.dynamic[0].span.clone()),
        Some("`app.${kind}`")
    );
}

#[test_util::test]
fn configured_callees_control_dynamic_recognition() {
    let source = "i18n.lookup(`app.${kind}`); unrelated.lookup(`app.${kind}`);";
    let references = scan(Path::new("app.ts"), source, &["i18n.lookup".into()])?;
    assert_eq!(references.dynamic.len(), 1);
    assert_eq!(references.dynamic[0].prefix.as_deref(), Some("app."));
}

#[test_util::test]
fn backend_interpolated_keys_are_dynamic() {
    for (name, source) in [
        ("app.py", "t(f\"app.{kind}\")"),
        ("app.rb", "t(\"app.#{kind}\")"),
        ("app.php", "<?php t(\"app.$kind\");"),
        ("app.kt", "fun main() { t(\"app.${kind}\") }"),
        ("app.swift", "let value = t(\"app.\\(kind)\")"),
        ("app.dart", "void main() { t(\"app.${kind}\"); }"),
        ("app.ex", "t(\"app.#{kind}\")"),
        ("App.cs", "class App { void Run() { t($\"app.{kind}\"); } }"),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert_eq!(references.dynamic.len(), 1, "{name}");
        assert_eq!(
            references.dynamic[0].prefix.as_deref(),
            Some("app."),
            "{name}"
        );
        assert!(
            references.literals.is_empty(),
            "{name}: {:?}",
            references.literals
        );
    }
}

#[test_util::test]
fn malformed_runtime_source_fails_instead_of_claiming_keys_are_unused() {
    assert!(scan(Path::new("app.ts"), "t(\"app.file\"", &["t".into()]).is_err());
}

const SOURCE_FIXTURES: &[(&str, &str)] = &[
    (
        "app.ts",
        include_str!("../../../../../../test-data/usages/app.ts"),
    ),
    (
        "react.tsx",
        include_str!("../../../../../../test-data/usages/react.tsx"),
    ),
    (
        "app.js",
        include_str!("../../../../../../test-data/usages/app.js"),
    ),
    (
        "app.rs",
        include_str!("../../../../../../test-data/usages/app.rs"),
    ),
    (
        "app.go",
        include_str!("../../../../../../test-data/usages/app.go"),
    ),
    (
        "app.py",
        include_str!("../../../../../../test-data/usages/app.py"),
    ),
    (
        "app.rb",
        include_str!("../../../../../../test-data/usages/app.rb"),
    ),
    (
        "app.php",
        include_str!("../../../../../../test-data/usages/app.php"),
    ),
    (
        "App.java",
        include_str!("../../../../../../test-data/usages/App.java"),
    ),
    (
        "app.kt",
        include_str!("../../../../../../test-data/usages/app.kt"),
    ),
    (
        "app.swift",
        include_str!("../../../../../../test-data/usages/app.swift"),
    ),
    (
        "app.dart",
        include_str!("../../../../../../test-data/usages/app.dart"),
    ),
    (
        "app.ex",
        include_str!("../../../../../../test-data/usages/app.ex"),
    ),
    (
        "app.lua",
        include_str!("../../../../../../test-data/usages/app.lua"),
    ),
    (
        "app.zig",
        include_str!("../../../../../../test-data/usages/app.zig"),
    ),
    (
        "App.cs",
        include_str!("../../../../../../test-data/usages/App.cs"),
    ),
    (
        "app.typ",
        include_str!("../../../../../../test-data/usages/app.typ"),
    ),
    (
        "component.vue",
        include_str!("../../../../../../test-data/usages/component.vue"),
    ),
    (
        "component.svelte",
        include_str!("../../../../../../test-data/usages/component.svelte"),
    ),
    (
        "component.astro",
        include_str!("../../../../../../test-data/usages/component.astro"),
    ),
    (
        "angular.html",
        include_str!("../../../../../../test-data/usages/angular.html"),
    ),
    (
        "page.html",
        include_str!("../../../../../../test-data/usages/page.html"),
    ),
];

#[test_util::test]
fn source_fixtures_keep_runtime_references_and_exclude_comments() {
    for &(name, source) in SOURCE_FIXTURES {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert!(
            references.literals.contains("app.live"),
            "{name}: {:?}",
            references.literals
        );
        for excluded in ["app.comment", "app.type", "app.style", "app.regex"] {
            assert!(
                !references.literals.contains(excluded),
                "{name}: {excluded}"
            );
        }
        if matches!(
            name,
            "component.vue" | "component.svelte" | "component.astro" | "angular.html"
        ) {
            assert!(
                references.literals.contains("app.attribute"),
                "{name}: {:?}",
                references.literals
            );
            if name != "angular.html" {
                assert!(
                    references.literals.contains("app.template"),
                    "{name}: {:?}",
                    references.literals
                );
            }
        }
        if matches!(
            name,
            "app.ts"
                | "app.js"
                | "react.tsx"
                | "component.vue"
                | "component.svelte"
                | "component.astro"
                | "angular.html"
                | "page.html"
        ) {
            assert_eq!(references.dynamic.len(), 1, "{name}");
            assert_eq!(
                references.dynamic[0].prefix.as_deref(),
                Some("app."),
                "{name}"
            );
            assert_eq!(
                source.get(references.dynamic[0].span.clone()),
                Some("`app.${kind}`"),
                "{name}"
            );
        }
        if matches!(
            name,
            "app.rs"
                | "app.go"
                | "app.py"
                | "app.rb"
                | "app.php"
                | "App.java"
                | "app.kt"
                | "app.swift"
                | "app.dart"
                | "app.ex"
                | "app.lua"
                | "app.zig"
                | "App.cs"
                | "app.typ"
        ) {
            assert!(
                references.dynamic.is_empty(),
                "{name}: a literal call was classified as dynamic"
            );
            let dynamic_source = source.replace("t(\"app.live\")", "t(key)");
            let dynamic = scan(Path::new(name), &dynamic_source, &["t".into()])?;
            assert!(dynamic.dynamic.is_empty(), "{name}");
        }
        if name == "app.rs" {
            assert!(references.identifiers.contains("AppVariant"));
            assert!(references.literals.contains("app.literal"));
        }
    }
}

#[test_util::test]
fn angular_pipes_classify_the_complete_input() {
    for (input, prefix) in [
        ("'app.live' + kind", Some("app.live")),
        ("'app.' + kind", Some("app.")),
        ("'app.live' | uppercase", None),
    ] {
        let source = format!("<p>{{{{ {input} | translate }}}}</p>");
        let references = scan(Path::new("app.html"), &source, &["translate".into()])?;
        assert_eq!(
            references.dynamic.len(),
            usize::from(prefix.is_some()),
            "{input}"
        );
        if let Some(reference) = references.dynamic.first() {
            assert_eq!(reference.prefix.as_deref(), prefix, "{input}");
            assert_eq!(source.get(reference.span.clone()), Some(input));
        }
    }
}

#[test_util::test]
fn framework_attribute_contexts_keep_executable_expressions() {
    for (name, source) in [
        (
            "app.html",
            r#"<p *ngFor="let item of items">{{ t('app.live') }}</p>"#,
        ),
        (
            "app.html",
            r#"<p *ngIf="items$ | async as items">{{ t('app.live') }}</p>"#,
        ),
        (
            "app.html",
            r#"<p (click)="doSomething(); t('app.live')"></p>"#,
        ),
        ("app.html", r#"<p title="{{ t('app.live') }}"></p>"#),
        ("app.html", r#"<p [title]="t(&quot;app.live&quot;)"></p>"#),
        (
            "component.astro",
            r#"<style define:vars={{ label: t("app.live") }}>.x {color: red}</style>"#,
        ),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert!(
            references.literals.contains("app.live"),
            "{source}: {:?}",
            references.literals
        );
        assert!(references.dynamic.is_empty(), "{source}");
    }
}

#[test_util::test]
fn framework_expression_contexts_cover_objects_blocks_and_nested_markup() {
    for (name, source) in [
        (
            "component.vue",
            r#"<div :class="{ active, 'is-disabled': disabled }">{{ t('app.live') }}</div>"#,
        ),
        (
            "component.svelte",
            "{#await promise then value}<p>{t('app.live')}</p>{/await}",
        ),
        (
            "component.svelte",
            "{#each rows as { id, name }, i}<p>{t('app.live')}</p>{/each}",
        ),
        (
            "component.svelte",
            "<Comp {...props} label={t('app.live')} />",
        ),
        (
            "component.svelte",
            "<p class={{ cool, lame: !cool }}>{t('app.live')}</p>",
        ),
        (
            "component.astro",
            "{items.map((item) => <li>{t('app.live')}</li>)}",
        ),
        (
            "component.astro",
            "<Comp {...props} label={t('app.live')} />",
        ),
        (
            "component.astro",
            "<script>const el = document.querySelector('#x') as HTMLElement; t('app.live');</script>",
        ),
        ("component.astro", "<p title=`${t('app.live')}`></p>"),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])
            .map_err(|error| std::io::Error::other(format!("{source}: {error}")))?;
        assert!(
            references.literals.contains("app.live"),
            "{source}: {:?}",
            references.literals
        );
        assert!(references.dynamic.is_empty(), "{source}");
    }
}

#[test_util::test]
fn complete_key_alternatives_remain_static() {
    for (name, source, dynamic) in [
        ("app.ts", "t(open ? 'app.live' : 'app.other');", false),
        ("app.ts", "t(['app.live', 'app.other']);", false),
        (
            "app.ts",
            "t(key ?? 'app.live'); t(key || 'app.other');",
            false,
        ),
        (
            "app.rs",
            "fn main() { t(if open { \"app.live\" } else { \"app.other\" }); }",
            false,
        ),
        (
            "app.rs",
            "fn main() { t(match open { true => \"app.live\", false => \"app.other\" }); }",
            false,
        ),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert!(references.literals.contains("app.live"), "{source}");
        assert!(references.literals.contains("app.other"), "{source}");
        assert_eq!(!references.dynamic.is_empty(), dynamic, "{source}");
    }
}

#[test_util::test]
fn php_static_and_nullsafe_calls_apply_dynamic_policy() {
    let source = r#"<?php Lang::get("app.$kind"); $translator?->translate("app.$kind");"#;
    let references = scan(
        Path::new("app.php"),
        source,
        &["get".into(), "translate".into()],
    )?;
    assert_eq!(references.dynamic.len(), 2);
    assert!(
        references
            .dynamic
            .iter()
            .all(|reference| reference.prefix.as_deref() == Some("app."))
    );
}

#[test_util::test]
fn framework_statement_and_binding_contexts_remain_valid() {
    for (name, source) in [
        (
            "component.vue",
            r#"<button @click="doSomething(); t('app.live')" />"#,
        ),
        (
            "component.svelte",
            "{@const label = t('app.live')}<p>{label}</p>",
        ),
        (
            "component.svelte",
            "{#snippet label(value)}<p>{t('app.live')}</p>{/snippet}",
        ),
        (
            "component.svelte",
            "{#await promise}<p>{t('app.live')}</p>{:then value}{value}{:catch error}{error}{/await}",
        ),
        (
            "app.html",
            "@let label = 'app.live' | translate;<p>{{ label }}</p>",
        ),
    ] {
        let references = scan(Path::new(name), source, &["t".into(), "translate".into()])
            .map_err(|error| std::io::Error::other(format!("{source}: {error}")))?;
        assert!(
            references.literals.contains("app.live"),
            "{source}: {:?}",
            references.literals
        );
    }
}

#[test_util::test]
fn malformed_attributes_report_original_source_locations() {
    let source = indoc::indoc! {r#"
        <template>
          <p>Heading</p>
          <p :title="t('app.live'" />
        </template>
    "#};
    let error = scan(Path::new("component.vue"), source, &["t".into()])
        .err()
        .ok_or_else(|| std::io::Error::other("malformed attribute was accepted"))?;
    assert!(error.to_string().contains("line 3,"), "{error}");
}

#[test_util::test]
fn svelte_each_keys_and_destructuring_defaults_are_runtime_expressions() {
    for source in [
        "{#each rows as row (t('app.live'))}<p>{row}</p>{/each}",
        "{#each rows as {label = t('app.live')}}<p>{label}</p>{/each}",
    ] {
        let references = scan(Path::new("component.svelte"), source, &["t".into()])?;
        assert!(references.literals.contains("app.live"), "{source}");
    }
}

#[test_util::test]
fn backend_conditional_literals_are_finite_choices() {
    for (name, source) in [
        ("app.py", "t('app.live' if open else 'app.other')"),
        (
            "app.kt",
            r#"fun main() { t(if (open) "app.live" else "app.other") }"#,
        ),
    ] {
        let references = scan(Path::new(name), source, &["t".into()])?;
        assert_eq!(
            references.literals,
            ["app.live".into(), "app.other".into()].into(),
            "{source}"
        );
        assert!(references.dynamic.is_empty(), "{source}");
    }
}

#[test_util::test]
fn vue_style_bindings_are_scanned_through_the_template_entry_point() {
    let source = indoc::indoc! {r#"
        <style scoped>
        /* content: v-bind('t("app.comment")'); */
        .label {
            content: v-bind('t("app.live")');
            color: "app.style";
        }
        </style>
    "#};
    let references = scan(Path::new("component.vue"), source, &["t".into()])?;
    assert_eq!(references.literals, ["app.live".into()].into());
    assert!(references.dynamic.is_empty());
}

#[test_util::test]
fn angular_pipes_and_member_calls_share_the_usage_policy() {
    for source in [
        "<p>{{ (open ? 'app.live' : 'app.other') | translate }}</p>",
        "<p>{{ (key ?? 'app.live') | translate }}{{ t('app.other') }}</p>",
    ] {
        let references = scan(
            Path::new("app.html"),
            source,
            &["t".into(), "translate".into()],
        )?;
        assert_eq!(
            references.literals,
            ["app.live".into(), "app.other".into()].into(),
            "{source}"
        );
        assert!(references.dynamic.is_empty(), "{source}");
    }
    let source = r#"<p [title]="translate.instant('app.' + kind)">{{ translate.instant('app.' + kind) }}</p>"#;
    let references = scan(Path::new("app.html"), source, &["translate.instant".into()])?;
    assert_eq!(references.dynamic.len(), 2);
    assert!(
        references
            .dynamic
            .iter()
            .all(|reference| reference.prefix.as_deref() == Some("app."))
    );
}

#[test_util::test]
fn astro_comment_only_and_empty_expressions_are_not_usages() {
    let source = indoc::indoc! {r"
        {/* t('app.comment') */}
        {}
        { // t('app.comment')
        }
        <p>{t('app.live')}</p>
    "};
    let references = scan(Path::new("component.astro"), source, &["t".into()])?;
    assert_eq!(references.literals, ["app.live".into()].into());
    assert!(references.dynamic.is_empty());
}

/// Typst calls count in markup and code mode, inside nested expressions and blocks.
#[test_util::test]
fn typst_calls_in_markup_and_code_are_static() {
    let source = indoc::indoc! {r#"
        = Title #t("app.heading")
        #t("app.markup") and #t("app.trailing")[body]
        #let label = t("app.let")
        #{
          let inner = t("app.block")
          if open { t("app.branch") } else { [#t("app.content")] }
        }
        #box(t("app.nested"))
        #t("app.arguments", name: user, count: 2)
        #t("app.outer", body: t("app.inner"))
        #t(count: 2, "app.named-first")
        #t(/* A note about the call. */ "app.commented")
        #i18n.t("app.member")
        #(t)("app.parenthesized")
        #t(("app.grouped"))
        #context t("app.context")
        #show heading: it => t("app.show")
        $ x = #t("app.math") $
        #t("app.\u{65}scaped")
    "#};
    let references = scan(Path::new("doc.typ"), source, &["t".into()])?;
    assert!(references.dynamic.is_empty(), "{:?}", references.literals);
    assert_eq!(
        references.literals,
        [
            "app.heading",
            "app.markup",
            "app.trailing",
            "app.let",
            "app.block",
            "app.branch",
            "app.content",
            "app.nested",
            "app.arguments",
            "app.outer",
            "app.inner",
            "app.named-first",
            "app.commented",
            "app.member",
            "app.parenthesized",
            "app.grouped",
            "app.context",
            "app.show",
            "app.math",
            "app.escaped",
        ]
        .map(str::to_owned)
        .into()
    );
}

/// Markup prose, comments, raw text, math text, and module paths are not Typst code literals.
#[test_util::test]
fn typst_text_comments_raw_and_paths_produce_no_references() {
    let source = indoc::indoc! {r#"
        #import "app.module": t
        #include "app.chapter"
        // #t("app.comment")
        /* #t("app.comment") */
        Prose quotes "app.markup" and mentions t("app.markup") without a hash.
        Inline raw `#t("app.raw")` stays verbatim.
        ```typ
        #t("app.raw")
        ```
        $ "app.math" $
        See @app.reference and <app.label>.
    "#};
    let references = scan(Path::new("doc.typ"), source, &["t".into()])?;
    assert!(references.literals.is_empty(), "{:?}", references.literals);
    assert!(references.dynamic.is_empty());
}

/// Visibly concatenated Typst keys trigger dynamic policy; opaque arguments do not.
#[test_util::test]
fn typst_concatenated_keys_are_dynamic_and_opaque_arguments_are_not() {
    let source = indoc::indoc! {r#"
        #t("app." + kind)
        #t(("app." + kind))
        #t(name: user, "app." + kind)
        #i18n.t("app." + kind, count: 1)
        #let tr(kind) = t("app." + kind)
        #t(prefix + ".title")
        #t(key)
        #t(keys.at(kind))
        #t(..args)
        #t[app.content]
        #other("app." + kind)
    "#};
    let references = scan(Path::new("doc.typ"), source, &["t".into()])?;

    // Five calls construct a key with a known prefix, and one has no literal prefix.
    let spans: Vec<_> = references
        .dynamic
        .iter()
        .map(|reference| {
            (
                source.get(reference.span.clone()),
                reference.prefix.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        spans,
        [
            (Some("\"app.\" + kind"), Some("app.")),
            (Some("\"app.\" + kind"), Some("app.")),
            (Some("\"app.\" + kind"), Some("app.")),
            (Some("\"app.\" + kind"), Some("app.")),
            (Some("\"app.\" + kind"), Some("app.")),
            (Some("prefix + \".title\""), None),
        ]
    );

    // Fragments still count as literal evidence, as in every other language.
    assert_eq!(references.literals, ["app.".into(), ".title".into()].into());
}

/// Dotted Typst function names match the complete member callee.
#[test_util::test]
fn typst_configured_callees_control_dynamic_recognition() {
    let source = r#"#i18n.lookup("app." + kind) #other.lookup("app." + kind)"#;
    let references = scan(Path::new("doc.typ"), source, &["i18n.lookup".into()])?;
    assert_eq!(references.dynamic.len(), 1);
    assert_eq!(references.dynamic[0].prefix.as_deref(), Some("app."));
}

#[test_util::test]
fn malformed_typst_code_fails_instead_of_claiming_keys_are_unused() {
    assert!(scan(Path::new("doc.typ"), "#t(\"app.file\"", &["t".into()]).is_err());
}

/// Template literals passed to unrelated calls or stored in objects are not translation calls.
#[test_util::test]
fn unrelated_templates_in_objects_and_calls_are_not_dynamic_usages() {
    let source = indoc::indoc! {r#"
        function sourceBase(field: Field, value: string) {
          return safeCreate(CreditContactSourceBaseSchema, {
            field,
            seenBase: safeCreate(CreditSeenSourceBaseSchema, {
              bindingId: `hypofy.contact.${CreditContactSourceBaseField[field].toLowerCase()}`,
              baseToken: `token-${value}`,
            }),
          });
        }
        function echoed(column: "name" | "email", token: string) {
          return { bindingId: `fincrm.contact.${column}`, baseToken: token };
        }
        const exists = ["jpg", "svg"].some(extension =>
          existsSync(`${PUBLIC_DIR}/images/document-examples/${name}.${extension}`)
        );
        card(`submissions.new-modal.step2.purpose.${choice.purpose}`).click();
    "#};
    let references = scan(Path::new("app.vitest.tsx"), source, &["t".into()])?;
    assert!(references.dynamic.is_empty());
    assert_eq!(references.calls, Vec::new());
}
