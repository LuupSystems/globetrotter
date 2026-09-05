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

Prefer the typed table when a useful type is known. With a configured `engine`, globetrotter checks
that every declared argument is used by at least one language, that every placeholder is declared,
and that a value substituted in one language is substituted in every language. An argument that only selects
a wording, as in `{{#if my_condition}}…{{else}}…{{/if}}`, may be left out of a language that has no
such distinction; a condition whose branches are identical is reported instead.

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

Ordinary `#` comments above a key or table move with it when sorting. A blank line after a
comment does not make it belong to the file.

### File-level comments

Use `#!` for explanations that belong to the whole catalog:

```toml
#! Advisor-facing names of the coarse credit document groups.
#! Keep them filesystem-friendly (no path separators).
#!
#! These names appear in the document picker.

[applicant]
en = "Applicant Documents"

# Shown when uploading property records.
[property]
en = "Property Documents"
```

The formatter collects every `#!` comment at the top of the file in its original source order,
followed by exactly one blank line. This holds even when a comment was written between tables,
inside an array, after a value or header, or at the end of the file. The header also stays in
place when new keys are added or the sort order changes.

Comment text is preserved verbatim, including spaces after `#!` and empty `#!` lines used for
paragraph breaks. Standalone comments retain their indentation; inline comments move to their own
lines starting with `#!`. Line endings are normalized to LF. `#!` inside a quoted key or a
translation string is ordinary text and is left untouched.

To migrate an existing file header, change each of its comment markers from `#` to `#!`, using
an empty `#!` line between paragraphs. There is no automatic migration or heuristic lint warning:
only the author can reliably distinguish a file header from a comment about the first key.
The syntax is valid TOML, so other parsers accept it. Older globetrotter versions treat it as an
ordinary comment and may move it during formatting.

### Section banners

An ordinary comment above a group stays with the following key, so sorting can move it into the
middle of that group. `#!` always belongs to the whole file; it does not anchor a section.
For groups that need a persistent banner, split them into separate translation files with a `#!`
header in each. The config's [input globs]({{< relref "configuration.md" >}}) can include all of
those files.

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
