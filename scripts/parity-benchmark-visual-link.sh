#!/usr/bin/env bash
set -euo pipefail

guide="${1:-docs/optimization/242-golden-snapshot-storage-proposal.md}"
[[ -f .yode/benchmarks/long-session-benchmark.md ]] || { echo "Benchmark snapshot missing" >&2; exit 1; }
rg -q 'long-session-benchmark.md' "$guide"

echo "Parity benchmark visual link ok"
