---
title: FAQ
weight: 10
---

# FAQ

## Does Globetrotter render translations at runtime?

No. It generates JSON data and language bindings. Your application loads the appropriate JSON file
and renders templates with its own Handlebars-compatible runtime or another integration.

## Why are languages fields in the same TOML table?

Keeping a key's translations together makes missing languages and placeholder differences visible
to both reviewers and the linter. Output is still split by language for runtime loading.

## Must generated files be committed?

No. Commit them when consumers need to build without the generator; otherwise generate them as part
of the build. In either case, establish one owner for the files and never edit them manually.

## Can one repository have several translation sets?

Yes. Add named entries under `configs`, or pass several config files with repeated `--config`
options. Each config has its own languages, inputs, settings, and outputs.

## Can I split a catalog across files?

Yes. `inputs` accepts several paths and glob patterns. Use `prefix`, `prepend_filename`, or
`prepend_relative_path` when file boundaries should become part of generated keys.

## How do I catch a missing translation?

List the required languages in the config and enable `strict: true`. Generation and lint then treat
a missing or empty language as an error.

## Are template arguments inferred?

Placeholders are discovered for validation, but their types are declared explicitly in
`arguments`. The declaration is what lets generated Rust and TypeScript bindings expose a stable
contract.

## What does `--dry-run` check?

It loads and validates configs, parses and combines inputs, validates templates when enabled, and
executes generation without writing outputs. Use it to inspect diagnostics; use
`globetrotter lint` for the broader translation-quality checks.

## Does the LLM judge replace human translation review?

No. It is an optional way to rank possible cross-language meaning drift. Findings are suggestions,
not proof, and small or poorly matched models can both miss real errors and report false ones.
Deterministic lint remains the required gate.

## Where should I report a problem?

Open an issue at [github.com/LuupSystems/globetrotter/issues](https://github.com/LuupSystems/globetrotter/issues)
with the smallest config and translation file that reproduce it. Include the CLI version and the
exact command.
