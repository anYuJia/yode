#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-.yode/benchmarks}"
golden_dir="${2:-$out_dir/golden/current}"

bash scripts/output-regression-snapshot.sh "$out_dir" >/dev/null
bash scripts/split-output-regression-snapshot.sh \
  "$out_dir/output-regression-snapshot.md" \
  "$out_dir/output-regression-sections" >/dev/null
bash scripts/build-snapshot-catalogs.sh \
  "$out_dir/output-regression-snapshot.md" \
  "$out_dir/catalogs" >/dev/null
bash scripts/benchmark-snapshot.sh "$out_dir" >/dev/null
bash scripts/parity-golden-snapshot-store.sh "$out_dir" "$golden_dir" >/dev/null

echo "Parity baseline refresh ok: $out_dir"
