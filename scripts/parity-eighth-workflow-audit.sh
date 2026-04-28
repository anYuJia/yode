#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-workflow-audit.sh >/dev/null
bash scripts/parity-workflow-trigger-audit.sh >/dev/null
bash scripts/parity-release-workflow-audit.sh >/dev/null
bash scripts/parity-workflow-matrix-audit.sh >/dev/null

echo "Eighth workflow audit ok"
