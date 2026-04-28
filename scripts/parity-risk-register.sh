#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/243-parity-risk-register.md}"

[[ -f "$doc" ]] || { echo "Risk register not found: $doc" >&2; exit 1; }
rg -q '^# Parity Risk Register' "$doc"
rg -q '^## Snapshot Drift' "$doc"
rg -q '^## Replay Coverage Drift' "$doc"
rg -q '^## Visual Diff Noise' "$doc"
rg -q '^## E2E Contract Drift' "$doc"
rg -q '^## Docs / Tracker Drift' "$doc"

echo "Parity risk register ok"
