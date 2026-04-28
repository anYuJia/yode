#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

out="$(bash scripts/parity-fixture-generate.sh generic custom "$tmp_dir/custom")"
[[ "$out" == "$tmp_dir/custom/custom.generic.md" ]] || {
  echo "Unexpected custom fixture path: $out" >&2
  exit 1
}
[[ -f "$out" ]] || { echo "Custom fixture missing: $out" >&2; exit 1; }

echo "Parity fixture custom path smoke ok"
