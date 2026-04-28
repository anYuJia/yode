#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/287-eighth-risk-register.md}"
bash scripts/parity-eighth-risk-register.sh "$doc" >/dev/null
rg -q '^# Eighth Risk Register' "$doc"

echo "Eighth risk validate ok"
