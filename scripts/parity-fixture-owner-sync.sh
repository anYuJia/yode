#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-docs/optimization/parity-automation-manifest.tsv}"
tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT

awk -F '\t' '
  NR > 1 && ($3 == "replay" || $3 == "e2e") { print $5 }
' "$manifest" | sort -u > "$tmp_file"

for owner in transcript-rendering markdown-rendering remote-workflow hooks-tasks inspector-confirm; do
  grep -qx "$owner" "$tmp_file" || {
    echo "Fixture owner missing from replay/e2e set: $owner" >&2
    exit 1
  }
done

echo "Parity fixture owner sync ok"
