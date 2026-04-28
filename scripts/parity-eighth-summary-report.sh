#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/290-eighth-summary-report.md}"

count="$(grep -Ec '^- `\[x\]` [0-9][0-9][0-9]' docs/optimization/256-eighth-100-claude-output-parity-tracker.md || true)"

cat >"$out_file" <<EOF
# Eighth Summary Report

- completed: $count
- workflow_audit: scripts/parity-workflow-audit.sh
- replay_hardening: scripts/parity-replay-hardening-audit.sh
- artifact_hardening: scripts/parity-artifact-hardening-audit.sh
EOF

echo "Eighth summary report written: $out_file"
