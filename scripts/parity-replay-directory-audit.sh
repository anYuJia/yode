#!/usr/bin/env bash
set -euo pipefail

dir="${1:-.yode/benchmarks/replay}"
bash scripts/parity-replay-serialize.sh "$dir" >/dev/null
[[ -d "$dir" ]] || { echo "Replay directory missing" >&2; exit 1; }
find "$dir" -type f | grep -q .

echo "Parity replay directory audit ok"
