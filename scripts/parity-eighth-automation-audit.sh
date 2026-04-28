#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-automation-audit.sh >/dev/null
bash scripts/parity-replay-hardening-audit.sh >/dev/null
bash scripts/parity-artifact-hardening-audit.sh >/dev/null

echo "Eighth automation audit ok"
