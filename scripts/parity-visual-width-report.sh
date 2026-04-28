#!/usr/bin/env bash
set -euo pipefail

baseline="${1:-.yode/benchmarks/output-regression-snapshot.md}"
out_file="${2:-.yode/benchmarks/visual-width-report.md}"

bash scripts/parity-visual-diff.sh \
  --cjk-width-report \
  --out "$out_file" \
  "$baseline" \
  "$baseline" >/dev/null

rg -q '## CJK Width Report' "$out_file"
echo "Parity visual width report ok: $out_file"
