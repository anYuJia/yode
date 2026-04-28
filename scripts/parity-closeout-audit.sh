#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-docs-ci.sh >/dev/null
bash scripts/parity-fixture-audit.sh >/dev/null
bash scripts/parity-command-audit.sh >/dev/null
bash scripts/parity-owner-enforcement.sh >/dev/null
bash scripts/parity-risk-register.sh >/dev/null
bash scripts/parity-limitations-ci.sh >/dev/null
bash scripts/parity-release-note-validate.sh >/dev/null
bash scripts/parity-seventh-seed-ci.sh >/dev/null
bash scripts/parity-summary-report.sh >/dev/null
bash scripts/parity-handoff-artifact.sh >/dev/null
bash scripts/parity-visual-inventory.sh >/dev/null
bash scripts/parity-signoff.sh >/dev/null
bash scripts/parity-docs-final-audit.sh >/dev/null
bash scripts/parity-script-final-sweep.sh >/dev/null

echo "Parity closeout audit ok"
