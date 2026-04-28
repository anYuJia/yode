#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/289-eighth-handoff-artifact.md}"
bash scripts/parity-eighth-handoff.sh "$doc" >/dev/null
rg -q '^# Eighth Handoff Artifact' "$doc"
rg -q '292-ninth-100-claude-output-parity-tracker.md' "$doc"

echo "Eighth handoff validate ok"
