#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-script-syntax-sweep.sh >/dev/null
bash scripts/parity-baseline-refresh.sh >/dev/null
bash scripts/parity-benchmark-ci.sh >/dev/null
bash scripts/parity-golden-store-ci.sh >/dev/null
bash scripts/parity-ci-matrix-report.sh >/dev/null
bash scripts/parity-doc-link-audit.sh >/dev/null
bash scripts/parity-manifest-sync-report.sh >/dev/null
bash scripts/parity-failure-report-ci.sh >/dev/null
bash scripts/parity-env-report.sh >/dev/null
bash scripts/parity-job-list.sh >/dev/null
bash scripts/parity-ci-smoke.sh >/dev/null
bash scripts/parity-artifact-cleanup.sh >/dev/null
bash scripts/parity-baseline-status.sh >/dev/null
bash scripts/parity-replay-inventory.sh >/dev/null

echo "Parity CI hardening audit ok"
