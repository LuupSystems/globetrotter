---
title: Documentation
bookToc: false
bookFlatSection: false
---

# Documentation

Globetrotter turns TOML translation catalogs into runtime JSON and generated language bindings. The
CLI also formats and lints the source catalogs, so one tool owns the full translation build step.

## Start here

- **[Introduction]({{< relref "introduction.md" >}})** — the design and when it is useful.
- **[Installation]({{< relref "installation.md" >}})** — install the `globetrotter` binary.
- **[Quick start]({{< relref "quick-start.md" >}})** — run the checked-in example and inspect its output.

## Use globetrotter

- **[Configuration]({{< relref "configuration.md" >}})** — config discovery, inputs, languages, and output paths.
- **[Translation files]({{< relref "translations.md" >}})** — catalog structure, arguments, and key prefixes.
- **[Generated outputs]({{< relref "outputs.md" >}})** — runtime JSON and generated bindings.
- **[Linting]({{< relref "linting.md" >}})** — built-in checks, source usage scanning, and LLM-assisted review.
- **[CLI reference]({{< relref "cli.md" >}})** — commands and shared options.
- **[Continuous integration]({{< relref "ci.md" >}})** — verify generated files and lint catalogs in CI.
- **[FAQ]({{< relref "faq.md" >}})** — common design and workflow questions.

> [!NOTE]
> The CLI and the version 1 YAML config are the primary user interfaces. The Rust crates expose the
> implementation for library users, but applications normally invoke the generator as a build step.
