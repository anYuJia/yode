#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-replay-hardening-audit.sh >/dev/null
echo "Eighth replay audit ok"
