#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

generic="$(bash scripts/parity-fixture-generate.sh generic sample "$tmp_dir")"
transcript="$(bash scripts/parity-generate-transcript-fixture.sh replay "$tmp_dir")"
markdown="$(bash scripts/parity-generate-markdown-fixture.sh visual "$tmp_dir")"
operator="$(bash scripts/parity-generate-operator-flow-fixture.sh e2e "$tmp_dir")"
minimized="$tmp_dir/minimized.md"

bash scripts/parity-fixture-minimize.sh "$markdown" "$minimized" >/dev/null

for path in "$generic" "$transcript" "$markdown" "$operator" "$minimized"; do
  [[ -f "$path" ]] || { echo "Fixture output missing: $path" >&2; exit 1; }
  [[ -s "$path" ]] || { echo "Fixture output empty: $path" >&2; exit 1; }
done

rg -q '^# Parity Fixture|^# Transcript Replay Fixture|^# Markdown Fixture|^# Operator Flow Fixture' \
  "$generic" "$transcript" "$markdown" "$operator"

echo "Parity fixture freshness ok"
