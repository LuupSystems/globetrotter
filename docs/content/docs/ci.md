---
title: Continuous integration
weight: 9
---

# Continuous integration

A translation pipeline normally has three independent gates:

1. source files are formatted;
2. source files pass lint;
3. committed generated files match the current catalogs and generator.

Keep those checks separate enough that a failure says what changed.

## Verify committed output

When generated files are committed, regenerate them and fail on a diff:

```bash
globetrotter format --check
globetrotter lint
globetrotter
git diff --exit-code -- generated/
```

Replace `generated/` with the output paths from your config. Running generation before `git diff`
tests the same config developers use locally; it does not rely on a second list of expected keys.

## GitHub Actions

This job installs the published CLI. A repository that builds Globetrotter itself should use its
working-tree binary instead.

```yaml
name: translations
on:
  pull_request:
  push:
    branches: [main]

jobs:
  translations:
    runs-on: ubuntu-24.04
    timeout-minutes: 10
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
      - name: Install globetrotter
        run: cargo install --locked globetrotter-cli
      - name: Check formatting
        run: globetrotter format --check
      - name: Lint translations
        run: globetrotter lint
      - name: Verify generated files
        run: |
          globetrotter
          git diff --exit-code -- generated/
```

Pin the CLI version when output stability across tool upgrades matters:

```bash
cargo install --locked globetrotter-cli --version 0.0.10
```

Update the pin and committed generated files together.

## Generate instead of commit

If generated files are build artifacts, run Globetrotter immediately before compiling the
consumer:

```yaml
- name: Generate translations
  run: globetrotter
- name: Build
  run: cargo build --locked
```

Cache compiler output, not generated translations. The generation step is small and should remain
visible in the build log.

## LLM review in CI

Keep `--llm-judge` outside a required correctness gate. It depends on an external model, is
advisory, and intentionally does not fail lint for semantic findings. A separate review job can
surface suggestions without making deterministic checks depend on model availability.
