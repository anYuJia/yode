#!/usr/bin/env bash
set -euo pipefail

bash scripts/parity-ci-dry-run.sh >/dev/null
echo "Eighth dry-run audit ok"
