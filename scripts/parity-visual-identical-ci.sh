#!/usr/bin/env bash
set -euo pipefail

baseline="${1:-.yode/benchmarks/output-regression-snapshot.md}"
bash scripts/parity-visual-diff.sh "$baseline" "$baseline" >/dev/null
echo "Parity visual identical CI ok"
