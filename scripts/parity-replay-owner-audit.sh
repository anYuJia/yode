#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/267-eighth-replay-owner-map.md}"
bash scripts/parity-replay-owner-map.sh docs/optimization/parity-automation-manifest.tsv "$out_file" >/dev/null
rg -q 'transcript-rendering' "$out_file"
rg -q 'remote-workflow' "$out_file"

echo "Parity replay owner audit ok"
