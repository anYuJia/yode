#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/290-eighth-summary-report.md}"
bash scripts/parity-eighth-summary-report.sh "$doc" >/dev/null
rg -q '^# Eighth Summary Report' "$doc"

echo "Eighth summary validate ok"
