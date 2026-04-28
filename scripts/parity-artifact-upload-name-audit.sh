#!/usr/bin/env bash
set -euo pipefail

workflow="${1:-.github/workflows/ci.yml}"
for artifact in parity-snapshot-artifacts parity-replay-artifacts parity-visual-docs-artifacts; do
  rg -q "name: ${artifact}" "$workflow"
done

echo "Parity artifact upload name audit ok"
