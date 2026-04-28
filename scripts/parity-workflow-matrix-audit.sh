#!/usr/bin/env bash
set -euo pipefail

workflow="${1:-.github/workflows/ci.yml}"

job_count="$(grep -c '^  parity-' "$workflow" || true)"
if (( job_count < 3 )); then
  echo "Expected at least 3 parity jobs, got $job_count" >&2
  exit 1
fi

echo "Parity workflow matrix audit ok"
