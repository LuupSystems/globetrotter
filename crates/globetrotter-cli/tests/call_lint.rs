//! CLI regressions for static translation-call checks: undefined keys and argument mismatches.

#![cfg(feature = "tree-sitter")]

use std::path::Path;
use std::process::{Command, Output};

fn lint(dir: &Path, args: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_globetrotter"))
        .current_dir(dir)
        .args(["lint", "--no-duplicates", "--color", "never", "-c"])
        .arg(dir.join("globetrotter.yaml"))
        .args(args)
        .output()
}

fn write(dir: &Path, name: &str, source: &str) -> std::io::Result<()> {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, source)
}

const CATALOG: &str = indoc::indoc! {r#"
    [title]
    en = "Title"

    [greeting]
    en = "Hello {{name}}"
    arguments = { name = "string" }
"#};

fn single_config(dir: &Path, usages: &str) -> std::io::Result<()> {
    write(dir, "catalog.toml", CATALOG)?;
    write(
        dir,
        "globetrotter.yaml",
        &indoc::formatdoc! {"
            version: 1
            config:
              languages: [en]
              engine: handlebars
              inputs: [catalog.toml]
              outputs: {{json: ['out/{{{{language}}}}.json']}}
              usages: {usages}
        "},
    )
}

/// Calls in code that never runs are checked, which Typst itself only does for evaluated calls.
#[test_util::test]
fn typst_calls_in_untaken_branches_are_reported() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    single_config(dir, "{roots: [src], calls: deny}")?;
    write(
        dir,
        "src/doc.typ",
        indoc::indoc! {r#"
            = #t("title")
            #if false {
              t("headline")
              t("greeting", name: 1)
            }
            #if false [#t("greeting", label: "Ada")]
        "#},
    )?;
    let output = lint(dir, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(!output.status.success(), "{stderr}");

    // The undefined key is reported at its own location.
    assert_eq!(
        stderr.matches("error[undefined-key]").count(),
        1,
        "{stderr}"
    );
    assert!(
        stderr.contains("translation key `headline` is not defined"),
        "{stderr}"
    );
    assert!(stderr.contains("doc.typ:3:5"), "{stderr}");

    // A mistyped value, an undeclared name, and the argument it leaves missing are mismatches.
    assert_eq!(
        stderr.matches("error[argument-mismatch]").count(),
        3,
        "{stderr}"
    );
    assert!(
        stderr.contains("argument `name` of translation key `greeting` must be `string`"),
        "{stderr}"
    );
    assert!(stderr.contains("doc.typ:4:17"), "{stderr}");
    assert!(
        stderr.contains("translation key `greeting` has no argument `label`"),
        "{stderr}"
    );
    assert!(
        stderr.contains("translation key `greeting` is missing argument `name`"),
        "{stderr}"
    );
    // Argument findings point at the key's declaration in the catalog.
    assert!(stderr.contains("key declared here"), "{stderr}");
    assert!(stderr.contains("catalog.toml:4:"), "{stderr}");
}

/// The policy is configurable per config and overridable on the command line.
#[test_util::test]
fn call_policy_levels_and_cli_override() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "src/app.ts",
        "t('title');\nt('greeting', { name: 'Ada' });\nt('missing');\n",
    )?;

    // The default policy warns, and warnings fail the lint like other findings.
    single_config(dir, "{roots: [src]}")?;
    let output = lint(dir, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(!output.status.success(), "{stderr}");
    assert_eq!(
        stderr.matches("warning[undefined-key]").count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("app.ts:3:3"), "{stderr}");

    // `allow` reports nothing.
    single_config(dir, "{roots: [src], calls: allow}")?;
    let output = lint(dir, &[])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // `deny` reports an error, and the CLI flag overrides the config in both directions.
    let output = lint(dir, &["--call-usages", "deny"])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(
        stderr.matches("error[undefined-key]").count(),
        1,
        "{stderr}"
    );
    single_config(dir, "{roots: [src], calls: deny}")?;
    let output = lint(dir, &["--call-usages", "allow"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // `--strict` promotes warnings, and a config-wide `allow` suppresses the code.
    single_config(dir, "{roots: [src]}")?;
    let output = lint(dir, &["--strict"])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(
        stderr.matches("error[undefined-key]").count(),
        1,
        "{stderr}"
    );
    write(
        dir,
        "globetrotter.yaml",
        indoc::indoc! {"
            version: 1
            config:
              languages: [en]
              engine: handlebars
              inputs: [catalog.toml]
              allow: [lint:undefined-key]
              usages: {roots: [src], calls: deny}
        "},
    )?;
    let output = lint(dir, &[])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A call is undefined only if no config that recognizes it defines its key.
#[test_util::test]
fn overlapping_configs_vouch_only_for_calls_they_recognize() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(dir, "alpha.toml", "[title]\nen = \"Alpha\"\n")?;
    write(dir, "beta.toml", "[title]\nen = \"Beta\"\n")?;
    write(
        dir,
        "shared/app.ts",
        indoc::indoc! {"
            t('alpha.title');
            t('beta.title');
            tr('alpha.title');
            t('missing');
        "},
    )?;
    write(
        dir,
        "globetrotter.yaml",
        indoc::indoc! {"
            version: 1
            configs:
              alpha:
                languages: [en]
                inputs: [{path: alpha.toml, prefix: alpha}]
                usages: {roots: [shared], functions: [t]}
              beta:
                languages: [en]
                inputs: [{path: beta.toml, prefix: beta}]
                usages: {roots: [shared], functions: [t, tr]}
        "},
    )?;
    let output = lint(dir, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(
        stderr.matches("warning[undefined-key]").count(),
        2,
        "{stderr}"
    );

    // Each config defines one of the `t` keys, so neither is reported.
    assert!(!stderr.contains("`beta.title` is not defined"), "{stderr}");

    // Only beta recognizes `tr`, and beta lacks the key that alpha defines.
    assert!(stderr.contains("app.ts:3:4"), "{stderr}");
    assert!(
        stderr.contains("translation key `alpha.title` is not defined"),
        "{stderr}"
    );

    // Both configs recognize the call with an undefined key and are named as its owners.
    assert!(stderr.contains("app.ts:4:3"), "{stderr}");
    assert!(
        stderr.contains("configs: alpha (") && stderr.contains(", beta ("),
        "{stderr}"
    );
}

/// TypeScript object literals and Python keyword arguments expose arguments statically.
#[test_util::test]
fn typescript_and_python_arguments_are_checked() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    single_config(dir, "{roots: [src]}")?;
    write(
        dir,
        "src/app.ts",
        indoc::indoc! {"
            t('greeting', { name: 'Ada' });
            t('greeting', { name });
            t('greeting', { ...values });
            t('greeting', values);
            t('greeting', { name: 1 });
            t('greeting');
        "},
    )?;
    write(
        dir,
        "src/app.py",
        indoc::indoc! {r#"
            t("greeting", name="Ada")
            t("greeting", **values)
            t("greeting", name=True)
            t("title", name="Ada")
        "#},
    )?;
    let output = lint(dir, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(
        stderr.matches("warning[argument-mismatch]").count(),
        4,
        "{stderr}"
    );
    assert!(stderr.contains("app.ts:5:17"), "{stderr}");
    assert!(stderr.contains("app.ts:6:3"), "{stderr}");
    assert!(stderr.contains("app.py:3:15"), "{stderr}");
    assert!(
        stderr.contains("translation key `title` has no argument `name`"),
        "{stderr}"
    );
    assert!(!stderr.contains("undefined-key"), "{stderr}");
}

/// A dynamic key's literal prefix must match some defined key.
#[test_util::test]
fn dynamic_prefixes_without_matching_keys_are_undefined() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        "[status.open]\nen = \"Open\"\n[status.paid]\nen = \"Paid\"\n",
    )?;
    write(
        dir,
        "globetrotter.yaml",
        indoc::indoc! {"
            version: 1
            config:
              languages: [en]
              inputs: [catalog.toml]
              usages: {roots: [src]}
        "},
    )?;
    write(
        dir,
        "src/doc.typ",
        "#t(\"status.\" + kind)\n#t(\"state.\" + kind)\n",
    )?;
    let output = lint(dir, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(
        stderr.matches("warning[undefined-key]").count(),
        1,
        "{stderr}"
    );
    assert!(
        stderr.contains("no translation key starts with `state.`"),
        "{stderr}"
    );
    assert!(stderr.contains("doc.typ:2:4"), "{stderr}");
}

/// A wrapper that supplies an argument makes it optional at call sites, but not untyped.
#[test_util::test]
fn implied_arguments_cover_wrappers_that_supply_them() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
            [greeting]
            en = "{{#if is_formal}}Good day{{else}}Hi{{/if}} {{name}}"
            arguments = { name = "string", is_formal = "boolean" }
        "#},
    )?;
    write(
        dir,
        "src/app.ts",
        indoc::indoc! {"
            // `t` wraps the translator and passes `is_formal` itself.
            t('greeting', { name: 'Ada' });
            t('greeting', { name: 'Ada', is_formal: true });
            t('greeting', { name: 'Ada', is_formal: 'yes' });
        "},
    )?;
    let config = |implied: &str| {
        write(
            dir,
            "globetrotter.yaml",
            &indoc::formatdoc! {"
                version: 1
                config:
                  languages: [en]
                  engine: handlebars
                  inputs: [catalog.toml]
                  outputs: {{json: ['out/{{{{language}}}}.json']}}
                  usages: {{roots: [src], implied_arguments: [{implied}]}}
            "},
        )
    };

    // Only the mistyped explicit value is reported.
    config("is_formal")?;
    let output = lint(dir, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(
        stderr.matches("warning[argument-mismatch]").count(),
        1,
        "{stderr}"
    );
    assert!(
        stderr.contains("argument `is_formal` of translation key `greeting` must be `boolean`"),
        "{stderr}"
    );
    assert!(stderr.contains("app.ts:4:30"), "{stderr}");

    // A misspelled implied argument implies nothing and is reported as a config warning.
    config("is_formel")?;
    let output = lint(dir, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("implied argument `is_formel` is not declared by any key"),
        "{stderr}"
    );
    assert_eq!(
        stderr.matches("warning[argument-mismatch]").count(),
        2,
        "{stderr}"
    );
}
