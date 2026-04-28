#!/usr/bin/env bash
set -euo pipefail

baseline="${1:-.yode/benchmarks/output-regression-snapshot.md}"
bash scripts/parity-visual-diff.sh --keep-hyperlinks "$baseline" "$baseline" >/dev/null
echo "Parity visual hyperlink CI ok"
