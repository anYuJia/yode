#!/usr/bin/env bash
set -euo pipefail

baseline="${1:-.yode/benchmarks/output-regression-snapshot.md}"
bash scripts/parity-visual-diff.sh --keep-ansi "$baseline" "$baseline" >/dev/null
echo "Parity visual ANSI CI ok"
