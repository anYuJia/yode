#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-tracker-count.sh >/dev/null
bash scripts/parity-docs-ci.sh >/dev/null
bash scripts/parity-fixture-audit.sh >/dev/null
bash scripts/parity-command-audit.sh >/dev/null
bash scripts/parity-script-syntax-sweep.sh >/dev/null

echo "Parity CI smoke ok"
