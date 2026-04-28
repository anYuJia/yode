#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/245-parity-release-note-draft.md}"
[[ -f "$doc" ]] || { echo "Release note draft missing: $doc" >&2; exit 1; }
rg -q '^# Parity Release Note Draft' "$doc"
rg -q '^## Status' "$doc"
rg -q '^## Highlights' "$doc"

echo "Parity release note validate ok"
