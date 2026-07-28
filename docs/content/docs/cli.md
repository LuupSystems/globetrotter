---
title: CLI reference
weight: 8
---

# CLI reference

Running `globetrotter` without a subcommand generates every configured output. The `format` and
`lint` subcommands operate on the same discovered inputs.

{{< terminal "help" >}}

## Generation

```bash
globetrotter [OPTIONS]
```

Common options:

| Option | Purpose |
|---|---|
| `-c, --config <PATH>` | Load a config file, or search a directory for one. Repeatable. |
| `-i, --translation <PATH>` | Add a translation file directly. Repeatable. |
| `--engine <ENGINE>` | Override the configured template engine. |
| `--strict[=<BOOL>]` | Promote warnings to errors. |
| `--check[=<BOOL>]` | Compile and validate templates. |
| `--dry-run[=<BOOL>]` | Run without writing output files. |
| `--absolute[=<BOOL>]` | Print absolute paths instead of paths relative to the shared base directory. |
| `--max-keys <N>` | Process a bounded prefix of each config for debugging. |
| `--color <CHOICE>` | Control ANSI color output. |
| `--log <LEVEL>` | Set the log level unless `RUST_LOG` overrides it. |
| `--log-format <FORMAT>` | Select pretty or JSON logs. |

When neither `--config` nor `--translation` is present, Globetrotter searches the current directory
for its config file.

## Format

```bash
globetrotter format [OPTIONS]
```

The formatter sorts translation keys while preserving comments. It defaults to ascending order.
`--check` exits non-zero when a file would change and is suitable for CI.

{{< terminal "format-help" >}}

## Lint

```bash
globetrotter lint [OPTIONS]
```

Lint includes all shared input and config options. Its own controls cover source-usage scanning,
duplicate detection, and the experimental LLM judge.

{{< terminal "lint-help" >}}

Useful groups:

- `--usages <DIR>` is repeatable and enables unused-key scanning.
- `--no-duplicates` disables both cross-key duplicate checks and identical-language checks.
- `--llm-judge` enables semantic drift review.
- `--llm-base-url`, `--llm-model`, and `--llm-api-key-env` select the endpoint.
- `--llm-concurrency`, `--llm-temperature`, and `--llm-effort` control requests.
- `--llm-prompt` loads a custom prompt containing `{key}` and `{languages}`.
- `--llm-min-confidence` filters reported findings after the cache.
- `--cache-dir` and `--llm-cache-capacity` control persisted verdicts.

The generated help above is captured from the working-tree binary during every documentation build,
so it is the authoritative list when flags change.
