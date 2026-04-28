#!/usr/bin/env bash
set -euo pipefail

doc="${1:-docs/optimization/244-parity-known-limitations.md}"

[[ -f "$doc" ]] || { echo "Known limitations doc not found: $doc" >&2; exit 1; }
rg -q '^# Parity Known Limitations' "$doc"
rg -q '^## Terminal Variance' "$doc"
rg -q '^## Snapshot Scope' "$doc"
rg -q '^## Replay Scope' "$doc"
rg -q '^## Manual Review Remainders' "$doc"

echo "Parity limitations ok"
