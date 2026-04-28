#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/287-eighth-risk-register.md}"

cat >"$out_file" <<'EOF'
# Eighth Risk Register

## CI Drift

- workflow jobs diverge from local parity scripts

## Replay Drift

- stored replay schema drifts from fixture generators

## Artifact Drift

- uploaded bundle contents diverge from local bundle manifest

## Review Drift

- visual and compare reports stop being generated or uploaded
EOF

echo "Eighth risk register written: $out_file"
