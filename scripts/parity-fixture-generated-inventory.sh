#!/usr/bin/env bash
set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash scripts/parity-fixture-pack.sh "$tmp_dir/fixtures" >/dev/null
bash scripts/parity-fixture-inventory.sh \
  "$tmp_dir/fixtures" \
  docs/optimization/258-parity-fixture-inventory.md >/dev/null

echo "Parity fixture generated inventory ok"
