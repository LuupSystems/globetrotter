#!/usr/bin/env bash
# Regenerate the README screenshot from the real globetrotter CLI.
#
# The output is captured through a pipe, so color is forced explicitly. Volatile timing and
# asynchronous completion order are normalized before rendering to avoid irrelevant PNG churn.
set -euo pipefail

export CLICOLOR_FORCE=1 TERM=xterm-256color
unset NO_COLOR

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
demo="$repo/test-data"
output="$demo/demo.png"
globetrotter="${GLOBETROTTER_BIN:-$repo/target/debug/globetrotter}"

command -v freeze >/dev/null || {
  echo "freeze not found — run 'mise install'" >&2
  exit 1
}

if [[ -z "${GLOBETROTTER_BIN:-}" ]]; then
  cargo build -p globetrotter-cli --manifest-path "$repo/Cargo.toml"
fi
[[ -x "$globetrotter" ]] || {
  echo "globetrotter binary not found or not executable: $globetrotter" >&2
  exit 1
}

ansi="$(
  cd "$demo"
  printf '\033[1;32m$\033[0m globetrotter -c globetrotter.yaml\n'
  "$globetrotter" -c globetrotter.yaml 2>&1
)"
ansi="$(
  printf '%s\n' "$ansi" |
    sed -E 's/completed in [0-9]+(\.[0-9]+)?(ns|µs|ms|s)/completed in 0.00ms/'
)"
prompt="$(printf '%s\n' "$ansi" | sed -n '1p')"
rows="$(printf '%s\n' "$ansi" | sed -n '/ wrote /p' | LC_ALL=C sort)"
footer="$(printf '%s\n' "$ansi" | sed -n '/completed in /p')"
ansi="$(printf '%s\n\n%s\n%s\n' "$prompt" "$rows" "$footer")"

printf '%s\n' "$ansi" |
  freeze \
    --language ansi \
    --output "$output" \
    --window \
    --shadow.blur 20 \
    --border.radius 8 \
    --padding 20

echo "wrote $output"
