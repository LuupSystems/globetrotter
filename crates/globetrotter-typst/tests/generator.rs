//! Golden output and optional Typst compiler integration tests.

use color_eyre::eyre;
use globetrotter_model::{Language, TemplateEngine, Translations};
use similar_asserts::assert_eq as sim_assert_eq;
use std::process::Command;

fn catalog() -> eyre::Result<Translations> {
    Ok(Translations::from_str(
        include_str!("../../../test-data/typst/catalog.toml"),
        0,
        true,
        &mut vec![],
    )?)
}

fn module(languages: &[Language]) -> eyre::Result<String> {
    Ok(globetrotter_typst::generate_module(
        &catalog()?,
        Some(&TemplateEngine::Handlebars),
        languages,
        true,
    )?)
}

#[test_util::test]
fn golden_module() -> eyre::Result<()> {
    sim_assert_eq!(
        have: module(&[Language::En, Language::De])?,
        want: include_str!("../../../test-data/typst/translations.typ")
    );
    Ok(())
}

/// Missing translations follow the JSON output: an error when strict, else a placeholder.
#[test_util::test]
fn missing_translations_follow_strict_mode() -> eyre::Result<()> {
    let translations = catalog()?;
    let generate = |strict| {
        globetrotter_typst::generate_module(
            &translations,
            Some(&TemplateEngine::Handlebars),
            &[Language::Fr],
            strict,
        )
    };
    assert!(matches!(
        generate(true),
        Err(globetrotter_typst::Error::MissingKey { .. })
    ));
    assert!(generate(false)?.contains(r#""greeting": "missing translation greeting for Fr","#));
    Ok(())
}

/// Language order, engine, and empty catalogs map to well-formed Typst values.
#[test_util::test]
fn formats_languages_engine_and_empty_catalogs() -> eyre::Result<()> {
    // Configured languages keep their order.
    assert!(module(&[Language::De, Language::En])?.contains("#let languages = (\"de\", \"en\")"));

    // A one-element Typst array needs a trailing comma to stay an array.
    assert!(module(&[Language::En])?.contains("#let languages = (\"en\",)"));

    // Without an engine or translations, the module still declares every value.
    let empty = globetrotter_typst::generate_module(&Translations::default(), None, &[], true)?;
    assert!(empty.contains("#let engine = none\n"));
    assert!(empty.contains("#let languages = ()\n"));
    assert!(empty.contains("#let arguments = (:)\n"));
    assert!(empty.contains("#let translations = (:)\n"));
    Ok(())
}

/// Typst imports the generated module and reads every string back unchanged.
///
/// Opt in with `task test:typst`; ordinary Rust tests need no external compiler.
#[test_util::test]
#[ignore = "requires the typst CLI on PATH"]
fn typst_imports_the_module() -> eyre::Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    std::fs::write(
        root.join("translations.typ"),
        module(&[Language::En, Language::De])?,
    )?;
    std::fs::write(
        root.join("check.typ"),
        include_str!("../../../test-data/typst/check.typ"),
    )?;
    let result = Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(root)
        .arg(root.join("check.typ"))
        .arg(root.join("check.pdf"))
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}
