//! Tests for the Typst rendering runtime in `runtime/typst/globetrotter`.
//!
//! The runtime tests compile Typst documents and are opt-in with `task test:typst`, because they
//! need the `typst` CLI on `PATH`.

use color_eyre::eyre;
use globetrotter_model::{Language, TemplateEngine, Translations};
use similar_asserts::assert_eq as sim_assert_eq;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod common;

const RUNTIME: &str = include_str!("../../../runtime/typst/globetrotter/lib.typ");
const FIXTURE_MODULE: &str =
    include_str!("../../../runtime/typst/globetrotter/tests/translations.typ");

/// Imports shared by every failing document.
const PREAMBLE: &str = r#"
#import "lib.typ": translator
#import "translations.typ" as catalog
#let helpers = (
  pluralize: (word, count) => word,
  join: (..values) => "",
  today: () => "",
)
#let t = translator(catalog, "en", helpers: helpers)
// Renders `source` as the only template of a module whose key `k` takes an `any` argument.
#let template(source) = translator(
  (
    version: "1",
    engine: "handlebars",
    languages: ("en",),
    arguments: (k: (x: "any")),
    translations: (en: (k: source)),
  ),
  "en",
)("k", x: (a: 1))
"#;

/// Documents that must fail to compile, with a fragment of the expected panic message.
const FAILURES: &[(&str, &str)] = &[
    // Module and language
    (
        r#"#translator(catalog, "fr")"#,
        r#"unknown language "fr", expected one of ("en", "de")"#,
    ),
    (
        r#"#translator((..dictionary(catalog), version: "2"), "en")"#,
        r#"module format version "2" is not supported"#,
    ),
    (
        r#"#translator((..dictionary(catalog), engine: "fluent"), "en")"#,
        r#"template engine "fluent" is not supported"#,
    ),
    (
        r#"#translator(catalog, "en", helpers: (pluralize: "s"))"#,
        r#"helper "pluralize" is not a function"#,
    ),
    // Keys and arguments
    (r#"#t("missing")"#, r#"unknown translation key "missing""#),
    (r"#t(1)", "unknown translation key 1"),
    (
        r#"#t("greeting")"#,
        r#"key "greeting": missing argument "name""#,
    ),
    (
        r#"#t("greeting", name: "Ada", extra: 1)"#,
        r#"key "greeting": unexpected argument "extra""#,
    ),
    (
        r#"#t("literal", extra: 1)"#,
        r#"key "literal": unexpected argument "extra""#,
    ),
    (
        r#"#t("greeting", "Ada")"#,
        r#"key "greeting": arguments must be named"#,
    ),
    // Argument types
    (
        r#"#t("greeting", name: 1)"#,
        r#"argument "name" must be string, found int"#,
    ),
    (
        r#"#t("greeting", name: [Ada])"#,
        r#"argument "name" must be string, found content"#,
    ),
    (
        r#"#t("cart.items", count: 1.5)"#,
        r#"argument "count" must be integer, found float"#,
    ),
    (
        r#"#t("choice", enabled: 1)"#,
        r#"argument "enabled" must be boolean, found int"#,
    ),
    (
        r#"#t("scalars", count: 1.5, total: 1, price: 1.5, flag: true, date: "", value: none)"#,
        r#"argument "count" must be number, found float"#,
    ),
    (
        r#"#t("scalars", count: 1, total: 1, price: "1", flag: true, date: "", value: none)"#,
        r#"argument "price" must be float, found str"#,
    ),
    (
        r#"#t("scalars", count: 1, total: 1, price: 1.5, flag: true, date: datetime.today(), value: none)"#,
        r#"argument "date" must be isodatetime, found datetime"#,
    ),
    (
        r#"#translator((..dictionary(catalog), arguments: (..catalog.arguments, greeting: (name: "text"))), "en")("greeting", name: "Ada")"#,
        r#"argument "name" has unsupported type "text""#,
    ),
    // Rendering
    (
        r#"#translator((..dictionary(catalog), engine: none), "en")("greeting", name: "Ada")"#,
        r#"key "greeting": declares arguments, but the module has no template engine"#,
    ),
    (
        r#"#translator(catalog, "en")("cart.items", count: 1)"#,
        r#"calls unknown helper "pluralize""#,
    ),
    (
        r#"#translator(catalog, "en", helpers: (pluralize: (..values) => [x]))("cart.items", count: 1)"#,
        "cannot render a value of type content",
    ),
    (
        r#"#t("field", user: (name: "Ada"))"#,
        r#"has no field "home town""#,
    ),
    (
        r#"#t("greeting", name: none)"#,
        r#"argument "name" must be string, found none"#,
    ),
    (
        r#"#template("{{x}}")"#,
        "cannot render a value of type dictionary",
    ),
    (
        r#"#template("{{y}}")"#,
        r#""{{y}}" references undeclared argument "y""#,
    ),
    // Unsupported or malformed templates
    (
        r#"#template("{{#each x}}{{/each}}")"#,
        r#"unsupported block "{{#each x}}""#,
    ),
    (
        r#"#template("{{#with x}}{{/with}}")"#,
        r#"unsupported block "{{#with x}}""#,
    ),
    (
        r#"#template("{{#if x}}a{{else if x}}b{{/if}}")"#,
        r#"unsupported construct "{{else if x}}""#,
    ),
    (
        r#"#template("{{> partial}}")"#,
        r#"unsupported construct "{{> partial}}""#,
    ),
    (
        r#"#template("{{~x}}")"#,
        r#"unsupported construct "{{~x}}""#,
    ),
    (
        r#"#template("{{x~}}")"#,
        r#"unsupported construct "{{x~}}""#,
    ),
    (
        r#"#template("{{@index}}")"#,
        r#"unsupported construct "{{@index}}""#,
    ),
    (
        r#"#template("{{^x}}a{{/x}}")"#,
        r#"unsupported construct "{{^x}}""#,
    ),
    (
        r#"#template("{{&x}}")"#,
        r#"unsupported construct "{{&x}}""#,
    ),
    (
        r#"#template("{{this}}")"#,
        r#"unsupported path in "{{this}}""#,
    ),
    (
        r#"#template("{{../x}}")"#,
        r#"unsupported path in "{{../x}}""#,
    ),
    (
        r#"#template("{{h x key=1}}")"#,
        r#"hash arguments are not supported in "{{h x key=1}}""#,
    ),
    (
        r#"#template("{{h (sub x)}}")"#,
        r#"unsupported path in "{{h (sub x)}}""#,
    ),
    (
        r#"#template("{{h \"x}}")"#,
        r#"malformed expression "{{h \"x}}""#,
    ),
    (
        r#"#template("\\{{x}}")"#,
        r#"escaped mustache "\{{" is not supported"#,
    ),
    (r#"#template("{{x")"#, r#"unterminated "{{" tag"#),
    (r#"#template("{{{x}}")"#, r#"unterminated "{{{" tag"#),
    (
        r#"#template("{{#if x}}a")"#,
        r#""{{#if x}}" has no matching "{{/if}}""#,
    ),
    (
        r#"#template("{{#if x}}a{{/unless}}")"#,
        r#""{{#if x}}" has no matching "{{/if}}""#,
    ),
    (
        r#"#template("{{#if x}}a{{else}}b{{else}}c{{/if}}")"#,
        r#""{{#if x}}" has no matching "{{/if}}""#,
    ),
    (r#"#template("{{/if}}")"#, r#"unexpected "{{/if}}""#),
    (r#"#template("{{else}}")"#, r#"unexpected "{{else}}""#),
];

fn package_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/typst/globetrotter")
}

fn compile(root: &Path, document: &Path) -> eyre::Result<Output> {
    Ok(Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(root)
        .arg(document)
        .arg(document.with_extension("pdf"))
        .output()?)
}

/// The committed fixture module matches what the generator produces for its catalog.
#[test_util::test]
fn fixture_module_is_current() -> eyre::Result<()> {
    let translations = Translations::from_str(
        include_str!("../../../runtime/typst/globetrotter/tests/catalog.toml"),
        0,
        true,
        &mut vec![],
    )?;
    let module = globetrotter_typst::generate_module(
        &translations,
        Some(&TemplateEngine::Handlebars),
        &[Language::En, Language::De],
        true,
    )?;
    sim_assert_eq!(have: module, want: common::with_current_version(FIXTURE_MODULE)?);
    Ok(())
}

/// Every supported construct renders as `tests/render.typ` asserts.
#[test_util::test]
#[ignore = "requires the typst CLI on PATH"]
fn runtime_renders_supported_constructs() -> eyre::Result<()> {
    let root = package_dir();
    let directory = tempfile::tempdir()?;
    let document = directory.path().join("render.typ");
    // Typst requires the main file inside the root, so the output goes to the temporary directory.
    let output = Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(&root)
        .arg(root.join("tests/render.typ"))
        .arg(document.with_extension("pdf"))
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Invalid calls and unsupported templates fail compilation with a message naming the cause.
#[test_util::test]
#[ignore = "requires the typst CLI on PATH"]
fn runtime_panics_on_invalid_calls_and_templates() -> eyre::Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    std::fs::write(root.join("lib.typ"), RUNTIME)?;
    std::fs::write(root.join("translations.typ"), FIXTURE_MODULE)?;

    let mut mismatches = Vec::new();
    for (index, &(body, expected)) in FAILURES.iter().enumerate() {
        let document = root.join(format!("case-{index}.typ"));
        std::fs::write(&document, format!("{PREAMBLE}{body}\n"))?;
        let output = compile(root, &document)?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() || !stderr.contains("panicked with") {
            mismatches.push(format!("{body}\n  compiled without a panic:\n{stderr}"));
        } else if !stderr.contains(expected) {
            mismatches.push(format!("{body}\n  expected {expected:?} in:\n{stderr}"));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n\n"));
    Ok(())
}
