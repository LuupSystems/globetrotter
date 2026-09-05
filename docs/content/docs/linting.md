---
title: Linting
weight: 7
---

# Linting

`globetrotter lint` validates translation sources without writing generated files:

{{< terminal "lint" >}}

The deterministic checks report:

- a missing required language;
- empty text or leading/trailing whitespace;
- a template that does not compile with the configured engine;
- a value substituted in some languages but not in others; a name that only selects a wording,
  such as `{{#if my_condition}}`, may be absent from a language without that distinction;
- a condition whose branches are identical, so it has no effect;
- placeholders that are undeclared, or arguments that are never used;
- identical translations within one key;
- duplicate text shared by different keys.

The template checks use the configured `engine` (or `--engine`) and are never run with a guessed
one: without an engine, or with one that has no template analysis yet, they are skipped and a note
says so. Handlebars is the only engine with template analysis today.

Findings are warnings by default and errors when strict mode is active in the config or on the
command line.

Every finding carries the code shown in brackets, as in `warning[duplicate]: …`. Suppress one by
listing it with the `lint:` prefix in an `allow` list — on the key, on an enclosing table, or on the
whole config. See [local lint exceptions]({{< relref "translations.md" >}}) for the scoping rules.

## Find unused keys

Pass one or more source directories to report translation keys that application code never
references:

```bash
globetrotter lint --usages ./src --usages ./packages
```

For a monorepo, declare the owning sources on each config instead:

```yaml
version: 1
configs:
  airtype:
    languages: [de, en, fr]
    inputs:
      - path: ./translations/**/*.toml
    usages:
      roots:
        - ../../apps/airtype
        - ../../packages/shared-components
      dynamic: deny
```

Declared roots resolve relative to the YAML file. Each config checks its own keys against its own
roots, even when another config defines the same spelling. Repeated CLI `--usages` paths replace
the declared roots of **every selected config** and resolve relative to the current directory.
With neither declared nor CLI roots, unused-key checking is disabled for that config.

The scan respects `.ignore` and Git ignore rules by default. These are independent controls:
`--no-ignore` bypasses `.ignore`; `--no-gitignore` bypasses `.gitignore`, Git's global excludes,
and `.git/info/exclude`. Use both to bypass both families. The corresponding config settings are
`usages.respect_ignore_files` and `usages.respect_gitignore`, both defaulting to `true`.

Hidden source directories are included unless an ignore rule excludes them. There is no hard-coded
list of build or dependency directory names, and nested globetrotter configs do not cut off
discovery within a declared root. Git metadata and configured generated files remain excluded
regardless of ignore settings. List shared packages as additional roots when needed.
Unreadable roots or source files, syntax errors in scanned runtime code, and source files over
4 MiB fail the scan instead of presenting incomplete results as unused-key findings.

Identical unused-key findings for the same source section and resolved key are emitted once, with
the affected config names and config files attached. A key unused by one config may still be used
by another, possibly with a different prefix. Check all owners before deleting a shared TOML
section.

### Static and dynamic usages

Usage detection is an improved string matcher, with Tree-sitter identifying real runtime string
literals and excluding comments, documentation, types, and regular expressions. Every exact key
literal counts independently, including constants, lookup tables, and conditional branches.
Keys without matching evidence in their config's source roots are reported as **potentially
unused**; the scan does not prove whole-program reachability.

Opaque arguments such as `t(key)`, `t?.(CONSTANT)`, `t(keys[kind])`, and `t(getKey(value))` add no
usages and produce no dynamic diagnostic. Their declarations can still provide literal evidence.
Argument type safety belongs to the host compiler or type checker; the scanner performs no
symbol resolution, constant propagation, or data-flow analysis. For example, a template assigned
to a variable and later passed to `t(key)` produces no dynamic diagnostic; keys with no other
matching evidence are still reported as potentially unused.

JavaScript/TypeScript, JSX/TSX (including React, Next.js, and Remix), Vue, Svelte, Astro, HTML
templates, Rust, Go, Python, Ruby, PHP, Java, Kotlin, Swift, Dart, Elixir, Lua, Zig, and C# are
supported. Vue/Svelte/Astro scripts and template expressions are parsed as embedded code with
diagnostics pointing to their original source locations. Vue style `v-bind(...)` expressions and
Astro’s `define:vars` attributes are scanned as executable code. Ordinary CSS strings, HTML
comments, and data script blocks are excluded. Angular-style interpolations, bound attributes,
and `translate` pipes are also recognized. Configured Rust outputs contribute their generated enum variants as static
usage forms, including references nested inside opaque arguments; deleting those
variants would break compilation. Generated files themselves do not keep keys alive.

Dynamic calls use the first argument of `t`, `$t`, or `translate`, including member calls such as
`i18n.t`. Configure wrappers or other function names explicitly:

```yaml
usages:
  roots: [./src]
  functions: [t, i18n.lookup]
  dynamic: warn
```

`functions` replaces the defaults. A name without a dot matches that name or the final member of
a callee; a dotted name matches the complete callee. This is syntactic recognition, not import or
type resolution: an unrelated function named `t` also matches unless you narrow the list.
Unrelated dynamic templates and TypeScript template-literal types never keep a subtree alive in
the default scanner.
Only visibly constructed key arguments, such as ``t(`dialog.${kind}`)`` or `t("dialog." + kind)`,
trigger dynamic policy. Prefix resemblance outside a recognized call never contributes matches.
Branches and arrays need no special evaluation: their string literals
already count, and opaque alternatives add no diagnostic.

HTML interpolation support targets Angular expressions. Server template engines such as Jinja,
Django templates, and Blade are not supported; exclude those sources through ignore rules.
Static HTML attribute text is not a code literal, so data-driven keys passed as plain attributes
need a code reference to count. This scanner does not perform whole-program data-flow analysis.

The bundled grammars have limits: Svelte each blocks currently need `as`, TypeScript assertions
inside their iterable can confuse the Svelte grammar, and Angular's `*ngIf` `then` clause is not
supported. Such syntax produces an explicit scan error rather than an incomplete unused-key
result. Move an asserted iterable into a script variable, use the supported template form, or
exclude the affected source through ignore rules when appropriate. Astro expression markup is
parsed as TSX; use self-closing void elements and JSX-style comments inside those expressions.

| Dynamic policy | Effect |
|---|---|
| `allow` (default) | A known literal prefix containing a dot keeps matching keys alive. |
| `warn` | Accept the same prefixes and report `dynamic-usage` for review. |
| `deny` | Report `dynamic-usage` as an error; inferred dynamic prefixes do not keep keys alive. |

A visible interpolation without a known prefix, such as ``t(`${kind}.title`)``, is still
reported under `warn` or `deny`, but cannot identify a subtree. An empty or unknown prefix
never marks the whole catalog as used. Opaque function results and
arbitrary computations are not evaluated or classified as string construction.
Warnings also make lint exit non-zero, consistent with the other lint checks. `--strict` promotes
warnings to errors. A config-wide `allow: ["lint:dynamic-usage"]` suppresses the dynamic-call
diagnostic but does not change whether its keys count as used.

Override every selected config's policy for one invocation:

```bash
globetrotter lint --dynamic-usages deny
```

### Lightweight builds

The `tree-sitter` Cargo feature owns all parser dependencies. It is enabled by default in
`globetrotter-cli` and opt-in for library consumers. A CLI built without it still checks usages
using lightweight text matching, and prints a note identifying that mode. Config ownership,
root precedence, and diagnostic grouping are unchanged.

Text matching is less precise: exact spellings in comments and types can count as usages, and
`${...}` prefixes are recognized without call or language context. Its dynamic policy controls
those detected prefixes; it does not enforce the direct-translation-call boundary.
Use the default parser-enabled CLI for precise lexical usage checks.

Disable duplicate detection for a run with `--no-duplicates`. For a deliberate exception on one
key, prefer its local `allow` list:

```toml
allow = ["lint:duplicate"]
```

## LLM-assisted drift review

`--llm-judge` adds an experimental semantic review. It sends all languages for one key to an
OpenAI-compatible endpoint and asks whether they tell the user the same thing:

```bash
# Local Ollama endpoint and the default model.
globetrotter lint --llm-judge

# A hosted or otherwise compatible endpoint.
globetrotter lint \
  --llm-judge \
  --llm-base-url https://api.example.com/v1 \
  --llm-model my-model \
  --llm-api-key-env MY_API_KEY
```

This review is advisory. Model findings are emitted as notes and never make lint fail by
themselves; inspect the reason and the translations. The judge is deliberately tuned for recall,
so false positives are expected.

Verdicts are cached by content. A rerun only judges changed keys, and changing
`--llm-min-confidence` re-filters cached findings without sending new requests. Use `--max-keys 25`
to evaluate a model or prompt on a bounded slice before reviewing a large catalog.

Model choice matters. Small models can miss genuine meaning changes while inventing problems in
correct translations. Evaluate a model against examples from your own catalog, keep temperature at
the reproducible default, and treat reported confidence as a ranking rather than a probability.

Suppress a reviewed, intentional divergence with:

```toml
allow = ["lint:llm-drift"]
```

The [CLI reference]({{< relref "cli.md" >}}) lists the endpoint, prompt, concurrency, effort, cache,
and confidence controls.
