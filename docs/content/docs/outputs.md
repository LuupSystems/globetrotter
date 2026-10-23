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

## Typst

Typst output writes one `.typ` module with the same translations as the JSON files plus the
argument types of every key. It carries data only: the
[Typst runtime package](#rendering-with-the-typst-runtime) imports it, validates calls, and renders
the templates. Configure one path or a list:

```yaml
version: 1
config:
  languages: [en, de]
  engine: handlebars
  inputs: [translations.toml]
  outputs:
    typst: generated/translations.typ
```

For a catalog with a `greeting` template and a `plain` literal, the module contains:

```typst
#let version = "1"

#let engine = "handlebars"

#let languages = ("en", "de")

#let arguments = (
  "greeting": ("name": "string"),
  "plain": (:),
)

#let translations = (
  "en": (
    "greeting": "Hello {{name}}!",
    "plain": "Hello world",
  ),
  "de": (
    "greeting": "Hallo {{name}}!",
    "plain": "Hallo Welt",
  ),
)
```

| Name | Value |
|---|---|
| `version` | Format version, shared with the JSON output. |
| `engine` | The configured template engine name, or `none`. |
| `languages` | Language codes in configured order. |
| `arguments` | Key to a dictionary of argument names and their catalog types: `any`, `string`, `number`, `integer`, `float`, `isodatetime`, or `boolean`. |
| `translations` | Language to a dictionary of key and source string, with the same keys as `arguments`. |

Keys and argument names are dictionary strings, so dots and dashes need no escaping. A key without
arguments is a literal that a runtime returns verbatim; every other key is a template for `engine`.
A runtime uses `arguments` to reject unknown keys and missing, unexpected, or mistyped arguments
before rendering, so a faulty call fails the document's compilation. Missing translations follow
the JSON output: they fail generation in strict mode and become placeholder text otherwise.

### From a Rust build script

The library's `typst` feature is enabled by default. A build dependency that disables default
features lists it explicitly:

```toml
[build-dependencies]
globetrotter = { version = "0", default-features = false, features = ["rust", "typst"] }
```

Add this output to the same `Outputs` builder used for Rust or JSON, then run the executor as in
[`examples/example-rust/build.rs`](https://github.com/LuupSystems/globetrotter/blob/main/examples/example-rust/build.rs):

```rust
let outputs = Outputs::new()
    .with_typst(globetrotter::typst::OutputConfig::new([
        out_dir.join("translations.typ"),
    ]));
let config = Config::new("pdf")
    .with_languages([Language::En, Language::De])
    .with_template_engine(TemplateEngine::Handlebars)
    .with_input(Input::new("translations.toml"))
    .with_outputs(outputs);
```

The module can be embedded with `include_str!(concat!(env!("OUT_DIR"), "/translations.typ"))`
and handed to an embedded Typst compiler at the path its document imports.

### Rendering with the Typst runtime

The repository ships a small pure-Typst package in
[`runtime/typst/globetrotter`](https://github.com/LuupSystems/globetrotter/tree/main/runtime/typst/globetrotter),
versioned with the module format it reads. It is not published to Typst Universe: vendor its
`lib.typ`, or copy the directory to `{data-dir}/typst/packages/local/globetrotter/0.1.0` and import
`@local/globetrotter:0.1.0`.

```typst
#import "@local/globetrotter:0.1.0": translator
#import "generated/translations.typ" as catalog

#let t = translator(catalog, "en", helpers: (
  pluralize: (word, count) => if count == 1 { word } else { word + "s" },
))

#t("greeting", name: "Ada")   // "Hello Ada!"
#t("cart.items", count: 3)    // "3 items" for "{{count}} {{pluralize \"item\" count}}"
```

`translator(module, lang, helpers: (:))` returns `t(key, ..arguments)`, which renders a `str`.
Arguments are named. A key without arguments returns its literal verbatim; a module generated with
`engine: none` renders only literals. Helpers are Typst functions that receive the evaluated
parameters positionally and return a `str`, number, boolean, or `none`; close over anything else
they need, such as the language.

Every faulty call panics and fails the document's compilation: an unknown language or key, a
positional value, a missing or unexpected argument, an argument of the wrong type, an unknown
helper, and any unsupported template construct. Messages name the key and the construct.

| Declared type | Accepted Typst types |
|---|---|
| `string`, `isodatetime` | `str` |
| `number`, `integer` | `int`, like the other generated bindings |
| `float` | `int` or `float` |
| `boolean` | `bool` |
| `any` | every type |

The runtime implements a Handlebars subset:

| Construct | Behavior |
|---|---|
| `{{path}}`, `{{{path}}}` | The value as plain text; neither form escapes HTML. |
| `{{[arg-name]}}`, `{{user.name}}` | Segment literals and dictionary fields of an argument. |
| `{{#if path}}…{{else}}…{{/if}}`, `{{#unless path}}…{{/unless}}` | Handlebars truthiness: `none`, `false`, zero, `""`, and `()` are false. |
| `{{! comment }}`, `{{!-- comment --}}` | Removed. |
| `{{name param …}}` | A registered helper, called with paths and string or number literals. |

Numbers render with an ASCII minus sign, booleans as `true` or `false`, and `none` as empty text.
`{{#each}}`, `{{else if}}`, partials, whitespace control, `this`, `../`, `@` data variables,
subexpressions, hash arguments, and `\{{` escapes panic. Keep catalogs that Typst documents
render within this subset; `check_templates` validates full Handlebars syntax, not the subset.

The [`examples/example-typst`](https://github.com/LuupSystems/globetrotter/tree/main/examples/example-typst)
directory generates a module, lints its document's usages, and renders it with a registered helper.

Run `task test:typst` to compile the generated fixture module, the runtime's construct and panic
tests, and the example. It needs the `typst` CLI on `PATH`, which `mise install` provides; normal
Rust tests do not invoke it.

### Usage scanning

`globetrotter lint` scans Typst documents like other sources. Calls such as
`#t("greeting", name: "Ada")` keep their keys alive, and `t("status." + kind)` follows the
configured dynamic policy. Every call with a literal key is also checked for an undefined key and
for named arguments that contradict the declaration, including calls in branches the document
never renders; `calls: deny` makes those findings errors. The default `functions` recognize `t`,
`$t`, and `translate`; list the translator's name when you bind it differently:

```yaml
usages:
  roots: [./document]
  functions: [tr]
  calls: deny
```

See [Linting]({{< relref "linting.md#static-and-dynamic-usages" >}}) for the recognized call shapes
and [call checks]({{< relref "linting.md#check-translation-calls" >}}) for the argument rules.

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
