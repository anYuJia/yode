#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-risk-register.sh "${1:-docs/optimization/243-parity-risk-register.md}"
echo "Parity risk register validate ok"
