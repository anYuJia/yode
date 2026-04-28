#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-replay-directory-audit.sh >/dev/null
bash scripts/parity-replay-version-validate.sh >/dev/null
bash scripts/parity-replay-schema-report.sh >/dev/null
bash scripts/parity-replay-normalization-ci.sh >/dev/null
bash scripts/parity-replay-smoke-bundle.sh >/dev/null
bash scripts/parity-replay-drift-report.sh >/dev/null
bash scripts/parity-replay-jsonl-inventory.sh >/dev/null
bash scripts/parity-replay-owner-audit.sh >/dev/null
bash scripts/parity-replay-sample-export.sh >/dev/null
bash scripts/parity-replay-clean.sh >/dev/null
bash scripts/parity-replay-retention.sh >/dev/null
bash scripts/parity-replay-summary-report.sh >/dev/null

echo "Parity replay hardening audit ok"
