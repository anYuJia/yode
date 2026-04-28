#!/usr/bin/env bash
set -euo pipefail

dir="${1:-.yode/benchmarks}"
max_files="${2:-200}"
max_mb="${3:-32}"

if [[ ! -d "$dir" ]]; then
  echo "Snapshot directory not found, skipped retention audit: $dir"
  exit 0
fi

file_count="$(find "$dir" -type f | wc -l | tr -d ' ')"
total_bytes="$(find "$dir" -type f -print0 | xargs -0 wc -c 2>/dev/null | tail -n 1 | awk '{print $1}')"
total_bytes="${total_bytes:-0}"
max_bytes="$(( max_mb * 1024 * 1024 ))"

if (( file_count > max_files )); then
  echo "Snapshot retention exceeded file budget: $file_count > $max_files" >&2
  exit 1
fi

if (( total_bytes > max_bytes )); then
  echo "Snapshot retention exceeded size budget: $total_bytes > $max_bytes" >&2
  exit 1
fi

echo "Parity snapshot retention ok: files=$file_count size_bytes=$total_bytes"
