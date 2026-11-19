# 1. Translation drift is judged by an LLM, not by embedding similarity

**Status:** accepted · **Date:** 2026-07-28 · **Supersedes:** the `globetrotter-semantic` crate and `lint --semantic`

## Context

`lint` checks a catalog's structure: missing and empty translations, broken templates, inconsistent placeholders and duplicates.
It cannot tell whether the languages of one key still say the same thing, for example after an English text changed and its German translation was not updated.

The first attempt, the `globetrotter-semantic` crate behind `lint --semantic`, compared cross-lingual embeddings and lexicons and reported a key when its similarity fell below a threshold.
It was unreliable on real catalogs for two structural reasons:

- A similarity threshold tuned for one embedding model does not transfer to another, so every model change needed retuning.
- An intentional locale adaptation, such as an idiom or a shortened label, scores like real drift, so better embeddings cannot separate the two.

An LLM that sees all languages of a key in one request scored 100% precision and recall on a labeled benchmark of 203 cases, with `gemma4:12b`, a strict prompt and medium reasoning effort.

## Decision

Semantic drift is reviewed by an LLM judge, `lint --llm-judge`, and no embedding or lexicon pipeline remains.

- The judge sends all languages of one key in one request to any OpenAI-compatible endpoint, a local Ollama by default.
- The response is constrained to a JSON schema derived from the `Verdict` type, with one lenient recovery attempt per key.
- Verdicts are cached on disk by content, so a rerun judges only changed keys.
- `--llm-min-confidence` filters after the cache, so tightening it sends no new requests.
- Findings are advisory notes carrying the model's reason and its self-reported confidence; they never fail `lint` on their own.
- `lint:llm-drift` in a key's `allow` list suppresses a reviewed, intentional divergence.

The judge is tuned for recall, so it may report false positives that a reviewer dismisses.

## Consequences

- A review needs a model endpoint and costs one request per changed key; `--max-keys` bounds a trial run on a slice of a large catalog.
- Results depend on the model: small models miss real meaning changes and invent problems in correct translations, so a model is evaluated against examples from the user's own catalog.
- The confidence value ranks findings; it is not a probability.
- The HTTP client stack is optional behind the `llm-judge` feature; see [ADR 4](0004-the-library-leaves-heavy-features-off-by-default.md).
- Revisit when a cheaper method matches the judge on a labeled benchmark of real catalogs.

## References

- `crates/globetrotter-llm-judge`: the client, prompt and verdict cache
- `crates/globetrotter/src/llm_judge.rs`: integration into `lint`
- `docs/content/docs/linting.md`, section "LLM-assisted drift review"
- Commit b74056d, which replaced the semantic pipeline
