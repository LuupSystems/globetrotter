# 4. The library leaves heavy features off by default, and the CLI turns them on

**Status:** accepted · **Date:** 2026-09-05

## Context

The `globetrotter` crate serves two kinds of users.
The `globetrotter` CLI wants every capability.
A library consumer, often a `build.rs` that generates bindings, wants a small dependency graph and a fast build.

Three features pull in large dependency trees: `rayon` brings a thread pool, `llm-judge` an HTTP client stack, and `tree-sitter` some twenty grammars.

## Decision

The `globetrotter` library's default features are only the output languages: `typescript`, `rust`, `golang`, `python` and `typst`.
`rayon`, `llm-judge` and `tree-sitter` are opt-in for library consumers.

`globetrotter-cli` depends on `globetrotter` with `default-features = false` and turns all of them on through its own default features, so the shipped binary has every capability.
`globetrotter` in turn depends on `globetrotter-model` with `default-features = false`, so the model's default `rayon` reaches it only through its own `rayon` feature.

The cargo-fc matrix treats the language features as mutually exclusive alternatives, and its `default` row still covers the shipped combination with every backend enabled.

## Consequences

- A build script that adds globetrotter compiles no thread pool, HTTP client or parser unless it asks for one.
- A CLI built with `--no-default-features` loses the LLM judge and falls back to text matching for usages, as [ADR 3](0003-usage-analysis-is-syntactic-and-counts-only-literal-evidence.md) describes.
- Every new heavy dependency needs the same decision: a library feature that is off by default, and a CLI feature that enables it.

## References

- `crates/globetrotter/Cargo.toml`, `[features]`
- `crates/globetrotter-cli/Cargo.toml`, `[features]` and the `globetrotter` dependency
- `crates/globetrotter-model/Cargo.toml`, `[features]`
- `docs/content/docs/linting.md`, section "Lightweight builds"
