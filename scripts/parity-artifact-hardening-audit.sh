#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-artifact-size-budget.sh >/dev/null
bash scripts/parity-artifact-tree-report.sh >/dev/null
bash scripts/parity-artifact-docs-audit.sh >/dev/null
bash scripts/parity-artifact-upload-name-audit.sh >/dev/null
bash scripts/parity-artifact-matrix-report.sh >/dev/null
bash scripts/parity-failure-route-upload-report.sh >/dev/null
bash scripts/parity-compare-report-inventory.sh >/dev/null
bash scripts/parity-artifact-summary-report.sh >/dev/null
bash scripts/parity-artifact-bundle-ci.sh >/dev/null

echo "Parity artifact hardening audit ok"
