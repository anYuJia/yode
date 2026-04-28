#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-script-syntax-sweep.sh >/dev/null
bash scripts/parity-ci-smoke.sh >/dev/null
bash scripts/parity-fixture-hardening-audit.sh >/dev/null
bash scripts/parity-visual-hardening-audit.sh >/dev/null

echo "Parity automation audit ok"
