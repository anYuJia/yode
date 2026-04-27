#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "$0")/snapshot-lib.sh"

out_dir="${1:-.yode/benchmarks}"
mkdir -p "$out_dir"
out_file="$out_dir/long-session-benchmark.md"

run_snapshot_capture \
  "print_long_session_benchmark_snapshot" \
  '^# Long Session Benchmark Snapshot' \
  "$out_file"

echo "Benchmark snapshot written to $out_file"
