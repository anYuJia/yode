#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/288-eighth-limitations-note.md}"
bash scripts/parity-eighth-limitations.sh "$doc" >/dev/null
rg -q '^# Eighth Limitations Note' "$doc"

echo "Eighth limitations validate ok"
