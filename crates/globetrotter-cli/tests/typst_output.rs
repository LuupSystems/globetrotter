//! Typst output follows the same path resolution and dry-run rules as other targets.

#![cfg(feature = "typst")]

use color_eyre::eyre;
use std::process::Command;

#[test_util::test]
fn generates_typst_from_yaml_and_honors_dry_run() -> eyre::Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    std::fs::create_dir(root.join("config"))?;
    std::fs::write(
        root.join("catalog.toml"),
        include_str!("../../../test-data/typst/catalog.toml"),
    )?;
    std::fs::write(
        root.join("config/globetrotter.yaml"),
        indoc::indoc! {"
            version: 1
            config:
              languages: [en, de]
              engine: handlebars
              inputs: [../catalog.toml]
              outputs:
                typst: [../generated/translations.typ, ../generated/copy.typ]
        "},
    )?;
    let run = |arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_globetrotter"))
            .current_dir(root)
            .args(["--config", "config/globetrotter.yaml", "--color", "never"])
            .args(arguments)
            .output()
    };

    // Dry runs create no output files.
    let result = run(&["--dry-run"])?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!root.join("generated").exists());
    let result = run(&[])?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let generated = std::fs::read_to_string(root.join("generated/translations.typ"))?;
    assert_eq!(
        generated,
        include_str!("../../../test-data/typst/translations.typ")
    );
    assert_eq!(
        generated,
        std::fs::read_to_string(root.join("generated/copy.typ"))?
    );
    Ok(())
}
