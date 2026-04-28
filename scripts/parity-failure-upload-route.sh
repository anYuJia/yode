#!/usr/bin/env bash
set -euo pipefail

row_id="${1:-055}"
bash scripts/parity-failure-report.sh --row "$row_id"
