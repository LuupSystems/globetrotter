# 3. Usage analysis is syntactic and counts only literal evidence

**Status:** accepted · **Date:** 2026-09-05

## Context

`lint` reports keys that no source references and checks translation calls against the catalog.
The first scanner was a single text sweep shared by every config.
Comments, type positions and unrelated templates counted as references, and a key one config had abandoned stayed alive because a sibling config's sources mentioned it.

A later version tried to prove keys dead.
It enumerated alternatives such as `t(open ? "a" : "b")`, reconciled Rust call arguments against generated variants, and suppressed literals nested in computed keys.
That machinery claimed more than a scanner without symbol resolution can establish.

## Decision

Usage analysis is syntactic, per config, and claims only what literal evidence supports.

- Each config owns the sources in its `usages.roots`, and its keys are matched only against them.
- Tree-sitter grammars, behind the `tree-sitter` feature, find runtime string literals and skip comments, documentation, types and regular expressions.
- Every exact key literal counts on its own, wherever it appears, including constants, lookup tables, ternary branches and arrays.
- Opaque arguments such as `t(key)` or `t(getKey(value))` contribute no usages and raise no diagnostic.
- Only visibly constructed keys, through interpolation or concatenation, fall under the `usages.dynamic` policy and the `dynamic-usage` lint.
- Keys without evidence are reported as potentially unused, never as dead.
- Static calls with a literal key are checked for `undefined-key` and `argument-mismatch`, and a call is reported only when no config that recognizes it accepts it.

The scanner deliberately does no symbol resolution, constant propagation or data-flow analysis.

## Consequences

- Results are predictable and explainable from the source text alone.
- A key reached only through an opaque value is reported as potentially unused and needs an `allow` entry or a literal reference.
- Because call checks are syntactic, they also cover code that never runs, such as an untaken Typst branch.
- Without the `tree-sitter` feature, a conservative text scan replaces the parsers, checks no calls, and says so.
- Supporting a new language means adding a grammar and its literal and call rules under `dead_keys/syntax`.

## References

- `crates/globetrotter/src/dead_keys/mod.rs`, `source.rs` and `call_checks.rs`
- `crates/globetrotter/src/dead_keys/syntax`
- `docs/content/docs/linting.md`, sections "Find unused keys" and "Check translation calls"
- Commits 9e77e58, 403e765 and 4196d05
