#!/usr/bin/env bash
set -euo pipefail

out_file="${1:-docs/optimization/283-eighth-compare-report-inventory.md}"

cat >"$out_file" <<'EOF'
# Eighth Compare Report Inventory

- candidate compare report
- catalog compare report
- visual diff report
- visual width report
EOF

echo "Parity compare report inventory written: $out_file"
