#!/usr/bin/env bash
set -euo pipefail

base_dir="${1:-.yode/benchmarks}"

for path in \
  "$base_dir/output-regression-snapshot.md" \
  "$base_dir/long-session-benchmark.md"; do
  if [[ -f "$path" ]]; then
    size="$(wc -c <"$path" | tr -d ' ')"
    lines="$(wc -l <"$path" | tr -d ' ')"
    echo "$path	size_bytes=$size	lines=$lines"
  else
    echo "$path	missing"
  fi
done
