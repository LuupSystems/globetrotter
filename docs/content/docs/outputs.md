---
title: Generated outputs
weight: 6
---

# Generated outputs

One run can write runtime JSON and compile-time bindings. All examples on this page are generated
from the checked-in [quick-start catalog]({{< relref "quick-start.md" >}}) before the documentation
site is built.

## Runtime JSON

Globetrotter writes one versioned JSON document per language. Its `translations` object maps fully
qualified keys to literal text or templates:

{{< example "quickstart/generated/translations_en.json" >}}

The key set is shared across language files, so an application can choose a file at runtime without
changing the lookup contract. Values without arguments remain literals; values with Handlebars
placeholders remain templates for the application to render.

The generated files contain data, not a runtime dependency on Globetrotter. Load them with the JSON
and templating libraries already used by your application.

## TypeScript

The TypeScript generator writes the structure and template argument contracts derived from the same
catalog:

{{< example "quickstart/generated/translations.ts" >}}

Import the generated definitions from application code, but keep the JSON as the source of runtime
text. This lets the compiler check keys and argument shapes without embedding every translation in
the JavaScript bundle.

## Rust

The Rust generator writes typed translation keys:

{{< example "quickstart/generated/translations.rs" >}}

The generated file can be included from a build script or committed at a stable source path. The
repository's [`examples/example-rust`](https://github.com/LuupSystems/globetrotter/tree/main/examples/example-rust)
shows a complete `build.rs` workflow.

## Commit or generate?

Both approaches are valid:

- **Commit generated files** when consumers should build without the generator installed. CI should
  regenerate them and fail on a diff.
- **Generate during the build** when every consumer already has a controlled build environment.
  Keep the build script deterministic and rerun it when catalogs change.

Do not mix ownership. If a file is generated, its header and repository workflow should make that
clear; application code should never patch it by hand.

## Preview without writing

Use a dry run to validate config, parse catalogs, and exercise generation without changing output
files:

```bash
globetrotter --dry-run
```

For large catalogs, `--max-keys N` provides a deliberately partial debugging run. Globetrotter emits
a warning when truncation is active so partial output cannot be mistaken for a full build.
