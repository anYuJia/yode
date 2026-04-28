#!/usr/bin/env bash
set -euo pipefail

bundle_dir="${1:-.yode/parity-artifacts}"
out_file="${2:-docs/optimization/284-eighth-artifact-summary-report.md}"
bash scripts/parity-artifact-bundle.sh "$bundle_dir" >/dev/null

count="$(find "$bundle_dir" -type f | wc -l | tr -d ' ')"
cat >"$out_file" <<EOF
# Eighth Artifact Summary Report

- files: $count
- bundle_dir: $bundle_dir
EOF

echo "Parity artifact summary report written: $out_file"
