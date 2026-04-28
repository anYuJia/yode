#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/259-sixth-parity-summary-report.md}"

fourth_done="$(grep -Ec '^- `\[x\]`? [0-9][0-9][0-9]' docs/optimization/236-fourth-100-claude-output-parity-tracker.md || true)"
fifth_done="$(grep -Ec '^- `\[x\]`? [0-9][0-9][0-9]' docs/optimization/238-fifth-100-claude-output-parity-tracker.md || true)"
sixth_done="$(grep -Ec '^- `\[x\]`? [0-9][0-9][0-9]' docs/optimization/240-sixth-100-claude-output-parity-tracker.md || true)"

cat >"$out_file" <<EOF
# Sixth Parity Summary Report

- fourth_done: $fourth_done
- fifth_done: $fifth_done
- sixth_done: $sixth_done
- local_ci: scripts/parity-ci-local.sh
- closeout_audit: scripts/parity-closeout-audit.sh
EOF

echo "Parity summary report written: $out_file"
