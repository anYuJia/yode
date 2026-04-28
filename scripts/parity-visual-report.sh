#!/usr/bin/env bash
set -euo pipefail

baseline="${1:-.yode/benchmarks/output-regression-snapshot.md}"
candidate="${2:-$baseline}"
out_file="${3:-.yode/benchmarks/visual-diff-report.md}"

bash scripts/parity-visual-diff.sh \
  --cjk-width-report \
  --out "$out_file" \
  "$baseline" \
  "$candidate" >/dev/null

echo "Parity visual report written: $out_file"
