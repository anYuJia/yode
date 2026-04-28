#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-.yode/parity-artifacts}"

bash scripts/parity-replay-serialize.sh .yode/benchmarks/replay >/dev/null
bash scripts/parity-artifact-bundle.sh "$out_dir" >/dev/null

manifest="$out_dir/MANIFEST.md"
[[ -f "$manifest" ]] || { echo "Artifact bundle manifest missing" >&2; exit 1; }
rg -q '^# Parity Artifact Bundle' "$manifest"
rg -q 'benchmarks/output-regression-snapshot.md' "$manifest"
rg -q 'docs/245-parity-release-note-draft.md' "$manifest"
rg -q 'benchmarks/replay/replay-index.json' "$manifest"
rg -q 'benchmarks/candidate-compare-report.md' "$manifest" || true
rg -q 'benchmarks/catalog-compare-report.md' "$manifest" || true
rg -q 'benchmarks/failure-route-report.md' "$manifest" || true

echo "Parity artifact bundle CI ok"
