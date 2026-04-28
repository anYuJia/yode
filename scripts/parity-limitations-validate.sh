#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-limitations-ci.sh "${1:-docs/optimization/244-parity-known-limitations.md}"
echo "Parity limitations validate ok"
