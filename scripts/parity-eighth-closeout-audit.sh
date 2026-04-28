#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-docs-ci.sh >/dev/null
bash scripts/parity-workflow-audit.sh >/dev/null
bash scripts/parity-replay-hardening-audit.sh >/dev/null
bash scripts/parity-artifact-hardening-audit.sh >/dev/null
bash scripts/parity-eighth-release-note-validate.sh >/dev/null
bash scripts/parity-eighth-risk-validate.sh >/dev/null
bash scripts/parity-eighth-limitations-validate.sh >/dev/null
bash scripts/parity-eighth-handoff-validate.sh >/dev/null
bash scripts/parity-eighth-summary-validate.sh >/dev/null
bash scripts/parity-ninth-seed-ci.sh >/dev/null

echo "Eighth closeout audit ok"
