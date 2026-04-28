#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/246-seventh-100-claude-output-parity-tracker.md}"
[[ -f "$doc" ]] || { echo "Seventh tracker missing: $doc" >&2; exit 1; }
rg -q '`0 / 100` 已完成' "$doc"
rg -q '^# Seventh 100 Claude Output / Interaction Parity Tracker' "$doc"

echo "Parity seventh seed CI ok"
