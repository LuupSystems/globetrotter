---
title: Linting
weight: 7
---

# Linting

`globetrotter lint` validates translation sources without writing generated files:

{{< terminal "lint" >}}

The deterministic checks report:

- a missing required language;
- empty text or leading/trailing whitespace;
- a Handlebars template that does not compile;
- a value substituted in some languages but not in others; a name that only selects a wording,
  such as `{{#if my_condition}}`, may be absent from a language without that distinction;
- a condition whose branches are identical, so it has no effect;
- placeholders that are undeclared, or arguments that are never used;
- identical translations within one key;
- duplicate text shared by different keys.

Findings are warnings by default and errors when strict mode is active in the config or on the
command line.

Every finding carries the code shown in brackets, as in `warning[duplicate]: …`. Suppress one by
listing it with the `lint:` prefix in an `allow` list — on the key, on an enclosing table, or on the
whole config. See [local lint exceptions]({{< relref "translations.md" >}}) for the scoping rules.

## Find unused keys

Pass one or more source directories to report translation keys that application code never
references:

```bash
globetrotter lint --usages ./src --usages ./packages
```

The scanner follows ignore files and treats dynamic key prefixes conservatively. It is a cleanup
tool, not a proof that every runtime-computed lookup is dead; review unused-key findings before
deleting translations.

Disable duplicate detection for a run with `--no-duplicates`. For a deliberate exception on one
key, prefer its local `allow` list:

```toml
allow = ["lint:duplicate"]
```

## LLM-assisted drift review

`--llm-judge` adds an experimental semantic review. It sends all languages for one key to an
OpenAI-compatible endpoint and asks whether they tell the user the same thing:

```bash
# Local Ollama endpoint and the default model.
globetrotter lint --llm-judge

# A hosted or otherwise compatible endpoint.
globetrotter lint \
  --llm-judge \
  --llm-base-url https://api.example.com/v1 \
  --llm-model my-model \
  --llm-api-key-env MY_API_KEY
```

This review is advisory. Model findings are emitted as notes and never make lint fail by
themselves; inspect the reason and the translations. The judge is deliberately tuned for recall,
so false positives are expected.

Verdicts are cached by content. A rerun only judges changed keys, and changing
`--llm-min-confidence` re-filters cached findings without sending new requests. Use `--max-keys 25`
to evaluate a model or prompt on a bounded slice before reviewing a large catalog.

Model choice matters. Small models can miss genuine meaning changes while inventing problems in
correct translations. Evaluate a model against examples from your own catalog, keep temperature at
the reproducible default, and treat reported confidence as a ranking rather than a probability.

Suppress a reviewed, intentional divergence with:

```toml
allow = ["lint:llm-drift"]
```

The [CLI reference]({{< relref "cli.md" >}}) lists the endpoint, prompt, concurrency, effort, cache,
and confidence controls.
