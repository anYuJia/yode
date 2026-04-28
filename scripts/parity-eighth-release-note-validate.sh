#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/286-eighth-release-note-draft.md}"
bash scripts/parity-eighth-release-note.sh "$doc" >/dev/null
rg -q '^# Eighth Release Note Draft' "$doc"

echo "Eighth release note validate ok"
