---
title: Quick start
weight: 3
---

# Quick start

This example generates runtime translations for English, German, and French, plus TypeScript and
Rust bindings. Every file shown here belongs to the runnable fixture under `docs/examples/quickstart`.

## 1. Write the catalog

Create `translations.toml`:

{{< example "quickstart/translations.toml" >}}

The table path is the translation key. Language fields hold the text, and `arguments` declares the
placeholders accepted by the Handlebars template.

## 2. Configure the outputs

Create `globetrotter.yaml` beside it:

{{< example "quickstart/globetrotter.yaml" >}}

The config:

- enables English, German, and French;
- checks Handlebars templates while generating;
- prefixes every key with `app`;
- writes one JSON file per language;
- writes TypeScript and Rust bindings once.

## 3. Generate

Run the CLI from the directory containing the config:

{{< terminal "generate" >}}

With no `--config` flag, `globetrotter` discovers `globetrotter.yaml` in the current directory.
Passing `-c globetrotter.yaml` makes the example explicit.

## 4. Inspect a runtime file

The English JSON output is generated from the catalog above:

{{< example "quickstart/generated/translations_en.json" >}}

The German and French files have the same keys, which lets application code select a language at
runtime without changing its lookup contract.

## 5. Inspect the bindings

The same run writes TypeScript:

{{< example "quickstart/generated/translations.ts" >}}

and Rust:

{{< example "quickstart/generated/translations.rs" >}}

These are generated files; do not edit them by hand. Change the TOML catalog or YAML config and run
Globetrotter again.

## 6. Lint the sources

```bash
globetrotter lint
```

{{< terminal "lint" >}}

The lint command reads the same discovered config but never writes output. Add it to CI alongside a
check that committed generated files are current.

Next: [configuration]({{< relref "configuration.md" >}}) and
[translation files]({{< relref "translations.md" >}}).
