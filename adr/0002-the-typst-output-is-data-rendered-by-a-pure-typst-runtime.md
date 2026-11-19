# 2. The Typst output is data, rendered and checked by a pure-Typst runtime

**Status:** accepted · **Date:** 2026-10-23

## Context

Typst documents need translations at compile time, whether the Typst CLI compiles them or a program embeds the Typst compiler.
The other outputs split into typed bindings for a host language and JSON files that a runtime renders.
Nothing outside the document checks a Typst call against the catalog, so a wrong key or argument has to fail the document's compilation itself.

## Decision

The `typst` output writes one `.typ` module that carries data only, and a separate pure-Typst package renders it.

- The module exports `version`, `engine`, `languages`, `arguments` and `translations`.
- Templates stay unrendered source strings, as in the JSON output, and `version` is the JSON output's format version.
- The runtime package in `runtime/typst/globetrotter` implements a minimal Handlebars subset: paths, `if`, `unless`, comments and registered helpers.
- Every invalid call panics: an unknown key, a positional value, a missing, unexpected or mistyped argument, and any construct outside the subset, such as `{{#each}}` or partials.
- The runtime rejects a module whose format version it does not read.

The runtime is versioned by the module format it reads, not by the globetrotter release, and it is not published to Typst Universe.

## Consequences

- A faulty translation call fails `typst compile` with a message naming the key, so a document never renders a broken string.
- Templates that need Handlebars features beyond the subset cannot be rendered in Typst; the panic names the unsupported construct.
- The runtime reimplements part of Handlebars in Typst, so it must render the supported constructs as Handlebars does.
- A change to the module's shape needs a new format version, which the runtime checks.
- Users vendor `lib.typ` or copy the package into their local Typst package directory.

## References

- `crates/globetrotter-typst/src/lib.rs`, `generate_module`, and its `header.typ`
- `runtime/typst/globetrotter/lib.typ` and its `README.md`
- `runtime/typst/globetrotter/tests`, compiled by `task test:typst`
- `examples/example-typst`, compiled by `task example:typst`
- `docs/content/docs/outputs.md`, section "Typst"
