#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/292-ninth-100-claude-output-parity-tracker.md}"
[[ -f "$doc" ]] || { echo "Ninth tracker missing" >&2; exit 1; }
rg -q '^# Ninth 100 Claude Output / Interaction Parity Tracker' "$doc"
rg -q '`0 / 100` 已完成' "$doc"

echo "Ninth seed CI ok"
