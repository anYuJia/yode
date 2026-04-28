#!/usr/bin/env bash
set -euo pipefail

dir="${1:-.yode/benchmarks/replay}"
bash scripts/parity-replay-serialize.sh "$dir" >/dev/null
rg -q '"version": "v1"' "$dir/replay-index.json"

echo "Parity replay version validate ok"
