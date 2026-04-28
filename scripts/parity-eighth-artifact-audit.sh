#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-artifact-hardening-audit.sh >/dev/null
echo "Eighth artifact audit ok"
