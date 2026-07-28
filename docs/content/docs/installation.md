---
title: Installation
weight: 2
---

# Installation

Globetrotter is distributed as the `globetrotter` command-line binary.

## Homebrew

```bash
brew install --cask LuupSystems/tap/globetrotter
```

The tap installs a prebuilt release. Upgrade it with the normal Homebrew workflow:

```bash
brew upgrade --cask globetrotter
```

## From crates.io

The binary is published by the `globetrotter-cli` crate:

```bash
cargo install --locked globetrotter-cli
```

`--locked` uses the dependency versions recorded for the published release.

## Prebuilt releases

Release archives for supported platforms are attached to the
[GitHub releases](https://github.com/LuupSystems/globetrotter/releases). Extract the archive and put
the `globetrotter` executable on your `PATH`.

## From source

```bash
git clone https://github.com/LuupSystems/globetrotter
cd globetrotter
cargo build --release --package globetrotter-cli
```

The binary is written to `target/release/globetrotter`.

## Verify the installation

```bash
globetrotter --version
globetrotter --help
```

{{< terminal "help" >}}

The CLI has no runtime service and does not need a database. A project only needs a YAML config and
one or more TOML translation files.

Next: [generate the quick-start example]({{< relref "quick-start.md" >}}).
