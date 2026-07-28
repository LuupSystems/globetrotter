---
title: globetrotter
type: docs
bookToc: false
---

<div class="gt-hero">
  <div class="gt-hero__text">
    <h1>globetrotter</h1>
    <p class="gt-hero__lead">Generate runtime translations and type-safe bindings for a polyglot application from one set of TOML translation files.</p>
    <div class="gt-hero__cmd">globetrotter</div>
    <div class="gt-hero__actions">
      <a class="gt-btn gt-btn--primary" href="{{< relref "/docs/introduction.md" >}}">Read the docs</a>
      <a class="gt-btn" href="https://github.com/LuupSystems/globetrotter">Source on GitHub</a>
    </div>
  </div>
  <div class="gt-hero__terminal">
    {{< terminal "generate" >}}
  </div>
</div>

<div class="gt-badges">

[![build status](https://img.shields.io/github/actions/workflow/status/LuupSystems/globetrotter/build.yaml?branch=main&label=build)](https://github.com/LuupSystems/globetrotter/actions/workflows/build.yaml)
[![test status](https://img.shields.io/github/actions/workflow/status/LuupSystems/globetrotter/test.yaml?branch=main&label=test)](https://github.com/LuupSystems/globetrotter/actions/workflows/test.yaml)
[![crates.io](https://img.shields.io/crates/v/globetrotter-cli)](https://crates.io/crates/globetrotter-cli)
[![docs.rs](https://img.shields.io/docsrs/globetrotter/latest?label=docs.rs)](https://docs.rs/globetrotter)

</div>

Globetrotter keeps the two parts of internationalization separate. Translation text stays in
language-specific JSON that an application can load at runtime. Translation keys and template
arguments become generated source code, so compilers and editors can catch mistakes before the
application starts.

<div class="gt-cards">
  <div class="gt-card">
    <h3>One catalog</h3>
    <p>Keep every language and each template argument contract together in readable TOML files.</p>
  </div>
  <div class="gt-card">
    <h3>Runtime JSON</h3>
    <p>Generate one versioned translation file per language and deploy text independently from application code.</p>
  </div>
  <div class="gt-card">
    <h3>Typed bindings</h3>
    <p>Generate Rust and TypeScript definitions from the same keys and argument declarations.</p>
  </div>
  <div class="gt-card">
    <h3>Translation linting</h3>
    <p>Find missing text, broken templates, argument mismatches, duplicate strings, and unused keys.</p>
  </div>
</div>

## A complete input

This catalog is part of the documentation's runnable example:

{{< example "quickstart/translations.toml" >}}

Its config generates JSON for three languages plus Rust and TypeScript bindings:

{{< example "quickstart/globetrotter.yaml" >}}

The documentation build runs that exact config with the current source tree before Hugo renders the
site. The [quick start]({{< relref "/docs/quick-start.md" >}}) shows the resulting files.

## Get started

```bash
brew install --cask LuupSystems/tap/globetrotter
globetrotter
```

Globetrotter discovers `globetrotter.yaml` in the current directory. Start with
[Installation]({{< relref "/docs/installation.md" >}}), then follow the
[Quick start]({{< relref "/docs/quick-start.md" >}}).

## Documentation

- [Introduction]({{< relref "/docs/introduction.md" >}}) — the runtime-data and generated-types model.
- [Configuration]({{< relref "/docs/configuration.md" >}}) — inputs, languages, settings, and outputs.
- [Translation files]({{< relref "/docs/translations.md" >}}) — keys, languages, arguments, and namespacing.
- [Generated outputs]({{< relref "/docs/outputs.md" >}}) — JSON, TypeScript, and Rust.
- [Linting]({{< relref "/docs/linting.md" >}}) — deterministic checks and the optional LLM review.
