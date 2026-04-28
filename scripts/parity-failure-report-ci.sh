#!/usr/bin/env bash
set -euo pipefail

row_output="$(bash scripts/parity-failure-report.sh --row 055)"
surface_output="$(bash scripts/parity-failure-report.sh --surface markdown)"

grep -q '^row=055' <<<"$row_output"
grep -q '^owner=doctor-export' <<<"$row_output"
grep -q '^owner=markdown-rendering' <<<"$surface_output"
grep -q '^next=' <<<"$surface_output"

echo "Parity failure report CI ok"
