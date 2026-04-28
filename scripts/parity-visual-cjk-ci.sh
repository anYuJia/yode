#!/usr/bin/env bash
set -euo pipefail

baseline="${1:-.yode/benchmarks/output-regression-snapshot.md}"
bash scripts/parity-visual-diff.sh --cjk-width-report "$baseline" "$baseline" >/dev/null
echo "Parity visual CJK CI ok"
