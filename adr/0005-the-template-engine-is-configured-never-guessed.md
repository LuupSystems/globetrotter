# 5. The template engine is configured, never guessed

**Status:** accepted · **Date:** 2026-09-05

## Context

Catalogs hold templates for one engine: Handlebars, Go templates, Mustache, Jinja2, or another one named in the config.
Only Handlebars has analysis support in globetrotter.

`lint` used to assume Handlebars when no engine was configured.
A catalog written for another engine then failed under the wrong parser, and the false errors drowned the real findings.

## Decision

Template checks run only for an explicitly configured engine that has analysis support.

- `template::Analyzer::for_engine` is the single place that maps an engine to its analysis, for validation and lint alike.
- Its match over `TemplateEngine` is exhaustive, so a new engine forces a decision there.
- Without a configured engine, lint skips template checks and emits one note pointing at `engine` and `--engine`.
- For an engine without analysis support, lint skips template checks and says so once.
- Validation reports a missing or unsupported engine once per catalog, not once per key.
- Every engine, including `Other`, is spelled by its name in configs and on the command line.

## Consequences

- A catalog for an unsupported engine gets structural checks without false template errors.
- Users who want template checks must set `engine`, and the note tells them so.
- Adding analysis for another engine means implementing it in `globetrotter-model/src/template` and returning it from `for_engine`.

## References

- `crates/globetrotter-model/src/template/mod.rs`, `Analyzer`
- `crates/globetrotter-model/src/lint.rs`, where lint resolves the analyzer
- Commit 2a550bb
