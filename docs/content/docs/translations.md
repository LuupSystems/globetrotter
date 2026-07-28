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

Supported types are:

| Type | Meaning |
|---|---|
| `any` | The caller may provide any value. |
| `string` | Text. |
| `number` | A numeric value. |
| `isodatetime` | An ISO 8601 date-time string. |

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

Every lint finding has a stable code. Suppress one code for a key only when the divergence is
intentional:

```toml
[product.proper_name]
en = "Globetrotter"
de = "Globetrotter"
allow = ["identical-languages"]
```

`allow = "all"` silences every lint for the key and should be rare; a specific code records the
reason more clearly and allows other checks to keep working.

Next: [generated outputs]({{< relref "outputs.md" >}}) and
[linting]({{< relref "linting.md" >}}).
