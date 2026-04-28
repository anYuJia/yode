#!/usr/bin/env bash
set -euo pipefail

replay_dir="${1:-.yode/benchmarks/replay}"
out_file="${2:-docs/optimization/274-eighth-replay-drift-report.md}"

if bash scripts/parity-replay-drift-check.sh "$replay_dir" >"$out_file"; then
  :
fi

echo "Parity replay drift report written: $out_file"
