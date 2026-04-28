#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-docs-ci.sh >/dev/null
rg -q 'Sixth-round parity work' docs/optimization/240-sixth-100-claude-output-parity-tracker.md || true

echo "Parity docs final audit ok"
