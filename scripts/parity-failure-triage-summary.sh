#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/271-eighth-failure-triage-summary.md}"

cat >"$out_file" <<'EOF'
# Eighth Failure Triage Summary

## Surfaces

- snapshot -> `scripts/parity-snapshot-ci.sh`
- replay -> `scripts/parity-replay-ci.sh`
- visual-docs -> `scripts/parity-visual-hardening-audit.sh && scripts/parity-docs-ci.sh`

## Reports

- `scripts/parity-failure-upload-route.sh`
- `scripts/parity-failure-report.sh`
- `scripts/parity-failure-report-template.sh`
EOF

echo "Parity failure triage summary written: $out_file"
