---
title: Translation files
weight: 5
---

# Translation files

Translation sources are TOML. Each table path is a translation key; its fields are spoken-language
codes and, optionally, an argument declaration.

{{< example "quickstart/translations.toml" >}}

## Keys

Nested TOML tables become dotted keys. This table:

```toml
[account.greeting]
en = "Welcome back"
de = "Willkommen zurück"
```

defines `account.greeting` before any input prefix is applied. In the quick-start config,
`prefix: app` makes the generated key `app.account.greeting`.

Keep keys about meaning rather than English wording. `navigation.sign_out` survives a copy edit;
`navigation.click_here` does not describe why the text exists.

## Languages

Language fields contain ordinary TOML strings, including multiline strings when the translation
needs line breaks:

```toml
[legal.notice]
en = """
Read the terms before continuing.
Changes take effect immediately.
"""
de = """
Lies die Bedingungen, bevor du fortfährst.
Änderungen gelten sofort.
"""
```

The config's `languages` list defines the expected set. With `strict: true`, a missing or empty
translation prevents generation. Without strict mode, the same condition is reported as a warning.

## Template arguments

Declare placeholders in an `arguments` table:

```toml
[account.greeting]
en = "Welcome back, {{name}}!"
de = "Willkommen zurück, {{name}}!"
arguments = { name = "string" }
```

Supported types, with the type each generator emits for them, are:

| Type | Meaning | Rust | TypeScript |
|---|---|---|---|
| `any` | The caller may provide any value. | `serde_json::Value` | `any` |
| `string` | Text. | `&str` | `string` |
| `number` | A numeric value, treated like `integer`. | `i64` | `number` |
| `integer` | A whole number. | `i64` | `number` |
| `float` | A number that may have a fractional part. | `f64` | `number` |
| `boolean` | A true or false value. | `bool` | `boolean` |
| `isodatetime` | An ISO 8601 date-time string. | `&str` | `string` |

The generated Rust enum derives only the comparison traits all of its fields support: a `float`
argument removes `Eq` and `Ord`, and an `any` argument removes `PartialOrd` and `Ord`.

An array shorthand declares untyped arguments:

```toml
arguments = ["name"]
```

Prefer the typed table when a useful type is known. Globetrotter checks that every declared
argument is used and that every Handlebars placeholder is declared and present in each language.

## Formatting

Format catalogs in place:

```bash
globetrotter format
```

By default keys are sorted ascending. Use `--order descending` to reverse the order, or verify
formatting without modifying files:

```bash
globetrotter format --check
```

The formatter preserves comments, so explanations for translators can stay beside the relevant
key.

## Local lint exceptions

Every lint finding has a stable code. Suppress one code for a key, only when the divergence is
intentional, by listing it with the `lint:` prefix:

```toml
[product.proper_name]
en = "Globetrotter"
de = "Globetrotter"
allow = ["lint:identical-languages"]
```

The prefix is required. A bare `identical-languages` is rejected rather than silently ignored, and
the namespace keeps the list open to non-lint directives later without a name ever meaning two
things.

An `allow` applies to the table it is written on and to every key nested under it, so a group or a
whole file can share one exception:

```toml
# Applies to every key in this file.
allow = ["lint:duplicate"]

[checkout]
# Applies to every key under `checkout`.
allow = ["lint:missing-language"]

[checkout.submit]
en = "Continue"
```

`allow = "lint:all"` silences every lint for the keys it covers and should be rare; a specific code
records the reason more clearly and allows other checks to keep working.

To suppress a code for a whole build rather than one file, use the config file's `allow` list — see
[configuration]({{< relref "configuration.md" >}}).

Next: [generated outputs]({{< relref "outputs.md" >}}) and
[linting]({{< relref "linting.md" >}}).
