#!/usr/bin/env bash
set -euo pipefail

fixture_dir="${1:-.yode/benchmarks/fixtures}"
out_file="${2:-}"

tmp_output="$(mktemp)"
trap 'rm -f "$tmp_output"' EXIT

{
  echo "# Parity Fixture Inventory"
  echo
  find "$fixture_dir" -type f 2>/dev/null | sort | while read -r path; do
    rel="${path#$fixture_dir/}"
    echo "- $rel"
  done
} >"$tmp_output"

if [[ -n "$out_file" ]]; then
  mkdir -p "$(dirname "$out_file")"
  cp "$tmp_output" "$out_file"
  echo "$out_file"
else
  cat "$tmp_output"
fi
