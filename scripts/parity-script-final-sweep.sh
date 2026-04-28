#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-script-syntax-sweep.sh >/dev/null
bash scripts/parity-job-list.sh >/dev/null

echo "Parity script final sweep ok"
