#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/291-eighth-signoff.md}"
bash scripts/parity-eighth-signoff.sh "$doc" >/dev/null
rg -q '^# Eighth Signoff' "$doc"

echo "Eighth signoff verify ok"
