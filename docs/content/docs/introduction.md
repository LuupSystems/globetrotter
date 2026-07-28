---
title: Introduction
weight: 1
---

# Introduction

Globetrotter is a code generator for applications that share translations across languages or
frameworks. It reads TOML catalogs, validates the translations as one set, and writes:

- one JSON file for each spoken language, intended for runtime loading;
- TypeScript definitions for the translation structure;
- Rust types for translation keys and their declared arguments.

The translation text and the type information come from the same source files. A key cannot change
in one output without changing in all of them.

## Why split runtime data from generated types?

Translation text changes for editorial reasons. Application code changes for behavioral reasons.
Bundling both into generated source means a wording correction requires rebuilding the application;
keeping everything as untyped JSON means misspelled keys and incorrect template arguments survive
until runtime.

Globetrotter uses a narrower contract:

1. Keys and argument declarations are build-time facts.
2. The translated strings are runtime data.
3. Every generated target is derived from the same catalog.

This gives a TypeScript or Rust compiler enough information to check application code while keeping
the deployed translation files ordinary JSON.

## The source model

A translation file is a TOML table per key:

{{< example "quickstart/translations.toml" >}}

Each language is a field on the key. `arguments` describes the values used by the configured
template engine. In this example, `account.greeting` requires a string named `name`, while
`navigation.sign_out` has no arguments.

The config decides how those keys are named and which artifacts to write:

{{< example "quickstart/globetrotter.yaml" >}}

The `app` prefix makes the first key `app.account.greeting` in generated output. Paths are resolved
relative to the config file, so the same config behaves consistently from a developer shell, a
build script, and CI.

## Where it fits

Globetrotter works well when:

- several services or frontends share a translation contract;
- translations must remain editable or deployable independently;
- template placeholders should be checked before release;
- generated bindings are preferable to hand-maintained key constants.

It does not render translations for your application. Use the runtime template engine or i18n
library that fits your stack; Globetrotter prepares and validates the data and types it consumes.

Next: [install the CLI]({{< relref "installation.md" >}}) or go straight to the
[quick start]({{< relref "quick-start.md" >}}).
