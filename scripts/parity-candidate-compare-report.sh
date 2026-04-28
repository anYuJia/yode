#!/usr/bin/env bash
set -euo pipefail

baseline="${1:-.yode/benchmarks/output-regression-snapshot.md}"
out_file="${2:-.yode/benchmarks/candidate-compare-report.md}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

bash scripts/output-regression-snapshot.sh "$tmp_dir" >/dev/null
bash scripts/parity-visual-diff.sh \
  --cjk-width-report \
  --out "$out_file" \
  "$baseline" \
  "$tmp_dir/output-regression-snapshot.md" >/dev/null

echo "Parity candidate compare report written: $out_file"
