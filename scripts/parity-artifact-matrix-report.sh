#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/281-eighth-artifact-matrix-report.md}"

cat >"$out_file" <<'EOF'
# Eighth Artifact Matrix Report

- parity-snapshot -> snapshot, benchmark, visual reports, golden current
- parity-replay -> replay index/json/jsonl, failure route report
- parity-visual-docs -> visual diff, width report, catalog compare, docs outputs
EOF

echo "Parity artifact matrix report written: $out_file"
