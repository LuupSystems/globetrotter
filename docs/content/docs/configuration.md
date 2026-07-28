---
title: Configuration
weight: 4
---

# Configuration

Globetrotter uses a versioned YAML file. By default it looks for `globetrotter.yaml` in the current
directory; pass `--config` (`-c`) to name a file or a directory to search.

{{< example "quickstart/globetrotter.yaml" >}}

Paths and glob patterns are resolved relative to the config file, not the shell's working
directory. That makes a checked-in config safe to invoke from a repository root, a nested build
script, or CI.

## Top-level structure

```yaml
version: 1
configs:
  app:
    # one independent translation build
```

`version` is required. `configs` is a mapping of named builds. A project can use one config for the
whole application or separate configs for independently deployed packages:

```yaml
version: 1
configs:
  web:
    # ...
  api:
    # ...
```

The name appears in progress and diagnostics, which makes failures attributable when several
configs run together.

## Languages

`languages` lists the translations each key is expected to provide and the JSON files to generate:

```yaml
languages: [en, de, fr]
```

Language codes are carried into the `{{language}}` placeholder in JSON output paths.

## Template engine and validation

```yaml
engine: handlebars
strict: true
check_templates: true
```

- `engine` selects placeholder parsing. Use `handlebars` for `{{name}}` expressions.
- `check_templates` compiles templates during generation.
- `strict` promotes warnings such as missing languages to errors.

The matching CLI flags override config values for an individual run. For example,
`globetrotter --dry-run` exercises the full pipeline without writing files.

## Inputs

An input can be a path:

```yaml
inputs:
  - ./translations.toml
```

or a mapping that controls its generated key namespace:

```yaml
inputs:
  - path: ./translations/**/*.toml
    prefix: app
    prepend_filename: true
    prepend_relative_path: true
    separator: "."
```

The path may be a single file or a glob. The optional fields compose the final key:

| Field | Effect |
|---|---|
| `prefix` | Adds a fixed namespace before every key from this input. |
| `prepend_filename` | Adds the input filename, without its extension. |
| `prepend_relative_path` | Adds directories below the glob's base path. |
| `separator` | Changes the separator used when joining key components. |

Use the smallest namespace that prevents collisions. A fixed application or package prefix is
usually enough; path-derived prefixes are useful for a large catalog split across directories.

## Outputs

JSON is configured as a path or list of paths. Include `{{language}}` when one config supports more
than one language:

```yaml
outputs:
  json:
    - ./generated/translations_{{language}}.json
```

TypeScript accepts a `type` path:

```yaml
outputs:
  typescript:
    type: ./generated/translations.ts
```

Rust accepts one path or a list:

```yaml
outputs:
  rust:
    - ./generated/translations.rs
```

Output directories are created as needed. The files are generated artifacts; keep their paths
stable and regenerate them rather than editing them manually.

## Multiple config paths

`--config` is repeatable and accepts files or directories:

```bash
globetrotter \
  --config packages/web/globetrotter.yaml \
  --config packages/api/globetrotter.yaml
```

Globetrotter finds the common base directory for concise progress paths while preserving each
config's own directory for input and output resolution.

Next: [translation files]({{< relref "translations.md" >}}) and
[generated outputs]({{< relref "outputs.md" >}}).
