#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-.yode/benchmarks}"
mkdir -p "$out_dir"
out_file="$out_dir/long-session-benchmark.md"

cargo test -p yode-tui print_long_session_benchmark_snapshot -- --nocapture \
  | sed -n '/^# Long Session Benchmark Snapshot/,$p' > "$out_file"

echo "Benchmark snapshot written to $out_file"
