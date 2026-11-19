# Agent guidelines

Project-specific instructions for coding agents working on globetrotter.

## Architecture decision records

Before changing an area that a decision governs, read its architecture decision record in `adr/`:

- [ADR 1](adr/0001-translation-drift-is-judged-by-an-llm-not-by-embedding-similarity.md): semantic drift review through an LLM judge
- [ADR 2](adr/0002-the-typst-output-is-data-rendered-by-a-pure-typst-runtime.md): the Typst output and its runtime package
- [ADR 3](adr/0003-usage-analysis-is-syntactic-and-counts-only-literal-evidence.md): unused-key detection and call checks
- [ADR 4](adr/0004-the-library-leaves-heavy-features-off-by-default.md): default features of the library and the CLI
- [ADR 5](adr/0005-the-template-engine-is-configured-never-guessed.md): template engine resolution

A change that alters a recorded decision adds a new ADR that supersedes the old one and sets the old one's status to `superseded by [ADR N](NNNN-….md)`.
An accepted ADR is otherwise a record: fix its facts and code pointers, never its decision.

## Layout

- `crates/globetrotter-model`: the catalog model, TOML parsing, validation, template analysis and catalog lints.
- `crates/globetrotter`: the library that runs a config, writes outputs and scans source usages (`dead_keys/`).
- `crates/globetrotter-cli`: the `globetrotter` binary.
- `crates/globetrotter-<language>`: one output generator per language.
- `crates/globetrotter-llm-judge`: the LLM client behind `lint --llm-judge`.
- `crates/test-util`: the `#[test_util::test]` attribute every test uses.
- `runtime/typst/globetrotter`: the pure-Typst rendering package.
- `docs/`: the Hugo documentation site.

## Commands

Run the tasks in `taskfile.yaml` rather than the cargo commands behind them; they carry the flags CI uses.

- `task test:fc` and `task lint:fc` are the gates CI runs, across every feature combination and, for lint, every target.
- `task test` and `task check` are the fast inner loop.
- `task test:typst` compiles the Typst runtime tests and the Typst example and needs `typst` from `mise.toml`.
- `task format` formats the workspace.
- `task docs:terminals` regenerates the docs examples and terminal snippets after a change to CLI output.

## Tests

Mark every test `#[test_util::test]`, not `#[test]` or `#[tokio::test]`.
The attribute installs `color_eyre` and lets the body use `?`.
Import `use color_eyre::eyre;` once per test module and qualify `eyre::Result`, `eyre::eyre!` and `eyre::bail!` through it.
Propagate fallible setup with `?`; `clippy.toml` keeps `unwrap` and `expect` denied in tests.

Golden tests compare generated output byte for byte with checked-in fixtures.
`.gitattributes` keeps LF line endings in every checkout for that reason.

## Configuration schema

`globetrotter.schema.json` is maintained by hand and rejects unknown keys.
Add every new config key, alias or enumeration value to it in the same change.

## Multiline strings

Use `indoc!` for multiline string literals and `formatdoc!` when they interpolate values.
