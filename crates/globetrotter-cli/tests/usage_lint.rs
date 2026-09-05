//! CLI regressions verify config ownership, dynamic policy, and diagnostic locations.

use std::path::Path;
use std::process::{Command, Output};

fn lint(dir: &Path, config: &Path, args: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_globetrotter"))
        .current_dir(dir)
        .args(["lint", "--no-duplicates", "--color", "never", "-c"])
        .arg(config)
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

#[test_util::test]
fn roots_are_owned_by_configs_and_shared_findings_are_grouped() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
            [file]
            en = "File"
            [folder]
            en = "Folder"
            "#},
    )?;
    write(dir, "a/app.ts", "t('dialog.file');")?;
    write(dir, "b/app.ts", "const value = 1;")?;
    write(
        dir,
        "config/globetrotter.yaml",
        indoc::indoc! {"
        version: 1
        configs:
          alpha:
            languages: [en]
            inputs: [{path: ../catalog.toml, prefix: dialog}]
            usages:
              roots: [../a]
          beta:
            languages: [en]
            inputs: [{path: ../catalog.toml, prefix: dialog}]
            usages:
              roots: [../b]
    "},
    )?;
    let config = dir.join("config/globetrotter.yaml");
    let output = lint(dir, &config, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(!output.status.success(), "{stderr}");
    assert_eq!(stderr.matches("warning[unused-key]").count(), 2, "{stderr}");
    assert_eq!(
        stderr
            .matches("translation key `dialog.folder` is potentially unused")
            .count(),
        1,
        "{stderr}"
    );
    assert!(
        stderr.contains("configs: alpha (") && stderr.contains(", beta ("),
        "{stderr}"
    );
    assert!(stderr.contains("configs: beta ("), "{stderr}");

    // An explicit CLI root replaces both declared roots, relative to the cwd.
    write(
        dir,
        "override/app.ts",
        "t('dialog.file'); t('dialog.folder');",
    )?;
    let output = lint(dir, &config, &["--usages", "override"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test_util::test]
fn multiple_roots_include_shared_packages_and_preserve_resolved_keys() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
        [file]
        en = "File"
        "#},
    )?;
    write(dir, "a/app.ts", "const value = 1;")?;
    write(dir, "shared/app.ts", "t('alpha.file');")?;
    write(dir, "b/app.ts", "const value = 1;")?;
    write(
        dir,
        "globetrotter.yaml",
        indoc::indoc! {"
        version: 1
        configs:
          alpha:
            languages: [en]
            inputs: [{path: catalog.toml, prefix: alpha}]
            usages: {roots: [a, shared]}
          beta:
            languages: [en]
            inputs: [{path: catalog.toml, prefix: beta}]
            usages: {roots: [b]}
    "},
    )?;
    let output = lint(dir, &dir.join("globetrotter.yaml"), &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(stderr.matches("warning[unused-key]").count(), 1, "{stderr}");
    assert!(
        stderr.contains("translation key `beta.file` is potentially unused"),
        "{stderr}"
    );
    assert!(stderr.contains("configs: beta ("), "{stderr}");
}

#[test_util::test]
fn dynamic_policy_is_per_config_and_cli_can_override_it() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
            [file]
            en = "File"
            [folder]
            en = "Folder"
            "#},
    )?;
    write(
        dir,
        "src/app.ts",
        indoc::indoc! {r"
        t(`dialog.${kind}`);
        "},
    )?;
    write(
        dir,
        "globetrotter.yaml",
        indoc::indoc! {"
        version: 1
        configs:
          lenient:
            languages: [en]
            inputs: [{path: catalog.toml, prefix: dialog}]
            usages: {roots: [src], dynamic: allow}
          strict:
            languages: [en]
            inputs: [{path: catalog.toml, prefix: dialog}]
            usages: {roots: [src], dynamic: deny}
    "},
    )?;
    let config = dir.join("globetrotter.yaml");
    let output = lint(dir, &config, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(!output.status.success());
    assert_eq!(stderr.matches("warning[unused-key]").count(), 2, "{stderr}");
    assert_eq!(
        stderr.matches("error[dynamic-usage]").count(),
        1,
        "{stderr}"
    );
    assert!(stderr.contains("app.ts:1:"), "{stderr}");
    assert!(!stderr.contains("configs: lenient"), "{stderr}");

    let output = lint(dir, &config, &["--dynamic-usages", "warn"])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(!output.status.success());
    assert_eq!(
        stderr.matches("warning[dynamic-usage]").count(),
        1,
        "{stderr}"
    );
    assert!(!stderr.contains("warning[unused-key]"), "{stderr}");
    assert!(
        stderr.contains("configs: lenient (") && stderr.contains(", strict ("),
        "{stderr}"
    );

    let output = lint(dir, &config, &["--dynamic-usages", "allow"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test_util::test]
fn invalid_roots_and_policy_are_actionable_failures() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
        [file]
        en = "File"
        "#},
    )?;
    for usages in [
        "{roots: [missing]}",
        "{roots: [src], dynamic: denny}",
        "{rooots: [src]}",
    ] {
        write(
            dir,
            "globetrotter.yaml",
            &format!(
                indoc::indoc! {r"
                    version: 1
                    config:
                      languages: [en]
                      inputs: [catalog.toml]
                      usages: {usages}
                    "},
                usages = usages
            ),
        )?;
        let output = lint(dir, &dir.join("globetrotter.yaml"), &[])?;
        assert!(!output.status.success());
        assert!(!String::from_utf8(output.stderr)?.contains("warning[unused-key]"));
    }
}

#[test_util::test]
fn ignore_files_are_default_and_both_opt_outs_include_ignored_sources() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
            [plain]
            en = "Plain"
            [ignored]
            en = "Ignored"
            [hidden]
            en = "Hidden"
            [nested]
            en = "Nested"
            [local]
            en = "Local"
            "#},
    )?;
    write(
        dir,
        ".gitignore",
        indoc::indoc! {r"
            src/ignored/
            !src/dist/
            !src/dist/**
            "},
    )?;
    write(
        dir,
        ".ignore",
        indoc::indoc! {r"
        src/local.ts
        "},
    )?;
    write(dir, "src/dist/app.ts", "t('plain');")?;
    write(dir, "src/ignored/app.ts", "t('ignored');")?;
    write(dir, "src/.hidden/app.ts", "t('hidden');")?;
    write(
        dir,
        "src/nested/globetrotter.yaml",
        indoc::indoc! {r"
        version: 1
        "},
    )?;
    write(dir, "src/nested/app.ts", "t('nested');")?;
    write(dir, "src/local.ts", "t('local');")?;
    let config = indoc::indoc! {r"
        version: 1
        config:
          languages: [en]
          inputs: [catalog.toml]
          usages: {roots: [src]}
        "};
    write(dir, "globetrotter.yaml", config)?;
    let path = dir.join("globetrotter.yaml");
    let output = lint(dir, &path, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(stderr.matches("warning[unused-key]").count(), 2, "{stderr}");
    assert!(
        !stderr.contains("translation key `plain` is potentially unused"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("translation key `nested` is potentially unused"),
        "{stderr}"
    );
    for flag in ["--no-ignore", "--no-gitignore"] {
        let output = lint(dir, &path, &[flag])?;
        let stderr = String::from_utf8(output.stderr)?;
        assert_eq!(
            stderr.matches("warning[unused-key]").count(),
            1,
            "{flag}: {stderr}"
        );
        let expected = if flag == "--no-ignore" {
            "ignored"
        } else {
            "local"
        };
        assert!(
            stderr.contains(&format!(
                "translation key `{expected}` is potentially unused"
            )),
            "{stderr}"
        );
    }
    let output = lint(dir, &path, &["--no-ignore", "--no-gitignore"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    write(
        dir,
        "globetrotter.yaml",
        &config.replace(
            "roots: [src]",
            "roots: [src], respect_ignore_files: false, respect_gitignore: false",
        ),
    )?;
    let output = lint(dir, &path, &[])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(feature = "tree-sitter")]
#[test_util::test]
fn types_comments_and_unrelated_templates_do_not_keep_keys_alive() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
            [file]
            en = "File"
            [folder]
            en = "Folder"
            "#},
    )?;
    write(
        dir,
        "src/app.ts",
        indoc::indoc! {r#"
        // t("dialog.file");
        type Key = `dialog.${string}`;
        const unrelated = `dialog.${kind}`;
    "#},
    )?;
    write(
        dir,
        "globetrotter.yaml",
        indoc::indoc! {r"
            version: 1
            config:
              languages: [en]
              inputs: [{path: catalog.toml, prefix: dialog}]
              usages: {roots: [src]}
            "},
    )?;
    let output = lint(dir, &dir.join("globetrotter.yaml"), &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(stderr.matches("warning[unused-key]").count(), 2, "{stderr}");
}

#[cfg(all(feature = "tree-sitter", feature = "rust"))]
#[test_util::test]
fn generated_rust_variants_are_static_and_generated_files_do_not_hide_siblings() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
            [file]
            en = "File"
            [folder]
            en = "Folder"
            "#},
    )?;
    write(
        dir,
        "globetrotter.yaml",
        indoc::indoc! {"
        version: 1
        config:
          languages: [en]
          inputs: [{path: catalog.toml, prefix: dialog}]
          outputs: {rust: src/generated.rs}
          usages: {roots: [src], dynamic: deny}
    "},
    )?;
    let config = dir.join("globetrotter.yaml");
    let generated = Command::new(env!("CARGO_BIN_EXE_globetrotter"))
        .current_dir(dir)
        .arg("-c")
        .arg(&config)
        .output()?;
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    for source in [
        "fn main() { t(Translation::DialogFile {}); }",
        "fn main() { t(&Translation::DialogFile {}); }",
        "use Translation::DialogFile; fn main() { t(DialogFile {}); }",
        "fn main() { let value = Translation::DialogFile {}; }",
    ] {
        write(
            dir,
            "src/main.rs",
            &format!(
                indoc::indoc! {r"
                // Translation::DialogFolder
                {source}"},
                source = source
            ),
        )?;
        let output = lint(dir, &config, &[])?;
        let stderr = String::from_utf8(output.stderr)?;
        assert_eq!(
            stderr.matches("warning[unused-key]").count(),
            1,
            "{source}: {stderr}"
        );
        assert!(
            stderr.contains("translation key `dialog.folder` is potentially unused"),
            "{stderr}"
        );
        assert!(!stderr.contains("dynamic-usage"), "{stderr}");
    }
    // A typed variant remains required for compilation inside a computed argument.
    write(
        dir,
        "src/main.rs",
        "fn main() { t(ComputedKey { base: Translation::DialogFile {}, suffix }); }",
    )?;
    let output = lint(dir, &config, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(stderr.matches("warning[unused-key]").count(), 1, "{stderr}");
    assert!(
        stderr.contains("translation key `dialog.folder` is potentially unused"),
        "{stderr}"
    );
    assert!(!stderr.contains("dynamic-usage"), "{stderr}");
    write(
        dir,
        "src/main.rs",
        "fn main() { t(ComputedKey { prefix: \"dialog.file\", suffix }); }",
    )?;
    let output = lint(dir, &config, &[])?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(stderr.matches("warning[unused-key]").count(), 1, "{stderr}");
    assert!(!stderr.contains("dynamic-usage"), "{stderr}");
}

#[cfg(not(feature = "tree-sitter"))]
#[test_util::test]
fn lightweight_builds_report_the_text_matching_limitation() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
        [file]
        en = "File"
        "#},
    )?;
    write(dir, "src/app.ts", "// t('file');")?;
    let output = Command::new(env!("CARGO_BIN_EXE_globetrotter"))
        .current_dir(dir)
        .args([
            "lint",
            "--no-duplicates",
            "--color",
            "never",
            "--translation",
            "catalog.toml",
            "--usages",
            "src",
        ])
        .output()?;
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stderr)?.contains("this build lacks the `tree-sitter` feature")
    );
}

#[cfg(feature = "tree-sitter")]
#[test_util::test]
fn opaque_calls_preserve_lexical_evidence_without_hiding_unused_keys() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
        [title]
        en = "Title"
        [unused]
        en = "Unused"
    "#},
    )?;
    write(
        dir,
        "src/app.ts",
        indoc::indoc! {r#"
        const key: TranslationKey = "account.title";
        t(key);
        t?.(key);
        object.t(key);
        t(keys[kind]);
        props.t(getKey(value));
        const constructed = `account.${section}`;
        t(constructed);
    "#},
    )?;
    write(
        dir,
        "globetrotter.yaml",
        indoc::indoc! {"
        version: 1
        config:
          languages: [en]
          inputs: [{path: catalog.toml, prefix: account}]
          usages: {roots: [src]}
    "},
    )?;
    for policy in ["allow", "warn", "deny"] {
        let output = lint(
            dir,
            &dir.join("globetrotter.yaml"),
            &["--dynamic-usages", policy],
        )?;
        let stderr = String::from_utf8(output.stderr)?;
        assert_eq!(
            stderr.matches("warning[unused-key]").count(),
            1,
            "{policy}: {stderr}"
        );
        assert!(
            stderr.contains("translation key `account.unused` is potentially unused"),
            "{stderr}"
        );
        assert!(!stderr.contains("dynamic-usage"), "{policy}: {stderr}");
    }
}

#[cfg(feature = "tree-sitter")]
#[test_util::test]
fn unknown_dynamic_prefixes_never_mark_the_whole_catalog_used() {
    let temp = tempfile::tempdir()?;
    let dir = temp.path();
    write(
        dir,
        "catalog.toml",
        indoc::indoc! {r#"
        [title]
        en = "Title"
        [other]
        en = "Other"
    "#},
    )?;
    write(dir, "src/app.ts", "t(`${kind}`);")?;
    write(
        dir,
        "globetrotter.yaml",
        indoc::indoc! {"
        version: 1
        config:
          languages: [en]
          inputs: [{path: catalog.toml, prefix: account}]
          usages: {roots: [src]}
    "},
    )?;
    for policy in ["allow", "warn", "deny"] {
        let output = lint(
            dir,
            &dir.join("globetrotter.yaml"),
            &["--dynamic-usages", policy],
        )?;
        let stderr = String::from_utf8(output.stderr)?;
        assert_eq!(
            stderr.matches("warning[unused-key]").count(),
            2,
            "{policy}: {stderr}"
        );
        assert_eq!(
            stderr.matches("[dynamic-usage]").count(),
            usize::from(policy != "allow"),
            "{policy}: {stderr}"
        );
    }
}
