#!/usr/bin/env bash
set -euo pipefail

workflow="${1:-.github/workflows/ci.yml}"

[[ -f "$workflow" ]] || { echo "Workflow missing: $workflow" >&2; exit 1; }

for job in parity-snapshot parity-replay parity-visual-docs; do
  rg -q "^  ${job}:" "$workflow"
done

rg -q '^permissions:' "$workflow"
rg -q 'contents: read' "$workflow"
rg -q '^concurrency:' "$workflow"
rg -q 'group: ci-\$\{\{ github.workflow \}\}-\$\{\{ github.ref \}\}' "$workflow"

for artifact in parity-snapshot-artifacts parity-replay-artifacts parity-visual-docs-artifacts; do
  rg -q "name: ${artifact}" "$workflow"
done

cache_count="$(grep -c 'Swatinem/rust-cache@v2' "$workflow" || true)"
if (( cache_count < 4 )); then
  echo "Expected rust-cache in rust and parity jobs, got $cache_count" >&2
  exit 1
fi

timeout_count="$(grep -c 'timeout-minutes:' "$workflow" || true)"
if (( timeout_count < 4 )); then
  echo "Expected timeout-minutes for rust and parity jobs, got $timeout_count" >&2
  exit 1
fi

echo "Parity workflow audit ok"
