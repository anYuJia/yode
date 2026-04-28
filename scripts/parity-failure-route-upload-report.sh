#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/282-eighth-failure-route-upload-report.md}"
bash scripts/parity-failure-upload-route.sh 055 >"$out_file"

echo "Parity failure route upload report written: $out_file"
