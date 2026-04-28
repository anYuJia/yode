#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/273-eighth-replay-schema-report.md}"

cat >"$out_file" <<'EOF'
# Eighth Replay Schema Report

- version
- created_at
- source_generator
- name
- kind
- path
- body
EOF

echo "Parity replay schema report written: $out_file"
