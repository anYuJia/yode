#!/usr/bin/env bash
set -euo pipefail

dir="${1:-.yode/benchmarks/replay}"
max_files="${2:-20}"
bash scripts/parity-replay-serialize.sh "$dir" >/dev/null
count="$(find "$dir" -type f | wc -l | tr -d ' ')"
if (( count > max_files )); then
  echo "Replay retention exceeded: $count > $max_files" >&2
  exit 1
fi

echo "Parity replay retention ok: files=$count"
