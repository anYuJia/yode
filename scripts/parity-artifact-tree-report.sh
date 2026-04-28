#!/usr/bin/env bash
set -euo pipefail

bundle_dir="${1:-.yode/parity-artifacts}"
out_file="${2:-docs/optimization/280-eighth-artifact-tree-report.md}"
bash scripts/parity-artifact-bundle.sh "$bundle_dir" >/dev/null

{
  echo "# Eighth Artifact Tree Report"
  echo
  find "$bundle_dir" -type f | sort | sed "s#^$bundle_dir/#- #"
} >"$out_file"

echo "Parity artifact tree report written: $out_file"
