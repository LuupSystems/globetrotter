#!/usr/bin/env bash
# Regenerate the runnable documentation example and its terminal snippets.
#
# Hugo reads the example inputs and generated outputs directly from docs/examples. Running this
# script before every site build keeps those files and the displayed CLI output tied to the
# working-tree globetrotter binary.
set -euo pipefail

export CLICOLOR_FORCE=1 TERM=xterm-256color
unset NO_COLOR

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
example="$repo/docs/examples/quickstart"
out="$repo/docs/assets/terminals"
globetrotter="${GLOBETROTTER_BIN:-$repo/target/debug/globetrotter}"

command -v terminal-to-html >/dev/null || {
  echo "terminal-to-html not found — run 'mise install'" >&2
  exit 1
}

if [[ -z "${GLOBETROTTER_BIN:-}" ]]; then
  cargo build -p globetrotter-cli --manifest-path "$repo/Cargo.toml"
fi
[[ -x "$globetrotter" ]] || {
  echo "globetrotter binary not found or not executable: $globetrotter" >&2
  exit 1
}

mkdir -p "$example/generated" "$out"

render() {
  local name="$1" label="$2"
  {
    printf '\033[1;32m$\033[0m %s\n\n' "$label"
    cat
  } | terminal-to-html >"$out/$name.html"
  echo "wrote $out/$name.html"
}

generated="$(
  cd "$example"
  "$globetrotter" -c globetrotter.yaml 2>&1
)"
generated="$(
  printf '%s\n' "$generated" |
    sed -E 's/completed in [0-9]+(\.[0-9]+)?(ns|µs|ms|s)/completed in 0.00ms/'
)"
rows="$(printf '%s\n' "$generated" | sed -n '/ wrote /p' | LC_ALL=C sort)"
footer="$(printf '%s\n' "$generated" | sed -n '/completed in /p')"
[[ -n "$rows" && -n "$footer" ]] || {
  echo "globetrotter did not produce the expected generation summary" >&2
  exit 1
}
printf '%s\n%s\n' "$rows" "$footer" |
  render generate "globetrotter -c globetrotter.yaml"

"$globetrotter" --help 2>&1 |
  render help "globetrotter --help"
"$globetrotter" format --help 2>&1 |
  render format-help "globetrotter format --help"
"$globetrotter" lint --help 2>&1 |
  render lint-help "globetrotter lint --help"

linted="$(
  cd "$example"
  "$globetrotter" lint 2>&1
)"
printf '%s\n' "$linted" |
  sed -E 's/no issues found in [0-9]+(\.[0-9]+)?(ns|µs|ms|s)/no issues found in 0ms/' |
  render lint "globetrotter lint"
