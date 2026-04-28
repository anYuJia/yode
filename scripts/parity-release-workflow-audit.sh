#!/usr/bin/env bash
set -euo pipefail

workflow="${1:-.github/workflows/release.yml}"
[[ -f "$workflow" ]] || { echo "Release workflow missing: $workflow" >&2; exit 1; }
rg -q '^name: Release' "$workflow"
rg -q 'tags:' "$workflow"
rg -q 'v\*' "$workflow"
rg -q 'generate_release_notes: true' "$workflow"

echo "Parity release workflow audit ok"
